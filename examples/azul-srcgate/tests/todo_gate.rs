//! Gate: no `todo!()` or `unimplemented!()` in the shipped code of azlin-preflate, azlin-compress
//! and azul-storage (AZ-C2, ARCH28 AR5a-10 / AR5b-0). These crates read bytes other devices and
//! other people wrote - a deflate stream, a compressed object, an answer of a bucket - and a
//! macro that panics on a path nobody wrote is a crash a stranger can cause. AZ-C2 took the
//! last ones out; the gate keeps them out. A real panic of an invariant is `unreachable!` or an
//! error; a missing feature is an `Unsupported` error. The budget is empty: there is nothing to
//! allow.
//!
//! Test code is not read (`#[cfg(test)]` items and `#[test]` functions are blanked), and a
//! `.rs` file under the roots that no build compiles fails the gate.

use std::path::{Path, PathBuf};

use azul_srcgate::{macro_calls, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azlin-preflate/src",
        min_files: 10,
    },
    Root {
        dir: "examples/azlin-compress/src",
        min_files: 10,
    },
    Root {
        dir: "examples/azul-storage/src",
        min_files: 30,
    },
];

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            "todo!",
            |t| macro_calls(t, "todo"),
            &["todo!();", "let _ = std::todo!(\"later\");"],
            &[
                "let todo = 1;",
                "let x = todo != 2;",
                "unreachable!(\"never\");",
            ],
        ),
        Rule::new(
            "unimplemented!",
            |t| macro_calls(t, "unimplemented"),
            &["unimplemented!();", "unimplemented!(\"the {x} case\")"],
            &["let unimplemented = 1;", "let _ = \"unimplemented!()\";"],
        ),
    ]
}

fn gate(roots: &'static [Root]) -> Gate<'static> {
    Gate {
        name: "no todo!/unimplemented! in azlin-preflate, azlin-compress and azul-storage",
        roots,
        rules: rules(),
        budget: Budget(&[]),
        orphans: Orphans::Fail,
    }
}

#[test]
fn the_storage_crates_hold_no_todo_and_no_unimplemented() {
    let found = gate(ROOTS).run();
    assert!(found.is_empty(), "{found:?}");
}

/// A gate that read nothing would pass: the roots hold the shipped files of the three crates -
/// their module trees, deep files included - and not their test files.
#[test]
fn the_gate_reads_the_shipped_files_of_the_three_crates_and_not_their_tests() {
    let read = azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Fail).unwrap();
    let rels: Vec<&str> = read.iter().map(|f| f.rel.as_str()).collect();
    for shipped in [
        "examples/azul-storage/src/lib.rs",
        "examples/azul-storage/src/meta/wal.rs",
        "examples/azul-storage/src/overlap.rs",
        "examples/azlin-preflate/src/lib.rs",
        "examples/azlin-compress/src/lib.rs",
    ] {
        assert!(
            rels.contains(&shipped),
            "{shipped} is not read: {} files",
            rels.len()
        );
    }
    assert!(
        !rels
            .iter()
            .any(|r| r.contains("/tests/") || r.ends_with("_tests.rs")),
        "a test file is read: {rels:?}"
    );
    let wal = read
        .iter()
        .find(|f| f.rel.ends_with("meta/wal.rs"))
        .unwrap();
    assert_eq!(wal.text.lines().count(), wal.source.lines().count());
}

/// A crate on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("azul-srcgate-todo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azul-storage/src",
    min_files: 1,
}];

fn check(base: &Path) -> Result<usize, Vec<String>> {
    gate(SMALL).check(base).map(|hits| hits.len())
}

#[test]
fn a_planted_todo_or_unimplemented_is_caught_with_its_file_and_line() {
    let base = base_with(
        "planted",
        &[
            (
                "examples/azul-storage/src/lib.rs",
                "mod inner;\npub fn a() -> u8 {\n    todo!()\n}\n",
            ),
            (
                "examples/azul-storage/src/inner.rs",
                "pub fn b() {\n    let _ = 1;\n    unimplemented!(\"b\");\n}\n",
            ),
        ],
    );
    let wrong = check(&base).expect_err("the planted macros are findings");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("examples/azul-storage/src/lib.rs:3") && w.contains("todo!")),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("examples/azul-storage/src/inner.rs:3")
                && w.contains("unimplemented!")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_todo_in_test_code_or_a_comment_is_no_finding_and_a_file_no_build_compiles_is() {
    let base = base_with(
        "clean",
        &[
            (
                "examples/azul-storage/src/lib.rs",
                "// todo!()\npub fn a() -> u8 {\n    1\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        todo!();\n    }\n}\n#[test]\nfn also_a_test() {\n    unimplemented!();\n}\n",
            ),
        ],
    );
    assert_eq!(check(&base), Ok(0));
    // A file nothing declares is code no gate can vouch for: it fails, with its path.
    std::fs::write(
        base.join("examples/azul-storage/src/forgotten.rs"),
        "pub fn c() { todo!() }\n",
    )
    .unwrap();
    let wrong = check(&base).expect_err("an undeclared file fails the gate");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("forgotten.rs") && w.contains("no build compiles")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_root_that_holds_too_few_files_was_read_in_the_wrong_place() {
    let base = base_with(
        "few",
        &[("examples/azul-storage/src/lib.rs", "pub fn a() {}\n")],
    );
    let wrong = gate(ROOTS)
        .check(&base)
        .expect_err("fewer files than the gate counts on");
    assert!(
        wrong.iter().any(|w| w.contains("fewer than the")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}
