//! Files under a root reached without a link: a folder that is a link (made by the user, or
//! swapped in while a sync runs) never carries a read or a write out of the root.

use std::fs;

use super::TempDir;
use crate::{
    beneath::{self, FileId, Kind, Root, Stat, Tri},
    Drive, DriveError, LocalDrive,
};
#[cfg(unix)]
use crate::{
    beneath::{BeneathError, Fingerprint},
    local::THROUGH_LINK,
};

/// A root and, beside it, a folder of the user's that must never be touched. The folder outside is
/// read by the tests that plant a link to it, which are the Unix ones (a Windows user makes a link
/// only with a privilege).
struct World {
    root: TempDir,
    #[cfg_attr(not(unix), allow(dead_code))]
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

    #[cfg_attr(not(unix), allow(dead_code))]
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

#[cfg(unix)]
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
fn a_temp_name_of_a_long_name_is_still_a_valid_name() {
    let long = "n".repeat(250);
    let tmp = beneath::temp_name(&long);
    assert!(tmp.len() <= 255, "{}", tmp.len());
    assert!(beneath::is_temp_name(&tmp));
}

/// A `Stat` as a system that gives no file identity (Windows until its file index is read, the
/// records written before ids were kept) shows it: inode and device 0.
fn stat_without_identity(size: u64, mtime_ns: u64) -> Stat {
    Stat {
        kind: Kind::File,
        size,
        mtime_ns,
        ctime_ns: 0,
        ino: 0,
        dev: 0,
        mode: 0o644,
        nlink: 1,
    }
}

#[test]
fn x_ar5a_8_a_file_id_the_system_did_not_give_is_never_the_same_file() {
    // 0 == 0 used to read as "the same file": every file of a system without ids was one file.
    let none = FileId::new(0, 0);
    assert!(!none.is_known());
    assert_eq!(none.same(&FileId::new(0, 0)), Tri::Unknown);
    assert_eq!(none.same(&FileId::UNKNOWN), Tri::Unknown);
    let a = FileId::new(3, 77);
    assert_eq!(a.same(&FileId::new(3, 77)), Tri::Same);
    assert_eq!(a.same(&FileId::new(3, 78)), Tri::Different, "another inode");
    assert_eq!(a.same(&FileId::new(4, 77)), Tri::Different, "another device");
    assert_eq!(a.same(&none), Tri::Unknown, "known against unknown is unknown");
    assert_eq!(none.same(&a), Tri::Unknown);
    assert_eq!(a.known(), Some((3, 77)));
    assert_eq!(none.parts(), (0, 0));

    let st = stat_without_identity(10, 5);
    assert!(!st.same_file(&st), "an entry without an identity is not its own twin");
    assert!(!st.unchanged(&st), "and is never certain to be unchanged");
    assert_eq!(st.fingerprint().matches(&st), Tri::Unknown);
    // What differs is still seen to differ: a size or a time says "not the same" for certain.
    assert_eq!(
        st.fingerprint().matches(&stat_without_identity(11, 5)),
        Tri::Different
    );
    assert_eq!(
        st.fingerprint().matches(&stat_without_identity(10, 6)),
        Tri::Different
    );
    // The answers combine: one "different" wins over an unknown, an unknown over a "same".
    assert_eq!(Tri::Same.and(Tri::Unknown), Tri::Unknown);
    assert_eq!(Tri::Unknown.and(Tri::Different), Tri::Different);
    assert_eq!(Tri::Same.and(Tri::Same), Tri::Same);
    assert!(Tri::Same.is_same() && !Tri::Unknown.is_same() && !Tri::Unknown.is_different());
}

#[cfg(unix)]
#[test]
fn x_ar5a_8_a_fingerprint_is_the_same_only_for_the_known_untouched_file() {
    let w = World::new("fingerprint");
    fs::write(w.root.path().join("f"), b"first").unwrap();
    fs::write(w.root.path().join("g"), b"first").unwrap();
    let top = Root::open(w.root.path()).unwrap().top().unwrap();
    let f = top.lstat("f").unwrap();
    assert!(f.id().is_known(), "a real inode is an identity");
    let print = Fingerprint::of(&f);
    assert_eq!(print.matches(&top.lstat("f").unwrap()), Tri::Same);
    assert!(f.same_file(&top.lstat("f").unwrap()));
    assert!(f.unchanged(&top.lstat("f").unwrap()));
    // Another file with the same size, kind and (nearly) the same time is another file.
    let g = top.lstat("g").unwrap();
    assert!(!f.same_file(&g));
    assert_ne!(print.matches(&Stat { mtime_ns: f.mtime_ns, ..g }), Tri::Same);
    // A write changes the size.
    fs::write(w.root.path().join("f"), b"second!").unwrap();
    assert_eq!(print.matches(&top.lstat("f").unwrap()), Tri::Different);
    // The same stat read back from a record that kept no inode is unknown, not equal.
    let from_record = Fingerprint::from_parts(Kind::File, f.size, f.mtime_ns, f.ctime_ns, f.dev, 0);
    assert_eq!(from_record.matches(&f), Tri::Unknown);
}

/// AZ-W15 (Linux): btrfs gives every subvolume a device number of its own and mounts none of them, so
/// "another device" read as "another disk mounted here" and a synced folder with a subvolume in it
/// skipped it and listed it as a mount. The mount table says which: only a folder it lists is a disk.
#[cfg(unix)]
#[test]
fn az_w15_a_folder_of_another_device_is_a_disk_only_when_the_mount_table_lists_it() {
    use std::path::{Path, PathBuf};
    // A btrfs volume at /data with a subvolume /data/sub (no line of its own) and a disk mounted at
    // "/data/usb stick" (a space is written \040).
    let table = "\
36 29 0:30 / /data rw,relatime shared:1 - btrfs /dev/sda1 rw,subvolid=5
37 36 8:17 / /data/usb\\040stick rw,relatime shared:2 - vfat /dev/sdb1 rw
38 29 0:31 / /proc rw - proc proc rw
";
    assert_eq!(
        beneath::mount_points(table).collect::<Vec<_>>(),
        [
            PathBuf::from("/data"),
            PathBuf::from("/data/usb stick"),
            PathBuf::from("/proc")
        ]
    );
    let is_disk = |path: &str| beneath::is_disk_in(Some(table), Some(Path::new(path)));
    assert!(is_disk("/data/usb stick"), "a disk mounted here");
    assert!(!is_disk("/data/sub"), "a subvolume of the same file system");
    assert!(
        !is_disk("/data/usb"),
        "the beginning of a mount point is not one"
    );
    assert!(!is_disk("/data/usb stick/inner"));
    // A table that could not be read, or a folder that could not be resolved, says nothing: the
    // device number stands and the folder is kept out.
    assert!(beneath::is_disk_in(None, Some(Path::new("/data/sub"))));
    assert!(beneath::is_disk_in(Some(table), None));
}
