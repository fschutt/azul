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

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    store::RemoteStore,
    sync::{
        faults,
        remote::{index_key, RemoteIndex},
        session::{SyncSession, SyncSetup},
    },
};

const PREFIX: &str = "faults/";

/// A pairing whose passes hear the fault seam under its folder.
fn faulted(folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> SyncSession {
    let remote: Arc<dyn RemoteStore> = store.clone();
    let root = folder.to_path_buf();
    SyncSession::plain(
        SyncSetup::new("d_faults", PREFIX, folder),
        state.to_path_buf(),
        "dev",
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
