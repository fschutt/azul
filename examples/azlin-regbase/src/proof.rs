//! Key proofs: "device key or token" (planning/architecture.md section 2.4 "refresh: device key or
//! token -> short-lived credentials"; DRIFT X-D-07 of S9b; build unit S9, lane W1-S9a).
//!
//! A device that lost its token (or whose family was revoked by a reuse) refreshes with its device
//! KEY instead: the node gives it a [`Challenge`], the device signs a message that names the
//! purpose, the account, the subject and that challenge, and the registry checks the signature with
//! the `sign_pub` it holds. The check is PURE: an Ed25519 verification and nothing else (no clock,
//! no randomness, no registry read); whether the challenge is fresh and whether the key is the
//! registered one are the caller's (S9b, S9c).
//!
//! The message is `signing_input(domain, account || subject || challenge)` with one domain per
//! purpose, so a proof made for one purpose is no proof for another ([`ProofPurpose::domain`]).
//! The domain is NUL-free ASCII and the preimage is `domain || 0x00 || bytes` (R4): two purposes
//! never give equal preimages. Verification is `verify_strict` (azlin-keys): a non-canonical
//! signature or a small-order key is refused like any other bad proof.
//!
//! `subject` is the 16 bytes of the id the purpose is about: the DEVICE id for
//! [`ProofPurpose::Refresh`] and [`ProofPurpose::EnrollFinish`], the MEMBER id of the app principal
//! for [`ProofPurpose::AppRefresh`].

use azlin_keys::sign::{sign_preimage, verify_preimage};
use azlin_keys::{Entropy, EntropyError, SignKeypair};

use crate::error::ProofError;
use crate::ids::AccountId;
use crate::sign::{
    signing_input, Domain, PublicKey32, Signature64, APP_REFRESH_V1, ENROLL_FINISH_V1, REFRESH_V1,
};

/// A challenge a node gives a device to sign: 16 random bytes, drawn by the node (the registry
/// draws nothing). Public data.
///
/// Implements planning/architecture.md section 2.4 (device key proof); the challenge travels in the
/// `hello` answer (X-D-07).
///
/// Caller: S6 (the `hello` answer draws it), S9b and S9c (the proof's message), S9h (the client
/// signs it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Challenge([u8; 16]);

impl Challenge {
    /// The number of bytes of a challenge.
    pub const LEN: usize = 16;

    /// The challenge with these bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Challenge {
        Challenge(bytes)
    }

    /// The bytes of the challenge.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// A fresh challenge from the entropy seam; on a failure there is none (never a challenge of
    /// zeroes).
    pub fn generate(ent: &mut dyn Entropy) -> Result<Challenge, EntropyError> {
        let mut bytes = [0u8; 16];
        ent.fill(&mut bytes)?;
        Ok(Challenge(bytes))
    }
}

/// What a key proof is for: each purpose has its own signature domain.
///
/// Implements planning/architecture.md section 2.4 and section 5.3 (app principals refresh with a
/// key pair, never a rotating token).
///
/// Caller: S9b (`Refresh` for devices, `AppRefresh` for app principals), S9c (`EnrollFinish`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProofPurpose {
    /// A device refreshes with its key instead of its token.
    Refresh,
    /// A new device claims its first token.
    EnrollFinish,
    /// An app principal refreshes with its key pair.
    AppRefresh,
}

impl ProofPurpose {
    /// Every purpose.
    pub const ALL: [ProofPurpose; 3] = [
        ProofPurpose::Refresh,
        ProofPurpose::EnrollFinish,
        ProofPurpose::AppRefresh,
    ];

    /// The signature domain of the purpose: `azlin-refresh-v1`, `azlin-enroll-finish-v1` or
    /// `azlin-app-refresh-v1`.
    pub const fn domain(self) -> Domain {
        match self {
            ProofPurpose::Refresh => REFRESH_V1,
            ProofPurpose::EnrollFinish => ENROLL_FINISH_V1,
            ProofPurpose::AppRefresh => APP_REFRESH_V1,
        }
    }
}

/// A key proof as it arrives: the challenge it answers and the signature over the proof message.
///
/// Caller: S9b (`DeviceProof::Key`), S9c (`EnrollFinish`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyProof {
    /// The challenge the signature answers.
    pub challenge: Challenge,
    /// The Ed25519 signature over [`proof_message`].
    pub signature: Signature64,
}

impl KeyProof {
    /// A proof from a challenge and signature BYTES: exactly 64 bytes, else
    /// [`ProofError::SignatureLength`] (a truncated or padded signature never reaches the
    /// verifier).
    pub fn from_parts(challenge: Challenge, signature: &[u8]) -> Result<KeyProof, ProofError> {
        let bytes = <[u8; 64]>::try_from(signature).map_err(|_| ProofError::SignatureLength)?;
        Ok(KeyProof {
            challenge,
            signature: Signature64::from_bytes(bytes),
        })
    }
}

/// The bytes a device signs for a proof: `signing_input(purpose's domain, account || subject ||
/// challenge)`.
///
/// Caller: `verify_key_proof`, `sign_key_proof`.
pub fn proof_message(
    purpose: ProofPurpose,
    account: &AccountId,
    subject: &[u8; 16],
    challenge: &Challenge,
) -> Vec<u8> {
    let body = [
        account.as_bytes().as_slice(),
        subject.as_slice(),
        challenge.as_bytes().as_slice(),
    ]
    .concat();
    signing_input(&purpose.domain(), &body)
}

/// Whether `proof` is a valid signature by `public` over exactly this purpose, account, subject and
/// the proof's own challenge. A proof for another purpose, account, subject or challenge, a
/// changed signature and another key are all [`ProofError::Bad`]; nothing panics on any input.
///
/// Pure: whether the challenge is fresh and `public` is the registered key is the caller's.
///
/// Implements planning/architecture.md section 2.4 ("refresh: device key or token").
///
/// Caller: S9b (`apply_refresh` with `DeviceProof::Key`, app principals), S9c (`EnrollFinish`).
pub fn verify_key_proof(
    purpose: ProofPurpose,
    account: &AccountId,
    subject: &[u8; 16],
    public: &PublicKey32,
    proof: &KeyProof,
) -> Result<(), ProofError> {
    let message = proof_message(purpose, account, subject, &proof.challenge);
    verify_preimage(&message, public, &proof.signature).map_err(|_| ProofError::Bad)
}

/// The proof a device makes: its signature over [`proof_message`] for `challenge`.
///
/// Seam: S9h (azlin-session signs the node's challenge with the device key) and the tests of S9b
/// and S9c.
pub fn sign_key_proof(
    purpose: ProofPurpose,
    account: &AccountId,
    subject: &[u8; 16],
    challenge: &Challenge,
    key: &SignKeypair,
) -> KeyProof {
    let message = proof_message(purpose, account, subject, challenge);
    KeyProof {
        challenge: *challenge,
        signature: sign_preimage(&message, key),
    }
}
