use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{api::ApiData, dllgen::license::License, docgen::HTML_ROOT};

/// Verifies that all example files referenced in api.json exist on the filesystem.
///
/// Returns Ok(()) if all examples exist, or an error listing all missing files.
/// Use `strict` mode to fail the build, or non-strict to just print warnings.
pub fn verify_examples(api_data: &ApiData, examples_dir: &Path, strict: bool) -> Result<()> {
    let mut missing_files: Vec<String> = Vec::new();

    // Get all versions and their examples
    for version in api_data.get_sorted_versions() {
        if let Some(version_data) = api_data.get_version(&version) {
            for example in &version_data.examples {
                // Check required languages (c, rust, python)
                let c_path = examples_dir.join(&example.code.c);
                if !c_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: C example missing: {}",
                        version, example.name, example.code.c
                    ));
                }

                let rust_path = examples_dir.join(&example.code.rust);
                if !rust_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: Rust example missing: {}",
                        version, example.name, example.code.rust
                    ));
                }

                let python_path = examples_dir.join(&example.code.python);
                if !python_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: Python example missing: {}",
                        version, example.name, example.code.python
                    ));
                }

                // Check C++ dialects (optional fields)
                if let Some(ref cpp03) = example.code.cpp03 {
                    let cpp_path = examples_dir.join(cpp03);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++03 example missing: {}",
                            version, example.name, cpp03
                        ));
                    }
                }
                if let Some(ref cpp11) = example.code.cpp11 {
                    let cpp_path = examples_dir.join(cpp11);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++11 example missing: {}",
                            version, example.name, cpp11
                        ));
                    }
                }
                if let Some(ref cpp14) = example.code.cpp14 {
                    let cpp_path = examples_dir.join(cpp14);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++14 example missing: {}",
                            version, example.name, cpp14
                        ));
                    }
                }
                if let Some(ref cpp17) = example.code.cpp17 {
                    let cpp_path = examples_dir.join(cpp17);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++17 example missing: {}",
                            version, example.name, cpp17
                        ));
                    }
                }
                if let Some(ref cpp20) = example.code.cpp20 {
                    let cpp_path = examples_dir.join(cpp20);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++20 example missing: {}",
                            version, example.name, cpp20
                        ));
                    }
                }
                if let Some(ref cpp23) = example.code.cpp23 {
                    let cpp_path = examples_dir.join(cpp23);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++23 example missing: {}",
                            version, example.name, cpp23
                        ));
                    }
                }

                // Check legacy cpp field if no per-version paths exist
                if example.code.cpp03.is_none()
                    && example.code.cpp11.is_none()
                    && example.code.cpp14.is_none()
                    && example.code.cpp17.is_none()
                    && example.code.cpp20.is_none()
                    && example.code.cpp23.is_none()
                {
                    if let Some(ref cpp) = example.code.cpp {
                        let cpp_path = examples_dir.join(cpp);
                        if !cpp_path.exists() {
                            missing_files.push(format!(
                                "[{}] {}: C++ example missing: {}",
                                version, example.name, cpp
                            ));
                        }
                    }
                }
            }
        }
    }

    if missing_files.is_empty() {
        println!("  [OK] All example files verified");
        Ok(())
    } else {
        let error_msg = format!(
            "Missing {} example file(s):\n  {}",
            missing_files.len(),
            missing_files.join("\n  ")
        );

        if strict {
            anyhow::bail!("{}", error_msg);
        } else {
            eprintln!("  [WARN] {}", error_msg);
            Ok(())
        }
    }
}

/// Deploy mode controls how missing assets are handled
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeployMode {
    /// Local development mode - missing binary assets create placeholders
    Local,
    /// CI mode - missing binary assets cause deployment to fail
    Strict,
}

/// Binary assets that must be built by CI (cannot be generated on-the-fly)
#[derive(Debug, Clone)]
pub struct BinaryAsset {
    pub filename: &'static str,
    pub description: &'static str,
    pub platform: Platform,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
    All,
}

impl BinaryAsset {
    pub const WINDOWS_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "azul.dll",
            description: "Windows 64-bit dynamic library",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.lib",
            description: "Windows 64-bit static library",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.dll.lib",
            description: "Windows MSVC import library",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.pyd",
            description: "Python Extension (Windows)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "LICENSE-WINDOWS.txt",
            description: "Windows License",
            platform: Platform::Windows,
        },
    ];

    pub const LINUX_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "libazul.so",
            description: "Linux 64-bit .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux.a",
            description: "Linux 64-bit .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "azul.cpython.so",
            description: "Python Extension (Linux)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "LICENSE-LINUX.txt",
            description: "Linux License",
            platform: Platform::Linux,
        },
    ];

    pub const MACOS_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "libazul.dylib",
            description: "MacOS 64-bit SO",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "libazul.macos.a",
            description: "MacOS 64-bit .a",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "azul.so",
            description: "Python Extension (macOS)",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "LICENSE-MACOS.txt",
            description: "MacOS License",
            platform: Platform::MacOS,
        },
    ];

    /// Cross-architecture assets (experimental, optional — only shown if present)
    pub const CROSS_ARCH_ASSETS: &'static [BinaryAsset] = &[
        // Linux cross-arch
        BinaryAsset {
            filename: "libazul.linux-i686.so",
            description: "Linux 32-bit x86 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-i686.a",
            description: "Linux 32-bit x86 .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-aarch64.so",
            description: "Linux ARM64 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-aarch64.a",
            description: "Linux ARM64 .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-armv7.so",
            description: "Linux ARMv7 .so (Raspberry Pi)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-armv7.a",
            description: "Linux ARMv7 .a (Raspberry Pi)",
            platform: Platform::Linux,
        },
        // Exotic Linux arches (experimental, big-endian + RISC-V) — match the
        // azul-linux-{ppc64,s390x,riscv64} artifacts the cross_build_binaries
        // job produces and the deploy lays out as libazul.linux-<arch>.{so,a}.
        BinaryAsset {
            filename: "libazul.linux-ppc64.so",
            description: "Linux PowerPC64 .so (big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-ppc64.a",
            description: "Linux PowerPC64 .a (big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-s390x.so",
            description: "Linux s390x .so (IBM Z, big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-s390x.a",
            description: "Linux s390x .a (IBM Z, big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-riscv64.so",
            description: "Linux RISC-V 64 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-riscv64.a",
            description: "Linux RISC-V 64 .a",
            platform: Platform::Linux,
        },
        // Windows cross-arch
        BinaryAsset {
            filename: "azul.i686.dll",
            description: "Windows 32-bit DLL (for 32-bit Windows 7 and later)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.i686.lib",
            description: "Windows 32-bit import library (for 32-bit Windows 7 and later)",
            platform: Platform::Windows,
        },
        // macOS cross-arch
        BinaryAsset {
            filename: "libazul.x86_64.dylib",
            description: "macOS Intel x86_64 .dylib",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "libazul.macos-x86_64.a",
            description: "macOS Intel x86_64 .a",
            platform: Platform::MacOS,
        },
        // Legacy 32-bit Windows (95/98/2000/XP) — needs a separate build because
        // the modern Rust/MSVC toolchain can no longer target those OS versions.
        BinaryAsset {
            filename: "azul.rust9x.dll",
            description: "Windows 32-bit DLL (for legacy Windows 95/98/2000/XP)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.rust9x.lib",
            description: "Windows 32-bit import library (for legacy Windows 95/98/2000/XP)",
            platform: Platform::Windows,
        },
    ];

    pub fn all() -> Vec<&'static BinaryAsset> {
        Self::WINDOWS_ASSETS
            .iter()
            .chain(Self::LINUX_ASSETS.iter())
            .chain(Self::MACOS_ASSETS.iter())
            .collect()
    }
}

/// Information about an asset file (present or missing)
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub filename: String,
    pub description: String,
    pub size: Option<u64>,
    pub is_present: bool,
}

impl AssetInfo {
    /// Format file size in human-readable form (KB, MB, GB)
    pub fn humanize_size(&self) -> String {
        match self.size {
            None => "N/A".to_string(),
            Some(bytes) => {
                if bytes < 1024 {
                    format!("{}B", bytes)
                } else if bytes < 1024 * 1024 {
                    format!("{:.1}KB", bytes as f64 / 1024.0)
                } else if bytes < 1024 * 1024 * 1024 {
                    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
                } else {
                    format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
                }
            }
        }
    }

    /// Check if an asset exists and get its size
    pub fn from_path(path: &Path, description: &str) -> Self {
        let is_present = path.exists();
        let size = if is_present {
            fs::metadata(path).ok().map(|m| m.len())
        } else {
            None
        };

        AssetInfo {
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            description: description.to_string(),
            size,
            is_present,
        }
    }
}

/// Collected asset information for a release version
#[derive(Debug, Clone)]
pub struct ReleaseAssets {
    pub windows: Vec<AssetInfo>,
    pub linux: Vec<AssetInfo>,
    pub macos: Vec<AssetInfo>,
    /// Cross-architecture binaries (optional, only shown if files exist)
    pub cross_arch: Vec<AssetInfo>,
    pub c_header: AssetInfo,
    pub cpp_headers: Vec<AssetInfo>,
    pub api_json: AssetInfo,
    pub examples_zip: AssetInfo,
}

impl ReleaseAssets {
    /// Collect asset information from the release directory
    pub fn collect(version_dir: &Path) -> Self {
        let mut windows = Vec::new();
        for asset in BinaryAsset::WINDOWS_ASSETS {
            windows.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let mut linux = Vec::new();
        for asset in BinaryAsset::LINUX_ASSETS {
            linux.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let mut macos = Vec::new();
        for asset in BinaryAsset::MACOS_ASSETS {
            macos.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let cpp_headers = vec![
            AssetInfo::from_path(&version_dir.join("azul03.hpp"), "C++03 Header"),
            AssetInfo::from_path(&version_dir.join("azul11.hpp"), "C++11 Header"),
            AssetInfo::from_path(&version_dir.join("azul14.hpp"), "C++14 Header"),
            AssetInfo::from_path(&version_dir.join("azul17.hpp"), "C++17 Header"),
            AssetInfo::from_path(&version_dir.join("azul20.hpp"), "C++20 Header"),
            AssetInfo::from_path(&version_dir.join("azul23.hpp"), "C++23 Header"),
        ];

        // Cross-arch assets: only include those that actually exist on disk
        let cross_arch: Vec<AssetInfo> = BinaryAsset::CROSS_ARCH_ASSETS
            .iter()
            .map(|asset| AssetInfo::from_path(&version_dir.join(asset.filename), asset.description))
            .filter(|a| a.is_present)
            .collect();

        ReleaseAssets {
            windows,
            linux,
            macos,
            cross_arch,
            c_header: AssetInfo::from_path(&version_dir.join("azul.h"), "C Header"),
            cpp_headers,
            api_json: AssetInfo::from_path(&version_dir.join("api.json"), "API Description"),
            examples_zip: AssetInfo::from_path(&version_dir.join("examples.zip"), "Examples"),
        }
    }

    /// Check if all binary assets are present (for strict mode)
    pub fn validate_binary_assets(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for asset in &self.windows {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }
        for asset in &self.linux {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }
        for asset in &self.macos {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

pub struct Config {
    pub build_windows: bool,
    pub build_linux: bool,
    pub build_macos: bool,
    pub build_python: bool,
    /// EXCEPTIONAL, opt-in (`--with-remill`): also build the ~130 MB libazul
    /// with the remill x86->WASM transpiler statically linked (web backend).
    /// Off by default — it's a ~30 min C++ build that needs
    /// `bash scripts/build_remill.sh` to populate third_party/remill-install.
    pub build_remill: bool,
    pub open: bool,
    pub apply_patch: bool,
    pub print_imports: bool,
    pub deploy_mode: DeployMode,
}

impl Config {
    pub fn print(&self) -> String {
        let mut v = Vec::new();
        let mut build = Vec::new();
        if self.build_windows {
            build.push("windows");
        }
        if self.build_linux {
            build.push("linux");
        }
        if self.build_macos {
            build.push("mac");
        }
        v.push(format!("build={}", build.join(",")));
        if self.build_python {
            v.push("python=true".to_string());
        }
        if self.build_remill {
            v.push("with-remill=true".to_string());
        }
        if self.open {
            v.push("open=true".to_string());
        }
        if self.apply_patch {
            v.push("apply-patch=true".to_string());
        }
        if self.print_imports {
            v.push("print-imports=true".to_string());
        }
        v.push(format!("mode={:?}", self.deploy_mode));
        v.join(" ")
    }

    pub fn from_args() -> Self {
        let args: Vec<String> = env::args().collect();
        let mut config = Self {
            build_windows: false,
            build_linux: false,
            build_macos: false,
            build_python: false,
            build_remill: false,
            open: false,
            apply_patch: false,
            print_imports: false,
            deploy_mode: DeployMode::Local,
        };

        for arg in &args[1..] {
            if let Some(value) = arg.strip_prefix("--build=") {
                config.parse_build_arg(value);
                continue;
            }

            if arg == "--open" {
                config.open = true;
                continue;
            }

            if arg == "--with-remill" {
                config.build_remill = true;
                continue;
            }

            if arg == "--apply-patch" {
                config.apply_patch = true;
                continue;
            }

            if arg == "--print-imports" {
                config.print_imports = true;
                continue;
            }

            if arg == "--strict" || arg == "--ci" {
                config.deploy_mode = DeployMode::Strict;
                continue;
            }
        }

        config
    }

    fn parse_build_arg(&mut self, value: &str) {
        if value == "all" {
            self.build_windows = true;
            self.build_linux = true;
            self.build_macos = true;
            self.build_python = true;
            return;
        }

        if value == "none" {
            return;
        }

        for target in value.split(',') {
            match target {
                "windows" => self.build_windows = true,
                "linux" => self.build_linux = true,
                "macos" => self.build_macos = true,
                "python" => self.build_python = true,
                _ => {}
            }
        }
    }
}

pub fn generate_license_files(version: &str, output_dir: &Path) -> Result<()> {
    println!("  Generating license files...");

    let dll_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dll");

    assert!(Path::new(dll_path).join("Xargo.toml").exists());

    let targets = &[
        ("LICENSE-WINDOWS.txt", "x86_64-pc-windows-msvc"),
        ("LICENSE-MACOS.txt", "aarch64-apple-darwin"),
        ("LICENSE-LINUX.txt", "x86_64-unknown-linux-gnu"),
        // Mobile: the iOS / Android slices pull a different dependency set
        // (platform backends, codecs) so their bundled-license lists differ
        // from desktop. cargo-license resolves per-target via CARGO_BUILD_TARGET
        // -> `cargo metadata --filter-platform`, which evaluates each crate's
        // `cfg(target_os = ...)` against cargo's built-in target spec. That is a
        // metadata-only query: it does NOT require the target toolchain be
        // installed (verified: both targets resolve full graphs, ~7.4k crates),
        // so these lists are real and distinct, not empty fallbacks.
        ("LICENSE-IOS.txt", "aarch64-apple-ios"),
        ("LICENSE-ANDROID.txt", "aarch64-linux-android"),
    ];

    for (f, target) in targets.iter() {
        // Use cargo-license to get dependency information
        let cargo_meta_cmd = cargo_metadata::MetadataCommand::new()
            .current_dir(dll_path)
            .env("CARGO_BUILD_TARGET", target)
            .clone();

        let opt = cargo_license::GetDependenciesOpt {
            avoid_dev_deps: true,
            avoid_build_deps: true,
            avoid_proc_macros: true,
            direct_deps_only: false,
            root_only: false,
        };

        let l = cargo_license::get_dependencies_from_cargo_lock(&cargo_meta_cmd, &opt)
            .unwrap_or_default()
            .into_iter()
            .map(|s| License {
                name: s.name.to_string(),
                version: s.version.to_string(),
                license_type: s.license.unwrap_or_default(),
                authors: s
                    .authors
                    .unwrap_or_default()
                    .split("|")
                    .map(|s| s.to_string())
                    .collect(),
            })
            .collect::<Vec<_>>();

        let default_license_text = vec![
            "[program] is based in part on the AZUL GUI toolkit (https://azul.rs),",
            "licensed under the MIT License (C) 2018 Felix Schütt.",
            "",
            "The AZUL GUI toolkit itself uses the following libraries:",
            "",
            "",
        ]
        .join("\r\n");

        let license_posttext = vec![
            "",
            "To generate the full text of the license for the license, please visit",
            "https://spdx.org/licenses/ and replace the license author in the source",
            "text in any given license with the name of the author listed above.",
        ]
        .join("\r\n");

        let mut s = String::new();
        s.push_str(&default_license_text);
        s.push_str(&crate::dllgen::license::format_license_authors(&l));
        s.push_str(&license_posttext);
        std::fs::write(&output_dir.join(f), &s)?;
    }

    println!("  - Generated license files");
    Ok(())
}

/// C++ headers for all supported standards
pub struct CppHeaders {
    pub cpp03: String,
    pub cpp11: String,
    pub cpp14: String,
    pub cpp17: String,
    pub cpp20: String,
    pub cpp23: String,
}

/// Where a release-dir file is sourced from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BindingSource {
    /// Generated by `azul-doc codegen all` under `target/codegen/`.
    Codegen,
    /// A hand-written per-language example under `examples/`. Used for the
    /// `hello-world.*` driver files (and a couple of build scaffolds) that the
    /// generator does not emit but the install steps still `curl`.
    Examples,
}

/// One file the per-language install steps `curl` out of `release/{version}/`,
/// together with where the deploy copies it FROM.
///
/// `dst` is the path **relative to `release/{version}/`** and must match the
/// curl target byte-for-byte (e.g. Haskell's nested `Azul/Types.hs`). `src` is
/// relative to either `target/codegen/` or `examples/` depending on `source`.
struct BindingFile {
    dst: &'static str,
    src: &'static str,
    source: BindingSource,
}

/// Every non-whitelist (`go`, `haskell`, `ada`, `pascal`, `zig`, `cobol`,
/// `fortran`, `perl`, `php`, `lisp`, `smalltalk`, `vb6`, `freebasic`,
/// `algol68`, `powershell`) binding + scaffolding file referenced by the
/// install instructions in api.json, mapped to its on-disk source.
///
/// The WHITELIST languages (c, cpp, rust, python, csharp, java, kotlin, lua,
/// ruby, node, ocaml) are intentionally absent: they download the native libs /
/// C·C++ headers (already laid down by the deploy) or generate their binding
/// locally, so nothing extra is copied for them.
///
/// `dst` values are flat because no two non-whitelist languages collide on a
/// filename — the only nested paths (`Azul/Types.hs`, `Azul/Internal/FFI.hs`)
/// are what the Haskell `curl -o src/Azul/...` steps genuinely request under
/// `release/{version}/`.
const BINDING_FILES: &[BindingFile] = &[
    // --- ada ---
    BindingFile { dst: "azul.ads", src: "azul.ads", source: BindingSource::Codegen },
    BindingFile { dst: "azul.adb", src: "azul.adb", source: BindingSource::Codegen },
    BindingFile { dst: "hello_world.gpr", src: "ada/hello_world.gpr", source: BindingSource::Examples },
    // --- algol68 ---
    BindingFile { dst: "azul.a68", src: "azul.a68", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.a68", src: "algol68/hello-world.a68", source: BindingSource::Examples },
    // --- cobol ---
    BindingFile { dst: "azul.cpy", src: "azul.cpy", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.cob", src: "cobol/hello-world.cob", source: BindingSource::Examples },
    // --- fortran (codegen emits Makefile.fortran; curl asks for `Makefile`) ---
    BindingFile { dst: "azul.f90", src: "azul.f90", source: BindingSource::Codegen },
    BindingFile { dst: "Makefile", src: "Makefile.fortran", source: BindingSource::Codegen },
    // --- freebasic ---
    BindingFile { dst: "azul.bi", src: "azul.bi", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.bas", src: "freebasic/hello-world.bas", source: BindingSource::Examples },
    // --- go ---
    BindingFile { dst: "azul.go", src: "go/azul.go", source: BindingSource::Codegen },
    BindingFile { dst: "types.go", src: "go/types.go", source: BindingSource::Codegen },
    BindingFile { dst: "functions.go", src: "go/functions.go", source: BindingSource::Codegen },
    BindingFile { dst: "wrappers.go", src: "go/wrappers.go", source: BindingSource::Codegen },
    BindingFile { dst: "go.mod", src: "go/go.mod", source: BindingSource::Codegen },
    // --- haskell (nested paths match the `curl -o src/Azul/...` steps) ---
    BindingFile { dst: "azul.cabal", src: "haskell/azul.cabal", source: BindingSource::Codegen },
    BindingFile { dst: "Azul.hs", src: "haskell/src/Azul.hs", source: BindingSource::Codegen },
    BindingFile { dst: "Azul/Types.hs", src: "haskell/src/Azul/Types.hs", source: BindingSource::Codegen },
    BindingFile { dst: "Azul/Internal/FFI.hs", src: "haskell/src/Azul/Internal/FFI.hs", source: BindingSource::Codegen },
    // The cabal package's C shim (cbits/) — without it the downloaded
    // package cannot build (azul.h is published separately at top level).
    BindingFile { dst: "azul_shims.c", src: "haskell/cbits/azul_shims.c", source: BindingSource::Codegen },
    BindingFile { dst: "HelloWorld.hs", src: "haskell/HelloWorld.hs", source: BindingSource::Examples },
    BindingFile { dst: "azul-example.cabal", src: "haskell/azul-example.cabal", source: BindingSource::Examples },
    // --- lisp (ships the ASDF driver system + example so the quickload flow works) ---
    BindingFile { dst: "azul.asd", src: "azul.asd", source: BindingSource::Codegen },
    BindingFile { dst: "azul.lisp", src: "azul.lisp", source: BindingSource::Codegen },
    BindingFile { dst: "azul-example.asd", src: "lisp/azul-example.asd", source: BindingSource::Examples },
    BindingFile { dst: "hello-world.lisp", src: "lisp/hello-world.lisp", source: BindingSource::Examples },
    // --- pascal ---
    BindingFile { dst: "azul.pas", src: "azul.pas", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.pas", src: "pascal/hello-world.pas", source: BindingSource::Examples },
    // --- perl ---
    BindingFile { dst: "Azul.pm", src: "Azul.pm", source: BindingSource::Codegen },
    BindingFile { dst: "cpanfile", src: "cpanfile", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.pl", src: "perl/hello-world.pl", source: BindingSource::Examples },
    // --- php (ships both the php-ffi driver and the native-extension driver) ---
    BindingFile { dst: "Azul.php", src: "Azul.php", source: BindingSource::Codegen },
    BindingFile { dst: "composer.json", src: "composer.json", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.php", src: "php/hello-world.php", source: BindingSource::Examples },
    BindingFile { dst: "hello-world-ext.php", src: "php/hello-world-ext.php", source: BindingSource::Examples },
    // --- powershell ---
    BindingFile { dst: "Azul.psd1", src: "Azul.psd1", source: BindingSource::Codegen },
    BindingFile { dst: "Azul.psm1", src: "Azul.psm1", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.ps1", src: "powershell/hello-world.ps1", source: BindingSource::Examples },
    // --- smalltalk ---
    BindingFile { dst: "Azul.st", src: "Azul.st", source: BindingSource::Codegen },
    BindingFile { dst: "BaselineOfAzul.st", src: "BaselineOfAzul.st", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.st", src: "smalltalk/HelloWorld.st", source: BindingSource::Examples },
    // --- vb6 (Azul.bas is the generated binding module; HelloWorld.* are the example app) ---
    BindingFile { dst: "Azul.bas", src: "vb6/Azul.bas", source: BindingSource::Codegen },
    BindingFile { dst: "HelloWorld.bas", src: "vb6/HelloWorld.bas", source: BindingSource::Examples },
    BindingFile { dst: "HelloWorld.vbp", src: "vb6/HelloWorld.vbp", source: BindingSource::Examples },
    // --- zig ---
    BindingFile { dst: "azul.zig", src: "azul.zig", source: BindingSource::Codegen },
    BindingFile { dst: "build.zig", src: "build.zig", source: BindingSource::Codegen },
    // --- odin (azul.odin is imported as the `azul/` subpackage; hello-world.odin
    //     is the `package main` driver next to it) ---
    BindingFile { dst: "azul/azul.odin", src: "azul.odin", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.odin", src: "odin/hello-world.odin", source: BindingSource::Examples },
    // --- candidate bindings (nim/racket/red): off-frontpage, CI-validated ---
    BindingFile { dst: "azul.nim", src: "azul.nim", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.nim", src: "nim/hello-world.nim", source: BindingSource::Examples },
    BindingFile { dst: "azul.rkt", src: "azul.rkt", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.rkt", src: "racket/hello-world.rkt", source: BindingSource::Examples },
    BindingFile { dst: "azul.reds", src: "azul.reds", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.red", src: "red/hello-world.red", source: BindingSource::Examples },
    // --- more candidate archetype-A bindings (d/crystal/v/swift/julia) ---
    // d: `module azul`, compiled alongside the driver (top-level, no subdir).
    BindingFile { dst: "azul.d", src: "azul.d", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.d", src: "d/hello-world.d", source: BindingSource::Examples },
    // crystal: single `lib LibAzul`, required as a sibling `./azul`.
    BindingFile { dst: "azul.cr", src: "azul.cr", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.cr", src: "crystal/hello-world.cr", source: BindingSource::Examples },
    // v: `azul/` subpackage + `module main` driver.
    BindingFile { dst: "azul/azul.v", src: "azul.v", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.v", src: "v/hello-world.v", source: BindingSource::Examples },
    // swift: thin layer over azul.h via a Clang module map (needs azul.h + modulemap).
    BindingFile { dst: "azul.swift", src: "azul.swift", source: BindingSource::Codegen },
    BindingFile { dst: "module.modulemap", src: "module.modulemap", source: BindingSource::Codegen },
    BindingFile { dst: "azul.h", src: "azul.h", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.swift", src: "swift/hello-world.swift", source: BindingSource::Examples },
    // julia: `azul/` subdir the driver `include`s; libazul dlopen'd via AZUL_LIB.
    BindingFile { dst: "azul/azul.jl", src: "azul.jl", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.jl", src: "julia/hello-world.jl", source: BindingSource::Examples },
    // --- csharp (the main-page FFI tabs below ship the generated binding so the
    //     install steps `curl` it from azul.rs instead of cloning + codegen) ---
    BindingFile { dst: "Azul.cs", src: "Azul.cs", source: BindingSource::Codegen },
    BindingFile { dst: "Azul.csproj", src: "Azul.csproj", source: BindingSource::Codegen },
    // --- ruby ---
    BindingFile { dst: "azul.rb", src: "azul.rb", source: BindingSource::Codegen },
    BindingFile { dst: "azul.gemspec", src: "azul.gemspec", source: BindingSource::Codegen },
    // --- lua ---
    BindingFile { dst: "azul.lua", src: "azul.lua", source: BindingSource::Codegen },
    // NOTE: the LuaRocks rockspec is handled dynamically in
    // copy_language_bindings() — its filename embeds the release version
    // (`azul-<version>-1.rockspec`), which a const list cannot express.
    // --- node ---
    BindingFile { dst: "azul.js", src: "node/azul.js", source: BindingSource::Codegen },
    BindingFile { dst: "package.json", src: "node/package.json", source: BindingSource::Codegen },
    // --- ocaml ---
    BindingFile { dst: "azul.ml", src: "azul.ml", source: BindingSource::Codegen },
    BindingFile { dst: "azul.mli", src: "azul.mli", source: BindingSource::Codegen },
    BindingFile { dst: "dune", src: "dune", source: BindingSource::Codegen },
    BindingFile { dst: "dune-project", src: "dune-project", source: BindingSource::Codegen },
    // --- kotlin ---
    BindingFile { dst: "Azul.kt", src: "kotlin/Azul.kt", source: BindingSource::Codegen },
    BindingFile { dst: "build.gradle.kts", src: "kotlin/build.gradle.kts", source: BindingSource::Codegen },
    BindingFile { dst: "settings.gradle.kts", src: "kotlin/settings.gradle.kts", source: BindingSource::Codegen },
    // --- example drivers for the SHIPPED frontpage languages, so every file
    //     the install steps compile/run can be curl'd from release/<v>/
    //     (2026-07-04 review: several tabs referenced files no step obtains) ---
    BindingFile { dst: "hello-world.c", src: "c/hello-world.c", source: BindingSource::Examples },
    BindingFile { dst: "hello-world.cpp", src: "cpp/cpp20/hello-world.cpp", source: BindingSource::Examples },
    // python ships the wheel as a BinaryAsset; the driver was only in examples.zip
    // before, so its `curl .../hello-world.py` install step 404'd. Ship it too.
    BindingFile { dst: "hello-world.py", src: "python/hello-world.py", source: BindingSource::Examples },
    // Azul.csproj above is the LIBRARY project; Hello.csproj is the runnable
    // Exe scaffold `dotnet run` needs next to hello-world.cs.
    BindingFile { dst: "hello-world.cs", src: "csharp/hello-world.cs", source: BindingSource::Examples },
    BindingFile { dst: "Hello.csproj", src: "csharp/Hello.csproj", source: BindingSource::Examples },
    BindingFile { dst: "HelloWorld.java", src: "java/HelloWorld.java", source: BindingSource::Examples },
    BindingFile { dst: "pom.xml", src: "java/pom.xml", source: BindingSource::Examples },
    BindingFile { dst: "HelloWorld.kt", src: "kotlin/HelloWorld.kt", source: BindingSource::Examples },
    BindingFile { dst: "hello-world.js", src: "node/hello-world.js", source: BindingSource::Examples },
    BindingFile { dst: "hello-world.rb", src: "ruby/hello-world.rb", source: BindingSource::Examples },
    BindingFile { dst: "hello-world.lua", src: "lua/hello-world.lua", source: BindingSource::Examples },
    BindingFile { dst: "hello_world.ml", src: "ocaml/hello_world.ml", source: BindingSource::Examples },
    // --- promotion candidates (zig/go/fortran/scala) ---
    BindingFile { dst: "hello-world.zig", src: "zig/hello-world.zig", source: BindingSource::Examples },
    BindingFile { dst: "main.go", src: "go/main.go", source: BindingSource::Examples },
    BindingFile { dst: "hello_world.f90", src: "fortran/hello_world.f90", source: BindingSource::Examples },
    BindingFile { dst: "HelloWorld.scala", src: "scala/HelloWorld.scala", source: BindingSource::Examples },
];

/// Copy every non-whitelist per-language binding + scaffolding file that the
/// install instructions `curl` out of `release/{version}/` into `version_dir`,
/// using the exact names the steps expect.
///
/// Sources are the `azul-doc codegen all` outputs under `codegen_dir`
/// (`target/codegen/`) and, for the `hello-world.*` driver files the generator
/// doesn't emit, the per-language `examples/` tree. The deploy that already
/// lays down `azul.h` / `azul*.hpp` for the whitelist C/C++ tabs calls this so
/// the non-whitelist tabs stop 404-ing.
///
/// Missing sources are warnings, not errors: this is additive and must never
/// break a deploy (e.g. when `codegen all` hasn't been run, the binding files
/// simply stay absent rather than aborting the build).
pub fn copy_language_bindings(
    version: &str,
    version_dir: &Path,
    codegen_dir: &Path,
    examples_dir: &Path,
) -> Result<()> {
    println!("  Copying per-language bindings into release/{}/...", version);

    let mut copied = 0usize;
    let mut missing: Vec<String> = Vec::new();

    for bf in BINDING_FILES {
        let src = match bf.source {
            BindingSource::Codegen => codegen_dir.join(bf.src),
            BindingSource::Examples => examples_dir.join(bf.src),
        };

        if !src.exists() {
            missing.push(format!("{} (from {})", bf.dst, src.display()));
            continue;
        }

        let dst = version_dir.join(bf.dst);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dst)?;
        copied += 1;
    }

    // LuaRocks rockspec: filename embeds the release version
    // (`azul-<version>-1.rockspec`, must match the `version = "..."` inside),
    // so it can't live in the const BINDING_FILES list.
    let rockspec = format!("azul-{}-1.rockspec", version);
    let rockspec_src = codegen_dir.join(&rockspec);
    if rockspec_src.exists() {
        fs::copy(&rockspec_src, version_dir.join(&rockspec))?;
        copied += 1;
    } else {
        missing.push(format!("{} (from {})", rockspec, rockspec_src.display()));
    }

    println!(
        "  - Copied {} per-language binding file(s) into release/{}/",
        copied, version
    );

    if !missing.is_empty() {
        eprintln!(
            "  [WARN] {} per-language binding source(s) not found (run `cargo run -p azul-doc -- \
             codegen all` first?):\n    {}",
            missing.len(),
            missing.join("\n    ")
        );
    }

    Ok(())
}

/// Creates examples.zip by reading example paths from api.json and loading files from disk.
/// This ensures the zip always matches the examples defined in api.json.
pub fn create_examples(
    version: &str,
    output_dir: &Path,
    azul_h: &str,
    cpp_headers: &CppHeaders,
    api_data: &ApiData,
    examples_dir: &Path,
) -> Result<()> {
    println!("  Creating example packages...");

    let source_zip_path = output_dir.join("examples.zip");
    let source_zip_file = File::create(&source_zip_path)?;

    let mut source_zip = zip::ZipWriter::new(source_zip_file);
    let options = zip::write::SimpleFileOptions::default();

    // Get examples from api.json for this version
    let version_data = api_data
        .get_version(version)
        .ok_or_else(|| anyhow::anyhow!("Version {} not found in api.json", version))?;

    // Track which files we've already added (to avoid duplicates)
    let mut added_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for example in &version_data.examples {
        // Add C example
        let c_path = &example.code.c;
        if !added_files.contains(c_path) {
            let full_path = examples_dir.join(c_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(c_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(c_path.clone());
            }
        }

        // Add Rust example
        let rust_path = &example.code.rust;
        if !added_files.contains(rust_path) {
            let full_path = examples_dir.join(rust_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(rust_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(rust_path.clone());
            }
        }

        // Add Python example
        let python_path = &example.code.python;
        if !added_files.contains(python_path) {
            let full_path = examples_dir.join(python_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(python_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(python_path.clone());
            }
        }

        // Add C++ examples for each standard
        let cpp_paths = [
            &example.code.cpp03,
            &example.code.cpp11,
            &example.code.cpp14,
            &example.code.cpp17,
            &example.code.cpp20,
            &example.code.cpp23,
            &example.code.cpp, // legacy fallback
        ];

        for cpp_opt in cpp_paths.iter() {
            if let Some(cpp_path) = cpp_opt {
                if !added_files.contains(cpp_path) {
                    let full_path = examples_dir.join(cpp_path);
                    if full_path.exists() {
                        let content = fs::read(&full_path)?;
                        source_zip.start_file(cpp_path, options)?;
                        source_zip.write_all(&content)?;
                        added_files.insert(cpp_path.clone());
                    }
                }
            }
        }

        // Every other language (ada, csharp, java, kotlin, lua, node, ocaml,
        // ruby, go, haskell, ...) is flattened into `extra`. Bundle them all so
        // examples.zip is the one-stop "all languages" source drop, not just
        // C/Rust/Python/C++.
        for path in example.code.extra.values() {
            if !added_files.contains(path) {
                let full_path = examples_dir.join(path);
                if full_path.exists() {
                    let content = fs::read(&full_path)?;
                    source_zip.start_file(path, options)?;
                    source_zip.write_all(&content)?;
                    added_files.insert(path.clone());
                }
            }
        }
    }

    // C header
    source_zip.start_file("include/azul.h", options)?;
    source_zip.write_all(azul_h.as_bytes())?;

    // C++ headers for all standards
    source_zip.start_file("include/cpp/azul03.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp03.as_bytes())?;
    source_zip.start_file("include/cpp/azul11.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp11.as_bytes())?;
    source_zip.start_file("include/cpp/azul14.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp14.as_bytes())?;
    source_zip.start_file("include/cpp/azul17.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp17.as_bytes())?;
    source_zip.start_file("include/cpp/azul20.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp20.as_bytes())?;
    source_zip.start_file("include/cpp/azul23.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp23.as_bytes())?;

    // Add README
    source_zip.start_file("README.md", options)?;
    source_zip.write_all(
        format!(
            "# Azul GUI Framework v{version}\n\n\
             Cross-platform GUI framework with bindings for Rust, C, C++, Python and \
             20+ other languages (all per-language `hello-world` / `widgets` sources are \
             bundled here, one directory per language).\n\n\
             This archive demonstrates the \"one dll, many small binaries\" model: a single \
             shared library (`libazul.so` / `libazul.dylib` / `azul.dll`) plus the compiled \
             demo apps under `demos/` (added by CI) — every app links the same one library \
             instead of bundling its own runtime. Build any example against the bundled lib; \
             headers are in `include/`.\n",
        )
        .as_bytes(),
    )?;

    // Finalize source zip
    source_zip.finish()?;

    println!(
        "  - Created example packages ({} files from api.json)",
        added_files.len()
    );

    Ok(())
}

/// Ship the generated Java (JNA) bindings as a single `azul-java.zip` in the
/// release dir. Java codegen emits ~6.4k per-type `.java` files plus a
/// `pom.xml`; that is far too many to expose (or `curl`) as individual URLs
/// like the other languages, so the Java install step downloads this one zip,
/// unzips it, and `mvn package`s. Sourced from `codegen_dir/java/` (the
/// `azul-doc codegen all` output). A missing source dir is a warning, not an
/// error (mirrors `copy_language_bindings`): the deploy must never abort.
pub fn create_java_bindings_zip(version_dir: &Path, codegen_dir: &Path) -> Result<()> {
    let java_dir = codegen_dir.join("java");
    if !java_dir.is_dir() {
        eprintln!(
            "  [WARN] Java bindings dir {} missing — skipping azul-java.zip (run `azul-doc codegen all`?)",
            java_dir.display()
        );
        return Ok(());
    }

    let zip_path = version_dir.join("azul-java.zip");
    let zip_file = File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default();

    let mut count = 0usize;
    let mut stack = vec![java_dir.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // Store paths relative to java/ (so the zip unpacks to a flat
            // project the bundled pom.xml expects).
            let rel = path.strip_prefix(&java_dir).unwrap();
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            zip.start_file(rel_str, options)?;
            zip.write_all(&fs::read(&path)?)?;
            count += 1;
        }
    }
    zip.finish()?;
    println!("  - Created azul-java.zip ({} files)", count);
    Ok(())
}

/// Render an unconditional `<li>` link into the per-release artifact dir
/// (`https://azul.rs/ui/release/{version}/{filename}`).
///
/// Unlike [`generate_asset_card`], this does NOT probe the filesystem: it always
/// emits a live link. The release page is built by the website-skeleton job
/// (with placeholder binaries) and is NOT regenerated after CI merges the real
/// artifacts — only file sizes are patched in place. So artifacts that aren't
/// placeholdered (Linux .deb/.rpm packages, the PDF guide, exotic-arch DLLs)
/// would never appear if we filtered on presence at generation time. For this
/// comprehensive "every artifact we ship" download index we therefore link
/// them unconditionally; a not-yet-built artifact 404s rather than vanishing.
/// Is `filename` a LARGE asset that must be hosted on the GitHub Release rather
/// than bundled into the GitHub Pages site?
///
/// The static `.a` libs are 200 MB+ each and the demo binaries are large too;
/// nine of those would blow past GitHub Pages' 1 GB artifact limit (and a future
/// Cloudflare Pages target caps single files at 25 MiB). So LARGE assets live on
/// the GitHub Release (≤2 GB/asset): every `.a` static lib, the demo binaries
/// (under the `demos/` path), and the `.deb`/`.rpm` system packages.
///
/// Everything else stays SMALL and on Pages: the ~16 MB `.so`/`.dll`/`.dylib`,
/// `.lib`/`.dll.lib`, the C/C++ headers, the Python extensions, `examples.zip`,
/// `api.json`, the per-language bindings, the `LICENSE-*.txt` files, the PDF.
fn is_large(filename: &str) -> bool {
    filename.ends_with(".a")
        || filename.ends_with(".deb")
        || filename.ends_with(".rpm")
        || filename.starts_with("demos/")
        || filename.contains("/demos/")
}

/// LARGE assets that the deploy step uploads to the Release as `.tar.gz`
/// (uncompressed binaries that compress ~2-3x): the static `.a` libs and the
/// demo executables. Already-compressed packages (.deb/.rpm/...) upload raw.
/// MUST stay in sync with the `*.a|*/demos/*` case in rust.yml's
/// "Host LARGE assets on the GitHub Release" step.
fn is_tarred(filename: &str) -> bool {
    filename.ends_with(".a") || filename.starts_with("demos/") || filename.contains("/demos/")
}

/// Build the download URL for a release asset, routing LARGE assets to the
/// GitHub Release and SMALL assets to the Pages-hosted release dir.
///
/// LARGE (see [`is_large`]) → `https://github.com/fschutt/azul/releases/download/{version}/{basename}`
/// (GitHub flattens the path: a demo at `demos/azul-maps-linux` uploads as the
/// bare `azul-maps-linux` asset). SMALL → `https://azul.rs/ui/release/{version}/{filename}`.
fn asset_url(version: &str, filename: &str) -> String {
    if is_large(filename) {
        // GitHub Release assets are flat — strip any `demos/` path prefix so the
        // link matches the uploaded asset name. Tarred assets (.a, demos) get a
        // `.tar.gz` suffix to match what the deploy step uploaded.
        let basename = filename.rsplit('/').next().unwrap_or(filename);
        let suffix = if is_tarred(filename) { ".tar.gz" } else { "" };
        format!("https://github.com/fschutt/azul/releases/download/{version}/{basename}{suffix}")
    } else {
        format!("{HTML_ROOT}/release/{version}/{filename}")
    }
}

fn release_link_li(version: &str, filename: &str, description: &str) -> String {
    let url = asset_url(version, filename);
    // The description already names the artifact; don't repeat the filename.
    format!("<li><a href='{url}'>{description}</a></li>", url = url, description = description)
}

/// A generic `.docs-card` download/link tile (azul-docs.css): title + optional
/// plain-prose subtitle line.
fn card(url: &str, title: &str, sub: &str) -> String {
    let sub_html =
        if sub.is_empty() { String::new() } else { format!("<p>{sub}</p>", sub = sub) };
    format!(
        "<a class='docs-card' href='{url}'><h4>{title}</h4>{sub_html}</a>",
        url = url,
        title = title,
        sub_html = sub_html
    )
}

/// A `.docs-card` tile with a monospaced filename line (docs-release.css
/// `.docs-card-file`).
fn file_card(url: &str, title: &str, filename: &str) -> String {
    format!(
        "<a class='docs-card' href='{url}'><h4>{title}</h4><p \
         class='docs-card-file'>{filename}</p></a>",
        url = url,
        title = title,
        filename = filename
    )
}

/// Unconditional release-artifact tile (routes LARGE assets to the GitHub
/// Release via [`asset_url`]). Shows the artifact basename under the title.
fn release_card(version: &str, filename: &str, description: &str) -> String {
    let url = asset_url(version, filename);
    let display = filename.rsplit('/').next().unwrap_or(filename);
    file_card(&url, description, display)
}

/// Present/missing-aware release-artifact tile.
fn generate_asset_card(version: &str, asset: &AssetInfo) -> String {
    if asset.is_present {
        // Sizes are intentionally omitted: the release page is generated by the
        // skeleton job from placeholder binaries, so any size shown here would be
        // the wrong (placeholder) value.
        release_card(version, &asset.filename, &asset.description)
    } else {
        format!(
            "<div class='docs-card is-missing'><h4>{description}</h4><p \
             class='docs-card-file'>{filename} - not available</p></div>",
            description = asset.description,
            filename = asset.filename
        )
    }
}

pub fn generate_release_html(version: &str, api_data: &ApiData, assets: &ReleaseAssets) -> String {
    let versiondata = api_data.get_version(version).unwrap();
    let prism_script = crate::docgen::get_prism_script();
    let releasenotes =
        comrak::markdown_to_html(&versiondata.notes.join("\r\n"), &comrak::Options::default());
    let git = &versiondata.git;
    let date = &versiondata.date;

    // Per-OS binary tiles (dll/so/dylib, import/static libs, Python
    // extensions, per-OS license text - same artifact sets as before, now
    // rendered as .docs-card download tiles).
    let card_join = "\n                ";
    let windows_assets: String = assets
        .windows
        .iter()
        .map(|a| generate_asset_card(version, a))
        .collect::<Vec<_>>()
        .join(card_join);

    let linux_assets: String = assets
        .linux
        .iter()
        .map(|a| generate_asset_card(version, a))
        .collect::<Vec<_>>()
        .join(card_join);

    let macos_assets: String = assets
        .macos
        .iter()
        .map(|a| generate_asset_card(version, a))
        .collect::<Vec<_>>()
        .join(card_join);

    // C header tile
    let c_header_link = generate_asset_card(version, &assets.c_header);

    // C++ header tiles (same present/missing handling + URLs as the asset
    // tiles: .hpp files are SMALL assets, so asset_url yields the identical
    // {HTML_ROOT}/release/{version}/{filename} href as before)
    let cpp_header_links: String = assets
        .cpp_headers
        .iter()
        .map(|a| generate_asset_card(version, a))
        .collect::<Vec<_>>()
        .join(card_join);

    // Exotic / cross-arch native libraries. Rendered unconditionally from the
    // static CROSS_ARCH_ASSETS list (NOT the present-only `assets.cross_arch`)
    // so every architecture we ship is linked on this comprehensive index even
    // though the skeleton build doesn't placeholder these files. Spans x86 →
    // riscv: i686/aarch64/armv7, ppc64/s390x/riscv64 (big-endian + RISC-V),
    // Windows i686, macOS Intel, and rust9x (Win98/XP).
    // "Additional architectures" grouped by OS (Windows / macOS / Linux), each
    // as its own sub-list. Classify by filename: azul.i686.*/azul.rust9x.* are
    // Windows, *macos*/*.x86_64.dylib are macOS, the rest (linux-*) are Linux.
    fn cross_group(version: &str, want: fn(&str) -> bool) -> String {
        BinaryAsset::CROSS_ARCH_ASSETS
            .iter()
            .filter(|a| want(a.filename))
            .map(|a| release_card(version, a.filename, a.description))
            .collect::<Vec<_>>()
            .join("\n                ")
    }
    let cross_win = cross_group(version, |f| {
        f.starts_with("azul.i686.") || f.starts_with("azul.rust9x.")
    });
    let cross_mac = cross_group(version, |f| f.contains("macos") || f.contains(".x86_64.dylib"));
    let cross_lin = cross_group(version, |f| f.contains("linux-"));
    let cross_arch_section = format!(
        "\n              <h3>Additional architectures</h3>\n              \
         <h4>Windows</h4>\n              <div class='docs-card-grid'>\n                {win}\n              </div>\n              \
         <h4>macOS</h4>\n              <div class='docs-card-grid'>\n                {mac}\n              </div>\n              \
         <h4>Linux</h4>\n              <div class='docs-card-grid'>\n                {lin}\n              </div>",
        win = cross_win, mac = cross_mac, lin = cross_lin
    );

    // API/examples tiles
    let api_json_link = generate_asset_card(version, &assets.api_json);
    let examples_zip_link = generate_asset_card(version, &assets.examples_zip);

    // ---- Linux packages (.deb / .rpm) ----------------------------------
    // nfpm conventional filenames: deb = `name_version_arch.deb`,
    // rpm = `name-version.arch.rpm`. The amd64 packages are built by
    // build_linux_packages; per-arch (arm64/ppc64/s390x/riscv64) are
    // experimental. nfpm maps the deb arch straight through (amd64/arm64/…)
    // and maps the rpm arch (amd64→x86_64, arm64→aarch64; ppc64/s390x/riscv64
    // pass through). The deploy copies them flat into release/{version}/.
    // Grouped by package type (all .deb, then all .rpm) rather than interleaved
    // by arch, so Debian/Ubuntu users and RPM users each see one contiguous run.
    let package_links: String = [
        ("azul_{V}_amd64.deb", ".deb (x86-64)"),
        ("azul_{V}_arm64.deb", ".deb (ARM64)"),
        ("azul_{V}_ppc64.deb", ".deb (PowerPC64)"),
        ("azul_{V}_s390x.deb", ".deb (s390x)"),
        ("azul_{V}_riscv64.deb", ".deb (RISC-V 64)"),
        // nfpm emits rpms WITH the release suffix (`name-version-1.arch.rpm`);
        // linking them without `-1` 404'd all five tiles (2026-07-04 audit).
        ("azul-{V}-1.x86_64.rpm", ".rpm (x86-64)"),
        ("azul-{V}-1.aarch64.rpm", ".rpm (ARM64)"),
        ("azul-{V}-1.ppc64.rpm", ".rpm (PowerPC64)"),
        ("azul-{V}-1.s390x.rpm", ".rpm (s390x)"),
        ("azul-{V}-1.riscv64.rpm", ".rpm (RISC-V 64)"),
    ]
    .iter()
    .map(|(fname, desc)| release_card(version, &fname.replace("{V}", version), desc))
    .collect::<Vec<_>>()
    .join("\n                ");

    // ---- PDF guide -------------------------------------------------------
    // The docs_pdf job renders the guide+API PDF; the deploy lays it out at
    // release/{version}/guide.pdf.
    let pdf_link = release_card(
        version,
        "guide.pdf",
        "Full guide + API reference (PDF)",
    );

    // ---- Code coverage report -------------------------------------------
    // The `coverage` CI job runs scripts/coverage.sh (grcov + llvm-tools) over
    // the azul-css/azul-core/azul-layout test suites and uploads `coverage/`
    // (an HTML report whose entry point is index.html) as the `coverage-report`
    // artifact. The deploy lays it out at release/{version}/coverage/.
    let coverage_link = release_link_li(
        version,
        "coverage",
        "Code coverage report (grcov HTML, generated by CI)",
    );

    // ---- Statistics / CI reports ----------------------------------------
    // Reports each CI job produces, laid out under release/{version}/statistics/
    // by the deploy job (job -> uploaded artifact -> merged here). Linked
    // unconditionally (like demos/packages): a not-yet-built report 404s rather
    // than vanishing, since the skeleton build doesn't placeholder these. The
    // per-OS rows (status board, dependency tree) cover the three CI runners.
    let stats_links: String = {
        let mut v: Vec<String> = Vec::new();
        v.push(coverage_link);
        v.push(release_link_li(
            version,
            "statistics/dependency-justifications.html",
            "Dependency justifications (why each crate is in the tree)",
        ));
        v.push(release_link_li(
            version,
            "statistics/cargo-deny.txt",
            "Supply-chain audit (cargo-deny: advisories, bans, licenses, sources)",
        ));
        // cargo-geiger: the CI job now pre-fetches crates then runs geiger
        // --offline (dodging the bundled-cargo `pending_ids.insert` panic on
        // this dep graph) and validates the output is a real report, else
        // ships an honest "unavailable" note — so the tile links a truthful
        // artifact again (was dropped when geiger only produced a 287-byte
        // panic stub).
        v.push(release_link_li(
            version,
            "statistics/cargo-geiger.txt",
            "Unsafe-code audit (cargo-geiger)",
        ));
        v.push(release_link_li(
            version,
            "statistics/clippy.txt",
            "Clippy lint summary",
        ));
        v.push(release_link_li(
            version,
            "statistics/sanitizers.txt",
            "Sanitizers (ASan / UBSan / TSan on the C examples)",
        ));
        for (os_label, os_slug) in [("Linux", "ubuntu"), ("macOS", "macos"), ("Windows", "windows")] {
            v.push(release_link_li(
                version,
                &format!("statistics/language-bindings-{os_slug}.txt"),
                &format!("Language binding status board ({os_label})"),
            ));
        }
        for (os_label, os_slug) in [("Linux", "ubuntu"), ("macOS", "macos"), ("Windows", "windows")] {
            v.push(release_link_li(
                version,
                &format!("statistics/dependency-tree-{os_slug}.txt"),
                &format!("Dependency tree ({os_label})"),
            ));
        }
        v.join("\n                ")
    };

    // ---- License files ---------------------------------------------------
    // generate_license_files writes the bundled third-party license text per
    // platform into release/{version}/. The project itself is MIT-licensed.
    const LICENSE_FILES: &[(&str, &str)] = &[
        ("LICENSE-LINUX.txt", "Bundled third-party licenses (Linux build)"),
        ("LICENSE-MACOS.txt", "Bundled third-party licenses (macOS build)"),
        ("LICENSE-WINDOWS.txt", "Bundled third-party licenses (Windows build)"),
        ("LICENSE-IOS.txt", "Bundled third-party licenses (iOS build)"),
        ("LICENSE-ANDROID.txt", "Bundled third-party licenses (Android build)"),
    ];
    let license_links: String = LICENSE_FILES
        .iter()
        .map(|(fname, desc)| release_card(version, fname, desc))
        .collect::<Vec<_>>()
        .join("\n                ");

    // ---- Language bindings ----------------------------------------------
    // Per-language install instructions + a working hello-world. Each entry
    // deep-links to that language's guide page (/guide/hello-world/<lang>),
    // which carries the install steps and the counter example; the examples
    // zip below carries the full source for each. The `lang` slug matches the
    // guide page slug 1:1 (rust/python/c/cpp/csharp/java/kotlin/lua/ruby/node/ocaml).
    const BINDING_LANGS: &[(&str, &str)] = &[
        ("rust", "Rust"),
        ("python", "Python"),
        ("c", "C"),
        ("cpp", "C++"),
        ("csharp", "C# / .NET"),
        ("java", "Java"),
        ("kotlin", "Kotlin"),
        ("lua", "Lua"),
        ("ruby", "Ruby"),
        ("node", "Node.js"),
        ("ocaml", "OCaml"),
    ];
    let binding_links: String = BINDING_LANGS
        .iter()
        .map(|(lang, label)| {
            card(
                &format!("{HTML_ROOT}/guide/hello-world/{lang}"),
                label,
                "Install instructions + hello-world",
            )
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    // ---- Demos — download & run -----------------------------------------
    // Self-contained release binaries of the demo "goal apps" (Rust apps built
    // statically against azul). The build_demos CI job stages them as
    // <crate>-<os>[.exe] and the deploy lays them into release/{version}/demos/.
    // Each demo has up to three OS variants (linux/macos/windows). We link them
    // unconditionally (like the exotic-arch/package links) since the skeleton
    // build doesn't placeholder these — a not-yet-built one 404s rather than
    // vanishing. (crate, friendly name, one-line description.)
    const DEMO_APPS: &[(&str, &str, &str)] = &[
        ("azul-paint", "AzulPaint", "a small raster paint / drawing app"),
        ("azul-widgets", "AzWidgets", "a showcase of all Azul widgets"),
        ("azul-maps", "AzulMaps", "a slippy-map tile viewer"),
        ("azul-vault", "AzulVault", "an encrypted SQLite-backed password vault"),
        ("azul-gamepad", "AzGamepad", "a live gamepad / controller input tester"),
        ("azul-camera-app", "AzCamera", "a webcam capture & preview widget demo"),
        (
            "azul-screenshare-app",
            "AzScreenShare",
            "a screen-capture / screenshare widget demo",
        ),
        ("azul-video-app", "AzVideo", "a video playback widget demo"),
        (
            "azul-meet",
            "AzMeet",
            "a tiny video-call demo (UDP + audio sink + microphone)",
        ),
        (
            "azul-self-test",
            "AzSelfTest",
            "unattended camera/mic/UDP/sensors/gamepad smoke test (logs to a file and exits)",
        ),
    ];
    // OS suffix → label + filename extension, matching the build_demos staging
    // names (azul-maps-linux, azul-maps-macos, azul-maps-windows.exe).
    const DEMO_OSES: &[(&str, &str, &str)] = &[
        ("linux", "Linux", ""),
        ("macos", "macOS", ""),
        ("windows", "Windows", ".exe"),
    ];
    // Grouped by OS: each OS is a heading with a sub-list of "Name: what it is",
    // so the OS and filename aren't repeated on every line. .apk = the demos set
    // up as a NativeActivity cdylib; every demo ships an installable iOS .app.
    const ANDROID_READY: &[&str] = &["azul-maps", "azul-paint", "azul-widgets", "azul-self-test"];
    // (os heading, path template with {c}=crate). Desktop = demos/<crate>-<os>[.exe];
    // mobile = mobile-apps/<crate>-{ios.app.zip,android.apk} (Pages-hosted).
    let os_groups: &[(&str, &str, fn(&str) -> bool)] = &[
        ("Linux", "demos/{c}-linux", |_| true),
        ("macOS", "demos/{c}-macos", |_| true),
        ("Windows", "demos/{c}-windows.exe", |_| true),
        ("iOS device (.ipa, signed)", "mobile-apps/{c}-ios.ipa", |_| true),
        ("iOS device (.app, sign-it-yourself)", "mobile-apps/{c}-ios.app.zip", |_| true),
        ("iOS Simulator (.app, unsigned)", "mobile-apps/{c}-ios-sim.app.zip", |_| true),
        ("Android (.apk, sideload)", "mobile-apps/{c}-android.apk",
            |c| ANDROID_READY.contains(&c)),
    ];
    let mut demo_links: String = os_groups
        .iter()
        .map(|(os_label, path_tmpl, include)| {
            let items: String = DEMO_APPS
                .iter()
                .filter(|(crate_name, _, _)| include(crate_name))
                .map(|(crate_name, friendly, _desc)| {
                    // Description intentionally dropped here: it's stated once under
                    // the "Demos" heading instead of repeated on every OS row.
                    release_link_li(
                        version,
                        &path_tmpl.replace("{c}", crate_name),
                        friendly,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n                    ");
            format!(
                "<li><strong>{os_label}</strong>\n                  \
                 <ul>\n                    {items}\n                  </ul></li>",
                os_label = os_label,
                items = items
            )
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    // Web (Docker): run any demo as a web app with one command. The link is the
    // demo's Dockerfile (a release-hosted copy of examples/<crate>/Dockerfile); it
    // FROMs ghcr.io/fschutt/azul and REUSES the Linux x86-64 desktop
    // binary — azul lifts it to WASM in-process (remill) and serves it, no
    // separate web build, no recompile. The label is the ready-to-run command.
    let web_items: String = DEMO_APPS
        .iter()
        .map(|(crate_name, friendly, _desc)| {
            let url = asset_url(version, &format!("{crate_name}.Dockerfile"));
            format!(
                "<li><strong>{friendly}</strong>\n                      \
                 <pre>\
                 <code class='language-bash'>docker build {url} -t {crate_name}\ndocker run -p 8080:8080 {crate_name}</code></pre></li>",
                friendly = friendly,
                url = url,
                crate_name = crate_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n                    ");
    demo_links.push_str(&format!(
        "\n                <li><strong>Web (Docker, experimental)</strong>\n                  \
         <ul>\n                    {web_items}\n                  </ul></li>",
        web_items = web_items
    ));

    // MIT license tile ahead of the bundled per-platform license texts.
    let mit_card = card(
        &format!("https://github.com/fschutt/azul/blob/{git}/LICENSE"),
        "Azul project license (MIT)",
        "The Azul project itself is MIT-licensed",
    );

    let main_html = format!(
        "<section class='docs-hero'>
      <div class='container'>
        <p class='docs-eyebrow'>Release</p>
        <h1>v{version}</h1>
        <p class='docs-lede'>Released {date} &middot; <a href='https://github.com/fschutt/azul/commit/{git}'>git {git}</a></p>
      </div>
    </section>
    <section class='docs-body'>
      <div class='container'>
        <div class='docs-layout'>
        <div class='docs-content'>
              <div id='releasenotes'>
              {releasenotes}
              </div>

              <nav class='release-jump' aria-label='Release sections'>
                <strong>Jump to:</strong>
                <ul>
                  <li><a href='#native-libraries'>Native libraries</a></li>
                  <li><a href='#debug-libraries'>Debug libraries</a></li>
                  <li><a href='#linux-packages'>Linux packages</a></li>
                  <li><a href='#demos'>Demos</a></li>
                  <li><a href='#language-bindings'>Installation</a></li>
                  <li><a href='#docs-guide'>Docs &amp; guide</a></li>
                  <li><a href='#agentic'>Agentic</a></li>
                  <li><a href='#statistics'>Statistics</a></li>
                  <li><a href='#license'>License</a></li>
                  <li><a href='#source'>Source</a></li>
                </ul>
              </nav>

              <h2 id='native-libraries'>Native libraries</h2>
              <h3>Windows</h3>
              <div class='docs-card-grid'>
                {windows_assets}
              </div>
              <h3>Linux</h3>
              <div class='docs-card-grid'>
                {linux_assets}
              </div>
              <h3>macOS</h3>
              <div class='docs-card-grid'>
                {macos_assets}
              </div>
              {cross_arch_section}

              <h2 id='debug-libraries'>Debug libraries</h2>
              <p class='release-note'>For debugging desktop applications, see the <a href='{HTML_ROOT}/guide/debugging'>Debugging guide</a>.</p>
              <div class='docs-card-grid'>
                <a class='docs-card' href='{HTML_ROOT}/release/{version}/libazuldbg.so'><h4>Debug library (Linux)</h4><p class='docs-card-file'>libazuldbg.so</p></a>
                <a class='docs-card' href='{HTML_ROOT}/release/{version}/libazuldbg.dylib'><h4>Debug library (macOS)</h4><p class='docs-card-file'>libazuldbg.dylib</p></a>
                <a class='docs-card' href='{HTML_ROOT}/release/{version}/azuldbg.dll'><h4>Debug library (Windows)</h4><p class='docs-card-file'>azuldbg.dll</p></a>
              </div>

              <h3>Mobile (iOS &amp; Android): drop-in libraries</h3>
              <p class='release-note'>For shipping on mobile, see the <a href='{HTML_ROOT}/guide/mobile'>mobile deploy guide</a>.</p>
              <div class='docs-card-grid'>
                <a class='docs-card' href='https://github.com/fschutt/azul/releases/download/{version}/libazul-android-arm64.a.tar.gz'><h4>Android arm64-v8a</h4><p class='docs-card-file'>libazul-android-arm64.a.tar.gz</p></a>
                <a class='docs-card' href='https://github.com/fschutt/azul/releases/download/{version}/libazul-android-x64.a.tar.gz'><h4>Android x86_64 (emulator)</h4><p class='docs-card-file'>libazul-android-x64.a.tar.gz</p></a>
                <a class='docs-card' href='https://github.com/fschutt/azul/releases/download/{version}/libazul-ios-arm64.a.tar.gz'><h4>iOS arm64 (device)</h4><p class='docs-card-file'>libazul-ios-arm64.a.tar.gz</p></a>
                <a class='docs-card' href='https://github.com/fschutt/azul/releases/download/{version}/libazul-ios-sim-arm64.a.tar.gz'><h4>iOS arm64 (simulator)</h4><p class='docs-card-file'>libazul-ios-sim-arm64.a.tar.gz</p></a>
              </div>

              <h3>C / C++ headers</h3>
              <div class='docs-card-grid'>
                {c_header_link}
                {cpp_header_links}
              </div>

              <h2 id='linux-packages'>Linux packages</h2>
              <div class='docs-card-grid'>
                {package_links}
              </div>

              <h3>apt (Debian / Ubuntu)</h3>
              <pre><code class='language-bash'># option 1: self-hosted apt repository (amd64 + arm64; unsigned, hence [trusted=yes])
echo 'deb [trusted=yes] {HTML_ROOT}/apt stable main' | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update
sudo apt install azul

# option 2: install the .deb straight from the GitHub release
curl -LO https://github.com/fschutt/azul/releases/download/{version}/azul_{version}_amd64.deb
sudo apt install ./azul_{version}_amd64.deb</code></pre>

              <h3>Homebrew (macOS)</h3>
              <pre><code class='language-bash'># self-hosted tap: a bare git repo served from azul.rs (installs libazul.dylib + azul.h)
brew tap fschutt/azul {HTML_ROOT}/homebrew-azul.git
brew install fschutt/azul/azul</code></pre>

              <h2 id='demos'>Demos</h2>
              <ul class='release-demos' id='demo-list'>
                {demo_links}
              </ul>
              <p class='release-note'><strong>macOS note:</strong> if macOS complains it cannot verify the developer, run <code>sh unquarantine.sh &lt;binary-name&gt;</code> (<a href='{HTML_ROOT}/release/{version}/unquarantine.sh'>unquarantine.sh</a>).</p>
              <p>Building, installing and debugging these &mdash; and going from Rust to a final .apk / .ipa cross-platform &mdash; is covered in the <a href='{HTML_ROOT}/guide/mobile'>Mobile guide</a>.</p>

              <h2 id='language-bindings'>Installation instructions</h2>
              <div class='docs-card-grid'>
                {binding_links}
              </div>
              <p class='release-note'>Azul is NOT published to PyPI, npm, RubyGems, NuGet, Maven Central or crates.io
              (same-named packages there are unrelated projects). Instead, azul.rs self-hosts
              its own distribution channels, regenerated from this exact release by CI
              (a channel whose package did not build in a given run is simply absent):</p>
              <pre><code class='language-bash'># macOS - Homebrew tap (self-hosted bare git repo, see above)
brew tap fschutt/azul {HTML_ROOT}/homebrew-azul.git
brew install fschutt/azul/azul

# Debian / Ubuntu - self-hosted apt repository (unsigned, hence [trusted=yes])
echo 'deb [trusted=yes] {HTML_ROOT}/apt stable main' | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update
sudo apt install azul

# Python - self-hosted PEP 503 index (NOT pypi.org); pip fetches {HTML_ROOT}/azul/
pip install azul --index-url {HTML_ROOT}

# Java / Kotlin - self-hosted maven2 repository
#   repository {HTML_ROOT}/maven + dependency rs.azul:azul:{version}

# Node - install the npm tarball straight from its stable URL
npm install {HTML_ROOT}/npm/azul-{version}.tgz

# C# / Ruby - stable file URLs (use as a local NuGet feed / local gem install)
#   {HTML_ROOT}/nuget/flatcontainer/azul/{version}/azul.{version}.nupkg
#   {HTML_ROOT}/gems/gems/azul-{version}.gem</code></pre>
              <p class='release-note'>Every binding file is also served
              directly from this page:</p>
              <pre><code class='language-bash'># grab one binding file directly (no examples.zip needed):
curl -O {HTML_ROOT}/release/{version}/Azul.cs
curl -O {HTML_ROOT}/release/{version}/Azul.hs
curl -LO {HTML_ROOT}/release/{version}/azul-java.zip</code></pre>

              <h3>Use Azul as a Rust dependency</h3>
              <pre><code class='language-toml'># Cargo.toml (azul is NOT on crates.io; the crate in the repo is azul-dll,
# renamed to `azul` for use)
[dependencies.azul]
package = \"azul-dll\"
git = \"https://github.com/fschutt/azul\"
tag = \"{version}\"

# Dynamic linking against a prebuilt azul.dll / libazul.so:
# features = [\"link-dynamic\"], default-features = false
# export AZ_LINK_PATH=/path/to/libazul</code></pre>

              <h2 id='docs-guide'>Docs &amp; guide</h2>
              <div class='docs-card-grid'>
                <a class='docs-card' href='{HTML_ROOT}/api/{version}'><h4>API documentation</h4><p>API documentation for this release</p></a>
                <a class='docs-card' href='{HTML_ROOT}/guide'><h4>Online guide</h4><p>Tutorials and how-tos</p></a>
                {pdf_link}
                {api_json_link}
                {examples_zip_link}
              </div>

              <h2 id='agentic'>Agentic</h2>
              <div class='docs-card-grid'>
                <a class='docs-card' href='{HTML_ROOT}/skill.md'><h4>AI agent skill (skill.md)</h4><p>install once to prime a coding agent</p></a>
                <a class='docs-card' href='{HTML_ROOT}/llms.txt'><h4>llms.txt</h4><p>compact API + guide index for LLMs</p></a>
                <a class='docs-card' href='{HTML_ROOT}/llms-full.txt'><h4>llms-full.txt</h4><p>full machine-readable index</p></a>
              </div>

              <h3>Deploy a web app (pre-lifted WASM base image: experimental preview)</h3>
              <p><a href='{HTML_ROOT}/guide/deploying-web'>Guide: deploying azul web apps</a></p>
              <pre><code class='language-bash'># prebuilt web base image (experimental; the web backend is not yet stable)
docker pull ghcr.io/fschutt/azul:{version}</code></pre>

              <h2 id='statistics'>Statistics</h2>
              <ul>
                {stats_links}
              </ul>

              <h2 id='license'>License</h2>
              <div class='docs-card-grid'>
                {mit_card}
                {license_links}
              </div>

              <h2 id='source'>Source</h2>
              <ul>
                <li><a href='https://github.com/fschutt/azul'>Git repository</a></li>
                <li><a href='https://github.com/fschutt/azul/tree/{version}'>Source tree at tag {version}</a></li>
                <li><a href='https://github.com/fschutt/azul/releases/tag/{version}'>GitHub release page</a></li>
                <li><a href='https://crates.io/crates/azul/{version}'>Crates.io</a></li>
                <li><a href='https://docs.rs/azul/{version}'>Docs.rs</a></li>
              </ul>
        </div>
        <aside class='docs-search-rail'>
          <div id='azul-search-mount' data-azs-inline></div>
        </aside>
        </div>
      </div>
    </section>"
    );

    // Self-truthing download tiles (2026-07-04 audit: 38 dead links looked
    // like normal downloads). Some artifacts are produced by OTHER CI jobs
    // and merged into the published tree after this page is generated, so
    // presence can't be known here — instead the page HEAD-probes its own
    // same-origin tile links in the browser and marks 404s with the existing
    // `is-missing` style. Cross-origin links (GitHub release assets) are
    // skipped: CORS blocks HEAD, and those large assets are CI-uploaded in
    // the same job that made this page, so they don't have the problem.
    let linkcheck_script = r#"<script>
    document.addEventListener('DOMContentLoaded', function() {
      var cards = Array.prototype.slice.call(
        document.querySelectorAll('a.docs-card, .docs-card a, li a')
      ).filter(function(a) {
        return a.href && a.origin === location.origin &&
               a.pathname.indexOf('/release/') !== -1 &&
               !a.pathname.endsWith('/');
      });
      var queue = cards.slice(); var inflight = 0; var MAX = 6;
      function pump() {
        while (inflight < MAX && queue.length) {
          (function(a) {
            inflight++;
            fetch(a.href, { method: 'HEAD' }).then(function(r) {
              if (r.status === 404) {
                var card = a.closest('.docs-card') || a.closest('li') || a;
                card.classList.add('is-missing');
                card.title = 'Not published for this release (yet)';
              }
            }).catch(function(){}).finally(function(){ inflight--; pump(); });
          })(queue.shift());
        }
      }
      pump();
    });
    </script>"#;

    // Sticky API search rail (user request 2026-07-04): the box stays on
    // screen while scrolling; results expand next to the content column.
    let page = crate::docgen::AzlinPage {
        title: format!("Azul GUI v{version} (git {git}) - Release Notes"),
        active_nav: "releases",
        head_extra: format!(
            "{prism_script}\n{}\n{}",
            crate::docgen::get_search_init(crate::docgen::PageKind::Other),
            linkcheck_script
        ),
        page_css: Some(include_str!("../../templates/docs-release.css")),
        main_html,
    };
    crate::docgen::azlin_page(&page, true)
}

pub fn generate_releases_index(versions: &[String]) -> String {
    // Newest release first (api.json versions arrive sorted ascending from
    // the BTreeMap, so iterate in reverse).
    let mut version_items = String::new();
    for (i, version) in versions.iter().rev().enumerate() {
        let meta = if i == 0 {
            "<p class='docs-meta'>Latest release</p>\n        "
        } else {
            ""
        };
        version_items.push_str(&format!(
            "<article class='docs-list-item'>
        <h3><a href='{HTML_ROOT}/release/{version}'>Azul {version}</a></h3>
        {meta}<p>Native libraries, language bindings, Linux packages, demos and release \
             notes for Azul {version}.</p>
        <a class='docs-read-more' href='{HTML_ROOT}/release/{version}'>Downloads &amp; release \
             notes</a>
      </article>\n      ",
        ));
    }

    let main_html = format!(
        "<section class='docs-hero'>
      <div class='container'>
        <p class='docs-eyebrow'>Releases</p>
        <h1>Releases</h1>
        <p class='docs-lede'>Every published version of the Azul GUI framework, with downloads, release notes and per-language install instructions.</p>
      </div>
    </section>
    <section class='docs-body'>
      <div class='container'>
        <div class='docs-list'>
      {version_items}
        </div>
      </div>
    </section>"
    );

    let page = crate::docgen::AzlinPage {
        title: "Releases - Azul GUI framework".to_string(),
        active_nav: "releases",
        head_extra: String::new(),
        page_css: Some(include_str!("../../templates/docs-release.css")),
        main_html,
    };
    crate::docgen::azlin_page(&page, true)
}

/// HTML body for a redirect stub: meta-refresh + JS replace + `rel=canonical`,
/// preserving any query string / hash. `target` is a site-absolute clean URL
/// (e.g. `/ui/guide/dom`).
fn redirect_stub_html(target: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
         <title>Redirecting\u{2026}</title>\n\
         <link rel=\"canonical\" href=\"{target}\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <meta http-equiv=\"refresh\" content=\"0; url={target}\">\n\
         <script>location.replace(\"{target}\" + location.search + location.hash);</script>\n\
         </head><body>\n\
         <p>This page has moved to <a href=\"{target}\">{target}</a>.</p>\n\
         </body></html>\n"
    )
}

/// Mirror the generated `/ui` page tree with root-level redirect stubs so that
/// links from before the "move docs under /ui" change (May 2026) — and bare
/// `.html` URLs — keep working instead of 404ing.
///
/// For every `ui/<p>.html` we write `<root>/<p>.html` (creating parent dirs)
/// that redirects to the canonical clean URL `/ui/<p>`. A static host (GitHub
/// Pages) serves `<root>/<p>.html` for BOTH `/<p>` and `/<p>.html`, so a single
/// stub covers the old-root path in both its clean and `.html` forms. Stubs are
/// never written over a real root file (the marketing landing's `index.html`,
/// `ws.html`, `os.html`), and the top-level `index.html` is always skipped so
/// the landing page is preserved. Returns the number of stubs written.
///
/// Non-HTML assets (release binaries, CSS, images) are NOT mirrored — a stray
/// legacy link to `/release/...` can't be HTML-redirected sensibly; those are
/// fixed at the source by routing asset URLs through `HTML_ROOT`.
pub fn generate_redirect_stubs(root_dir: &Path, ui_dir: &Path) -> Result<usize> {
    let mut count = 0usize;
    let mut stack = vec![ui_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("html") {
                continue;
            }
            let rel = match path.strip_prefix(ui_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            // Canonical clean target: drop `.html`; an `index.html` maps to its
            // directory URL.
            let target = if rel_str == "index.html" {
                "/ui/".to_string()
            } else if let Some(stem) = rel_str.strip_suffix("/index.html") {
                format!("/ui/{stem}/")
            } else {
                format!("/ui/{}", &rel_str[..rel_str.len() - ".html".len()])
            };
            let dest = root_dir.join(rel);
            // Never clobber a real root file (marketing landing etc.).
            if dest.exists() {
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, redirect_stub_html(&target))?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn copy_static_assets(output_dir: &Path) -> Result<()> {
    println!("Copying static assets...");

    // Get the templates directory (relative to the doc crate root)
    let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let fonts_source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");

    // Create assets directories
    let fonts_dir = output_dir.join("fonts");
    let images_dir = output_dir.join("images");
    fs::create_dir_all(&fonts_dir)?;
    fs::create_dir_all(&images_dir)?;

    // Copy CSS file at runtime (so edits take effect without recompiling)
    fs::copy(templates_dir.join("main.css"), output_dir.join("main.css"))?;

    // Copy JavaScript file at runtime
    fs::copy(
        templates_dir.join("prism_code_highlighter.js"),
        output_dir.join("prism_code_highlighter.js"),
    )?;

    // Search panel assets. Embedded via include_str! so the binary stays
    // self-contained; we still write them out as separate static files so
    // the browser caches them independently of any HTML page.
    const AZUL_SEARCH_JS: &str = include_str!("../../templates/azul-search.js");
    const AZUL_SEARCH_CSS: &str = include_str!("../../templates/azul-search.css");
    fs::write(output_dir.join("azul-search.js"), AZUL_SEARCH_JS)?;
    fs::write(output_dir.join("azul-search.css"), AZUL_SEARCH_CSS)?;

    // TEMPORARY doc-review tool (referenced from get_common_head_tags). Lets the
    // maintainer select text on any page and attach a comment persisted in the
    // browser's IndexedDB, then export every comment as one JSON. Remove this
    // write + the <script> tag + the template file in a later release.
    const AZUL_REVIEW_JS: &str = include_str!("../../templates/azul-review.js");
    fs::write(output_dir.join("azul-review.js"), AZUL_REVIEW_JS)?;

    // Copy logo SVG at runtime
    fs::copy(templates_dir.join("logo.svg"), output_dir.join("logo.svg"))?;

    // Copy favicon.ico for local development
    let favicon_source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/assets/images/favicon.ico");
    if favicon_source.exists() {
        fs::copy(&favicon_source, output_dir.join("favicon.ico"))?;
    }

    // Copy fleur-de-lis SVG (for navigation) at runtime
    fs::copy(
        templates_dir.join("fleur-de-lis.svg"),
        images_dir.join("fleur-de-lis.svg"),
    )?;

    // Copy font files at runtime. Site fonts (see main.css @font-face):
    // Playfair Display (big headings), Imbue (subtitles/section headings, opsz+
    // wght), Red Hat Display (body), Red Hat Mono (code) — all self-hosted OFL.
    for f in [
        "InstrumentSerif-Regular.ttf",
        "Imbue-VariableFont_opsz,wght.ttf",
        "RedHatDisplay-VariableFont_wght.ttf",
        "RedHatMono-VariableFont_wght.ttf",
    ] {
        fs::copy(fonts_source_dir.join(f), fonts_dir.join(f))?;
    }

    // Create favicon
    fs::write(output_dir.join("favicon.ico"), "Favicon placeholder")?;

    println!("Static assets copied successfully");
    Ok(())
}

/// Build the pagefind search index over the generated guide pages.
///
/// Drives the floating search panel on guide pages (the API search uses a
/// codegen-emitted index instead). Tries the `pagefind` binary first, then
/// `npx pagefind` so contributors who don't `cargo install pagefind` can
/// still rely on a Node toolchain.
///
/// Failures here are non-fatal: the JS adapter falls back to the API
/// search index if `/pagefind/pagefind.js` 404s, so guide pages stay
/// searchable either way.
pub fn run_pagefind(deploy_dir: &Path) -> Result<()> {
    use std::process::Command;

    let guide_dir = deploy_dir.join("guide");
    if !guide_dir.is_dir() {
        println!("  [INFO] No guide/ directory in deploy, skipping pagefind");
        return Ok(());
    }

    let site_arg = deploy_dir
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("deploy path is not utf-8"))?
        .to_string();

    // Pagefind args, shared between the direct binary and the npx fallback.
    // `--include-characters` keeps `_` `.` `-` as part of words so technical
    // identifiers (`fn_args`, `azul.rs`, `kebab-case`) match correctly.
    let pf_args: Vec<String> = vec![
        "--site".into(),
        site_arg,
        "--output-subdir".into(),
        "pagefind".into(),
        "--glob".into(),
        "guide/**/*.html".into(),
        "--include-characters".into(),
        "_.-".into(),
        "--quiet".into(),
    ];

    // Try the direct binary first; fall back to `npx` so contributors with
    // a Node toolchain don't need to install the binary separately.
    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    attempts.push(("pagefind", pf_args.clone()));
    let mut npx_args: Vec<String> = vec!["--yes".into(), "pagefind@latest".into()];
    npx_args.extend(pf_args.iter().cloned());
    attempts.push(("npx", npx_args));

    for (cmd, args) in &attempts {
        match Command::new(cmd).args(args).status() {
            Ok(status) if status.success() => {
                println!("  [OK] Generated pagefind index via `{}`", cmd);
                return Ok(());
            }
            Ok(status) => {
                eprintln!("  [WARN] `{}` exited {}, trying next launcher", cmd, status);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Try the next launcher silently — common case when the
                // pagefind binary isn't on PATH but Node is available.
            }
            Err(e) => {
                eprintln!("  [WARN] `{}` invocation failed: {}", cmd, e);
            }
        }
    }

    eprintln!(
        "  [INFO] pagefind not available; guide pages will fall back to the API search index. \
         Install via `cargo install pagefind` or rely on `npx pagefind`."
    );
    Ok(())
}

/// Generate NFPM configuration YAML from api.json package metadata
///
/// This function reads the package configuration from api.json and generates
/// an nfpm.yaml file that can be used to build .deb, .rpm, and .apk packages.
///
/// # Arguments
/// * `version` - The version string to use (e.g., "0.0.5")
/// * `api_data` - The parsed api.json data
/// * `output_dir` - Directory where nfpm.yaml should be written
///
/// # Returns
/// The path to the generated nfpm.yaml file, or an error if package config is missing
pub fn generate_nfpm_yaml(version: &str, api_data: &ApiData, output_dir: &Path) -> Result<PathBuf> {
    let version_data = api_data
        .get_version(version)
        .ok_or_else(|| anyhow::anyhow!("Version {} not found in api.json", version))?;

    let package = version_data
        .package
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No package configuration found for version {}", version))?;

    let yaml = generate_nfpm_yaml_content(version, package);

    // Write the file
    fs::create_dir_all(output_dir)?;
    let output_path = output_dir.join("nfpm.yaml");
    fs::write(&output_path, &yaml)?;

    println!("Generated NFPM config: {}", output_path.display());
    Ok(output_path)
}

/// Generate NFPM YAML content from package config
fn generate_nfpm_yaml_content(version: &str, package: &crate::api::PackageConfig) -> String {
    let mut yaml = String::new();

    // Basic package info
    yaml.push_str(&format!("name: {}\n", package.name));
    yaml.push_str("arch: amd64\n");
    yaml.push_str("platform: linux\n");
    yaml.push_str(&format!("version: {}\n", version));
    if !package.maintainer.is_empty() {
        yaml.push_str(&format!("maintainer: {}\n", package.maintainer));
    }
    yaml.push_str(&format!("description: |\n  {}\n", package.description));
    yaml.push_str(&format!("homepage: {}\n", package.homepage));
    yaml.push_str(&format!("license: \"{}\"\n", package.license));
    if !package.vendor.is_empty() {
        yaml.push_str(&format!("vendor: {}\n", package.vendor));
    }
    if !package.section.is_empty() {
        yaml.push_str(&format!("section: {}\n", package.section));
    }
    if !package.priority.is_empty() {
        yaml.push_str(&format!("priority: {}\n", package.priority));
    }

    // Global dependencies (shared by deb/rpm, but rpm uses different syntax in overrides)
    // These top-level fields apply to all packagers
    if !package.linux.depends.is_empty() {
        yaml.push_str("\ndepends:\n");
        for dep in &package.linux.depends {
            yaml.push_str(&format!("  - {}\n", dep));
        }
    }

    if !package.linux.recommends.is_empty() {
        yaml.push_str("\nrecommends:\n");
        for rec in &package.linux.recommends {
            yaml.push_str(&format!("  - {}\n", rec));
        }
    }

    if !package.linux.suggests.is_empty() {
        yaml.push_str("\nsuggests:\n");
        for sug in &package.linux.suggests {
            yaml.push_str(&format!("  - {}\n", sug));
        }
    }

    // RPM-specific configuration (only group is a valid field here)
    if !package.rpm.group.is_empty() {
        yaml.push_str("\nrpm:\n");
        yaml.push_str(&format!("  group: {}\n", package.rpm.group));
    }

    // Use overrides for packager-specific dependency versions
    let has_rpm_overrides = !package.rpm.depends.is_empty();
    if has_rpm_overrides {
        yaml.push_str("\noverrides:\n");
        yaml.push_str("  rpm:\n");
        if !package.rpm.depends.is_empty() {
            yaml.push_str("    depends:\n");
            for dep in &package.rpm.depends {
                yaml.push_str(&format!("      - {}\n", dep));
            }
        }
    }

    // Contents (files to include) - now under linux
    if !package.linux.contents.is_empty() {
        yaml.push_str("\ncontents:\n");
        for content in &package.linux.contents {
            yaml.push_str(&format!("  - src: {}\n", content.src));
            yaml.push_str(&format!("    dst: {}\n", content.dst));
            if !content.content_type.is_empty() && content.content_type != "file" {
                yaml.push_str(&format!("    type: {}\n", content.content_type));
            }
        }
    }

    yaml
}
