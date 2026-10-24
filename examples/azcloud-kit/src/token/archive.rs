//! A locked drive's way back. A drive unpaid past its read-only time is locked and its files
//! move to offline storage; the token server gives it back only to its owner, and it keeps no
//! name or address to know the owner by. So the owner proves it with a key only the owner has:
//! one of the drive's recovery keys - the recovery code's, or this device's own key registered as
//! one while the code was at hand ([`crate::device_key`]). The signature covers the token
//! server's challenge as an [`archive_statement`] - its purpose, the drive, the nonce, the expiry
//! and the paid checkout it unlocks - and counts once; the checkout pays the three months the
//! drive owes, and the files come back.

use super::*;
use azlin_contract::{
    archive_statement, ArchiveChallenge, ArchiveProof, ArchiveRefusal, ArchiveWire,
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

    /// Proves the locked drive `drive_id` is this owner's: a challenge, `sign` signs its
    /// [`archive_statement`] with one of the drive's recovery keys (`key_id`, when known: which
    /// one) naming the paid checkout `checkout_id` it unlocks (none: the proof alone), then
    /// `POST /v1/drives/{id}/archive/proof`.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals as [`TokenError::Refused`] with the contract's code: `bad_signature` (no key of
    /// the drive signed it), `no_key` (the drive has none: it cannot be brought back),
    /// `challenge_used`, `bad_challenge`, `not_archived`, `not_verified`, `checkout_not_paid`,
    /// `checkout_other_tier`.
    pub fn archive_prove(
        &self,
        drive_id: &str,
        drive_token: &str,
        checkout_id: Option<&str>,
        key_id: Option<&str>,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<ProofAnswer, TokenError> {
        let id = check_id(drive_id)?;
        let challenge = self.archive_challenge(id, drive_token)?;
        let statement = archive_statement(
            id,
            &challenge.nonce,
            challenge.expires_at.unix(),
            checkout_id,
        );
        let signature = sign(statement.as_bytes())
            .map_err(|e| TokenError::Config(format!("The proof is not signed: {e}")))?;
        let proof = ArchiveProof {
            challenge: challenge.challenge,
            checkout_id: checkout_id.map(str::to_string),
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
}
