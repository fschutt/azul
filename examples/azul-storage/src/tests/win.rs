//! What Windows tells that std does not (ARCH28 RF49 / AR5a-9). The model - a handle's numbers as
//! a [`Stat`], what a reparse point makes of an entry, the cap of FAT, the errors that say "busy" -
//! runs here on every host with Windows' own numbers, as the name policy does with
//! `System::Windows`. The tests named `windows_` call the system and run on a Windows machine only:
//! `cargo test -p azul-storage --features encryption windows_`.

use crate::{
    beneath::{self, Fingerprint, Kind, Tri},
    win::{
        fat_cap, file_index, filetime_ns, is_held_open, is_held_open_code, is_transparent_tag,
        kind_of, Info, ATTR_DIRECTORY, ATTR_OFFLINE, ATTR_READONLY, ATTR_RECALL_ON_DATA_ACCESS,
        ATTR_REPARSE_POINT, FAT_FILE_MAX, HELD_BACKOFF_MS,
    },
};

/// 1970-01-01 as a FILETIME.
const EPOCH: i64 = 116_444_736_000_000_000;

const SYMLINK: u32 = 0xA000_000C;
const MOUNT_POINT: u32 = 0xA000_0003;
const WOF: u32 = 0x8000_0017;
const DEDUP: u32 = 0x8000_0013;
const CLOUD: u32 = 0x9000_001A;
const CLOUD_F: u32 = 0x9000_F01A;
const APPEXECLINK: u32 = 0x8000_001B;
const PROJFS: u32 = 0x9000_001C;
const FILE: u32 = 0x20;

fn file_info() -> Info {
    Info {
        attributes: FILE,
        reparse_tag: 0,
        size: 10,
        written: EPOCH + 5_000_000,
        changed: EPOCH + 9_000_000,
        volume: 0xAB12_CD34,
        index: 0x0003_0000_0000_0042,
        wide_id: false,
        links: 1,
    }
}

#[test]
fn az_w6_the_numbers_of_a_handle_are_an_identity_and_a_change_time_not_the_old_zeros() {
    let info = file_info();
    let st = info.stat();
    assert_eq!(st.kind, Kind::File);
    // The volume serial number and the file index; the times as nanoseconds since 1970.
    assert_eq!((st.dev, st.ino), (0xAB12_CD34, 0x0003_0000_0000_0042));
    assert!(
        st.id().is_known(),
        "Windows files used to have ino = dev = 0"
    );
    assert_eq!(st.mtime_ns, 500_000_000, "5 000 000 ticks of 100 ns");
    assert_eq!(
        st.ctime_ns, 900_000_000,
        "the change time, which a copy of the time cannot set back"
    );
    assert_eq!((st.size, st.nlink, st.mode), (10, 1, 0o644));
    // The same file looked at twice is the same file, by the comparison production uses.
    let print: Fingerprint = st.fingerprint();
    assert_eq!(print.matches(&info.stat()), Tri::Same);
    // Another file of the volume, a write that kept the size and the time (only the change time
    // moved), and a file renamed over by another one are each a difference.
    assert_eq!(
        print.matches(
            &Info {
                index: 0x43,
                ..info
            }
            .stat()
        ),
        Tri::Different
    );
    assert_eq!(
        print.matches(
            &Info {
                changed: info.changed + 1,
                ..info
            }
            .stat()
        ),
        Tri::Different,
        "an edit that kept the size and the modification time was never re-read"
    );
    assert_eq!(
        print.matches(
            &Info {
                volume: 0x1234,
                ..info
            }
            .stat()
        ),
        Tri::Different
    );
}

#[test]
fn az_w6_a_file_without_an_index_or_with_an_id_wider_than_it_has_no_identity_and_is_never_the_same()
{
    let print = file_info().stat().fingerprint();
    // No index (a file system that gives none) and ReFS' 128-bit ids both read as "unknown", never
    // as an identity two files could share.
    for unknown in [
        Info {
            index: 0,
            ..file_info()
        },
        Info {
            wide_id: true,
            ..file_info()
        },
    ] {
        let st = unknown.stat();
        assert_eq!((st.dev, st.ino), (0, 0));
        assert!(!st.id().is_known());
        assert_eq!(print.matches(&st), Tri::Unknown);
        assert!(
            !st.same_file(&st),
            "two entries with no identity are not the same file"
        );
    }
    // A change time the system did not give (0) is not compared, as on a Unix disk without one.
    let no_change_time = Info {
        changed: 0,
        ..file_info()
    };
    assert_eq!(no_change_time.stat().ctime_ns, 0);
    assert_eq!(print.matches(&no_change_time.stat()), Tri::Same);
}

#[test]
fn az_w6_file_times_count_from_1970_and_never_wrap() {
    assert_eq!(filetime_ns(EPOCH), 0);
    assert_eq!(filetime_ns(EPOCH + 1), 100);
    assert_eq!(filetime_ns(EPOCH + 10_000_000), 1_000_000_000, "a second");
    assert_eq!(filetime_ns(0), 0, "1601");
    assert_eq!(filetime_ns(-5), 0);
    assert_eq!(filetime_ns(i64::MAX), u64::MAX, "saturates, never wraps");
    assert_eq!(file_index(0x0003_0000, 0x42), 0x0003_0000_0000_0042);
    assert_eq!(file_index(u32::MAX, u32::MAX), u64::MAX);
}

#[test]
fn az_w15_a_link_is_a_link_and_what_a_filter_serves_reads_as_itself() {
    let link = |attributes, tag| kind_of(attributes | ATTR_REPARSE_POINT, tag);
    // The reparse points that name another object: never read, never walked into.
    for tag in [SYMLINK, MOUNT_POINT, APPEXECLINK, PROJFS, 0, 0x1234_5678] {
        assert_eq!(link(FILE, tag), Kind::Link, "file {tag:#x}");
        assert_eq!(link(ATTR_DIRECTORY, tag), Kind::Link, "folder {tag:#x}");
    }
    // Compressed by the system, deduplicated, or hydrated by a sync provider: the file itself.
    for tag in [WOF, DEDUP, CLOUD, CLOUD_F] {
        assert!(is_transparent_tag(tag), "{tag:#x}");
        assert_eq!(link(FILE, tag), Kind::File, "file {tag:#x}");
        assert_eq!(link(ATTR_DIRECTORY, tag), Kind::Dir, "folder {tag:#x}");
    }
    // No tag is "transparent" when it names another object.
    assert!(!is_transparent_tag(SYMLINK) && !is_transparent_tag(MOUNT_POINT));
    assert_eq!(kind_of(FILE, 0), Kind::File);
    assert_eq!(kind_of(ATTR_DIRECTORY, 0), Kind::Dir);
    assert_eq!(kind_of(FILE | ATTR_READONLY, 0), Kind::File);
}

#[test]
fn az_w15_a_file_that_is_not_on_this_computer_is_not_read_to_be_hashed() {
    // A cloud placeholder that reading downloads, or an offline file that reading recalls: hashing
    // every one of a OneDrive folder would download all of it. It is held back as a link is.
    assert_eq!(
        kind_of(
            FILE | ATTR_REPARSE_POINT | ATTR_RECALL_ON_DATA_ACCESS,
            CLOUD
        ),
        Kind::Link
    );
    assert_eq!(
        kind_of(FILE | ATTR_REPARSE_POINT | ATTR_OFFLINE, CLOUD_F),
        Kind::Link
    );
    assert_eq!(kind_of(FILE | ATTR_OFFLINE, 0), Kind::Link);
    // A folder placeholder is a folder: listing it asks for names, not for bytes.
    assert_eq!(
        kind_of(
            ATTR_DIRECTORY | ATTR_REPARSE_POINT | ATTR_RECALL_ON_DATA_ACCESS,
            CLOUD
        ),
        Kind::Dir
    );
    // The same file hydrated (the attributes are gone) is read.
    assert_eq!(kind_of(FILE | ATTR_REPARSE_POINT, CLOUD), Kind::File);
}

#[test]
fn az_w3_fat_holds_less_than_four_gibibytes_and_nothing_else_is_capped() {
    assert_eq!(FAT_FILE_MAX, 4 * 1024 * 1024 * 1024 - 1);
    for name in ["FAT32", "fat32", "FAT", " FAT32 "] {
        assert_eq!(fat_cap(name), Some(FAT_FILE_MAX), "{name:?}");
    }
    for name in ["NTFS", "exFAT", "ReFS", "CSVFS", "", "FAT32x"] {
        assert_eq!(fat_cap(name), None, "{name:?}");
    }
}

#[test]
fn az_w11_a_busy_file_is_told_from_a_broken_one_and_the_wait_for_it_is_bounded() {
    // ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION: another program has the file.
    assert!(is_held_open_code(32) && is_held_open_code(33));
    // Access denied, not found, already exists, disk full: something else.
    for code in [0, 2, 3, 5, 80, 112, 183, 1224] {
        assert!(!is_held_open_code(code), "{code}");
    }
    // The same numbers on a system that is not Windows are other errors (EPIPE, EDOM).
    let e32 = std::io::Error::from_raw_os_error(32);
    assert_eq!(is_held_open(&e32), cfg!(windows));
    assert_eq!(beneath::is_held_open(&e32), cfg!(windows));
    // A rename another program holds up waits for it five times, a second in all - not for ever.
    assert_eq!(HELD_BACKOFF_MS.len(), 5);
    assert!(
        HELD_BACKOFF_MS.iter().sum::<u64>() <= 1_500,
        "{HELD_BACKOFF_MS:?}"
    );
    assert!(
        HELD_BACKOFF_MS.windows(2).all(|w| w[0] < w[1]),
        "each wait longer than the last"
    );
}

#[cfg(windows)]
mod on_windows {
    use std::fs;

    use super::super::TempDir;
    use crate::beneath::{self, Kind, Root, Tri};

    /// The system gives a file an identity: another name of it (a hard link) is the same file, a
    /// copy of it is another, and the rename of a file keeps its identity.
    #[test]
    fn windows_az_w6_a_file_has_an_identity_a_link_and_a_rename_keep_and_a_copy_does_not() {
        let tmp = TempDir::new("win-identity");
        let a = tmp.path().join("a.txt");
        fs::write(&a, b"one").unwrap();
        fs::hard_link(&a, tmp.path().join("link.txt")).unwrap();
        fs::copy(&a, tmp.path().join("copy.txt")).unwrap();
        let tree = Root::open(tmp.path()).unwrap();
        let top = tree.top().unwrap();
        let (sa, slink, scopy) = (
            top.lstat("a.txt").unwrap(),
            top.lstat("link.txt").unwrap(),
            top.lstat("copy.txt").unwrap(),
        );
        assert!(sa.id().is_known(), "{sa:?}");
        assert!(sa.ctime_ns != 0, "the change time is read: {sa:?}");
        assert_eq!(sa.id().same(&slink.id()), Tri::Same);
        assert_eq!(sa.id().same(&scopy.id()), Tri::Different);
        assert_eq!(sa.nlink, 2);
        // An open handle tells the same as the path.
        let (file, opened) = top.open_read("a.txt").unwrap();
        assert_eq!(opened.id().same(&sa.id()), Tri::Same);
        drop(file);
        // A rename keeps the file; a case-only rename too.
        top.rename("a.txt", &top, "A2.txt").unwrap();
        assert_eq!(top.lstat("A2.txt").unwrap().id().same(&sa.id()), Tri::Same);
        top.rename("A2.txt", &top, "a2.txt").unwrap();
        assert_eq!(top.lstat("a2.txt").unwrap().id().same(&sa.id()), Tri::Same);
    }

    /// A junction is a link and is never walked through; a folder beside it is a folder.
    #[test]
    fn windows_az_w15_a_junction_is_a_link_and_a_plain_folder_is_not() {
        let tmp = TempDir::new("win-junction");
        let target = tmp.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::create_dir(tmp.path().join("plain")).unwrap();
        let junction = tmp.path().join("junction");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        let tree = Root::open(tmp.path()).unwrap();
        let top = tree.top().unwrap();
        assert_eq!(top.lstat("junction").unwrap().kind, Kind::Link);
        assert_eq!(top.lstat("plain").unwrap().kind, Kind::Dir);
        assert!(top.child("plain", false).is_ok());
        assert!(matches!(
            top.child("junction", false),
            Err(beneath::BeneathError::ThroughLink(_))
        ));
    }

    /// The free space of a folder is a number, and the file system of its disk has a name.
    #[test]
    fn windows_az_w3_the_free_space_of_a_folder_is_known_and_its_file_system_named() {
        let tmp = TempDir::new("win-free-space");
        match beneath::free_space(tmp.path()) {
            crate::cap::Cap::Known(bytes) => assert!(bytes > 0),
            other => panic!("the disk of a folder says its free space: {other:?}"),
        }
        let name = crate::win::file_system_name(tmp.path()).unwrap();
        assert!(!name.is_empty(), "{name:?}");
        // The cap of the file system it names: FAT only.
        assert_eq!(
            beneath::file_size_limit(tmp.path()).is_some(),
            crate::win::fat_cap(&name).is_some()
        );
    }

    /// A virus scanner or an indexer has the new file open for a moment, without sharing delete:
    /// the rename fails with a sharing violation and used to give up on the first. It waits.
    #[test]
    fn windows_az_w11_a_rename_waits_for_a_program_that_has_the_file_open_for_a_moment() {
        use std::os::windows::fs::OpenOptionsExt;
        let tmp = TempDir::new("win-rename-held");
        let top = Root::open(tmp.path()).unwrap().top().unwrap();
        fs::write(tmp.path().join("new.tmp"), b"new").unwrap();
        // FILE_SHARE_READ | FILE_SHARE_WRITE: the holder does not let the file be renamed.
        let holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(tmp.path().join("new.tmp"))
            .unwrap();
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            drop(holder);
        });
        top.rename("new.tmp", &top, "final.txt")
            .expect("the rename waited for the holder");
        letting_go.join().unwrap();
        assert_eq!(fs::read(tmp.path().join("final.txt")).unwrap(), b"new");
    }

    /// A rename that must not replace takes the name in one step or says `AlreadyExists`; one that
    /// replaces does, a read-only target included.
    #[test]
    fn windows_az_w11_a_rename_takes_a_free_name_replaces_when_asked_and_never_otherwise() {
        let tmp = TempDir::new("win-rename");
        let top = Root::open(tmp.path()).unwrap().top().unwrap();
        fs::write(tmp.path().join("new.tmp"), b"new").unwrap();
        fs::write(tmp.path().join("taken"), b"old").unwrap();
        let refused = top.rename_noreplace("new.tmp", &top, "taken").unwrap_err();
        assert!(refused.is_already_exists(), "{refused:?}");
        assert_eq!(fs::read(tmp.path().join("taken")).unwrap(), b"old");
        assert!(
            tmp.path().join("new.tmp").exists(),
            "the source is left where it was"
        );
        top.rename_noreplace("new.tmp", &top, "free").unwrap();
        assert_eq!(fs::read(tmp.path().join("free")).unwrap(), b"new");
        // Replace over a read-only file.
        fs::write(tmp.path().join("again.tmp"), b"again").unwrap();
        let mut permissions = fs::metadata(tmp.path().join("taken"))
            .unwrap()
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(tmp.path().join("taken"), permissions).unwrap();
        top.rename("again.tmp", &top, "taken").unwrap();
        assert_eq!(fs::read(tmp.path().join("taken")).unwrap(), b"again");
    }
}
