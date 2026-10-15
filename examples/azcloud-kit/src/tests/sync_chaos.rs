//! The sync under hostile conditions, as the chaos E2E (scripts/azdrive_chaos_e2e.py) meets
//! them in the running app: what a failed pass leaves for the app to say, the files a pass
//! cannot take, names a disk cannot hold side by side, and a folder dated by a clock that runs
//! ahead.

use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant, SystemTime},
};

use azul_storage::{testing::TempDir, DriveError, ServiceError};

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device, PREFIX},
};
use crate::{
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore},
    sync::{
        local::{self, hash_bytes},
        remote::{blob_key, index_key},
        session::{FileState, SyncSession, SyncSetup},
        RemoteFile, RemoteIndex, Rules,
    },
};

/// A drive that answers every write with `refusal`; reads go through.
struct Refusing {
    inner: Arc<S3Bucket>,
    refusal: DriveError,
}

impl RemoteStore for Refusing {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.inner.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        self.inner.fetch(key, size)
    }
    fn put(&self, _key: &str, _data: &[u8]) -> CloudResult<String> {
        Err(CloudError::Drive(self.refusal.clone()))
    }
    fn put_if(&self, key: &str, data: &[u8], _: Option<&str>) -> CloudResult<Option<String>> {
        self.put(key, data).map(Some)
    }
    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        self.inner.head(key)
    }
    fn delete(&self, key: &str) -> CloudResult<()> {
        self.put(key, b"").map(|_| ())
    }
    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.inner.list(prefix)
    }
}

fn session_over(store: Arc<dyn RemoteStore>, folder: &TempDir, state: &TempDir) -> SyncSession {
    SyncSession::plain(
        SyncSetup::new("d_test", "", folder.path()),
        state.path().to_path_buf(),
        "laptop",
        store,
    )
}

#[test]
fn a_failed_pass_keeps_the_error_tables_code_and_a_good_pass_clears_it() {
    let bucket = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-chaos-problem");
    let state = TempDir::new("azcloud-chaos-problem-state");
    fs::write(folder.path().join("new.txt"), b"new").unwrap();
    let removed = DriveError::Service(ServiceError {
        status: 403,
        code: String::from("InvalidAccessKeyId"),
        message: String::from("this device was removed from the drive"),
        request_id: Some(String::from("n1-77")),
        azlin_error: Some(String::from("credentials_revoked")),
        ..ServiceError::default()
    });
    let offline = DriveError::Transport(String::from("http://n1: Connection refused"));
    for (refusal, code) in [(removed, "credentials_revoked"), (offline, "network")] {
        let store: Arc<dyn RemoteStore> = Arc::new(Refusing {
            inner: bucket.clone(),
            refusal,
        });
        let session = session_over(store, &folder, &state);
        assert!(session.pass(&AtomicBool::new(false), &mut |_| {}).is_err());
        let problem = session
            .states()
            .last_problem
            .expect("the errors table's code is kept for the app's status line");
        assert_eq!(problem.code, code);
    }
    let kept = session_over(bucket.clone(), &folder, &state).states();
    assert_eq!(
        kept.last_problem.map(|p| p.request_id),
        Some(None),
        "a transport error has no request id; the last one is the one kept"
    );
    let session = session_over(bucket.clone(), &folder, &state);
    session.pass(&AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(session.states().last_problem, None);
}

#[cfg(unix)]
#[test]
fn a_pass_names_the_files_it_cannot_take_and_why() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let bucket = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-chaos-skipped");
    let state = TempDir::new("azcloud-chaos-skipped-state");
    fs::write(folder.path().join("plain.txt"), b"plain").unwrap();
    symlink(
        folder.path().join("plain.txt"),
        folder.path().join("link.txt"),
    )
    .unwrap();
    let secret = folder.path().join("secret.txt");
    fs::write(&secret, b"secret").unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads whatever the permissions say.
    let unreadable = fs::read(&secret).is_err();
    let session = session_over(bucket.clone(), &folder, &state);
    let passed = session.pass(&AtomicBool::new(false), &mut |_| {});
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
    passed.unwrap();
    let states = session.states();
    assert!(
        states
            .skipped
            .get("link.txt")
            .is_some_and(|why| why.contains("symbolic link")),
        "{:?}",
        states.skipped
    );
    assert!(matches!(
        states.state_of("link.txt"),
        Some(FileState::Error(_))
    ));
    if unreadable {
        assert!(
            states
                .skipped
                .get("secret.txt")
                .is_some_and(|why| why.contains("cannot be read")),
            "{:?}",
            states.skipped
        );
    }
    assert_eq!(states.cannot_sync(), if unreadable { 2 } else { 1 });
    // Once it can be read, it goes up and is no longer named.
    session.pass(&AtomicBool::new(false), &mut |_| {}).unwrap();
    assert!(!session.states().skipped.contains_key("secret.txt"));
}

/// Whether the disk under `dir` takes `spelled` and `other` for one name.
fn folds(dir: &Path, spelled: &str, other: &str) -> bool {
    let probe = dir.join(spelled);
    fs::write(&probe, b"x").unwrap();
    let folds = dir.join(other).exists();
    let _ = fs::remove_file(&probe);
    folds
}

/// Another device's commit of `files` (a device on a disk that keeps every spelling apart):
/// their blobs and the index.
fn other_device_commits(store: &S3Bucket, files: &[(&str, &[u8])]) {
    let mut index = RemoteIndex::empty();
    for (key, bytes) in files {
        let hash = hash_bytes(bytes);
        store.write(&blob_key(PREFIX, &hash), bytes.to_vec());
        index.generation += 1;
        index.files.insert(
            (*key).to_string(),
            RemoteFile {
                hash,
                size: bytes.len() as u64,
                mtime: 1,
                gen: index.generation,
                device: String::from("linux"),
            },
        );
    }
    store.write(&index_key(PREFIX), index.to_bytes());
}

#[test]
fn a_file_whose_name_another_spelling_holds_here_stays_on_the_drive_and_says_why() {
    let probe = TempDir::new("azcloud-chaos-probe");
    let nfc = "Caf\u{e9}.txt";
    let nfd = "Cafe\u{301}.txt";
    let twins: Vec<(&str, &str)> = [("Notes.txt", "notes.txt"), (nfc, nfd)]
        .into_iter()
        .filter(|(a, b)| folds(probe.path(), a, b))
        .collect();
    if twins.is_empty() {
        // A disk that keeps every spelling apart (Linux): nothing collides here.
        return;
    }
    for (first, second) in twins {
        let store = S3Bucket::new();
        let laptop = Device::new("laptop-twins");
        other_device_commits(
            &store,
            &[
                (first, &b"the first spelling"[..]),
                (second, &b"the second spelling"[..]),
            ],
        );
        let before = laptop.sync(&store);
        store.clear_log();
        let after = laptop.sync(&store);
        for report in [&before, &after] {
            assert!(
                report.changed_during_sync.is_empty(),
                "a twin is no change during the run: {:?}",
                report.changed_during_sync
            );
            assert_eq!(report.file_errors.len(), 1, "{:?}", report.file_errors);
            assert!(
                report.file_errors[0].1.contains("same name"),
                "{:?}",
                report.file_errors
            );
            assert_eq!(report.deleted_there, 0);
        }
        // The twin is not fetched again pass after pass.
        assert_eq!(
            store.count_under("GET", &format!("{PREFIX}.azlin/blobs/")),
            0
        );
        // The drive keeps both; the file here is one of them, whole.
        assert_eq!(index_of(&store).files.len(), 2);
        let here = laptop.read(first).expect("one of the two is here");
        assert!(
            here == b"the first spelling" || here == b"the second spelling",
            "{here:?}"
        );
    }
}

#[test]
fn a_file_dated_in_the_future_does_not_hold_a_pass_back() {
    let folder = TempDir::new("azcloud-chaos-future");
    let path = folder.path().join("from-the-future.txt");
    fs::write(&path, b"made on a computer whose clock is a day ahead").unwrap();
    let ahead = SystemTime::now() + Duration::from_secs(86_400);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(ahead)
        .unwrap();
    let started = Instant::now();
    local::settle(
        folder.path(),
        &Rules::base(),
        Duration::from_millis(500),
        Duration::from_secs(20),
        &|| false,
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a folder that does not change waited {:?} because a file's date lies ahead",
        started.elapsed()
    );
}
