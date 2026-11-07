//! Gate: the system's temp folder is read in `azul_storage::scratch` and nowhere else of the
//! shipped code (AZ-C6, AZ-W4, AZ-U14, X-AR5a-23, ARCH28 RF48 / AR5a-8).
//!
//! What a program keeps for a while and nobody asked for - the spool of an encrypted write, the
//! copy of a cloud file in its viewer, a page waiting for the PDF reader, the token vault of a run
//! that knows no configuration folder - lived in `std::env::temp_dir()` under a fixed or
//! guessable name, made with the default mode: another user could read it, or make the folder
//! first, and a kill left it for good. `Scratch` is the one place: a folder of this user's only,
//! checked on every use, with names that say which process made a file, swept. This gate keeps
//! the raw calls out of the four storage crates' shipped code: `env::temp_dir` as a call and as a
//! function value (`.unwrap_or_else(std::env::temp_dir)`).
//!
//! What stays is a counted allowance with its reason: the test helper, the harness, and the two
//! places that name a temp folder because no configuration folder is known (their lanes convert
//! them to private per-user folders). The counts are exact and only go down. Comments and test
//! items are not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{Allow, Budget, Gate, Located, Orphans, Root, Rule};

/// The rule's name.
const TEMP: &str = "temp-dir";

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 80,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 72,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 88,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 26,
    },
];

/// Every read of the temp folder: `env::temp_dir` followed by a call, or alone as a function
/// value. A local named `temp_dir` is none.
fn temp_dir_reads(t: &[Located]) -> Vec<usize> {
    let mut lines = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if x.tok.is_ident("temp_dir")
            && i >= 3
            && t[i - 1].tok.is_punct(':')
            && t[i - 2].tok.is_punct(':')
            && t[i - 3].tok.is_ident("env")
        {
            lines.push(x.line);
        }
    }
    lines
}

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        TEMP,
        temp_dir_reads,
        &[
            "let dir = std::env::temp_dir().join(\"AzDrive-open\");",
            "let dir = config.unwrap_or_else(std::env::temp_dir);",
            "let here = env::temp_dir();",
            "use std::env::temp_dir;",
        ],
        &[
            "let temp_dir = folder.join(\"preview\");",
            "let dir = scratch::root();",
            "let dir = Scratch::new(\"open\").dir().to_path_buf();",
            "let here = std::env::current_dir();",
            "let named = \"temp_dir\";",
            "let job = Job::Preview { temp_dir };",
        ],
    )]
}

/// Where the temp folder is read today. TO SHRINK, never to grow: the text says who removes it.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/scratch.rs",
        TEMP,
        1,
        "the one place: the root of a program that has no cache folder is `<temp>/azul-scratch-<uid>`, \
         made 0700 and checked on every use",
    ),
    Allow::exactly(
        "examples/azul-storage/src/testing.rs",
        TEMP,
        1,
        "TempDir: the folder of this crate's tests and (the `testing` feature) the apps' tests; \
         a test's own, named after its process and a counter, removed on drop",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/shared.rs",
        TEMP,
        1,
        "lock_folder_for: the keyring's lock folder when the system knows no configuration folder \
         at all - every program of the user must agree on one path, so it cannot be a run's \
         scratch; Z30-B (a private per-user folder)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/e2e/",
        TEMP,
        2,
        "the E2E commands (registered only when AZ_E2E or AZ_DEBUG is set) make their working \
         folders in the system's temp folder, as another program on the computer would; counted, \
         so a new one is seen",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the temp folder is read in scratch.rs only; the rest is the counted list",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_temp_folder_is_read_in_scratch_rs_and_the_counted_allowances_only() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every read: `cargo test -p azul-srcgate --test scratch_gate -- --ignored --nocapture
/// print_reads`, to write the list above.
#[test]
#[ignore = "prints the reads; the list above is written from it"]
fn print_reads() {
    let hits = azul_srcgate::hits(
        &azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}")),
        &rules(),
    );
    let mut per_file: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for h in &hits {
        *per_file.entry(h.rel.as_str()).or_default() += 1;
    }
    for (file, n) in per_file {
        println!("{n:>3} {file}");
    }
}

/// Every allowance names a file that exists: a deleted or renamed file is a failure on its own.
#[test]
fn every_allowance_names_a_file_that_exists() {
    let base = azul_srcgate::workspace();
    for a in ALLOWED {
        assert!(
            base.join(a.file.trim_end_matches('/')).exists(),
            "{} is allowed ({}) and is not there",
            a.file,
            a.why
        );
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-scratch-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azul-drive/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/opening.rs",
    "temp-dir",
    2,
    "the planted reads",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

#[test]
fn a_planted_temp_dir_read_in_an_unlisted_file_is_caught_and_a_listed_one_is_counted() {
    let planted = [
        "pub fn open_dir() -> std::path::PathBuf {",
        "    std::env::temp_dir().join(\"AzDrive-open\")",
        "}",
        "pub fn fallback(config: Option<std::path::PathBuf>) -> std::path::PathBuf {",
        "    config.unwrap_or_else(std::env::temp_dir)",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "planted",
        &[
            ("examples/azul-drive/src/lib.rs", "mod opening;\n"),
            ("examples/azul-drive/src/opening.rs", planted.as_str()),
        ],
    );
    // nobody may read it: both the call and the function value are findings, with their lines.
    let wrong = check(&base, &[]).expect_err("an unlisted read is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/opening.rs:2")),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/opening.rs:5")),
        "{wrong:?}"
    );
    // the list takes them, by count.
    assert_eq!(check(&base, LISTED), Ok(2));
    // a third read in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azul-drive/src/opening.rs"),
        format!("{planted}pub fn more() {{\n    let _ = std::env::temp_dir();\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("one more read than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x temp-dir where its allowance says 2")),
        "{wrong:?}"
    );
    // a site that was converted leaves a stale count: the list shrinks with the code.
    let converted = [
        "pub fn open_dir() -> std::path::PathBuf {",
        "    azul_storage::scratch::Scratch::new(\"open\").dir().to_path_buf()",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azul-drive/src/opening.rs"), converted).unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer reads than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x temp-dir where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_local_named_temp_dir_a_comment_a_message_and_a_test_are_no_read() {
    let quiet = [
        "// std::env::temp_dir() is what scratch.rs reads",
        "/// `env::temp_dir()` is the shared folder",
        "pub fn a(temp_dir: &std::path::Path) -> std::path::PathBuf {",
        "    println!(\"std::env::temp_dir()\");",
        "    temp_dir.join(\"preview\")",
        "}",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t() {",
        "        let _ = std::env::temp_dir();",
        "    }",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "quiet",
        &[("examples/azul-drive/src/lib.rs", quiet.as_str())],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
