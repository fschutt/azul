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

/// A folder has an identity, so its rename is one rename: renamed on one device while a file in
/// it is renamed on the other, both renames hold (A3, C1).
#[test]
fn a_folder_renamed_on_one_device_and_a_file_in_it_renamed_on_the_other_keep_both_renames() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-compose");
    let b = Device::new("dev-b-compose");
    a.write("dir/child.txt", b"child");
    a.write("x.txt", b"x");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("dir", "dir2");
    fs::rename(b.path().join("dir/child.txt"), b.path().join("dir/child2.txt")).unwrap();
    a.sync(&store);
    b.sync(&store);
    a.sync(&store);
    for device in [&a, &b] {
        assert_eq!(device.read("dir2/child2.txt").as_deref(), Some(&b"child"[..]));
        assert!(!is_dir(device, "dir"), "the old name is gone");
        assert!(device.read("dir2/child.txt").is_none());
    }
    assert_eq!(a.tree(), b.tree());
}

/// A file put into a folder under its old name, after another device renamed the folder,
/// lands in the renamed folder: the folder is not made again under its old name.
#[test]
fn a_file_put_into_a_folder_the_drive_renamed_lands_in_the_renamed_folder() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-into");
    let b = Device::new("dev-b-into");
    a.write("photos/a.jpg", b"a");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    a.sync(&store);
    b.write("photos/new.jpg", b"new");
    b.sync(&store);
    a.sync(&store);
    for device in [&a, &b] {
        assert_eq!(device.read("pictures/new.jpg").as_deref(), Some(&b"new"[..]));
        assert_eq!(device.read("pictures/a.jpg").as_deref(), Some(&b"a"[..]));
        assert!(!is_dir(device, "photos"));
    }
}

/// A folder renamed here while a file in it was edited on the drive keeps the edit, under the
/// new name: neither the old folder nor the old version comes back.
#[test]
fn a_folder_renamed_here_keeps_an_edit_made_meanwhile_on_the_drive_under_its_new_name() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-edit-in");
    let b = Device::new("dev-b-edit-in");
    a.write("docs/x.txt", b"version 1");
    a.sync(&store);
    b.sync(&store);
    b.write("docs/x.txt", b"version 2, from b");
    b.sync(&store);
    a.rename_folder("docs", "papers");
    a.sync(&store);
    b.sync(&store);
    a.sync(&store);
    for device in [&a, &b] {
        assert_eq!(
            device.read("papers/x.txt").as_deref(),
            Some(&b"version 2, from b"[..])
        );
        assert!(!is_dir(device, "docs"));
    }
    assert_eq!(a.tree(), b.tree());
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
