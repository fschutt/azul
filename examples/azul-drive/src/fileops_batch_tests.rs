//! A multi-select delete or move is one batch for the drive: on an encrypted drive one index
//! commit for all the files (not one per file), in a bucket one DeleteObjects request.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use azul_storage::{testing::TempDir, BatchDelete, BatchMove, Drive, DriveError, LocalDrive};

use crate::fileops::{delete_items, plan_transfer, run_transfer, SourceItem, TransferKind};

/// A local drive that counts single renames and deletes and the batches of them.
struct Batching {
    inner: LocalDrive,
    renames: AtomicU32,
    deletes: AtomicU32,
    move_batches: AtomicU32,
    delete_batches: AtomicU32,
}

impl Batching {
    fn new(tmp: &TempDir) -> Self {
        let inner = LocalDrive::new(tmp.path().join("home"));
        for key in ["a.txt", "b.txt", "c.txt"] {
            inner.put(key, b"words").unwrap();
        }
        inner.create_folder("docs/").unwrap();
        Self {
            inner,
            renames: AtomicU32::new(0),
            deletes: AtomicU32::new(0),
            move_batches: AtomicU32::new(0),
            delete_batches: AtomicU32::new(0),
        }
    }

    fn count(counter: &AtomicU32) -> u32 {
        counter.load(Ordering::SeqCst)
    }
}

impl Drive for Batching {
    fn list(&self, request: &azul_storage::ListRequest) -> Result<azul_storage::ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.inner.copy(from, to)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.create_folder(prefix)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.renames.fetch_add(1, Ordering::SeqCst);
        self.inner.rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.delete_folder(prefix)
    }
    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.inner.local_path(key)
    }
    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        self.delete_batches.fetch_add(1, Ordering::SeqCst);
        self.inner.delete_many(items)
    }
    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        self.move_batches.fetch_add(1, Ordering::SeqCst);
        self.inner.move_many(items)
    }
}

fn file(key: &str) -> SourceItem {
    SourceItem {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

#[test]
fn a_multi_select_delete_is_one_batch_into_the_trash_and_one_for_good() {
    let tmp = TempDir::new("batch-delete");
    let drive = Batching::new(&tmp);
    let items = [file("a.txt"), file("b.txt"), file("c.txt")];
    let gone = delete_items(&drive, &items, Some("20261015-120000-1")).expect("into the trash");
    assert_eq!(gone.len(), 3);
    assert_eq!(Batching::count(&drive.renames), 0, "no file renamed on its own");
    assert_eq!(Batching::count(&drive.move_batches), 1);
    for (key, trashed) in &gone {
        assert!(drive.inner.head(trashed).is_ok(), "{key} is in the trash");
        assert!(drive.inner.head(key).is_err(), "{key} left its folder");
    }
    let trashed: Vec<SourceItem> = gone.iter().map(|(_, t)| file(t)).collect();
    delete_items(&drive, &trashed, None).expect("for good");
    assert_eq!(Batching::count(&drive.deletes), 0, "no file deleted on its own");
    assert_eq!(Batching::count(&drive.delete_batches), 1);
}

#[test]
fn a_multi_select_move_within_a_drive_is_one_batch() {
    let tmp = TempDir::new("batch-move");
    let drive = Batching::new(&tmp);
    let items = [file("a.txt"), file("b.txt")];
    let plan = plan_transfer(&drive, &items, &drive, "docs/", true, TransferKind::Move, "Copy")
        .expect("the plan");
    assert_eq!(plan.moves.len(), 2, "renamed in place, nothing copied");
    let report = run_transfer(
        &plan,
        &drive,
        &drive,
        TransferKind::Move,
        &AtomicBool::new(false),
        &mut |_| {},
    );
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.done, 2);
    assert_eq!(Batching::count(&drive.renames), 0);
    assert_eq!(Batching::count(&drive.move_batches), 1);
    assert!(drive.inner.head("docs/a.txt").is_ok() && drive.inner.head("docs/b.txt").is_ok());
}

/// A bucket as the transfer sees it (no local paths), counting the single writes and the
/// batches of them.
struct Bucket {
    inner: LocalDrive,
    singles: AtomicU32,
    put_batches: AtomicU32,
}

impl Bucket {
    fn new(tmp: &TempDir) -> Self {
        Self {
            inner: LocalDrive::new(tmp.path().join("bucket")),
            singles: AtomicU32::new(0),
            put_batches: AtomicU32::new(0),
        }
    }
}

impl Drive for Bucket {
    fn list(&self, request: &azul_storage::ListRequest) -> Result<azul_storage::ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.singles.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, bytes)
    }
    fn put_file(
        &self,
        key: &str,
        path: &std::path::Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        self.singles.fetch_add(1, Ordering::SeqCst);
        self.inner.put_file(key, path, progress)
    }
    fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> Result<u64, DriveError> {
        self.singles.fetch_add(1, Ordering::SeqCst);
        self.inner.put_from(key, body)
    }
    fn put_many(&self, items: &[azul_storage::BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        self.put_batches.fetch_add(1, Ordering::SeqCst);
        items
            .iter()
            .map(|item| self.inner.put(item.key, item.bytes).map(|()| None))
            .collect()
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.inner.copy(from, to)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.create_folder(prefix)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.inner.rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.delete_folder(prefix)
    }
}

/// `n` files of `size` bytes each under `pics/` of a folder on this computer, each its own bytes.
fn small_files(tmp: &TempDir, n: usize, size: usize) -> LocalDrive {
    let home = LocalDrive::new(tmp.path().join("home"));
    for i in 0..n {
        let bytes: Vec<u8> = (0..size).map(|b| (b * 31 + i * 7) as u8).collect();
        home.put(&format!("pics/p{i:03}.jpg"), &bytes).unwrap();
    }
    home
}

fn pasted(source: &LocalDrive, target: &Bucket) -> (crate::fileops::TransferReport, usize) {
    let items = [SourceItem {
        key: String::from("pics/"),
        is_folder: true,
        size: None,
    }];
    let plan = plan_transfer(source, &items, target, "", false, TransferKind::Copy, "Copy").expect("the plan");
    let mut files_done = 0;
    let report = run_transfer(
        &plan,
        source,
        target,
        TransferKind::Copy,
        &AtomicBool::new(false),
        &mut |p| files_done = p.files_done,
    );
    (report, files_done)
}

/// Pasting a folder of photos from this computer into a drive sends its small files in one
/// batch: an encrypted drive packs a batch together and names it in one index change, while
/// one file at a time made every small file a pack of its own (300 files, 1309 objects).
#[test]
fn a_paste_of_small_files_from_this_computer_is_one_batch_and_a_large_file_goes_alone() {
    let tmp = TempDir::new("batch-paste");
    let home = small_files(&tmp, 40, 3_000);
    let big: Vec<u8> = (0..azul_storage::SMALL_FILE_BYTES as usize + 1).map(|b| b as u8).collect();
    home.put("pics/big.raw", &big).unwrap();
    let bucket = Bucket::new(&tmp);
    let (report, files_done) = pasted(&home, &bucket);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.done, 41);
    assert_eq!(files_done, 41, "the progress counts every file");
    assert_eq!(bucket.put_batches.load(Ordering::SeqCst), 1, "the 40 small files in one batch");
    assert_eq!(bucket.singles.load(Ordering::SeqCst), 1, "only the large file on its own");
    for i in 0..40 {
        let key = format!("pics/p{i:03}.jpg");
        assert_eq!(bucket.inner.get(&key).unwrap(), home.get(&key).unwrap(), "{key}");
    }
    assert_eq!(bucket.inner.get("pics/big.raw").unwrap(), big);
}

/// A batch holds at most a few packs' worth of bytes, so a big folder of small files goes in
/// several and never sits in memory whole.
#[test]
fn a_paste_of_more_small_files_than_a_batch_holds_goes_in_several_batches() {
    let tmp = TempDir::new("batch-paste-many");
    let size = 400 * 1024;
    let n = (azul_storage::SMALL_BATCH_BYTES as usize / size) + 5;
    let home = small_files(&tmp, n, size);
    let bucket = Bucket::new(&tmp);
    let (report, files_done) = pasted(&home, &bucket);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!((report.done, files_done), (n, n));
    assert_eq!(bucket.put_batches.load(Ordering::SeqCst), 2);
    assert_eq!(bucket.singles.load(Ordering::SeqCst), 0);
}
