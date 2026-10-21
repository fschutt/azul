//! The drive's recovery keys: what they sign, the lockdown by a key, the lookup, adding,
//! removing and replacing one.

use super::*;

/// What a recovery-key lockdown answers (202): the drive is read-only until `pending_until`
/// (the recovery code may cancel until then), and `drive_token` is the new family this side gets
/// after it. `Debug` shows no token.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryLockdown {
    /// The request's nonce (new for every request).
    pub nonce: String,
    /// In seconds since 1970.
    pub pending_until: Option<u64>,
    pub drive_token: String,
}

impl fmt::Debug for RecoveryLockdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryLockdown")
            .field("nonce", &self.nonce)
            .field("pending_until", &self.pending_until)
            .field("drive_token", &"<hidden>")
            .finish()
    }
}

/// The member a recovery-key lockdown adds while it is pending: the device that holds the
/// recovery code, the owner's when the 48 hours are over.
pub const RECOVERY_MEMBER: &str = "recovery-pending";

/// What a recovery-key lockdown signs: `lockdown:<drive>:<nonce>`.
#[must_use]
pub fn recovery_lockdown_message(drive_id: &str, nonce: &str) -> String {
    format!("lockdown:{drive_id}:{nonce}")
}

/// What a cancel of a recovery-key lockdown signs (F12: "the recovery code always wins"):
/// `lockdown-cancel:<drive>:<nonce>`.
#[must_use]
pub fn lockdown_cancel_message(drive_id: &str, nonce: &str) -> String {
    format!("lockdown-cancel:{drive_id}:{nonce}")
}

/// What a change of the recovery key signs with the CURRENT key (F12 C):
/// `recovery:<drive>:<new key>:<nonce>`.
#[must_use]
pub fn recovery_key_message(drive_id: &str, new_key: &str, nonce: &str) -> String {
    format!("recovery:{drive_id}:{new_key}:{nonce}")
}

/// What a recovery lookup signs: `recovery-lookup:<challenge>`.
#[must_use]
pub fn recovery_lookup_message(challenge: &str) -> String {
    format!("recovery-lookup:{challenge}")
}

/// What adding a recovery key signs (by a current key of the drive): `recovery-add:<drive>:<the
/// new key as sent>:<nonce>`.
#[must_use]
pub fn recovery_add_message(drive_id: &str, public_key_base64: &str, nonce: &str) -> String {
    format!("recovery-add:{drive_id}:{public_key_base64}:{nonce}")
}

/// What removing one signs: `recovery-remove:<drive>:<key_id>:<nonce>`.
#[must_use]
pub fn recovery_remove_message(drive_id: &str, key_id: &str, nonce: &str) -> String {
    format!("recovery-remove:{drive_id}:{key_id}:{nonce}")
}

/// A drive a recovery key belongs to ([`TokenServer::recovery_lookup`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDrive {
    pub drive_id: String,
    /// The key's id at the drive (`rk_...`; `rk_legacy` for a key from before several keys).
    pub key_id: String,
}

/// One of a drive's recovery keys as the token server lists it ([`TokenServer::recovery_keys`]):
/// public halves only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryKeyInfo {
    pub key_id: String,
    /// The owner's words (`recovery code` for the kit's).
    pub label: String,
    /// Ed25519, standard base64.
    pub recovery_pubkey: String,
    /// The day it was added (seconds since 1970); none for a key from before.
    pub created_at: Option<u64>,
    /// A token master MAC'd it: it signs lockdowns and lookups (not yet: 503 `not_verified`).
    pub verified: bool,
}

impl RecoveryKeyInfo {
    fn from_value(value: &Value) -> RecoveryKeyInfo {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        RecoveryKeyInfo {
            key_id: text("key_id"),
            label: text("label"),
            recovery_pubkey: text("recovery_pubkey"),
            created_at: value["created_at"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
            verified: value["verified"].as_bool().unwrap_or(false),
        }
    }
}

/// A lockdown nonce: 16 random bytes, hex (32 characters; the token server takes 16 to 128 and
/// each once per drive).
fn lockdown_nonce() -> Result<String, TokenError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| {
        TokenError::Config(String::from(
            "This computer's random source does not answer: no lockdown request can be made.",
        ))
    })?;
    Ok(crate::period::hex(&bytes))
}

// ==== The client ====

impl<'a> TokenServer<'a> {
    /// Registers the drive's recovery key (`POST /v1/drives/{id}/recovery {"recovery_pubkey"}`,
    /// [`crate::recovery::RecoveryKey::public_base64`]): what a lockdown without a drive token
    /// is signed with. A grant: send it with the newest drive token
    /// ([`crate::SharedKeyring::with_drive_token`]).
    pub fn set_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
    ) -> Result<(), TokenError> {
        let path = format!("/v1/drives/{}/recovery", check_id(drive_id)?);
        let body = json!({ "recovery_pubkey": public_key_base64.trim() });
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
            .map(|_| ())
    }

    /// The lockdown by the drive's recovery key (`POST /v1/drives/{id}/lockdown {"nonce",
    /// "signature"}`): `sign` signs [`recovery_lockdown_message`] of a nonce made new for this
    /// request (the token server answers a request it saw before with 409 `nonce_used`) and
    /// answers the signature as the token server takes it (base64). It takes effect after 48 h
    /// unless a device cancels it; no drive token is sent.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals (`nonce_used`, `no_recovery_key`, a bad signature: 401 without a drive token is
    /// a refusal too).
    pub fn recovery_lockdown(
        &self,
        drive_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<RecoveryLockdown, TokenError> {
        let id = check_id(drive_id)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_lockdown_message(id, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The lockdown request is not signed: {e}")))?;
        let path = format!("/v1/drives/{id}/lockdown");
        let body = json!({ "nonce": nonce, "signature": signature });
        let value = self.call(Method::Post, &path, None, Some(&body))?;
        Ok(RecoveryLockdown {
            nonce,
            pending_until: value["pending_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
            drive_token: value["drive_token"].as_str().unwrap_or_default().to_string(),
        })
    }

    /// The drives the recovery key `public_key_base64` belongs to, for a computer that never had
    /// them (§18.8: the kit leaves the drive id out): a challenge (`POST /v1/recovery/challenge`),
    /// `sign` signs [`recovery_lookup_message`] of it with the key, then `POST
    /// /v1/recovery/lookup`. No drive token. An empty list: no drive has the key.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (the lookup is not sent); the token
    /// server's refusals (401 `bad_challenge` or `unauthorized`, 503 `not_verified`, 429
    /// `rate_limited`).
    pub fn recovery_lookup(
        &self,
        public_key_base64: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<Vec<FoundDrive>, TokenError> {
        let value = self.call(Method::Post, "/v1/recovery/challenge", None, Some(&json!({})))?;
        let challenge = value["challenge"].as_str().unwrap_or_default().to_string();
        if challenge.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the recovery challenge is missing",
            )));
        }
        let signature = sign(recovery_lookup_message(&challenge).as_bytes())
            .map_err(|e| TokenError::Config(format!("The lookup is not signed: {e}")))?;
        let body = json!({
            "recovery_pubkey": public_key_base64.trim(),
            "challenge": challenge,
            "signature": signature,
        });
        let value = self.call(Method::Post, "/v1/recovery/lookup", None, Some(&body))?;
        Ok(value["drives"]
            .as_array()
            .map(|drives| {
                drives
                    .iter()
                    .filter_map(|d| {
                        Some(FoundDrive {
                            drive_id: d["drive_id"].as_str()?.to_string(),
                            key_id: d["key_id"].as_str().unwrap_or_default().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The drive's recovery keys (`GET /v1/drives/{id}/recovery_keys`, a read).
    pub fn recovery_keys(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<Vec<RecoveryKeyInfo>, TokenError> {
        let path = format!("/v1/drives/{}/recovery_keys", check_id(drive_id)?);
        let value = self.call(Method::Get, &path, Some(token_of(drive_token)?), None)?;
        Ok(value["keys"]
            .as_array()
            .map(|keys| keys.iter().map(RecoveryKeyInfo::from_value).collect())
            .unwrap_or_default())
    }

    /// One more recovery key of the drive (`POST /v1/drives/{id}/recovery_keys`, a grant): `sign`
    /// signs [`recovery_add_message`] with a CURRENT key of the drive (F12: the drive token
    /// alone adds none once the drive has a key).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals (403 `recovery_key_required`, 401 a key that is not the drive's, 409
    /// `recovery_key_exists` / `too_many_recovery_keys`, 403 `lockdown_pending`).
    pub fn add_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
        label: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<RecoveryKeyInfo, TokenError> {
        let id = check_id(drive_id)?;
        let token = token_of(drive_token)?;
        let public = public_key_base64.trim();
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_add_message(id, public, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The new key is not signed: {e}")))?;
        let body = json!({
            "recovery_pubkey": public,
            "label": label.trim(),
            "nonce": nonce,
            "signature": signature,
        });
        let path = format!("/v1/drives/{id}/recovery_keys");
        let value = self.call(Method::Post, &path, Some(token), Some(&body))?;
        Ok(RecoveryKeyInfo::from_value(&value))
    }

    /// One recovery key fewer (`DELETE /v1/drives/{id}/recovery_keys/{key_id}`, a grant): `sign`
    /// signs [`recovery_remove_message`] with a current key. Never the drive's last key.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] for a key id that is no path segment, or when `sign` cannot sign
    /// (nothing is sent); the token server's refusals (409 `last_recovery_key`, 404
    /// `no_such_key`, 403 `recovery_key_required`).
    pub fn remove_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        key_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<(), TokenError> {
        let id = check_id(drive_id)?;
        let key = check_id(key_id)?;
        let token = token_of(drive_token)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_remove_message(id, key, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The removal is not signed: {e}")))?;
        let body = json!({ "nonce": nonce, "signature": signature });
        let path = format!("/v1/drives/{id}/recovery_keys/{key}");
        self.call_or_null(Method::Delete, &path, Some(token), Some(&body))
            .map(|_| ())
    }

    /// Cancels a pending recovery-key lockdown (`POST /v1/drives/{id}/lockdown/cancel {"nonce",
    /// "signature"}`, F12 - "the recovery code always wins"): `sign` signs
    /// [`lockdown_cancel_message`] of a nonce made new for this request with the drive's
    /// recovery key (whoever holds the code; a device's drive token alone cancels nothing, so no
    /// token is sent). A 409 `no_pending_lockdown` when none is pending, `nonce_used` for a
    /// request the server saw before.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals.
    pub fn lockdown_cancel_signed(
        &self,
        drive_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<Value, TokenError> {
        let id = check_id(drive_id)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(lockdown_cancel_message(id, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The cancel request is not signed: {e}")))?;
        let path = format!("/v1/drives/{id}/lockdown/cancel");
        let body = json!({ "nonce": nonce, "signature": signature });
        self.call_or_null(Method::Post, &path, None, Some(&body))
    }

    /// Replaces the drive's recovery key with `public_key_base64` (`POST
    /// /v1/drives/{id}/recovery {"recovery_pubkey", "nonce", "signature"}`, F12 C): the CURRENT
    /// key signs [`recovery_key_message`] of the new key and a fresh nonce - a thief with a
    /// device token cannot swap the owner's code out. A grant: with the newest drive token. The
    /// drive's first key needs no signature ([`Self::set_recovery_key`]).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign_with_current` cannot sign (nothing is sent); the token
    /// server's refusals (401 a signature of another key).
    pub fn replace_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
        sign_with_current: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<(), TokenError> {
        let id = check_id(drive_id)?;
        let new_key = public_key_base64.trim();
        let nonce = lockdown_nonce()?;
        let signature = sign_with_current(recovery_key_message(id, new_key, &nonce).as_bytes())
            .map_err(|e| {
                TokenError::Config(format!("The new recovery key is not signed: {e}"))
            })?;
        let path = format!("/v1/drives/{id}/recovery");
        let body = json!({ "recovery_pubkey": new_key, "nonce": nonce, "signature": signature });
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
            .map(|_| ())
    }
}
