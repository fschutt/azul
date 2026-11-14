//! Algorithm 1: the X25519 sealed box (planning/architecture.md section 5.5, "1 X25519 sealed
//! box"; build unit S15, lane W1-S15b), and the X25519 half the hybrid shares.
//!
//! `wrap_x25519` draws an ephemeral X25519 key from the entropy seam, agrees on a shared secret
//! with the recipient's key (an all-zero result, which a key of low order gives, is REFUSED:
//! such a secret keys nothing), derives the AEAD key with `combiner::alg1_key`, and seals the
//! plaintext under the header as associated data. The draws are, in this order: the 32-byte
//! ephemeral secret, then the 24-byte nonce.
//!
//! This is the construction of today's member wrap (`crypto/keys.rs`: ephemeral key, low-order
//! result refused, BLAKE3 `derive_key` over the shared secret, the ephemeral and the member key,
//! XChaCha20-Poly1305), with the header authenticated and the whole envelope signed.

use x25519_dalek::{PublicKey, StaticSecret};

use super::alg_table::Alg;
use super::combiner::{alg1_key, Alg1Input};
use super::ctx::Ctx;
use super::error::WrapError;
use super::opening::seal_assembled;
use super::wire::{Envelope, Header, Params, MAX_PLAINTEXT};
use crate::ids::{DeviceId, KeyId};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::SignKeypair;

/// A recipient that has an X25519 key only: the public key an envelope of algorithm 1 is
/// sealed to. Its `rcpt` is `KeyId::of_bundle(kx, None)`.
///
/// Caller: INT-S15-1, S15c1 (`GroupKey::as_recipient`: a space key is wrapped to a group's
/// X25519 key), S15d (a contact's key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecipientX25519 {
    /// The recipient's X25519 public key (the `kx_pub` column).
    pub kx: [u8; 32],
}

impl RecipientX25519 {
    /// The `rcpt` of an envelope sealed to this recipient: the id of the bundle `(kx)` with no
    /// ML-KEM key.
    pub fn key_id(&self) -> KeyId {
        KeyId::of_bundle(&self.kx, None)
    }
}

/// The X25519 public key of a secret: `mul_base` of the clamped scalar.
///
/// The temporary `StaticSecret` is wiped when it drops.
pub(crate) fn public_of(secret: &Secret<32>) -> [u8; 32] {
    let key = StaticSecret::from(*secret.expose());
    PublicKey::from(&key).to_bytes()
}

/// The X25519 shared secret of `secret` and the public key `public`, or `None` when it is the
/// all-zero secret that a key of low order gives (it depends on the public key alone, never on
/// the secret). Every caller refuses `None`: a non-contributory result never keys anything.
pub(crate) fn dh(secret: &Secret<32>, public: &[u8; 32]) -> Option<Secret<32>> {
    let key = StaticSecret::from(*secret.expose());
    let shared = key.diffie_hellman(&PublicKey::from(*public));
    if shared.was_contributory() {
        Some(Secret::from_bytes(*shared.as_bytes()))
    } else {
        None
    }
}

/// Wraps `plaintext` to the X25519 key `r` as an envelope of algorithm 1 for the context `ctx`,
/// signed by the device `signer` with `key`.
///
/// The ephemeral key and the nonce come from `ent` (32, then 24 bytes); an entropy failure, a
/// recipient key of low order ([`WrapError::NonContributory`]) and a plaintext longer than
/// `MAX_PLAINTEXT` each return an error and NO envelope.
///
/// Caller: INT-S15-1 (azul-storage wraps), S15c1 (member, group and space wraps for recipients
/// without an ML-KEM key), S15d (alg 5 over alg 1).
pub fn wrap_x25519(
    r: &RecipientX25519,
    ctx: &Ctx,
    plaintext: &[u8],
    signer: &DeviceId,
    key: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, WrapError> {
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(WrapError::PlaintextTooLarge);
    }
    let ephemeral = Secret::<32>::generate(ent)?;
    let epk = public_of(&ephemeral);
    let shared = dh(&ephemeral, &r.kx).ok_or(WrapError::NonContributory)?;
    let aead_key = alg1_key(&Alg1Input {
        shared: &shared,
        epk: &epk,
        rcpt_kx: &r.kx,
    });
    let header = Header {
        alg: Alg::X25519SealedBox,
        rcpt: r.key_id(),
        epk: Some(epk),
        kem_ct: None,
        params: Params::None,
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_assembled(&header, &aead_key, plaintext, key, ent)
}
