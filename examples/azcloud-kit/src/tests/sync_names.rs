//! The name policy of a synced folder: what can never be a file, what one system cannot hold,
//! what misleads a reader, and how a disk folds names.

use crate::sync::names::{
    display_risks, escaped, fold, folder_problem, here_problem, key_problem, nfc, portability,
    segment_problem, windows_problem, System, Volume,
};

#[test]
fn a_name_that_leaves_the_folder_or_names_its_bookkeeping_is_no_file_anywhere() {
    for bad in [
        "",
        "/etc/passwd",
        "//server/share/x",
        "\\\\server\\share",
        "C:x",
        "c:/Windows/x",
        "a/../b",
        "./a",
        "a//b",
        "a/",
        "a\\b",
        "nul\0byte",
        "bell\u{7}",
        "del\u{7f}",
        "c1\u{85}",
        "non\u{fdd0}char",
        "end\u{fffe}",
        ".azlin/sync/root-id",
        ".azlin",
    ] {
        assert!(key_problem(bad).is_some(), "{bad:?}");
    }
    assert!(
        key_problem(&"n".repeat(256)).is_some(),
        "a name over 255 bytes"
    );
    for fine in [
        "a.txt",
        "dir/sub/a b.txt",
        "résumé.pdf",
        "x/.azlin/y",
        "日本語.txt",
    ] {
        assert_eq!(key_problem(fine), None, "{fine:?}");
    }
    assert_eq!(
        segment_problem(".."),
        Some(String::from(
            "a \"..\" in its path climbs out of its folder"
        ))
    );
}

#[test]
fn windows_refuses_its_characters_device_names_trailing_dots_and_long_paths() {
    for bad in [
        "a:b.txt",
        "dir/C:x",
        "q?.txt",
        "star*.txt",
        "pipe|.txt",
        "quote\".txt",
        "lt<.txt",
        "CON",
        "con.txt",
        "nul.tar.gz",
        "Aux",
        "COM1.log",
        "lpt9",
        "COM\u{b9}",
        "dot.",
        "space ",
        "dir./x",
    ] {
        assert!(windows_problem(bad).is_some(), "{bad:?}");
    }
    assert!(windows_problem(&format!("{}/x.txt", "d".repeat(230))).is_some());
    for fine in ["console.txt", "COM10", "conx", "a.b.c", "LPT"] {
        assert_eq!(windows_problem(fine), None, "{fine:?}");
    }
}

/// The sync and every other place a key becomes a path on Windows read one table: a name the
/// sync lists as one Windows cannot hold is a name the storage's path check for Windows refuses,
/// and the other way round.
#[test]
fn the_sync_and_a_path_on_windows_refuse_the_same_names() {
    for name in [
        "a:b.txt",
        "dir/C:x",
        "q?.txt",
        "pipe|.txt",
        "lt<.txt",
        "CON",
        "con.txt",
        "nul.tar.gz",
        "COM\u{b9}",
        "dot.",
        "space ",
        "dir./x",
        "fine.txt",
        "console.txt",
        "COM10",
        "a.b.c",
    ] {
        let sync = here_problem(name, System::Windows, &Volume::default()).is_some();
        let path = azul_storage::key::check_path_key_on(name, azul_storage::key::System::Windows)
            .is_err();
        assert_eq!(sync, path, "{name:?}");
    }
}

#[test]
fn a_name_valid_here_and_not_elsewhere_is_named_for_the_systems_that_cannot_hold_it() {
    let warned = portability("a:b.txt", System::Linux);
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].0, System::Windows);
    assert!(portability("plain.txt", System::Linux).is_empty());
    // On Windows itself it is invalid here, not a warning.
    assert!(here_problem("a:b.txt", System::Windows, &Volume::default()).is_some());
    assert!(here_problem("a:b.txt", System::Linux, &Volume::default()).is_none());
    // A FAT or exFAT disk refuses the same names on any system.
    let fat = Volume {
        windows_names: true,
        ..Volume::default()
    };
    assert!(here_problem("a:b.txt", System::Macos, &fat).is_some());
}

#[test]
fn names_that_mislead_a_reader_are_flagged_and_always_shown_escaped() {
    let reversed = "invoice\u{202e}fdp.exe";
    assert!(!display_risks(reversed).is_empty());
    assert_eq!(escaped(reversed), "invoice<U+202E>fdp.exe");
    assert!(
        !display_risks("pay\u{200b}pal.txt").is_empty(),
        "an invisible character"
    );
    assert!(
        !display_risks("invoice\u{2024}pdf").is_empty(),
        "a look-alike dot"
    );
    assert!(
        !display_risks("p\u{430}ypal.txt").is_empty(),
        "latin and cyrillic in one name"
    );
    assert!(!display_risks(" lead.txt").is_empty());
    assert!(display_risks("plain.txt").is_empty());
    assert!(display_risks("Москва.txt").is_empty(), "one script is fine");
    assert_eq!(escaped("tab\there"), "tab<U+0009>here");
}

#[test]
fn a_disk_that_ignores_case_or_normalization_folds_names_together() {
    let composed = "caf\u{e9}.txt";
    let decomposed = "cafe\u{301}.txt";
    assert_ne!(composed, decomposed);
    assert_eq!(nfc(decomposed), composed);
    let apfs = Volume {
        case_insensitive: true,
        norm_insensitive: true,
        ..Volume::default()
    };
    assert_eq!(fold(decomposed, &apfs), fold(composed, &apfs));
    assert_eq!(fold("Report.TXT", &apfs), fold("report.txt", &apfs));
    let ext4 = Volume::default();
    assert_ne!(fold(decomposed, &ext4), fold(composed, &ext4));
    assert_ne!(fold("Report.txt", &ext4), fold("report.txt", &ext4));
    let casefold_ext4 = Volume {
        case_insensitive: true,
        ..Volume::default()
    };
    assert_eq!(
        fold("Report.txt", &casefold_ext4),
        fold("report.txt", &casefold_ext4)
    );
}

/// AZ-W1. The version-control folders and the folder's bookkeeping are compared without regard
/// to case on EVERY system: APFS and NTFS read `.GIT` as `.git`, and a drive is shared between
/// systems, so the answer cannot wait for a probe of this disk. The last name of a key counts as
/// much as the folders above it: a file or link `.git` is refused like the folder
/// (X-DELTA-AZDRIVE-FILES-9; a submodule's pointer file is no loss, it points into the unsynced
/// `.git/modules`).
#[test]
fn az_w1_protected_folders_are_refused_in_any_case_on_every_system() {
    for bad in [
        "proj/.GIT/hooks/pre-commit",
        "proj/.Git/config",
        ".git/config",
        "a/b/.HG/hgrc",
        ".hg/hgrc",
        "x/.Svn/entries",
        ".SVN/entries",
        ".AZLIN/sync/trash/20261104/x",
        ".Azlin/sync/tmp/azcloud-sync-0.blob",
        ".azLIN",
        "proj/.git",
        "sub/module/.GIT",
        "a/.Hg",
        "x/.svn",
    ] {
        assert!(key_problem(bad).is_some(), "{bad:?}");
        for system in System::ALL {
            assert!(
                here_problem(bad, system, &Volume::default()).is_some(),
                "{bad:?} on {system:?}, on a disk that tells case apart"
            );
        }
    }
    for fine in [
        "x/.azlin/y",
        "x/.AZLIN/y",
        "proj/.gitignore",
        "proj/.gitattributes",
        ".github/workflows/ci.yml",
        "my.git/x",
        "gits/readme.md",
        ".hgignore",
        ".svnignore/x",
    ] {
        assert_eq!(key_problem(fine), None, "{fine:?}");
        for system in System::ALL {
            assert_eq!(
                here_problem(fine, system, &Volume::default()),
                None,
                "{fine:?} on {system:?}"
            );
        }
    }
    // A folder entry of the drive (no slash at its end) is a folder: its last name counts.
    for folder in ["proj/.GIT", ".Hg", "a/.svn"] {
        assert!(folder_problem(folder).is_some(), "{folder:?}");
    }
    for folder in ["proj/src", "a/.github", "x/.azlin"] {
        assert_eq!(folder_problem(folder), None, "{folder:?}");
    }
    // One table for every kind of entry: what refuses a folder refuses a file of that name.
    for key in ["proj/.GIT", ".Hg", "a/.svn", "GIT~1", "x/.git."] {
        assert_eq!(folder_problem(key), key_problem(key), "{key:?}");
    }
}

/// AZ-W7. NTFS gives `.git` the short name `GIT~1` and `.azlin` `AZLIN~1` and reads `.git.`, a
/// name with a stream suffix or one with a code point it ignores as the folder itself: the
/// bypass of AZ-W1 on a disk that is not told apart by case alone. Such a name is refused like
/// the folder, on every system, since a drive is shared between them.
#[test]
fn az_w7_the_short_names_and_other_spellings_of_a_protected_folder_are_refused_like_it() {
    for bad in [
        "GIT~1/hooks/pre-commit",
        "proj/git~1/config",
        "x/HG~1/hgrc",
        "SVN~2/entries",
        "AZLIN~1/sync/tmp/x",
        "azlin~1/cache",
        "proj/.git./config",
        "proj/.git /config",
        ".azlin./sync/x",
        "proj/.git::$INDEX_ALLOCATION/hooks/x",
        "proj/.g\u{200c}it/hooks/x",
        "proj/\u{feff}.git/config",
        "git~1",
        "proj/git~1",
        "proj/.git.",
    ] {
        assert!(key_problem(bad).is_some(), "{bad:?}");
        for system in System::ALL {
            assert!(
                here_problem(bad, system, &Volume::default()).is_some(),
                "{bad:?} on {system:?}"
            );
        }
    }
    for fine in [
        "proj/gitx~1/a",
        "proj/GIT/readme",
        "proj/git~/a",
        "x/azlin~1/y",
        "proj/.gitx/a",
    ] {
        let problem =
            key_problem(fine).or_else(|| here_problem(fine, System::Linux, &Volume::default()));
        assert_eq!(problem, None, "{fine:?}");
    }
}

/// ARCH29 C6c, sync/mod.rs. The pass has ONE create, `names::make_parent_of`: a file or a link a run
/// writes, a file moved here, a folder renamed to the drive's new name, a file opened from the cloud and
/// a file put back from the trash are made through it, after the rows of a name from the drive. A name
/// the full rows refuse - a repository's or the bookkeeping's folder in any spelling, a reserved Windows
/// name on a disk that reads them - is refused BEFORE a folder of it is made, on every volume (the old
/// walk took any `&str` and a flag, and made a drive-renamed `.git`: FILES-1). A name that is on the
/// disk already is walked to by `walk_entry` and `walk_record`, which have no create: a missing folder
/// stays missing. The call sites are counted by `names_gate` (`make-parent-of-call`).
#[test]
fn c6c_the_syncs_one_create_makes_a_folder_only_for_a_name_the_full_rows_admit() {
    use crate::sync::{
        names::{make_parent_of, walk_entry, walk_record, Policy},
        root::unprobed_volume,
    };
    use azul_storage::{
        beneath::{BeneathError, Root},
        testing::TempDir,
    };
    let tmp = TempDir::new("c6c-make-parent");
    let tree = Root::open(tmp.path()).unwrap();
    let refused = [
        ".git/hooks/post-commit",
        ".GIT/hooks/post-commit",
        "proj/.git./hooks/x",
        "proj/.git /hooks/x",
        "GIT~1/config",
        "a/.g\u{200c}it/x",
        ".hg/x",
        "proj/.SVN/x",
        ".azlin/sync/x",
        ".AZLIN/x",
        "AZLIN~1/sync/x",
        ".git::$INDEX_ALLOCATION/x",
    ];
    for volume in [Volume::default(), unprobed_volume()] {
        let policy = Policy::here(volume);
        for key in refused {
            let walked = make_parent_of(&tree, &policy, key);
            assert!(
                matches!(walked, Err(BeneathError::BadName(_))),
                "{key:?} on {volume:?}"
            );
        }
    }
    // The strict side (a disk that could not be probed) also refuses the names Windows reads.
    let strict = Policy::here(unprobed_volume());
    for key in ["docs/CON", "a/nul.txt", "x/aux"] {
        assert!(
            matches!(
                make_parent_of(&tree, &strict, key),
                Err(BeneathError::BadName(_))
            ),
            "{key:?}"
        );
    }
    assert_eq!(
        std::fs::read_dir(tmp.path()).unwrap().count(),
        0,
        "a folder was made for a name the rows refuse"
    );
    // A name the rows admit is made with its folders, and the name is the last one.
    let policy = Policy::here(Volume::default());
    let (dir, name) = make_parent_of(&tree, &policy, "docs/2026/a.txt").unwrap();
    assert_eq!(name, "a.txt");
    assert!(dir.path().ends_with("docs/2026"), "{:?}", dir.path());
    // What is on the disk is walked to, never made: a missing folder stays missing, whatever the
    // spelling.
    for key in refused {
        assert!(walk_entry(&tree, key).is_err(), "{key:?}");
        assert!(walk_record(&tree, key).is_err(), "{key:?}");
    }
    assert_eq!(
        std::fs::read_dir(tmp.path()).unwrap().count(),
        1,
        "only `docs` is there: a look made a folder"
    );
    let (found, name) = walk_entry(&tree, "docs/2026/a.txt").unwrap();
    assert_eq!(name, "a.txt");
    assert!(found.path().ends_with("docs/2026"));
}
