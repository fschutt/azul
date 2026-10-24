//! Names Windows cannot take as one name of a path, refused before a local drive's key becomes
//! a path on its disk: a `:` (`C:x` is relative to drive C's current folder, so a path built
//! from it leaves the root; `a.txt:s` writes an alternate data stream), device names (`CON`,
//! `aux.txt`, `COM1`), and a trailing dot or space (Windows drops it, so two names are one
//! file). The key is refused and says why; nothing is renamed.

use std::fs;

use super::TempDir;
use crate::{key::windows_name_problem, Drive, LocalDrive};

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
