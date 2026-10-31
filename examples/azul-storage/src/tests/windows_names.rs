//! Names Windows cannot take as one name of a path, refused before a local drive's key becomes
//! a path on its disk: a `:` (`C:x` is relative to drive C's current folder, so a path built
//! from it leaves the root; `a.txt:s` writes an alternate data stream), device names (`CON`,
//! `aux.txt`, `COM1`), and a trailing dot or space (Windows drops it, so two names are one
//! file). The key is refused and says why; nothing is renamed.

use std::fs;

use super::TempDir;
use crate::{
    key::{check_path_key_on, windows_name_problem, System},
    Drive, LocalDrive,
};

/// A key becomes a path on Windows only when every name of it is one Windows takes as it is
/// meant: the same table on every computer that checks a key for Windows, so a name made on
/// macOS never writes a stream, a drive-relative path or a device there. macOS and Linux take
/// these names as they are.
#[test]
fn a_key_that_becomes_a_path_on_windows_never_names_a_stream_a_drive_or_a_device() {
    for key in [
        "a.txt:stream",
        "dir/C:evil.txt",
        "notes/12:30.txt",
        "x:",
        "CON",
        "dir/aux.txt",
        "nul.tar.gz",
        "COM1.log",
        "trailing.",
        "dir./x",
        "dir/space ",
        "a<b",
        "q?.txt",
        "tab\there",
    ] {
        assert!(check_path_key_on(key, System::Windows).is_err(), "{key:?}");
        for system in [System::Macos, System::Linux] {
            assert!(check_path_key_on(key, system).is_ok(), "{key:?} on {system:?}");
        }
    }
    // The shape of a key is the same everywhere.
    for key in ["a/../b", "/abs", "C:/x", "a\\b", "nul\0", "a//b", "./a"] {
        for system in System::ALL {
            assert!(check_path_key_on(key, system).is_err(), "{key:?} on {system:?}");
        }
    }
    for key in ["a.txt", "console.txt", "dir/COM10", ".hidden/x", "na\u{ef}ve.md"] {
        for system in System::ALL {
            assert!(check_path_key_on(key, system).is_ok(), "{key:?} on {system:?}");
        }
    }
}

/// Every caller that only asks whether a key may be a path here (a bucket in a folder, a
/// migration, a share) is covered on Windows: the check itself refuses what Windows misreads.
#[cfg(windows)]
#[test]
fn the_path_key_check_on_windows_refuses_a_stream_and_a_drive_relative_name() {
    for key in ["a.txt:stream", "dir/C:x", "CON", "trailing."] {
        assert!(crate::key::check_path_key(key).is_err(), "{key:?}");
    }
}

#[test]
fn windows_refuses_a_colon_a_device_name_and_a_trailing_dot_or_space_in_a_name() {
    for name in [
        "C:x",
        "a.txt:stream",
        "x:",
        "CON",
        "con",
        "aux.txt",
        "nul.tar.gz",
        "CON .txt",
        "COM1",
        "lpt9.log",
        "Com\u{b9}",
        "conin$",
        "name.",
        "name ",
        "a<b",
        "a>b",
        "a\"b",
        "a|b",
        "a?b",
        "a*b",
        "tab\there",
    ] {
        assert!(windows_name_problem(name).is_some(), "{name:?}");
    }
    for name in [
        "a.txt",
        "console.txt",
        "CONFIG",
        "com10",
        "lpt",
        "auxiliary",
        ".hidden",
        "na\u{ef}ve.md",
        "a b.txt",
    ] {
        assert!(windows_name_problem(name).is_none(), "{name:?}");
    }
}

#[cfg(not(windows))]
#[test]
fn a_colon_stays_a_valid_name_where_the_system_takes_it() {
    let root = TempDir::new("windows-names-elsewhere");
    let drive = LocalDrive::without_manifest(root.path());
    drive.put("notes/12:30.txt", b"lunch").unwrap();
    assert_eq!(drive.get("notes/12:30.txt").unwrap(), b"lunch");
    let contained = LocalDrive::without_manifest(root.path()).contained();
    contained.put("aux.txt", b"aux").unwrap();
    assert_eq!(contained.get("aux.txt").unwrap(), b"aux");
    assert_eq!(fs::read_dir(root.path().join("notes")).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn a_local_drive_on_windows_refuses_such_a_key_and_writes_nothing() {
    use crate::{
        beneath::{BeneathError, Root},
        DriveError,
    };

    let root = TempDir::new("windows-names");
    for contained in [false, true] {
        let mut drive = LocalDrive::without_manifest(root.path());
        if contained {
            drive = drive.contained();
        }
        for key in [
            "dir/C:evil.txt",
            "a.txt:stream",
            "CON",
            "dir/aux.txt",
            "trailing.",
            "dir/space ",
        ] {
            assert!(
                matches!(drive.put(key, b"x"), Err(DriveError::InvalidKey { .. })),
                "{key}"
            );
        }
        assert!(drive.create_folder("C:x/").is_err());
    }
    assert!(
        fs::read_dir(root.path()).unwrap().next().is_none(),
        "nothing was made in the root"
    );
    let tree = Root::open(root.path()).unwrap();
    assert!(matches!(
        tree.dir("C:x", true),
        Err(BeneathError::BadName(_))
    ));
    assert!(matches!(
        tree.parent_of("a.txt:s", false),
        Err(BeneathError::BadName(_))
    ));
}
