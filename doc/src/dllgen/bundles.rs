//! Pre-rendered binding bundles + the self-contained `azul` Rust crate.
//!
//! WHY (2026-09-07): every language's binding was reachable only as loose files
//! under `release/<ver>/` (Haskell: eight `curl -o` lines into a two-package
//! tree), and Rust had NOTHING pre-rendered at all — the docs sent users to
//! clone the repo, build azul-doc and run `codegen all` (a 1.9 GB checkout for
//! a hello world, per the 2026-08 Rust GUI survey), and rust-analyzer could not
//! see the API because `dll/src/lib.rs` `include!`s files from outside the
//! crate directory.
//!
//! Per release this module writes, next to the loose files and from the same
//! inputs (so the two cannot drift):
//!
//! * `azul-<lang>-<ver>.tar.gz` — one per language (or C++ dialect group) that
//!   declares a `bundle` map in api.json (`installation.languages.<lang>.bundle`:
//!   release file → path inside the bundle). The map is the install steps'
//!   source of truth: the steps say `tar xzf`, the map says what comes out.
//! * `azul-rust-<ver>.tar.gz` — a self-contained `azul` crate whose generated
//!   sources live INSIDE `src/`, so rust-analyzer resolves every type without a
//!   generator or a build script. `azul-<ver>.crate` is the same crate in the
//!   layout cargo's sparse registry protocol expects (azul.rs/ui/cargo).
//! * `bindings-<ver>.tar.gz` — every rendered binding, the headers, api.json
//!   and the Rust crate, for agents and for "give me everything".
//!
//! tar.gz, not zip: macOS, every Linux and Windows 10+ (bsdtar) unpack it with
//! `tar xzf`, so no install step needs `unzip`. The archives are written by the
//! small ustar writer below — no archive crate for the deploy, and the bytes
//! are reproducible (fixed mtime, no uid/gid from the build machine), which is
//! what the post-deploy freshness check compares against the CDN.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

use crate::api::Installation;

/// The build script of the pre-rendered crate shares its link logic with
/// `dll/build.rs` — see the header of that file.
const LINK_RS: &str = include_str!("../../../dll/build_link.rs");

/// Fixed mtime for every archive entry (2026-01-01T00:00:00Z) so a rebuild of
/// the same inputs is byte-identical. Nothing in a bundle depends on when the
/// deploy ran, and a stable archive is what makes "the CDN serves the bytes
/// this deploy published" a meaningful check.
const ARCHIVE_MTIME: u64 = 1_767_225_600;

// ---------------------------------------------------------------------------
// ustar + gzip writer
// ---------------------------------------------------------------------------

/// Minimal, deterministic tar.gz writer: regular files only, mode 0644,
/// uid/gid 0, fixed mtime. Directories are implied — every extractor creates
/// missing parents.
pub struct TarGz {
    gz: flate2::write::GzEncoder<File>,
    entries: usize,
    bytes: u64,
}

impl TarGz {
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path)
            .with_context(|| format!("cannot create {}", path.display()))?;
        // GzEncoder's default header carries mtime 0 and no filename: stable.
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        Ok(TarGz { gz, entries: 0, bytes: 0 })
    }

    /// Append one regular file. `name` is the path inside the archive
    /// (forward slashes, no leading `/`).
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<()> {
        let header = ustar_header(name, data.len() as u64)?;
        self.gz.write_all(&header)?;
        self.gz.write_all(data)?;
        let pad = (512 - (data.len() % 512)) % 512;
        if pad > 0 {
            self.gz.write_all(&[0u8; 512][..pad])?;
        }
        self.entries += 1;
        self.bytes += data.len() as u64;
        Ok(())
    }

    /// Append the file at `src` under `name`.
    pub fn add_path(&mut self, name: &str, src: &Path) -> Result<()> {
        let data = fs::read(src).with_context(|| format!("cannot read {}", src.display()))?;
        self.add(name, &data)
    }

    /// Write the end-of-archive marker and flush. Returns (entries, bytes).
    pub fn finish(mut self) -> Result<(usize, u64)> {
        self.gz.write_all(&[0u8; 1024])?;
        let file = self.gz.finish()?;
        file.sync_all()?;
        Ok((self.entries, self.bytes))
    }
}

/// Split an archive path into the ustar (prefix, name) pair: `name` holds up
/// to 100 bytes, `prefix` up to 155, joined by a `/` the reader re-inserts.
fn split_ustar_name(name: &str) -> Result<(&str, &str)> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        bail!("invalid archive entry name {name:?}");
    }
    if name.len() <= 100 {
        return Ok(("", name));
    }
    // Longest prefix (ending before a '/') that leaves a name of <= 100 bytes.
    for (i, ch) in name.char_indices().rev() {
        if ch == '/' && name.len() - i - 1 <= 100 && i <= 155 && i > 0 {
            return Ok((&name[..i], &name[i + 1..]));
        }
    }
    bail!("archive entry name too long for ustar: {name:?}")
}

fn ustar_header(name: &str, size: u64) -> Result<[u8; 512]> {
    let (prefix, base) = split_ustar_name(name)?;
    let mut h = [0u8; 512];
    h[..base.len()].copy_from_slice(base.as_bytes());
    h[100..108].copy_from_slice(b"0000644\0");
    h[108..116].copy_from_slice(b"0000000\0");
    h[116..124].copy_from_slice(b"0000000\0");
    h[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    h[136..148].copy_from_slice(format!("{ARCHIVE_MTIME:011o}\0").as_bytes());
    h[148..156].copy_from_slice(b"        ");
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    Ok(h)
}

// ---------------------------------------------------------------------------
// The pre-rendered `azul` crate
// ---------------------------------------------------------------------------

/// Everything in the crate, as (path relative to the crate root, bytes). One
/// list feeds the tarball, the `.crate` and the union bundle.
fn rust_crate_files(
    version: &str,
    codegen_dir: &Path,
    example: Option<&[u8]>,
) -> Result<Vec<(String, Vec<u8>)>> {
    let external = codegen_dir.join("dll_api_external.rs");
    let reexports = codegen_dir.join("reexports.rs");
    for p in [&external, &reexports] {
        if !p.is_file() {
            bail!(
                "{} is missing — run `azul-doc codegen all` before `deploy`",
                p.display()
            );
        }
    }
    let mut files = vec![
        ("Cargo.toml".to_string(), rust_crate_manifest(version).into_bytes()),
        ("build.rs".to_string(), rust_crate_build_rs().into_bytes()),
        ("README.md".to_string(), rust_crate_readme(version).into_bytes()),
        ("src/lib.rs".to_string(), rust_crate_lib_rs(version).into_bytes()),
        (
            "src/generated/dll_api_external.rs".to_string(),
            fs::read(&external)?,
        ),
        ("src/generated/reexports.rs".to_string(), fs::read(&reexports)?),
    ];
    if let Some(ex) = example {
        files.push(("examples/hello-world.rs".to_string(), ex.to_vec()));
    }
    Ok(files)
}

fn rust_crate_manifest(version: &str) -> String {
    format!(
        r#"# Pre-rendered Rust binding for azul {version}, generated by `azul-doc deploy`
# from the release's api.json. The sources under src/generated/ ARE the API —
# do not edit them; the next release ships the next azul-rust-<version>.tar.gz.
#
# This crate links the PREBUILT libazul (nothing of azul is compiled here).
# build.rs finds the library in AZ_LINK_PATH, next to this crate or one
# directory above it, or where brew / apt / dnf installed it — see README.md.
[package]
name = "azul"
version = "{version}"
edition = "2021"
license = "MIT"
description = "Azul GUI framework - the pre-rendered Rust API over the prebuilt libazul"
homepage = "https://azul.rs/"
repository = "https://github.com/fschutt/azul"
build = "build.rs"
# Served from azul.rs/ui/cargo (sparse registry); published to crates.io by
# scripts/publish_upstream.sh when CARGO_REGISTRY_TOKEN is present.

[lib]
name = "azul"
crate-type = ["rlib"]

[features]
# The in-repo crate (azul-dll) has link-static / link-dynamic build modes. This
# crate only ever links the prebuilt library, so `link-dynamic` is accepted as
# a no-op for compatibility with the documented `--features link-dynamic`.
default = ["link-dynamic"]
link-dynamic = []
"#
    )
}

fn rust_crate_build_rs() -> String {
    format!(
        r#"// build.rs of the pre-rendered `azul` crate: links the prebuilt libazul.
//
// Everything below the `main` function is dll/build_link.rs from the azul
// repository, copied in verbatim by azul-doc at deploy time, so this crate and
// the in-repo azul-dll crate search the same places in the same order:
//   1. AZ_LINK_PATH (or AZ_DLL_PATH): comma-separated dirs or library files
//   2. this crate's directory, then its parent (the project that unpacked it)
//   3. the system library directories (brew / apt / dnf installs)
// A libazul.a / azul.lib found instead of the shared library is linked
// statically. Nothing found = a warning here and a linker error later.
#![allow(dead_code)]

use std::{{
    env, fs,
    path::{{Path, PathBuf}},
    process::Command,
}};

fn main() {{
    let target = env::var("TARGET").unwrap_or_default();
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut local = vec![crate_dir.clone()];
    if let Some(parent) = crate_dir.parent() {{
        local.push(parent.to_path_buf());
    }}
    configure_dynamic_linking(&target, &crate_dir, &local);
}}

{LINK_RS}"#
    )
}

fn rust_crate_lib_rs(version: &str) -> String {
    format!(
        r#"//! Azul GUI framework — the pre-rendered Rust API for azul {version}.
//!
//! Generated by `azul-doc deploy` from this release's api.json. The whole API
//! surface lives in `src/generated/` inside this crate, so rust-analyzer
//! resolves every type without running a generator or a build script.
//!
//! ```no_run
//! use azul::prelude::*;
//! ```
//!
//! build.rs links the prebuilt libazul — see README.md (`AZ_LINK_PATH`).

// The generated code is FFI glue: it is not written to the style lints, and a
// path dependency is linted like local code, so silence them here rather than
// in a user's build log.
#![allow(
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unused_doc_comments,
    improper_ctypes,
    mismatched_lifetime_syntaxes,
    unexpected_cfgs,
    static_mut_refs,
    deprecated,
    clippy::all
)]

#[macro_use]
extern crate alloc;

/// The generated C-ABI surface: `Az*` types, `extern "C"` declarations and the
/// safe wrappers over them (`azul::ffi::dll::*`).
#[path = "generated/dll_api_external.rs"]
mod __ffi_external;

pub use __ffi_external::__dll_api_inner::dll;

pub mod ffi {{
    pub use crate::__ffi_external::__dll_api_inner::*;
}}

// The unprefixed public API — `azul::app::App`, `azul::dom::Dom`,
// `azul::prelude::*` — re-exported from the ffi layer.
include!("generated/reexports.rs");
"#
    )
}

fn rust_crate_readme(version: &str) -> String {
    format!(
        r#"# azul {version} — pre-rendered Rust binding

This directory is a complete Cargo crate named `azul`: the Rust API of azul
{version}, generated from the release's `api.json`, with the generated sources
inside `src/generated/` (rust-analyzer sees every type; nothing to generate).
It links the **prebuilt** libazul — nothing of azul is compiled on your machine.

## Use it

```sh
# 1. the native library — from a package manager ...
brew tap fschutt/azul https://azul.rs/ui/brew.git && brew install fschutt/azul/azul   # macOS
echo 'deb [trusted=yes] https://azul.rs/ui/apt stable main' | sudo tee /etc/apt/sources.list.d/azul.list && sudo apt update && sudo apt install azul   # Debian/Ubuntu
# ... or a download next to this crate (any of libazul.dylib / libazul.so / azul.dll)
curl -O https://azul.rs/ui/release/{version}/libazul.dylib

# 2. depend on this crate by path
cargo add azul --path ./azul-rust-{version}
```

`build.rs` looks for the library in, in order: `AZ_LINK_PATH` (or `AZ_DLL_PATH`;
comma-separated directories or library files), this crate's directory, its
parent (your project), then the system library directories. A `libazul.a` /
`azul.lib` found instead of the shared library is linked statically.

```sh
export AZ_LINK_PATH=/path/to/dir-with-libazul   # only for an unusual location
cargo run --example hello-world                 # the counter example, from this directory
```

The same crate is served from azul.rs's cargo registry:

```toml
# .cargo/config.toml
[registries]
azul = {{ index = "sparse+https://azul.rs/ui/cargo/" }}
```

```sh
cargo add azul --registry azul
```

Docs: https://azul.rs/ui/api/{version} · guide: https://azul.rs/ui/guide/hello-world/rust
"#
    )
}

// ---------------------------------------------------------------------------
// Bundles
// ---------------------------------------------------------------------------

/// What `create_bindings_bundles` wrote and what it could not.
#[derive(Debug, Default)]
pub struct BundleReport {
    /// Archive file names written into the release directory.
    pub written: Vec<String>,
    /// `bundle` map entries whose source file was not in the release
    /// directory, as "<archive>: <source>". A deploy in strict mode must fail
    /// on these — a bundle missing a file the install steps rely on is exactly
    /// the "silently broken" state this exists to end.
    pub missing: Vec<String>,
}

/// Archive name for a language (or dialect group): `azul-<key>-<ver>.tar.gz`.
pub fn language_bundle_name(key: &str, version: &str) -> String {
    format!("azul-{key}-{version}.tar.gz")
}

/// `bindings-<ver>.tar.gz`
pub fn union_bundle_name(version: &str) -> String {
    format!("bindings-{version}.tar.gz")
}

/// `azul-rust-<ver>.tar.gz`
pub fn rust_bundle_name(version: &str) -> String {
    language_bundle_name("rust", version)
}

/// `azul-<ver>.crate` (cargo sparse-registry layout of the same crate).
pub fn rust_crate_name(version: &str) -> String {
    format!("azul-{version}.crate")
}

/// Per-language bundle maps, with C++ dialect variants merged into their group
/// (`cpp03`…`cpp23` → `cpp`): key → (release file → path inside the archive).
pub fn bundle_maps(installation: &Installation) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (lang, cfg) in &installation.languages {
        let Some(map) = &cfg.bundle else { continue };
        if map.is_empty() {
            continue;
        }
        let key = cfg.dialect_of.clone().unwrap_or_else(|| lang.clone());
        out.entry(key).or_default().extend(map.clone());
    }
    out
}

/// Write every bundle for `version` into `version_dir`. Never aborts on a
/// missing source (the deploy must produce a page even without codegen); the
/// report says what was skipped and the caller decides how loud to be.
pub fn create_bindings_bundles(
    version: &str,
    version_dir: &Path,
    codegen_dir: &Path,
    installation: &Installation,
) -> Result<BundleReport> {
    let mut report = BundleReport::default();

    // --- the Rust crate: tarball + .crate ----------------------------------
    let example = fs::read(version_dir.join("hello-world.rs")).ok();
    match rust_crate_files(version, codegen_dir, example.as_deref()) {
        Ok(files) => {
            let tgz = rust_bundle_name(version);
            let root = format!("azul-rust-{version}");
            let mut tar = TarGz::create(&version_dir.join(&tgz))?;
            for (path, bytes) in &files {
                tar.add(&format!("{root}/{path}"), bytes)?;
            }
            let (n, _) = tar.finish()?;
            println!("  - Created {tgz} ({n} files)");
            report.written.push(tgz);

            let krate = rust_crate_name(version);
            let root = format!("azul-{version}");
            let mut tar = TarGz::create(&version_dir.join(&krate))?;
            for (path, bytes) in &files {
                tar.add(&format!("{root}/{path}"), bytes)?;
            }
            tar.finish()?;
            println!("  - Created {krate} (cargo registry layout)");
            report.written.push(krate);
        }
        Err(e) => {
            eprintln!("  [WARN] Rust crate bundle skipped: {e}");
            report
                .missing
                .push(format!("{}: {e}", rust_bundle_name(version)));
        }
    }

    // --- per-language bundles from the api.json `bundle` maps --------------
    for (key, map) in bundle_maps(installation) {
        let name = language_bundle_name(&key, version);
        let path = version_dir.join(&name);
        let mut tar = TarGz::create(&path)?;
        let mut missing_here = 0usize;
        for (src, dst) in &map {
            let src_path = version_dir.join(src);
            if src.ends_with(".zip") && dst.ends_with('/') {
                // "<zip>": "<dir>/" — unpack the zip's tree under <dir>/, the
                // layout the steps' `unzip -o <zip> -d <dir>` used to create.
                match File::open(&src_path).and_then(|f| {
                    zip::ZipArchive::new(f)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }) {
                    Ok(mut archive) => {
                        for i in 0..archive.len() {
                            let mut entry = archive.by_index(i)?;
                            if entry.is_dir() {
                                continue;
                            }
                            let mut data = Vec::with_capacity(entry.size() as usize);
                            entry.read_to_end(&mut data)?;
                            tar.add(&format!("{dst}{}", entry.name()), &data)?;
                        }
                    }
                    Err(e) => {
                        missing_here += 1;
                        report.missing.push(format!("{name}: {src} ({e})"));
                    }
                }
                continue;
            }
            if !src_path.is_file() {
                missing_here += 1;
                report.missing.push(format!("{name}: {src}"));
                continue;
            }
            tar.add_path(dst, &src_path)?;
        }
        let (n, _) = tar.finish()?;
        if missing_here > 0 {
            eprintln!("  [WARN] {name}: {missing_here} of {} files missing", map.len());
        }
        println!("  - Created {name} ({n} files)");
        report.written.push(name);
    }

    // --- the union: the whole user-facing codegen tree + api.json + crate --
    let union = union_bundle_name(version);
    let root = format!("bindings-{version}");
    let mut tar = TarGz::create(&version_dir.join(&union))?;
    let mut n_codegen = 0usize;
    if codegen_dir.is_dir() {
        for entry in walk_files(codegen_dir)? {
            let rel = entry.strip_prefix(codegen_dir).unwrap();
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if is_internal_codegen_output(&rel_str) {
                continue;
            }
            tar.add_path(&format!("{root}/{rel_str}"), &entry)?;
            n_codegen += 1;
        }
    } else {
        report
            .missing
            .push(format!("{union}: codegen dir {} (run `azul-doc codegen all`)", codegen_dir.display()));
    }
    let api_json = version_dir.join("api.json");
    if api_json.is_file() {
        tar.add_path(&format!("{root}/api.json"), &api_json)?;
    } else {
        report.missing.push(format!("{union}: api.json"));
    }
    if let Ok(files) = rust_crate_files(version, codegen_dir, example.as_deref()) {
        for (path, bytes) in &files {
            tar.add(&format!("{root}/rust/azul-rust-{version}/{path}"), bytes)?;
        }
    }
    let (n, _) = tar.finish()?;
    println!("  - Created {union} ({n} files, {n_codegen} from codegen)");
    report.written.push(union);

    Ok(report)
}

/// Codegen outputs that are the DLL's own build inputs, not a user-facing
/// binding: the transmute bodies, the memory-layout tests, the pyo3/php glue
/// (those ship compiled, as the wheel / the extension), the compressed api.json
/// (api.json itself is added uncompressed), and the two files that make up the
/// Rust crate (added under rust/ as the crate instead).
fn is_internal_codegen_output(rel: &str) -> bool {
    matches!(
        rel,
        "dll_api_internal.rs"
            | "dll_api_external.rs"
            | "memtest.rs"
            | "python_api.rs"
            | "php_api.rs"
            | "reexports.rs"
    ) || rel.ends_with(".br")
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).with_context(|| format!("cannot read {}", d.display()))? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(h: &[u8; 512], from: usize, to: usize) -> String {
        // Numeric fields end in "\0" or "\0 " (the checksum); strip both.
        String::from_utf8_lossy(&h[from..to])
            .trim_matches(|c| c == '\0' || c == ' ')
            .to_string()
    }

    #[test]
    fn ustar_header_round_trips_its_fields_and_checksum() {
        let h = ustar_header("azul-haskell/src/Azul/Internal/FFI.hs", 1234).unwrap();
        assert_eq!(field(&h, 0, 100), "azul-haskell/src/Azul/Internal/FFI.hs");
        assert_eq!(field(&h, 100, 108), "0000644");
        assert_eq!(u64::from_str_radix(&field(&h, 124, 136), 8).unwrap(), 1234);
        assert_eq!(u64::from_str_radix(&field(&h, 136, 148), 8).unwrap(), ARCHIVE_MTIME);
        assert_eq!(h[156], b'0');
        assert_eq!(&h[257..263], b"ustar\0");
        // The checksum is the byte sum with the checksum field read as spaces.
        let mut copy = h;
        copy[148..156].copy_from_slice(b"        ");
        let expected: u32 = copy.iter().map(|&b| u32::from(b)).sum();
        assert_eq!(u32::from_str_radix(&field(&h, 148, 156), 8).unwrap(), expected);
    }

    #[test]
    fn long_names_split_on_a_slash_into_prefix_and_name() {
        let dir = "a".repeat(60);
        let file = "b".repeat(60);
        let name = format!("{dir}/{file}");
        let (prefix, base) = split_ustar_name(&name).unwrap();
        assert_eq!(prefix, dir);
        assert_eq!(base, file);
        assert!(split_ustar_name(&"c".repeat(101)).is_err());
        assert!(split_ustar_name("/abs").is_err());
    }

    #[test]
    fn archives_are_reproducible_and_padded_to_512() {
        let dir = tempfile::tempdir().unwrap();
        let write = |p: &Path| {
            let mut t = TarGz::create(p).unwrap();
            t.add("x/a.txt", b"hello").unwrap();
            t.add("y", &[7u8; 513]).unwrap();
            t.finish().unwrap()
        };
        let a = dir.path().join("a.tar.gz");
        let b = dir.path().join("b.tar.gz");
        assert_eq!(write(&a), (2, 518));
        write(&b);
        assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
        // Decompress and check the raw tar geometry: 2 headers + 1 + 2 data
        // blocks + 2 end blocks = 7 * 512.
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(File::open(&a).unwrap())
            .read_to_end(&mut raw)
            .unwrap();
        assert_eq!(raw.len(), 7 * 512);
        assert_eq!(&raw[512..517], b"hello");
        assert!(raw[raw.len() - 1024..].iter().all(|&b| b == 0));
    }

    #[test]
    fn the_crate_manifest_names_the_release_and_the_lib_includes_its_sources() {
        let m = rust_crate_manifest("0.2.0");
        assert!(m.contains("name = \"azul\""));
        assert!(m.contains("version = \"0.2.0\""));
        let lib = rust_crate_lib_rs("0.2.0");
        assert!(lib.contains("#[path = \"generated/dll_api_external.rs\"]"));
        assert!(lib.contains("include!(\"generated/reexports.rs\")"));
        // The shared link logic really is embedded, with the entry point the
        // in-repo build.rs calls.
        let b = rust_crate_build_rs();
        assert!(b.contains("fn configure_dynamic_linking(target: &str, base_dir: &Path, local_dirs: &[PathBuf])"));
        assert!(b.contains("fn emit_static_system_deps"));
    }
}
