//! [`KeyringStore`] over azul's keyring, for the apps. Feature `azul`.
//!
//! azul's `Keyring::get_blocking` / `store_blocking` / `delete_blocking` run the keyring op on
//! the calling thread and return its answer: the entries `CallbackInfo::keyring_*` reads and
//! writes on the UI thread (in a headless or E2E run azul's stand-in, which both reach). Call it
//! from an azul `Thread`, never from a UI callback: a read may wait for the system's unlock
//! prompt.

use azul::{error::KeyringResult, misc::Keyring};

use crate::keyring::{KeyringError, KeyringStore};

/// The system keyring through azul. Call only from an azul `Thread`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AzulKeyring;

impl AzulKeyring {
    #[must_use]
    pub fn new() -> Self {
        AzulKeyring
    }
}

/// What a keyring answer that is no success says.
fn error_of(answer: &KeyringResult) -> KeyringError {
    match answer {
        KeyringResult::Denied => KeyringError::Denied,
        KeyringResult::Unavailable => KeyringError::Unavailable,
        KeyringResult::NotFound => KeyringError::Failed(String::from("the entry is not there")),
        KeyringResult::Error => KeyringError::Failed(String::from("the keyring reported an error")),
        _ => KeyringError::Failed(String::from("the keyring answered something else")),
    }
}

impl KeyringStore for AzulKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        match Keyring::get_blocking(key) {
            KeyringResult::Retrieved(secret) => Ok(Some(secret.as_str().to_string())),
            KeyringResult::NotFound => Ok(None),
            other => Err(error_of(&other)),
        }
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        match Keyring::store_blocking(key, secret, false) {
            KeyringResult::Stored => Ok(()),
            other => Err(error_of(&other)),
        }
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        match Keyring::delete_blocking(key) {
            KeyringResult::Deleted | KeyringResult::NotFound => Ok(()),
            other => Err(error_of(&other)),
        }
    }
}
