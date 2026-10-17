//! The OS keyring as the storage layer sees it: one secret text per entry name ([`config::
//! keyring_key`](crate::config::keyring_key) for a drive), read and written BLOCKING, from any
//! thread. The apps' keyring is azul's (`AzulKeyring`, feature `azul`: the same entries
//! `CallbackInfo::keyring_*` reads and writes on the UI thread), the tests' a map
//! ([`MemoryKeyring`], feature `testing`).
//!
//! What a worker thread needs it for: an Azlin drive's session is re-read before its drive token
//! is spent (another process may have spent it already) and written before anything else uses
//! the new one; azcloud-kit does both under a lock every process shares. Nothing here logs a
//! secret or puts one in `Debug` output.

use std::fmt;

/// Why the keyring could not help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringError {
    /// The user refused (a biometric prompt cancelled or failed).
    Denied,
    /// There is no keyring on this system (none installed, none running).
    Unavailable,
    /// The keyring failed: why (locked, an I/O error, an answer that is no text).
    Failed(String),
}

impl fmt::Display for KeyringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyringError::Denied => f.write_str("the keyring refused (the prompt was declined)"),
            KeyringError::Unavailable => f.write_str("no keyring is available on this system"),
            KeyringError::Failed(why) => write!(f, "the keyring failed: {why}"),
        }
    }
}

impl std::error::Error for KeyringError {}

/// The keyring: one secret text per entry name. Blocking: call it from a worker thread, never
/// from a UI callback (a read may wait for the system's unlock prompt).
pub trait KeyringStore: Send + Sync {
    /// The text under `key`; `None` when there is no such entry.
    ///
    /// # Errors
    ///
    /// When the keyring cannot be read.
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError>;

    /// Stores `secret` under `key`, replacing what was there.
    ///
    /// # Errors
    ///
    /// When the keyring does not take it.
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError>;

    /// Removes the entry `key` (an entry that is not there is no error).
    ///
    /// # Errors
    ///
    /// When the keyring cannot remove it.
    fn delete(&self, key: &str) -> Result<(), KeyringError>;
}

/// A keyring in a map, for tests: what every clone of an `Arc` of it shares. `Debug` shows the
/// entry names, never a text.
#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
pub struct MemoryKeyring {
    entries: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

#[cfg(any(test, feature = "testing"))]
impl MemoryKeyring {
    /// An empty keyring.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, std::collections::BTreeMap<String, String>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(any(test, feature = "testing"))]
impl fmt::Debug for MemoryKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = self.entries().keys().cloned().collect();
        f.debug_struct("MemoryKeyring")
            .field("entries", &names)
            .finish()
    }
}

#[cfg(any(test, feature = "testing"))]
impl KeyringStore for MemoryKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        Ok(self.entries().get(key).cloned())
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.entries().insert(key.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.entries().remove(key);
        Ok(())
    }
}
