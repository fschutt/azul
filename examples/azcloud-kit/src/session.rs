//! What an app keeps in the OS keyring for an Azlin drive: the drive token (a refresh token, a
//! new one with every refresh) and the S3 credentials of the last refresh. ONE JSON text under
//! azul-storage's `config::keyring_key(<drive id>)` - the entry an S3 drive's credentials have,
//! and readable as them (`Credentials::from_keyring_secret` takes the fields it knows), so an
//! app without this kit still opens the drive until the credentials run out. Where the drive
//! is (endpoint, bucket) is its drives-file entry's, not the session's.

use std::fmt;

use azul_storage::Credentials;
use serde::{Deserialize, Serialize};

use crate::token::TokenError;

/// Credentials with less than this left are refreshed before they are used (seconds).
pub const REFRESH_MARGIN_SECS: u64 = 3600;
/// Credentials are used as they are while more than this is left (seconds): a call never
/// starts with credentials about to run out.
pub const VALID_MARGIN_SECS: u64 = 60;

/// The keyring's text of an Azlin drive. `Debug` shows no secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzlinSession {
    pub drive_id: String,
    pub drive_token: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// When the credentials stop working, in seconds since 1970; `None`: keys that do not
    /// expire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

impl fmt::Debug for AzlinSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzlinSession")
            .field("drive_id", &self.drive_id)
            .field("drive_token", &"<hidden>")
            .field("credentials", &"<hidden>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl AzlinSession {
    /// The keyring entry's text (JSON).
    #[must_use]
    pub fn to_keyring_secret(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The keyring entry's text read back.
    pub fn from_keyring_secret(text: &str) -> Result<AzlinSession, TokenError> {
        // The parser's message is not passed on: it could quote the secret.
        let session: AzlinSession = serde_json::from_str(text.trim()).map_err(|_| {
            TokenError::Protocol(String::from(
                "the keyring entry does not hold an Azlin drive's session",
            ))
        })?;
        if session.drive_token.trim().is_empty() || session.access_key_id.trim().is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the keyring entry has no drive token or no credentials",
            )));
        }
        Ok(session)
    }

    /// The S3 credentials (with the session token of temporary ones).
    #[must_use]
    pub fn credentials(&self) -> Credentials {
        let credentials = Credentials::new(&self.access_key_id, &self.secret_access_key);
        match self.session_token.as_deref().filter(|t| !t.is_empty()) {
            Some(token) => credentials.with_session_token(token),
            None => credentials,
        }
    }

    /// The credentials must be refreshed before they are used at `now` (seconds since 1970):
    /// less than [`REFRESH_MARGIN_SECS`] are left.
    #[must_use]
    pub fn needs_refresh(&self, now: u64) -> bool {
        self.expires_at
            .is_some_and(|at| at <= now.saturating_add(REFRESH_MARGIN_SECS))
    }

    /// The credentials still work at `now`, with [`VALID_MARGIN_SECS`] to spare.
    #[must_use]
    pub fn is_valid_at(&self, now: u64) -> bool {
        self.expires_at
            .is_none_or(|at| at > now.saturating_add(VALID_MARGIN_SECS))
    }
}
