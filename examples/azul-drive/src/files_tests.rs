//! The file operations as the app builds them (X-DELTA-AZDRIVE-FILES-2 and the findings that need
//! the chain a real drive has): the drives here come out of `Slot::open`, the function production
//! calls, not out of a `LocalDrive::new` of the test. The lane's earlier tests used bare
//! `LocalDrive`s and small fakes, so nothing built the `RetryingDrive` over a `LocalDrive` that
//! Home and every added folder are - and the guards that rest on `Drive::site()` and
//! `Drive::remove_empty_folder()` were never reached.

use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::{
    config::{DriveEntry, DriveLocation},
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
    xfer::Site,
    Drive, DriveError,
};

use crate::fileops::{
    delete_items, delete_way, plan_transfer, run_transfer, trash_stamp, ConflictChoice, DeleteWay,
    SourceItem, TransferKind,
};

/// The drive of a slot over the folder `root`, opened as the app opens it ([`crate::Slot::open`]).
fn opened(id: &str, root: &Path, keys: &TempDir) -> Arc<dyn Drive> {
    let keyring = azcloud_kit::SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        keys.path(),
    );
    let mut slot = crate::Slot::new(DriveEntry {
        id: id.to_string(),
        name: id.to_string(),
        location: DriveLocation::Local {
            root: root.to_string_lossy().into_owned(),
        },
    });
    slot.open(
        &crate::RotatedSessions::default(),
        &crate::KeyRecords::default(),
        None,
        None,
        &keyring,
    )
    .expect("a folder of this computer opens")
}

fn item(key: &str) -> SourceItem {
    SourceItem {
        key: key.to_string(),
        is_folder: key.ends_with('/'),
        size: None,
    }
}

/// A Home with a folder of files, a subfolder, an empty folder and a file.
fn seeded(tmp: &TempDir) -> (Arc<dyn Drive>, std::path::PathBuf) {
    let root = tmp.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    let keys = TempDir::new("files-keys-seed");
    let home = opened("home", &root, &keys);
    home.put("docs/a.txt", b"alpha").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    home.put("readme.txt", b"hello").unwrap();
    home.create_folder("empty/").unwrap();
    (home, root)
}

/// X-DELTA-AZDRIVE-FILES-2. The drive of a slot over a folder says where it is: the folder.
#[test]
fn x_delta_azdrive_files_2_the_drive_a_slot_opens_over_a_folder_is_that_folder() {
    let tmp = TempDir::new("dx2-site");
    let keys = TempDir::new("dx2-site-keys");
    let root = tmp.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    let home = opened("home", &root, &keys);
    assert!(
        matches!(home.site(), Site::Disk { .. }),
        "the slot's drive is a place of its own (the wrapper's address): {:?}",
        home.site()
    );
    // A second slot over the same tree, and one over a folder inside it, are the same place and
    // a place inside it - not two unknown ones.
    let again = opened("again", &root, &keys);
    assert!(home.site().same_place(&again.site()));
}

/// X-DELTA-AZDRIVE-FILES-2, the Move that ends the way a user sees it: a folder moved out of a
/// slot the app opened. The files land and are retired, and the emptied folder is removed - not
/// "failed" with the folder left behind (the trait's default tried to delete the folder as a file).
#[test]
fn x_delta_azdrive_files_2_a_folder_moved_out_of_a_slot_the_app_opened_leaves_nothing_behind() {
    let tmp = TempDir::new("dx2-move");
    let (home, root) = seeded(&tmp);
    let keys = TempDir::new("dx2-move-keys");
    let other_root = tmp.path().join("other");
    std::fs::create_dir_all(&other_root).unwrap();
    let other = opened("other", &other_root, &keys);
    let plan = plan_transfer(
        &*home,
        &[item("docs/")],
        &*other,
        "",
        TransferKind::Move,
        "Copy",
    )
    .unwrap();
    let report = run_transfer(
        &plan,
        &*home,
        &*other,
        TransferKind::Move,
        &AtomicBool::new(false),
        &mut |_| {},
    );
    assert!(
        report.failed.is_empty(),
        "the move ends 'failed': {:?}",
        report.failed
    );
    assert!(report.left.is_empty(), "{:?}", report.left);
    assert_eq!(other.get("docs/a.txt").unwrap(), b"alpha");
    assert_eq!(other.get("docs/sub/b.txt").unwrap(), b"beta");
    assert!(
        !root.join("docs").exists(),
        "the emptied folder stays behind in Home"
    );
    assert_eq!(
        home.get("readme.txt").unwrap(),
        b"hello",
        "the rest is untouched"
    );
}

/// X-DELTA-AZDRIVE-FILES-2, AZ-U1's refusal in the product: a folder is never moved into itself
/// through another slot - a drive added on a folder inside Home, a second slot over Home.
#[test]
fn x_delta_azdrive_files_2_a_folder_never_goes_into_itself_through_slots_the_app_opened() {
    let tmp = TempDir::new("dx2-itself");
    let (home, root) = seeded(&tmp);
    let keys = TempDir::new("dx2-itself-keys");
    // A drive added on `docs` (Home's own subfolder) and a second slot over Home.
    let inside = opened("inside", &root.join("docs"), &keys);
    let again = opened("again", &root, &keys);
    let refused = |target: &dyn Drive, prefix: &str| {
        plan_transfer(
            &*home,
            &[item("docs/")],
            target,
            prefix,
            TransferKind::Move,
            "Copy",
        )
    };
    // Into the inner drive's root: it would land as docs/docs/.
    assert!(
        matches!(refused(&*inside, ""), Err(DriveError::InvalidKey { .. })),
        "a folder into a drive on a folder inside it"
    );
    // Below itself, through the second slot.
    assert!(matches!(
        refused(&*again, "docs/sub/"),
        Err(DriveError::InvalidKey { .. })
    ));
    // Onto its own place: nothing to do.
    assert!(refused(&*again, "").unwrap().is_empty());
    assert_eq!(home.get("docs/sub/b.txt").unwrap(), b"beta");
}

/// X-DELTA-AZDRIVE-FILES-2: a file is its own destination through a second slot over its folder:
/// the run refuses it, and nothing is replaced by itself (the engine's own-destination check rests
/// on `site()`).
#[test]
fn x_delta_azdrive_files_2_a_file_is_not_its_own_destination_through_a_second_slot() {
    for kind in [TransferKind::Move, TransferKind::Copy] {
        let tmp = TempDir::new("dx2-self");
        let (home, root) = seeded(&tmp);
        let keys = TempDir::new("dx2-self-keys");
        let again = opened("again", &root, &keys);
        let mut plan =
            plan_transfer(&*again, &[item("readme.txt")], &*home, "", kind, "Copy").unwrap();
        for index in plan.conflicts() {
            plan.choose(index, ConflictChoice::Replace);
        }
        let report = run_transfer(
            &plan,
            &*again,
            &*home,
            kind,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{kind:?}: {report:?}");
        assert_eq!(
            home.get("readme.txt").unwrap(),
            b"hello",
            "{kind:?}: the only copy is still there"
        );
    }
}

/// X-DELTA-AZDRIVE-FILES-6. Ctrl+A in a local drive's root with "Hidden items" on selects
/// `.azdrive-trash/` with the other rows. The selection was sorted by "ALL of it is in the trash"
/// (it was not), so the delete was a trashing one whose `_` arm called `delete_folder` on the
/// trash itself: every earlier delete (kept 30 days, the user's safety net) was removed, and, when
/// the row ran after the folders the batch had renamed into its own `.azdrive-trash/<stamp>/`, the
/// batch's folders went too - under the toast "Moved N items to the trash, Ctrl+Z brings them
/// back".
#[test]
fn x_delta_azdrive_files_6_a_trashing_delete_of_a_selection_that_holds_the_trash_keeps_the_old_deletes(
) {
    let tmp = TempDir::new("dx6-trash");
    let (home, root) = seeded(&tmp);
    let first = trash_stamp(1_759_300_000, 1);
    let earlier = delete_items(&*home, &[item("readme.txt")], Some(&first));
    assert!(earlier.failed.is_empty(), "{earlier:?}");
    home.put("c.txt", b"c").unwrap();
    // The trash row comes AFTER the folder (a sort that puts the dot-folder last).
    let second = trash_stamp(1_759_300_100, 2);
    let mixed = delete_items(
        &*home,
        &[item("docs/"), item("c.txt"), item(".azdrive-trash/")],
        Some(&second),
    );
    let trash = root.join(".azdrive-trash");
    assert!(
        trash.join(&first).join("readme.txt").is_file(),
        "an earlier delete went with the trash folder"
    );
    assert!(
        trash.join(&second).join("docs").join("a.txt").is_file(),
        "the batch's own folder went with the trash folder"
    );
    assert!(trash.join(&second).join("c.txt").is_file());
    assert_eq!(mixed.failed.len(), 1, "{mixed:?}");
    assert_eq!(mixed.failed[0].0, ".azdrive-trash/");
    assert!(
        matches!(mixed.failed[0].1, DriveError::InvalidKey { .. }),
        "{mixed:?}"
    );
    assert_eq!(mixed.gone.len(), 2, "docs/ and c.txt went: {mixed:?}");
}

/// X-DELTA-AZDRIVE-FILES-6, the decision the Delete key takes (`delete_selected` asks it): a
/// selection that mixes the trash with other rows is always asked.
#[test]
fn x_delta_azdrive_files_6_a_selection_that_mixes_the_trash_with_other_rows_is_always_asked() {
    let a = item("a.txt");
    let trash = item(".azdrive-trash/");
    let old = item(".azdrive-trash/20261001-031500-1/x.txt");
    // Nothing of it in the trash: a local drive trashes; otherwise it is asked, or for good when
    // the question is off.
    assert_eq!(
        delete_way(&[a.clone()], true, false, true),
        DeleteWay::ToTrash
    );
    assert_eq!(delete_way(&[a.clone()], true, true, true), DeleteWay::Ask);
    assert_eq!(
        delete_way(&[a.clone()], true, true, false),
        DeleteWay::ForGood
    );
    assert_eq!(
        delete_way(&[a.clone()], false, false, false),
        DeleteWay::Ask
    );
    // All of it in the trash: for good, the setting decides.
    assert_eq!(
        delete_way(&[trash.clone()], true, false, false),
        DeleteWay::ForGood
    );
    assert_eq!(delete_way(&[old], true, false, true), DeleteWay::Ask);
    // A mix: asked whatever the setting says (it was a trashing delete).
    for confirm in [false, true] {
        for permanent in [false, true] {
            assert_eq!(
                delete_way(&[a.clone(), trash.clone()], true, permanent, confirm),
                DeleteWay::Ask,
                "permanent {permanent}, confirm {confirm}"
            );
        }
    }
}

/// X-DELTA-AZDRIVE-FILES-11. Share > Print on Windows ran `powershell -Command Start-Process -Verb
/// Print -FilePath <path>`: `-Command` joins what follows into one script text, so a file named
/// `x;calc.exe` or `$(calc)x.txt` (legal on NTFS, synced from a shared drive) was code. The script
/// is fixed now and the path is in the environment; no argument holds any part of a name.
#[test]
fn x_delta_azdrive_files_11_a_file_name_is_never_part_of_the_print_command() {
    use crate::actions::{windows_print_command, PRINT_FILE_VAR};
    for name in [
        "x;calc.exe",
        "$(calc)x.txt",
        "a b.txt",
        "it's.txt",
        "`calc`.txt",
        "a&b|c.txt",
        "-Command evil.txt",
        "x\"; calc; \".txt",
    ] {
        let path = std::path::Path::new("C:\\Users\\me\\Documents").join(name);
        let command = windows_print_command(&path);
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter().all(|arg| !arg.contains(name)),
            "{name}: the name is in the command line: {args:?}"
        );
        // The same fixed script for every name.
        assert_eq!(
            args.last().map(String::as_str),
            Some("Start-Process -Verb Print -FilePath $env:AZDRIVE_PRINT_FILE")
        );
        let handed: Vec<_> = command
            .get_envs()
            .filter(|(key, _)| *key == std::ffi::OsStr::new(PRINT_FILE_VAR))
            .collect();
        assert_eq!(handed.len(), 1, "{name}");
        assert_eq!(handed[0].1, Some(path.as_os_str()), "{name}");
    }
}
