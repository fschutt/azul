//! One device's state folder: the device minted once, the lock, the folders a sync may not
//! take, the atomic (and private) writes.

use std::{fs, time::Duration};

use azul_storage::testing::TempDir;

use crate::state::{clean_device_name, write_atomic, OutFile, StateDir};

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

/// AZ-W12, the kit's sites: the state folder `create_private_dir` makes and the private file
/// `write_atomic` puts in it were `rwx------` / `rw-------` on Unix and nothing on Windows. A state
/// folder on a volume that lets every user read (`--state-dir D:\\az`) inherited that.
#[cfg(windows)]
#[test]
fn windows_az_w12_the_state_folder_and_a_private_file_in_it_are_closed_to_other_users() {
    use crate::state::create_private_dir;
    struct RemoveOnDrop(std::path::PathBuf);
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let list_of = |path: &std::path::Path| -> String {
        let out = std::process::Command::new("icacls")
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).replace(&*path.to_string_lossy(), "")
    };
    let open_to_others = |list: &str| {
        [
            "Everyone",
            "BUILTIN\\Users",
            "Authenticated Users",
            "INTERACTIVE",
        ]
        .iter()
        .any(|who| list.contains(who))
    };
    // A folder under the Public folder, whose list a new folder would inherit.
    let public = std::path::PathBuf::from(std::env::var_os("PUBLIC").expect("%PUBLIC%"))
        .join(format!("azcloud-kit-state-{}", std::process::id()));
    let _clean = RemoveOnDrop(public.clone());
    let state = public.join("state");
    create_private_dir(&state.join("sync")).unwrap();
    for folder in [&public, &state, &state.join("sync")] {
        let list = list_of(folder);
        assert!(!open_to_others(&list), "{folder:?}: {list}");
    }
    let secrets = state.join("secrets.json");
    write_atomic(&secrets, b"one", true).unwrap();
    write_atomic(&secrets, b"two", true).unwrap();
    assert_eq!(fs::read(&secrets).unwrap(), b"two");
    assert!(!open_to_others(&list_of(&secrets)));
}

/// X-DELTA-AZDRIVE-FILES-28 (AZ-W12): the journal of intents and the operations log were opened with a
/// Unix-only `mode(0o600)`; in a state folder that existed before, a new one inherited the folder's
/// list on Windows, and the oplog names every file. A new append-only file is made private the
/// moment it is made (on Windows by an access list of its own), and one that is there is appended
/// to, never truncated or replaced.
#[test]
fn x_delta_azdrive_files_28_a_new_append_only_file_is_private_in_a_folder_that_was_there() {
    use std::io::Write;

    use crate::state::open_private_append;
    let dir = TempDir::new("dx28-append");
    // A state folder that existed before (the default mode of its maker), not made private by us.
    let state = dir.path().join("state");
    fs::create_dir_all(&state).unwrap();
    let path = state.join("journal.jsonl");
    let mut file = open_private_append(&path).unwrap();
    file.write_all(b"one\n").unwrap();
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut again = open_private_append(&path).unwrap();
    again.write_all(b"two\n").unwrap();
    drop(again);
    assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    // A name that cannot be made is an error, not a file somewhere else.
    assert!(open_private_append(&state.join("missing").join("journal.jsonl")).is_err());
}

/// X-DELTA-KEYS2-4: the file a new recovery code is to go into is made first, empty and readable by
/// this user only, so that a path that cannot be written is found out before the first call of the
/// token server or the bucket; it goes again when the command ends with it still empty; a file that
/// was there is proven writable and stays as it is; the code goes in through the atomic private
/// write.
#[test]
fn x_delta_keys2_4_an_out_file_is_made_first_and_goes_again_when_nothing_was_written() {
    let dir = TempDir::new("azcloud-out");
    // Below a file: no folder can be made for it.
    let file = dir.path().join("a-file");
    fs::write(&file, "x").unwrap();
    assert!(
        OutFile::prepare(&file.join("code.txt")).is_err(),
        "refused at once"
    );

    let path = dir.path().join("code.txt");
    let out = OutFile::prepare(&path).unwrap();
    assert_eq!(out.path(), path);
    assert_eq!(fs::metadata(&path).unwrap().len(), 0, "empty");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "this user's only: {mode:o}");
    }
    drop(out);
    assert!(!path.exists(), "nothing was written into it: it goes again");

    let out = OutFile::prepare(&path).unwrap();
    out.write(b"the code\n").unwrap();
    drop(out);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "the code\n",
        "a file with the secret stays"
    );

    // An existing file is not touched by the preparation or by the end of a command that wrote
    // nothing into it.
    let again = OutFile::prepare(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "the code\n");
    drop(again);
    assert!(path.exists());
}

/// X-DELTA-KEYS2-4: a path that was proven writable and is not any more - a folder stands in its
/// place - answers the write with an error (the caller shows the secret instead) and leaves no
/// empty file behind for a path that is a folder.
#[test]
fn x_delta_keys2_4_an_out_file_that_cannot_be_written_any_more_says_so() {
    let dir = TempDir::new("azcloud-out-gone");
    let path = dir.path().join("code.txt");
    let out = OutFile::prepare(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(out.write(b"the code\n").is_err());
    drop(out);
    assert!(path.is_dir(), "a folder that is not the file it made stays");
}
