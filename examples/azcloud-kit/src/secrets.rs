//! The secrets of one state folder: `secrets.json`, readable by this user
//! only (0600 on Unix, an access list for this user on Windows), written through a temporary file
//! that is flushed to the disk before the rename.
//!
//! The entries are named as the OS keyring entries the apps use, so moving a
//! device to the OS keyring changes where the strings live, not what they are:
//!
//! - `azul-storage/s3/<drive>`: the current 12-hour credentials, as
//!   azul-storage's keyring JSON (`access_key_id`, `secret_access_key`,
//!   `session_token`) - what AzDrive's S3 drive opens with.
//! - `azul-storage/azlin/<drive>`: the drive token, the refresh token of this
//!   device's token family. It rotates on every refresh, and the old one, once
//!   used again, revokes the family: it is written before anything else.
//!
//! A GUI app keeps the same entries in the OS keyring (azul's
//! `CallbackInfo::keyring_*`); the command line cannot reach that keyring
//! without libazul, hence the file. Nothing here prints a secret: errors name
//! the entry, never its value.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use azul_storage::{
    footprint::{Allow, KeyEntry, Protection},
    keyring::{KeyringError, KeyringStore, ENTRY_LOCK_WAIT},
    lock::{HeldLock, LockDir},
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{fail, CloudError, CloudResult},
    state::write_json,
};

/// The `format` of the file.
pub const SECRETS_FORMAT: &str = "azcloud.secrets";

/// The lock a change of the whole file is made under (`locks/` beside the file, where the state
/// folder keeps its locks).
pub(crate) const FILE_LOCK: &str = "secrets-file";
/// How long a change of the file waits for another process's change of it: a blink, a read and a
/// flushed write.
const FILE_LOCK_WAIT: Duration = Duration::from_secs(30);

/// The lock of one entry held across a read, a decision and a write of it
/// ([`KeyringStore::hold`]).
pub(crate) fn entry_lock(key: &str) -> String {
    format!("entry {key}")
}

/// The keyring entry of a drive's credentials (azul-storage's name).
#[must_use]
pub fn credentials_entry(drive_id: &str) -> String {
    azul_storage::config::keyring_key(drive_id)
}

/// The keyring entry of a drive's drive token.
#[must_use]
pub fn drive_token_entry(drive_id: &str) -> String {
    KeyEntry::AzlinToken.name(drive_id)
}

#[derive(Default, Serialize, Deserialize)]
struct SecretsFile {
    format: String,
    version: u32,
    entries: BTreeMap<String, String>,
}

/// The secrets file of a state folder. A plain file: any program of this user reads it, so a
/// drive's key goes into it only when the user said so ([`FileSecrets::allowing`]); the drive
/// token and the credentials, which the token server revokes and which expire, always do.
#[derive(Clone)]
pub struct FileSecrets {
    path: PathBuf,
    allow: Allow,
}

impl std::fmt::Debug for FileSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSecrets")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl FileSecrets {
    /// The secrets file at `path`, which takes no drive key ([`FileSecrets::allowing`]).
    #[must_use]
    pub fn new(path: PathBuf) -> FileSecrets {
        FileSecrets {
            path,
            allow: Allow::Nothing,
        }
    }

    /// This file with the user's word on keeping a drive's keys in it: `--allow-plain-secrets`
    /// at the command line, the bridge's `plain_secrets` setting (AZ-K7).
    #[must_use]
    pub fn allowing(mut self, allow: Allow) -> FileSecrets {
        self.allow = allow;
        self
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The folder of this file's locks: `locks/` beside it - the state folder's, whose other
    /// locks (the refresh, the vault) are there, so every process on the folder takes turns at
    /// the same ones.
    fn locks(&self) -> LockDir {
        LockDir::new(
            self.path
                .parent()
                .map_or_else(|| PathBuf::from("locks"), |dir| dir.join("locks")),
        )
    }

    fn load(&self) -> CloudResult<SecretsFile> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => fail!("{}: {e}", self.path.display()),
        };
        // The parser's message is not passed on: it can quote what it read.
        let file = match bytes {
            Some(bytes) => serde_json::from_slice::<SecretsFile>(&bytes).map_err(|_| {
                CloudError::failed(format!(
                    "{} is not an azcloud secrets file (it does not parse)",
                    self.path.display()
                ))
            })?,
            None => SecretsFile {
                format: SECRETS_FORMAT.to_string(),
                version: 1,
                entries: BTreeMap::new(),
            },
        };
        if file.format != SECRETS_FORMAT {
            fail!(
                "{} is not an azcloud secrets file (format {:?})",
                self.path.display(),
                file.format
            );
        }
        Ok(file)
    }

    /// The secret of `name`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// When the file cannot be read.
    pub fn get(&self, name: &str) -> CloudResult<Option<String>> {
        Ok(self.load()?.entries.get(name).cloned())
    }

    /// Stores `secret` under `name` (replacing what was there).
    ///
    /// # Errors
    ///
    /// When the file cannot be read or written.
    pub fn set(&self, name: &str, secret: &str) -> CloudResult<()> {
        // The whole file is read, changed and written back: held, so that two processes on one
        // state folder (two commands, the command line and the bridge) do not lose each other's
        // change - a rotated drive token that is lost revokes the device's family
        // (X-DELTA-AZDRIVE-KEYS-26). Fails closed where the file system has no file locks.
        let _held = self.locks().lock(FILE_LOCK, FILE_LOCK_WAIT)?;
        let mut file = self.load()?;
        file.entries.insert(name.to_string(), secret.to_string());
        write_json(&self.path, &file, true)
    }

    /// Forgets `name`; whether there was one.
    ///
    /// # Errors
    ///
    /// When the file cannot be read or written.
    pub fn remove(&self, name: &str) -> CloudResult<bool> {
        let _held = self.locks().lock(FILE_LOCK, FILE_LOCK_WAIT)?;
        let mut file = self.load()?;
        let had = file.entries.remove(name).is_some();
        if had {
            write_json(&self.path, &file, true)?;
        }
        Ok(had)
    }
}

/// The secrets file as azul-storage's keyring: the command line keeps an encrypted drive's
/// keys here (the drive key, this device's member key), under the names the apps use in the OS
/// keyring. The errors name the file, never a secret.
impl KeyringStore for FileSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        FileSecrets::get(self, key).map_err(|e| KeyringError::Failed(e.to_string()))
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        FileSecrets::set(self, key, secret).map_err(|e| KeyringError::Failed(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.remove(key)
            .map(|_| ())
            .map_err(|e| KeyringError::Failed(e.to_string()))
    }

    fn protection(&self) -> Protection {
        Protection::PlainFile { allow: self.allow }
    }

    /// The entry's lock is a file lock in `locks/` beside the secrets file, held by every process
    /// that reaches the same state folder - not the default's, which holds among the threads of
    /// this process only (two commands enrolling at once made two member keys, and the second
    /// one's was kept: AZ-K19, X-DELTA-AZDRIVE-KEYS-26).
    fn hold(&self, key: &str) -> Result<HeldLock, KeyringError> {
        self.locks()
            .lock(&entry_lock(key), ENTRY_LOCK_WAIT)
            .map_err(|e| KeyringError::Failed(e.to_string()))
    }
}
