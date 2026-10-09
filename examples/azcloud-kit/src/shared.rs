//! The keyring every process of this user shares, and the locks its entries change under: what
//! an app keeps that must outlive it and its other windows - an Azlin drive's session (its drive
//! token rotates with every refresh), the unfinished checkouts. Every change of such an entry
//! is read, decided and written under the entry's lock ([`crate::lock`]), so a second window or
//! a second app never works from a text another one has replaced.
//!
//! Blocking, like the keyring it wraps (azul's in the apps, a map in the tests): call it from an
//! azul `Thread`.

use std::{fmt, sync::Arc, time::Duration};

use azul_storage::{config::keyring_key, keyring::KeyringStore};

use crate::{
    bundle::DriveBundle,
    error::CloudResult,
    lock::{HeldLock, LockDir},
    session::AzlinSession,
};

/// How long a change waits for another process's change of the same entry: a refresh holds the
/// lock for one call to the token server, which azul's HTTP client gives up on after 90 s.
pub const LOCK_WAIT: Duration = Duration::from_secs(120);

/// The keyring and the locks of its entries. `Debug` shows where the locks are, nothing of the
/// keyring.
#[derive(Clone)]
pub struct SharedKeyring {
    keyring: Arc<dyn KeyringStore>,
    locks: LockDir,
}

impl fmt::Debug for SharedKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedKeyring")
            .field("locks", &self.locks)
            .finish_non_exhaustive()
    }
}

impl SharedKeyring {
    /// `keyring`, its entries changed under the locks in `locks`.
    #[must_use]
    pub fn new(keyring: Arc<dyn KeyringStore>, locks: LockDir) -> SharedKeyring {
        SharedKeyring { keyring, locks }
    }

    /// Where the locks are.
    #[must_use]
    pub fn locks(&self) -> &LockDir {
        &self.locks
    }

    /// Takes the lock of the entry `key`: one holder in every process of this user at a time.
    ///
    /// # Errors
    ///
    /// When another holder keeps it longer than [`LOCK_WAIT`], or it cannot be taken.
    pub fn lock(&self, key: &str) -> CloudResult<HeldLock> {
        self.locks.lock(key, LOCK_WAIT)
    }

    /// The text of the entry `key`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// When the keyring cannot be read.
    pub fn get(&self, key: &str) -> CloudResult<Option<String>> {
        Ok(self.keyring.get(key)?)
    }

    /// Stores `text` under `key`.
    ///
    /// # Errors
    ///
    /// When the keyring does not take it.
    pub fn set(&self, key: &str, text: &str) -> CloudResult<()> {
        Ok(self.keyring.set(key, text)?)
    }

    /// Removes the entry `key`.
    ///
    /// # Errors
    ///
    /// When the keyring cannot remove it.
    pub fn delete(&self, key: &str) -> CloudResult<()> {
        Ok(self.keyring.delete(key)?)
    }

    /// The session of a new drive (`bundle`: a sign-up, a paid checkout's) into the drive's
    /// keyring entry - unless the entry holds a session of that drive already, which stays (it
    /// may have rotated since: the sign-up's token is spent then). Under the drive's lock, as
    /// every refresh of the drive is. The entry's text now, and whether it was there already.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken or the keyring cannot be read or written.
    pub fn keep_new_drive(&self, bundle: &DriveBundle) -> CloudResult<(String, bool)> {
        let key = keyring_key(bundle.drive_id());
        let _lock = self.lock(&key)?;
        if let Some(text) = self.get(&key)? {
            let ours = AzlinSession::from_keyring_secret(&text)
                .is_ok_and(|session| session.drive_id == bundle.drive_id());
            if ours {
                return Ok((text, true));
            }
        }
        let text = bundle.session().to_keyring_secret();
        self.set(&key, &text)?;
        Ok((text, false))
    }
}
