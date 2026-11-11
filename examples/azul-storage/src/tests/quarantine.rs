//! The system's "downloaded from elsewhere" mark on what AzDrive brings from a drive to this disk
//! (AZ-W20): the transfer engine marks a file that lands on this disk from a drive that is not a
//! folder of it, and a copy from one folder of this disk to another is no download.

use super::TempDir;
use crate::{
    ctl::Quiet,
    durable::Expect,
    quarantine, transfer,
    xfer::{plan, run, Item},
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
};

/// A drive that is not a folder of this computer (a bucket, in effect): it answers from a folder
/// but has no path of its own for a key.
struct Remote(LocalDrive);

impl Drive for Remote {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.0.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.0.head(key)
    }
}

fn file(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

/// AZ-W20: through `xfer::run`, the function a Download / Copy / Paste calls. A file that comes
/// from a drive is marked; a copy from one folder of this disk to another adds no mark - it is no
/// download - and keeps the one the file had (X-DELTA-AZDRIVE-FILES-16 carries a file's extended
/// attributes to its copy, the quarantine mark among them, as the system's own copy does): a copy
/// that dropped the mark would be a way round the system's second look at what came from elsewhere
/// (asserted on macOS, where the mark is an attribute the copy carries).
#[test]
fn az_w20_a_file_a_transfer_brings_from_a_drive_is_marked_and_a_copy_on_this_disk_adds_no_mark_and_keeps_the_one_it_had(
) {
    let tmp = TempDir::new("quarantine-transfer");
    let cloud = Remote(LocalDrive::without_manifest(tmp.path().join("cloud")));
    cloud
        .put("tools/setup.command", b"#!/bin/sh\necho hi\n")
        .unwrap();
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let copy = LocalDrive::without_manifest(tmp.path().join("copy"));
    assert!(
        cloud.local_path("tools/setup.command").is_none(),
        "given: a drive that is no folder of this computer"
    );

    // From the drive to this disk: the file carries the mark where the system has one.
    let item = [file("tools/setup.command")];
    let downloaded = plan(&cloud, &item, &home, "", false, "Copy").unwrap();
    let report = run(&downloaded, &cloud, &home, false, &Quiet, &mut |_| {});
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let landed = tmp.path().join("home").join("setup.command");
    assert!(landed.is_file(), "{landed:?}");
    assert_eq!(quarantine::is_marked(&landed), quarantine::supported());

    // From one folder of this disk to another: nobody downloaded it, so a file that was never
    // marked has a copy that is not marked either.
    std::fs::write(tmp.path().join("home").join("notes.txt"), b"mine").unwrap();
    assert!(!quarantine::is_marked(
        &tmp.path().join("home").join("notes.txt")
    ));
    let item = [file("notes.txt")];
    let copied = plan(&home, &item, &copy, "", false, "Copy").unwrap();
    let report = run(&copied, &home, &copy, false, &Quiet, &mut |_| {});
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let unmarked = tmp.path().join("copy").join("notes.txt");
    assert!(unmarked.is_file(), "{unmarked:?}");
    assert!(!quarantine::is_marked(&unmarked), "the copy added no mark");

    // The file that WAS marked keeps its mark in the copy: where it came from does not change
    // with the folder it is copied into.
    let item = [file("setup.command")];
    let copied = plan(&home, &item, &copy, "", false, "Copy").unwrap();
    let report = run(&copied, &home, &copy, false, &Quiet, &mut |_| {});
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let second = tmp.path().join("copy").join("setup.command");
    assert!(second.is_file(), "{second:?}");
    // On macOS the mark is an extended attribute, which `xattrs::carry` puts on the copy. Windows'
    // stream is not one of the attributes it carries (xattrs.rs says so), and what the copy keeps
    // there is not asserted here.
    if cfg!(target_os = "macos") {
        assert!(
            quarantine::is_marked(&second),
            "the copy of a marked file is marked"
        );
    }
}

/// The mark is the same value the sync sets, and `mark_downloaded` swallows what a disk without
/// marks refuses: the file is there.
#[test]
fn az_w20_mark_downloaded_marks_where_there_is_a_mark_and_never_fails_a_download() {
    let tmp = TempDir::new("quarantine-mark");
    let path = tmp.path().join("a.js");
    std::fs::write(&path, b"1").unwrap();
    assert!(!quarantine::is_marked(&path));
    quarantine::mark_downloaded(&path);
    assert_eq!(quarantine::is_marked(&path), quarantine::supported());
    // A file that is not there is no failure either (a download that went away meanwhile).
    quarantine::mark_downloaded(&tmp.path().join("gone.js"));
}

/// X-DELTA-AZDRIVE-FILES-19: the mark was put on by the transfer engine's `copy_one`, one caller of
/// the copy; the rescue copy of a read-only drive calls `copy_object_guarded` itself and a shared
/// drive's `setup.exe` landed in `Azlin rescue/` unmarked. The copy marks, whoever calls it.
#[test]
fn x_delta_azdrive_files_19_copy_object_marks_a_file_it_brings_from_a_drive() {
    let tmp = TempDir::new("quarantine-copy-object");
    let cloud = Remote(LocalDrive::without_manifest(tmp.path().join("cloud")));
    cloud
        .put("tools/setup.exe", b"MZ not really a program")
        .unwrap();
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let len = transfer::copy_object_guarded(
        &cloud,
        "tools/setup.exe",
        None,
        &home,
        "tools/setup.exe",
        Expect::Absent,
        &Quiet,
    )
    .unwrap();
    assert_eq!(len, 23);
    let landed = tmp.path().join("home").join("tools").join("setup.exe");
    assert!(landed.is_file());
    assert_eq!(quarantine::is_marked(&landed), quarantine::supported());

    // From one folder of this disk to another: nobody downloaded it.
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    transfer::copy_object_guarded(
        &home,
        "tools/setup.exe",
        None,
        &other,
        "setup.exe",
        Expect::Absent,
        &Quiet,
    )
    .unwrap();
    assert!(!quarantine::is_marked(
        &tmp.path().join("other").join("setup.exe")
    ));
}
