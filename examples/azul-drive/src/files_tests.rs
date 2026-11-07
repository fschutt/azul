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

use crate::fileops::{plan_transfer, run_transfer, ConflictChoice, SourceItem, TransferKind};

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
