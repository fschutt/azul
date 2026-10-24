//! [`KeyringStore`] over azul's keyring, for the apps. Feature `azul`.
//!
//! azul's `Keyring::get_blocking_in` / `store_blocking_in` / `delete_blocking_in` run the
//! keyring op on the calling thread and return its answer, in the keyring service of this
//! keyring's [`KeyringNamespace`]: the entries `CallbackInfo::keyring_*_in` of the same namespace
//! reads and writes on the UI thread (in a headless or E2E run azul's stand-in, which both
//! reach). Call it from an azul `Thread`, never from a UI callback: a read may wait for the
//! system's unlock prompt.

use azul::{
    error::KeyringResult,
    misc::{Keyring, KeyringNamespace as AzulNamespace},
};

use crate::keyring::{KeyringError, KeyringNamespace, KeyringStore};

/// The system keyring through azul, in one namespace. Call only from an azul `Thread`.
#[derive(Debug, Clone)]
pub struct AzulKeyring {
    /// The keyring service every entry of this keyring lives in.
    namespace: KeyringNamespace,
}

impl AzulKeyring {
    /// The keyring of `namespace` (the Azlin apps: [`KeyringNamespace::azlin_shared`]).
    #[must_use]
    pub fn new(namespace: KeyringNamespace) -> Self {
        AzulKeyring { namespace }
    }

    /// The namespace this keyring reads and writes.
    #[must_use]
    pub fn namespace(&self) -> &KeyringNamespace {
        &self.namespace
    }

    fn service(&self) -> AzulNamespace {
        azul_namespace(&self.namespace)
    }
}

/// azul's form of `namespace`, for a UI callback's `CallbackInfo::keyring_*_in`: the entries
/// it reads and writes are this crate's [`AzulKeyring`]'s of the same namespace.
#[must_use]
pub fn azul_namespace(namespace: &KeyringNamespace) -> AzulNamespace {
    AzulNamespace::create(namespace.as_str())
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
        match Keyring::get_blocking_in(self.service(), key) {
            KeyringResult::Retrieved(secret) => Ok(Some(secret.as_str().to_string())),
            KeyringResult::NotFound => Ok(None),
            other => Err(error_of(&other)),
        }
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        match Keyring::store_blocking_in(self.service(), key, secret, false) {
            KeyringResult::Stored => Ok(()),
            other => Err(error_of(&other)),
        }
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        match Keyring::delete_blocking_in(self.service(), key) {
            KeyringResult::Deleted | KeyringResult::NotFound => Ok(()),
            other => Err(error_of(&other)),
        }
    }
}
