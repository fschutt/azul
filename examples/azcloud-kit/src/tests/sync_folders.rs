//! Folders as entries of the drive's index: an empty folder is made, deleted and renamed on
//! every device like a file, and a folder emptied by deleting its files stays a folder.

use std::fs;

use super::{fake_s3::S3Bucket, sync::Device};

fn mkdir(device: &Device, key: &str) {
    fs::create_dir_all(device.path().join(key)).unwrap();
}

fn is_dir(device: &Device, key: &str) -> bool {
    device.path().join(key).is_dir()
}

#[test]
fn an_empty_folder_made_on_one_device_is_made_on_the_other() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-mkdir");
    let b = Device::new("dev-b-mkdir");
    a.write("notes/a.md", b"alpha");
    mkdir(&a, "photos/2026");
    a.sync(&store);
    b.sync(&store);
    assert!(is_dir(&b, "photos/2026"), "{:?}", super::sync::index_of(&store).files);
    let again = b.sync(&store);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    assert!(!again.index_written, "{}", again.summary());
}

#[test]
fn an_empty_folder_deleted_on_one_device_is_deleted_on_the_other() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-rmdir");
    let b = Device::new("dev-b-rmdir");
    a.write("keep.txt", b"kept");
    mkdir(&a, "empty/inner");
    a.sync(&store);
    b.sync(&store);
    assert!(is_dir(&b, "empty/inner"));
    a.remove_folder("empty");
    a.sync(&store);
    b.sync(&store);
    assert!(!is_dir(&b, "empty"), "the emptied tree is gone here too");
    assert!(b.read("keep.txt").is_some());
    let back = a.sync(&store);
    assert!(!is_dir(&a, "empty"), "{}", back.summary());
}

#[test]
fn an_empty_folder_renamed_on_one_device_is_renamed_on_the_other() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-mvdir");
    let b = Device::new("dev-b-mvdir");
    mkdir(&a, "old name");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("old name", "new name");
    a.sync(&store);
    b.sync(&store);
    assert!(is_dir(&b, "new name"));
    assert!(!is_dir(&b, "old name"));
}

/// Deleting the last file of a folder deletes the file, not the folder: the folder stays on
/// every device, empty.
#[test]
fn a_folder_emptied_on_another_device_stays_a_folder_here() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-emptied");
    let b = Device::new("dev-b-emptied");
    a.write("docs/only.txt", b"the only file");
    a.write("top.txt", b"top");
    a.sync(&store);
    b.sync(&store);
    a.remove("docs/only.txt");
    let sent = a.sync(&store);
    assert_eq!(sent.deleted_there, 1, "{}", sent.summary());
    let got = b.sync(&store);
    assert_eq!(got.deleted_here, 1, "{}", got.summary());
    assert!(b.read("docs/only.txt").is_none());
    assert!(is_dir(&b, "docs"), "the folder stays, empty");
    // And the next passes keep it on both sides.
    b.sync(&store);
    a.sync(&store);
    assert!(is_dir(&a, "docs") && is_dir(&b, "docs"));
}
