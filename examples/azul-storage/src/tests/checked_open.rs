//! A checked upload opens its file once - never through a link as its last name, never
//! blocking on a FIFO, and only the file the scan saw (its device and inode) - and reads
//! nothing else: a link, a FIFO or a copy swapped in under its name, before the upload or
//! between a cut-off and the resumed upload, sends no byte of what it points at.

use std::{
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::mpsc,
    time::{Duration, SystemTime},
};

use super::{
    checked_upload::{drive_on, file_with, no_progress, scanned},
    fake_bucket::FakeBucket,
    TempDir,
};
use crate::{
    checked::{open_checked, CheckedUpload, FileCheck},
    Drive, LocalDrive,
};

/// What a scan found of `path` with the bytes `named`, with its device and inode.
fn identified(path: &Path, named: &[u8]) -> FileCheck {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(path).unwrap();
    FileCheck {
        dev: meta.dev(),
        ino: meta.ino(),
        ..scanned(path, named)
    }
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path).unwrap().modified().unwrap()
}

/// Gives `path` the modification time `at`: a file swapped in that looks unchanged.
fn set_time(path: &Path, at: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

fn mkfifo(path: &Path) {
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: a valid C string.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
}

#[test]
fn a_checked_open_takes_the_scanned_file_and_never_a_link_a_copy_or_a_fifo() {
    let tmp = TempDir::new("checked-open");
    let (path, bytes) = file_with(&tmp, "video.bin", 3, 1);
    let check = identified(&path, &bytes);
    let at = modified(&path);
    assert!(open_checked(&path, &check).unwrap().is_some());
    // The scanned file moves away (its inode stays taken); a twin of the same bytes and time
    // comes in under its name, as a link, then as a copy, then a FIFO does.
    fs::rename(&path, tmp.path().join("moved.bin")).unwrap();
    let twin = tmp.path().join("twin.bin");
    fs::write(&twin, &bytes).unwrap();
    set_time(&twin, at);
    std::os::unix::fs::symlink(&twin, &path).unwrap();
    assert!(
        open_checked(&path, &check).unwrap().is_none(),
        "a link is never opened"
    );
    fs::remove_file(&path).unwrap();
    fs::copy(&twin, &path).unwrap();
    set_time(&path, at);
    assert!(
        open_checked(&path, &check).unwrap().is_none(),
        "another file with the same bytes and time"
    );
    fs::remove_file(&path).unwrap();
    mkfifo(&path);
    assert!(
        open_checked(&path, &check).unwrap().is_none(),
        "a FIFO, without waiting for a writer"
    );
}

#[test]
fn a_link_swapped_in_before_a_resumed_upload_sends_none_of_its_bytes() {
    let tmp = TempDir::new("checked-swap");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "video.bin", 10, 5);
    let check = identified(&path, &bytes);
    let at = modified(&path);
    let s3 = FakeBucket::new();
    // The connection drops after four parts.
    s3.take_parts(Some(4));
    let cut = drive_on(&s3, &resume).put_file_checked("video.bin", &path, &check, &no_progress);
    assert!(cut.is_err(), "{cut:?}");
    // A secret of the same size and time takes the file's name as a link.
    let secret = tmp.path().join("secret.bin");
    fs::write(&secret, vec![0x5a; bytes.len()]).unwrap();
    set_time(&secret, at);
    fs::rename(&path, tmp.path().join("moved.bin")).unwrap();
    std::os::unix::fs::symlink(&secret, &path).unwrap();
    s3.take_parts(None);
    s3.clear_log();
    let answer = drive_on(&s3, &resume)
        .put_file_checked("video.bin", &path, &check, &no_progress)
        .unwrap();
    assert_eq!(answer, CheckedUpload::Changed);
    assert_eq!(
        s3.count("PUT part"),
        0,
        "none of the secret: {:?}",
        s3.log()
    );
    assert!(s3.object("video.bin").is_none());
}

#[test]
fn a_fifo_swapped_in_never_blocks_a_checked_upload() {
    let tmp = TempDir::new("checked-fifo");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "video.bin", 10, 6);
    let check = identified(&path, &bytes);
    fs::rename(&path, tmp.path().join("moved.bin")).unwrap();
    mkfifo(&path);
    let s3 = FakeBucket::new();
    let bucket = s3.clone();
    let (tell, heard) = mpsc::channel();
    // Its own thread: an open that waits for a writer would hold this test for ever.
    std::thread::spawn(move || {
        let drive = drive_on(&bucket, &resume);
        let _ = tell.send(drive.put_file_checked("video.bin", &path, &check, &no_progress));
    });
    let answer = heard
        .recv_timeout(Duration::from_secs(10))
        .expect("the upload returned instead of waiting at the FIFO");
    assert_eq!(answer.unwrap(), CheckedUpload::Changed);
    assert_eq!(s3.count("POST start"), 0, "{:?}", s3.log());
}

#[test]
fn a_drive_without_parts_uploads_no_link_swapped_in_even_with_the_named_bytes() {
    let tmp = TempDir::new("checked-open-local");
    std::fs::create_dir_all(tmp.path().join("drive")).unwrap();
    let drive = LocalDrive::new(tmp.path().join("drive"));
    let path = tmp.path().join("a.txt");
    fs::write(&path, b"the file").unwrap();
    let check = identified(&path, b"the file");
    let at = modified(&path);
    let twin = tmp.path().join("twin.txt");
    fs::write(&twin, b"the file").unwrap();
    set_time(&twin, at);
    fs::rename(&path, tmp.path().join("moved.txt")).unwrap();
    std::os::unix::fs::symlink(&twin, &path).unwrap();
    assert_eq!(
        drive
            .put_file_checked("a.txt", &path, &check, &no_progress)
            .unwrap(),
        CheckedUpload::Changed
    );
    assert!(drive.get("a.txt").is_err(), "nothing written");
}
