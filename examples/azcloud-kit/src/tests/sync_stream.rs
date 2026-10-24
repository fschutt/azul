//! Big files through the sync: streamed up (never whole in memory, the BLAKE3 checked as the
//! file is read - a file that changes on the way is not completed and waits for the next run),
//! fetched down into a file, and no cap of a gibibyte on what a sync takes.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    error::CloudResult,
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
    sync::{
        local::{hash_bytes, path_of},
        remote::blob_key,
        sync_folder, LocalRoot, SyncOptions, MAX_FILE_BYTES,
    },
};

const PREFIX: &str = "stream/";

/// The bucket of the tests, with what the sync asked of it counted; `before_stream` runs right
/// before a streamed PUT (another program writing the file meanwhile).
struct Watched {
    inner: S3Bucket,
    puts: Mutex<Vec<String>>,
    streamed: Mutex<Vec<String>>,
    fetched: AtomicUsize,
    fetched_to_file: AtomicUsize,
    before_stream: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Watched {
    fn new() -> Watched {
        let mut inner = S3Bucket::new();
        // Parts of 1 MiB: a big file goes up as a multipart upload.
        inner.bucket.set_part_size(1024 * 1024);
        Watched {
            inner,
            puts: Mutex::new(Vec::new()),
            streamed: Mutex::new(Vec::new()),
            fetched: AtomicUsize::new(0),
            fetched_to_file: AtomicUsize::new(0),
            before_stream: Mutex::new(None),
        }
    }
}

impl RemoteStore for Watched {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.inner.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
        self.inner.fetch(key, size)
    }
    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.puts.lock().unwrap().push(key.to_string());
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
    fn put_from(&self, key: &str, body: &mut dyn Read, size: u64) -> CloudResult<String> {
        self.streamed.lock().unwrap().push(key.to_string());
        if let Some(edit) = self.before_stream.lock().unwrap().take() {
            edit();
        }
        self.inner.put_from(key, body, size)
    }
    fn fetch_to(&self, key: &str, size: u64, dest: &Path) -> CloudResult<bool> {
        self.fetched_to_file.fetch_add(1, Ordering::SeqCst);
        self.inner.fetch_to(key, size, dest)
    }
}

fn root(dir: &TempDir) -> LocalRoot {
    LocalRoot::folder(dir.path())
}

fn sync(
    store: &Watched,
    folder: &TempDir,
    state: &TempDir,
    device: &str,
) -> crate::sync::SyncReport {
    sync_folder(
        store,
        &root(folder),
        &state.path().join("index.json"),
        &SyncOptions::new(PREFIX, "d-test", device).unwrap(),
    )
    .unwrap()
}

fn big(seed: u8) -> Vec<u8> {
    (0..BIG_BLOB as usize + 3 * 1024 * 1024 + 11)
        .map(|i| (i % 249) as u8 ^ seed)
        .collect()
}

fn write(folder: &TempDir, key: &str, bytes: &[u8]) -> PathBuf {
    let path = path_of(folder.path(), key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_big_file_goes_up_streamed_and_comes_down_into_a_file() {
    let store = Watched::new();
    let (a, a_state) = (TempDir::new("stream-a"), TempDir::new("stream-a-state"));
    let bytes = big(1);
    write(&a, "video.bin", &bytes);
    write(&a, "small.txt", b"small");
    let report = sync(&store, &a, &a_state, "a");
    assert!(report.errors.is_empty(), "{}", report.summary());
    let blob = blob_key(PREFIX, &hash_bytes(&bytes));
    assert_eq!(*store.streamed.lock().unwrap(), vec![blob.clone()]);
    assert!(
        !store.puts.lock().unwrap().contains(&blob),
        "the big file never as one buffer"
    );
    assert_eq!(store.inner.read(&blob).unwrap(), bytes);

    let (b, b_state) = (TempDir::new("stream-b"), TempDir::new("stream-b-state"));
    let report = sync(&store, &b, &b_state, "b");
    assert!(report.errors.is_empty(), "{}", report.summary());
    assert_eq!(fs::read(b.path().join("video.bin")).unwrap(), bytes);
    assert_eq!(fs::read(b.path().join("small.txt")).unwrap(), b"small");
    assert_eq!(
        store.fetched_to_file.load(Ordering::SeqCst),
        1,
        "the big blob into a file"
    );
    assert_eq!(
        store.fetched.load(Ordering::SeqCst),
        1,
        "the small one in memory"
    );
}

#[test]
fn a_big_file_that_changes_on_its_way_up_is_not_completed_and_waits_for_the_next_run() {
    let store = Watched::new();
    let (a, a_state) = (
        TempDir::new("stream-change"),
        TempDir::new("stream-change-state"),
    );
    let bytes = big(2);
    let path = write(&a, "video.bin", &bytes);
    let edited = big(3);
    let edit_to = edited.clone();
    *store.before_stream.lock().unwrap() = Some(Box::new(move || {
        fs::write(&path, &edit_to).unwrap();
    }));
    let report = sync(&store, &a, &a_state, "a");
    assert!(
        report
            .changed_during_sync
            .iter()
            .any(|line| line.starts_with("video.bin")),
        "{}",
        report.summary()
    );
    let old_blob = blob_key(PREFIX, &hash_bytes(&bytes));
    assert!(
        store.inner.read(&old_blob).is_none(),
        "no blob under a name its content does not have"
    );
    let again = sync(&store, &a, &a_state, "a");
    assert!(again.errors.is_empty(), "{}", again.summary());
    assert_eq!(
        store
            .inner
            .read(&blob_key(PREFIX, &hash_bytes(&edited)))
            .unwrap(),
        edited,
        "the next run sends the file as it is"
    );
}

#[test]
fn a_sync_takes_files_above_a_gibibyte() {
    let opts = SyncOptions::new(PREFIX, "d-test", "a").unwrap();
    assert!(opts.max_file_bytes > 1 << 30);
    assert_eq!(MAX_FILE_BYTES, 5 * (1u64 << 40), "S3's largest object");
}
