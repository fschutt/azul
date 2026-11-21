//! C16: hygiene of the pack module. The library code of `src/pack/` names no clock, file, network, thread,
//! random source or panicking call and lowers no lint (rules 6 and 7 of the launch rules), every public item
//! names its caller or the later unit it is a seam of (rule 9), every file of the folder is declared by
//! `pack/mod.rs` and every declared file exists (rule 10: the module tree resolves), and each file cites the
//! plan by section. `cargo clippy --all-targets -- -D warnings` on Rust 1.91 and on 1.99, rustfmt and the
//! wasm32 check are the lane's holds: cargo cannot run inside `cargo test`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn pack_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/pack")
}

/// The module files of the lane, by name.
const FILES: [&str; 9] = [
    "mod",
    "header",
    "trailer",
    "padding",
    "builder",
    "reader",
    "repack",
    "sealed_check",
    "names",
];

fn read(name: &str) -> String {
    fs::read_to_string(pack_dir().join(format!("{name}.rs"))).expect("a source file of the lane")
}

#[test]
fn c16_crate_hygiene() {
    // Everything before a `#[cfg(test)]` module, comments left out.
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
        "expect(",
    ];
    let mut lines_read = 0usize;
    for name in FILES {
        let text = read(name);
        let code = text.split("#[cfg(test)]").next().expect("a first part");
        for (n, line) in code.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            lines_read += 1;
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/pack/{name}.rs line {}: `{b}` in library code: {t}",
                    n + 1
                );
            }
            // Indexing a slice or an array by a number read from outside is clippy's `indexing_slicing`
            // (denied at the crate root); a bare `[..]` or `[n]` after an identifier is the shape to find.
            let bytes = t.as_bytes();
            for (i, &b) in bytes.iter().enumerate() {
                if b == b'[' && i > 0 {
                    let before = bytes[i - 1];
                    let indexing = before.is_ascii_alphanumeric()
                        || before == b'_'
                        || before == b')'
                        || before == b']';
                    let attribute_or_type = t.starts_with("#[") || t.starts_with("#![");
                    let after = bytes.get(i + 1).copied().unwrap_or(b' ');
                    let literal_type = after == b'u' || after == b'&' || after == b']';
                    assert!(
                        !indexing || attribute_or_type || literal_type || t.contains("macro_rules"),
                        "src/pack/{name}.rs line {}: indexing in library code: {t}",
                        n + 1
                    );
                }
            }
        }
    }
    assert!(
        lines_read > 500,
        "the scan read only {lines_read} lines: has the layout changed?"
    );
}

/// Rule 9 of the launch rules: every public item carries a doc line `Caller: ...` or `Seam: ...`. Checked for
/// every item that starts with `pub` and one of `fn`, `struct`, `enum`, `trait`, `const`, `type`, at any
/// indentation (methods and associated constants included); fields and variants are documented, and named
/// by their type.
#[test]
fn c16_every_public_item_names_its_caller_or_its_seam() {
    let mut checked = 0usize;
    for name in FILES {
        let text = read(name);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            let is_item = [
                "pub fn ",
                "pub struct ",
                "pub enum ",
                "pub trait ",
                "pub const ",
                "pub type ",
            ]
            .iter()
            .any(|p| t.starts_with(p));
            if !is_item {
                continue;
            }
            let mut doc = String::new();
            let mut j = i;
            while j > 0 {
                j -= 1;
                let above = lines[j].trim();
                if let Some(rest) = above.strip_prefix("///") {
                    doc.push_str(rest);
                    doc.push('\n');
                } else if above.starts_with("#[") {
                    continue;
                } else {
                    break;
                }
            }
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/pack/{name}.rs line {}: `{t}` has no `Caller:` or `Seam:` doc line",
                i + 1
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 45,
        "the scan found only {checked} public items: has the layout changed?"
    );
}

/// The module tree resolves (rule 10): `mod.rs` declares exactly the files of the folder, and every file it
/// declares is there; every other module of the crate is declared by `lib.rs` as before.
#[test]
fn c16_the_pack_module_tree_resolves() {
    let mod_rs = read("mod");
    let mut declared = BTreeSet::new();
    for line in mod_rs.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let t = t
            .strip_prefix("pub(crate) ")
            .or_else(|| t.strip_prefix("pub "))
            .unwrap_or(t);
        if let Some(name) = t.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) {
            declared.insert(name.trim().to_string());
        }
    }
    let mut on_disk = BTreeSet::new();
    for entry in fs::read_dir(pack_dir()).expect("the pack folder") {
        let path = entry.expect("an entry").path();
        let file = path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .to_string();
        if file.starts_with('.') {
            continue;
        }
        assert!(
            file.ends_with(".rs"),
            "{file}: only .rs files live in src/pack"
        );
        if file != "mod.rs" {
            on_disk.insert(file.trim_end_matches(".rs").to_string());
        }
    }
    assert_eq!(
        declared, on_disk,
        "mod.rs declares the files of the folder, and only those"
    );
    let expected: BTreeSet<String> = FILES
        .iter()
        .filter(|f| **f != "mod")
        .map(|f| f.to_string())
        .collect();
    assert_eq!(on_disk, expected, "the files of the brief");

    // lib.rs declares the module, and has not been changed by this lane (its attributes are S14b's).
    let lib = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("lib.rs");
    assert!(lib.contains("pub mod pack;"));
    assert!(lib.contains("#![forbid(unsafe_code)]"));
}

/// The code cites the plan (planning/architecture.md section 5.7 and the others it implements).
#[test]
fn c16_every_file_cites_the_plan() {
    for name in FILES {
        // The text of the file with its comment markers and line breaks flattened, so that a citation
        // that is wrapped over two lines of a doc comment is still read.
        let text = read(name);
        let flat: Vec<&str> = text
            .split_whitespace()
            .filter(|word| *word != "//!" && *word != "///")
            .collect();
        assert!(
            flat.join(" ").contains("planning/architecture.md section"),
            "src/pack/{name}.rs does not cite planning/architecture.md by section"
        );
    }
    // The drift rows the code refers to exist in the register, by id: only checked when the register is
    // present (the azul checkout does not carry it).
    let register = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../azul-apps/notes/audits/closeout-2026-11/delta/W1S14C.tsv");
    if let Ok(rows) = fs::read_to_string(&register) {
        let mut cited = BTreeSet::new();
        for name in FILES {
            for word in read(name).split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
                if word.starts_with("X-DELTA-W1S14C-") {
                    cited.insert(word.to_string());
                }
            }
        }
        for id in &cited {
            assert!(
                rows.contains(id.as_str()),
                "{id} is cited in src/pack and is not in {}",
                register.display()
            );
        }
        assert!(!cited.is_empty());
    }
}
