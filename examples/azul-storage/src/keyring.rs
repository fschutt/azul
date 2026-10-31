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

/// Which keyring service an app's entries live in (azul's `KeyringNamespace`): an app sees
/// only the entries of the namespaces it names. The Azlin apps (AzDrive, AzMail, the bridge,
/// the CLI) share theirs - the drive sessions, the S3 credentials, the open checkouts - in
/// [`KeyringNamespace::azlin_shared`]; any other app keeps its own ([`KeyringNamespace::app`],
/// its app id), so a program built on azul does not read them by accident.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyringNamespace(String);

impl KeyringNamespace {
    /// The service the Azlin apps share.
    pub const AZLIN_SHARED: &'static str = "io.azlin.shared";

    /// The namespace every Azlin app reads and writes its shared entries in.
    #[must_use]
    pub fn azlin_shared() -> Self {
        KeyringNamespace(Self::AZLIN_SHARED.to_string())
    }

    /// The app's own namespace: its app id, which azul fills in.
    #[must_use]
    pub fn app() -> Self {
        KeyringNamespace(String::new())
    }

    /// The namespace `service` (reverse-DNS, `org.example.Editor`); a blank one is the app's own.
    #[must_use]
    pub fn named(service: &str) -> Self {
        KeyringNamespace(service.trim().to_string())
    }

    /// The service name; empty for the app's own namespace.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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

/// Whether a keyring read may ask the user (the system's unlock or "allow" dialog, Touch ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    /// The user asked for what the read serves (opened a drive, chose "Unlock").
    User,
    /// Background work (a start, a timer): a keyring that never asks (azul's quiet ops) answers
    /// [`KeyringError::Denied`] where it would have had to.
    Never,
}

/// Reads `keys` one after the other, each once, and answers each (in order). Asking the user,
/// their first refusal ends the round: the entries after it answer [`KeyringError::Denied`]
/// unread, so one "No" is never followed by one more prompt per entry. A quiet round (`keyring`
/// never asks) reads every entry: one that would need the user does not hold back the rest.
pub fn read_entries(
    keyring: &dyn KeyringStore,
    keys: &[String],
    asking: Asking,
) -> Vec<(String, Result<Option<String>, KeyringError>)> {
    let mut refused = false;
    keys.iter()
        .map(|key| {
            let answer = if refused {
                Err(KeyringError::Denied)
            } else {
                keyring.get(key)
            };
            if asking == Asking::User && answer == Err(KeyringError::Denied) {
                refused = true;
            }
            (key.clone(), answer)
        })
        .collect()
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
