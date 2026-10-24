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
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
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
        azlin_error: Some(azul_storage::AzlinError::CredentialsRevoked),
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
    // An encrypted drive this device holds no key for is the table's locked drive.
    let store: Arc<dyn RemoteStore> = Arc::new(Refusing {
        inner: bucket.clone(),
        refusal: DriveError::Locked {
            drive: String::from("d_1"),
        },
    });
    let session = session_over(store, &folder, &state);
    assert!(session.pass(&AtomicBool::new(false), &mut |_| {}).is_err());
    assert_eq!(
        session.states().last_problem.map(|p| p.code).as_deref(),
        Some("locked")
    );
    // A refusal the drive words itself (a grant that takes no writes) keeps its words.
    let store: Arc<dyn RemoteStore> = Arc::new(Refusing {
        inner: bucket.clone(),
        refusal: DriveError::Denied {
            message: String::from("this drive takes no writes"),
        },
    });
    let session = session_over(store, &folder, &state);
    assert!(session.pass(&AtomicBool::new(false), &mut |_| {}).is_err());
    assert_eq!(session.states().last_problem, None);
    let kept = session_over(bucket.clone(), &folder, &state).states();
    assert!(kept.last_error.is_some(), "the drive's own words are kept");
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
                mtime_ns: 0,
                gen: index.generation,
                device: String::from("linux"),
                exec: false,
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

/// Bytes of a file of twelve parts of 1 MiB (it goes up as a multipart upload).
fn twelve_parts() -> Vec<u8> {
    (0..BIG_BLOB as usize + 3 * 1024 * 1024 + 11)
        .map(|i| (i % 251) as u8)
        .collect()
}

#[test]
fn a_big_file_cut_off_by_a_dropped_connection_resumes_with_the_parts_it_lacked() {
    let resume = TempDir::new("azcloud-chaos-resume");
    let mut store = S3Bucket::new();
    store.bucket.set_part_size(1024 * 1024);
    store.bucket.set_resume_dir(resume.path());
    let laptop = Device::new("laptop-resume");
    let bytes = twelve_parts();
    laptop.write("video.bin", &bytes);
    let blob = blob_key(PREFIX, &hash_bytes(&bytes));
    // The connection drops after four parts: the run cannot finish the file.
    store.take_parts(Some(4));
    let _ = laptop.sync_with(&store, &laptop.opts());
    assert!(store.read(&blob).is_none());
    store.take_parts(None);
    store.clear_log();
    let report = laptop.sync(&store);
    assert!(report.errors.is_empty(), "{}", report.summary());
    assert_eq!(store.read(&blob).as_deref(), Some(&bytes[..]));
    assert_eq!(store.count("PARTS"), 1, "the bucket says what it holds");
    assert_eq!(
        store.count_under("PUT", &blob),
        8,
        "the eight parts it lacked, never the first four again"
    );
    assert_eq!(
        store.count_under("POST", &blob),
        1,
        "one Complete, no new upload started"
    );
}

/// A drive whose PUTs of one key get no answer (a connection that drops for that request).
struct OneCut {
    inner: Arc<S3Bucket>,
    cut: String,
}

impl RemoteStore for OneCut {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.inner.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        self.inner.fetch(key, size)
    }
    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        if key == self.cut {
            return Err(CloudError::Drive(DriveError::Transport(String::from(
                "connection reset by peer",
            ))));
        }
        self.inner.put(key, data)
    }
    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        self.inner.put_if(key, data, if_match)
    }
    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        self.inner.head(key)
    }
    fn delete(&self, key: &str) -> CloudResult<()> {
        self.inner.delete(key)
    }
    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.inner.list(prefix)
    }
}

#[test]
fn a_transient_failure_of_one_upload_lets_the_other_files_reach_the_drive() {
    let bucket = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-chaos-one-cut");
    let state = TempDir::new("azcloud-chaos-one-cut-state");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(folder.path().join(name), name.as_bytes()).unwrap();
    }
    let cut: Arc<dyn RemoteStore> = Arc::new(OneCut {
        inner: bucket.clone(),
        cut: blob_key("", &hash_bytes(b"b.txt")),
    });
    let pass = session_over(cut, &folder, &state)
        .pass(&AtomicBool::new(false), &mut |_| {})
        .expect("one file's dropped connection fails that file, not the pass");
    let report = pass.report.expect("a sync pass");
    assert_eq!(
        report
            .file_errors
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["b.txt"]
    );
    let index = RemoteIndex::parse(&bucket.read(&index_key("")).expect("an index")).unwrap();
    assert!(index.files.contains_key("a.txt") && index.files.contains_key("c.txt"));
    assert!(!index.files.contains_key("b.txt"));
    assert!(matches!(
        pass.states.state_of("b.txt"),
        Some(FileState::Error(_))
    ));
    // The next pass sends the one that waited, and only it.
    bucket.clear_log();
    session_over(bucket.clone(), &folder, &state)
        .pass(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(
        bucket.count_under("PUT", ".azlin/blobs/"),
        1,
        "{:?}",
        bucket.log
    );
    let index = RemoteIndex::parse(&bucket.read(&index_key("")).unwrap()).unwrap();
    assert!(index.files.contains_key("b.txt"));
}

#[test]
fn a_folder_renamed_here_is_no_burst_of_changes() {
    // 300 files (more than the burst guard's 200) in a folder; the folder renamed: the drive
    // gets the new names, nothing waits for the user, no bytes go up again.
    let store = S3Bucket::new();
    let laptop = Device::new("laptop-rename-burst");
    for i in 0..300 {
        laptop.write(
            &format!("photos/p{i:03}.txt"),
            format!("photo {i}\n").as_bytes(),
        );
    }
    laptop.sync(&store);
    let before = store.count_under("PUT", &format!("{PREFIX}.azlin/blobs/"));
    laptop.rename_folder("photos", "pictures");
    let report = laptop.sync(&store);
    assert!(
        report.paused.is_none(),
        "a rename paused the uploads: {:?}",
        report.paused
    );
    assert!(report.mass_delete.is_none());
    let index = index_of(&store);
    assert_eq!(index.files.len(), 300);
    assert!(index.files.keys().all(|k| k.starts_with("pictures/")));
    assert_eq!(
        store.count_under("PUT", &format!("{PREFIX}.azlin/blobs/")),
        before,
        "a rename moves no bytes"
    );
}

#[test]
fn a_mass_delete_the_user_confirmed_is_no_burst_to_ask_about_again() {
    // 300 files (more than the burst guard's 200) deleted here: the mass-delete guard asks;
    // the answer "delete them on the drive too" sends them - without a second question.
    let store = S3Bucket::new();
    let laptop = Device::new("laptop-mass-then-burst");
    for i in 0..300 {
        laptop.write(
            &format!("old/f{i:03}.txt"),
            format!("old file {i}\n").as_bytes(),
        );
    }
    laptop.write("keep.txt", b"kept\n");
    laptop.sync(&store);
    laptop.remove_folder("old");
    // The command line's run stops (an app's run holds the question instead).
    assert!(
        laptop.sync_with(&store, &laptop.opts()).is_err(),
        "the mass-delete guard asks first"
    );
    let mut confirmed = laptop.opts();
    confirmed.allow_mass_delete = true;
    let report = laptop.sync_with(&store, &confirmed).unwrap();
    assert!(
        report.paused.is_none(),
        "the burst guard asked again about the deletes just confirmed: {:?}",
        report.paused
    );
    let index = index_of(&store);
    assert_eq!(index.files.keys().collect::<Vec<_>>(), ["keep.txt"]);
}

#[test]
fn a_folder_changed_while_it_is_read_is_read_again_once_it_is_quiet() {
    // A `rm -rf` running while a pass reads the folder: the chaos E2E's encrypted-bulk saw the
    // burst guard ask about the 2,488 files of 10,000 gone when the scan passed, and the
    // mass-delete question for all of them only after. A pass plans from a folder that did not
    // change while it was read.
    let folder = TempDir::new("azcloud-chaos-torn-scan");
    for i in 0..10 {
        fs::write(folder.path().join(format!("f{i}.txt")), b"a file").unwrap();
    }
    let rules = Rules::base();
    let mut reads = 0;
    let scan = local::scan_unchanged(
        folder.path(),
        &rules,
        Duration::from_millis(50),
        Duration::from_secs(10),
        &|| false,
        || {
            reads += 1;
            let scan = local::scan(
                folder.path(),
                &rules,
                &std::collections::BTreeMap::new(),
                0,
                &[],
                u64::MAX,
            )?;
            if reads == 1 {
                // The delete goes on while this read ends: half of the files go.
                for i in 0..5 {
                    fs::remove_file(folder.path().join(format!("f{i}.txt"))).unwrap();
                }
            }
            Ok(scan)
        },
    )
    .unwrap();
    assert_eq!(reads, 2, "the torn read is read again");
    assert_eq!(scan.files.len(), 5, "{:?}", scan.files.keys());

    // A folder that did not change is read once.
    let mut once = 0;
    local::scan_unchanged(
        folder.path(),
        &rules,
        Duration::from_millis(50),
        Duration::from_secs(10),
        &|| false,
        || {
            once += 1;
            local::scan(
                folder.path(),
                &rules,
                &std::collections::BTreeMap::new(),
                0,
                &[],
                u64::MAX,
            )
        },
    )
    .unwrap();
    assert_eq!(once, 1);
}
