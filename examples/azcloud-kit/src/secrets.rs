//! The secrets of one state folder: `secrets.json`, readable by this user
//! only (0600 on Unix), written through a temporary file that is flushed to
//! the disk before the rename.
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

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{fail, CloudError, CloudResult},
    state::write_json,
};

/// The `format` of the file.
pub const SECRETS_FORMAT: &str = "azcloud.secrets";

/// The keyring entry of a drive's credentials (azul-storage's name).
#[must_use]
pub fn credentials_entry(drive_id: &str) -> String {
    azul_storage::config::keyring_key(drive_id)
}

/// The keyring entry of a drive's drive token.
#[must_use]
pub fn drive_token_entry(drive_id: &str) -> String {
    format!("azul-storage/azlin/{drive_id}")
}

#[derive(Default, Serialize, Deserialize)]
struct SecretsFile {
    format: String,
    version: u32,
    entries: BTreeMap<String, String>,
}

/// The secrets file of a state folder.
#[derive(Clone)]
pub struct FileSecrets {
    path: PathBuf,
}

impl std::fmt::Debug for FileSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSecrets")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl FileSecrets {
    #[must_use]
    pub fn new(path: PathBuf) -> FileSecrets {
        FileSecrets { path }
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
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
        let mut file = self.load()?;
        let had = file.entries.remove(name).is_some();
        if had {
            write_json(&self.path, &file, true)?;
        }
        Ok(had)
    }
}
