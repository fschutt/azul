//! Gate: a name from a drive, a device, a user or a listing becomes a path only through the name
//! policy of `azul_storage::names` (AZ-W1, AZ-W7, X-AR5a-1/-2/-13, ARCH28 RF42 / AR5a-3).
//!
//! Three rules over the shipped code of the four storage crates:
//!
//! - **`bookkeeping-name`**: the name of the engine's own folder (`.azlin`, and the paths of the
//!   sync inside it) is spelled out in `names.rs` only. Twelve places compared a name against the
//!   literal, case-sensitively, so a remote `.GIT` or `.AZLIN` walked past the check on a disk that
//!   folds case; they ask `Protected::is_bookkeeping` now, and the constants (`BOOKKEEPING`,
//!   `SYNC_DIR`, `TRASH_DIR`, `SPOOL_DIR`, `RECORD_FILE`) are used where the engine builds one of
//!   its own paths. What the rule does not read: the keys of the drive's own protocol
//!   (`.azlin/keys/..`, `.azlin/meta/..`, `.azlin/drop/..`): they are constants of their modules,
//!   written by this software and never compared with a name from a drive.
//! - **`vcs-name`**: the same for the version-control folders (`.git`, `.hg`, `.svn`): a name from
//!   a drive is compared with them through `Protected::is_vcs` (every spelling a disk reads as
//!   theirs), and the literals stand in `names.rs`, in the scan's exclusion rules and where AzDrive
//!   reads a repository's own folders.
//! - **`parent-of-call`**: a call of `Root::parent_of` is `Policy::walk` (names.rs) and nothing
//!   else since the contained `LocalDrive` walks by `OnDisk` and `Admitted` keys (ARCH29 C6c). Its
//!   argument is a `LocalKey`, so the compiler already refuses a `&str`; this rule keeps the list of
//!   callers short, so a new entry point is a new line here and a decision about its `Purpose`.
//!
//! - **`walk-local-call`** (ARCH29 C6c): `names::walk_local(root, key, create)` takes any `&str` and
//!   a flag, and walked a name from a DRIVE with only the shape rows (X-DELTA-AZDRIVE-FILES-1: a
//!   folder the drive renamed to `.git` was made on the victim's disk). Its callers move, one file
//!   at a time, to `OnDisk` (a name that is on the disk or in a record this device wrote:
//!   `Root::parent_of_existing`, never makes a folder) and `Admitted` (a name to be made:
//!   `Root::parent_of_new`). The count of this rule per file only goes down; the deleting step is
//!   after the recorded baseline.
//! - **`walk-existing-call`** and **`make-parent-of-call`**: the sync's own walks (`sync/names.rs`):
//!   `names::walk_entry` and `names::walk_record` walk to a name that is on the disk (an entry of a
//!   scan, a record this device wrote) and cannot make a folder; `names::make_parent_of` is the one
//!   create, after the rows of a name from the drive. The create sites are counted by file: a new
//!   one is a line here and a question about where its name came from.
//! - **`on-disk-key`**: the callers of `OnDisk::of_entry` and `OnDisk::of_record`, the only ways to
//!   an `OnDisk` key - a directory entry or a record this device wrote. A new caller is a new
//!   claim about where a key came from, and a line here.
//! - **`parent-of-existing-call`** and **`parent-of-new-call`**: the callers of the two walks that
//!   take an `OnDisk` and an `Admitted` key; each file lists what it makes.
//!
//! Not read: a `.push(` or `.join(` of a segment taken from `split('/')` (a lexer cannot tell a
//! segment from any other string); the `LocalKey` type and the walk are what carry that.
//! Comments and test items are not read; the counts are exact and only go down.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, method_calls, strings, Allow, Budget, Gate, Orphans, Root, Rule};

const BOOKKEEPING_NAME: &str = "bookkeeping-name";
const VCS_NAME: &str = "vcs-name";
const PARENT_OF: &str = "parent-of-call";
const WALK_LOCAL: &str = "walk-local-call";
const WALK_EXISTING: &str = "walk-existing-call";
const MAKE_PARENT_OF: &str = "make-parent-of-call";
const ON_DISK_KEY: &str = "on-disk-key";
const PARENT_OF_EXISTING: &str = "parent-of-existing-call";
const PARENT_OF_NEW: &str = "parent-of-new-call";

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

/// Whether a string literal is the engine's own folder or a path of the sync inside it.
fn is_bookkeeping_name(s: &str) -> bool {
    s == ".azlin" || s == ".azlin/" || s.starts_with(".azlin/sync") || s.contains("/.azlin")
}

/// Whether a string literal is a version-control folder's name or a path inside it.
fn is_vcs_name(s: &str) -> bool {
    [".git", ".hg", ".svn"]
        .iter()
        .any(|v| s == *v || s.strip_prefix(*v).is_some_and(|rest| rest.starts_with('/')))
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            BOOKKEEPING_NAME,
            |t| strings(t, is_bookkeeping_name),
            &[
                "let hidden = name == \".azlin\";",
                "let folder = \".azlin/\";",
                "let to = format!(\".azlin/sync/trash/{stamp}\");",
                "let root = home.join(\"/.azlin/\");",
            ],
            &[
                "let keys = \".azlin/keys/\";",
                "let url = \"https://token.azlin.io\";",
                "let app = \"io.azlin.bridge\";",
                "let say = \"the .azlin folder, synced on its own\";",
                "let other = \".azlinx\";",
            ],
        ),
        Rule::new(
            VCS_NAME,
            |t| strings(t, is_vcs_name),
            &[
                "let hidden = name == \".git\";",
                "let skip = \".svn/\";",
                "let hooks = \".git/hooks/pre-commit\";",
            ],
            &[
                "let file = \".gitignore\";",
                "let other = \"git\";",
                "let url = \"https://github.com/x/y.git\";",
                "let sub = \"a.git/b\";",
            ],
        ),
        Rule::new(
            PARENT_OF,
            |t| method_calls(t, "parent_of"),
            &["let (dir, name) = root.parent_of(&key, true)?;"],
            &[
                "let parent = fileops::parent_of(&item);",
                "pub fn parent_of(key: &LocalKey) {}",
                "let x = parent_of_leaf(1);",
            ],
        ),
        Rule::new(
            WALK_LOCAL,
            |t| calls(t, "names::walk_local"),
            &[
                "let (dir, name) = names::walk_local(&tree, key, false)?;",
                "let walked = azul_storage::names::walk_local(tree, key, true);",
            ],
            &[
                "let (dir, name) = walk(&tree, key);",
                "let x = walk_local_total(1);",
                "pub fn walk_local(root: &Root, key: &str, create: bool) {}",
            ],
        ),
        Rule::new(
            WALK_EXISTING,
            |t| {
                let mut lines = calls(t, "names::walk_entry");
                lines.extend(calls(t, "names::walk_record"));
                lines.sort_unstable();
                lines
            },
            &[
                "let (dir, name) = names::walk_entry(&tree, key)?;",
                "let (dir, name) = names::walk_record(&tree, key)?;",
            ],
            &[
                "let (dir, name) = walk_entry(&tree, key);",
                "pub fn walk_record(tree: &Root, key: &str) {}",
                "let x = names::walk_entry_total(1);",
            ],
        ),
        Rule::new(
            MAKE_PARENT_OF,
            |t| calls(t, "names::make_parent_of"),
            &["let (dir, name) = names::make_parent_of(&tree, &policy, key)?;"],
            &[
                "let (dir, name) = make_parent_of(&tree, &policy, key);",
                "pub fn make_parent_of(tree: &Root, policy: &Policy, key: &str) {}",
            ],
        ),
        Rule::new(
            ON_DISK_KEY,
            |t| {
                let mut lines = calls(t, "OnDisk::of_entry");
                lines.extend(calls(t, "OnDisk::of_record"));
                lines.sort_unstable();
                lines
            },
            &[
                "let on_disk = OnDisk::of_entry(key)?;",
                "let on_disk = names::OnDisk::of_record(key)?;",
            ],
            &[
                "let on_disk = of_entry(key);",
                "pub fn of_record(key: &str) -> OnDisk {}",
                "let x = OnDisk::as_str(&k);",
            ],
        ),
        Rule::new(
            PARENT_OF_EXISTING,
            |t| method_calls(t, "parent_of_existing"),
            &["let (dir, name) = tree.parent_of_existing(&on_disk)?;"],
            &[
                "let x = parent_of_existing(1);",
                "pub fn parent_of_existing(&self, key: &OnDisk) {}",
            ],
        ),
        Rule::new(
            PARENT_OF_NEW,
            |t| method_calls(t, "parent_of_new"),
            &["let (dir, name) = tree.parent_of_new(&admitted)?;"],
            &[
                "let x = parent_of_new(1);",
                "pub fn parent_of_new(&self, key: &Admitted) {}",
            ],
        ),
    ]
}

/// Where the engine's own folder is spelled out, and who calls `parent_of`, today. TO SHRINK.
const ALLOWED: &[Allow] = &[
    Allow::file(
        "examples/azul-storage/src/names.rs",
        "the one place: BOOKKEEPING, SYNC_DIR, TRASH_DIR, SPOOL_DIR, RECORD_FILE, and the \
         Policy::walk that is `parent_of`'s caller",
    ),
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        ON_DISK_KEY,
        1,
        "existing_key: the contained LocalDrive's key of a walk to what is there (a read, a delete, \
         the source of a rename, a folder removed), made from a key its call checked already \
         (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/data.rs",
        ON_DISK_KEY,
        1,
        "local_path: the path of a key for a message to the user - a file that is on the disk, an \
         entry; nothing is opened or made by it (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        PARENT_OF_EXISTING,
        3,
        "the contained LocalDrive's walks to what is there: contained_parent_existing, \
         contained_remove_folder, parent_dir (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        PARENT_OF_NEW,
        3,
        "the contained LocalDrive's walks that make a folder, each after Policy::admitted for a name \
         a person chose (new_key): contained_parent_new, contained_make_folder, parent_dir (ARCH29 \
         C6c)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/migrate.rs",
        BOOKKEEPING_NAME,
        1,
        "is_plaintext_key: a bucket key under `.azlin/` is no plaintext file of the drive: Z30-R3 \
         (Protected::is_bookkeeping)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/index.rs",
        BOOKKEEPING_NAME,
        1,
        "SYNC_BOOKKEEPING: the sync's `.azlin/sync/` under the drive index's reserved names: \
         Z30-R3 (names::SYNC_DIR)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/rules.rs",
        VCS_NAME,
        3,
        "the scan exclusions `.git/`, `.hg/`, `.svn/`: they keep the files out of uploads and \
         give the report its reason, while Protected (through admit) refuses a name from the \
         drive in any spelling",
    ),
    Allow::exactly(
        "examples/azul-drive/src/git_repo.rs",
        VCS_NAME,
        1,
        "the branch chip looks for a repository's own `.git` folder: it reads a repository on \
         this computer, it does not make a path from a drive's name",
    ),
    Allow::exactly(
        "examples/azul-drive/src/ui_git.rs",
        VCS_NAME,
        2,
        "the git view reads a repository's own `.git/objects` and `.git/logs`",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/rules.rs",
        BOOKKEEPING_NAME,
        1,
        "the scan exclusion `/.azlin/`: it keeps the files out of uploads and gives the report \
         its reason, while Protected (through admit) is what refuses a name from the drive",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/settings.rs",
        BOOKKEEPING_NAME,
        1,
        "the text `~/.azlin/config.json` a settings row shows: the user's config folder, no \
         synced folder's bookkeeping",
    ),
    Allow::exactly(
        "examples/azul-appkit/src/azlin_config.rs",
        BOOKKEEPING_NAME,
        1,
        "CONFIG_DIR: the user's `~/.azlin` config folder, a different thing from a synced \
         folder's bookkeeping",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/local.rs",
        ON_DISK_KEY,
        2,
        "stat_now (the lstat of a key the engine knows: a scan's entry, a base entry) and \
         prune_empty_parents (the folders of a file that was here): keys of the engine's records, \
         looked at, never made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/local.rs",
        PARENT_OF_EXISTING,
        2,
        "the walks of stat_now and prune_empty_parents: to what is there, no folder made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/session.rs",
        ON_DISK_KEY,
        1,
        "keep_in_trash: the conflict's key is the engine's own record of a file here, read for the \
         copy the trash keeps (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/session.rs",
        PARENT_OF_EXISTING,
        1,
        "keep_in_trash's walk to the file: no folder made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        WALK_EXISTING,
        10,
        "the pass's walks to a name that is on the disk: four a scan's entry (open_found, move_here's \
         source, the delete, the conflict copy), six a record of the engine's (read_here, the JSON \
         merge, a folder moved, a folder removed, hash_here, evict_file); no folder is made (ARCH29 \
         C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        MAKE_PARENT_OF,
        5,
        "the pass's creates, each after the rows of a name from the drive: move_here's target, \
         write_download, write_link, fetch_one and move_folders_here's target (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/names.rs",
        ON_DISK_KEY,
        2,
        "walk_entry and walk_record: the one place of the sync that says where an OnDisk key came \
         from (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/names.rs",
        PARENT_OF_EXISTING,
        2,
        "walk_entry and walk_record walk to what is there (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/names.rs",
        PARENT_OF_NEW,
        1,
        "make_parent_of: the sync's one create, after Policy::admitted (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/journal.rs",
        WALK_EXISTING,
        1,
        "inside: the journal's names (records of this device) walked to for a recovery, never made \
         (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/verify.rs",
        WALK_EXISTING,
        1,
        "restore: the base's key of a damaged copy (a record of this device's) walked to for the replacement, never made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/trash.rs",
        WALK_EXISTING,
        2,
        "already_back (the oplog's key of a trashed file, a record) and untrash's source (an entry of \
         the trash's folders): walked to, never made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/trash.rs",
        MAKE_PARENT_OF,
        1,
        "untrash's target: the folders of the name a file goes back to, made after the rows of a \
         synced name (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/guard.rs",
        WALK_EXISTING,
        1,
        "sample: a key of the scan or of an action of the pass, looked at for its first bytes, never made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/e2e/",
        BOOKKEEPING_NAME,
        9,
        "the E2E commands look at the disk by name on purpose (the bookkeeping is what some of \
         them check); counted, so a new name there is seen",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the engine's own names are spelled in names.rs; parent_of is called from the list",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_bookkeeping_name_is_spelled_in_names_rs_and_parent_of_is_called_from_the_list() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every hit: `cargo test -p azul-srcgate --test names_gate -- --ignored --nocapture
/// print_hits`, to write the list above.
#[test]
#[ignore = "prints the hits; the list above is written from it"]
fn print_hits() {
    let hits = azul_srcgate::hits(
        &azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}")),
        &rules(),
    );
    let mut per_file: std::collections::BTreeMap<(&str, &str), usize> =
        std::collections::BTreeMap::new();
    for h in &hits {
        *per_file
            .entry((h.rel.as_str(), h.rule.as_str()))
            .or_default() += 1;
    }
    for ((file, rule), n) in per_file {
        println!("{n:>3} {rule:<16} {file}");
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
    let base =
        std::env::temp_dir().join(format!("azul-srcgate-names-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azcloud-kit/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::exactly(
    "examples/azcloud-kit/src/trashing.rs",
    "bookkeeping-name",
    1,
    "the planted literal",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

#[test]
fn a_planted_bookkeeping_literal_and_a_parent_of_call_in_an_unlisted_file_are_caught() {
    let trashing = [
        "pub fn is_hidden(name: &str) -> bool {",
        "    name == \".azlin\"",
        "}",
        "",
    ]
    .join("\n");
    let walking = [
        "pub fn walk(root: &Root, key: &LocalKey) {",
        "    let _ = root.parent_of(key, false);",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "planted",
        &[
            (
                "examples/azcloud-kit/src/lib.rs",
                "mod trashing;\nmod walking;\n",
            ),
            ("examples/azcloud-kit/src/trashing.rs", trashing.as_str()),
            ("examples/azcloud-kit/src/walking.rs", walking.as_str()),
        ],
    );
    let wrong = check(&base, &[]).expect_err("both are findings");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azcloud-kit/src/trashing.rs:2")
                && w.contains("bookkeeping-name")),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azcloud-kit/src/walking.rs:2")
                && w.contains("parent-of-call")),
        "{wrong:?}"
    );
    // the list takes the literal by count; the call stays a finding.
    let wrong = check(&base, LISTED).expect_err("the call is not listed");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].contains("walking.rs:2"), "{wrong:?}");
    let _ = std::fs::remove_dir_all(base);
}

const LISTED_WALKS: &[Allow] = &[
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        WALK_LOCAL,
        1,
        "the planted walk",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        ON_DISK_KEY,
        1,
        "the planted key",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        PARENT_OF_EXISTING,
        1,
        "the planted walk to what is there",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        PARENT_OF_NEW,
        1,
        "the planted walk that makes",
    ),
];

/// ARCH29 C6c. The old walk (a `&str` and a flag), the claim that a key is on this disk, and the two
/// walks of the new keys are each a counted site: an unlisted file is a finding with its line, a
/// listed one is counted, a second `walk_local` in it is a new site, and a file converted off the
/// old walk leaves a stale count (the list shrinks with the code).
#[test]
fn a_planted_walk_local_an_on_disk_key_and_the_two_new_walks_are_caught_and_counted() {
    let walking = [
        "pub fn walk(tree: &Root, key: &str, admitted: &Admitted) {",
        "    let _ = names::walk_local(tree, key, true);",
        "    let on_disk = OnDisk::of_record(key);",
        "    let _ = tree.parent_of_existing(&on_disk);",
        "    let _ = tree.parent_of_new(admitted);",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "walks",
        &[
            ("examples/azcloud-kit/src/lib.rs", "mod walking;\n"),
            ("examples/azcloud-kit/src/walking.rs", walking.as_str()),
        ],
    );
    let wrong = check(&base, &[]).expect_err("four sites, none listed");
    assert_eq!(wrong.len(), 4, "{wrong:?}");
    for (line, rule) in [
        (2, "walk-local-call"),
        (3, "on-disk-key"),
        (4, "parent-of-existing-call"),
        (5, "parent-of-new-call"),
    ] {
        assert!(
            wrong.iter().any(|w| w
                .starts_with(&format!("examples/azcloud-kit/src/walking.rs:{line}"))
                && w.contains(rule)),
            "{rule} at line {line} in {wrong:?}"
        );
    }
    assert_eq!(check(&base, LISTED_WALKS), Ok(4));
    // a second walk_local in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azcloud-kit/src/walking.rs"),
        format!("{walking}pub fn more(tree: &Root) {{\n    let _ = names::walk_local(tree, \"k\", false);\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED_WALKS).expect_err("one more walk than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x walk-local-call where its allowance says 1")),
        "{wrong:?}"
    );
    // converted off the old walk: the list shrinks with the code.
    let converted = [
        "pub fn walk(tree: &Root, key: &str, admitted: &Admitted) {",
        "    let on_disk = OnDisk::of_record(key);",
        "    let _ = tree.parent_of_existing(&on_disk);",
        "    let _ = tree.parent_of_new(admitted);",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azcloud-kit/src/walking.rs"), converted).unwrap();
    let wrong = check(&base, LISTED_WALKS).expect_err("fewer walks than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x walk-local-call where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_drives_protocol_keys_urls_and_messages_are_no_bookkeeping_name() {
    let quiet = [
        "// the .azlin folder: \".azlin\"",
        "pub const KEY: &str = \".azlin/keys/recovery.key\";",
        "pub const URL: &str = \"https://token.azlin.io\";",
        "pub fn say() -> String {",
        "    String::from(\"the .azlin folder, synced on its own\")",
        "}",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t() {",
        "        let _ = \".azlin/sync/trash\";",
        "        let _ = root.parent_of(&key, false);",
        "    }",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "quiet",
        &[("examples/azcloud-kit/src/lib.rs", quiet.as_str())],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
