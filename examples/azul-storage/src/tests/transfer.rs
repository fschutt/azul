use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    transfer::{copy_object, download_path, download_to_file, upload_file},
    ByteRange, Credentials, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
    S3Config, S3Drive,
};

/// A local drive that counts the reads and the streamed and file writes (it is no folder on
/// this computer to a copy: `local_path` is the trait's `None`).
struct Counting {
    inner: LocalDrive,
    gets: AtomicU32,
    ranges: AtomicU32,
    heads: AtomicU32,
    put_froms: AtomicU32,
    put_files: Mutex<Vec<(String, PathBuf)>>,
}

impl Counting {
    fn new(inner: LocalDrive) -> Self {
        Counting {
            inner,
            gets: AtomicU32::new(0),
            ranges: AtomicU32::new(0),
            heads: AtomicU32::new(0),
            put_froms: AtomicU32::new(0),
            put_files: Mutex::new(Vec::new()),
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
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.put_froms.fetch_add(1, Ordering::SeqCst);
        self.inner.put_from(key, body)
    }
    fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        self.put_files
            .lock()
            .unwrap()
            .push((key.to_string(), path.to_path_buf()));
        self.inner.put_file(key, path, progress)
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

/// A bucket on the fake service (its ETags tell a download whether it may resume).
fn bucket_on(s3: &Arc<FakeBucket>) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        s3.transport(),
    )
    .unwrap()
}

/// The files of `dir` whose name contains `part`.
fn files_named(dir: &Path, part: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(part))
        .collect()
}

/// 11 chunks of 1 KiB (the last one short).
fn eleven_chunks(seed: u8) -> Vec<u8> {
    (0..10 * 1024 + 5).map(|i| (i % 241) as u8 ^ seed).collect()
}

#[test]
fn a_big_download_fetches_several_ranges_at_once() {
    let tmp = TempDir::new("transfer-parallel");
    let s3 = FakeBucket::new();
    let bytes = eleven_chunks(1);
    s3.write("big.bin", &bytes);
    s3.slow_ranges(Duration::from_millis(40));
    let dest = tmp.path().join("big.bin");
    let written = download_to_file(
        &bucket_on(&s3),
        "big.bin",
        Some(bytes.len() as u64),
        &dest,
        1024,
    )
    .unwrap();
    assert_eq!(written, bytes.len() as u64);
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    assert_eq!(s3.count("GET object"), 11, "{:?}", s3.log());
    let most = s3.most_ranges_at_once();
    assert!(most > 1 && most <= 4, "{most} ranges at once");
    assert_eq!(
        files_named(tmp.path(), "big.bin"),
        vec![String::from("big.bin")]
    );
}

#[test]
fn a_download_that_failed_half_way_resumes_with_the_ranges_it_still_lacks() {
    let tmp = TempDir::new("transfer-resume");
    let s3 = FakeBucket::new();
    let bytes = eleven_chunks(2);
    s3.write("big.bin", &bytes);
    s3.take_ranges(Some(4));
    let dest = tmp.path().join("big.bin");
    let size = Some(bytes.len() as u64);
    assert!(download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).is_err());
    assert!(!dest.exists(), "nothing half-done under the file's name");
    s3.take_ranges(None);
    s3.clear_log();
    download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    assert_eq!(
        s3.count("GET object"),
        7,
        "only the 7 missing ranges: {:?}",
        s3.log()
    );
    assert_eq!(
        files_named(tmp.path(), "big.bin"),
        vec![String::from("big.bin")],
        "the kept part and its state are gone"
    );
}

#[test]
fn a_download_whose_object_changed_meanwhile_starts_over() {
    let tmp = TempDir::new("transfer-changed");
    let s3 = FakeBucket::new();
    s3.write("big.bin", &eleven_chunks(3));
    s3.take_ranges(Some(4));
    let dest = tmp.path().join("big.bin");
    let size = Some(eleven_chunks(3).len() as u64);
    assert!(download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).is_err());
    let newer = eleven_chunks(4);
    s3.write("big.bin", &newer);
    s3.take_ranges(None);
    s3.clear_log();
    download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        newer,
        "never parts of both versions"
    );
    assert_eq!(s3.count("GET object"), 11);
}

#[test]
fn upload_file_sends_the_file_through_put_file() {
    let tmp = TempDir::new("transfer-put-file");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    let source = tmp.path().join("notes.txt");
    std::fs::write(&source, b"notes").unwrap();
    assert_eq!(upload_file(&drive, &source, "docs/notes.txt").unwrap(), 5);
    assert_eq!(
        *drive.put_files.lock().unwrap(),
        vec![(String::from("docs/notes.txt"), source.clone())]
    );
    assert_eq!(drive.get("docs/notes.txt").unwrap(), b"notes");
}

#[test]
fn copying_a_local_file_into_another_drive_sends_it_through_put_file() {
    let tmp = TempDir::new("transfer-copy-up");
    let source = LocalDrive::new(tmp.path().join("here"));
    source.put("docs/a.txt", b"alpha").unwrap();
    let target = Counting::new(LocalDrive::new(tmp.path().join("there")));
    let mut seen = Vec::new();
    let copied = copy_object(
        &source,
        "docs/a.txt",
        Some(5),
        &target,
        "in/a.txt",
        &mut |n| seen.push(n),
    )
    .unwrap();
    assert_eq!(copied, 5);
    let sent = target.put_files.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "the file itself, not its bytes in memory");
    assert_eq!(sent[0].0, "in/a.txt");
    assert_eq!(target.get("in/a.txt").unwrap(), b"alpha");
    assert_eq!(seen.last(), Some(&5));
}

#[test]
fn copying_a_big_object_between_two_remote_drives_streams_it_in_ranges() {
    let tmp = TempDir::new("transfer-copy-across");
    let source = Counting::new(LocalDrive::new(tmp.path().join("a")));
    let big: Vec<u8> = (0..crate::transfer::CHUNK as usize + 10)
        .map(|i| (i % 253) as u8)
        .collect();
    source.inner.put("big.bin", &big).unwrap();
    let target = Counting::new(LocalDrive::new(tmp.path().join("b")));
    let mut seen = Vec::new();
    copy_object(
        &source,
        "big.bin",
        Some(big.len() as u64),
        &target,
        "copy.bin",
        &mut |n| seen.push(n),
    )
    .unwrap();
    assert_eq!(target.inner.get("copy.bin").unwrap(), big);
    assert_eq!(
        source.gets.load(Ordering::SeqCst),
        0,
        "never the whole object at once"
    );
    assert_eq!(source.ranges.load(Ordering::SeqCst), 2);
    assert_eq!(
        target.put_froms.load(Ordering::SeqCst),
        1,
        "one streamed write"
    );
    assert_eq!(seen.last().copied(), Some(big.len() as u64));
}
