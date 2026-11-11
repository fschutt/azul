//! The name policy ([`crate::names`], ARCH28 RF42): the protected names in every spelling a disk
//! reads as theirs, the key that passed the policy, and the file name a download gets.

use crate::{
    key::System,
    manifest::is_reserved_key,
    names::{
        folder_problem, here_problem, key_problem, leaf_for_text, local_key, Policy, Protected,
        ProtectedKind, Purpose, Volume,
    },
    Drive, DriveError, LocalDrive,
};

use super::TempDir;

/// Spellings of `.git` a disk reads as `.git`: other case, a code point the disk ignores, dots and
/// spaces at the end (NTFS drops them), a stream suffix, the NTFS short name.
const GIT: &[&str] = &[
    ".git",
    ".GIT",
    ".Git",
    ".gIt",
    ".g\u{200c}it",
    ".gi\u{200d}t",
    "\u{feff}.git",
    ".git.",
    ".git ",
    ".git. .",
    ".GIT.",
    ".git::$INDEX_ALLOCATION",
    ".git:stream",
    "GIT~1",
    "git~1",
    "Git~2",
];

const HG: &[&str] = &[".hg", ".HG", ".Hg", ".hg.", "HG~1", ".h\u{200c}g"];
const SVN: &[&str] = &[".svn", ".SVN", ".sVn", ".\u{17f}vn", "SVN~1", ".svn "];
const AZLIN: &[&str] = &[
    ".azlin",
    ".AZLIN",
    ".Azlin",
    ".aZlIn",
    ".azlin.",
    ".azlin ",
    ".a\u{200d}zlin",
    "AZLIN~1",
    "azlin~1",
    ".azlin::$INDEX_ALLOCATION",
];

/// Names that are not protected, though they look like one.
const ORDINARY: &[&str] = &[
    ".gitignore",
    ".github",
    "my.git",
    "gits",
    ".hgignore",
    ".svnignore",
    "azlin",
    "x.azlin",
    ".azlinx",
    "git~",
    "git~a",
    "git~1x",
    ".gitx~1",
    "~1",
    "",
    ".",
    "readme.md",
];

#[test]
fn a_protected_name_is_one_in_every_spelling_a_disk_reads_as_its_own() {
    for name in GIT {
        assert_eq!(Protected::kind(name), Some(ProtectedKind::Vcs), "{name:?}");
    }
    for name in HG.iter().chain(SVN) {
        assert_eq!(Protected::kind(name), Some(ProtectedKind::Vcs), "{name:?}");
    }
    for name in AZLIN {
        assert_eq!(
            Protected::kind(name),
            Some(ProtectedKind::Bookkeeping),
            "{name:?}"
        );
        assert!(Protected::is_bookkeeping(name) && !Protected::is_vcs(name));
    }
    for name in ORDINARY {
        assert_eq!(Protected::kind(name), None, "{name:?}");
    }
    // What a repository reads for its configuration is warned of, a system's own folders are the
    // rules' business: neither is refused.
    for name in [".gitmodules", ".GITATTRIBUTES", "GITMOD~1", "gitatt~1"] {
        assert_eq!(
            Protected::kind(name),
            Some(ProtectedKind::VcsMetadata),
            "{name:?}"
        );
    }
    for name in ["$RECYCLE.BIN", "System Volume Information"] {
        assert_eq!(
            Protected::kind(name),
            Some(ProtectedKind::OsPrivate),
            "{name:?}"
        );
    }
    // Only a spelling that is not the canonical one is a disguise: a copy of a repository keeps
    // its own `.git`.
    assert!(!Protected::is_disguised(".git") && !Protected::is_disguised(".azlin"));
    for name in [".GIT", "GIT~1", ".git.", ".AZLIN", ".g\u{200c}it"] {
        assert!(Protected::is_disguised(name), "{name:?}");
    }
    assert!(!Protected::is_disguised("readme.md"));
}

#[test]
fn the_first_protected_folder_of_a_key_is_found_where_a_sync_refuses_it() {
    assert_eq!(
        Protected::in_key("proj/.GIT/hooks/pre-commit"),
        Some((1, ProtectedKind::Vcs, ".GIT"))
    );
    assert_eq!(
        Protected::in_key(".azlin/sync/x"),
        Some((0, ProtectedKind::Bookkeeping, ".azlin"))
    );
    assert_eq!(
        Protected::in_key("GIT~1/hooks/x"),
        Some((0, ProtectedKind::Vcs, "GIT~1"))
    );
    // The bookkeeping name is the root's alone; a user's `x/.azlin` is a folder like any other.
    assert_eq!(Protected::in_key("x/.azlin/y"), None);
    assert_eq!(Protected::in_key("a/.gitmodules/b"), None);
}

/// X-DELTA-AZDRIVE-FILES-9 (and -1). A version-control name is protected whatever the entry: the
/// LAST name of a key counts too, so a file or a link named `.git` (git follows its `gitdir:`
/// line to a repository another participant syncs) and a folder the drive renames to `.GIT` are
/// refused in every spelling. The old rule let the last name of a file key through as "an
/// ordinary file" (a submodule's pointer); a pointer into the unsynced `.git/modules` is useless
/// on another device anyway.
#[test]
fn x_delta_azdrive_files_9_a_version_control_name_is_protected_as_the_last_name_too() {
    for name in GIT.iter().chain(HG).chain(SVN) {
        let key = format!("proj/{name}");
        assert_eq!(
            Protected::in_key(&key),
            Some((1, ProtectedKind::Vcs, *name)),
            "{key:?}"
        );
        assert!(key_problem(&key).is_some(), "{key:?} as a file or link");
        assert!(folder_problem(&key).is_some(), "{key:?} as a folder entry");
        for policy in policies() {
            assert!(
                policy.admit(&key, Purpose::Sync).is_err(),
                "{key:?} is admitted on {:?}",
                policy.system
            );
            // The walk that makes a folder or a file refuses it before anything is made.
            let tmp = TempDir::new("x-delta-files-9");
            let tree = crate::beneath::Root::open(tmp.path()).unwrap();
            assert!(
                policy.walk(&tree, &key, Purpose::Sync, true).is_err(),
                "{key:?}"
            );
            assert!(
                !tmp.path().join("proj").exists(),
                "a folder was made for {key:?}"
            );
        }
    }
    // The root's `.azlin` is refused as a last name as before, a user's own `x/.azlin` is not.
    assert!(key_problem(".azlin").is_some());
    assert_eq!(key_problem("x/.azlin"), None);
    // What looks like a protected name and is not stays admitted, as a file and as a folder.
    for key in [
        "proj/.gitignore",
        "proj/.gitmodules",
        "proj/my.git",
        "gits",
        "proj/git~",
    ] {
        assert_eq!(key_problem(key), None, "{key:?}");
        assert_eq!(folder_problem(key), None, "{key:?}");
    }
}

/// Every system and every disk: the answer of a sync cannot wait for a probe of this disk.
fn policies() -> Vec<Policy> {
    let volumes = [
        Volume::default(),
        Volume {
            case_insensitive: true,
            norm_insensitive: true,
            ..Volume::default()
        },
        Volume {
            windows_names: true,
            ..Volume::default()
        },
    ];
    System::ALL
        .into_iter()
        .flat_map(|system| {
            volumes
                .into_iter()
                .map(move |volume| Policy { system, volume })
        })
        .collect()
}

#[test]
fn az_w7_a_synced_key_under_a_protected_name_is_refused_in_every_spelling_on_every_system() {
    let mut keys: Vec<String> = Vec::new();
    for name in GIT.iter().chain(HG).chain(SVN) {
        keys.push(format!("proj/{name}/hooks/pre-commit"));
    }
    for name in AZLIN {
        keys.push(format!("{name}/sync/trash/20261104/x"));
    }
    for key in &keys {
        // `.git:stream` and the like carry a colon: refused anyway on Windows, and as a name
        // under a protected folder on the others.
        assert!(key_problem(key).is_some(), "{key:?}");
        for policy in policies() {
            assert!(
                here_problem(key, policy.system, &policy.volume).is_some(),
                "{key:?} on {:?}",
                policy.system
            );
            assert!(
                policy.admit(key, Purpose::Sync).is_err(),
                "{key:?} is admitted on {:?}",
                policy.system
            );
        }
    }
    // The ordinary names still go in, with the protected look-alikes among them.
    for key in [
        "a.txt",
        "proj/.gitignore",
        "proj/.github/workflows/ci.yml",
        "my.git/x",
        "x/.azlin/y",
        "proj/.gitmodules",
        "gits/readme.md",
        ".hgignore",
    ] {
        for policy in policies() {
            assert!(
                policy.admit(key, Purpose::Sync).is_ok(),
                "{key:?} on {:?}",
                policy.system
            );
        }
    }
}

#[test]
fn a_key_is_admitted_for_what_it_is_for_and_only_then_is_a_local_key() {
    let here = Policy::here(Volume::default());
    // A copy of a repository keeps its `.git`; a spelling only a disk would merge into one is
    // not admitted for a download, an opened file, a rescue copy or a copy.
    for purpose in [
        Purpose::Download,
        Purpose::Open,
        Purpose::Rescue,
        Purpose::Copy,
    ] {
        assert!(
            here.admit("proj/.git/config", purpose).is_ok(),
            "{purpose:?}"
        );
        assert!(
            here.admit("proj/.GIT/hooks/x", purpose).is_err(),
            "{purpose:?}"
        );
        assert!(here.admit("GIT~1/hooks/x", purpose).is_err(), "{purpose:?}");
        assert!(here.admit(".AZLIN/x", purpose).is_err(), "{purpose:?}");
        assert!(here.admit("a/../b", purpose).is_err(), "{purpose:?}");
    }
    // A name the user typed, and a key already on this disk, are held to the shape only.
    for purpose in [Purpose::UserTyped, Purpose::Local] {
        assert!(here.admit("proj/.GIT/x", purpose).is_ok(), "{purpose:?}");
        for bad in [
            "", "/etc/x", "a//b", "a/./b", "a/../b", "a\\b", "x\0y", "a/",
        ] {
            assert!(here.admit(bad, purpose).is_err(), "{bad:?} {purpose:?}");
        }
    }
    let key = local_key("notes/2026/plan.md").unwrap();
    assert_eq!(key.as_str(), "notes/2026/plan.md");
    assert_eq!(&*key, "notes/2026/plan.md");
    assert_eq!(
        key.under(std::path::Path::new("/data")),
        std::path::Path::new("/data/notes/2026/plan.md")
    );
    // Windows reads `C:x` and `nul.txt` as something else, whatever computer asks.
    let windows = Policy {
        system: System::Windows,
        volume: Volume::default(),
    };
    for bad in ["C:x", "a/nul.txt", "a.txt:s", "dir/aux", "end."] {
        assert!(windows.admit(bad, Purpose::Local).is_err(), "{bad:?}");
    }
    // A refusal says the key and why, and is the drive's or the walk's error where one is asked.
    let refused = here.admit("a/../b", Purpose::Local).unwrap_err();
    assert_eq!(refused.key, "a/../b");
    assert!(matches!(
        DriveError::from(refused.clone()),
        DriveError::InvalidKey { .. }
    ));
    assert!(matches!(
        crate::beneath::BeneathError::from(refused),
        crate::beneath::BeneathError::BadName(_)
    ));
}

#[test]
fn the_drives_own_bookkeeping_is_reserved_in_every_spelling() {
    for key in [
        ".azlin",
        ".azlin/sync",
        ".AZLIN/x",
        "AZLIN~1/cache",
        ".azlin./x",
        ".a\u{200c}zlin/meta",
    ] {
        assert!(is_reserved_key(key), "{key:?}");
    }
    for key in ["x/.azlin", ".azlinx", "azlin", "docs/.AZLIN/y", ""] {
        assert!(!is_reserved_key(key), "{key:?}");
    }
    // A drive over a folder refuses to write there, whatever the case it was spelt in.
    let tmp = TempDir::new("names-reserved");
    let drive = LocalDrive::without_manifest(tmp.path());
    for key in [".AZLIN/x", "AZLIN~1/x", ".azlin/x"] {
        assert!(
            matches!(drive.put(key, b"x"), Err(DriveError::InvalidKey { .. })),
            "{key:?}"
        );
    }
    assert!(!tmp.path().join(".AZLIN").exists());
    drive.put("x/.azlin/y", b"a user's own folder").unwrap();
}

#[test]
fn leaf_for_text_makes_a_name_of_what_a_person_wrote() {
    assert_eq!(leaf_for_text("Photos 2026"), "Photos 2026");
    assert_eq!(
        leaf_for_text("  Holiday/2026: the *best*  "),
        "Holiday-2026- the -best-"
    );
    assert_eq!(leaf_for_text("CON"), "_CON");
    assert_eq!(leaf_for_text("nul.txt"), "_nul.txt");
    assert_eq!(
        leaf_for_text(".. . "),
        "",
        "nothing is left of dots and spaces"
    );
    assert_eq!(leaf_for_text(""), "");
    assert_eq!(leaf_for_text("AZLIN~1"), "_AZLIN~1");
    assert_eq!(leaf_for_text(".git"), "git", "the dots at its ends go");
    assert_eq!(leaf_for_text("bell\u{7}"), "bell-");
    let long = leaf_for_text(&"é".repeat(200));
    assert!(
        long.len() <= 120 && long.chars().all(|c| c == 'é'),
        "{}",
        long.len()
    );
    assert_eq!(Policy::here(Volume::default()).leaf_for_text("CON"), "_CON");
}

/// RF42 step 3: the path argument of a walk is a `LocalKey`, so there is no way to the disk that
/// skips the policy. A key walked for the sync is admitted first (a protected name in any
/// spelling is refused before a folder of it is made), a key of the engine's own records keeps
/// the shape rows, and a key that climbs out of its folder is a bad name on every path.
#[test]
fn rf42_a_walk_takes_only_a_key_the_policy_let_through_and_makes_nothing_for_one_it_refused() {
    use crate::{
        beneath::{BeneathError, Root},
        names::walk_local,
    };
    let tmp = TempDir::new("rf42-walk");
    let tree = Root::open(tmp.path()).unwrap();
    // A plain key: the folders are made and the name is the last one.
    let (dir, name) = walk_local(&tree, "docs/2026/a.txt", true).unwrap();
    assert_eq!(name, "a.txt");
    assert!(dir.path().ends_with("docs/2026"), "{:?}", dir.path());
    // Shapes that leave the folder are bad names, however the key is spelled.
    for key in ["../x", "a/../../x", "/abs", "a//b", "a\\b", ""] {
        assert!(
            matches!(walk_local(&tree, key, true), Err(BeneathError::BadName(_))),
            "{key:?}"
        );
    }
    // For the sync a protected name in any spelling is refused BEFORE a folder of it is made.
    let policy = Policy::here(Volume::default());
    for key in [
        ".GIT/hooks/pre-commit",
        ".azlin/sync/x",
        "a/.g\u{200c}it/config",
        "GIT~1/config",
    ] {
        let refused = policy.walk(&tree, key, Purpose::Sync, true);
        assert!(matches!(refused, Err(BeneathError::BadName(_))), "{key:?}");
    }
    for made in [".GIT", ".azlin", "a", "GIT~1"] {
        assert!(!tmp.path().join(made).exists(), "{made} was made");
    }
    // The same policy lets a plain synced key through.
    let (_, name) = policy
        .walk(&tree, "docs/b.txt", Purpose::Sync, false)
        .unwrap();
    assert_eq!(name, "b.txt");
    // The engine's own records are walked as they are on the disk (the trash is under .azlin).
    let (_, kept) = walk_local(&tree, ".azlin/sync/trash/2026/c.txt", true).unwrap();
    assert_eq!(kept, "c.txt");
    assert!(tmp.path().join(".azlin/sync/trash/2026").is_dir());
}

/// X-DELTA-AZDRIVE-FILES-32 (h): `.gitmodules` and `.gitattributes` were "warned of" in the doc and
/// nothing warned. A note says it, in any spelling and at any depth; other names have none.
#[test]
fn x_delta_azdrive_files_32_a_repositorys_own_configuration_is_warned_of() {
    use crate::names::vcs_metadata_note;
    for key in [
        ".gitmodules",
        "proj/.GITATTRIBUTES",
        "a/b/GITMOD~1",
        "proj/.gitmodules/x",
    ] {
        let note = vcs_metadata_note(key).unwrap_or_default();
        assert!(
            note.contains("repository's own configuration"),
            "{key:?}: {note:?}"
        );
    }
    for key in ["proj/.gitignore", "readme.md", "proj/.git/config", "gits/x"] {
        assert_eq!(vcs_metadata_note(key), None, "{key:?}");
    }
}

/// ARCH29 C6c. An `OnDisk` key is a name that is on this disk already or in a record this device
/// wrote: it has the shape rows (what `Purpose::Local` checked) and nothing is made from it. The
/// walk finds what is there; for a folder that is not, it fails and leaves the disk as it was - the
/// spellings of a protected name included (the disk has them already; a key that only reads makes no
/// folder of one).
#[test]
fn c6c_an_on_disk_key_walks_to_what_is_there_and_never_makes_a_folder() {
    use crate::{beneath::Root, names::OnDisk};
    let tmp = TempDir::new("c6c-on-disk");
    let tree = Root::open(tmp.path()).unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/2026")).unwrap();
    std::fs::write(tmp.path().join("docs/2026/a.txt"), b"a").unwrap();
    // A file that is there: its folder and its name, from an entry and from a record alike.
    for on_disk in [
        OnDisk::of_entry("docs/2026/a.txt").unwrap(),
        OnDisk::of_record("docs/2026/a.txt").unwrap(),
    ] {
        let (dir, name) = tree.parent_of_existing(&on_disk).unwrap();
        assert_eq!(name, "a.txt");
        assert!(dir.lstat(name).is_ok());
    }
    // A folder that is not there stays not there.
    let missing = [
        "missing/b/c.txt",
        "docs/2027/x.txt",
        "GIT~1/config",
        ".hg/hooks/x",
        ".azlin/sync/x",
    ];
    for key in missing {
        let on_disk = OnDisk::of_record(key).unwrap();
        let failed = tree.parent_of_existing(&on_disk).err();
        assert!(
            matches!(&failed, Some(e) if e.is_not_found()),
            "{key:?}: {failed:?}"
        );
    }
    for made in ["missing", "docs/2027", "GIT~1", ".hg", ".azlin"] {
        assert!(!tmp.path().join(made).exists(), "{made} was made");
    }
    // Shapes that leave the folder are no key of what is on a disk, as they were none for Local.
    for key in [
        "", "/etc/x", "a//b", "a/./b", "a/../b", "a\\b", "x\0y", "a/",
    ] {
        assert!(OnDisk::of_entry(key).is_err(), "{key:?}");
        assert!(OnDisk::of_record(key).is_err(), "{key:?}");
    }
    let refused = OnDisk::of_entry("a/../b").unwrap_err();
    assert_eq!(refused.key, "a/../b");
}

/// ARCH29 C6c. A name is made here only from an `Admitted` key, and every name the full rows refuse
/// - a protected folder in any spelling (`.git`, `.hg`, `.svn`, `.azlin`, `GIT~1`, `.GIT.`), a
/// reserved Windows name - is never one, whoever renamed the folder: `Policy::admitted` refuses it
/// and `Root::parent_of_new` takes nothing else, so the walk that made a drive-renamed `.git`
/// folder on the victim's disk (X-DELTA-AZDRIVE-FILES-1) has no argument to be given. What is on
/// the disk already (`Purpose::Local`) never becomes one, and a plain key is made with its folders.
#[test]
fn c6c_a_name_is_made_only_from_an_admitted_key_and_the_full_rows_refuse_what_a_drive_renamed() {
    use crate::beneath::Root;
    let tmp = TempDir::new("c6c-new");
    let tree = Root::open(tmp.path()).unwrap();
    // The sync: a version-control name in any position and spelling, the bookkeeping first.
    for name in GIT.iter().chain(HG).chain(SVN) {
        for key in [
            format!("proj/{name}/hooks/post-commit"),
            format!("{name}/x"),
            format!("proj/{name}"),
        ] {
            for policy in policies() {
                assert!(
                    policy.admitted(&key, Purpose::Sync).is_err(),
                    "{key:?} on {:?}",
                    policy.system
                );
            }
        }
    }
    for name in AZLIN {
        for policy in policies() {
            let key = format!("{name}/sync/x");
            assert!(policy.admitted(&key, Purpose::Sync).is_err(), "{key:?}");
        }
    }
    // The other purposes about a drive's names refuse a spelling that only a disk reads as a
    // protected name (a copy of a repository keeps its own `.git`).
    for purpose in [
        Purpose::Download,
        Purpose::Open,
        Purpose::Rescue,
        Purpose::Copy,
    ] {
        for name in [".GIT", "GIT~1", ".git.", ".HG", ".AZLIN"] {
            let key = format!("proj/{name}/x");
            let here = Policy::here(Volume::default());
            assert!(here.admitted(&key, purpose).is_err(), "{key:?} {purpose:?}");
        }
    }
    // A reserved Windows name, whichever computer asks.
    let windows = Policy {
        system: System::Windows,
        volume: Volume::default(),
    };
    for bad in ["CON", "a/NUL.txt", "dir/aux", "COM1", "a.txt:s", "end."] {
        assert!(
            windows.admitted(bad, Purpose::Sync).is_err(),
            "{bad:?} is admitted for the sync on Windows"
        );
    }
    // What is on the disk already makes no new name, whatever its spelling.
    let here = Policy::here(Volume::default());
    for key in ["docs/a.txt", "proj/.git/config", ".azlin/sync/x"] {
        assert!(here.admitted(key, Purpose::Local).is_err(), "{key:?}");
    }
    // Nothing was made for any of it.
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    // A plain key is admitted, and made with its folders; a repository's own `.git` is a copy's.
    let admitted = here.admitted("docs/2026/a.txt", Purpose::Sync).unwrap();
    assert_eq!(admitted.as_str(), "docs/2026/a.txt");
    let (dir, name) = tree.parent_of_new(&admitted).unwrap();
    assert_eq!(name, "a.txt");
    assert!(dir.path().ends_with("docs/2026"), "{:?}", dir.path());
    assert!(tmp.path().join("docs/2026").is_dir());
    let copy = here.admitted("proj/.git/config", Purpose::Copy).unwrap();
    assert!(tree.parent_of_new(&copy).is_ok());
    assert!(tmp.path().join("proj/.git").is_dir());
    // A name the user typed is held to the shape, as it was.
    assert!(here.admitted("proj/.GIT/x", Purpose::UserTyped).is_ok());
    assert!(here.admitted("a/../b", Purpose::UserTyped).is_err());
}
