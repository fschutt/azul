//! A contained local drive ([`LocalDrive::contained`]) makes and removes folders the way it
//! writes files: from the root, one name at a time, never through a link below it. A link that
//! is the file itself is replaced or removed, and what it points at is left alone.

use std::{fs, path::Path};

use super::TempDir;
use crate::{local::THROUGH_LINK, Drive, DriveError, LocalDrive};

/// A root whose `inbox` links to a folder outside it, which holds `secret.txt` and
/// `sub/kept.txt`.
#[cfg(unix)]
fn linked(what: &str) -> (TempDir, TempDir, LocalDrive) {
    let root = TempDir::new(&format!("contained-links-root-{what}"));
    let outside = TempDir::new(&format!("contained-links-outside-{what}"));
    fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    fs::create_dir(outside.path().join("sub")).unwrap();
    fs::write(outside.path().join("sub").join("kept.txt"), b"kept").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("inbox")).unwrap();
    let drive = LocalDrive::without_manifest(root.path()).contained();
    (root, outside, drive)
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
fn through_link(result: Result<(), DriveError>) -> bool {
    matches!(result, Err(DriveError::InvalidKey { reason, .. }) if reason == THROUGH_LINK)
}

#[cfg(unix)]
#[test]
fn a_contained_drive_makes_or_removes_no_folder_through_a_link() {
    let (root, outside, drive) = linked("folders");
    assert!(through_link(drive.create_folder("inbox/new/")));
    assert!(through_link(drive.create_folder("inbox/")));
    assert!(through_link(drive.delete_folder("inbox/sub/")));
    assert!(drive.delete_folder("inbox/").is_err());
    assert_eq!(names(outside.path()), ["secret.txt", "sub"]);
    assert_eq!(
        fs::read(outside.path().join("sub").join("kept.txt")).unwrap(),
        b"kept"
    );
    assert!(
        fs::symlink_metadata(root.path().join("inbox"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link stays as the user made it"
    );
    // A folder that holds a link goes, link and all; the link's folder outside stays.
    fs::create_dir(root.path().join("w")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("w").join("out")).unwrap();
    drive.delete_folder("w/").unwrap();
    assert!(fs::symlink_metadata(root.path().join("w")).is_err());
    assert_eq!(names(outside.path()), ["secret.txt", "sub"]);
}

#[cfg(unix)]
#[test]
fn a_link_as_the_file_itself_is_replaced_or_removed_never_followed() {
    let (root, outside, drive) = linked("file");
    let secret = outside.path().join("secret.txt");
    std::os::unix::fs::symlink(&secret, root.path().join("note.txt")).unwrap();
    drive.put("note.txt", b"a note").unwrap();
    assert_eq!(fs::read(&secret).unwrap(), b"secret");
    assert!(
        fs::symlink_metadata(root.path().join("note.txt"))
            .unwrap()
            .is_file(),
        "a file of its own now"
    );
    std::os::unix::fs::symlink(&secret, root.path().join("gone.txt")).unwrap();
    drive.delete("gone.txt").unwrap();
    assert!(fs::symlink_metadata(root.path().join("gone.txt")).is_err());
    assert_eq!(fs::read(&secret).unwrap(), b"secret");
}

#[test]
fn a_contained_drive_makes_and_removes_its_own_folders_as_any_drive_does() {
    let root = TempDir::new("contained-links-plain");
    let drive = LocalDrive::without_manifest(root.path()).contained();
    drive.create_folder("w/v/").unwrap();
    assert!(root.path().join("w").join("v").is_dir());
    drive.create_folder("w/v/").unwrap();
    drive.put("w/v/a.txt", b"a").unwrap();
    drive.put("w/b.txt", b"b").unwrap();
    assert!(
        drive.create_folder("w/b.txt/").is_err(),
        "a file has this name"
    );
    assert!(
        drive.delete_folder("w/b.txt/").is_err(),
        "a file is no folder"
    );
    drive.delete_folder("w/").unwrap();
    assert!(!root.path().join("w").exists());
    drive.delete_folder("w/").unwrap();
    assert!(!names(root.path()).contains(&String::from("w")));
}
