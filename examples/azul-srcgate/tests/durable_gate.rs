//! Gate: a file of the user's, of this computer's state or of a cache is put in place only through
//! `azul_storage::durable` (AZ-U5, AZ-S19, X-AR5a-4/-6, ARCH28 RF41 / AR5a-1 / AR5a-10).
//!
//! About nineteen private implementations of "write a file and put it in place" had three
//! durability levels (none, the file, the file and its folder), five temporary names and no mode
//! or time policy: a power cut left an empty target, a copy lost the 0600 of a key, a failed
//! write left hidden files in the user's folder. `durable::{publish, replace, replace_cache,
//! replace_keeping}` is the one way: staged beside the name, synced, checked against what the name
//! must still hold, renamed, the folder synced. This gate keeps the raw calls out of the four
//! storage crates' shipped code: `fs::rename`, `fs::copy`, `File::create`, `fs::write` and a
//! `.create(true)` / `.create_new(true)` open. The descriptor-relative primitives of `beneath.rs`
//! (and, outside this gate's roots, `durable.rs` itself) are where they live.
//!
//! What stays is a counted allowance with its reason, and the lane that removes it: a lock file
//! made with `create_new` (the file IS the mutual exclusion), an append-only log (a line appended
//! is no file put in place), a rename that is the user's own rename or a marker moved aside, the
//! test harness (it plays other actors on the disk on purpose), and the writers of files whose
//! lane has not converted them yet. The counts are exact and only go down: a new raw write is a
//! failure, and so is an allowance whose site went away - the commit that converts a writer
//! lowers its count in the same commit.
//!
//! What it finds is what is written: `use std::fs::rename; rename(a, b)` is no `fs::rename(`.
//! Comments and test items are not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, Allow, Budget, Gate, Located, Orphans, Root, Rule};

/// The rule's name.
const RAW: &str = "raw-write";

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 65,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 20,
    },
];

/// Every raw call that puts a file in place: the four functions, and a `.create(true)` or
/// `.create_new(true)` of an `OpenOptions`.
fn raw_writes(t: &[Located]) -> Vec<usize> {
    let mut lines = Vec::new();
    for path in ["fs::rename", "fs::copy", "File::create", "fs::write"] {
        lines.extend(calls(t, path));
    }
    for (i, x) in t.iter().enumerate() {
        let creates = x.tok.is_ident("create") || x.tok.is_ident("create_new");
        if creates
            && i > 0
            && t[i - 1].tok.is_punct('.')
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('('))
            && t.get(i + 2).is_some_and(|n| n.tok.is_ident("true"))
        {
            lines.push(x.line);
        }
    }
    lines.sort_unstable();
    lines
}

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        RAW,
        raw_writes,
        &[
            "std::fs::rename(&tmp, path)?;",
            "let n = fs::copy(&from, &to)?;",
            "let f = std::fs::File::create(path)?;",
            "std::fs::write(path, bytes)?;",
            "let f = OpenOptions::new().write(true).create_new(true).open(p)?;",
            "let f = fs::File::options().create(true).append(true).open(p)?;",
        ],
        &[
            "dir.rename(from, &target, to)?;",
            "let f = File::open(path)?;",
            "let f = OpenOptions::new().append(true).open(p)?;",
            "let ok = options.create(false);",
            "durable::replace(path, bytes, Mode::Shared)?;",
            "let rename = 1; let write = 2;",
        ],
    )]
}

/// Where a raw write stands today. TO SHRINK, never to grow: the group says who converts it.
const ALLOWED: &[Allow] = &[
    // ---- the one module of the descriptor-relative primitives ----
    Allow::file(
        "examples/azul-storage/src/beneath.rs",
        "the primitives durable.rs is built on: create_new and rename of a folder handle, and \
         their path-based fallbacks where the system has no descriptor-relative calls (Windows, \
         until Z30-PLAT's win.rs)",
    ),
    // ---- A. a file that IS a token: a lock, an append-only log, a marker ----
    Allow::exactly(
        "examples/azul-storage/src/lock.rs",
        RAW,
        1,
        "the lock file of a user's processes: opened (or made) to be flock'ed - the file is the \
         mutual exclusion, there is no content to put in place (moved down from azcloud-kit; \
         StateDir::lock takes it now, with no age: W13)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/docs.rs",
        RAW,
        1,
        "a document that is no JSON is renamed aside (`.damaged-<time>`) before a fresh one is \
         written: a quarantine rename, its bytes stay",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/bucket.rs",
        RAW,
        1,
        "FileLock: the `<object>.lock` of a folder bucket, flock'ed for one compare and one write",
    ),
    Allow::exactly(
        "examples/azul-storage/src/manifest.rs",
        RAW,
        1,
        "the manifest log of a data tree is append-only: a line appended is no file put in place",
    ),
    Allow::exactly(
        "examples/azul-storage/src/packing.rs",
        RAW,
        1,
        "the known-objects log of a pack cache is append-only (Z30-R3)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/journal.rs",
        RAW,
        1,
        "the sync's journal of intents is append-only: its lines ARE the record",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/oplog.rs",
        RAW,
        1,
        "the operations log is append-only: its lines ARE the record",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/migrate.rs",
        RAW,
        1,
        "the note of a first-start migration is appended to the legacy folder's note file",
    ),
    Allow::exactly(
        "examples/azul-storage/src/multipart.rs",
        RAW,
        2,
        "the record of a streamed upload is written and flock'ed under a draft name, then \
         renamed: the lock follows the file through the rename, and the record is advisory (a \
         sweep aborts an upload no process holds)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/base.rs",
        RAW,
        1,
        "the old JSON base is renamed to `.migrated` once, after the database holds it: a marker \
         moved aside",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/local.rs",
        RAW,
        1,
        "a state file nobody can read is moved aside to `.unreadable`: a quarantine rename",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/quarantine.rs",
        RAW,
        1,
        "Windows: the Zone.Identifier stream of a downloaded file - a named stream of that file, \
         not a file put in place",
    ),
    // ---- B. the test harness plays other actors on the disk on purpose ----
    Allow::file(
        "examples/azul-drive/src/e2e/",
        "the E2E commands (registered only when AZ_E2E or AZ_DEBUG is set) make, move, copy and \
         tamper with files the way another device, another program or a damaged disk would",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/folder_remote.rs",
        RAW,
        6,
        "the folder remote plays the harness' server (adapter `engine`): its blob store, its \
         counter, its request log and its lock are the server's own, no product data",
    ),
    // ---- C. the writers whose lane has not converted them yet (each lowers its count) ----
    Allow::exactly(
        "examples/azul-storage/src/encrypted_upload.rs",
        RAW,
        1,
        "the piece an encrypted copy is written into until it is whole (then adopted by \
         publish_staged, in the same folder): a seekable file beside its final name, AZL1 \
         writes its header last. The folder is the resume store of an upload, swept by age \
         (`sweep`) - not scratch (RF48 decided so). A `durable` writer that hands out a seekable \
         staged file would replace it: Z30-A1B",
    ),
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        RAW,
        3,
        "the three job states of the encryption flows: Z30-A1B (AR5b-4 step 2)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/paper.rs",
        RAW,
        1,
        "write_private: the printed recovery sheet's copy, 0600: Z30-A1B (durable::replace with \
         Mode::Private)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/picker.rs",
        RAW,
        1,
        "the file picker's registration beside drives.json: Z30-C2 (durable::replace)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/renewals.rs",
        RAW,
        2,
        "the shown-notices file and the waitlist: Z30-B (durable::replace)",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/azlin_config.rs",
        RAW,
        2,
        "AzlinConfig::store writes `<path>.tmp` and renames it (a fixed temp name, the default \
         mode): Z30-C2 (AR5b-4 step 2, durable::replace with Mode::Private)",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/update.rs",
        RAW,
        4,
        "the updater's state file (tmp and rename), the install copy and the kept download's \
         rename: Z30-C1 (X-AR5a-6, durable::replace / publish)",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/ui.rs",
        RAW,
        1,
        "`--shot`: the screenshot a developer asked for, written where they said",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "files are put in place only through azul_storage::durable",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn files_are_put_in_place_only_through_the_durable_module_and_the_counted_allowances() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every raw write: `cargo test -p azul-srcgate --test durable_gate -- --ignored
/// --nocapture print_writes`, to write the list above.
#[test]
#[ignore = "prints the raw writes; the list above is written from it"]
fn print_writes() {
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

/// The allowances of every file are exactly the files the gate reads: a stale entry (a file
/// that was deleted or renamed) is a failure on its own, with the entry's reason.
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
        "azul-srcgate-durable-{name}-{}",
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
    dir: "examples/azul-storage/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::exactly(
    "examples/azul-storage/src/spool.rs",
    "raw-write",
    2,
    "the planted writer",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

#[test]
fn a_planted_raw_write_in_an_unlisted_file_is_caught_and_a_listed_one_is_counted() {
    let planted = [
        "pub fn save(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {",
        "    let tmp = path.with_extension(\"tmp\");",
        "    std::fs::write(&tmp, bytes)?;",
        "    std::fs::rename(&tmp, path)",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "planted",
        &[
            ("examples/azul-storage/src/lib.rs", "mod spool;\n"),
            ("examples/azul-storage/src/spool.rs", planted.as_str()),
        ],
    );
    // nobody may write raw: both calls are findings, with their lines.
    let wrong = check(&base, &[]).expect_err("an unlisted raw write is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-storage/src/spool.rs:3")),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-storage/src/spool.rs:4")),
        "{wrong:?}"
    );
    // the list takes them, by count.
    assert_eq!(check(&base, LISTED), Ok(2));
    // a third raw write in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azul-storage/src/spool.rs"),
        format!("{planted}pub fn more() {{\n    let _ = std::fs::File::create(\"x\");\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("one more write than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x raw-write where its allowance says 2")),
        "{wrong:?}"
    );
    // a writer that was converted leaves a stale count: the list shrinks with the code.
    let converted = [
        "pub fn save(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {",
        "    azul_storage::durable::replace(path, bytes, azul_storage::durable::Mode::Shared)",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azul-storage/src/spool.rs"), converted).unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer writes than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x raw-write where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_write_named_in_a_comment_a_test_a_message_or_made_by_a_folder_handle_is_no_raw_write() {
    let quiet = [
        "// std::fs::write(path, bytes)",
        "/// `fs::rename(a, b)` is what durable does",
        "pub fn a(dir: &Dir) {",
        "    println!(\"fs::copy(a, b)\");",
        "    dir.rename(\"a\", dir, \"b\").unwrap();",
        "    let _ = File::open(\"x\");",
        "}",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t() {",
        "        std::fs::write(\"x\", b\"y\").unwrap();",
        "        let _ = std::fs::File::create(\"z\");",
        "    }",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "quiet",
        &[("examples/azul-storage/src/lib.rs", quiet.as_str())],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
