//! Where the bridge keeps its secrets: the per-install password, and the mail account's DKIM key
//! and submission password when it sends. The entries have the OS keyring's names (AzMail's
//! own for the account's), so moving them into the keyring changes where the strings live, not
//! what they are.
//!
//! Today the store is azcloud-kit's [`FileSecrets`]: `secrets.json` in the bridge's state
//! folder, readable by this user only (0600), written through a flushed temporary file - what
//! the `azcloud` command line keeps, since a process without libazul cannot reach the OS
//! keyring (azul's keyring is the dll's). [`SecretStore`] is the seam the OS keyring plugs into
//! later (azul-storage's keyring trait once it is there).

use std::{collections::BTreeMap, sync::Mutex};

use azcloud_kit::secrets::FileSecrets;

/// The entry of the bridge's own password.
pub const PASSWORD_ENTRY: &str = "AzulBridge/password";

/// The entry of a mail account's DKIM private key: AzMail's keyring name for it.
#[must_use]
pub fn dkim_entry(account_id: &str) -> String {
    azmail_core::send::dkim_keyring_key(account_id)
}

/// The entry of a mail account's password for signed-in submission: AzMail's keyring name.
#[must_use]
pub fn sign_in_entry(account_id: &str) -> String {
    azmail_core::account::keyring_key(account_id)
}

/// A place for named secrets. Nothing here prints or logs a secret; errors name the entry.
pub trait SecretStore: Send + Sync {
    /// The secret `name`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// When the store cannot be read (the reason, never the secret).
    fn get(&self, name: &str) -> Result<Option<String>, String>;

    /// Keeps `secret` as `name`, replacing what was there.
    ///
    /// # Errors
    ///
    /// When the store cannot be written.
    fn set(&self, name: &str, secret: &str) -> Result<(), String>;
}

/// The secrets file of a state folder.
#[derive(Debug, Clone)]
pub struct FileSecretStore(pub FileSecrets);

impl SecretStore for FileSecretStore {
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        self.0.get(name).map_err(|e| e.to_string())
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), String> {
        self.0.set(name, secret).map_err(|e| e.to_string())
    }
}

/// Secrets in memory (the tests).
#[derive(Debug, Default)]
pub struct MemorySecretStore(Mutex<BTreeMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.to_string(), secret.to_string());
        Ok(())
    }
}
