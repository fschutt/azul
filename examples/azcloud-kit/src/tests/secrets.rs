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
