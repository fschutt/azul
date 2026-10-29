//! The calls this device sent that grant something (a lockdown, a pick-up's claim, a member, a
//! restore, a recovery key) and whose final answer it has not read yet: one small file per open
//! call, `<scope>-<drive>.json`, holding the idempotency key the request carries.
//!
//! The key is written before the request is sent and the file is deleted only when a final
//! answer came, so every retry - after a timeout, a 503, a crash or a restart - carries the same
//! key, and the token server answers it with the grant it made the first time instead of making
//! a second one or refusing a token its first answer replaced. No secret is in the file: a key
//! is worth nothing without the caller's own credential (its drive token, its claim ticket).
//! The folder lives with the locks of the keyring the credentials are in, so every process of
//! this user that spends the same credentials finds the same open calls.

use std::{
    fs,
    path::{Path, PathBuf},
};

use azlin_contract::{IdempotencyKey, IdempotentScope};
use serde::{Deserialize, Serialize};

use crate::{
    state::{read_json, write_json},
    token::TokenError,
};

/// The folder of the open calls, beside the locks of a keyring or in a state folder.
pub const OUTBOX_DIR: &str = "outbox";

/// The open calls in one folder.
#[derive(Debug, Clone)]
pub struct Outbox {
    dir: PathBuf,
}

/// What an open call's file says.
#[derive(Serialize, Deserialize)]
struct Entry {
    scope: IdempotentScope,
    drive: String,
    /// The request's own words, so another request of the same scope gets a key of its own.
    what: String,
    key: IdempotencyKey,
}

/// A call that is open: its key, until a final answer closes it.
#[derive(Debug)]
#[must_use = "an open call is closed by its final answer"]
pub struct OpenCall {
    path: PathBuf,
    key: IdempotencyKey,
    kept: Result<(), String>,
}

/// Whether `outcome` is the final answer of a call: an answer, or a refusal that will not change
/// when asked again. What passes by itself ([`crate::answer`]: no answer, an answer cut off, the
/// rate limit, a 5xx, 408 and 429) leaves the call open.
#[must_use]
pub fn is_final<T>(outcome: &Result<T, TokenError>) -> bool {
    outcome.as_ref().err().is_none_or(is_final_error)
}

/// Whether the refusal `e` is final ([`is_final`]).
#[must_use]
pub fn is_final_error(e: &TokenError) -> bool {
    !crate::answer::of_token_error(e).is_transient()
}

/// A key from the OS random source.
fn fresh_key() -> Result<IdempotencyKey, TokenError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| {
        TokenError::Config(String::from(
            "This computer's random source does not answer: the request is not sent.",
        ))
    })?;
    Ok(IdempotencyKey::fresh(bytes))
}

impl Outbox {
    /// The open calls in `dir` (made with the first one).
    pub fn new(dir: impl Into<PathBuf>) -> Outbox {
        Outbox { dir: dir.into() }
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    fn file_of(&self, scope: IdempotentScope, drive_id: &str) -> PathBuf {
        // a drive id is letters, digits, `_` and `-`; nothing else reaches a file name
        let drive: String = drive_id
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{}-{drive}.json", scope.as_str()))
    }

    /// The call `scope` of `drive_id` for `what` (the request's own words: a member's name, a
    /// restore's prefix and time; empty when the scope says it all): the key it was sent with
    /// when it is open (its answer was lost), else a new key, written down before it is returned.
    /// A call of the same scope for other words is replaced: its answer is not asked for again.
    ///
    /// # Errors
    ///
    /// No random source. A key that cannot be written still names the request within this
    /// process ([`OpenCall::kept`]): the call is not refused for it, a lockdown is needed most
    /// when things go wrong.
    pub fn open(
        &self,
        scope: IdempotentScope,
        drive_id: &str,
        what: &str,
    ) -> Result<OpenCall, TokenError> {
        let path = self.file_of(scope, drive_id);
        let (drive, what) = (drive_id.trim(), what.trim());
        if let Ok(Some(entry)) = read_json::<Entry>(&path) {
            if entry.scope == scope && entry.drive == drive && entry.what == what {
                return Ok(OpenCall {
                    path,
                    key: entry.key,
                    kept: Ok(()),
                });
            }
        }
        let key = fresh_key()?;
        let entry = Entry {
            scope,
            drive: drive.to_string(),
            what: what.to_string(),
            key,
        };
        let kept = write_json(&path, &entry, true).map_err(|e| e.to_string());
        Ok(OpenCall { path, key, kept })
    }

    /// Whether a call `scope` of `drive_id` is open, whatever its words: its key and its words.
    #[must_use]
    pub fn pending(&self, scope: IdempotentScope, drive_id: &str) -> Option<(IdempotencyKey, String)> {
        let entry = read_json::<Entry>(&self.file_of(scope, drive_id)).ok()??;
        (entry.scope == scope && entry.drive == drive_id.trim()).then_some((entry.key, entry.what))
    }

    /// Sends a call with its key ([`Outbox::open`]) and closes it when the outcome is final
    /// ([`is_final`]); an open one is sent again with the same key by the next call.
    ///
    /// # Errors
    ///
    /// What `send` answers, or no random source for a new key.
    pub fn send<T>(
        &self,
        scope: IdempotentScope,
        drive_id: &str,
        what: &str,
        send: impl FnOnce(&IdempotencyKey) -> Result<T, TokenError>,
    ) -> Result<T, TokenError> {
        let call = self.open(scope, drive_id, what)?;
        let outcome = send(call.key());
        if is_final(&outcome) {
            call.close();
        }
        outcome
    }
}

impl OpenCall {
    /// The key every try of the call carries.
    #[must_use]
    pub fn key(&self) -> &IdempotencyKey {
        &self.key
    }

    /// Whether the key was written down (else only this process asks again with it).
    ///
    /// # Errors
    ///
    /// Why it could not be written.
    pub fn kept(&self) -> Result<(), &str> {
        self.kept.as_ref().map(|_| ()).map_err(String::as_str)
    }

    /// A final answer came: the call is closed. A file that cannot be deleted (the folder turned
    /// read-only) gives its key to the next call of the same scope, drive and words; the token
    /// server answers that only to the same caller, and only while the old grant is unused.
    pub fn close(self) {
        // gone already (another process closed it) is closed too
        let _ = fs::remove_file(&self.path);
    }
}
