//! C19: the skeleton declares every module of the content family. Every file under `src/` is
//! declared by the module file above it (`lib.rs`, or the `mod.rs` of its folder), every `mod`
//! line has its file, and every stub that W1-S14b pre-declared for a later lane exists: either as
//! the one-line stub `//! owned by lane W1-S14<x>` or, once its lane has replaced it, as that lane's
//! module. The fuzz project declares the seven targets of the family, in order.
//!
//! This test keeps holding while the later lanes fill their stubs and add sibling files inside their
//! own folders: it reads the `mod` lines of each `mod.rs` it meets. It also holds rule 10 of the
//! launch rules (the module tree resolves at every moment): a `mod` line without its file, or a file
//! no `mod` line declares, is a red test here and a panic in the gates that walk the declarations.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The module names a source file declares with `mod x;`, `pub mod x;` or `pub(crate) mod x;`
/// (comments are skipped).
fn declared_modules(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let t = match t.strip_prefix("pub(crate) ") {
            Some(rest) => rest,
            None => t.strip_prefix("pub ").unwrap_or(t),
        };
        if let Some(rest) = t.strip_prefix("mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim();
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

/// The modules a folder holds as files and sub-folders, apart from the module file itself.
fn modules_on_disk(dir: &Path, module_file: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(dir).expect("a folder under src") {
        let entry = entry.expect("a directory entry");
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue; // `.DS_Store` and the like are no modules
        }
        if path.is_dir() {
            assert!(
                path.join("mod.rs").is_file(),
                "{} is a folder without a mod.rs: a module folder holds its mod.rs",
                path.display()
            );
            names.insert(file_name);
        } else {
            assert!(
                file_name.ends_with(".rs"),
                "{}: only .rs files live under src",
                path.display()
            );
            if file_name != module_file {
                names.insert(file_name.trim_end_matches(".rs").to_string());
            }
        }
    }
    names
}

/// Checks one folder and, recursively, its sub-folders.
fn check_folder(dir: &Path, module_file: &str) {
    let text = fs::read_to_string(dir.join(module_file)).expect("the module file of the folder");
    let declared = declared_modules(&text);
    let on_disk = modules_on_disk(dir, module_file);
    let orphans: Vec<_> = on_disk.difference(&declared).collect();
    let missing: Vec<_> = declared.difference(&on_disk).collect();
    assert!(
        orphans.is_empty(),
        "{}: files that no `mod` line declares (an orphan no build compiles): {orphans:?}",
        dir.display()
    );
    assert!(
        missing.is_empty(),
        "{}: `mod` lines without their file: {missing:?}",
        dir.display()
    );
    for name in &on_disk {
        let sub = dir.join(name);
        if sub.is_dir() {
            check_folder(&sub, "mod.rs");
        }
    }
}

#[test]
fn c19_skeleton_declares_every_family_module() {
    let src = crate_root().join("src");
    check_folder(&src, "lib.rs");

    // The stubs of the later lanes: each exists; while it is still a stub it is the one line
    // `//! owned by lane W1-S14<x>` of the lane the brief names.
    let stubs: [(&str, &str); 5] = [
        ("pack/mod.rs", "W1-S14c"),
        ("pointer/mod.rs", "W1-S14d"),
        ("index/mod.rs", "W1-S14e1"),
        ("dedup/mod.rs", "W1-S14e1"),
        ("prune/mod.rs", "W1-S14e2"),
    ];
    let lib = fs::read_to_string(src.join("lib.rs")).expect("src/lib.rs");
    let declared = declared_modules(&lib);
    for (path, lane) in stubs {
        let file = src.join(path);
        let text =
            fs::read_to_string(&file).unwrap_or_else(|_| panic!("the stub {path} is missing"));
        let first = text.lines().next().unwrap_or("");
        let still_a_stub = first.starts_with("//! owned by lane W1-S14");
        if still_a_stub {
            assert_eq!(
                text.trim_end(),
                format!("//! owned by lane {lane}"),
                "{path}: a stub is exactly one line naming its lane"
            );
        }
        let module = path.split('/').next().unwrap_or("");
        assert!(
            declared.contains(module),
            "lib.rs does not declare the module of {path}"
        );
        assert!(!text.trim().is_empty(), "{path} is empty");
    }

    // The crate root keeps its attributes (later lanes never edit lib.rs).
    assert!(lib.contains("#![forbid(unsafe_code)]"));

    // The vocabulary and the chunk modules of W1-S14b exist and are not stubs.
    for name in ["ids", "units", "error", "limits", "codec", "chunk"] {
        let text = fs::read_to_string(src.join(format!("{name}.rs"))).expect("a module of S14b");
        assert!(
            !text.starts_with("//! owned by lane"),
            "{name}.rs is a real module of W1-S14b"
        );
        assert!(declared.contains(name), "lib.rs does not declare {name}");
    }
}

#[test]
fn c19_the_fuzz_project_declares_the_seven_targets_of_the_family() {
    let manifest =
        fs::read_to_string(crate_root().join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    let mut bins = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in manifest.lines() {
        let t = line.trim();
        if t == "[[bin]]" {
            if let Some(done) = current.take() {
                bins.push(done);
            }
            current = Some((String::new(), String::new()));
        } else if let Some((name, path)) = current.as_mut() {
            if let Some(v) = t.strip_prefix("name = ") {
                *name = v.trim_matches('"').to_string();
            } else if let Some(v) = t.strip_prefix("path = ") {
                *path = v.trim_matches('"').to_string();
            }
        }
    }
    if let Some(done) = current.take() {
        bins.push(done);
    }
    let want = [
        "chunk_open",
        "pack_header",
        "pack_trailer",
        "pointer_decode",
        "chunk_list_decode",
        "index_shard",
        "prune_journal",
    ];
    let names: Vec<&str> = bins.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, want, "the seven pre-declared targets, in this order");
    for (name, path) in &bins {
        assert_eq!(path, &format!("fuzz_targets/{name}.rs"), "{name}");
    }
    // Its own workspace and lock, libfuzzer-sys 0.4, the crate by path.
    assert!(
        manifest.contains("\n[workspace]\n"),
        "the fuzz project is its own workspace"
    );
    assert!(manifest.contains("libfuzzer-sys = \"0.4\""));
    assert!(manifest.contains("azlin-content = { path = \"..\" }"));
    assert!(manifest.contains("cargo-fuzz = true"));
    let ignore = fs::read_to_string(crate_root().join("fuzz/.gitignore")).expect("fuzz/.gitignore");
    assert!(ignore.lines().any(|l| l == "Cargo.lock") && ignore.lines().any(|l| l == "corpus/"));
}
