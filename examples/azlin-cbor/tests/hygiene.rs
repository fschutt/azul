//! C14, the part a test can hold: the manifest has no dependency of any kind, the crate root
//! forbids unsafe code and denies the panicking constructs, and the library code names no clock,
//! file, network, thread, random source or panicking call (rules 6 and 7 of the launch rules).
//! `cargo clippy --all-targets -- -D warnings`, `cargo check --target wasm32-unknown-unknown` and
//! `cargo tree` are the lane's holds: cargo cannot run inside `cargo test`.

use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn c14_crate_hygiene() {
    // No dependency, no dev-dependency, no build-dependency, no feature to enable one.
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let mut section = String::new();
    for line in manifest.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
            continue;
        }
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        assert!(
            !section.contains("dependencies") && section != "[features]",
            "{section} has a line: {l} (the crate has no dependency: std only)"
        );
    }

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

    // The library code: everything before a `#[cfg(test)]` module, comments left out; for the crate
    // root only the part after its attributes (which name `unsafe_code` and the denied lints).
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
    ];
    for name in ["lib", "value", "enc", "dec", "int_map", "limits", "error"] {
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
