//! AzKeys' files in the data tree, laid out as the user's S3 bucket will be:
//!
//! - `keys/vaults/<vault-uuid>.azkv` - one encrypted vault file per vault ([`crate::crypto`]);
//! - `keys/backups/<vault-uuid>.azkv` - the file as it was before the last save;
//! - `keys/exports/audit-<date>.txt` - an exported audit report (no secrets);
//! - `keys/settings.json` - azul-appkit's settings file (theme, mode, AzKeys' values).
//!
//! The work on them - list, create, unlock (Argon2id: a quarter second), save, change the master
//! password - is [`run`] on a drive, called from an azul `Thread` (`jobs.rs`), never from a
//! callback; plain Rust, tested on a temporary folder.

use zeroize::Zeroizing;

use azul_storage::{Drive, DriveError};

use crate::crypto::{Envelope, KdfParams, SecretKey, VaultError, SUFFIX};
use crate::vault::Vault;

/// AzKeys' folder in the data root.
pub const APP_FOLDER: &str = "keys";
/// The folder of the vault files (a listing prefix).
pub const VAULTS: &str = "keys/vaults/";

/// The key of a vault's file.
#[must_use]
pub fn file_key(vault_id: &str) -> String {
    format!("{VAULTS}{vault_id}{SUFFIX}")
}

/// The key of a vault file's backup (the file before the last save).
#[must_use]
pub fn backup_key(vault_id: &str) -> String {
    format!("{APP_FOLDER}/backups/{vault_id}{SUFFIX}")
}

/// The OS keyring entry that holds a vault's key on this device.
#[must_use]
pub fn keyring_name(vault_id: &str) -> String {
    format!("azkeys.vault.{vault_id}")
}

/// The key of an exported audit report (`date` = `YYYY-MM-DD`).
#[must_use]
pub fn audit_key(date: &str) -> String {
    format!("{APP_FOLDER}/exports/audit-{date}.txt")
}

/// A vault file found in the folder, not opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultFile {
    pub key: String,
    pub envelope: Envelope,
}

impl VaultFile {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.envelope.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.envelope.name
    }
}

/// The vault files of a listing `(key, bytes)`, by name; a file that is no vault is a problem
/// (one sentence each).
#[must_use]
pub fn read_listing(files: Vec<(String, Vec<u8>)>) -> (Vec<VaultFile>, Vec<String>) {
    let mut vaults = Vec::new();
    let mut problems = Vec::new();
    for (key, bytes) in files {
        if !key.ends_with(SUFFIX) {
            continue;
        }
        match Envelope::parse(&bytes) {
            Ok(envelope) => vaults.push(VaultFile { key, envelope }),
            Err(e) => problems.push(format!("{key}: {e}")),
        }
    }
    vaults.sort_by_cached_key(|v| (azul_pim::search::fold(v.name()), v.key.clone()));
    (vaults, problems)
}

/// Something to do with the vault files. Holds secrets: never printed.
pub enum Work {
    /// Every vault file's header.
    List,
    /// A new vault file for `vault`, sealed with `password` (`kdf`: the cost and salt; `None` =
    /// the default cost and a fresh salt).
    Create {
        vault: Vault,
        password: Zeroizing<String>,
        kdf: Option<KdfParams>,
    },
    /// Open the file at `key` with the master password.
    Unlock {
        key: String,
        password: Zeroizing<String>,
    },
    /// Open the file at `key` with the vault key from the OS keyring.
    UnlockWithKey { key: String, vault_key: SecretKey },
    /// Write the vault's JSON sealed afresh, keeping the file's sealed key.
    Save {
        key: String,
        envelope: Envelope,
        vault_key: SecretKey,
        name: String,
        json: Zeroizing<Vec<u8>>,
    },
    /// Seal the vault key under a new master password.
    ChangePassword {
        key: String,
        envelope: Envelope,
        vault_key: SecretKey,
        password: Zeroizing<String>,
        kdf: Option<KdfParams>,
    },
    /// Write a file that holds no secret (an audit report).
    Put { key: String, bytes: Vec<u8> },
}

/// An opened vault: its file, the file's header and sealed key, the vault key, the content.
pub struct Opened {
    pub key: String,
    pub envelope: Envelope,
    pub vault_key: SecretKey,
    pub vault: Vault,
}

/// What a [`Work`] did.
pub enum Done {
    Listed {
        vaults: Vec<VaultFile>,
        problems: Vec<String>,
    },
    Created(Opened),
    Unlocked(Opened),
    Saved {
        key: String,
        envelope: Envelope,
    },
    PasswordChanged {
        key: String,
        envelope: Envelope,
    },
    Put {
        key: String,
    },
    Failed(Failure),
}

/// Why a [`Work`] failed, for the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// "list", "create", "unlock", "save", "change the password", "write".
    pub what: &'static str,
    /// The master password was wrong.
    pub wrong_password: bool,
    /// The keyring's key does not open the vault.
    pub wrong_key: bool,
    /// A sentence for the user.
    pub message: String,
}

impl Failure {
    fn of(what: &'static str, error: &VaultError) -> Failure {
        Failure {
            what,
            wrong_password: *error == VaultError::WrongPassword,
            wrong_key: *error == VaultError::WrongKey,
            message: error.to_string(),
        }
    }

    fn io(what: &'static str, error: &DriveError) -> Failure {
        Failure {
            what,
            wrong_password: false,
            wrong_key: false,
            message: error.to_string(),
        }
    }
}

impl Failure {
    /// The same failure, of another piece of work.
    fn with_what(mut self, what: &'static str) -> Failure {
        self.what = what;
        self
    }
}

/// The vault file at `key`, parsed (not opened).
fn read_envelope(drive: &dyn Drive, key: &str) -> Result<Envelope, Failure> {
    let bytes = drive.get(key).map_err(|e| match e {
        DriveError::NotFound { .. } => Failure {
            what: "read",
            wrong_password: false,
            wrong_key: false,
            message: "the vault file is gone (moved or deleted)".to_string(),
        },
        other => Failure::io("read", &other),
    })?;
    Envelope::parse(&bytes).map_err(|e| Failure::of("read", &e))
}

/// The vault of a file whose vault key is known; its id and name are the file's.
fn open(key: String, envelope: Envelope, vault_key: SecretKey) -> Done {
    let json = match envelope.open_data(&vault_key) {
        Ok(json) => json,
        Err(e) => return Done::Failed(Failure::of("unlock", &e)),
    };
    match Vault::from_json(&json) {
        Ok(mut vault) => {
            vault.id = envelope.id.clone();
            vault.name = envelope.name.clone();
            Done::Unlocked(Opened {
                key,
                envelope,
                vault_key,
                vault,
            })
        }
        Err(message) => Done::Failed(Failure {
            what: "unlock",
            wrong_password: false,
            wrong_key: false,
            message,
        }),
    }
}

/// Does `work` on `drive`.
pub fn run(drive: &dyn Drive, work: Work) -> Done {
    match work {
        Work::List => {
            let keys = match azul_appkit::files::list_all(drive, VAULTS) {
                Ok(keys) => keys,
                // No folder yet: no vault yet.
                Err(DriveError::NotFound { .. }) => Vec::new(),
                Err(e) => return Done::Failed(Failure::io("list", &e)),
            };
            let mut files = Vec::new();
            let mut problems = Vec::new();
            for key in keys.into_iter().filter(|k| k.ends_with(SUFFIX)) {
                match drive.get(&key) {
                    Ok(bytes) => files.push((key, bytes)),
                    Err(DriveError::NotFound { .. }) => {}
                    Err(e) => problems.push(format!("{key}: {e}")),
                }
            }
            let (vaults, mut unreadable) = read_listing(files);
            problems.append(&mut unreadable);
            Done::Listed { vaults, problems }
        }
        Work::Create {
            vault,
            password,
            kdf,
        } => {
            let created = (|| {
                let kdf = match kdf {
                    Some(kdf) => kdf,
                    None => KdfParams::fresh()?,
                };
                let json = vault.to_json();
                Envelope::create(&vault.id, &vault.name, &password, kdf, &json)
            })();
            let (envelope, vault_key) = match created {
                Ok(pair) => pair,
                Err(e) => return Done::Failed(Failure::of("create", &e)),
            };
            let key = file_key(&vault.id);
            if let Err(e) = drive.put(&key, &envelope.to_bytes()) {
                return Done::Failed(Failure::io("create", &e));
            }
            Done::Created(Opened {
                key,
                envelope,
                vault_key,
                vault,
            })
        }
        Work::Unlock { key, password } => {
            let envelope = match read_envelope(drive, &key) {
                Ok(envelope) => envelope,
                Err(f) => return Done::Failed(f.with_what("unlock")),
            };
            match envelope.unwrap_key(&password) {
                Ok(vault_key) => open(key, envelope, vault_key),
                Err(e) => Done::Failed(Failure::of("unlock", &e)),
            }
        }
        Work::UnlockWithKey { key, vault_key } => match read_envelope(drive, &key) {
            Ok(envelope) => open(key, envelope, vault_key),
            Err(f) => Done::Failed(f.with_what("unlock")),
        },
        Work::Save {
            key,
            envelope,
            vault_key,
            name,
            json,
        } => {
            let next = match envelope.reseal(&vault_key, &name, &json) {
                Ok(next) => next,
                Err(e) => return Done::Failed(Failure::of("save", &e)),
            };
            // The file as it was, kept once: a save that goes wrong leaves the last good one.
            let _ = drive.copy(&key, &backup_key(&envelope.id));
            match drive.put(&key, &next.to_bytes()) {
                Ok(()) => Done::Saved {
                    key,
                    envelope: next,
                },
                Err(e) => Done::Failed(Failure::io("save", &e)),
            }
        }
        Work::ChangePassword {
            key,
            envelope,
            vault_key,
            password,
            kdf,
        } => {
            let next = (|| {
                let kdf = match kdf {
                    Some(kdf) => kdf,
                    None => KdfParams::fresh()?,
                };
                envelope.rewrap(&vault_key, &password, kdf)
            })();
            let next = match next {
                Ok(next) => next,
                Err(e) => return Done::Failed(Failure::of("change the password", &e)),
            };
            let _ = drive.copy(&key, &backup_key(&envelope.id));
            match drive.put(&key, &next.to_bytes()) {
                Ok(()) => Done::PasswordChanged {
                    key,
                    envelope: next,
                },
                Err(e) => Done::Failed(Failure::io("change the password", &e)),
            }
        }
        Work::Put { key, bytes } => match drive.put(&key, &bytes) {
            Ok(()) => Done::Put { key },
            Err(e) => Done::Failed(Failure::io("write", &e)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Item, Kind};
    use azul_pim::testing::TempDir;
    use azul_storage::LocalDrive;

    fn cheap() -> Option<KdfParams> {
        Some(KdfParams::with_cost(64, 1, 1).expect("random salt"))
    }

    fn pw(text: &str) -> Zeroizing<String> {
        Zeroizing::new(text.to_string())
    }

    fn sample_vault() -> Vault {
        let mut v = Vault::new("Personal", 1);
        let mut item = Item::new(Kind::Login, "CodeHost", 1);
        item.password = "q7#Rt!vW2m".to_string();
        v.items.push(item);
        v
    }

    fn created(drive: &LocalDrive) -> Opened {
        match run(
            drive,
            Work::Create {
                vault: sample_vault(),
                password: pw("master"),
                kdf: cheap(),
            },
        ) {
            Done::Created(opened) => opened,
            Done::Failed(f) => panic!("create failed: {}", f.message),
            _ => panic!("not created"),
        }
    }

    #[test]
    fn a_new_vault_is_written_listed_unlocked_saved_and_unlocked_again() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let opened = created(&drive);
        assert_eq!(opened.key, file_key(&opened.vault.id));
        assert!(opened.key.starts_with("keys/vaults/") && opened.key.ends_with(".azkv"));

        let Done::Listed { vaults, problems } = run(&drive, Work::List) else {
            panic!("not listed")
        };
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].name(), "Personal");
        assert_eq!(vaults[0].id(), opened.vault.id);

        let Done::Unlocked(unlocked) = run(
            &drive,
            Work::Unlock {
                key: opened.key.clone(),
                password: pw("master"),
            },
        ) else {
            panic!("not unlocked")
        };
        assert_eq!(unlocked.vault, opened.vault);
        assert_eq!(unlocked.vault_key, opened.vault_key);

        let mut changed = unlocked.vault.clone();
        changed.items[0].title = "CodeHost (example)".to_string();
        let save = Work::Save {
            key: unlocked.key.clone(),
            envelope: unlocked.envelope.clone(),
            vault_key: unlocked.vault_key.clone(),
            name: changed.name.clone(),
            json: changed.to_json(),
        };
        let Done::Saved { envelope, .. } = run(&drive, save) else {
            panic!("not saved")
        };
        assert_eq!(
            envelope.key, unlocked.envelope.key,
            "a save keeps the sealed key"
        );

        let Done::Unlocked(again) = run(
            &drive,
            Work::Unlock {
                key: unlocked.key.clone(),
                password: pw("master"),
            },
        ) else {
            panic!("not unlocked again")
        };
        assert_eq!(again.vault.items[0].title, "CodeHost (example)");
    }

    #[test]
    fn a_wrong_password_and_a_stale_device_key_fail_as_such() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let opened = created(&drive);
        let Done::Failed(f) = run(
            &drive,
            Work::Unlock {
                key: opened.key.clone(),
                password: pw("Master"),
            },
        ) else {
            panic!("a wrong password opened the vault")
        };
        assert!(f.wrong_password && !f.wrong_key);
        let stale = SecretKey::random().expect("random");
        let Done::Failed(f) = run(
            &drive,
            Work::UnlockWithKey {
                key: opened.key.clone(),
                vault_key: stale,
            },
        ) else {
            panic!("a stale key opened the vault")
        };
        assert!(f.wrong_key && !f.wrong_password);
        let Done::Unlocked(_) = run(
            &drive,
            Work::UnlockWithKey {
                key: opened.key.clone(),
                vault_key: opened.vault_key.clone(),
            },
        ) else {
            panic!("the right key did not open the vault")
        };
        let Done::Failed(f) = run(
            &drive,
            Work::Unlock {
                key: file_key("missing"),
                password: pw("master"),
            },
        ) else {
            panic!("a missing file opened")
        };
        assert!(!f.wrong_password && !f.message.is_empty());
    }

    #[test]
    fn a_new_master_password_opens_and_the_old_one_does_not() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let opened = created(&drive);
        let change = Work::ChangePassword {
            key: opened.key.clone(),
            envelope: opened.envelope.clone(),
            vault_key: opened.vault_key.clone(),
            password: pw("new master"),
            kdf: cheap(),
        };
        let Done::PasswordChanged { .. } = run(&drive, change) else {
            panic!("not changed")
        };
        assert!(matches!(
            run(
                &drive,
                Work::Unlock {
                    key: opened.key.clone(),
                    password: pw("master")
                }
            ),
            Done::Failed(Failure {
                wrong_password: true,
                ..
            })
        ));
        assert!(matches!(
            run(
                &drive,
                Work::Unlock {
                    key: opened.key.clone(),
                    password: pw("new master")
                }
            ),
            Done::Unlocked(_)
        ));
    }

    #[test]
    fn a_save_keeps_the_previous_file_as_a_backup() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let opened = created(&drive);
        let before = drive.get(&opened.key).expect("written");
        let save = Work::Save {
            key: opened.key.clone(),
            envelope: opened.envelope.clone(),
            vault_key: opened.vault_key.clone(),
            name: "Personal".to_string(),
            json: opened.vault.to_json(),
        };
        assert!(matches!(run(&drive, save), Done::Saved { .. }));
        assert_eq!(
            drive.get(&backup_key(&opened.vault.id)).expect("backup"),
            before
        );
        assert_ne!(
            drive.get(&opened.key).expect("written"),
            before,
            "a fresh nonce"
        );
    }

    #[test]
    fn a_file_in_the_vault_folder_that_is_no_vault_is_a_problem_not_a_vault() {
        let (vaults, problems) = read_listing(vec![
            ("keys/vaults/broken.azkv".to_string(), b"not json".to_vec()),
            (
                "keys/vaults/other.azkv".to_string(),
                br#"{"format":"x"}"#.to_vec(),
            ),
        ]);
        assert!(vaults.is_empty());
        assert_eq!(problems.len(), 2);
        assert!(problems[0].contains("broken.azkv"), "{problems:?}");
    }

    #[test]
    fn the_files_live_in_the_keys_folder_and_the_keyring_entry_is_named_after_the_vault() {
        assert_eq!(file_key("abc"), "keys/vaults/abc.azkv");
        assert_eq!(backup_key("abc"), "keys/backups/abc.azkv");
        assert_eq!(audit_key("2026-10-03"), "keys/exports/audit-2026-10-03.txt");
        assert_eq!(keyring_name("abc"), "azkeys.vault.abc");
    }
}
