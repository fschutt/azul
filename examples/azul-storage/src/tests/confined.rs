//! A confined folder ([`LocalDrive::confined`], [`crate::confined`]): no change goes through a
//! symbolic link below its root, whoever made the link and whenever.

use std::{fs, os::unix::fs::symlink};

use super::TempDir;
use crate::{confined, Drive, LocalDrive};

/// A root whose `inbox` links to a folder outside it that holds `secret.txt`.
fn linked() -> (TempDir, TempDir, LocalDrive) {
    let root = TempDir::new("confined-root");
    let outside = TempDir::new("confined-outside");
    fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    symlink(outside.path(), root.path().join("inbox")).unwrap();
    let drive = LocalDrive::without_manifest(root.path()).confined();
    (root, outside, drive)
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn nothing_is_written_made_renamed_copied_or_deleted_through_a_link_below_the_root() {
    let (root, outside, drive) = linked();
    fs::write(root.path().join("mine.txt"), b"mine").unwrap();
    assert!(drive.put("inbox/evil.plist", b"run me").is_err());
    assert!(drive
        .put_from("inbox/sub/evil.plist", &mut &b"run me"[..])
        .is_err());
    assert!(drive.delete("inbox/secret.txt").is_err());
    assert!(drive.rename("mine.txt", "inbox/mine.txt").is_err());
    assert!(drive.create_folder("inbox/new/").is_err());
    assert!(drive.copy("mine.txt", "inbox/copy.txt").is_err());
    let err = drive.put("inbox/evil.plist", b"run me").unwrap_err().to_string();
    assert!(err.contains("symbolic link"), "says why: {err}");
    assert_eq!(names(outside.path()), vec!["secret.txt"]);
    assert_eq!(fs::read(outside.path().join("secret.txt")).unwrap(), b"secret");
    assert!(
        fs::symlink_metadata(root.path().join("inbox"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link stays as the user made it"
    );
    assert_eq!(fs::read(root.path().join("mine.txt")).unwrap(), b"mine");
}

#[test]
fn a_link_as_the_file_itself_is_replaced_or_removed_never_followed() {
    let (root, outside, drive) = linked();
    let secret = outside.path().join("secret.txt");
    symlink(&secret, root.path().join("note.txt")).unwrap();
    drive.put("note.txt", b"a note").unwrap();
    assert_eq!(fs::read(&secret).unwrap(), b"secret");
    let note = fs::symlink_metadata(root.path().join("note.txt")).unwrap();
    assert!(note.is_file(), "a file of its own now");
    symlink(&secret, root.path().join("gone.txt")).unwrap();
    drive.delete("gone.txt").unwrap();
    assert!(fs::symlink_metadata(root.path().join("gone.txt")).is_err());
    assert_eq!(fs::read(&secret).unwrap(), b"secret");
}

#[test]
fn a_confined_drive_writes_renames_and_deletes_as_any_other_and_leaves_no_temporary_file() {
    let root = TempDir::new("confined-plain");
    let drive = LocalDrive::without_manifest(root.path()).confined();
    drive.put("a/b/c.txt", b"c").unwrap();
    assert_eq!(drive.get("a/b/c.txt").unwrap(), b"c");
    assert_eq!(drive.put_from("a/d.txt", &mut &b"dd"[..]).unwrap(), 2);
    drive.put("a/d.txt", b"ddd").unwrap();
    assert_eq!(drive.get("a/d.txt").unwrap(), b"ddd");
    drive.rename("a/b/c.txt", "x/c.txt").unwrap();
    assert!(drive.get("a/b/c.txt").is_err());
    assert_eq!(drive.get("x/c.txt").unwrap(), b"c");
    assert!(drive.rename("a/d.txt", "x/c.txt").is_err(), "the name is taken");
    drive.rename("a/", "z/").unwrap();
    assert_eq!(drive.get("z/d.txt").unwrap(), b"ddd");
    drive.copy("z/d.txt", "y/d.txt").unwrap();
    assert_eq!(drive.get("y/d.txt").unwrap(), b"ddd");
    drive.create_folder("w/v/").unwrap();
    assert!(root.path().join("w").join("v").is_dir());
    drive.delete("z/d.txt").unwrap();
    drive.delete("z/d.txt").unwrap();
    assert!(drive.delete("w/").is_err(), "a folder is no file");
    drive.delete_folder("w/").unwrap();
    assert!(!root.path().join("w").exists());
    assert!(drive.put("q/", b"x").is_err());
    assert_eq!(names(root.path()), vec!["x", "y", "z"]);
    assert_eq!(names(&root.path().join("z")), vec!["b"]);
}

#[test]
fn a_folder_on_the_way_that_is_a_link_is_refused_and_named_so() {
    let (root, _outside, _drive) = linked();
    let err = confined::Folder::open(root.path(), &["inbox"], false)
        .err()
        .unwrap();
    assert!(confined::through_link(&err), "{err}");
    let err = confined::Folder::open(root.path(), &["inbox", "deeper"], true)
        .err()
        .unwrap();
    assert!(confined::through_link(&err), "{err}");
    fs::write(root.path().join("file"), b"f").unwrap();
    let err = confined::Folder::open(root.path(), &["file", "x"], true)
        .err()
        .unwrap();
    assert!(!confined::through_link(&err), "a file on the way is no link: {err}");
    let made = confined::Folder::open(root.path(), &["new", "deeper"], true).unwrap();
    assert!(made.path().is_dir());
    assert_eq!(made.kind("nothing").unwrap(), None);
}
