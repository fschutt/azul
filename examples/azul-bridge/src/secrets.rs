//! Where the bridge keeps its secrets: the per-install password, and the mail account's DKIM key
//! and submission password when it sends. Every store is azul-storage's
//! [`KeyringStore`] (CLIENT17's trait), and the entries have the OS keyring's names (AzMail's own
//! for the account's), so AzMail and AzDrive read the same entries the bridge writes:
//!
//! - [`KeyringChoice::Os`]: the OS keyring through azul's blocking keyring calls
//!   (`azul_storage::azul_keyring::AzulKeyring`; the bridge built with its `os-keyring` feature,
//!   which links libazul - what the apps ship with). Keychain on macOS, Credential Manager on
//!   Windows, the Secret Service on Linux, all under azul's one service name.
//! - [`KeyringChoice::File`]: azcloud-kit's [`FileSecrets`] - `secrets.json` in the bridge's state
//!   folder, readable by this user only (0600), written through a flushed temporary file - for a
//!   build without libazul (the CI, a server without a desktop session).
//!
//! The drive's own session (its credentials and rotating drive token) stays in the state
//! folder's secrets file: azcloud-kit's device account keeps it there.

use std::sync::Arc;

use azcloud_kit::secrets::FileSecrets;
pub use azul_storage::keyring::{KeyringError, KeyringStore};

/// The entry of the bridge's own password.
pub const PASSWORD_ENTRY: &str = azcloud_kit::bridge::PASSWORD_ENTRY;

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

/// The secrets file of a state folder as a keyring.
#[derive(Debug, Clone)]
pub struct FileKeyring(pub FileSecrets);

fn failed(e: impl std::fmt::Display) -> KeyringError {
    KeyringError::Failed(e.to_string())
}

impl KeyringStore for FileKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.0.get(key).map_err(failed)
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.0.set(key, secret).map_err(failed)
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.0.remove(key).map(|_| ()).map_err(failed)
    }
}

/// Which keyring the bridge keeps its secrets in (`bridge.json`'s `keyring`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyringChoice {
    Os,
    File,
}

impl KeyringChoice {
    /// What this build can reach: the OS keyring when it links libazul, else the file.
    #[must_use]
    pub fn default_for_build() -> KeyringChoice {
        if cfg!(feature = "os-keyring") {
            KeyringChoice::Os
        } else {
            KeyringChoice::File
        }
    }

    /// `os` / `file`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            KeyringChoice::Os => "os",
            KeyringChoice::File => "file",
        }
    }

    /// `os` / `file` read back; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<KeyringChoice> {
        match text.trim().to_ascii_lowercase().as_str() {
            "os" => Some(KeyringChoice::Os),
            "file" => Some(KeyringChoice::File),
            _ => None,
        }
    }
}

/// The keyring `choice` names, the file one in `secrets` (the state folder's).
///
/// # Errors
///
/// The OS keyring asked of a build without libazul.
pub fn open(choice: KeyringChoice, secrets: FileSecrets) -> Result<Arc<dyn KeyringStore>, String> {
    match choice {
        KeyringChoice::File => Ok(Arc::new(FileKeyring(secrets))),
        KeyringChoice::Os => os_keyring(),
    }
}

#[cfg(feature = "os-keyring")]
fn os_keyring() -> Result<Arc<dyn KeyringStore>, String> {
    Ok(Arc::new(azul_storage::azul_keyring::AzulKeyring::new()))
}

#[cfg(not(feature = "os-keyring"))]
fn os_keyring() -> Result<Arc<dyn KeyringStore>, String> {
    Err(String::from(
        "this azul-bridge was built without the OS keyring (cargo feature os-keyring, which \
         links libazul): use --keyring file",
    ))
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    #[test]
    fn the_file_keyring_keeps_entries_under_their_keyring_names() {
        let dir = TempDir::new("bridge-keyring");
        let keyring = open(KeyringChoice::File, FileSecrets::new(dir.0.join("secrets.json"))).unwrap();
        assert_eq!(keyring.get(PASSWORD_ENTRY).unwrap(), None);
        keyring.set(PASSWORD_ENTRY, "k7m2p-9qxat").unwrap();
        keyring.set(&dkim_entry("ada@example.org"), "pem").unwrap();
        assert_eq!(keyring.get(PASSWORD_ENTRY).unwrap().as_deref(), Some("k7m2p-9qxat"));
        keyring.delete(PASSWORD_ENTRY).unwrap();
        keyring.delete(PASSWORD_ENTRY).unwrap();
        assert_eq!(keyring.get(PASSWORD_ENTRY).unwrap(), None);
        assert_eq!(dkim_entry("ada@example.org"), "AzMail/ada@example.org/dkim");
        assert_eq!(sign_in_entry("ada@example.org"), "AzMail/ada@example.org/imap");
    }

    #[test]
    fn the_keyring_choice_reads_back_and_a_build_without_libazul_has_no_os_keyring() {
        for choice in [KeyringChoice::Os, KeyringChoice::File] {
            assert_eq!(KeyringChoice::parse(choice.name()), Some(choice));
        }
        assert_eq!(KeyringChoice::parse(" OS "), Some(KeyringChoice::Os));
        assert_eq!(KeyringChoice::parse("vault"), None);
        if !cfg!(feature = "os-keyring") {
            assert_eq!(KeyringChoice::default_for_build(), KeyringChoice::File);
            let dir = TempDir::new("bridge-keyring");
            assert!(open(KeyringChoice::Os, FileSecrets::new(dir.0.join("s.json"))).is_err());
        }
    }
}
