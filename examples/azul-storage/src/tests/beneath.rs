//! Files under a root reached without a link: a folder that is a link (made by the user, or
//! swapped in while a sync runs) never carries a read or a write out of the root.

use std::fs;

use super::TempDir;
use crate::{
    beneath::{self, BeneathError, Kind, Root},
    local::THROUGH_LINK,
    Drive, DriveError, LocalDrive,
};

/// A root and, beside it, a folder of the user's that must never be touched.
struct World {
    root: TempDir,
    outside: TempDir,
}

impl World {
    fn new(what: &str) -> World {
        World {
            root: TempDir::new(&format!("beneath-root-{what}")),
            outside: TempDir::new(&format!("beneath-outside-{what}")),
        }
    }

    /// `<root>/<name>` as a link to the outside folder.
    #[cfg(unix)]
    fn link_out(&self, name: &str) {
        std::os::unix::fs::symlink(self.outside.path(), self.root.path().join(name)).unwrap();
    }

    fn outside_names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.outside.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

fn through_link(result: Result<(), DriveError>) -> bool {
    matches!(result, Err(DriveError::InvalidKey { reason, .. }) if reason == THROUGH_LINK)
}

#[cfg(unix)]
#[test]
fn a_contained_put_under_a_linked_folder_is_refused_and_lands_nowhere_outside() {
    let w = World::new("put");
    w.link_out("inbox");
    let drive = LocalDrive::without_manifest(w.root.path()).contained();
    assert!(through_link(
        drive.put("inbox/evil.plist", b"<plist>run me</plist>")
    ));
    let mut body: &[u8] = b"streamed";
    assert!(matches!(
        drive.put_from("inbox/streamed.plist", &mut body),
        Err(DriveError::InvalidKey { reason, .. }) if reason == THROUGH_LINK
    ));
    assert!(w.outside_names().is_empty(), "{:?}", w.outside_names());
    // The link itself is left as the user made it.
    assert!(fs::symlink_metadata(w.root.path().join("inbox"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn a_contained_rename_or_copy_into_a_linked_folder_is_refused() {
    let w = World::new("rename");
    w.link_out("inbox");
    let drive = LocalDrive::without_manifest(w.root.path()).contained();
    drive.put("a.txt", b"mine").unwrap();
    assert!(through_link(drive.rename("a.txt", "inbox/a.txt")));
    assert!(through_link(drive.copy("a.txt", "inbox/b.txt")));
    assert!(w.outside_names().is_empty(), "{:?}", w.outside_names());
    assert_eq!(drive.get("a.txt").unwrap(), b"mine");
}

#[cfg(unix)]
#[test]
fn a_contained_read_or_delete_never_goes_through_a_link() {
    let w = World::new("read");
    fs::write(w.outside.path().join("secret"), b"the user's secret").unwrap();
    w.link_out("dir");
    std::os::unix::fs::symlink(
        w.outside.path().join("secret"),
        w.root.path().join("secret-link"),
    )
    .unwrap();
    let drive = LocalDrive::without_manifest(w.root.path()).contained();
    assert!(matches!(
        drive.get("secret-link"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.get("dir/secret"),
        Err(DriveError::InvalidKey { reason, .. }) if reason == THROUGH_LINK
    ));
    assert!(through_link(drive.delete("dir/secret")));
    assert_eq!(
        fs::read(w.outside.path().join("secret")).unwrap(),
        b"the user's secret"
    );
}

#[test]
fn a_contained_drive_reads_and_writes_its_own_files_as_any_drive_does() {
    let w = World::new("plain");
    let drive = LocalDrive::new(w.root.path()).contained();
    drive.put("docs/sub/a.txt", b"alpha").unwrap();
    assert_eq!(drive.get("docs/sub/a.txt").unwrap(), b"alpha");
    assert_eq!(drive.head("docs/sub/a.txt").unwrap().size, 5);
    drive.rename("docs/sub/a.txt", "docs/b.txt").unwrap();
    assert_eq!(drive.get("docs/b.txt").unwrap(), b"alpha");
    drive.delete("docs/b.txt").unwrap();
    assert!(matches!(
        drive.get("docs/b.txt"),
        Err(DriveError::NotFound { .. })
    ));
    // No temporary file is left behind.
    let left: Vec<String> = fs::read_dir(w.root.path().join("docs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| beneath::is_temp_name(n))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[cfg(unix)]
#[test]
fn a_folder_swapped_for_a_link_after_it_was_listed_fails_the_walk() {
    let w = World::new("swap");
    fs::create_dir_all(w.root.path().join("a/b")).unwrap();
    let root = Root::open(w.root.path()).unwrap();
    assert!(root.dir("a/b", false).is_ok());
    fs::rename(w.root.path().join("a"), w.root.path().join("a-old")).unwrap();
    w.link_out("a");
    assert!(matches!(root.dir("a/b", false), Err(BeneathError::ThroughLink(at)) if at == "a"));
    assert!(matches!(
        root.dir("a", true),
        Err(BeneathError::ThroughLink(_))
    ));
    assert!(w.outside_names().is_empty());
}

#[cfg(unix)]
#[test]
fn a_fifo_is_refused_by_open_read_without_blocking() {
    let w = World::new("fifo");
    let path = std::ffi::CString::new(
        w.root
            .path()
            .join("pipe")
            .as_os_str()
            .to_string_lossy()
            .as_bytes(),
    )
    .unwrap();
    // SAFETY: a valid C string.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let top = Root::open(w.root.path()).unwrap().top().unwrap();
    assert_eq!(top.lstat("pipe").unwrap().kind, Kind::Fifo);
    // Without O_NONBLOCK this open would wait for a writer for ever.
    assert!(matches!(
        top.open_read("pipe"),
        Err(BeneathError::ThroughLink(_))
    ));
}

#[cfg(unix)]
#[test]
fn a_link_named_in_a_folder_is_seen_as_a_link_and_never_opened() {
    let w = World::new("lstat");
    fs::write(w.outside.path().join("secret"), b"secret").unwrap();
    std::os::unix::fs::symlink(w.outside.path().join("secret"), w.root.path().join("s")).unwrap();
    let top = Root::open(w.root.path()).unwrap().top().unwrap();
    assert_eq!(top.lstat("s").unwrap().kind, Kind::Link);
    assert!(matches!(
        top.open_read("s"),
        Err(BeneathError::ThroughLink(_))
    ));
    assert_eq!(top.read_link("s").unwrap(), w.outside.path().join("secret"));
}

#[test]
fn names_lists_every_entry_and_a_second_listing_lists_them_again() {
    let w = World::new("names");
    for name in ["a", "b", "c.txt"] {
        fs::write(w.root.path().join(name), name).unwrap();
    }
    fs::create_dir(w.root.path().join("sub")).unwrap();
    let top = Root::open(w.root.path()).unwrap().top().unwrap();
    let list = |top: &beneath::Dir| {
        let mut names: Vec<String> = top
            .names()
            .unwrap()
            .into_iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(list(&top), vec!["a", "b", "c.txt", "sub"]);
    assert_eq!(list(&top), vec!["a", "b", "c.txt", "sub"]);
}

#[test]
fn write_atomically_replaces_the_file_whole_and_leaves_no_temp_file() {
    let w = World::new("atomic");
    let root = Root::open(w.root.path()).unwrap();
    let dir = root.dir("x/y", true).unwrap();
    dir.write_atomically("f.txt", b"first", 0o644).unwrap();
    dir.write_atomically("f.txt", b"second", 0o644).unwrap();
    assert_eq!(
        fs::read(w.root.path().join("x/y/f.txt")).unwrap(),
        b"second"
    );
    let names: Vec<String> = dir
        .names()
        .unwrap()
        .into_iter()
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["f.txt"]);
}

#[test]
fn a_temp_name_of_a_long_name_is_still_a_valid_name() {
    let long = "n".repeat(250);
    let tmp = beneath::temp_name(&long);
    assert!(tmp.len() <= 255, "{}", tmp.len());
    assert!(beneath::is_temp_name(&tmp));
}
