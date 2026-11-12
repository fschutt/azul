//! The run of a plan (ARCH28 RF43 / AR5a-2 step 4): every file lands before its source is
//! touched, a stop is looked for inside the file, a name made since the plan is not replaced, and
//! a move deletes the versions it copied and removes only the folders it emptied.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{xfer_local::Versioned, TempDir};
use crate::{
    ctl::{Ctl, Quiet},
    xfer::{plan as make_plan, run, Choice, Item, Plan, Report},
    Drive, DriveError, LocalDrive,
};

fn file(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

fn folder(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: true,
        size: None,
    }
}

fn go(plan: &Plan, source: &dyn Drive, target: &dyn Drive, removes_source: bool) -> Report {
    run(plan, source, target, removes_source, &Quiet, &mut |_| {})
}

fn conflict(report: &Report, key: &str) -> bool {
    report
        .failed
        .iter()
        .any(|(what, why)| what == key && matches!(why, DriveError::Conflict { .. }))
}

/// A stop that comes once `threshold` bytes of a file are written.
struct StopAtByte {
    threshold: u64,
    heard: AtomicU64,
}

impl StopAtByte {
    fn new(threshold: u64) -> StopAtByte {
        StopAtByte {
            threshold,
            heard: AtomicU64::new(0),
        }
    }
}

impl Ctl for StopAtByte {
    fn bytes(&self, done: u64) {
        self.heard.fetch_max(done, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.heard.load(Ordering::SeqCst) >= self.threshold
    }
}

fn two_drives(name: &str) -> (TempDir, LocalDrive, LocalDrive) {
    let tmp = TempDir::new(name);
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    (tmp, home, other)
}

/// AZ-U2: a cross-drive folder Move ended by deleting the folder as it was NOW: files made there
/// after the plan (the plan can be hours old after a conflict dialog) were never copied and were
/// deleted. Only what landed is retired; a folder that still holds anything stays.
#[test]
fn az_u2_a_folder_move_deletes_what_it_copied_and_not_what_was_made_since() {
    let (tmp, home, other) = two_drives("u2");
    home.put("docs/a.txt", b"alpha").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    // Made while the dialog was open, and never in the plan.
    home.put("docs/late.txt", b"late").unwrap();
    home.put("docs/sub/later.txt", b"later").unwrap();
    let report = go(&plan, &home, &other, true);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.done, 2);
    assert_eq!(other.get("docs/a.txt").unwrap(), b"alpha");
    assert_eq!(other.get("docs/sub/b.txt").unwrap(), b"beta");
    assert!(home.get("docs/a.txt").is_err(), "the copied file went");
    assert!(home.get("docs/sub/b.txt").is_err());
    assert_eq!(
        home.get("docs/late.txt").unwrap(),
        b"late",
        "never copied, never deleted"
    );
    assert_eq!(home.get("docs/sub/later.txt").unwrap(), b"later");
    assert_eq!(report.left, vec![String::from("docs/")], "the folder stays");
    assert!(tmp.path().join("home").join("docs").is_dir());
}

/// A folder whose files all landed is removed, with the empty folders in it.
#[test]
fn a_folder_move_that_lands_everything_removes_the_emptied_folders() {
    let (tmp, home, other) = two_drives("u2-clean");
    home.put("docs/a.txt", b"alpha").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    home.create_folder("docs/empty/").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    let report = go(&plan, &home, &other, true);
    assert!(
        report.failed.is_empty() && report.left.is_empty(),
        "{report:?}"
    );
    assert!(!tmp.path().join("home").join("docs").exists());
    assert_eq!(other.get("docs/sub/b.txt").unwrap(), b"beta");
}

/// AZ X-AR5a-10: the move's last step removed the folder with `remove_dir_all` after a listing that
/// does not show a socket: it was removed with the folder, never copied.
#[cfg(unix)]
#[test]
fn x_ar5a_10_a_move_keeps_what_the_listing_cannot_see() {
    use std::os::unix::net::UnixListener;
    let (tmp, home, other) = two_drives("x10-move");
    home.put("docs/a.txt", b"alpha").unwrap();
    let socket = tmp.path().join("home").join("docs").join("s");
    let _listener = UnixListener::bind(&socket).unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    assert_eq!(plan.files.len(), 1, "the socket is not in the plan");
    let report = go(&plan, &home, &other, true);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(other.get("docs/a.txt").unwrap(), b"alpha");
    assert!(
        socket.exists(),
        "the socket was not deleted with its folder"
    );
    assert_eq!(report.left, vec![String::from("docs/")]);
}

/// A file written to while it was copied is not the file that was copied: it stays, and the
/// move says so. (The edit is made when the first progress report comes - after the version was
/// noted, before the source is retired.)
#[test]
fn az_u2_a_source_that_changed_since_it_was_noted_is_not_retired() {
    let (tmp, home, other) = two_drives("u2-edit");
    let big = vec![7u8; 2 * 1024 * 1024];
    home.put("a.bin", &big).unwrap();
    home.put("b.txt", b"b").unwrap();
    let plan = make_plan(
        &home,
        &[file("a.bin"), file("b.txt")],
        &other,
        "",
        true,
        "Copy",
    )
    .unwrap();
    let source_path = tmp.path().join("home").join("a.bin");
    let mut edited = false;
    let report = run(&plan, &home, &other, true, &Quiet, &mut |progress| {
        if !edited && progress.current == "a.bin" {
            edited = true;
            std::fs::write(&source_path, b"edited while it was copied").unwrap();
        }
    });
    assert!(edited, "the edit was made");
    assert!(conflict(&report, "a.bin"), "{report:?}");
    assert_eq!(
        home.get("a.bin").unwrap(),
        b"edited while it was copied",
        "the edit is still there"
    );
    assert!(home.get("b.txt").is_err(), "the untouched file moved");
    assert_eq!(other.get("b.txt").unwrap(), b"b");
}

/// AZ-U10: the cancel was read between files. A stop inside a file leaves the target as it was
/// and no temporary file, whatever the file's size.
#[test]
fn az_u10_a_stop_inside_a_file_leaves_the_target_as_it_was() {
    let (tmp, home, other) = two_drives("u10");
    home.put("big.bin", &vec![9u8; 4 * 1024 * 1024]).unwrap();
    other.put("old/keep.txt", b"keep").unwrap();
    let plan = make_plan(&home, &[file("big.bin")], &other, "", true, "Copy").unwrap();
    let report = run(&plan, &home, &other, true, &StopAtByte::new(1), &mut |_| {});
    assert!(report.cancelled, "{report:?}");
    assert!(
        report.failed.is_empty(),
        "a stop is no failure: {:?}",
        report.failed
    );
    assert_eq!(report.done, 0);
    assert!(
        other.get("big.bin").is_err(),
        "no half of a file under its name"
    );
    assert_eq!(
        home.get("big.bin").unwrap().len(),
        4 * 1024 * 1024,
        "the source is intact"
    );
    let leftovers: Vec<String> = std::fs::read_dir(tmp.path().join("other"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".azul-storage-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// A stop while a REPLACE copies leaves the old file as it was.
#[test]
fn az_u10_a_stop_inside_a_replace_leaves_the_file_it_replaces() {
    let (_tmp, home, other) = two_drives("u10-replace");
    home.put("big.bin", &vec![9u8; 4 * 1024 * 1024]).unwrap();
    other.put("big.bin", b"the old one").unwrap();
    let mut plan = make_plan(&home, &[file("big.bin")], &other, "", false, "Copy").unwrap();
    plan.choose(0, Choice::Replace);
    let report = run(
        &plan,
        &home,
        &other,
        false,
        &StopAtByte::new(1),
        &mut |_| {},
    );
    assert!(report.cancelled);
    assert_eq!(other.get("big.bin").unwrap(), b"the old one");
}

/// A move whose files landed before the stop retires them: each file is at the source or at the
/// target.
#[test]
fn a_stopped_move_retires_the_files_that_landed_before_the_stop() {
    let (_tmp, home, other) = two_drives("u10-landed");
    home.put("a.txt", b"first").unwrap();
    home.put("z.bin", &vec![1u8; 4 * 1024 * 1024]).unwrap();
    let plan = make_plan(
        &home,
        &[file("a.txt"), file("z.bin")],
        &other,
        "",
        true,
        "Copy",
    )
    .unwrap();
    // a.txt is 5 bytes: it lands before the threshold; z.bin is stopped.
    let report = run(
        &plan,
        &home,
        &other,
        true,
        &StopAtByte::new(1_000),
        &mut |_| {},
    );
    assert!(report.cancelled, "{report:?}");
    assert_eq!(other.get("a.txt").unwrap(), b"first");
    assert!(home.get("a.txt").is_err(), "it landed, so it was retired");
    assert!(other.get("z.bin").is_err());
    assert!(
        home.get("z.bin").is_ok(),
        "the stopped file is still at the source"
    );
}

/// AZ-U25: the conflict flags were fixed at the plan. A file made at the target since is not
/// replaced; the source stays, whether the move was one.
#[test]
fn az_u25_a_file_made_at_the_target_since_the_plan_is_not_replaced() {
    for removes_source in [false, true] {
        let (_tmp, home, other) = two_drives("u25");
        home.put("a.txt", b"new").unwrap();
        let plan = make_plan(&home, &[file("a.txt")], &other, "", removes_source, "Copy").unwrap();
        assert!(plan.conflicts().is_empty());
        other
            .put("a.txt", b"made while the dialog was open")
            .unwrap();
        let report = go(&plan, &home, &other, removes_source);
        assert!(conflict(&report, "a.txt"), "{report:?}");
        assert_eq!(report.done, 0);
        assert_eq!(
            other.get("a.txt").unwrap(),
            b"made while the dialog was open"
        );
        assert_eq!(home.get("a.txt").unwrap(), b"new", "the source stays");
    }
}

/// A Replace replaces the version the user was asked about, and no other.
#[test]
fn az_u25_a_replace_replaces_the_version_the_plan_saw_and_no_other() {
    let (_tmp, home, other) = two_drives("u25-replace");
    home.put("a.txt", b"new").unwrap();
    other.put("a.txt", b"old").unwrap();
    let mut plan = make_plan(&home, &[file("a.txt")], &other, "", false, "Copy").unwrap();
    assert_eq!(plan.conflicts(), vec![0]);
    assert!(
        plan.files[0].target_etag.is_some(),
        "the version the user is asked about"
    );
    plan.choose(0, Choice::Replace);
    // Saved again while the dialog was open.
    other.put("a.txt", b"edited since").unwrap();
    let report = go(&plan, &home, &other, false);
    assert!(conflict(&report, "a.txt"), "{report:?}");
    assert_eq!(other.get("a.txt").unwrap(), b"edited since");
    // A fresh plan sees the new version; replacing it works.
    let mut plan = make_plan(&home, &[file("a.txt")], &other, "", false, "Copy").unwrap();
    plan.choose(0, Choice::Replace);
    let report = go(&plan, &home, &other, false);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(other.get("a.txt").unwrap(), b"new");
}

/// Keep both writes the newcomer under its own name and touches the old file in no way.
#[test]
fn keep_both_leaves_the_old_file_and_lands_the_new_one_beside_it() {
    let (_tmp, home, other) = two_drives("keep-both");
    home.put("a.txt", b"new").unwrap();
    other.put("a.txt", b"old").unwrap();
    let mut plan = make_plan(&home, &[file("a.txt")], &other, "", true, "Copy").unwrap();
    plan.choose(0, Choice::KeepBoth);
    let report = go(&plan, &home, &other, true);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(other.get("a.txt").unwrap(), b"old");
    assert_eq!(other.get("a (2).txt").unwrap(), b"new");
    assert!(
        home.get("a.txt").is_err(),
        "a move retires the source that landed"
    );
}

/// A skipped conflict is not copied and, in a move, not deleted; the folder stays with it.
#[test]
fn a_skipped_file_stays_where_it_is_and_keeps_its_folder() {
    let (tmp, home, other) = two_drives("skip");
    home.put("docs/a.txt", b"new a").unwrap();
    home.put("docs/b.txt", b"new b").unwrap();
    other.put("docs/a.txt", b"old a").unwrap();
    let mut plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    plan.choose(plan.unresolved().unwrap(), Choice::Skip);
    let report = go(&plan, &home, &other, true);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.skipped, 1);
    assert_eq!(other.get("docs/a.txt").unwrap(), b"old a");
    assert_eq!(other.get("docs/b.txt").unwrap(), b"new b");
    assert_eq!(
        home.get("docs/a.txt").unwrap(),
        b"new a",
        "skipped, so still here"
    );
    assert!(home.get("docs/b.txt").is_err());
    assert!(tmp.path().join("home").join("docs").is_dir());
}

/// A file that vanished after the plan fails alone: the others land and are retired.
#[test]
fn a_file_gone_since_the_plan_fails_alone() {
    let (_tmp, home, other) = two_drives("gone");
    home.put("docs/a.txt", b"a").unwrap();
    home.put("docs/b.txt", b"b").unwrap();
    let plan = make_plan(&home, &[folder("docs/")], &other, "", true, "Copy").unwrap();
    home.delete("docs/a.txt").unwrap();
    let report = go(&plan, &home, &other, true);
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].0, "docs/a.txt");
    assert_eq!(other.get("docs/b.txt").unwrap(), b"b");
    assert!(home.get("docs/b.txt").is_err());
}

/// AZ-U17: the renames of a move within one drive that happened are what the report lists - what
/// Ctrl+Z puts back - and a rename that was refused (the name was made since) is not among them.
#[test]
fn az_u17_a_partly_failed_move_in_place_reports_what_was_renamed() {
    let tmp = TempDir::new("u17");
    let home = LocalDrive::without_manifest(tmp.path());
    home.put("a.txt", b"a").unwrap();
    home.put("b.txt", b"b").unwrap();
    home.create_folder("dest/").unwrap();
    let plan = make_plan(
        &home,
        &[file("a.txt"), file("b.txt")],
        &home,
        "dest/",
        true,
        "Copy",
    )
    .unwrap();
    assert_eq!(plan.moves.len(), 2);
    // Made since the plan.
    home.put("dest/b.txt", b"in the way").unwrap();
    let report = go(&plan, &home, &home, true);
    assert_eq!(report.done, 1);
    assert_eq!(
        report.moved,
        vec![(String::from("a.txt"), String::from("dest/a.txt"))]
    );
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "b.txt");
    assert_eq!(home.get("dest/b.txt").unwrap(), b"in the way");
    assert_eq!(home.get("b.txt").unwrap(), b"b");
}

/// Small files into a drive that has only the basic calls (no conditions): the batch asks the
/// drive, the drive says it cannot, the name is looked at and written. A name made since the plan
/// is still not replaced.
#[test]
fn small_files_into_a_bucket_that_cannot_ask_look_first() {
    let tmp = TempDir::new("bucket-small");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    home.put("a.txt", b"alpha").unwrap();
    home.put("b.txt", b"beta").unwrap();
    let bucket = Versioned::default();
    let plan = make_plan(
        &home,
        &[file("a.txt"), file("b.txt")],
        &bucket,
        "",
        true,
        "Copy",
    )
    .unwrap();
    bucket.write("b.txt", b"made meanwhile");
    let report = go(&plan, &home, &bucket, true);
    assert_eq!(report.done, 1, "{report:?}");
    assert!(conflict(&report, "b.txt"), "{report:?}");
    assert_eq!(bucket.get("a.txt").unwrap(), b"alpha");
    assert_eq!(bucket.get("b.txt").unwrap(), b"made meanwhile");
    assert!(home.get("a.txt").is_err(), "the landed one was retired");
    assert_eq!(home.get("b.txt").unwrap(), b"beta", "the refused one stays");
}

/// The file is its own destination (a plan made some other way): refused, never retired.
#[test]
fn a_file_that_is_its_own_destination_is_refused_and_kept() {
    let tmp = TempDir::new("self");
    let home = LocalDrive::new(tmp.path().join("home"));
    home.put("a.txt", b"only copy").unwrap();
    let again = LocalDrive::new(tmp.path().join("home"));
    let mut plan = Plan::default();
    plan.files.push(crate::xfer::PlannedFile {
        source_key: String::from("a.txt"),
        target_key: String::from("a.txt"),
        size: Some(9),
        conflict: true,
        choice: Some(Choice::Replace),
        source_etag: None,
        target_etag: None,
        target_size: None,
        target_modified: None,
        keep_as: None,
    });
    let report = go(&plan, &home, &again, true);
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.done, 0);
    assert_eq!(home.get("a.txt").unwrap(), b"only copy");
}
