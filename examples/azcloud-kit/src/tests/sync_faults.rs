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
