//! A file's upload that must hold the bytes a scan named ([`Drive::put_file_checked`]): a sync
//! names a blob by the BLAKE3 of its file, so an upload of a file that changed since - before,
//! while its parts go up, or between a dropped connection and the resumed upload - must never
//! complete; and a big file cut off half way resumes with the parts the bucket holds instead of
//! starting again from its first part.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    checked::{CheckedUpload, FileCheck},
    Credentials, Drive, DriveError, LocalDrive, S3Config, S3Drive,
};

/// 2026-10-10T00:00:00Z.
const NOW: u64 = 1_791_590_400;
const PART: usize = 1024;

pub(super) fn drive_on(s3: &Arc<FakeBucket>, resume: &Path) -> S3Drive {
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
    .with_clock(|| NOW)
    .with_part_size(PART)
    .with_resume_dir(resume)
}

pub(super) fn file_with(dir: &TempDir, name: &str, parts: usize, seed: u8) -> (PathBuf, Vec<u8>) {
    let bytes: Vec<u8> = (0..parts * PART + 7)
        .map(|i| (i % 251) as u8 ^ seed)
        .collect();
    let path = dir.path().join(name);
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

/// What a scan found of `path` with the bytes `named` (their BLAKE3).
pub(super) fn scanned(path: &Path, named: &[u8]) -> FileCheck {
    let meta = std::fs::metadata(path).unwrap();
    let modified_ns = meta
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    FileCheck {
        size: meta.len(),
        modified_ns,
        blake3: blake3::hash(named).to_hex().to_string(),
    }
}

pub(super) fn no_progress(_: u64) {}

#[test]
fn a_checked_upload_cut_off_resumes_with_the_missing_parts_and_completes_once() {
    let tmp = TempDir::new("checked-resume");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "video.bin", 10, 1);
    let check = scanned(&path, &bytes);
    let s3 = FakeBucket::new();
    // The connection drops after four parts.
    s3.take_parts(Some(4));
    let cut = drive_on(&s3, &resume).put_file_checked("video.bin", &path, &check, &no_progress);
    assert!(matches!(cut, Err(DriveError::Transport(_))), "{cut:?}");
    assert_eq!(s3.uploads().len(), 1, "kept for the next try, not aborted");
    s3.take_parts(None);
    s3.clear_log();
    let sent = drive_on(&s3, &resume)
        .put_file_checked("video.bin", &path, &check, &no_progress)
        .unwrap();
    assert_eq!(sent, CheckedUpload::Sent(bytes.len() as u64));
    assert_eq!(s3.object("video.bin").unwrap(), bytes);
    assert_eq!(s3.count("POST start"), 0, "the same upload: {:?}", s3.log());
    assert_eq!(s3.count("PUT part"), 7, "only the parts it lacked");
    assert_eq!(s3.count("POST complete"), 1);
    assert!(s3.uploads().is_empty());
}

#[test]
fn a_file_that_changed_since_its_scan_is_not_uploaded() {
    let tmp = TempDir::new("checked-changed-before");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "video.bin", 10, 2);
    let check = scanned(&path, &bytes);
    // Written again after the scan: other bytes, another time.
    std::thread::sleep(Duration::from_millis(20));
    let (_, edited) = file_with(&tmp, "video.bin", 10, 3);
    let s3 = FakeBucket::new();
    let answer = drive_on(&s3, &resume)
        .put_file_checked("video.bin", &path, &check, &no_progress)
        .unwrap();
    assert_eq!(answer, CheckedUpload::Changed);
    assert!(s3.object("video.bin").is_none());
    assert_eq!(s3.count("POST start"), 0, "nothing sent: {:?}", s3.log());
    // As the next scan sees it, it goes up.
    let again = drive_on(&s3, &resume)
        .put_file_checked("video.bin", &path, &scanned(&path, &edited), &no_progress)
        .unwrap();
    assert_eq!(again, CheckedUpload::Sent(edited.len() as u64));
    assert_eq!(s3.object("video.bin").unwrap(), edited);
}

#[test]
fn an_upload_whose_bytes_are_not_the_named_ones_is_never_completed() {
    // The size and the time say "unchanged", the bytes do not (two writes in one clock tick):
    // the whole file is hashed again before the upload completes.
    let tmp = TempDir::new("checked-other-bytes");
    let resume = tmp.path().join("resume");
    let (path, _) = file_with(&tmp, "video.bin", 10, 4);
    let mut check = scanned(&path, b"the bytes the scan saw");
    check.size = std::fs::metadata(&path).unwrap().len();
    let s3 = FakeBucket::new();
    let answer = drive_on(&s3, &resume)
        .put_file_checked("video.bin", &path, &check, &no_progress)
        .unwrap();
    assert_eq!(answer, CheckedUpload::Changed);
    assert!(
        s3.object("video.bin").is_none(),
        "no object under a name its bytes are not"
    );
    assert_eq!(s3.count("POST complete"), 0, "{:?}", s3.log());
    assert!(s3.uploads().is_empty(), "the parts are aborted");
    // One part or less: the same, in one PUT that is never sent.
    let small = tmp.path().join("small.bin");
    std::fs::write(&small, b"small").unwrap();
    let check = scanned(&small, b"other");
    let answer = drive_on(&s3, &resume)
        .put_file_checked("small.bin", &small, &check, &no_progress)
        .unwrap();
    assert_eq!(answer, CheckedUpload::Changed);
    assert!(s3.object("small.bin").is_none());
}

#[test]
fn a_drive_without_parts_streams_the_file_and_refuses_other_bytes() {
    let tmp = TempDir::new("checked-local");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    std::fs::create_dir_all(tmp.path().join("drive")).unwrap();
    let path = tmp.path().join("a.txt");
    std::fs::write(&path, b"the file").unwrap();
    let wrong = scanned(&path, b"not the file");
    assert_eq!(
        drive
            .put_file_checked("a.txt", &path, &wrong, &no_progress)
            .unwrap(),
        CheckedUpload::Changed
    );
    assert!(drive.get("a.txt").is_err(), "nothing written");
    let right = scanned(&path, b"the file");
    assert_eq!(
        drive
            .put_file_checked("a.txt", &path, &right, &no_progress)
            .unwrap(),
        CheckedUpload::Sent(8)
    );
    assert_eq!(drive.get("a.txt").unwrap(), b"the file");
}
