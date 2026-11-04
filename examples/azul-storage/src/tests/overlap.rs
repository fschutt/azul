//! Two drives over one tree (AZ-U1, AZ-S1/U4): the app builds them itself - Home and a folder
//! added as a drive, a synced listing (`sync:<id>`), an upload's `os:<folder>` - and ids cannot
//! tell that a file is its own destination or a folder is moved into itself.

use super::TempDir;
use crate::{overlap::same_local_file, Drive, LocalDrive};

/// `home/` with a few files, and the drives over it: the tree itself, the tree again, a drive
/// on a folder inside it.
fn trees() -> (TempDir, LocalDrive, LocalDrive, LocalDrive) {
    let tmp = TempDir::new("overlap");
    let home = LocalDrive::new(tmp.path().join("home"));
    home.put("docs/a.txt", b"alpha").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    home.put("readme.txt", b"hello").unwrap();
    home.create_folder("empty/").unwrap();
    let again = LocalDrive::new(tmp.path().join("home"));
    let inner = LocalDrive::without_manifest(tmp.path().join("home").join("docs"));
    (tmp, home, again, inner)
}

#[test]
fn a_file_is_the_same_file_through_another_drive_over_its_folder() {
    let (tmp, home, again, inner) = trees();
    // The same tree twice, and a drive on a folder inside it.
    assert!(same_local_file(&home, "docs/a.txt", &again, "docs/a.txt"));
    assert!(same_local_file(&home, "docs/a.txt", &inner, "a.txt"));
    assert!(same_local_file(
        &inner,
        "sub/b.txt",
        &home,
        "docs/sub/b.txt"
    ));
    // Another file is another file, and a name nobody has made is nobody's yet.
    assert!(!same_local_file(
        &home,
        "docs/a.txt",
        &again,
        "docs/sub/b.txt"
    ));
    assert!(!same_local_file(
        &home,
        "docs/a.txt",
        &again,
        "docs/new.txt"
    ));
    assert!(!same_local_file(
        &home,
        "docs/missing.txt",
        &again,
        "docs/missing.txt"
    ));
    // A copy elsewhere is a copy.
    let other = LocalDrive::new(tmp.path().join("other"));
    other.put("docs/a.txt", b"alpha").unwrap();
    assert!(!same_local_file(&home, "docs/a.txt", &other, "docs/a.txt"));
}

#[cfg(unix)]
#[test]
fn a_link_and_a_hard_link_name_the_same_file() {
    let (tmp, home, again, _) = trees();
    let root = tmp.path().join("home");
    std::os::unix::fs::symlink(root.join("docs"), root.join("alias")).unwrap();
    assert!(same_local_file(&home, "docs/a.txt", &again, "alias/a.txt"));
    std::fs::hard_link(root.join("readme.txt"), root.join("second.txt")).unwrap();
    assert!(same_local_file(&home, "readme.txt", &again, "second.txt"));
    assert!(!same_local_file(&home, "readme.txt", &again, "docs/a.txt"));
}

#[test]
fn a_drive_that_is_not_on_this_disk_is_never_the_same_place() {
    use crate::{ByteRange, DriveError, ListPage, ListRequest, ObjectInfo};
    struct Far;
    impl Drive for Far {
        fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
            Ok(ListPage::default())
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::NotFound { key: key.into() })
        }
        fn get_range(&self, key: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::NotFound { key: key.into() })
        }
        fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
            Ok(())
        }
        fn delete(&self, _: &str) -> Result<(), DriveError> {
            Ok(())
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            Err(DriveError::NotFound { key: key.into() })
        }
    }
    let (_tmp, home, _, _) = trees();
    assert!(!same_local_file(&home, "readme.txt", &Far, "readme.txt"));
    assert!(!same_local_file(&Far, "readme.txt", &home, "readme.txt"));
}
