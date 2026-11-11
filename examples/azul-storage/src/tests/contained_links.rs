//! A contained local drive ([`LocalDrive::contained`]) makes and removes folders the way it
//! writes files: from the root, one name at a time, never through a link below it. A link that
//! is the file itself is replaced or removed, and what it points at is left alone.

use std::{fs, path::Path};

use super::TempDir;
#[cfg(unix)]
use crate::{local::THROUGH_LINK, DriveError};
use crate::{Drive, LocalDrive};

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

/// ARCH29 C6c, local.rs. A contained drive walks by `OnDisk` and `Admitted` keys. A read, a delete, a
/// folder removed and the source of a rename walk to what is there and make NO folder - whatever the
/// spelling of the name, a protected one included (the disk has the name already: nothing is made for a
/// look). A write, a folder made and the target of a rename make the folders of a name that passed the
/// rows of a name a person chose (the shape and this system's: the user's own folder keeps its `.git`),
/// and a key that leaves the folder is refused before anything is made. Green on the old code too: it pins
/// what the two key types now hold.
#[test]
fn c6c_a_contained_drive_makes_no_folder_for_what_only_looks_and_makes_them_for_what_writes() {
    let root = TempDir::new("contained-c6c");
    let drive = LocalDrive::without_manifest(root.path()).contained();
    for key in ["missing/a.txt", ".GIT/hooks/x", "GIT~1/config", "x/.hg/y"] {
        assert!(
            matches!(drive.get(key), Err(crate::DriveError::NotFound { .. })),
            "{key:?}"
        );
        assert_eq!(drive.delete(key), Ok(()), "{key:?}");
        assert!(drive.head(key).is_err(), "{key:?}");
        let folder = format!(
            "{}/",
            key.rsplit_once('/').map_or(key, |(folder, _)| folder)
        );
        assert_eq!(drive.remove_empty_folder(&folder), Ok(true), "{folder:?}");
        assert!(drive.rename(key, "elsewhere/b.txt").is_err(), "{key:?}");
    }
    assert!(
        names(root.path()).is_empty(),
        "a folder was made for a look: {:?}",
        names(root.path())
    );
    // A write makes its folders; a folder is made; a rename makes the target's.
    drive.put("docs/2026/a.txt", b"a").unwrap();
    drive.create_folder("projects/new/").unwrap();
    drive
        .rename("docs/2026/a.txt", "archive/2026/a.txt")
        .unwrap();
    assert_eq!(drive.get("archive/2026/a.txt").unwrap(), b"a");
    assert!(root.path().join("projects/new").is_dir());
    // The user's own folder keeps its repository: a name a person chose is held to the shape.
    drive.put("proj/.git/config", b"c").unwrap();
    assert_eq!(drive.get("proj/.git/config").unwrap(), b"c");
    // A key that leaves the folder makes nothing.
    for key in ["../escape.txt", "a/../../b.txt"] {
        assert!(drive.put(key, b"x").is_err(), "{key:?}");
    }
    assert!(
        !root.path().join("a").exists(),
        "a folder was made for a key that leaves it"
    );
}
