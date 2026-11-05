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
//! - **`parent-of-call`**: a call of `Root::parent_of` is one of two kinds - `Policy::walk`
//!   (names.rs), and the contained `LocalDrive`'s walks, which hold a `LocalKey` of their own
//!   (local.rs). Its argument is a `LocalKey`, so the compiler already refuses a `&str`; this rule
//!   keeps the list of callers short, so a new entry point is a new line here and a decision about
//!   its `Purpose`.
//!
//! Not read: a `.push(` or `.join(` of a segment taken from `split('/')` (a lexer cannot tell a
//! segment from any other string); the `LocalKey` type and the walk are what carry that.
//! Comments and test items are not read; the counts are exact and only go down.

use std::path::{Path, PathBuf};

use azul_srcgate::{method_calls, strings, Allow, Budget, Gate, Orphans, Root, Rule};

const BOOKKEEPING_NAME: &str = "bookkeeping-name";
const VCS_NAME: &str = "vcs-name";
const PARENT_OF: &str = "parent-of-call";

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
        PARENT_OF,
        5,
        "the contained LocalDrive's walks (read, rename, make and remove a folder): each holds a \
         LocalKey of its own, made from a key its call checked already",
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
    Allow::file(
        "examples/azul-drive/src/e2e/",
        "the E2E commands look at the disk by name on purpose (the bookkeeping is what some of \
         them check)",
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
