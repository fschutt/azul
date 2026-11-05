//! One device's state folder: the device minted once, the lock, the folders a sync may not
//! take, the atomic (and private) writes.

use std::{fs, time::Duration};

use azul_storage::testing::TempDir;

use crate::state::{clean_device_name, write_atomic, StateDir};

#[test]
fn a_device_is_minted_once_and_its_name_is_safe_in_a_file_name() {
    let dir = TempDir::new("azcloud-device");
    let state = StateDir::open(&dir.path().join("state")).unwrap();
    let first = state.device(Some("Ann's MacBook / Pro")).unwrap();
    assert!(first.id.starts_with("dev-"), "{first:?}");
    assert_eq!(first.id.len(), "dev-".len() + 10, "{first:?}");
    assert_eq!(first.name, "Ann-s-MacBook-Pro");
    let again = state.device(Some("other")).unwrap();
    assert_eq!(again, first, "the second call reads it");
    assert_eq!(clean_device_name("  "), None);
    assert_eq!(clean_device_name("a/../b").as_deref(), Some("a-..-b"));
    assert_eq!(
        clean_device_name(&"x".repeat(80)).map(|n| n.len()),
        Some(32)
    );
}

#[test]
fn two_devices_get_two_ids() {
    let one = TempDir::new("azcloud-device-one");
    let two = TempDir::new("azcloud-device-two");
    let a = StateDir::open(one.path()).unwrap().device(None).unwrap();
    let b = StateDir::open(two.path()).unwrap().device(None).unwrap();
    assert_ne!(a.id, b.id);
}

#[test]
fn a_lock_is_exclusive_until_dropped() {
    let dir = TempDir::new("azcloud-lock");
    let state = StateDir::open(dir.path()).unwrap();
    let held = state.lock("refresh", Duration::ZERO).unwrap();
    assert!(state.lock("refresh", Duration::ZERO).is_err());
    drop(held);
    assert!(state.lock("refresh", Duration::ZERO).is_ok());
}

#[test]
fn a_folder_that_holds_the_state_folder_or_lies_in_it_overlaps() {
    let dir = TempDir::new("azcloud-overlap");
    let state = StateDir::open(&dir.path().join("home").join("state")).unwrap();
    assert!(state.overlaps(&dir.path().join("home")));
    assert!(state.overlaps(&dir.path().join("home").join("state").join("sync")));
    assert!(!state.overlaps(&dir.path().join("data")));
    assert!(!state.overlaps(&dir.path().join("home").join("stateful")));
}

#[test]
fn an_atomic_write_replaces_the_file_whole_and_a_private_one_is_the_users_only() {
    let dir = TempDir::new("azcloud-write");
    let path = dir.path().join("a").join("secrets.json");
    write_atomic(&path, b"one", true).unwrap();
    write_atomic(&path, b"two", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"two");
    let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no temporary file is left");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}

#[test]
fn az_s19_a_state_file_that_cannot_be_replaced_leaves_the_old_one_and_no_temporary_file() {
    // The base, the device and the outbox keys all go in through `write_atomic`, which is
    // `azul_storage::durable::replace`: file synced, renamed, folder synced - or nothing.
    let dir = TempDir::new("azcloud-s19");
    let state = dir.path().join("state");
    let path = state.join("device.json");
    write_atomic(&path, b"first", false).unwrap();
    // A folder in the file's place: the rename is refused, as a full disk refuses a write.
    let blocked = state.join("blocked.json");
    fs::create_dir_all(blocked.join("inner")).unwrap();
    assert!(write_atomic(&blocked, b"new", false).is_err());
    assert!(blocked.is_dir(), "the folder in the way is not replaced");
    let mut names: Vec<String> = fs::read_dir(&state)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["blocked.json", "device.json"],
        "no temporary file of either name convention is left"
    );
    assert_eq!(fs::read(&path).unwrap(), b"first");
    // A shared file is readable by others as a new file is; a private one is not.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(bits(&path) & 0o600, 0o600);
        write_atomic(&state.join("keys.json"), b"k", true).unwrap();
        assert_eq!(bits(&state.join("keys.json")), 0o600);
    }
}
