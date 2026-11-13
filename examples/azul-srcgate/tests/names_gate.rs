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
//! - **`parent-of-call`**, **`walk-local-call`**, **`local-key-call`**, **`purpose-local`**,
//!   **`policy-walk-call`**, **`root-dir-with-flag`**, **`dir-child-with-flag`** and
//!   **`admitted-call`** hold at ZERO, everywhere - `names.rs` and `beneath.rs` included - since the
//!   deleting commits of ARCH29 C6c: `Root::parent_of(&LocalKey, create)`, `names::walk_local(root,
//!   key, create)`, `names::local_key`, `Purpose::Local`, `Policy::walk(root, key, purpose, create)`,
//!   `Root::dir(rel, create)`, `Dir::child(name, create)` and the interim `Policy::admitted` took a
//!   `&str` (or a key) and a FLAG, and walked a name from a DRIVE with only the shape rows
//!   (X-DELTA-AZDRIVE-FILES-1: a folder the drive renamed to `.git` was made on the victim's disk).
//!   A function that makes a folder takes a typed key now: `Root::parent_of_new` and `Root::dir_new`
//!   an `Admitted` one (the rows of its purpose, the protected names in every spelling among
//!   them), `Root::own_dir` and `Root::trash_dir` the engine's own folders; what is on the disk is
//!   walked to by `Root::parent_of_existing` (an `OnDisk` key) and `Root::dir_existing`, which have
//!   no create. A new use of an old name is a finding (X-DELTA-STRUCT-DRIVE-1).
//! - **`admit-call`**: the callers of `Policy::admit` (`.admit(key, Purpose::X)`), the one maker of
//!   an `Admitted` key; each file lists them, so which PURPOSE a site chooses is a counted line and a
//!   decision, not a surprise (the review of C6c found `Purpose::UserTyped` chosen where the weakest
//!   rows made folders).
//! - **`walk-existing-call`** and **`make-parent-of-call`**: the sync's own walks (`sync/names.rs`):
//!   `names::walk_entry` and `names::walk_record` walk to a name that is on the disk (an entry of a
//!   scan, a record this device wrote) and cannot make a folder; `names::make_parent_of` is the one
//!   create, after the rows of a name from the drive. The create sites are counted by file: a new
//!   one is a line here and a question about where its name came from.
//! - **`on-disk-key`**: the callers of `OnDisk::of_entry` and `OnDisk::of_record`, the only ways to
//!   an `OnDisk` key - a key to LOOK AT, never to make. A new caller is a new claim about where a
//!   key came from, and a line here.
//! - **`parent-of-existing-call`**, **`parent-of-new-call`**, **`dir-new-call`**, **`own-dir-call`**,
//!   **`trash-dir-call`** and **`make-child-call`**: the callers of the walks, by file; each file
//!   lists what it makes.
//!
//! Not read: a `.push(` or `.join(` of a segment taken from `split('/')` (a lexer cannot tell a
//! segment from any other string); the `LocalKey` type and the walk are what carry that.
//! Comments and test items are not read; the counts are exact and only go down.

use std::path::{Path, PathBuf};

use azul_srcgate::{
    calls, method_calls, strings, Allow, Budget, Gate, Located, Orphans, Root, Rule, Tok,
};

const BOOKKEEPING_NAME: &str = "bookkeeping-name";
const VCS_NAME: &str = "vcs-name";
const PARENT_OF: &str = "parent-of-call";
const WALK_LOCAL: &str = "walk-local-call";
const LOCAL_KEY: &str = "local-key-call";
const PURPOSE_LOCAL: &str = "purpose-local";
const WALK_EXISTING: &str = "walk-existing-call";
const MAKE_PARENT_OF: &str = "make-parent-of-call";
const ON_DISK_KEY: &str = "on-disk-key";
const PARENT_OF_EXISTING: &str = "parent-of-existing-call";
const PARENT_OF_NEW: &str = "parent-of-new-call";
const POLICY_WALK: &str = "policy-walk-call";
const ROOT_DIR_FLAG: &str = "root-dir-with-flag";
const CHILD_FLAG: &str = "dir-child-with-flag";
const ADMITTED_CALL: &str = "admitted-call";
const ADMIT_CALL: &str = "admit-call";
const DIR_NEW: &str = "dir-new-call";
const OWN_DIR: &str = "own-dir-call";
const TRASH_DIR_CALL: &str = "trash-dir-call";
const MAKE_CHILD: &str = "make-child-call";

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

/// The index of the closer that matches the opener at `open`, or the end of `t`.
fn close_of(t: &[Located], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, x) in t.iter().enumerate().skip(open) {
        if let Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
    }
    t.len()
}

/// The lines of the method calls `.name(args)` whose argument tokens `keep` accepts.
fn method_calls_where(t: &[Located], name: &str, keep: impl Fn(&[Located]) -> bool) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident(name) || i == 0 || !t[i - 1].tok.is_punct('.') {
            continue;
        }
        if !t.get(i + 1).is_some_and(|n| n.tok.is_punct('(')) {
            continue;
        }
        let close = close_of(t, i + 1).min(t.len());
        if keep(&t[i + 2..close]) {
            found.push(x.line);
        }
    }
    found
}

/// The lines of the method calls `.name(` and of the same function written as a path call
/// (`Type::name(&receiver, ..)`): what `method_calls` alone missed (X-DELTA-STRUCT3-18).
fn method_or_path_calls(t: &[Located], name: &str, path: &str) -> Vec<usize> {
    let mut lines = method_calls(t, name);
    lines.extend(calls(t, path));
    lines.sort_unstable();
    lines
}

/// Whether the argument tokens hold a comma outside every bracket: two arguments or more.
fn has_two_arguments(args: &[Located]) -> bool {
    let mut depth = 0usize;
    for x in args {
        if let Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => return true,
                _ => {}
            }
        }
    }
    false
}

/// Whether the argument tokens name the type `Purpose`: the call chooses what a name is admitted for.
fn names_a_purpose(args: &[Located]) -> bool {
    args.iter().any(|x| x.tok.is_ident("Purpose"))
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
            LOCAL_KEY,
            |t| calls(t, "names::local_key"),
            &["let key = names::local_key(key)?;"],
            &[
                "let key = local_key(key);",
                "pub fn local_key(key: &str) {}",
                "let x = names::local_key_total(1);",
            ],
        ),
        Rule::new(
            PURPOSE_LOCAL,
            |t| {
                t.windows(4)
                    .filter(|w| {
                        w[0].tok.is_ident("Purpose")
                            && w[1].tok.is_punct(':')
                            && w[2].tok.is_punct(':')
                            && w[3].tok.is_ident("Local")
                    })
                    .map(|w| w[0].line)
                    .collect()
            },
            &["let rows = policy.admit(key, Purpose::Local);"],
            &[
                "let rows = policy.admit(key, Purpose::Sync);",
                "let x = Source::Local;",
                "let y = Purpose::LocalCopy;",
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
            |t| method_or_path_calls(t, "parent_of_existing", "Root::parent_of_existing"),
            &[
                "let (dir, name) = tree.parent_of_existing(&on_disk)?;",
                "let (dir, name) = Root::parent_of_existing(&tree, &on_disk)?;",
            ],
            &[
                "let x = parent_of_existing(1);",
                "pub fn parent_of_existing(&self, key: &OnDisk) {}",
            ],
        ),
        Rule::new(
            PARENT_OF_NEW,
            |t| method_or_path_calls(t, "parent_of_new", "Root::parent_of_new"),
            &[
                "let (dir, name) = tree.parent_of_new(&admitted)?;",
                "let (dir, name) = Root::parent_of_new(&tree, &admitted)?;",
            ],
            &[
                "let x = parent_of_new(1);",
                "pub fn parent_of_new(&self, key: &Admitted) {}",
            ],
        ),
        Rule::new(
            POLICY_WALK,
            |t| method_calls_where(t, "walk", names_a_purpose),
            &[
                "let (dir, name) = policy.walk(&tree, key, Purpose::Sync, true)?;",
                "let walked = Policy::here(Volume::default()).walk(root, key, Purpose::UserTyped, false);",
            ],
            &[
                "let more = self.walk(&format!(\"{key}/\"), out)?;",
                "workspace.walk(\"\", 0, &mut out);",
                "pub fn walk(&self, root: &Root, key: &str, purpose: Purpose) {}",
            ],
        ),
        Rule::new(
            ROOT_DIR_FLAG,
            |t| method_calls_where(t, "dir", has_two_arguments),
            &[
                "let folder = tree.dir(folder, true)?;",
                "let trash = root.dir(TRASH_DIR, false).ok()?;",
            ],
            &[
                "let path = scratch.dir().to_path_buf();",
                "let folder = tree.dir_existing(folder)?;",
                "let here = other.dir(a);",
            ],
        ),
        Rule::new(
            CHILD_FLAG,
            |t| method_calls_where(t, "child", has_two_arguments),
            &[
                "let inner = dir.child(name, false)?;",
                "parent.child(name, true).map(|_| ())",
            ],
            &[
                "let inner = dir.child(name)?;",
                "account_dir(root, id).child(OUTBOX_DIR)",
                "let id = pty.child().id();",
            ],
        ),
        Rule::new(
            ADMITTED_CALL,
            |t| method_calls(t, "admitted"),
            &["let admitted = policy.admitted(key, Purpose::Sync)?;"],
            &[
                "let admitted = policy.admit(key, Purpose::Sync)?;",
                "pub fn admitted(&self, key: &str) {}",
                "let x = admitted(1);",
            ],
        ),
        Rule::new(
            ADMIT_CALL,
            |t| {
                // A call that names a purpose, or any call of two arguments (a purpose in a variable is
                // one: `.admit(key, purpose)`), and the same function as a path call
                // (`Policy::admit(&policy, key, purpose)`, X-DELTA-STRUCT3-18).
                let mut lines = method_calls_where(t, "admit", |args| {
                    names_a_purpose(args) || has_two_arguments(args)
                });
                lines.extend(calls(t, "Policy::admit"));
                lines.sort_unstable();
                lines
            },
            &[
                "let admitted = policy.admit(key, Purpose::Sync)?;",
                "let rows = Policy::here(Volume::default()).admit(&key, Purpose::Copy);",
                "let admitted = policy.admit(&key, purpose)?;",
                "let rows = Policy::admit(&policy, &key, Purpose::UserTyped);",
                "let rows = azul_storage::names::Policy::admit(&policy, key, Purpose::Copy)?;",
            ],
            &[
                "if self.told.admit(code) {}",
                "let kept: Vec<Problem> = rows.into_iter().filter(|p| tally.admit(p.code)).collect();",
                "chat.admit(device);",
                "pub fn admit(&self, key: &str, purpose: Purpose) {}",
                "let x = Other::admit(1);",
            ],
        ),
        Rule::new(
            DIR_NEW,
            |t| method_or_path_calls(t, "dir_new", "Root::dir_new"),
            &[
                "let dir = tree.dir_new(&admitted)?;",
                "let dir = Root::dir_new(&tree, &admitted)?;",
            ],
            &[
                "let x = dir_new(1);",
                "pub fn dir_new(&self, key: &Admitted) {}",
            ],
        ),
        Rule::new(
            OWN_DIR,
            |t| method_or_path_calls(t, "own_dir", "Root::own_dir"),
            &[
                "let dir = root.own_dir(SYNC_DIR)?;",
                "let dir = Root::own_dir(&root, SYNC_DIR)?;",
            ],
            &[
                "let x = own_dir(1);",
                "pub fn own_dir(&self, rel: &'static str) {}",
            ],
        ),
        Rule::new(
            TRASH_DIR_CALL,
            |t| method_or_path_calls(t, "trash_dir", "Root::trash_dir"),
            &[
                "let trash = root.trash_dir(stamp, &on_disk)?;",
                "let trash = Root::trash_dir(&root, stamp, &on_disk)?;",
            ],
            &[
                "let x = trash_dir(1);",
                "pub fn trash_dir(&self, stamp: &str, key: &OnDisk) {}",
            ],
        ),
        Rule::new(
            MAKE_CHILD,
            |t| method_or_path_calls(t, "make_child", "Dir::make_child"),
            &[
                "let made = parent.make_child(name)?;",
                "let made = Dir::make_child(&parent, name)?;",
            ],
            &[
                "let x = make_child(1);",
                "pub(crate) fn make_child(&self, name: &str) {}",
            ],
        ),
    ]
}

/// Where the engine's own folder is spelled out, and who calls `parent_of`, today. TO SHRINK.
const ALLOWED: &[Allow] = &[
    // ---- the one place the engine's own names are spelled; the deleted walks are held at zero here too ----
    Allow::exactly(
        "examples/azul-storage/src/names.rs",
        BOOKKEEPING_NAME,
        5,
        "the one place: BOOKKEEPING, SYNC_DIR, TRASH_DIR, SPOOL_DIR, RECORD_FILE",
    ),
    Allow::exactly(
        "examples/azul-storage/src/names.rs",
        VCS_NAME,
        3,
        "VCS_FOLDERS and the metadata names of Protected: the one table of the protected names",
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
        "examples/azcloud-kit/src/sync/movein.rs",
        ON_DISK_KEY,
        1,
        "move_in's look at the destination the user typed: is it taken? (a look, no folder made) \
         (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/movein.rs",
        PARENT_OF_EXISTING,
        1,
        "move_in's look at the destination: walked to, never made (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/movein.rs",
        PARENT_OF_NEW,
        1,
        "copy_one: the folders of each copy, made after Policy::admit for a name of the user's own folder \
         (Purpose::Copy: the folder moves in with its names, a repository with its .git) (ARCH29 C6c; \
         X-DELTA-STRUCT-DRIVE-2: a key that was only checked makes no folder)",
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
        "the contained LocalDrive's walks that make a folder, each after Policy::admit for a copy's rows \
         (new_key: Purpose::Copy): contained_parent_new, contained_make_folder, parent_dir (ARCH29 \
         C6c; X-DELTA-STRUCT-DRIVE-2)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        MAKE_CHILD,
        1,
        "contained_make_folder: the last folder of a name a person chose, made after `new_key` (an \
         `Admitted` key) made its parents (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/beneath.rs",
        MAKE_CHILD,
        4,
        "the four walks that make - `Root::dir_new` and `parent_of_new` (an `Admitted` key), `own_dir` \
         (a literal) and `trash_dir` (the engine's own trash folder): each is the step of `walk_with` \
         (ARCH29 C6c; X-DELTA-STRUCT-DRIVE-1)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/testing.rs",
        MAKE_CHILD,
        1,
        "make_folder_chain: a test's tree of folders too deep for a path (the support of the apps' tests \
         behind the `testing` feature)",
    ),
    // ---- who chooses which purpose a name is admitted for (`Policy::admit`): one counted line each ----
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        ADMIT_CALL,
        1,
        "new_key: the contained LocalDrive's key of a name that is made (a write, a folder, the target of \
         a rename): Purpose::Copy (a key that was only checked makes no folder)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/xfer/plan.rs",
        ADMIT_CALL,
        1,
        "NameCheck::refuses: the names a transfer writes into a folder of this computer (Purpose::Copy; a \
         download and a paste are copies)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/fileops.rs",
        ADMIT_CALL,
        1,
        "check_name_on: a name a person typed for a new folder or a rename (Purpose::UserTyped); the name \
         is only checked here, no folder is made from the answer",
    ),
    Allow::exactly(
        "examples/azul-drive/src/archive.rs",
        ADMIT_CALL,
        1,
        "target_of_on: a name of the rescue copy of a locked drive (Purpose::Rescue: a path under the \
         rescue folder, which the copy makes itself)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/names.rs",
        ADMIT_CALL,
        1,
        "make_parent_of: the sync's one create (Purpose::Sync, the rows of a synced name)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        ADMIT_CALL,
        1,
        "apply_folders: a folder the plan makes here (Purpose::Sync)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        DIR_NEW,
        1,
        "apply_folders: the folder of an `Admitted` key (Purpose::Sync), made without a link and on the \
         folder's own disk (ARCH29 C6c)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/session.rs",
        ADMIT_CALL,
        2,
        "the folders of cloud-only files (Purpose::Sync: no folder of a protected name is made for a name \
         of the drive) and decrypt's name of an opened copy (Purpose::Open)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/session.rs",
        DIR_NEW,
        1,
        "the folders of cloud-only files, made for a key the rows of a synced name admitted \
         (X-DELTA-STRUCT-DRIVE-1: it was `tree.dir(folder, true)` for the keys of the drive's index)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/movein.rs",
        ADMIT_CALL,
        2,
        "copy_one and copy_tree: the names of the user's own folder that moves in (Purpose::Copy: a \
         repository keeps its `.git`, a spelling a disk would merge into one is refused)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/movein.rs",
        DIR_NEW,
        1,
        "copy_tree: the folder of a folder that moves in, made after the rows of a name a person chose \
         (X-DELTA-STRUCT-DRIVE-1: it was `tree.dir(key, true)`)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/root.rs",
        ON_DISK_KEY,
        1,
        "trash_target: the key of a file that goes into the trash is a record of this device's, whose own \
         folders are copied under the pass's trash folder",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/root.rs",
        OWN_DIR,
        3,
        "the engine's own folders, from literals: the marker's (write_marker), the spool (spool) and the \
         probe's (probe_volume)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/root.rs",
        TRASH_DIR_CALL,
        1,
        "trash_target: the trash folder of a pass and the folders of the file that goes into it, under \
         `.azlin/sync/trash/<stamp>/` (X-DELTA-STRUCT-DRIVE-1)",
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
#[ignore = "X-EDGE-C-1: prints the hits; the list above is written from it"]
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

const LISTED_MAKERS: &[Allow] = &[
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        ADMIT_CALL,
        1,
        "the planted admission",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        DIR_NEW,
        1,
        "the planted folder of an admitted key",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        OWN_DIR,
        1,
        "the planted own folder",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        TRASH_DIR_CALL,
        1,
        "the planted trash folder",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/walking.rs",
        MAKE_CHILD,
        1,
        "the planted child",
    ),
];

/// ARCH29 C6c, X-DELTA-STRUCT-DRIVE-1. What the deleting commits removed stays removed - `Policy::walk`
/// (a purpose and a flag), `Root::dir(rel, create)`, `Dir::child(name, create)` and the interim
/// `Policy::admitted` are findings anywhere, with their lines, even in a file whose makers are listed -,
/// and every function that makes a folder is a counted line: a new maker, or a purpose chosen at a new
/// `admit`, is a new site, and a maker that went away leaves a stale count.
#[test]
fn a_planted_use_of_the_deleted_walks_is_caught_and_the_makers_are_counted() {
    let makers = [
        "pub fn make(tree: &Root, policy: &Policy, key: &str, on_disk: &OnDisk, dir: &Dir) {",
        "    let admitted = policy.admit(key, Purpose::Sync).unwrap();",
        "    let _ = tree.dir_new(&admitted);",
        "    let _ = tree.own_dir(SYNC_DIR);",
        "    let _ = tree.trash_dir(stamp, on_disk);",
        "    let _ = dir.make_child(\"x\");",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "makers",
        &[
            ("examples/azcloud-kit/src/lib.rs", "mod walking;\n"),
            ("examples/azcloud-kit/src/walking.rs", makers.as_str()),
        ],
    );
    let wrong = check(&base, &[]).expect_err("five makers, none listed");
    assert_eq!(wrong.len(), 5, "{wrong:?}");
    for (line, rule) in [
        (2, "admit-call"),
        (3, "dir-new-call"),
        (4, "own-dir-call"),
        (5, "trash-dir-call"),
        (6, "make-child-call"),
    ] {
        assert!(
            wrong.iter().any(|w| w
                .starts_with(&format!("examples/azcloud-kit/src/walking.rs:{line}"))
                && w.contains(rule)),
            "{rule} at line {line} in {wrong:?}"
        );
    }
    assert_eq!(check(&base, LISTED_MAKERS), Ok(5));
    // A second admission is a new site (a purpose chosen somewhere new): the count says so.
    std::fs::write(
        base.join("examples/azcloud-kit/src/walking.rs"),
        format!("{makers}pub fn more(policy: &Policy) {{\n    let _ = policy.admit(\"k\", Purpose::UserTyped);\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED_MAKERS).expect_err("one more admission than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x admit-call where its allowance says 1")),
        "{wrong:?}"
    );
    // The deleted forms are findings even in a file whose makers are listed: no allowance takes them.
    let deleted = [
        "pub fn old(tree: &Root, policy: &Policy, dir: &Dir, key: &str) {",
        "    let _ = policy.walk(tree, key, Purpose::Sync, true);",
        "    let _ = tree.dir(key, true);",
        "    let _ = dir.child(key, false);",
        "    let _ = policy.admitted(key, Purpose::Sync);",
        "    let _ = tree.parent_of(key, true);",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azcloud-kit/src/walking.rs"), deleted).unwrap();
    let wrong = check(&base, LISTED_MAKERS).expect_err("the deleted forms are no site anyone owns");
    for (line, rule) in [
        (2, "policy-walk-call"),
        (3, "root-dir-with-flag"),
        (4, "dir-child-with-flag"),
        (5, "admitted-call"),
        (6, "parent-of-call"),
    ] {
        assert!(
            wrong.iter().any(|w| w
                .starts_with(&format!("examples/azcloud-kit/src/walking.rs:{line}"))
                && w.contains(rule)),
            "{rule} at line {line} in {wrong:?}"
        );
    }
    // What looks like them and is none: a method of another type with the same name, a walk with no
    // purpose, a `child()` that takes one argument, a `dir()` that takes none.
    let clean = [
        "pub fn fine(ws: &Workspace, scratch: &Scratch, pty: &Pty, tally: &Tally) {",
        "    ws.walk(\"\", 0, &mut out);",
        "    let _ = scratch.dir();",
        "    let _ = pty.child().id();",
        "    let _ = tally.admit(code);",
        "    let _ = tree.dir_existing(key);",
        "    let _ = dir.child(key);",
        "}",
        "",
    ]
    .join("\n");
    std::fs::write(base.join("examples/azcloud-kit/src/walking.rs"), clean).unwrap();
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}

// ==== Every crate that depends on azul-storage, at count zero (X-DELTA-STRUCT3-18) ====
//
// `names` and `beneath` are `pub mod`s of azul-storage, and the gate read azul-storage and three of the 30
// crates that depend on it: a new call in the bridge, mail or contacts crates such as
// `let k = policy.admit(&drive_key, Purpose::UserTyped)?; Root::dir_new(&tree, &k)?` moved no gate line and made
// an exact `.git` or `.azlin` folder from a drive's name. The list of crates is read from the manifests when the
// gate runs - a crate added tomorrow is read tomorrow - and every one of them is held at ZERO for every rule that
// admits or makes a name (the text rules about the engine's own spelling stay with the four crates that own the
// bookkeeping). A crate that needs one of these is a new line in the list above, with its reason.

/// The four crates the allowances above are written for.
const OWNERS: &[&str] = &["azul-storage", "azcloud-kit", "azul-drive", "azul-appkit"];

/// Whether the manifest `text` names azul-storage as a dependency of any table (`azul-storage = ..`,
/// `azul-storage.workspace = true`, `[dependencies.azul-storage]`).
fn depends_on_storage(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        line.strip_prefix("azul-storage")
            .is_some_and(|rest| rest.trim_start().starts_with(['=', '.']))
            || (line.starts_with('[') && line.ends_with("dependencies.azul-storage]"))
    })
}

/// Every package of `examples/` that depends on azul-storage and is none of [`OWNERS`], as the roots a gate
/// reads: from the manifests, at run time.
fn dependents() -> &'static [Root] {
    static FOUND: std::sync::OnceLock<Vec<Root>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        let base = azul_srcgate::workspace().join("examples");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&base)
            .unwrap_or_else(|e| panic!("{}: {e}", base.display()))
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(manifest) = std::fs::read_to_string(entry.path().join("Cargo.toml")) else {
                continue;
            };
            if OWNERS.contains(&name.as_str()) || !depends_on_storage(&manifest) {
                continue;
            }
            found.push(Root {
                dir: Box::leak(format!("examples/{name}").into_boxed_str()),
                min_files: 1,
            });
        }
        found.sort_by_key(|root| root.dir);
        assert!(
            found.len() >= 20,
            "the manifests of the crates that depend on azul-storage were read in the wrong place: {found:?}"
        );
        found
    })
}

/// The rules that admit or make a name: every rule of the gate but the two that read string literals (the engine's
/// own spelling, which a crate that merely mentions `.git` or `.azlin` in a message may hold).
fn making_rules() -> Vec<Rule> {
    rules()
        .into_iter()
        .filter(|rule| rule.name != BOOKKEEPING_NAME && rule.name != VCS_NAME)
        .collect()
}

#[test]
fn x_delta_struct3_18_no_other_crate_that_depends_on_azul_storage_admits_or_makes_a_name() {
    Gate {
        name: "no crate but the four admits or makes a name from a drive, a device or a person",
        roots: dependents(),
        rules: making_rules(),
        budget: Budget(&[]),
        orphans: Orphans::Ignore,
    }
    .run();
}

/// The manifests are read as the gate reads them: the three spellings of a dependency, nothing for a crate that
/// does not name azul-storage (and one that only has a longer name).
#[test]
fn x_delta_struct3_18_the_dependency_is_found_in_every_spelling_of_a_manifest() {
    assert!(depends_on_storage(
        "[dependencies]\nazul-storage = { path = \"../azul-storage\" }\n"
    ));
    assert!(depends_on_storage("[dev-dependencies]\nazul-storage = { path = \"../azul-storage\", features = [\"testing\"] }\n"));
    assert!(depends_on_storage(
        "[dependencies]\nazul-storage.workspace = true\n"
    ));
    assert!(depends_on_storage(
        "[dependencies.azul-storage]\npath = \"../azul-storage\"\n"
    ));
    assert!(!depends_on_storage(
        "[dependencies]\nazul-storage-extras = \"1\"\n# azul-storage = 1\n"
    ));
    assert!(!depends_on_storage(
        "[package]\nname = \"azul-storage-tool\"\n"
    ));
}

/// The path call and the two-argument call in a crate nobody listed are findings with their lines, at zero: the
/// planted tree is one crate of the dependents, and the old gate - `.method(` spellings only, four crates - read
/// none of it.
#[test]
fn x_delta_struct3_18_a_path_call_and_a_purpose_in_a_variable_are_caught_in_a_crate_nobody_listed()
{
    let planted = [
        "pub fn open(policy: &Policy, tree: &Root, key: &str, purpose: Purpose) {",
        "    let admitted = Policy::admit(policy, key, purpose).unwrap();",
        "    let dir = Root::dir_new(tree, &admitted).unwrap();",
        "    let again = policy.admit(key, purpose).unwrap();",
        "    let (parent, name) = Root::parent_of_new(tree, &again).unwrap();",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "other-crate",
        &[
            ("examples/azul-bridge/src/lib.rs", "mod names;\n"),
            ("examples/azul-bridge/src/names.rs", planted.as_str()),
        ],
    );
    const OTHER: &[Root] = &[Root {
        dir: "examples/azul-bridge/src",
        min_files: 1,
    }];
    let wrong = Gate {
        name: "the planted dependents",
        roots: OTHER,
        rules: making_rules(),
        budget: Budget(&[]),
        orphans: Orphans::Ignore,
    }
    .check(&base)
    .expect_err("a crate nobody listed admits and makes names");
    let at = |line: usize, rule: &str| {
        wrong.iter().any(|w| {
            w.starts_with(&format!("examples/azul-bridge/src/names.rs:{line}:")) && w.contains(rule)
        })
    };
    assert!(at(2, ADMIT_CALL), "the path call: {wrong:?}");
    assert!(at(3, DIR_NEW), "the path call of the walk: {wrong:?}");
    assert!(at(4, ADMIT_CALL), "a purpose in a variable: {wrong:?}");
    assert!(
        at(5, PARENT_OF_NEW),
        "the path call of the other walk: {wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}
