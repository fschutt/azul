//! C22, the part a test can hold: the manifest lists exactly the dependencies of the brief's section
//! 3 (and none of the forbidden ones), the crate root forbids unsafe code and denies the panicking
//! constructs, the library code of this lane's modules names no clock, file, network, thread, random
//! source or panicking call (rules 6 and 7 of the launch rules), and every public item names its
//! caller or the later unit it is a seam of (rule 9).
//!
//! `cargo clippy --all-targets -- -D warnings`, the build on Rust 1.91 and on 1.99,
//! `cargo check --target wasm32-unknown-unknown` and `cargo tree` are the lane's holds: cargo cannot
//! run inside `cargo test`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The keys of a manifest section (`[dependencies]`, `[features]`): the text before ` =`.
fn section_keys(manifest: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut keys = Vec::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if !inside || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = t.split_once('=') {
            keys.push(key.trim().to_string());
        }
    }
    keys
}

/// The value text of one key of a manifest section (up to the end of the line).
fn section_value(manifest: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if inside {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// The source files of W1-S14b that are library code (the stubs belong to the later lanes).
const LIBRARY_FILES: [&str; 9] = [
    "lib", "ids", "units", "error", "limits", "codec", "chunk", "gear", "chunker",
];

#[test]
fn c22_crate_hygiene() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // The dependencies of the brief's section 3, exactly: nothing more, nothing less.
    let dependencies: BTreeSet<String> = section_keys(&manifest, "[dependencies]")
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = [
        "azlin-keys",
        "azlin-cbor",
        "blake3",
        "chacha20poly1305",
        "zeroize",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        dependencies, expected,
        "the dependencies of the crate are the brief's section 3"
    );
    for forbidden in [
        "zstd",
        "brotli",
        "azlin-compress",
        "fastcdc",
        "rand",
        "tokio",
        "serde",
        "serde_json",
        "subtle",
        "thiserror",
        "hex",
        "sha2",
        "getrandom",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    // azlin-keys without its default features (the OS entropy source stays out of the content crate).
    let keys = section_value(&manifest, "[dependencies]", "azlin-keys").expect("azlin-keys line");
    assert!(keys.contains("default-features = false"), "{keys}");
    // The only dev-dependency is azlin-keys with its test doubles.
    let dev: Vec<String> = section_keys(&manifest, "[dev-dependencies]");
    assert_eq!(dev, vec!["azlin-keys".to_string()]);
    let dev_keys =
        section_value(&manifest, "[dev-dependencies]", "azlin-keys").expect("azlin-keys line");
    assert!(dev_keys.contains("test-support"), "{dev_keys}");
    assert!(section_keys(&manifest, "[build-dependencies]").is_empty());
    assert!(!root().join("build.rs").exists());
    // No features: the manifest is final (R2), and the crate has none.
    assert!(section_keys(&manifest, "[features]").is_empty());

    // Package facts: edition, Rust version, not published.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-content\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "edition").as_deref(),
        Some("\"2021\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "rust-version").as_deref(),
        Some("\"1.91\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "publish").as_deref(),
        Some("false")
    );

    // The crate root.
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    for lint in [
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::panic",
        "clippy::unreachable",
        "clippy::todo",
        "clippy::unimplemented",
        "clippy::indexing_slicing",
    ] {
        assert!(lib.contains(lint), "the crate root does not deny {lint}");
    }
    assert!(lib.contains("#![warn(missing_docs)]"));

    // The library code of this lane's modules: everything before a `#[cfg(test)]` module, comments
    // left out; for the crate root only the part after its attributes.
    let banned = [
        "unsafe",
        "std::fs",
        "std::net",
        "std::time",
        "std::thread",
        "std::process",
        "std::env",
        "SystemTime",
        "Instant",
        "rand::",
        "serde",
        ".unwrap()",
        ".expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "unimplemented!",
        "unwrap_or_default",
        "unwrap_unchecked",
        "get_unchecked",
        "mem::transmute",
        "println!",
        "dbg!",
        ".ok()",
        "let _ =",
        "allow(",
    ];
    for name in LIBRARY_FILES {
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("source file");
        let code = text.split("#[cfg(test)]").next().expect("a first part");
        let code = if name == "lib" {
            code.split("pub mod ")
                .nth(1)
                .expect("the module declarations")
        } else {
            code
        };
        for (n, line) in code.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/{name}.rs: `{b}` in library code: {t} (near line {})",
                    n + 1
                );
            }
        }
    }
}

/// Rule 9 of the launch rules: every public item carries a doc line `Caller: ...` or `Seam: ...`.
/// Checked for the items at the start of a line (types, traits, functions, constants) and for the
/// items of the `bytes_id!` and `contexts!` macros; methods are covered by their type.
#[test]
fn c22_every_public_item_names_its_caller_or_its_seam() {
    let mut checked = 0usize;
    for name in LIBRARY_FILES {
        if name == "lib" {
            continue;
        }
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("source file");
        let lines: Vec<&str> = text.lines().collect();
        let mut in_macro_call = false;
        for (i, line) in lines.iter().enumerate() {
            if *line == "bytes_id! {" || *line == "contexts! {" {
                in_macro_call = true;
                continue;
            }
            if in_macro_call && *line == "}" {
                in_macro_call = false;
                continue;
            }
            let is_item = (line.starts_with("pub ")
                && !line.starts_with("pub use ")
                && !line.starts_with("pub mod "))
                || (in_macro_call
                    && line.starts_with("    ")
                    && !line.trim().is_empty()
                    && !line.trim_start().starts_with("//")
                    && !line.trim_start().starts_with("#["));
            if !is_item {
                continue;
            }
            // The doc block above (attributes in between are skipped).
            let mut doc = String::new();
            let mut j = i;
            while j > 0 {
                j -= 1;
                let t = lines[j].trim();
                if let Some(rest) = t.strip_prefix("///") {
                    doc.push_str(rest);
                    doc.push('\n');
                } else if t.starts_with("#[") {
                    continue;
                } else {
                    break;
                }
            }
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/{name}.rs line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 40,
        "the scan found only {checked} public items: has the layout changed?"
    );
}
