//! Two drives over one tree (AZ-U1, AZ-S1/U4): the app builds them itself - Home and a folder
//! added as a drive, a synced listing (`sync:<id>`), an upload's `os:<folder>` - and ids cannot
//! tell that a file is its own destination or a folder is moved into itself. The sites of the
//! drives and the disk under them can (ARCH28 RF43 / AR5a-2, the overlap table).

use super::TempDir;
use crate::{
    xfer::{overlap, Overlap, Site},
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo, ScopedDrive,
};

/// `home/` with a few files, and the drives over it: the tree itself, the tree again, a drive
/// on a folder inside it.
fn trees() -> (TempDir, LocalDrive, LocalDrive, LocalDrive) {
    let tmp = TempDir::new("xfer-site");
    let home = LocalDrive::new(tmp.path().join("home"));
    home.put("docs/a.txt", b"alpha").unwrap();
    home.put("docs/sub/b.txt", b"beta").unwrap();
    home.put("readme.txt", b"hello").unwrap();
    home.create_folder("empty/").unwrap();
    let again = LocalDrive::new(tmp.path().join("home"));
    let inner = LocalDrive::without_manifest(tmp.path().join("home").join("docs"));
    (tmp, home, again, inner)
}

fn between(a: &dyn Drive, a_key: &str, b: &dyn Drive, b_key: &str) -> Overlap {
    overlap(&a.site(), a_key, &b.site(), b_key)
}

/// A drive that is no folder and no bucket: its only identity is itself. (Not a zero-sized type:
/// two of those may share an address.)
struct Far(#[allow(dead_code)] u8);

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

#[test]
fn a_file_is_the_same_file_through_another_drive_over_its_folder() {
    let (tmp, home, again, inner) = trees();
    // The same tree twice, and a drive on a folder inside it.
    assert_eq!(
        between(&home, "docs/a.txt", &again, "docs/a.txt"),
        Overlap::Same
    );
    assert_eq!(between(&home, "docs/a.txt", &inner, "a.txt"), Overlap::Same);
    assert_eq!(
        between(&inner, "sub/b.txt", &home, "docs/sub/b.txt"),
        Overlap::Same
    );
    // Another file is another file, and a name nobody has made is nobody's yet.
    assert_eq!(
        between(&home, "docs/a.txt", &again, "docs/sub/b.txt"),
        Overlap::Disjoint
    );
    assert_eq!(
        between(&home, "docs/a.txt", &again, "docs/new.txt"),
        Overlap::Disjoint
    );
    // A copy elsewhere is a copy.
    let other = LocalDrive::new(tmp.path().join("other"));
    other.put("docs/a.txt", b"alpha").unwrap();
    assert_eq!(
        between(&home, "docs/a.txt", &other, "docs/a.txt"),
        Overlap::Disjoint
    );
}

#[cfg(unix)]
#[test]
fn a_link_and_a_hard_link_name_the_same_file() {
    let (tmp, home, again, _) = trees();
    let root = tmp.path().join("home");
    std::os::unix::fs::symlink(root.join("docs"), root.join("alias")).unwrap();
    assert_eq!(
        between(&home, "docs/a.txt", &again, "alias/a.txt"),
        Overlap::Same
    );
    std::fs::hard_link(root.join("readme.txt"), root.join("second.txt")).unwrap();
    assert_eq!(
        between(&home, "readme.txt", &again, "second.txt"),
        Overlap::Same
    );
    assert_eq!(
        between(&home, "readme.txt", &again, "docs/a.txt"),
        Overlap::Disjoint
    );
}

#[test]
fn a_drive_that_is_not_on_this_disk_is_never_the_same_place_as_a_folder() {
    let (_tmp, home, _, _) = trees();
    assert_eq!(
        between(&home, "readme.txt", &Far(0), "readme.txt"),
        Overlap::Disjoint
    );
    assert_eq!(
        between(&Far(0), "readme.txt", &home, "readme.txt"),
        Overlap::Disjoint
    );
    assert_eq!(
        between(&home, "docs/", &Far(0), "docs/sub/"),
        Overlap::Disjoint
    );
    assert_eq!(
        between(&Far(0), "docs/", &home, "docs/sub/"),
        Overlap::Disjoint
    );
}

#[test]
fn a_folder_holds_itself_and_what_is_below_it_through_any_drive_over_the_tree() {
    let (tmp, home, again, inner) = trees();
    // Itself, and below: as the same tree sees it, and as the drive on docs/ does.
    assert_eq!(between(&home, "docs/", &again, "docs/"), Overlap::Same);
    assert_eq!(
        between(&home, "docs/", &again, "docs/sub/"),
        Overlap::SourceHoldsTarget
    );
    assert_eq!(
        between(&home, "docs/", &inner, "sub/"),
        Overlap::SourceHoldsTarget
    );
    // inner's docs/ is docs/docs/, which is not made: it would lie inside docs/.
    assert_eq!(
        between(&home, "docs/", &inner, "docs/"),
        Overlap::SourceHoldsTarget
    );
    assert_eq!(
        between(&inner, "sub/", &home, "docs/sub/deeper/"),
        Overlap::SourceHoldsTarget
    );
    // A folder not made yet counts by its place.
    assert_eq!(
        between(&home, "docs/", &again, "docs/new/deep/"),
        Overlap::SourceHoldsTarget
    );
    assert!(between(&home, "docs/", &again, "docs/new/deep/").target_inside_source());
    // What is beside it or above it is not inside it; the folder above holds the one below.
    assert_eq!(
        between(&home, "docs/sub/", &again, "docs/"),
        Overlap::TargetHoldsSource
    );
    assert_eq!(
        between(&home, "docs/sub/", &again, "empty/"),
        Overlap::Disjoint
    );
    assert_eq!(between(&home, "docs/", &again, "empty/"), Overlap::Disjoint);
    assert_eq!(
        between(&home, "docs/", &again, "docs-2/"),
        Overlap::Disjoint,
        "a longer name is not below it"
    );
    let other = LocalDrive::new(tmp.path().join("other"));
    other.create_folder("docs/").unwrap();
    assert_eq!(between(&home, "docs/", &other, "docs/"), Overlap::Disjoint);
}

#[cfg(unix)]
#[test]
fn a_folder_reached_through_a_link_is_the_folder() {
    let (tmp, home, again, _) = trees();
    let root = tmp.path().join("home");
    std::os::unix::fs::symlink(root.join("docs"), root.join("alias")).unwrap();
    // alias/ is docs/: moving docs/ into alias/sub/ moves it into itself.
    assert!(between(&home, "docs/", &again, "alias/sub/").target_inside_source());
    assert_eq!(between(&home, "docs/", &again, "alias/"), Overlap::Same);
    // A link that points outside the tree puts nothing of it in the tree.
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("away")).unwrap();
    assert_eq!(
        between(&home, "docs/", &again, "away/docs/"),
        Overlap::Disjoint
    );
}

#[test]
fn two_drives_over_one_bucket_are_one_place_and_a_grant_is_a_part_of_it() {
    let a = Site::bucket("https://s3.example.com/", "photos");
    let b = Site::bucket("https://S3.EXAMPLE.com", "photos");
    let other_bucket = Site::bucket("https://s3.example.com", "music");
    assert!(a.same_place(&b), "the endpoint's spelling does not matter");
    assert!(!a.same_place(&other_bucket));
    assert_eq!(overlap(&a, "x/", &b, "x/y.txt"), Overlap::SourceHoldsTarget);
    assert_eq!(overlap(&a, "x/y.txt", &b, "x/"), Overlap::TargetHoldsSource);
    assert_eq!(overlap(&a, "x/", &other_bucket, "x/"), Overlap::Disjoint);
    // Names in a bucket are exact: a case twin is another key.
    assert_eq!(overlap(&a, "x/", &b, "X/"), Overlap::Disjoint);
    // A file and a folder of one name are two keys there.
    assert_eq!(overlap(&a, "x", &b, "x/"), Overlap::Disjoint);
    // A drive that is a grant to a folder of the bucket sees that folder's keys.
    let scoped = a.under("2026/");
    assert!(!a.same_place(&scoped));
    assert_eq!(
        overlap(&a, "2026/", &scoped, ""),
        Overlap::Same,
        "its root is that folder"
    );
    assert_eq!(
        overlap(&a, "2026/", &scoped, "trip/"),
        Overlap::SourceHoldsTarget
    );
    assert_eq!(overlap(&a, "2025/", &scoped, "trip/"), Overlap::Disjoint);
}

#[test]
fn a_drive_nothing_is_known_of_is_the_same_place_as_itself_and_unknown_to_another() {
    let (_tmp, home, _, _) = trees();
    let (one, two) = (Far(1), Far(2));
    assert_eq!(between(&one, "a/", &one, "a/"), Overlap::Same);
    assert_eq!(
        between(&one, "a/", &one, "a/b/"),
        Overlap::SourceHoldsTarget
    );
    assert_eq!(between(&one, "a/", &one, "b/"), Overlap::Disjoint);
    assert_eq!(between(&one, "a/", &two, "a/"), Overlap::Unknown);
    assert!(one.site().same_place(&one.site()));
    assert!(!one.site().same_place(&two.site()));
    assert!(!one.site().same_place(&home.site()));
    // A clone of one pointer is the one object.
    let shared: std::sync::Arc<dyn Drive> = std::sync::Arc::new(Far(3));
    let again = shared.clone();
    assert!(shared.site().same_place(&again.site()));
}

#[test]
fn a_scoped_drive_is_a_part_of_the_drive_it_looks_through() {
    let (_tmp, home, again, _) = trees();
    let scoped = ScopedDrive::new(again, "docs/", true).unwrap();
    assert!(!home.site().same_place(&scoped.site()));
    assert_eq!(between(&home, "docs/", &scoped, ""), Overlap::Same);
    assert_eq!(
        between(&home, "docs/", &scoped, "sub/"),
        Overlap::SourceHoldsTarget
    );
    assert_eq!(between(&home, "empty/", &scoped, ""), Overlap::Disjoint);
}
