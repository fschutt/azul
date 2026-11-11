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
