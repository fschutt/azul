//! Quarantine marks on other people's files.

use azul_storage::testing::TempDir;

use crate::sync::quarantine::{is_marked, macos_value, mark, supported, OwnDevices, Writers};

#[test]
fn a_file_marked_carries_the_systems_mark_where_the_system_has_one() {
    let dir = TempDir::new("quarantine");
    let path = dir.path().join("tool.command");
    std::fs::write(&path, b"#!/bin/sh\necho hi\n").unwrap();
    assert!(!is_marked(&path));
    let set = mark(&path, 1_761_300_000).unwrap();
    assert_eq!(set, supported());
    assert_eq!(is_marked(&path), supported());
}

#[test]
fn the_macos_value_names_a_download_its_time_and_the_agent() {
    assert_eq!(macos_value(0x68fa_0000, "ev"), "0081;68fa0000;AzDrive;ev");
}

#[test]
fn only_other_peoples_devices_need_the_mark() {
    let own = OwnDevices(vec![String::from("laptop"), String::from("desk")]);
    assert!(own.is_own("desk"));
    assert!(!own.is_own("stranger"));
}
