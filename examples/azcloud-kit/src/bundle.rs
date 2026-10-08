//! What the token server answers for a drive (a sign-up, a paid checkout, a refresh): the
//! drive's entry for the drives file, its S3 credentials, its drive token.
//!
//! ```json
//! { "drive": { "id": "d_k3f9", "name": "Azlin Storage",
//!              "location": { "kind": "s3", "endpoint": "...", "region": "us-east-1",
//!                            "bucket": "d-k3f9", "path_style": true,
//!                            "auth": { "type": "azlin", "drive_id": "d_k3f9",
//!                                      "account_url": "..." } } },
//!   "credentials": { "access_key_id": "...", "secret_access_key": "...",
//!                    "session_token": "...", "expires_at": "2026-10-08T21:15:00Z" },
//!   "drive_token": "dt_<family>.<generation>.<random>",
//!   "quota_bytes": 100000000000, "read_only": false, "tier": "100GB",
//!   "period_until": "2026-11-07T09:15:00Z", "nodes": [...], "failover": [...] }
//! ```
//!
//! A token server that hands out long-lived keys says `"auth": {"type": "keyring"}` and no
//! `expires_at`. `Debug` never shows a secret.

use std::fmt;

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    time::parse_iso8601,
};
use serde_json::Value;

use crate::{session::AzlinSession, token::TokenError};

/// A bundle's S3 credentials. `Debug` shows none of them.
#[derive(Clone, PartialEq, Eq)]
pub struct BundleCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Temporary credentials' session token.
    pub session_token: Option<String>,
    /// When they stop working, in seconds since 1970; `None`: long-lived keys.
    pub expires_at: Option<u64>,
}

impl fmt::Debug for BundleCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BundleCredentials")
            .field("access_key_id", &"<hidden>")
            .field("secret_access_key", &"<hidden>")
            .field("session_token", &self.session_token.as_ref().map(|_| "<hidden>"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// A drive as the token server describes it. `Debug` never shows a secret.
#[derive(Clone, PartialEq, Eq)]
pub struct DriveBundle {
    /// The drives-file entry as the server wrote it (its name is the server's default; see
    /// [`Self::entry_named`]).
    pub entry: DriveEntry,
    pub credentials: BundleCredentials,
    /// This device's drive token: what the next refresh spends.
    pub drive_token: String,
    /// The tier's size.
    pub quota_bytes: Option<u64>,
    /// The drive takes no writes (unpaid past its grace period, locked down).
    pub read_only: bool,
    /// `100GB`, `1TB`.
    pub tier: Option<String>,
    /// Paid until, in seconds since 1970.
    pub period_until: Option<u64>,
}

impl fmt::Debug for DriveBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DriveBundle")
            .field("entry", &self.entry)
            .field("credentials", &self.credentials)
            .field("drive_token", &"<hidden>")
            .field("quota_bytes", &self.quota_bytes)
            .field("read_only", &self.read_only)
            .field("tier", &self.tier)
            .finish_non_exhaustive()
    }
}

/// A timestamp of a bundle: RFC 3339, or seconds since 1970.
fn time_of(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(parse_iso8601))
}

impl DriveBundle {
    /// Reads a bundle's text.
    pub fn parse(text: &str) -> Result<DriveBundle, TokenError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?;
        DriveBundle::from_value(&value)
    }

    /// Reads a bundle's JSON: refused without a drive, an S3 location, credentials or a drive
    /// token.
    pub fn from_value(value: &Value) -> Result<DriveBundle, TokenError> {
        let entry: DriveEntry = serde_json::from_value(value["drive"].clone()).map_err(|_| {
            TokenError::Protocol(String::from(
                "the answer names no drive with an id, a name and a location",
            ))
        })?;
        if entry.id.trim().is_empty() || !matches!(entry.location, DriveLocation::S3 { .. }) {
            return Err(TokenError::Protocol(String::from(
                "the answer's drive is not an S3 bucket with an id",
            )));
        }
        let credentials = &value["credentials"];
        let text = |v: &Value| v.as_str().unwrap_or_default().trim().to_string();
        let credentials = BundleCredentials {
            access_key_id: text(&credentials["access_key_id"]),
            secret_access_key: text(&credentials["secret_access_key"]),
            session_token: Some(text(&credentials["session_token"])).filter(|t| !t.is_empty()),
            expires_at: time_of(&credentials["expires_at"]),
        };
        if credentials.access_key_id.is_empty() || credentials.secret_access_key.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the answer has no credentials",
            )));
        }
        let drive_token = text(&value["drive_token"]);
        if drive_token.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the answer has no drive token",
            )));
        }
        Ok(DriveBundle {
            entry,
            credentials,
            drive_token,
            quota_bytes: value["quota_bytes"].as_u64(),
            read_only: value["read_only"].as_bool().unwrap_or(false),
            tier: value["tier"].as_str().map(str::to_string),
            period_until: time_of(&value["period_until"]),
        })
    }

    /// The drive's id (`d_...`).
    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.entry.id
    }

    /// The drives-file entry under the user's `name`; an Azlin drive whose bundle names no
    /// token server gets `token_url`, the one it came from (its refreshes go there).
    #[must_use]
    pub fn entry_named(&self, name: &str, token_url: &str) -> DriveEntry {
        let mut entry = self.entry.clone();
        let name = name.trim();
        if !name.is_empty() {
            entry.name = name.to_string();
        }
        if let DriveLocation::S3 {
            auth: DriveAuth::Azlin { account_url, .. },
            ..
        } = &mut entry.location
        {
            if account_url.trim().is_empty() {
                *account_url = token_url.trim().trim_end_matches('/').to_string();
            }
        }
        entry
    }

    /// What the keyring keeps for the drive: the drive token and these credentials.
    #[must_use]
    pub fn session(&self) -> AzlinSession {
        AzlinSession {
            drive_id: self.entry.id.clone(),
            drive_token: self.drive_token.clone(),
            access_key_id: self.credentials.access_key_id.clone(),
            secret_access_key: self.credentials.secret_access_key.clone(),
            session_token: self.credentials.session_token.clone(),
            expires_at: self.credentials.expires_at,
        }
    }
}
