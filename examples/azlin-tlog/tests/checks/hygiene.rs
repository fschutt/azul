//! The crate's rules (launch rules 6 and 7, brief section 3), the part a test can hold: the manifest names the
//! four allowed dependencies and nothing else; the crate root forbids unsafe code and denies the panicking
//! constructs; every source file is declared; and the library code names no clock, file, network, thread,
//! random source, environment, hash map (no random iteration order) or panicking call. `cargo clippy
//! --all-targets -- -D warnings`, `cargo check --target wasm32-unknown-unknown` and `rustfmt --check` are the
//! lane's holds: cargo cannot run inside `cargo test`.

use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The module names of `src/`: every `*.rs` file but `lib.rs`.
fn source_files() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root().join("src"))
        .expect("src")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".rs"))
        .map(|name| name.trim_end_matches(".rs").to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn the_manifest_names_the_four_allowed_dependencies_and_nothing_else() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let mut section = String::new();
    let mut dependencies: Vec<String> = Vec::new();
    for line in manifest.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
            continue;
        }
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if section == "[dependencies]" {
            dependencies.push(l.split('=').next().unwrap().trim().to_string());
        } else {
            assert!(
                !section.contains("dependencies") && section != "[features]",
                "{section} has a line: {l} (the crate has its dependencies in [dependencies] only)"
            );
        }
    }
    dependencies.sort();
    assert_eq!(
        dependencies,
        ["azlin-merkle", "base64", "ed25519-dalek", "sha2"],
        "serde, thiserror, rand and every I/O crate are forbidden"
    );
}

#[test]
fn the_crate_root_forbids_unsafe_and_denies_the_panicking_constructs() {
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
}

#[test]
fn every_source_file_is_declared_in_the_crate_root() {
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    let names = source_files();
    assert!(names.len() >= 12, "{names:?}");
    for name in names.iter().filter(|n| *n != "lib") {
        assert!(
            lib.contains(&format!("\nmod {name};")),
            "src/{name}.rs is not declared in lib.rs"
        );
    }
}

#[test]
fn the_library_code_is_pure_and_cannot_panic() {
    let banned = [
        "unsafe",
        "std::fs",
        "std::net",
        "std::io",
        "std::time",
        "std::thread",
        "std::process",
        "std::env",
        "SystemTime",
        "Instant",
        "rand::",
        "HashMap",
        "HashSet",
        ".unwrap()",
        ".expect(",
        "unwrap_or",
        "panic!",
        "unreachable!",
        "todo!",
        "unimplemented!",
        "assert!",
        "assert_eq!",
        "println!",
        "eprintln!",
        "dbg!",
        "unwrap_unchecked",
        "get_unchecked",
        "mem::transmute",
        "#[cfg(test)]",
        "#[allow(",
    ];
    let mut checked = 0;
    for name in source_files() {
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("source file");
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") || t.starts_with("#![") {
                continue;
            }
            checked += 1;
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/{name}.rs:{}: `{b}` in library code: {t}",
                    n + 1
                );
            }
            // a slice index with a variable is a panic in waiting: the lint denies it, this reads the same
            assert!(!t.contains("as_ptr"), "src/{name}.rs:{}: {t}", n + 1);
        }
    }
    assert!(checked > 1_500, "the scan read {checked} lines");
}

#[test]
fn the_planning_sections_the_crate_implements_are_cited() {
    // every module cites planning/architecture.md by section (code cites the plan: launch rule 12)
    for name in [
        "bundle",
        "checkpoint",
        "cosign",
        "log",
        "monitor",
        "note",
        "policy",
        "record",
        "witness",
    ] {
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("source file");
        assert!(
            text.contains("planning/architecture.md section 2.20"),
            "src/{name}.rs does not cite planning/architecture.md section 2.20"
        );
    }
}
