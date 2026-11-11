//! A pass crashed or paused at a named point (the fault seam, `sync::faults`, heard through
//! `SyncSession::with_points`), the way a harness plays a process killed or stopped there:
//! what the drive and the folder hold afterwards, and the next pass.

use std::{
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use azul_storage::{lock::LockDir, testing::TempDir};

use super::fake_s3::S3Bucket;
use crate::{
    store::RemoteStore,
    sync::{
        faults,
        remote::{index_key, RemoteIndex},
        session::{Resolution, SyncSession, SyncSetup, SyncStates, PASS_LOCK},
    },
};

const PREFIX: &str = "faults/";

/// A pairing whose passes hear the fault seam under its folder.
fn faulted(folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> SyncSession {
    faulted_as("dev", folder, state, store)
}

/// [`faulted`] on the device `device` (its name is in the conflict copies it makes).
fn faulted_as(device: &str, folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> SyncSession {
    let remote: Arc<dyn RemoteStore> = store.clone();
    let root = folder.to_path_buf();
    SyncSession::plain(
        SyncSetup::new("d_faults", PREFIX, folder),
        state.to_path_buf(),
        device,
        remote,
    )
    .with_points(Arc::new(move |name: &str| faults::point(&root, name)))
}

fn pass(session: &SyncSession) -> std::thread::Result<()> {
    catch_unwind(AssertUnwindSafe(|| {
        session
            .pass(&AtomicBool::new(false), &mut |_| {})
            .map(|_| ())
            .expect("the pass itself succeeds");
    }))
}

#[test]
fn a_pass_crashed_after_its_blobs_went_up_commits_no_index_and_the_next_pass_does() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-blob");
    let state = TempDir::new("azcloud-faults-blob-state");
    fs::write(folder.path().join("f.txt"), "one").unwrap();
    let session = faulted(folder.path(), state.path(), &store);
    faults::arm_crash(folder.path(), "after-blob");
    let payload = pass(&session).expect_err("the pass crashes at after-blob");
    // The crash itself leaves the pass (not a failed pass the session noted).
    let point = faults::crashed_at(payload.as_ref());
    assert_eq!(point.as_deref(), Some("after-blob"));
    assert!(store.read(&index_key(PREFIX)).is_none(), "no index was committed");
    pass(&session).expect("the next pass runs through");
    let index = RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap();
    assert!(index.files.contains_key("f.txt"));
    faults::clear(folder.path());
}

#[test]
fn a_pass_crashed_after_its_commit_leaves_the_index_written_and_the_next_pass_sends_nothing_again() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-commit");
    let state = TempDir::new("azcloud-faults-commit-state");
    fs::write(folder.path().join("f.txt"), "two").unwrap();
    let session = faulted(folder.path(), state.path(), &store);
    faults::arm_crash(folder.path(), "after-commit");
    let payload = pass(&session).expect_err("the pass crashes at after-commit");
    assert_eq!(faults::crashed_at(payload.as_ref()).as_deref(), Some("after-commit"));
    assert_eq!(faults::crashes(folder.path()), 1);
    let index = RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap();
    assert!(index.files.contains_key("f.txt"), "the commit landed");
    let puts = store.count("PUT");
    pass(&session).expect("the next pass runs through");
    assert_eq!(store.count("PUT"), puts, "nothing is sent twice");
    assert_eq!(fs::read_to_string(folder.path().join("f.txt")).unwrap(), "two");
    faults::clear(folder.path());
}

#[test]
fn a_pass_paused_at_a_point_waits_there_and_finishes_once_released() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-pause");
    let state = TempDir::new("azcloud-faults-pause-state");
    fs::write(folder.path().join("f.txt"), "three").unwrap();
    let session = Arc::new(faulted(folder.path(), state.path(), &store));
    faults::arm_pause(folder.path(), "after-scan");
    let running = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || session.pass(&AtomicBool::new(false), &mut |_| {}).is_ok())
    };
    assert_eq!(
        faults::wait_paused(folder.path(), Duration::from_secs(30)).as_deref(),
        Some("after-scan")
    );
    assert!(store.read(&index_key(PREFIX)).is_none(), "nothing went up while paused");
    faults::release(folder.path());
    assert!(running.join().expect("the pass ends"), "the pass succeeds after the pause");
    assert!(store.read(&index_key(PREFIX)).is_some());
    faults::clear(folder.path());
}

#[test]
fn a_pass_crashed_before_it_moved_a_file_here_keeps_the_old_name_and_the_next_pass_moves_it() {
    let store = Arc::new(S3Bucket::new());
    let (a, a_state) = (TempDir::new("azcloud-faults-move-a"), TempDir::new("azcloud-faults-move-a-state"));
    let (b, b_state) = (TempDir::new("azcloud-faults-move-b"), TempDir::new("azcloud-faults-move-b-state"));
    let on_a = faulted(a.path(), a_state.path(), &store);
    let on_b = faulted(b.path(), b_state.path(), &store);
    fs::create_dir_all(a.path().join("docs")).unwrap();
    fs::write(a.path().join("docs/report.txt"), "the report").unwrap();
    pass(&on_a).expect("a sends the file");
    pass(&on_b).expect("b gets the file");
    fs::create_dir_all(a.path().join("archive")).unwrap();
    fs::rename(a.path().join("docs/report.txt"), a.path().join("archive/final.txt")).unwrap();
    pass(&on_a).expect("a sends the move");
    faults::arm_crash(b.path(), "before-rename");
    let payload = pass(&on_b).expect_err("b's pass crashes before it moves the file");
    assert_eq!(faults::crashed_at(payload.as_ref()).as_deref(), Some("before-rename"));
    assert!(b.path().join("docs/report.txt").exists(), "the file is still under its old name");
    pass(&on_b).expect("the next pass runs through");
    assert_eq!(
        fs::read_to_string(b.path().join("archive/final.txt")).unwrap(),
        "the report"
    );
    assert!(!b.path().join("docs/report.txt").exists());
    faults::clear(a.path());
    faults::clear(b.path());
}

/// A4: a pass that crashed after its commit and before it saved its base sent this device's own
/// change; an edit made after it is no conflict with that change.
#[test]
fn an_edit_after_a_pass_that_crashed_after_its_commit_is_no_conflict() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-lost-ack");
    let state = TempDir::new("azcloud-faults-lost-ack-state");
    fs::write(folder.path().join("main.swift"), "first").unwrap();
    let session = faulted(folder.path(), state.path(), &store);
    pass(&session).expect("the first pass");
    fs::write(folder.path().join("main.swift"), "second").unwrap();
    faults::arm_crash(folder.path(), "after-commit");
    assert!(pass(&session).is_err(), "the pass crashes after its commit");
    fs::write(folder.path().join("main.swift"), "third").unwrap();
    pass(&session).expect("the next pass runs through");
    let index = RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap();
    assert_eq!(index.files.len(), 1, "no conflict copy on the drive: {:?}", index.files.keys());
    assert_eq!(
        index.files["main.swift"].hash,
        crate::sync::local::hash_bytes(b"third")
    );
    let here: Vec<_> = fs::read_dir(folder.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".azlin")
        .collect();
    assert_eq!(here, ["main.swift"], "no conflict copy here");
    faults::clear(folder.path());
}

/// AZ-S10: a pass cut after its commit (the conflict's copy is on the drive with this side's
/// bytes) and before the rename of this side's file to the copy's name. The next pass finishes
/// that conflict: the copy stays, no second copy is made, and nothing is deleted on the drive.
/// RED on the old code (not run): `adopt_sent` took the copy into the base first, so the next
/// pass planned `DeleteRemote` for it - a tombstone - and a second copy ("... 2)"); the last
/// assertion on `index.deleted` fails with the copy's name in it.
#[test]
fn az_s10_a_conflict_cut_between_its_commit_and_its_rename_is_finished_not_deleted() {
    let store = Arc::new(S3Bucket::new());
    let (a, a_state) = (
        TempDir::new("azcloud-faults-s10-a"),
        TempDir::new("azcloud-faults-s10-a-state"),
    );
    let (b, b_state) = (
        TempDir::new("azcloud-faults-s10-b"),
        TempDir::new("azcloud-faults-s10-b-state"),
    );
    let on_a = faulted_as("laptop", a.path(), a_state.path(), &store);
    let on_b = faulted_as("phone", b.path(), b_state.path(), &store);
    fs::write(a.path().join("plan.txt"), "base").unwrap();
    pass(&on_a).expect("a sends the file");
    pass(&on_b).expect("b gets it");
    fs::write(a.path().join("plan.txt"), "a's edit").unwrap();
    fs::write(b.path().join("plan.txt"), "b's edit").unwrap();
    pass(&on_a).expect("a sends its edit");
    pass(&on_b).expect("b meets the conflict and holds it");
    on_b.resolve("plan.txt", Resolution::KeepBoth)
        .expect("the user keeps both");
    faults::arm_crash(b.path(), "conflict-start");
    let payload = pass(&on_b).expect_err("b's pass crashes after its commit, before the rename");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("conflict-start")
    );
    let cut = RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap();
    let copy: Vec<String> = cut
        .files
        .keys()
        .filter(|key| key.contains("(conflict"))
        .cloned()
        .collect();
    assert_eq!(
        copy.len(),
        1,
        "the commit landed with the copy: {:?}",
        cut.files.keys()
    );
    let copy = copy[0].clone();
    assert_eq!(
        fs::read_to_string(b.path().join("plan.txt")).unwrap(),
        "b's edit",
        "the file was not renamed yet"
    );

    pass(&on_b).expect("the next pass runs through");

    let index = RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap();
    let copies: Vec<&String> = index
        .files
        .keys()
        .filter(|key| key.contains("(conflict"))
        .collect();
    assert_eq!(copies, [&copy], "the same copy, never a second one");
    assert_eq!(
        fs::read_to_string(b.path().join(&copy)).unwrap(),
        "b's edit",
        "this side's bytes are in the copy here"
    );
    assert_eq!(
        fs::read_to_string(b.path().join("plan.txt")).unwrap(),
        "a's edit",
        "the drive's version keeps the name"
    );
    assert!(
        index.deleted.is_empty(),
        "nothing is deleted on the drive: {:?}",
        index.deleted.keys()
    );
    faults::clear(a.path());
    faults::clear(b.path());
}

/// AZ-S16. Nothing kept two passes on one pairing apart: a second window or the command line ran
/// beside the first, recovered the journal under it and saved the states it had read before the
/// other's. One pass at a time now; the second is told so after two seconds and changes nothing -
/// not the drive, not the states (no failure is recorded: it did not run). RED not run: the old
/// code runs the pass and `unwrap_err` panics.
#[test]
fn az_s16_a_pass_on_a_pairing_another_pass_holds_is_told_so_and_changes_nothing() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-s16");
    let state = TempDir::new("azcloud-faults-s16-state");
    fs::write(folder.path().join("a.txt"), "one").unwrap();
    let session = faulted(folder.path(), state.path(), &store);
    let other = LockDir::new(state.path().join("locks"))
        .lock(PASS_LOCK, Duration::from_secs(1))
        .expect("the other pass holds the pairing");
    let told = session
        .pass(&AtomicBool::new(false), &mut |_| {})
        .expect_err("a pass runs beside another")
        .to_string();
    assert!(told.contains("another window or command"), "{told}");
    assert!(store.read(&index_key(PREFIX)).is_none(), "nothing went up");
    assert_eq!(
        session.states(),
        SyncStates::default(),
        "the pass did not run: no failure is recorded"
    );
    drop(other);
    pass(&session).expect("once the other pass is over, this one runs");
    assert!(store.read(&index_key(PREFIX)).is_some());
    faults::clear(folder.path());
}

/// X-DELTA-AZDRIVE-FILES-26. The read-back (`verify`) takes no pass lock: it ran beside a pass,
/// replaced files and rewrote the base under it, and a pass that read the folder meanwhile could
/// resurrect a delete or add conflict copies. It takes the pass's turn now, and says so when
/// another holds it. RED not run: the old code verifies beside the pass and `unwrap_err` panics.
#[test]
fn x_delta_azdrive_files_26_the_read_back_waits_for_the_pass_that_holds_the_pairing() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-dx26");
    let state = TempDir::new("azcloud-faults-dx26-state");
    fs::write(folder.path().join("a.txt"), "one").unwrap();
    let session = faulted(folder.path(), state.path(), &store);
    pass(&session).expect("the folder is synced once");
    let other = LockDir::new(state.path().join("locks"))
        .lock(PASS_LOCK, Duration::from_secs(1))
        .expect("a pass holds the pairing");
    let told = session
        .verify()
        .expect_err("the read-back runs beside a pass")
        .to_string();
    assert!(told.contains("another window or command"), "{told}");
    assert!(told.contains("this check waits"), "{told}");
    drop(other);
    let report = session
        .verify()
        .expect("once the pass is over, the check runs");
    assert!(report.mismatches.is_empty(), "{report:?}");
    faults::clear(folder.path());
}

/// X-DELTA-AZDRIVE-SYNC-9 (AZ-S16). A pass and the read-back hold the pairing's lock, but the app's
/// own edits of the base - a delete, a rename, "Free up space", "send it again", a conflict's answer
/// - took none: a pass that began earlier saved the base it had loaded over the edit (the answer
/// "Keep" was lost and the question came back, a file the user deleted came back as a
/// placeholder). Each now takes the pass's turn, waits for it (`with_edit_wait`; a minute by
/// default, on the app's worker thread) and says so when the pass is still running; nothing is
/// changed then. RED not run: the old code edits beside the pass and `unwrap_err` panics.
#[test]
fn x_delta_azdrive_sync_9_the_apps_edits_of_the_base_take_the_passs_turn_and_change_nothing_beside_it(
) {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-s9");
    let state = TempDir::new("azcloud-faults-s9-state");
    fs::write(folder.path().join("a.txt"), "one").unwrap();
    fs::write(folder.path().join("b.txt"), "two").unwrap();
    let session =
        faulted(folder.path(), state.path(), &store).with_edit_wait(Duration::from_millis(200));
    pass(&session).expect("the folder is synced once");
    let before = fs::read(state.path().join(crate::sync::session::INDEX_FILE)).unwrap();

    let other = LockDir::new(state.path().join("locks"))
        .lock(PASS_LOCK, Duration::from_secs(1))
        .expect("a pass holds the pairing");
    let told = |what: &str, answer: Result<(), crate::error::CloudError>| {
        let text = answer
            .expect_err(&format!("{what} ran beside a pass"))
            .to_string();
        assert!(text.contains("pass"), "{what}: {text}");
    };
    told(
        "delete",
        session.delete(&[String::from("a.txt")]).map(|_| ()),
    );
    told("rename", session.rename("a.txt", "c.txt"));
    told(
        "free up",
        session.free_up(&[String::from("b.txt")]).map(|_| ()),
    );
    told("send again", session.send_kept(&[String::from("a.txt")]));
    told("resolve", session.resolve("a.txt", Resolution::KeepMine));
    assert_eq!(
        fs::read_to_string(folder.path().join("a.txt")).unwrap(),
        "one",
        "nothing was deleted or renamed beside the pass"
    );
    assert!(!folder.path().join("c.txt").exists());
    assert_eq!(
        fs::read(state.path().join(crate::sync::session::INDEX_FILE)).unwrap(),
        before,
        "the base is as the pass left it"
    );

    drop(other);
    let taken = session
        .delete(&[String::from("a.txt")])
        .expect("once the pass is over, the delete runs");
    assert_eq!(taken, vec![String::from("a.txt")]);
    assert!(!folder.path().join("a.txt").exists());
    faults::clear(folder.path());
}

/// X-AR5a-22. The states mixed the user's answers (a pin, a "keep both", an answer to a question)
/// with the pass's results, and the pass saved the copy it read when it began: a pin made while it
/// ran was gone when it ended. The pass's results are merged into the file as it is then; the pin
/// stays. RED not run: the old code writes its start-time copy over the pin.
#[test]
fn x_ar5a_22_a_pin_made_while_a_pass_runs_is_not_lost_when_the_pass_ends() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-x22");
    let state = TempDir::new("azcloud-faults-x22-state");
    fs::write(folder.path().join("a.txt"), "one").unwrap();
    let session = Arc::new(faulted(folder.path(), state.path(), &store));
    faults::arm_pause(folder.path(), "after-scan");
    let running = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || session.pass(&AtomicBool::new(false), &mut |_| {}).is_ok())
    };
    assert_eq!(
        faults::wait_paused(folder.path(), Duration::from_secs(30)).as_deref(),
        Some("after-scan")
    );
    // The user pins a file while the pass is paused in the middle of its work.
    session.pin(&[String::from("a.txt")], true).unwrap();
    faults::release(folder.path());
    assert!(running.join().expect("the pass ends"), "the pass succeeds");
    let states = session.states();
    assert!(
        states.pinned.contains("a.txt"),
        "the pin outlived the pass: {:?}",
        states.pinned
    );
    assert!(
        states.files.contains_key("a.txt"),
        "and the pass's results are there"
    );
    // The states name every file of the pairing: this user's alone, the folder and the file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [state.path().to_path_buf(), state.path().join("files.json")] {
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{path:?} is {:o}", mode & 0o777);
        }
    }
    faults::clear(folder.path());
}

/// A pairing synced once with one file `key` of `bytes` in it, and its session.
fn synced_with(
    name: &str,
    store: &Arc<S3Bucket>,
    key: &str,
    bytes: &str,
) -> (TempDir, TempDir, SyncSession) {
    let folder = TempDir::new(&format!("azcloud-faults-{name}"));
    let state = TempDir::new(&format!("azcloud-faults-{name}-state"));
    fs::write(folder.path().join(key), bytes).unwrap();
    let session = faulted(folder.path(), state.path(), store);
    pass(&session).expect("the folder is synced once");
    (folder, state, session)
}

/// The drive's index as the bucket holds it.
fn drive_index(store: &S3Bucket) -> RemoteIndex {
    RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap()
}

/// The pairing's base as the state folder holds it.
fn base_of(state: &Path) -> crate::sync::LocalIndex {
    crate::sync::LocalIndex::load(&state.join(crate::sync::session::INDEX_FILE))
        .expect("the base reads")
        .expect("the folder was synced")
}

/// X-DELTA-SYNC2-1. "Free up space" unlinked the file and only THEN wrote its base entry as "in the
/// cloud only": a process cut between the two (a quit, a laptop that sleeps, a power cut) left a base
/// that still said "synced here" beside an empty name, and the next pass read the empty name as the
/// user's own delete - a tombstone, the file gone from the drive and from every device. The record
/// comes first now. RED on the old order (the point `after-evict-unlink` stood between the unlink and
/// the record): the next pass commits the tombstone and `index.files` no longer lists the file.
#[test]
fn x_delta_sync2_1_a_free_up_cut_after_the_unlink_leaves_the_copy_on_the_drive_alone() {
    let store = Arc::new(S3Bucket::new());
    let (folder, state, session) = synced_with(
        "s2-1-after",
        &store,
        "big.txt",
        "the bytes of the version on the drive",
    );
    faults::arm_crash(folder.path(), "after-evict-unlink");
    let payload = catch_unwind(AssertUnwindSafe(|| {
        session.free_up(&[String::from("big.txt")]).map(|_| ())
    }))
    .expect_err("the process is cut when the file has left");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("after-evict-unlink")
    );
    assert!(
        !folder.path().join("big.txt").exists(),
        "the file had left this device"
    );
    drop(session);

    // A new process: the folder, the state folder and the drive are all it has.
    let session = faulted(folder.path(), state.path(), &store);
    pass(&session).expect("the next pass runs through");
    let index = drive_index(&store);
    assert!(
        index.files.contains_key("big.txt"),
        "the drive still holds the file it was told to keep: {:?}",
        index.files.keys()
    );
    assert!(
        index.deleted.is_empty(),
        "nothing was deleted on the drive: {:?}",
        index.deleted.keys()
    );
    assert!(
        base_of(state.path()).files["big.txt"].cloud_only,
        "and the base still says it is in the cloud only"
    );
    let opened = session
        .open("big.txt")
        .expect("it opens: it comes down from the drive");
    assert_eq!(
        fs::read_to_string(opened).unwrap(),
        "the bytes of the version on the drive"
    );
    faults::clear(folder.path());
}

/// X-DELTA-SYNC2-1, the other cut: after the base says "in the cloud only" and before the file leaves
/// (the point is new with the new order). The file is here beside a cloud-only entry: the next pass
/// reads it (it is the drive's version), keeps it, deletes nothing on the drive, and the entry is a
/// whole one again.
#[test]
fn x_delta_sync2_1_a_free_up_cut_after_the_record_and_before_the_unlink_keeps_the_file_and_the_drive(
) {
    let store = Arc::new(S3Bucket::new());
    let (folder, state, session) = synced_with(
        "s2-1-before",
        &store,
        "big.txt",
        "the bytes of the version on the drive",
    );
    faults::arm_crash(folder.path(), "before-evict-unlink");
    let payload = catch_unwind(AssertUnwindSafe(|| {
        session.free_up(&[String::from("big.txt")]).map(|_| ())
    }))
    .expect_err("the process is cut before the file leaves");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("before-evict-unlink")
    );
    assert!(
        base_of(state.path()).files["big.txt"].cloud_only,
        "the base was written first"
    );
    assert_eq!(
        fs::read_to_string(folder.path().join("big.txt")).unwrap(),
        "the bytes of the version on the drive",
        "the file is where it was"
    );
    drop(session);

    let session = faulted(folder.path(), state.path(), &store);
    pass(&session).expect("the next pass runs through");
    let index = drive_index(&store);
    assert!(
        index.files.contains_key("big.txt"),
        "{:?}",
        index.files.keys()
    );
    assert!(index.deleted.is_empty(), "{:?}", index.deleted.keys());
    assert_eq!(
        fs::read_to_string(folder.path().join("big.txt")).unwrap(),
        "the bytes of the version on the drive",
        "the pass left the file alone"
    );
    assert!(
        !base_of(state.path()).files["big.txt"].cloud_only,
        "the entry is a whole one again: the file is here"
    );
    faults::clear(folder.path());
}

/// X-DELTA-SYNC2-1, a base that cannot be written ("no space left" on the very disk being freed, a
/// state folder that is read-only): the old order had unlinked the file by then, so the free-up
/// answered "kept" for a file that was gone and the next pass deleted it on the drive. The record
/// is the first step now: nothing has happened to the file when it fails. RED on the old order: the
/// file is freed (no record was asked for, the unlink did not care) and `freed.freed` lists it.
#[test]
fn x_delta_sync2_1_a_base_that_cannot_be_written_keeps_the_file_here_and_the_drive_as_it_is() {
    let store = Arc::new(S3Bucket::new());
    let (folder, state, session) = synced_with(
        "s2-1-full",
        &store,
        "big.txt",
        "the bytes of the version on the drive",
    );
    faults::set_disk_full(folder.path(), true);
    let freed = session.free_up(&[String::from("big.txt")]);
    faults::set_disk_full(folder.path(), false);
    let freed = freed.expect("free_up answers");
    assert!(
        freed.freed.is_empty(),
        "a file whose record cannot be written is not freed: {freed:?}"
    );
    assert_eq!(freed.kept.len(), 1, "{freed:?}");
    assert_eq!(
        fs::read_to_string(folder.path().join("big.txt")).unwrap(),
        "the bytes of the version on the drive"
    );
    assert!(!base_of(state.path()).files["big.txt"].cloud_only);
    pass(&session).expect("the next pass runs through");
    let index = drive_index(&store);
    assert!(index.files.contains_key("big.txt"));
    assert!(index.deleted.is_empty(), "{:?}", index.deleted.keys());
    faults::clear(folder.path());
}

/// X-DELTA-AZDRIVE-SYNC-16 (b), the window the first fix left: the file is looked at (and, when its
/// times moved, hashed) and only then removed, BY NAME - an editor that saves by rename in between
/// makes a NEW file under the name, which the unlink took and which exists nowhere else. The file is
/// taken aside in one step now and looked at there: the new file is put back where it was. The
/// pause stands where the old code was about to unlink. RED on the old code: the new file is gone
/// and the free-up says it freed the file.
#[test]
fn x_delta_azdrive_sync_16_an_editors_save_between_the_look_and_the_removal_is_never_removed() {
    let store = Arc::new(S3Bucket::new());
    let (folder, state, session) =
        synced_with("s16-window", &store, "doc.txt", "the version on the drive");
    let session = Arc::new(session);
    faults::arm_pause(folder.path(), "before-evict-unlink");
    let freeing = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || session.free_up(&[String::from("doc.txt")]))
    };
    assert_eq!(
        faults::wait_paused(folder.path(), Duration::from_secs(30)).as_deref(),
        Some("before-evict-unlink"),
        "the free-up reached the step before the removal"
    );
    // The editor saves by rename: a new file, another inode, under the name.
    fs::write(
        folder.path().join(".doc.txt.editor"),
        "the user's new version",
    )
    .unwrap();
    fs::rename(
        folder.path().join(".doc.txt.editor"),
        folder.path().join("doc.txt"),
    )
    .unwrap();
    faults::release(folder.path());
    let freed = freeing
        .join()
        .expect("the free-up ends")
        .expect("the free-up answers");
    assert!(
        freed.freed.is_empty(),
        "the file that was looked at is not the one that is there: {freed:?}"
    );
    assert_eq!(freed.kept.len(), 1, "{freed:?}");
    assert_eq!(
        fs::read_to_string(folder.path().join("doc.txt")).unwrap(),
        "the user's new version",
        "the new file is where the editor put it"
    );
    let left: Vec<String> = fs::read_dir(folder.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".azlin" && n != "doc.txt")
        .collect();
    assert!(left.is_empty(), "nothing is left beside it: {left:?}");
    assert!(
        !base_of(state.path()).files["doc.txt"].cloud_only,
        "the base is as it was: the file is here"
    );
    // The next pass sees an edit and sends it.
    pass(&session).expect("the next pass runs through");
    assert_eq!(
        drive_index(&store).files["doc.txt"].hash,
        crate::sync::local::hash_bytes(b"the user's new version"),
        "the new version went to the drive as an edit"
    );
    faults::clear(folder.path());
}

/// X-DELTA-SYNC2-1: a file that cannot be taken away (the folder is read-only) is kept, and the base
/// entry that was written first is put back whole - a cloud-only entry beside a file that is here
/// would be read once and repaired, but the base says what is true.
#[cfg(unix)]
#[test]
fn x_delta_sync2_1_a_file_that_cannot_be_taken_away_keeps_its_whole_base_entry() {
    use std::os::unix::fs::PermissionsExt;
    let store = Arc::new(S3Bucket::new());
    let (folder, state, session) = synced_with(
        "s2-1-ro",
        &store,
        "a.txt",
        "the bytes of the version on the drive",
    );
    let before = base_of(state.path()).files["a.txt"].clone();
    let own = |mode: u32| fs::set_permissions(folder.path(), fs::Permissions::from_mode(mode));
    own(0o555).unwrap();
    // A user who may write to a read-only folder (root) cannot play this.
    let probe = folder.path().join(".probe");
    if fs::write(&probe, b"x").is_ok() {
        fs::remove_file(&probe).unwrap();
        own(0o755).unwrap();
        return;
    }
    let freed = session.free_up(&[String::from("a.txt")]);
    own(0o755).unwrap();
    let freed = freed.expect("free_up answers");
    assert!(freed.freed.is_empty(), "{freed:?}");
    assert_eq!(freed.kept.len(), 1, "{freed:?}");
    assert!(folder.path().join("a.txt").exists(), "the file stays");
    assert_eq!(
        base_of(state.path()).files["a.txt"],
        before,
        "the entry is put back whole, with the identity it had"
    );
    pass(&session).expect("the next pass runs through");
    assert!(drive_index(&store).deleted.is_empty());
    faults::clear(folder.path());
}

/// X-DELTA-SYNC2-1, the other caller of the freeing: the size cap frees files after EVERY pass of a
/// pairing that has one (`keep_cap`), inside the pass's own lock and with nobody watching. A cap of
/// nothing frees the file the pass just sent; the process is cut as the file leaves; a new process
/// runs the next pass: the drive keeps the file (the old order recorded the freeing after the
/// unlink, and the next pass deleted the file on the drive).
#[test]
fn x_delta_sync2_1_a_pass_cut_while_the_size_cap_frees_a_file_leaves_the_drive_alone() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-faults-s2-1-cap");
    let state = TempDir::new("azcloud-faults-s2-1-cap-state");
    fs::write(
        folder.path().join("big.txt"),
        "the bytes of the version on the drive",
    )
    .unwrap();
    let capped = || {
        let remote: Arc<dyn RemoteStore> = store.clone();
        let root = folder.path().to_path_buf();
        let mut setup = SyncSetup::new("d_faults", PREFIX, folder.path());
        // At most 0 GB here: what is left of the folder after the pass is freed.
        setup.keep_gb = Some(0);
        SyncSession::plain(setup, state.path().to_path_buf(), "dev", remote)
            .with_points(Arc::new(move |name: &str| faults::point(&root, name)))
    };
    faults::arm_crash(folder.path(), "after-evict-unlink");
    let payload = pass(&capped()).expect_err("the pass is cut while the cap frees the file");
    assert_eq!(
        faults::crashed_at(payload.as_ref()).as_deref(),
        Some("after-evict-unlink")
    );
    assert!(
        !folder.path().join("big.txt").exists(),
        "the cap had freed it"
    );
    assert!(
        drive_index(&store).files.contains_key("big.txt"),
        "the pass had sent it before it was freed"
    );

    // A new process: the next pass.
    pass(&capped()).expect("the next pass runs through");
    let index = drive_index(&store);
    assert!(
        index.files.contains_key("big.txt"),
        "the drive still holds the file: {:?}",
        index.files.keys()
    );
    assert!(
        index.deleted.is_empty(),
        "nothing was deleted on the drive: {:?}",
        index.deleted.keys()
    );
    assert!(base_of(state.path()).files["big.txt"].cloud_only);
    faults::clear(folder.path());
}
