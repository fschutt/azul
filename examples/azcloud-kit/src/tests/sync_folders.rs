//! Folders as entries of the drive's index: an empty folder is made, deleted and renamed on
//! every device like a file, and a folder emptied by deleting its files stays a folder.

use std::{collections::BTreeMap, fs, path::Path};

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device},
};

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

/// The files of the folder's trash, by content.
fn trash_of(folder: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = vec![folder.join(".azlin").join("sync").join("trash")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                out.push(fs::read(entry.path()).unwrap());
            }
        }
    }
    out
}

/// The files that are in `tree` or in `trash` with these `bytes`: how many times each is kept.
fn kept(tree: &BTreeMap<String, Vec<u8>>, trash: &[Vec<u8>], bytes: &[u8]) -> usize {
    tree.values().filter(|b| b.as_slice() == bytes).count()
        + trash.iter().filter(|b| b.as_slice() == bytes).count()
}

/// X-DELTA-EDGE (E5b, T6, case 1). Another order than the tested one: B commits a NEW file under the old
/// folder name BEFORE A syncs its rename. The drive then holds `photos/new.jpg` beside the rename
/// of `photos`; when A syncs it takes the file along into the renamed folder - and every device ends
/// with `pictures/` holding both, no `photos/`, nothing trashed, no conflict copy, and a quiet second
/// round.
#[test]
fn a_new_file_committed_under_the_old_folder_name_before_the_rename_syncs_lands_in_the_renamed_folder(
) {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-order");
    let b = Device::new("dev-b-order");
    a.write("photos/a.jpg", b"a");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    b.write("photos/new.jpg", b"new");
    b.sync(&store);
    a.sync(&store);
    b.sync(&store);
    a.sync(&store);
    for device in [&a, &b] {
        assert_eq!(
            device.read("pictures/new.jpg").as_deref(),
            Some(&b"new"[..]),
            "{:?}",
            device.tree().keys()
        );
        assert_eq!(device.read("pictures/a.jpg").as_deref(), Some(&b"a"[..]));
        assert!(!is_dir(device, "photos"), "{:?}", device.tree().keys());
        assert!(trash_of(device.path()).is_empty(), "nothing was trashed");
        assert!(
            !device.tree().keys().any(|k| k.contains("(conflict")),
            "no copy"
        );
    }
    assert_eq!(a.tree(), b.tree());
    for device in [&a, &b] {
        let again = device.sync(&store);
        assert!(!again.index_written, "{}", again.summary());
        assert_eq!(
            (again.files_up, again.files_down),
            (0, 0),
            "{}",
            again.summary()
        );
    }
}

/// X-DELTA-EDGE (E5b, T6, case 2). Both devices rename one folder, to different names: the drive's
/// name wins (`Renames.clashes`: "renamed here and on the drive to different names; it takes the
/// drive's"), the files B added under its own name follow, and no file is lost: both end with
/// `pictures/{a.jpg,b.jpg}`, no `images/`, nothing in the trash.
#[test]
fn a_folder_renamed_differently_on_both_devices_takes_the_drives_name_and_keeps_every_file() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-clash");
    let b = Device::new("dev-b-clash");
    a.write("photos/a.jpg", b"a");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    a.sync(&store);
    b.rename_folder("photos", "images");
    b.write("images/b.jpg", b"b");
    let report = b.sync(&store);
    assert!(
        report.notes.iter().any(|n| n
            .contains("renamed here and on the drive to different names; it takes the drive's")),
        "{:?}",
        report.notes
    );
    a.sync(&store);
    b.sync(&store);
    for device in [&a, &b] {
        assert_eq!(
            device.read("pictures/a.jpg").as_deref(),
            Some(&b"a"[..]),
            "{:?}",
            device.tree().keys()
        );
        assert_eq!(
            device.read("pictures/b.jpg").as_deref(),
            Some(&b"b"[..]),
            "{:?}",
            device.tree().keys()
        );
        assert!(!is_dir(device, "images"), "{:?}", device.tree().keys());
        assert!(!is_dir(device, "photos"));
        assert!(trash_of(device.path()).is_empty());
    }
    assert_eq!(a.tree(), b.tree());
}

/// X-DELTA-EDGE (E5b, T6, case 3). A renames `photos` to `pictures` and syncs; B has a folder
/// `pictures` of its own with a file in it that is not synced yet: when the rename arrives the new
/// name is taken here, so it cannot be made as one - its files follow one by one (`moves_fit` false,
/// "its new name is taken"). Every byte of the folders' files is in the live tree or in the trash
/// exactly once on B, the drive holds each once, and the second round is quiet.
#[test]
fn a_folder_renamed_to_a_name_the_other_device_already_uses_falls_back_to_files_and_loses_none() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-taken");
    let b = Device::new("dev-b-taken");
    a.write("photos/a.jpg", b"a");
    a.write("photos/b.jpg", b"b");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    a.sync(&store);
    b.write("pictures/z.jpg", b"z");
    let report = b.sync(&store);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("its new name is taken")),
        "{:?}",
        report.notes
    );
    a.sync(&store);
    b.sync(&store);
    let (tree, trash) = (b.tree(), trash_of(b.path()));
    for bytes in [&b"a"[..], b"b", b"z"] {
        assert_eq!(
            kept(&tree, &trash, bytes),
            1,
            "{:?}: {:?}",
            String::from_utf8_lossy(bytes),
            tree.keys()
        );
    }
    let index = index_of(&store);
    for bytes in [&b"a"[..], b"b", b"z"] {
        let hash = crate::sync::local::hash_bytes(bytes);
        assert_eq!(
            index.files.values().filter(|f| f.hash == hash).count(),
            1,
            "the drive holds {:?} once",
            String::from_utf8_lossy(bytes)
        );
    }
    for device in [&a, &b] {
        let again = device.sync(&store);
        assert!(!again.index_written, "{}", again.summary());
        assert_eq!(
            (again.files_up, again.files_down),
            (0, 0),
            "{}",
            again.summary()
        );
    }
}

/// X-DELTA-EDGE (E5b, T6, case 3, the renaming side). The same two devices the other way round: B's
/// `pictures` is on the drive first, then A syncs its rename of `photos` to the same name: the new
/// name is taken THERE, so A's files go one by one ("its new name is taken there") - and no byte is
/// lost on either device.
#[test]
fn a_folder_renamed_to_a_name_the_drive_already_has_goes_file_by_file_and_loses_none() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-taken-there");
    let b = Device::new("dev-b-taken-there");
    a.write("photos/a.jpg", b"a");
    a.write("photos/b.jpg", b"b");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    b.write("pictures/z.jpg", b"z");
    b.sync(&store);
    let report = a.sync(&store);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("its new name is taken there")),
        "{:?}",
        report.notes
    );
    b.sync(&store);
    a.sync(&store);
    for device in [&a, &b] {
        let (tree, trash) = (device.tree(), trash_of(device.path()));
        for bytes in [&b"a"[..], b"b", b"z"] {
            assert!(
                kept(&tree, &trash, bytes) >= 1,
                "{:?} is kept: {:?}",
                String::from_utf8_lossy(bytes),
                tree.keys()
            );
        }
        assert!(!is_dir(device, "photos"), "{:?}", tree.keys());
    }
    assert_eq!(a.tree(), b.tree());
}

/// X-DELTA-EDGE (E5b, T6, case 4). A disk with no stable inode (FAT, exFAT, SMB): the scan finds every
/// folder with identity 0, so a folder renamed here is no rename it can tell - the files travel as
/// deletes and new files, and the other device matches them by content and renames what it has
/// (`report.moved`) instead of fetching them again. The scenario of
/// `a_file_put_into_a_folder_the_drive_renamed_lands_in_the_renamed_folder` with A's folder identities
/// blanked through the fault seam: the content-based move is made, and no byte is lost on either device.
#[test]
fn a_folder_renamed_on_a_disk_that_gives_no_identity_travels_as_content_moves_and_loses_none() {
    use crate::sync::faults;
    let store = S3Bucket::new();
    let a = Device::new("dev-a-blank");
    let b = Device::new("dev-b-blank");
    faults::set_blank_folder_ids(a.path(), true);
    a.write("photos/a.jpg", b"a jpeg that is the same wherever it is");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("photos", "pictures");
    a.sync(&store);
    b.write("photos/new.jpg", b"new");
    let report = b.sync(&store);
    assert!(
        report
            .moved
            .iter()
            .any(|m| m == "photos/a.jpg -> pictures/a.jpg"),
        "the file is renamed here by its content, not fetched again: {:?} {}",
        report.moved,
        report.summary()
    );
    assert_eq!(report.bytes_down, 0, "{}", report.summary());
    a.sync(&store);
    b.sync(&store);
    a.sync(&store);
    faults::clear(a.path());
    for device in [&a, &b] {
        let (tree, trash) = (device.tree(), trash_of(device.path()));
        for bytes in [&b"a jpeg that is the same wherever it is"[..], b"new"] {
            assert!(
                kept(&tree, &trash, bytes) >= 1,
                "{:?} is kept: {:?}",
                String::from_utf8_lossy(bytes),
                tree.keys()
            );
        }
        assert!(!is_dir(device, "photos") || device.read("photos/a.jpg").is_none());
    }
    assert_eq!(a.tree(), b.tree());
}
