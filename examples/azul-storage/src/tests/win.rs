//! What Windows tells that std does not (ARCH28 RF49 / AR5a-9). The model - a handle's numbers as
//! a [`Stat`], what a reparse point makes of an entry, the cap of FAT, the errors that say "busy" -
//! runs here on every host with Windows' own numbers, as the name policy does with
//! `System::Windows`. The tests named `windows_` call the system and run on a Windows machine only:
//! `cargo test -p azul-storage --features encryption windows_`.

use crate::{
    beneath::{self, Fingerprint, Kind, Tri},
    win::{
        fat_cap, file_index, filetime_ns, is_held_open, is_held_open_code, is_sid_string,
        is_transparent_tag, is_under, kind_of, means_no_acls, private_sddl, verbatim_text, Info,
        ATTR_DIRECTORY, ATTR_OFFLINE, ATTR_READONLY, ATTR_RECALL_ON_DATA_ACCESS,
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

const USER: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

/// std converts a long path for its own calls; the wide calls of win.rs (the rename, the free space,
/// the volume, the access list) do it with this: a path past 260 characters is the extended form.
#[test]
fn az_w11_a_long_path_goes_to_the_wide_calls_in_its_extended_length_form() {
    assert_eq!(
        verbatim_text(r"C:\Users\me\sync\a.txt").as_deref(),
        Some(r"\\?\C:\Users\me\sync\a.txt")
    );
    assert_eq!(
        verbatim_text(r"\\server\share\sync\a.txt").as_deref(),
        Some(r"\\?\UNC\server\share\sync\a.txt")
    );
    // Already extended, a device path, a relative path and a drive-relative one need none.
    for none in [
        r"\\?\C:\a",
        r"\\?\UNC\server\share\a",
        r"\\.\pipe\x",
        r"a\b",
        r"C:a",
        "",
    ] {
        assert_eq!(verbatim_text(none), None, "{none:?}");
    }
}

/// AZ-W14: the check that a file opened below the root still lies under it compares resolved paths
/// at a component boundary, without regard to case.
#[test]
fn az_w14_a_resolved_path_is_under_a_root_only_at_a_component_boundary_and_without_regard_to_case()
{
    let root = r"\\?\C:\Users\me\sync";
    for under in [
        r"\\?\C:\Users\me\sync",
        r"\\?\C:\Users\me\sync\a\b.txt",
        r"\\?\c:\users\ME\SYNC\a",
        r"\\?\C:\Users\me\sync\",
    ] {
        assert!(is_under(root, under), "{under}");
    }
    for elsewhere in [
        r"\\?\C:\Users\me\sync2\a",
        r"\\?\C:\Users\me",
        r"\\?\D:\Users\me\sync\a",
        r"\\?\C:\Users\me\syn",
        r"\\?\C:\other\Users\me\sync",
        r"\\?\UNC\evil\share\sync\a",
        "",
    ] {
        assert!(!is_under(root, elsewhere), "{elsewhere:?}");
    }
    // A root that is a whole drive, and one that was written with its closing backslash.
    assert!(is_under(r"\\?\C:\", r"\\?\C:\any\thing"));
    assert!(!is_under(r"\\?\C:\", r"\\?\D:\any"));
    assert!(is_under(
        r"\\?\C:\Users\me\sync\",
        r"\\?\C:\Users\me\sync\a"
    ));
}

#[test]
fn az_w12_what_is_private_is_closed_to_everyone_but_this_user_the_system_and_the_administrators() {
    let folder = private_sddl(USER, true).unwrap();
    assert_eq!(
        folder,
        format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{USER})"),
        "a folder: protected, inherited by what is made in it"
    );
    let file = private_sddl(USER, false).unwrap();
    assert_eq!(
        file,
        format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{USER})"),
        "a file: protected"
    );
    // Three entries and no more: nothing for Everyone, the users or the authenticated users, and
    // "P" - nothing is inherited from the folder above (a data volume's root grants its users read).
    for list in [&folder, &file] {
        assert!(list.starts_with("D:P("), "{list}");
        assert_eq!(list.matches("(A;").count(), 3, "{list}");
        assert!(!list.contains("WD") && !list.contains(";BU)") && !list.contains(";AU)"));
    }
}

#[test]
fn az_w12_only_a_sid_string_is_put_into_an_access_list_and_a_disk_without_lists_is_not_a_refusal() {
    for sid in ["S-1-5-18", "S-1-5-32-544", USER] {
        assert!(is_sid_string(sid), "{sid}");
    }
    for text in [
        "",
        "S-1-",
        "S-1--5",
        "S-1-5-",
        "S-1-5--18",
        "S-1-5-1a",
        "s-1-5-18",
        "S-2-5-18",
        "S-1-5 18",
        // Whatever else could be written into the list's text.
        "S-1-5-21-1-2-3-1001)(A;;FA;;;WD",
    ] {
        assert!(!is_sid_string(text), "{text:?}");
        assert_eq!(private_sddl(text, true), None, "{text:?}");
    }
    // ERROR_INVALID_FUNCTION and ERROR_NOT_SUPPORTED: FAT and exFAT keep no lists. Access denied,
    // not found and a bad parameter are refusals.
    assert!(means_no_acls(1) && means_no_acls(50));
    for code in [0, 2, 5, 87, 1314] {
        assert!(!means_no_acls(code), "{code}");
    }
}

#[cfg(windows)]
mod on_windows {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use super::super::TempDir;
    use crate::{
        beneath::{self, Kind, Root, Tri},
        durable::{self, Mode},
    };

    /// What `icacls` lists for `path`, without the path itself (which names `Users`, too).
    fn access_list(path: &Path) -> String {
        let out = std::process::Command::new("icacls")
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).replace(&*path.to_string_lossy(), "")
    }

    /// A folder under the Public folder - which every user may use, so what is made in it inherits
    /// a list that grants others - removed again when this drops.
    struct Public(PathBuf);

    impl Public {
        fn new(what: &str) -> Public {
            let public = std::env::var_os("PUBLIC")
                .map(PathBuf::from)
                .expect("%PUBLIC%");
            Public(public.join(format!("azul-storage-{what}-{}", std::process::id())))
        }
    }

    impl Drop for Public {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn open_to_others(list: &str) -> bool {
        [
            "Everyone",
            "BUILTIN\\Users",
            "Authenticated Users",
            "INTERACTIVE",
        ]
        .iter()
        .any(|who| list.contains(who))
    }

    /// AZ-W12: a state folder made on a volume that lets every user read inherited that, and its
    /// files with it: `private` was a silent no-op on Windows.
    #[test]
    fn windows_az_w12_a_private_folder_and_what_is_made_in_it_are_closed_to_other_users() {
        let public = Public::new("private-dirs");
        let deeper = public.0.join("state").join("deeper");
        durable::create_private_dirs(&deeper).unwrap();
        for folder in [&public.0, &public.0.join("state"), &deeper] {
            let list = access_list(folder);
            assert!(!open_to_others(&list), "{folder:?}: {list}");
            assert!(
                list.contains("SYSTEM") && list.contains("Administrators"),
                "{list}"
            );
        }
        // What is made in it inherits that, and is this user's to read and write.
        let file = deeper.join("secrets.json");
        durable::replace(&file, b"secret", Mode::Private).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"secret");
        let list = access_list(&file);
        assert!(!open_to_others(&list), "{list}");
        // A folder that is there is not taken over.
        let before = access_list(&deeper);
        durable::create_private_dirs(&deeper).unwrap();
        assert_eq!(access_list(&deeper), before);
    }

    /// AZ-W12, the site that makes folders on the way: a private write into folders that are not
    /// there makes them private (`parent_of`), outermost first, and the file in them is closed too.
    #[test]
    fn windows_az_w12_a_private_write_that_makes_its_folders_makes_them_private() {
        let public = Public::new("private-write");
        let file = public.0.join("a").join("b").join("secrets.json");
        durable::replace(&file, b"s3cret", Mode::Private).unwrap();
        for folder in [
            public.0.clone(),
            public.0.join("a"),
            public.0.join("a").join("b"),
        ] {
            let list = access_list(&folder);
            assert!(!open_to_others(&list), "{folder:?}: {list}");
        }
        assert!(!open_to_others(&access_list(&file)));
        assert_eq!(fs::read(&file).unwrap(), b"s3cret");
    }

    /// AZ-W12, scratch: the root and the purpose's folder, and the folders kept in it, are closed to
    /// other users (scratch holds decrypted copies of files).
    #[test]
    fn windows_az_w12_the_folders_of_scratch_are_closed_to_other_users() {
        let public = Public::new("scratch");
        let scratch = crate::scratch::Scratch::under(&public.0.join("root"), "open");
        scratch.ensure().unwrap();
        let kept = scratch.keep("a tag").unwrap();
        for folder in [
            public.0.join("root"),
            scratch.dir().to_path_buf(),
            kept.clone(),
        ] {
            let list = access_list(&folder);
            assert!(!open_to_others(&list), "{folder:?}: {list}");
        }
        // And a file made there is the user's to read and write.
        let file = scratch.file().unwrap();
        drop(file);
    }

    /// AZ-W12: a private file made in a folder that others may read is closed to them before a byte
    /// of it is written, and a second write over it keeps the list.
    #[test]
    fn windows_az_w12_a_private_file_in_a_folder_others_may_read_is_closed_to_them() {
        let public = Public::new("private-file");
        fs::create_dir_all(&public.0).unwrap();
        let file = public.0.join("secrets.json");
        durable::replace(&file, b"one", Mode::Private).unwrap();
        let list = access_list(&file);
        assert!(!open_to_others(&list), "{list}");
        assert!(
            list.contains("SYSTEM") && list.contains("Administrators"),
            "{list}"
        );
        durable::replace(&file, b"two", Mode::Private).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"two");
        assert!(!open_to_others(&access_list(&file)));
        // A file that asked for no privacy keeps what its folder gives it.
        let shared = public.0.join("shared.txt");
        durable::replace(&shared, b"all", Mode::Shared).unwrap();
        assert!(
            open_to_others(&access_list(&shared)),
            "the control: the folder is open"
        );
    }

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
        assert!(top.child("plain").is_ok());
        assert!(matches!(
            top.child("junction"),
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

    /// A junction (no privilege needed) from `link` to `target`.
    fn junction(link: &Path, target: &Path) {
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
    }

    /// AZ-W14: every step looked at its names with `symlink_metadata` and then went by path, and a
    /// `Dir` that lives for a whole scan re-resolved its path at every call: a folder swapped for a
    /// junction after the walk sent the next read, create, rename or delete into another tree. The
    /// file says where it really is once it is open; a step that opens nothing asks its folder first.
    #[test]
    fn windows_az_w14_a_folder_swapped_for_a_junction_after_the_walk_is_never_used_through() {
        let tmp = TempDir::new("win-swap");
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("a.txt"), b"mine").unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("a.txt"), b"theirs").unwrap();
        fs::write(outside.join("victim.txt"), b"keep").unwrap();

        let tree = Root::open(&root).unwrap();
        let sub = tree.top().unwrap().child("sub").unwrap();
        // Walked as a folder; then the folder moves away and a junction to the other tree takes
        // its name.
        fs::rename(root.join("sub"), root.join("sub.moved")).unwrap();
        junction(&root.join("sub"), &outside);

        let through = |result: Result<(), beneath::BeneathError>| {
            matches!(result, Err(beneath::BeneathError::ThroughLink(_)))
        };
        // Read: the other tree's `a.txt` exists under the same name - the old code opened it.
        assert!(through(sub.open_read("a.txt").map(drop)));
        // Create: refused, and nothing is left in the other tree.
        assert!(through(sub.create_new("planted.txt", 0o644).map(drop)));
        assert!(!outside.join("planted.txt").exists());
        // Steps that open nothing.
        assert!(through(sub.unlink("victim.txt")));
        assert!(through(sub.rename("victim.txt", &sub, "renamed.txt")));
        assert!(through(sub.rename_noreplace(
            "victim.txt",
            &sub,
            "renamed.txt"
        )));
        assert_eq!(fs::read(outside.join("victim.txt")).unwrap(), b"keep");
        assert!(!outside.join("renamed.txt").exists());
        // The control: a folder that was not swapped works as before.
        fs::create_dir_all(root.join("plain")).unwrap();
        let plain = tree.top().unwrap().child("plain").unwrap();
        fs::write(root.join("plain").join("x.txt"), b"x").unwrap();
        assert!(plain.open_read("x.txt").is_ok());
        plain.create_new("y.txt", 0o644).unwrap();
        plain.rename("x.txt", &plain, "z.txt").unwrap();
        plain.unlink("z.txt").unwrap();
    }

    /// std makes a path past 260 characters usable for its own calls; the rename, the free space and
    /// the volume of win.rs call the wide functions themselves, and give them the extended form.
    #[test]
    fn windows_az_w11_a_rename_and_the_free_space_work_in_a_folder_past_260_characters() {
        let tmp = TempDir::new("win-long");
        let mut deep = tmp.path().to_path_buf();
        for i in 0..6 {
            deep.push(format!("{i}{}", "d".repeat(50)));
        }
        assert!(deep.as_os_str().len() > 300, "{}", deep.display());
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("a.txt"), b"a").unwrap();
        let top = Root::open(&deep).unwrap().top().unwrap();
        top.rename("a.txt", &top, "b.txt").unwrap();
        top.rename_noreplace("b.txt", &top, "c.txt").unwrap();
        assert_eq!(fs::read(deep.join("c.txt")).unwrap(), b"a");
        assert!(beneath::free_space(&deep).is_known());
        assert!(crate::win::file_system_name(&deep).is_ok());
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
