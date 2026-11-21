//! A transfer that is told to stop, and a read that names its limit (ARCH28 RF48 step 1):
//! [`crate::Drive::put_file`] and the copy loops ask a [`Ctl`] before every part or chunk and end
//! with [`DriveError::Cancelled`] and nothing made visible; [`crate::Drive::get_limited`] gives
//! the limit to the transport and every wrapper forwards it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    ctl::Quiet,
    transfer::{copy_object_with_ctl, download_with_ctl, ProgressReader},
    Credentials, Ctl, Drive, DriveError, HttpCall, HttpReply, Limit, LocalDrive, Method, S3Config,
    S3Drive, ScopedDrive, Transport,
};

/// The tests' part size (a real bucket wants 5 MiB and more).
const PART: usize = 1024;

/// Says stop once `after` bytes were heard (`0`: from the start).
struct StopAfter {
    after: u64,
    heard: AtomicU64,
}

impl StopAfter {
    fn new(after: u64) -> StopAfter {
        StopAfter {
            after,
            heard: AtomicU64::new(0),
        }
    }
}

impl Ctl for StopAfter {
    fn bytes(&self, done: u64) {
        self.heard.fetch_max(done, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.heard.load(Ordering::SeqCst) >= self.after
    }
}

/// Passes every request to the fake and keeps what the GETs among them may be answered with.
struct Spy {
    inner: Box<dyn Transport>,
    answers: Arc<Mutex<Vec<u64>>>,
}

impl Transport for Spy {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        if call.method == Method::Get {
            self.answers.lock().unwrap().push(call.budget().max_answer);
        }
        self.inner.send(call)
    }
}

fn config() -> S3Config {
    S3Config {
        endpoint: String::from("http://127.0.0.1:9000"),
        region: String::from("us-east-1"),
        bucket: BUCKET.to_string(),
        path_style: true,
    }
}

fn drive_on(s3: &Arc<FakeBucket>, resume: Option<&Path>) -> S3Drive {
    let drive = S3Drive::new(
        config(),
        Credentials::new("AKIDTEST", "test-secret"),
        s3.transport(),
    )
    .unwrap()
    .with_clock(|| 1_791_590_400)
    .with_part_size(PART);
    match resume {
        Some(dir) => drive.with_resume_dir(dir),
        None => drive,
    }
}

/// A drive whose GETs are recorded: the `max_answer` of each.
fn spied(s3: &Arc<FakeBucket>) -> (S3Drive, Arc<Mutex<Vec<u64>>>) {
    let answers = Arc::new(Mutex::new(Vec::new()));
    let spy = Spy {
        inner: s3.transport(),
        answers: answers.clone(),
    };
    let drive = S3Drive::new(
        config(),
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(spy),
    )
    .unwrap()
    .with_clock(|| 1_791_590_400);
    (drive, answers)
}

/// `parts` parts and a bit, a different byte pattern for every `seed`.
fn file_with(dir: &TempDir, name: &str, parts: usize, seed: u8) -> (PathBuf, Vec<u8>) {
    let bytes: Vec<u8> = (0..parts * PART + 7)
        .map(|i| (i % 251) as u8 ^ seed)
        .collect();
    let path = dir.path().join(name);
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

fn state_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn rf48_a_stopped_file_upload_aborts_the_multipart_upload_and_forgets_its_state() {
    let tmp = TempDir::new("rf48-stopped-upload");
    let resume = tmp.path().join("resume");
    let (path, _) = file_with(&tmp, "big.bin", 6, 3);
    let s3 = FakeBucket::new();
    s3.slow_parts(Duration::from_millis(30));
    // Stops once the first part is up: the parts in flight finish, nothing is completed.
    let ctl = StopAfter::new(PART as u64);
    let got = drive_on(&s3, Some(&resume)).put_file("big.bin", &path, &ctl);
    assert_eq!(got, Err(DriveError::Cancelled), "{:?}", s3.log());
    assert!(s3.object("big.bin").is_none(), "no object: {:?}", s3.log());
    assert_eq!(s3.count("POST complete"), 0, "{:?}", s3.log());
    assert_eq!(s3.count("DELETE abort"), 1, "{:?}", s3.log());
    assert!(s3.uploads().is_empty(), "the service holds no upload");
    assert!(
        state_files(&resume).is_empty(),
        "a stop forgets the state a failure would keep: {:?}",
        state_files(&resume)
    );
}

#[test]
fn rf48_a_failed_file_upload_still_keeps_its_state_for_the_next_try() {
    let tmp = TempDir::new("rf48-failed-upload");
    let resume = tmp.path().join("resume");
    let (path, _) = file_with(&tmp, "big.bin", 6, 4);
    let s3 = FakeBucket::new();
    // The connection is lost after two parts: no stop was asked, so the upload is kept.
    s3.take_parts(Some(2));
    let got = drive_on(&s3, Some(&resume)).put_file("big.bin", &path, &Quiet);
    assert!(
        got.is_err() && got != Err(DriveError::Cancelled),
        "a lost connection is no cancel: {got:?}"
    );
    assert_eq!(state_files(&resume).len(), 1, "{:?}", s3.log());
    assert_eq!(s3.count("DELETE abort"), 0, "{:?}", s3.log());
}

#[test]
fn rf48_a_ctl_that_is_stopped_before_the_upload_starts_sends_nothing() {
    let tmp = TempDir::new("rf48-stopped-before");
    let (path, _) = file_with(&tmp, "big.bin", 3, 5);
    let s3 = FakeBucket::new();
    let got = drive_on(&s3, None).put_file("big.bin", &path, &StopAfter::new(0));
    assert_eq!(got, Err(DriveError::Cancelled));
    assert!(s3.log().is_empty(), "no request: {:?}", s3.log());
}

#[test]
fn rf48_a_stopped_streamed_upload_is_cancelled_not_an_io_error_and_is_aborted() {
    let s3 = FakeBucket::new();
    let body = vec![7u8; 6 * PART + 3];
    // Two parts are read, then the stop: the third read fails with the stop.
    let ctl = StopAfter::new(2 * PART as u64);
    let mut source = &body[..];
    let mut reader = ProgressReader::new(&mut source, &ctl);
    let got = drive_on(&s3, None).put_from("streamed.bin", &mut reader);
    assert_eq!(got, Err(DriveError::Cancelled), "{:?}", s3.log());
    assert!(s3.object("streamed.bin").is_none());
    assert_eq!(s3.count("POST complete"), 0, "{:?}", s3.log());
    assert_eq!(s3.count("DELETE abort"), 1, "{:?}", s3.log());
    assert!(s3.uploads().is_empty(), "the service holds no upload");
}

#[test]
fn rf48_a_stopped_stream_into_a_folder_leaves_no_file_and_is_cancelled() {
    let tmp = TempDir::new("rf48-stopped-stream-local");
    let drive = LocalDrive::without_manifest(tmp.path().join("drive"));
    drive.put("kept.txt", b"before").unwrap();
    let body = vec![1u8; 1024 * 1024];
    let ctl = StopAfter::new(1);
    let mut source = &body[..];
    let mut reader = ProgressReader::new(&mut source, &ctl);
    let got = drive.put_from("kept.txt", &mut reader);
    assert_eq!(got, Err(DriveError::Cancelled));
    assert_eq!(drive.get("kept.txt").unwrap(), b"before");
    let names: Vec<OsString> = std::fs::read_dir(tmp.path().join("drive"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![OsString::from("kept.txt")], "no temporary file");
}

#[test]
fn rf48_a_stopped_copy_on_disk_leaves_the_target_as_it_was_and_no_temp_file() {
    let tmp = TempDir::new("rf48-stopped-copy");
    let source = LocalDrive::without_manifest(tmp.path().join("from"));
    let target = LocalDrive::without_manifest(tmp.path().join("to"));
    source.put("big.bin", &vec![9u8; 1024 * 1024]).unwrap();
    target.put("big.bin", b"the old one").unwrap();
    // Stops after the first chunk.
    let ctl = StopAfter::new(1);
    let got = copy_object_with_ctl(&source, "big.bin", None, &target, "big.bin", &ctl);
    assert_eq!(got, Err(DriveError::Cancelled));
    assert_eq!(target.get("big.bin").unwrap(), b"the old one");
    let names: Vec<OsString> = std::fs::read_dir(tmp.path().join("to"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![OsString::from("big.bin")], "no temporary file");
}

#[test]
fn rf48_a_stopped_ranged_download_publishes_nothing_and_the_next_one_resumes() {
    let tmp = TempDir::new("rf48-stopped-download");
    let s3 = FakeBucket::new();
    let bytes: Vec<u8> = (0..10 * 1024).map(|i| (i % 251) as u8).collect();
    s3.write("big.bin", &bytes);
    let drive = drive_on(&s3, None);
    let dest = tmp.path().join("out").join("big.bin");
    let size = bytes.len() as u64;
    let got = download_with_ctl(
        &drive,
        "big.bin",
        Some(size),
        &dest,
        1024,
        &StopAfter::new(1),
    );
    assert_eq!(got, Err(DriveError::Cancelled));
    assert!(!dest.exists(), "nothing is put in place");
    let again = download_with_ctl(&drive, "big.bin", Some(size), &dest, 1024, &Quiet).unwrap();
    assert_eq!(again, size);
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
}

#[test]
fn rf48_get_limited_refuses_a_bigger_file_of_a_folder_and_names_the_limit() {
    let tmp = TempDir::new("rf48-get-limited-local");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("k.bin", &[1u8; 100]).unwrap();
    let err = drive
        .get_limited("k.bin", Limit::new("a test file", 10))
        .unwrap_err();
    assert!(
        matches!(&err, DriveError::Protocol(why) if why.contains("a test file") && why.contains("k.bin")),
        "{err:?}"
    );
    let whole = drive
        .get_limited("k.bin", Limit::new("a test file", 100))
        .unwrap();
    assert_eq!(whole.len(), 100);
    let gone = drive
        .get_limited("missing.bin", Limit::new("a test file", 100))
        .unwrap_err();
    assert!(matches!(gone, DriveError::NotFound { .. }), "{gone:?}");
}

#[test]
fn rf48_an_s3_get_limited_gives_the_limit_to_the_transport_and_every_wrapper_forwards_it() {
    let s3 = FakeBucket::new();
    s3.write("docs/k.bin", &[5u8; 100]);
    let (drive, seen) = spied(&s3);
    let limit = Limit::new("a test file", 4096);
    assert_eq!(drive.get_limited("docs/k.bin", limit).unwrap().len(), 100);
    assert_eq!(*seen.lock().unwrap(), vec![4096]);
    // An object past the limit is refused, whatever the transport did.
    let tight = Limit::new("a test file", 99);
    let err = drive.get_limited("docs/k.bin", tight).unwrap_err();
    assert!(matches!(err, DriveError::Protocol(_)), "{err:?}");
    // A grant's view, a box, a shared pointer: the same limit reaches the transport.
    seen.lock().unwrap().clear();
    let scoped = ScopedDrive::new(drive, "docs/", false).unwrap();
    assert_eq!(scoped.get_limited("k.bin", limit).unwrap().len(), 100);
    let boxed: Box<dyn Drive> = Box::new(scoped);
    assert_eq!(boxed.get_limited("k.bin", limit).unwrap().len(), 100);
    let shared = Arc::new(boxed);
    assert_eq!(shared.get_limited("k.bin", limit).unwrap().len(), 100);
    assert_eq!(*seen.lock().unwrap(), vec![4096, 4096, 4096]);
}

#[test]
fn rf48_a_cancel_is_a_drive_error_that_reads_as_a_sentence_and_never_as_a_failure() {
    let text = DriveError::Cancelled.to_string();
    assert!(text.contains("stopped"), "{text}");
    // An io error that carries the stop becomes the cancel wherever `?` meets it.
    let from_io: DriveError = crate::ctl::Stopped::io().into();
    assert_eq!(from_io, DriveError::Cancelled);
    let other: DriveError = std::io::Error::other("the disk failed").into();
    assert!(matches!(other, DriveError::Io(_)), "{other:?}");
}

/// A stop reaches an encrypted write through the AZL1 stream: the read that was stopped is
/// `CryptoError::Stopped` inside the crypto layer and `Cancelled` out of the drive - not "file
/// error: ...: stopped" - and no object and no name is made.
#[cfg(feature = "encryption")]
#[test]
fn rf48_a_stopped_encrypted_upload_is_cancelled_not_an_io_failure_and_makes_no_name() {
    use super::mem_bucket::MemBucket;
    use crate::{
        crypto::DriveKey,
        encrypted::{EncryptedDrive, MemoryIndex},
    };
    let drive = EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    let body = vec![7u8; 3 << 20];
    let ctl = StopAfter::new(1);
    let mut source = &body[..];
    let mut reader = ProgressReader::new(&mut source, &ctl);
    let got = drive.put_from("film.bin", &mut reader);
    assert_eq!(got, Err(DriveError::Cancelled));
    let named = drive.head("film.bin");
    assert!(
        matches!(named, Err(DriveError::NotFound { .. })),
        "{named:?}"
    );
}
