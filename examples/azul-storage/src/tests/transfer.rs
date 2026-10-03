use std::sync::atomic::{AtomicU32, Ordering};

use super::TempDir;
use crate::{
    transfer::{download_path, download_to_file, upload_file},
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
};

/// A local drive that counts the reads.
struct Counting {
    inner: LocalDrive,
    gets: AtomicU32,
    ranges: AtomicU32,
    heads: AtomicU32,
}

impl Counting {
    fn new(inner: LocalDrive) -> Self {
        Counting {
            inner,
            gets: AtomicU32::new(0),
            ranges: AtomicU32::new(0),
            heads: AtomicU32::new(0),
        }
    }
}

impl Drive for Counting {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.heads.fetch_add(1, Ordering::SeqCst);
        self.inner.head(key)
    }
}

#[test]
fn a_small_object_downloads_in_one_get() {
    let tmp = TempDir::new("transfer-small");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("mail/a.eml", b"hello").unwrap();
    let dest = tmp.path().join("out").join("a.eml");
    let written = download_to_file(&drive, "mail/a.eml", Some(5), &dest, 1024).unwrap();
    assert_eq!(written, 5);
    assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
    assert_eq!(drive.gets.load(Ordering::SeqCst), 1);
    assert_eq!(drive.ranges.load(Ordering::SeqCst), 0);
    assert_eq!(drive.heads.load(Ordering::SeqCst), 0);
}

#[test]
fn a_big_object_downloads_in_ranged_chunks() {
    let tmp = TempDir::new("transfer-big");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("big.bin", b"0123456789").unwrap();
    let dest = tmp.path().join("big.bin");
    let written = download_to_file(&drive, "big.bin", Some(10), &dest, 4).unwrap();
    assert_eq!(written, 10);
    assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789");
    assert_eq!(drive.ranges.load(Ordering::SeqCst), 3);
    assert_eq!(drive.gets.load(Ordering::SeqCst), 0);
}

#[test]
fn an_unknown_size_is_asked_for_first() {
    let tmp = TempDir::new("transfer-head");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("a.txt", b"abc").unwrap();
    let dest = tmp.path().join("a.txt");
    download_to_file(&drive, "a.txt", None, &dest, 1024).unwrap();
    assert_eq!(drive.heads.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), b"abc");
}

#[test]
fn a_failed_download_leaves_no_partial_file() {
    let tmp = TempDir::new("transfer-fail");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    let dest = tmp.path().join("missing.txt");
    assert!(download_to_file(&drive, "missing.txt", Some(3), &dest, 1024).is_err());
    assert!(!dest.exists());
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("missing"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_download_does_not_overwrite_an_existing_file() {
    let tmp = TempDir::new("transfer-name");
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001.eml"))
    );
    std::fs::write(tmp.path().join("0001.eml"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001 (1).eml"))
    );
    std::fs::write(tmp.path().join("0001 (1).eml"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001 (2).eml"))
    );
    std::fs::write(tmp.path().join("README"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "README"),
        Some(tmp.path().join("README (1)"))
    );
    assert_eq!(download_path(tmp.path(), "mail/.."), None);
}

#[test]
fn upload_file_puts_the_local_bytes_under_the_key() {
    let tmp = TempDir::new("transfer-upload");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    let source = tmp.path().join("notes.txt");
    std::fs::write(&source, b"notes").unwrap();
    assert_eq!(upload_file(&drive, &source, "docs/notes.txt").unwrap(), 5);
    assert_eq!(drive.get("docs/notes.txt").unwrap(), b"notes");
}
