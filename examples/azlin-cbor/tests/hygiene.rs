//! C18 (it supersedes C14: the crate has ONE dependency since `M-CBOR-SECRET`, decision D-W1S14A and
//! rule R17), the part a test can hold: the manifest names `zeroize` and nothing else, with default
//! features off and `alloc` only; azul's lock gives `azlin-cbor` that one edge; the crate root forbids
//! unsafe code and denies the panicking constructs; and the library code names no clock, file,
//! network, thread, random source or panicking call (rules 6 and 7 of the launch rules).
//! `cargo clippy --all-targets -- -D warnings`, `cargo check --target wasm32-unknown-unknown`,
//! `cargo doc` and `cargo tree` are the lane's holds: cargo cannot run inside `cargo test`.

use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The lines of `text` that are not test code, numbered from 1: an item (or statement) that follows
/// a `#[cfg(test)]` line is left out, to its closing brace or its semicolon.
fn non_test_code(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "#[cfg(test)]" {
            let (mut j, mut depth, mut opened) = (i + 1, 0i32, false);
            while j < lines.len() {
                let line = lines[j];
                for c in line.chars() {
                    match c {
                        '{' => {
                            depth += 1;
                            opened = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                j += 1;
                if (opened && depth <= 0) || (!opened && line.trim_end().ends_with(';')) {
                    break;
                }
            }
            i = j;
            continue;
        }
        out.push((i + 1, lines[i]));
        i += 1;
    }
    out
}

#[test]
fn c18_crate_hygiene() {
    // The manifest: exactly one dependency, zeroize, default features off, `alloc` only; no
    // dev-dependency, no build-dependency, no feature.
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
            dependencies.push(l.to_string());
        } else {
            assert!(
                !section.contains("dependencies") && section != "[features]",
                "{section} has a line: {l} (the crate has one dependency, in [dependencies])"
            );
        }
    }
    assert_eq!(
        dependencies.len(),
        1,
        "one dependency, zeroize: {dependencies:?}"
    );
    let dep = &dependencies[0];
    assert!(
        dep.starts_with("zeroize = "),
        "the one dependency is zeroize: {dep}"
    );
    assert!(
        dep.contains("default-features = false") && dep.contains("features = [\"alloc\"]"),
        "zeroize with default features off and `alloc` only: {dep}"
    );

    // azul's lock: the entry of this crate lists zeroize and nothing else (the one edge).
    if let Ok(lock) = fs::read_to_string(root().join("../../Cargo.lock")) {
        let entries: Vec<&str> = lock
            .split("\n\n")
            .filter(|b| b.contains("name = \"azlin-cbor\""))
            .collect();
        assert_eq!(entries.len(), 1, "one lock entry of azlin-cbor");
        assert!(
            entries[0].ends_with("dependencies = [\n \"zeroize\",\n]"),
            "the lock entry of azlin-cbor lists zeroize only: {}",
            entries[0]
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

    // The library code, comments and test code left out; for the crate root only what follows its
    // attributes (which name `unsafe_code` and the denied lints).
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
    for name in [
        "lib", "value", "enc", "dec", "int_map", "limits", "error", "secret",
    ] {
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("source file");
        let mut code = non_test_code(&text);
        if name == "lib" {
            let first = code
                .iter()
                .position(|(_, l)| l.starts_with("pub mod "))
                .expect("the module declarations");
            code.drain(..first);
        }
        assert!(code.len() > 5, "src/{name}.rs: no library code found");
        for (n, line) in code {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/{name}.rs:{n}: `{b}` in library code: {t}"
                );
            }
        }
    }
}
