use super::TempDir;
use crate::{Drive, DriveError, ListRequest, LocalDrive, ScopedDrive};

fn seeded(tmp: &TempDir) -> LocalDrive {
    let drive = LocalDrive::new(tmp.path());
    drive.put("users/ann/mail/inbox/1.eml", b"one").unwrap();
    drive.put("users/ann/docs/a.txt", b"doc").unwrap();
    drive.put("users/ben/mail/inbox/2.eml", b"two").unwrap();
    drive
}

#[test]
fn a_scoped_drive_lists_relative_to_its_prefix() {
    let tmp = TempDir::new("scoped-list");
    let drive = ScopedDrive::new(seeded(&tmp), "users/ann/", false).unwrap();
    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec!["docs/".to_string(), "mail/".to_string()]);
    let inbox = drive.list(&ListRequest::folder("mail/inbox/")).unwrap();
    assert_eq!(inbox.objects[0].key, "mail/inbox/1.eml");
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"doc");
    assert_eq!(drive.head("docs/a.txt").unwrap().key, "docs/a.txt");
}

#[test]
fn a_read_only_scope_refuses_put_and_delete() {
    let tmp = TempDir::new("scoped-readonly");
    let drive = ScopedDrive::new(seeded(&tmp), "users/ann/", false).unwrap();
    assert!(matches!(
        drive.put("docs/b.txt", b"x"),
        Err(DriveError::Denied { .. })
    ));
    assert!(matches!(
        drive.delete("docs/a.txt"),
        Err(DriveError::Denied { .. })
    ));
    assert!(tmp.path().join("users/ann/docs/a.txt").exists());
    assert!(!tmp.path().join("users/ann/docs/b.txt").exists());
}

#[test]
fn a_writable_scope_writes_under_its_prefix() {
    let tmp = TempDir::new("scoped-write");
    let drive = ScopedDrive::new(seeded(&tmp), "users/ann/", true).unwrap();
    drive.put("docs/b.txt", b"new").unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("users/ann/docs/b.txt")).unwrap(),
        b"new"
    );
}

#[test]
fn a_scoped_key_cannot_climb_out_of_its_prefix() {
    let tmp = TempDir::new("scoped-escape");
    let drive = ScopedDrive::new(seeded(&tmp), "users/ann/", true).unwrap();
    assert!(matches!(
        drive.get("../ben/mail/inbox/2.eml"),
        Err(DriveError::InvalidKey { .. })
    ));
    assert!(matches!(
        drive.list(&ListRequest::folder("../")),
        Err(DriveError::InvalidKey { .. })
    ));
}

#[test]
fn a_scope_prefix_must_be_a_folder() {
    let tmp = TempDir::new("scoped-prefix");
    assert!(ScopedDrive::new(seeded(&tmp), "users/ann", false).is_err());
    assert!(ScopedDrive::new(LocalDrive::new(tmp.path()), "../", false).is_err());
    assert!(ScopedDrive::new(LocalDrive::new(tmp.path()), "", false).is_ok());
}
