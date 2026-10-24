//! A locked drive's way back. A drive unpaid past its read-only time is locked and its files
//! move to offline storage; the token server gives it back only to its owner, and it keeps no
//! name or address to know the owner by. So the owner proves it with a key only the owner has:
//! one of the drive's recovery keys (from the recovery code) or the key this device registered
//! while the drive was open ([`crate::device_key`]). The token server's challenge is signed and
//! answered once; then the owner pays the time the drive was kept unpaid, and the files come
//! back.

use super::*;
use azlin_contract::{
    archive_proof_message, ArchiveChallenge, ArchiveProof, ArchiveRefusal, ArchiveWire,
    DeviceKeyAnswer, DeviceKeyRegistration, ProofKey,
};

/// What a proof came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofAnswer {
    /// Proven; the drive stays locked: it waits for the payment of its grace and read-only
    /// time, or its files are on their way back from offline storage (`restore_pending`).
    Locked(ArchiveWire),
    /// Proven and paid, and its files never left the servers: the drive is open again.
    Restored,
}

/// A refused proof as a [`TokenError`]: the contract's refusals of the proof are refusals
/// ([`TokenError::Refused`] with their code), never "sign in again" - a wrong key or a used
/// challenge says nothing about this device's drive token.
fn proof_refusal(reply: &HttpReply) -> TokenError {
    let said: Option<ArchiveRefusal> = serde_json::from_slice(&reply.body).ok();
    match said {
        Some(r) if r != ArchiveRefusal::Unknown => {
            let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
            TokenError::Refused {
                status: reply.status,
                code: r.as_str().to_string(),
                message: value["message"].as_str().unwrap_or_default().to_string(),
            }
        }
        _ => refusal(reply, true),
    }
}

impl TokenServer<'_> {
    /// A challenge to prove the locked drive `drive_id` is this owner's (`POST
    /// /v1/drives/{id}/archive/challenge`, the drive token): good for five minutes, answered once.
    ///
    /// # Errors
    ///
    /// The token server's refusals (409 `not_archived` for a drive that is not locked).
    pub fn archive_challenge(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<ArchiveChallenge, TokenError> {
        let path = format!("/v1/drives/{}/archive/challenge", check_id(drive_id)?);
        let reply = self.exchange(Method::Post, &path, Some(token_of(drive_token)?), Some(&json!({})))?;
        if !reply.is_success() {
            return Err(proof_refusal(&reply));
        }
        serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the challenge does not read")))
    }

    /// Proves the locked drive `drive_id` is this owner's: a challenge, `sign` signs
    /// [`archive_proof_message`] of it with the `key` (`key_id`, when known: which one), then
    /// `POST /v1/drives/{id}/archive/proof`.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals as [`TokenError::Refused`] with the contract's code: `bad_signature` (no key of
    /// the drive signed it), `no_key` (the drive has none: it cannot be brought back),
    /// `challenge_used`, `bad_challenge`, `not_archived`, `not_verified`.
    pub fn archive_prove(
        &self,
        drive_id: &str,
        drive_token: &str,
        key: ProofKey,
        key_id: Option<&str>,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<ProofAnswer, TokenError> {
        let id = check_id(drive_id)?;
        let challenge = self.archive_challenge(id, drive_token)?;
        let signature = sign(archive_proof_message(id, &challenge.challenge).as_bytes())
            .map_err(|e| TokenError::Config(format!("The proof is not signed: {e}")))?;
        let proof = ArchiveProof {
            challenge: challenge.challenge,
            key,
            key_id: key_id.map(str::to_string),
            signature,
        };
        let body = serde_json::to_value(&proof)
            .map_err(|_| TokenError::Protocol(String::from("the proof does not write")))?;
        let path = format!("/v1/drives/{id}/archive/proof");
        let reply = self.exchange(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))?;
        if !reply.is_success() {
            return Err(proof_refusal(&reply));
        }
        let value: Value = serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?;
        if value["state"].as_str() == Some("restored") {
            return Ok(ProofAnswer::Restored);
        }
        serde_json::from_value(value)
            .map(ProofAnswer::Locked)
            .map_err(|_| TokenError::Protocol(String::from("the answer names no archive")))
    }

    /// This device's key for the open drive `drive_id` (`POST /v1/drives/{id}/device_keys`,
    /// the drive token): its id, which a proof may name. The same key again answers its id.
    ///
    /// # Errors
    ///
    /// The token server's refusals (403 `drive_archived`: a locked drive takes no key; 401
    /// `bad_signature`; 409 `too_many_keys`).
    pub fn register_device_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        registration: &DeviceKeyRegistration,
    ) -> Result<DeviceKeyAnswer, TokenError> {
        let path = format!("/v1/drives/{}/device_keys", check_id(drive_id)?);
        let body = serde_json::to_value(registration)
            .map_err(|_| TokenError::Protocol(String::from("the key does not write")))?;
        let reply = self.exchange(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))?;
        if !reply.is_success() {
            // a bad signature here is the device's key, not its drive token
            if reply.status == 401 {
                return Err(proof_refusal(&reply));
            }
            return Err(refusal(&reply, true));
        }
        serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer names no key")))
    }
}
