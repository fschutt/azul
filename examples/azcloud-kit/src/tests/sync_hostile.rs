//! What a hostile or broken server, network or disk does to a pass, as the engine names it: a
//! server that moved, a drive gone read-only, an object over the server's size cap, a file made
//! immutable, a folder that cannot be listed for a while, a crash inside a download's body, and
//! attributes that are not synced.

use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::{testing::TempDir, AzlinError, DriveError, ServiceError};

use super::fake_s3::S3Bucket;
use crate::{
    error::CloudError,
    store::RemoteStore,
    sync::{
        faults,
        problems::{code_of_error, of_failed_pass, Code, Severity},
        remote::{index_key, RemoteIndex},
        session::{SyncSession, SyncSetup},
    },
};

const PREFIX: &str = "hostile/";

/// A plain pairing of `folder` whose passes hear the fault seam under it.
fn session(folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> SyncSession {
    let remote: Arc<dyn RemoteStore> = store.clone();
    let root = folder.to_path_buf();
    SyncSession::plain(
        SyncSetup::new("d_hostile", PREFIX, folder),
        state.to_path_buf(),
        "dev",
        remote,
    )
    .with_points(Arc::new(move |name: &str| faults::point(&root, name)))
}

fn remote_keys(store: &S3Bucket) -> Vec<String> {
    store
        .read(&index_key(PREFIX))
        .and_then(|bytes| RemoteIndex::parse(&bytes).ok())
        .map(|index| index.files.keys().cloned().collect())
        .unwrap_or_default()
}

fn service(status: u16, code: &str, azlin: Option<AzlinError>) -> CloudError {
    CloudError::Drive(DriveError::Service(ServiceError {
        status,
        code: code.to_string(),
        message: String::from("the service said no"),
        azlin_error: azlin,
        ..ServiceError::default()
    }))
}

#[test]
fn the_new_codes_are_written_as_the_sync_harness_spells_them() {
    assert_eq!(Code::ServerMoved.as_str(), "server-moved");
    assert_eq!(Code::ReadOnly.as_str(), "read-only");
    assert_eq!(Code::XattrsNotSynced.as_str(), "xattrs-not-synced");
    assert_eq!(Code::XattrsNotSynced.severity(), Severity::Warning);
    assert_eq!(Code::ServerMoved.severity(), Severity::Error);
    assert!(Code::ServerMoved.is_pass_wide());
    assert!(!Code::XattrsNotSynced.is_pass_wide());
}

#[test]
fn a_moved_server_is_its_own_problem_and_not_a_sign_in() {
    let moved = service(301, "PermanentRedirect", None);
    assert_eq!(code_of_error(&moved), Code::ServerMoved);
    assert_eq!(of_failed_pass(&moved).code, Code::ServerMoved);
    assert_eq!(code_of_error(&service(308, "PermanentRedirect", None)), Code::ServerMoved);
}

#[test]
fn a_read_only_drive_says_so_instead_of_asking_to_sign_in_again() {
    for azlin in [
        AzlinError::ReadOnlyUnpaid,
        AzlinError::ReadOnlyLockdown,
        AzlinError::ReadOnlyOperator,
        AzlinError::DriveBanned,
    ] {
        let e = service(403, "AccessDenied", Some(azlin));
        assert_eq!(code_of_error(&e), Code::ReadOnly, "{e}");
    }
    // Revoked keys still ask to sign in.
    assert_eq!(code_of_error(&service(403, "AccessDenied", None)), Code::SignInNeeded);
}

#[test]
fn an_object_over_the_servers_cap_is_too_large_and_a_full_drive_is_quota() {
    assert_eq!(code_of_error(&service(413, "EntityTooLarge", None)), Code::TooLarge);
    assert_eq!(
        code_of_error(&service(400, "EntityTooLarge", Some(AzlinError::ObjectTooLarge))),
        Code::TooLarge
    );
    assert_eq!(
        code_of_error(&service(403, "QuotaExceeded", Some(AzlinError::QuotaExceeded))),
        Code::QuotaExceeded
    );
}

#[test]
fn an_immutable_file_or_folder_is_locked_not_an_io_error() {
    let e = CloudError::Io(String::from("rename a.txt: Operation not permitted (os error 1)"));
    assert_eq!(code_of_error(&e), Code::Locked);
    let e = CloudError::Io(String::from("Permission denied (os error 13)"));
    assert_eq!(code_of_error(&e), Code::Unreadable);
}

#[test]
fn a_folder_that_cannot_be_listed_for_one_scan_deletes_nothing_and_the_next_scan_takes_it_again() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-hostile-readdir");
    let state = TempDir::new("azcloud-hostile-readdir-state");
    fs::create_dir_all(folder.path().join("photos")).unwrap();
    for i in 0..3 {
        fs::write(folder.path().join(format!("photos/{i}.jpg")), format!("photo {i}")).unwrap();
    }
    let s = session(folder.path(), state.path(), &store);
    s.pass(&AtomicBool::new(false), &mut |_| {}).expect("the first pass");
    assert_eq!(remote_keys(&store).len(), 3);
    let eio = faults::errno_of("EIO").expect("EIO is known");
    faults::set_readdir_error(folder.path(), "photos", eio, 1);
    s.pass(&AtomicBool::new(false), &mut |_| {})
        .expect("a pass with one unreadable folder still runs");
    assert_eq!(remote_keys(&store).len(), 3, "nothing in the unreadable folder was deleted");
    let listed = s.problems();
    assert!(
        listed.problems.iter().any(|p| p.code == Code::Unreadable && p.key == "photos"),
        "{:?}",
        listed.problems
    );
    s.pass(&AtomicBool::new(false), &mut |_| {}).expect("the clean pass");
    assert!(!s.problems().problems.iter().any(|p| p.code == Code::Unreadable));
    assert_eq!(remote_keys(&store).len(), 3);
    faults::clear(folder.path());
}

#[test]
fn a_played_readdir_error_lasts_the_scans_it_was_set_for() {
    let root = Path::new("/hostile-test/readdir");
    faults::set_readdir_error(root, "a/b/", 5, 2);
    assert!(faults::readdir_error(root, "a/b").is_none(), "nothing before a scan starts");
    faults::scan_started(root);
    assert_eq!(faults::readdir_error(root, "a/b").and_then(|e| e.raw_os_error()), Some(5));
    assert!(faults::readdir_error(root, "a").is_none(), "only that folder");
    faults::scan_started(root);
    assert!(faults::readdir_error(root, "a/b").is_some(), "the second scan");
    faults::scan_started(root);
    assert!(faults::readdir_error(root, "a/b").is_none(), "gone after two scans");
    faults::clear(root);
}
