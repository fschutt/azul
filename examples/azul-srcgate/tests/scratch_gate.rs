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
//!
//! Second rule (ARCH29 C6a, X-DELTA-AZDRIVE-SYNC-2): the copy of a drive's file that an app is
//! handed is ONE procedure, `Scratch::open_copy` - a folder per file and version, fetched only into
//! a free place, a stranger's file put aside, marked as downloaded, read-only, and noted as made so
//! that a sweep never takes the user's edit. It was written twice and the second copy forgot the
//! lessons of the first (an edit saved beside the copy was swept after three days). Its pieces
//! (`Scratch::keep`, `Scratch::note_made`, `make_read_only` and `quarantine::mark_downloaded`) are
//! public; a call of one outside `scratch.rs` is a procedure composed by hand and is a finding
//! unless it is listed with its reason. `mark_downloaded` also marks what a download or the sync
//! writes into a folder of the user's (those are listed: no copy is handed to an app there); a
//! NEW call is the question "is this a copy for an app?" asked at the line that makes it
//! (ARCH29 C6a, X-DELTA-AZDRIVE-FILES-19).

use std::path::{Path, PathBuf};

use azul_srcgate::{Allow, Budget, Gate, Located, Orphans, Root, Rule};

/// The rule's name.
const TEMP: &str = "temp-dir";

/// The second rule's name: a piece of the open-copy procedure called outside it.
const COPY_PIECE: &str = "copy-piece";

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

/// Every call of a piece of the open-copy procedure: `note_made`, `make_read_only` and
/// `mark_downloaded` by their names (a path before them does not matter, a definition
/// `fn note_made(` and an import `use ..::mark_downloaded;` are none) and `.keep(` in a file that
/// names `Scratch` (another type's `keep` is none: the file must name the type).
fn copy_pieces(t: &[Located]) -> Vec<usize> {
    let names_scratch = t.iter().any(|x| x.tok.is_ident("Scratch"));
    let mut lines = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let called = t.get(i + 1).is_some_and(|next| next.tok.is_punct('('));
        let defined = i >= 1 && t[i - 1].tok.is_ident("fn");
        if !called || defined {
            continue;
        }
        let method = i >= 1 && t[i - 1].tok.is_punct('.');
        let piece = x.tok.is_ident("note_made")
            || x.tok.is_ident("make_read_only")
            || x.tok.is_ident("mark_downloaded")
            || (x.tok.is_ident("keep") && method && names_scratch);
        if piece {
            lines.push(x.line);
        }
    }
    lines
}

fn rules() -> Vec<Rule> {
    vec![
        temp_dir_rule(),
        Rule::new(
            COPY_PIECE,
            copy_pieces,
            &[
                "let made = Scratch::note_made(&folder, &name);",
                "let _ = scratch::make_read_only(&dest);",
                "let folder = Scratch::new(\"open\").keep(&tag).unwrap();",
                "make_read_only(&dest).ok();",
                "Scratch::under(&root, \"open\")\n    .keep(&tag)\n    .unwrap();",
                "quarantine::mark_downloaded(&dest);",
                "mark_downloaded(&copy);",
            ],
            &[
                "let kept = rotation.keep();",
                "let copy = Scratch::new(\"open\").open_copy(of, &mut fetch);",
                "let state = Scratch::made_state(&folder, &name);",
                "let path = scratch.kept(&tag);",
                "fn note_made(folder: &Path) {}",
                "fn make_read_only(path: &Path) {}",
                "fn mark_downloaded(path: &Path) {}",
                "let note_made = 1;",
                "let marked = quarantine::is_marked(&dest);",
                "use crate::quarantine::mark_downloaded;",
            ],
        ),
    ]
}

fn temp_dir_rule() -> Rule {
    Rule::new(
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
    )
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
        "examples/azul-storage/src/scratch.rs",
        COPY_PIECE,
        4,
        "the one place: `Scratch::open_copy` calls `keep`, `mark_downloaded`, `make_read_only` and \
         `note_made` once each, in that order, so that no copy is handed out without its folder, \
         its mark, its mode and its record",
    ),
    Allow::exactly(
        "examples/azul-storage/src/transfer.rs",
        COPY_PIECE,
        1,
        "copy_object_when: the destination of a drive-to-disk transfer (a Download, a Paste, the \
         rescue copy of a read-only drive) carries the system's `downloaded from elsewhere` mark \
         (AZ-W20); it is the user's file in a folder they chose, no copy handed to an app (that \
         is Scratch::open_copy)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        COPY_PIECE,
        1,
        "fetch_one: a file the sync writes into the synced folder from another person's device \
         carries the mark (AZ-W20); it is a file of the user's folder, no copy handed to an app \
         (the decrypted copy of an encrypted pairing is session.rs's `decrypt`, which asks \
         Scratch::open_copy)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/picker.rs",
        COPY_PIECE,
        1,
        "copy_root: the picker's STAGING folder of one drive (`picked`, one kept folder a drive): \
         the transfer queue fetches the picked objects into it and `hand_out` moves each into an \
         open copy (`Scratch::open_copy`: a folder per file and version, read-only, noted as made) \
         before the app is given anything; nothing is handed out from it (X-DELTA-SYNC2-4)",
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
        name:
            "the temp folder is read, and a copy for an app is made, in scratch.rs only; the rest \
               is the counted list",
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

const LISTED_PIECES: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/opening.rs",
    COPY_PIECE,
    3,
    "the planted pieces",
)];

/// X-DELTA-AZDRIVE-SYNC-2. The open-copy procedure composed by hand - a folder kept, made
/// read-only, noted - outside `scratch.rs` is a finding, with its lines; a listed file is counted,
/// and a count that no longer holds (the code was converted to `Scratch::open_copy`) fails too.
#[test]
fn a_planted_open_copy_composed_by_hand_is_caught_and_a_listed_one_is_counted() {
    let planted = [
        "use azul_storage::scratch::{self, Scratch};",
        "pub fn open_copy(copies: &Scratch, name: &str) {",
        "    let folder = copies",
        "        .keep(\"tag\")",
        "        .unwrap();",
        "    let _ = scratch::make_read_only(&folder.join(name));",
        "    let _ = Scratch::note_made(&folder, name);",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "pieces",
        &[
            ("examples/azul-drive/src/lib.rs", "mod opening;\n"),
            ("examples/azul-drive/src/opening.rs", planted.as_str()),
        ],
    );
    let wrong = check(&base, &[]).expect_err("an unlisted procedure is a finding");
    assert_eq!(wrong.len(), 3, "{wrong:?}");
    for line in [4, 6, 7] {
        assert!(
            wrong
                .iter()
                .any(|w| w.starts_with(&format!("examples/azul-drive/src/opening.rs:{line}"))),
            "line {line} in {wrong:?}"
        );
    }
    assert_eq!(check(&base, LISTED_PIECES), Ok(3));
    // a fourth piece in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azul-drive/src/opening.rs"),
        format!("{planted}pub fn more(c: &Scratch) {{\n    let _ = c.keep(\"other\");\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED_PIECES).expect_err("one more piece than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 4 x copy-piece where its allowance says 3")),
        "{wrong:?}"
    );
    // converted to the one procedure: the list shrinks with the code.
    let converted = [
        "use azul_storage::scratch::{CopyOf, Scratch};",
        "pub fn open_copy(copies: &Scratch, of: CopyOf<'_>) {",
        "    let _ = copies.open_copy(of, &mut |_, _| Ok(()));",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azul-drive/src/opening.rs"), converted).unwrap();
    let wrong = check(&base, LISTED_PIECES).expect_err("fewer pieces than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x copy-piece where its allowance says 3")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

const LISTED_MARKS: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/opening.rs",
    COPY_PIECE,
    2,
    "the planted marks",
)];

/// ARCH29 C6a: the download mark is a piece of the open-copy procedure too. A copy handed to an app
/// that is marked by hand (`quarantine::mark_downloaded` through its path or imported) outside
/// `scratch.rs` is a finding with its line; a listed file is counted (a new mark beside the listed
/// one is seen), and a count that no longer holds (the code asks `Scratch::open_copy`) fails too.
#[test]
fn a_planted_mark_downloaded_outside_scratch_rs_is_caught_and_a_listed_one_is_counted() {
    let planted = [
        "use azul_storage::quarantine::{self, mark_downloaded};",
        "pub fn hand_over(copy: &std::path::Path, other: &std::path::Path) {",
        "    quarantine::mark_downloaded(copy);",
        "    mark_downloaded(other);",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "marks",
        &[
            ("examples/azul-drive/src/lib.rs", "mod opening;\n"),
            ("examples/azul-drive/src/opening.rs", planted.as_str()),
        ],
    );
    let wrong = check(&base, &[]).expect_err("an unlisted mark is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    for line in [3, 4] {
        assert!(
            wrong
                .iter()
                .any(|w| w.starts_with(&format!("examples/azul-drive/src/opening.rs:{line}"))),
            "line {line} in {wrong:?}"
        );
    }
    // the import on line 1 is no call.
    assert!(
        !wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/opening.rs:1:")),
        "{wrong:?}"
    );
    assert_eq!(check(&base, LISTED_MARKS), Ok(2));
    // a third mark in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azul-drive/src/opening.rs"),
        format!("{planted}pub fn more(p: &std::path::Path) {{\n    mark_downloaded(p);\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED_MARKS).expect_err("one more mark than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x copy-piece where its allowance says 2")),
        "{wrong:?}"
    );
    // converted to the one procedure: the list shrinks with the code.
    let converted = [
        "use azul_storage::scratch::{CopyOf, Scratch};",
        "pub fn hand_over(copies: &Scratch, of: CopyOf<'_>) {",
        "    let _ = copies.open_copy(of, &mut |_, _| Ok(()));",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azul-drive/src/opening.rs"), converted).unwrap();
    let wrong = check(&base, LISTED_MARKS).expect_err("fewer marks than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x copy-piece where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// Another type's `keep`, a file that does not name `Scratch`, a definition, a comment and a test
/// are no piece of the procedure.
#[test]
fn another_types_keep_a_definition_a_comment_and_a_test_are_no_piece_of_the_procedure() {
    let quiet = [
        "// Scratch::note_made(&folder, name), scratch::make_read_only(&path) and",
        "// quarantine::mark_downloaded(&path) are scratch's",
        "pub fn paid(list: &mut PaidList, record: &PaidRecord) {",
        "    list.keep(record);",
        "    let kept = rotation.keep();",
        "}",
        "pub fn note_made(folder: &std::path::Path) -> bool {",
        "    folder.exists()",
        "}",
        "pub fn mark_downloaded(path: &std::path::Path) -> bool {",
        "    path.exists()",
        "}",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t(c: &Scratch) {",
        "        let _ = c.keep(\"tag\");",
        "        let _ = Scratch::note_made(&folder, \"a\");",
        "        quarantine::mark_downloaded(&folder);",
        "    }",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "pieces-quiet",
        &[("examples/azul-drive/src/lib.rs", quiet.as_str())],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
