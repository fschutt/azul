//! Extended attributes carried with a copy on this computer (X-DELTA-AZDRIVE-FILES-16): a copy and
//! a Move across disks had the bytes, the mode and the time, and not the Finder tags, the labels,
//! the comments or the resource fork - and a Move deleted the source with no message.

use std::path::Path;

use super::TempDir;
use crate::{
    ctl::Quiet,
    xattrs::{carry, carry_with, is_managed, set_for_test, value_for_test},
    xfer::{plan, run, Item},
    Drive, LocalDrive,
};

/// An attribute name every system takes (Linux wants the `user.` namespace).
const NAME: &str = "user.azul-test";

/// Sets the attribute on `path`; `false` when this disk keeps no attributes (a test then has
/// nothing to look at).
fn tagged(path: &Path, value: &[u8]) -> bool {
    set_for_test(path, NAME, value).is_ok()
}

#[test]
fn x_delta_azdrive_files_16_the_attributes_of_a_file_come_with_its_copy() {
    let tmp = TempDir::new("dx16-carry");
    let (from, to) = (tmp.path().join("a.txt"), tmp.path().join("b.txt"));
    std::fs::write(&from, b"bytes").unwrap();
    std::fs::write(&to, b"bytes").unwrap();
    // Nothing to carry: nothing to say.
    assert_eq!(carry(&from, &to), crate::xattrs::Carried::default());
    if !tagged(&from, b"red") {
        return;
    }
    let carried = carry(&from, &to);
    assert_eq!(carried.lost, Vec::<String>::new());
    assert!(carried.carried >= 1, "{carried:?}");
    assert_eq!(value_for_test(&to, NAME).unwrap(), b"red");
}

#[test]
fn x_delta_azdrive_files_16_an_attribute_the_disk_refuses_is_named_as_lost() {
    let tmp = TempDir::new("dx16-lost");
    let (from, to) = (tmp.path().join("a.txt"), tmp.path().join("b.txt"));
    std::fs::write(&from, b"bytes").unwrap();
    std::fs::write(&to, b"bytes").unwrap();
    if !tagged(&from, b"red") {
        return;
    }
    let refused =
        |_: &Path, _: &[u8], _: &[u8]| Err(std::io::Error::from(std::io::ErrorKind::Unsupported));
    let carried = carry_with(&from, &to, &refused);
    assert_eq!(carried.carried, 0);
    assert_eq!(carried.lost, vec![String::from(NAME)]);
}

/// What the system manages itself is neither carried nor lost.
#[test]
fn x_delta_azdrive_files_16_what_the_system_manages_is_not_the_copys_to_carry() {
    for managed in [
        &b"com.apple.provenance"[..],
        b"com.apple.macl",
        b"com.apple.rootless",
        b"security.selinux",
        b"security.capability",
    ] {
        assert!(is_managed(managed), "{}", String::from_utf8_lossy(managed));
    }
    for carried in [
        &b"com.apple.metadata:_kMDItemUserTags"[..],
        b"com.apple.FinderInfo",
        b"com.apple.ResourceFork",
        b"com.apple.quarantine",
        b"user.xdg.origin.url",
        b"system.posix_acl_access",
    ] {
        assert!(!is_managed(carried), "{}", String::from_utf8_lossy(carried));
    }
}

/// A Move keeps the source whose attributes the copy lacks; a Copy says it; a whole copy is silent.
#[test]
fn x_delta_azdrive_files_16_a_move_keeps_a_source_whose_attributes_the_copy_lacks() {
    use crate::xfer::run::{attrs_outcome, AttrsOutcome};
    let lost = vec![String::from("com.apple.metadata:_kMDItemUserTags")];
    assert_eq!(attrs_outcome(&[], true, "a.jpg"), AttrsOutcome::Whole);
    assert_eq!(attrs_outcome(&[], false, "a.jpg"), AttrsOutcome::Whole);
    match attrs_outcome(&lost, true, "a.jpg") {
        AttrsOutcome::KeepSource(why) => {
            assert!(
                why.contains("a.jpg") && why.contains("_kMDItemUserTags"),
                "{why}"
            );
            assert!(why.contains("kept where it was"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    match attrs_outcome(&lost, false, "a.jpg") {
        AttrsOutcome::Note(note) => assert!(note.contains("a.jpg"), "{note}"),
        other => panic!("{other:?}"),
    }
}

/// Through `xfer::run`, the function a Move calls: the copy has the tag and the source is retired.
#[test]
fn x_delta_azdrive_files_16_a_move_between_two_disks_brings_the_attribute_and_retires_the_source() {
    let tmp = TempDir::new("dx16-move");
    let home = LocalDrive::without_manifest(tmp.path().join("home"));
    let other = LocalDrive::without_manifest(tmp.path().join("other"));
    home.put("photo.jpg", b"jpeg bytes").unwrap();
    let source_path = tmp.path().join("home").join("photo.jpg");
    if !tagged(&source_path, b"holiday") {
        return;
    }
    let item = [Item {
        key: String::from("photo.jpg"),
        is_folder: false,
        size: None,
    }];
    let planned = plan(&home, &item, &other, "", true, "Copy").unwrap();
    let report = run(&planned, &home, &other, true, &Quiet, &mut |_| {});
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let copy = tmp.path().join("other").join("photo.jpg");
    assert_eq!(value_for_test(&copy, NAME).unwrap(), b"holiday");
    assert!(
        home.get("photo.jpg").is_err(),
        "the source went: nothing was lost"
    );
}
