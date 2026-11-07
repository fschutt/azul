//! The secrets file: entries under the OS keyring's names, errors that never show a value.

use azul_storage::{
    keyring::{KeyringError, KeyringStore},
    testing::TempDir,
};

use crate::secrets::{credentials_entry, drive_token_entry, FileSecrets};

#[test]
fn the_secrets_file_is_a_keyring_for_the_command_lines_drive_keys() {
    let dir = TempDir::new("azcloud-secrets-keyring");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    let keyring: &dyn KeyringStore = &secrets;
    assert_eq!(keyring.get("azul-storage/drive-key/d_1").unwrap(), None);
    keyring
        .set("azul-storage/drive-key/d_1", "{\"kind\": \"drive-key\"}")
        .unwrap();
    assert_eq!(
        secrets.get("azul-storage/drive-key/d_1").unwrap().as_deref(),
        Some("{\"kind\": \"drive-key\"}"),
        "the same entry the file keeps"
    );
    keyring.delete("azul-storage/drive-key/d_1").unwrap();
    keyring.delete("azul-storage/drive-key/d_1").unwrap();
    assert_eq!(keyring.get("azul-storage/drive-key/d_1").unwrap(), None);
    std::fs::write(secrets.path(), "not json sesame").unwrap();
    match keyring.get("x") {
        Err(KeyringError::Failed(why)) => assert!(!why.contains("sesame"), "{why}"),
        other => panic!("not a keyring failure: {other:?}"),
    }
}

#[test]
fn a_secret_is_kept_under_its_keyring_name_and_errors_never_show_it() {
    let dir = TempDir::new("azcloud-secrets");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    assert_eq!(secrets.get("x").unwrap(), None);
    secrets
        .set(&drive_token_entry("d_1"), "dt_f.0.sesame")
        .unwrap();
    secrets.set(&credentials_entry("d_1"), "{}").unwrap();
    assert_eq!(
        secrets.get("azul-storage/azlin/d_1").unwrap().as_deref(),
        Some("dt_f.0.sesame")
    );
    assert_eq!(credentials_entry("d_1"), "azul-storage/s3/d_1");
    assert!(secrets.remove("azul-storage/azlin/d_1").unwrap());
    assert!(!secrets.remove("azul-storage/azlin/d_1").unwrap());
    assert!(!format!("{secrets:?}").contains("sesame"));
    std::fs::write(
        secrets.path(),
        r#"{"format": "other", "version": 1, "entries": {"a": "sesame"}}"#,
    )
    .unwrap();
    let err = secrets.get("a").unwrap_err().to_string();
    assert!(!err.contains("sesame"), "{err}");
    std::fs::write(secrets.path(), r#"{"format": "azcloud.secrets", "entries": "sesame"}"#)
        .unwrap();
    let err = secrets.get("a").unwrap_err().to_string();
    assert!(!err.contains("sesame"), "a file that does not parse is not quoted: {err}");
}

/// AZ-K7: the secrets file is a plain file. It says so, and a drive's key goes into it only when
/// the user allowed it (the command line's `--allow-plain-secrets`); a credential does either way.
#[cfg(feature = "encryption")]
#[test]
fn az_k7_the_secrets_file_takes_a_drives_key_only_when_the_user_allowed_it() {
    use azul_storage::{
        crypto::{device::plan_drive_key, DriveKey},
        footprint::{Allow, KeyEntry, Protection},
    };

    let dir = TempDir::new("azcloud-secrets-plain");
    let path = dir.path().join("secrets.json");
    let file = FileSecrets::new(path.clone());
    assert_eq!(file.protection(), Protection::PlainFile { allow: Allow::Nothing });

    let key = DriveKey::generate().unwrap();
    let refused = plan_drive_key(&file, "d_1", &key).unwrap_err().to_string();
    assert!(refused.contains("plain file"), "{refused}");
    assert!(!refused.contains("sesame"), "{refused}");
    assert!(!path.exists(), "nothing was written for a refused key");

    // A credential is kept in it all the same.
    file.set(&credentials_entry("d_1"), "{}").unwrap();
    file.set(&drive_token_entry("d_1"), "dt_f.0.sesame").unwrap();

    let allowed = file.clone().allowing(Allow::PlainSecrets);
    assert_eq!(allowed.protection(), Protection::PlainFile { allow: Allow::PlainSecrets });
    plan_drive_key(&allowed, "d_1", &key).unwrap();
    assert!(allowed.get(&KeyEntry::PlannedKey.name("d_1")).unwrap().is_some());
}

/// AZ-K7: the state folder carries the user's word to every secrets file it hands out - the
/// account holds a copy of the folder, so a command that opened the folder allowed gets it.
#[test]
fn az_k7_the_state_folder_hands_out_a_secrets_file_that_carries_the_users_word() {
    use azul_storage::{
        footprint::{Allow, Protection},
        keyring::KeyringStore,
    };

    let dir = TempDir::new("azcloud-state-plain");
    let state = crate::state::StateDir::open(dir.path()).unwrap();
    assert_eq!(
        state.secrets().protection(),
        Protection::PlainFile { allow: Allow::Nothing }
    );
    let allowed = state.allowing_plain_secrets(Allow::PlainSecrets);
    assert_eq!(
        allowed.clone().secrets().protection(),
        Protection::PlainFile { allow: Allow::PlainSecrets }
    );
}

/// X-DELTA-AZDRIVE-KEYS-26: a change of the secrets file is made under a lock every process on the
/// state folder takes turns at. The whole file was read, changed and written back with nothing held:
/// two processes lost each other's change - a rotated drive token that is lost revokes the device's
/// family. Here the lock is held by "another process" (the test); the change waits for it and lands
/// after, and the file is not touched meanwhile (the old code wrote at once).
#[test]
fn x_delta_azdrive_keys_26_a_change_of_the_secrets_file_waits_for_the_lock_of_the_file() {
    use std::{sync::mpsc, time::Duration};

    use azul_storage::lock::LockDir;

    use crate::secrets::FILE_LOCK;

    let dir = TempDir::new("azcloud-secrets-lock");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    secrets.set("a", "one").unwrap();
    let held = LockDir::new(dir.path().join("locks"))
        .lock(FILE_LOCK, Duration::from_secs(5))
        .unwrap();

    let (done_tx, done) = mpsc::channel();
    let writer = {
        let secrets = secrets.clone();
        std::thread::spawn(move || {
            secrets.set("b", "two").unwrap();
            let removed = secrets.remove("a").unwrap();
            done_tx.send(removed).unwrap();
        })
    };
    assert!(
        done.recv_timeout(Duration::from_millis(300)).is_err(),
        "the change is waiting for the lock of the file"
    );
    assert_eq!(secrets.get("b").unwrap(), None, "and has written nothing");
    assert_eq!(secrets.get("a").unwrap().as_deref(), Some("one"));
    drop(held);
    assert!(done.recv_timeout(Duration::from_secs(10)).unwrap());
    writer.join().unwrap();
    assert_eq!(secrets.get("b").unwrap().as_deref(), Some("two"));
    assert_eq!(secrets.get("a").unwrap(), None);
}

/// X-DELTA-AZDRIVE-KEYS-26: many changes from several threads of one state folder lose none (the
/// threads stand for processes: the lock is the OS's file lock and the process's slot both).
#[test]
fn x_delta_azdrive_keys_26_changes_of_the_secrets_file_from_several_writers_lose_none() {
    let dir = TempDir::new("azcloud-secrets-many");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    let writers: Vec<_> = (0..4)
        .map(|writer| {
            let secrets = secrets.clone();
            std::thread::spawn(move || {
                for i in 0..10 {
                    secrets.set(&format!("w{writer}/e{i}"), "x").unwrap();
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    for writer in 0..4 {
        for i in 0..10 {
            assert!(
                secrets.get(&format!("w{writer}/e{i}")).unwrap().is_some(),
                "w{writer}/e{i} was lost"
            );
        }
    }
}

/// X-DELTA-AZDRIVE-KEYS-26: the secrets file's `hold` of an entry is a lock of the state folder's
/// `locks/`, not the default's among the threads of this process: another holder of the same lock
/// (another process on the folder) is kept out, and a second `hold` of the entry waits.
#[test]
fn x_delta_azdrive_keys_26_the_hold_of_an_entry_is_a_lock_every_process_on_the_folder_sees() {
    use std::time::Duration;

    use azul_storage::lock::LockDir;

    use crate::secrets::entry_lock;

    let dir = TempDir::new("azcloud-secrets-hold");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    let keyring: &dyn KeyringStore = &secrets;
    let held = keyring.hold("azul-storage/drive-key/d_1").unwrap();
    let other = LockDir::new(dir.path().join("locks"));
    let busy = other
        .lock(
            &entry_lock("azul-storage/drive-key/d_1"),
            Duration::from_millis(100),
        )
        .unwrap_err();
    assert!(busy.is_busy(), "another process is kept out: {busy}");
    // Another entry is not held.
    drop(
        other
            .lock(
                &entry_lock("azul-storage/drive-key/d_2"),
                Duration::from_millis(100),
            )
            .unwrap(),
    );
    drop(held);
    drop(
        other
            .lock(
                &entry_lock("azul-storage/drive-key/d_1"),
                Duration::from_millis(100),
            )
            .unwrap(),
    );
}
