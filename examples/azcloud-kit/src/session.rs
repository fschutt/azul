//! What an app keeps in the OS keyring for an Azlin drive: the drive token (a refresh token, a
//! new one with every refresh) and the S3 credentials of the last refresh. ONE JSON text under
//! azul-storage's `config::keyring_key(<drive id>)` - the entry an S3 drive's credentials have,
//! and readable as them (`Credentials::from_keyring_secret` takes the fields it knows), so an
//! app without this kit still opens the drive until the credentials run out. Where the drive
//! is (endpoint, bucket) is its drives-file entry's, not the session's.

use std::fmt;

use azul_storage::Credentials;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

use crate::token::TokenError;

/// Credentials with less than this left are refreshed before they are used (seconds).
pub const REFRESH_MARGIN_SECS: u64 = 3600;
/// Credentials are used as they are while more than this is left (seconds): a call never
/// starts with credentials about to run out.
pub const VALID_MARGIN_SECS: u64 = 60;

/// A drive token: spent once by a refresh, which answers the next one. No `Clone`, `Debug` or
/// `Display`: a copy that outlives the spend is how a device ends up sending a spent token (the
/// token server then revokes it), so the refresh takes it only from the keyring under the
/// drive's lock ([`crate::shared::SharedKeyring::spend`]). Wiped from memory when dropped.
pub struct DriveToken(Zeroizing<String>);

impl DriveToken {
    /// `text`, trimmed.
    #[must_use]
    pub fn new(text: &str) -> DriveToken {
        DriveToken(Zeroizing::new(text.trim().to_string()))
    }

    /// The token, for the request that sends it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether there is no token.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PartialEq for DriveToken {
    fn eq(&self, other: &DriveToken) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for DriveToken {}

impl PartialEq<str> for DriveToken {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for DriveToken {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Serialize for DriveToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DriveToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<DriveToken, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        Ok(DriveToken::new(&text))
    }
}

/// The keyring's text of an Azlin drive. `Debug` shows no secret; no `Clone` (its drive token
/// has none): read it again from the keyring.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
pub struct AzlinSession {
    pub drive_id: String,
    pub drive_token: DriveToken,
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
        let session = Self::from_keyring_secret_without_credentials(text)?;
        if session.drive_token.trim().is_empty() || session.access_key_id.trim().is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the keyring entry has no drive token or no credentials",
            )));
        }
        Ok(session)
    }

    /// The keyring entry's text read back whether or not it holds credentials yet. A drive
    /// being recovered keeps only its drive token while the token server's notice runs (no
    /// credentials are handed out meanwhile); finishing the recovery trades that token for
    /// them. Everything that opens the bucket reads the entry with
    /// [`Self::from_keyring_secret`].
    pub fn from_keyring_secret_without_credentials(text: &str) -> Result<AzlinSession, TokenError> {
        // The parser's message is not passed on: it could quote the secret.
        serde_json::from_str(text.trim()).map_err(|_| {
            TokenError::Protocol(String::from(
                "the keyring entry does not hold an Azlin drive's session",
            ))
        })
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
