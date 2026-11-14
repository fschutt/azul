//! Sealed fields: display data (names, emails) encrypted under the account's seal key (planning/
//! architecture.md section 5.9 "`sealed` | display data (names, emails), encrypted under the account's
//! seal key, which only members hold (`reg_wrap` kind `seal`; deleting it shreds them, 5.17)"; section
//! 5.17 "The registry rows go. The team log is append-only and hash-chained, so its entries stay, but
//! their `sealed` fields (names, emails) become unreadable everywhere ... and deleting that key shreds
//! them"; build unit S17, lane W1-S17c2).
//!
//! A sealed field is `version (1) || nonce (24) || ciphertext || tag (16)`: XChaCha20-Poly1305 with a
//! fresh random 24-byte nonce from the entropy seam, and the associated data
//! `"azlin-teamlog-sealed-v1" || 0x00 || entry hash || field name`. The associated data BINDS the field
//! to its entry and to its name: a sealed name moved to another entry, or a sealed email read as a
//! sealed name, does not open. The check (`admit`) never needs the seal key and never reads a sealed
//! field (it is opaque bytes with a size limit), so destroying the key leaves the chain verifying and
//! every field unreadable; nothing in this crate caches a plaintext (an opened field is a `Zeroizing`
//! buffer).

use azlin_seams::{ByteCount, Entropy, EntropyError, Hash32, Secret};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::ops::SEALED_MAX;

/// The version byte of a sealed field.
const VERSION: u8 = 1;

/// The length of the XChaCha20 nonce.
const NONCE_LEN: usize = 24;

/// The length of the Poly1305 tag.
const TAG_LEN: usize = 16;

/// The prefix of the associated data: a context no other use of the key can share.
const AD_CONTEXT: &[u8] = b"azlin-teamlog-sealed-v1";

/// The account's seal key: 32 bytes, wiped on drop, never printed (a `Secret`). It is wrapped to every
/// member (`reg_wrap` kind `seal`); this crate only uses it.
///
/// Implements planning/architecture.md section 5.9 (`sealed`) and section 5.17 (the seal key shreds the
/// sealed fields).
///
/// Caller: `seal_field`, `open_field`; the glue that unwraps the seal key from `reg_wrap`.
#[derive(Debug)]
pub struct SealKey(Secret<32>);

impl SealKey {
    /// The seal key made of these 32 bytes.
    ///
    /// Implements planning/architecture.md section 5.9.
    ///
    /// Caller: the glue that unwraps the seal key, the tests.
    pub fn new(secret: Secret<32>) -> SealKey {
        SealKey(secret)
    }

    /// A fresh seal key from the entropy seam; on an entropy failure there is no key.
    ///
    /// Implements planning/architecture.md section 5.9 and section 1.4 rule 1 (randomness is a
    /// parameter).
    ///
    /// Caller: the owner's device at the account's creation (the keys lane), the tests.
    pub fn generate(ent: &mut dyn Entropy) -> Result<SealKey, EntropyError> {
        Ok(SealKey(Secret::generate(ent)?))
    }
}

/// Why a field could not be sealed or opened.
///
/// Implements planning/architecture.md section 5.9 and section 5.17.
///
/// Caller: `seal_field`, `open_field`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    /// The entropy source gave no nonce: nothing was sealed.
    #[error("the random source gave no nonce: {0}")]
    Entropy(EntropyError),
    /// The sealed field would be longer than the limit of a sealed field.
    #[error("the sealed field would be longer than the limit")]
    TooLong,
    /// The bytes are no sealed field (too short, or another version).
    #[error("the sealed field is malformed")]
    Malformed,
    /// The field does not open under this key, this entry hash and this field name: a wrong key, a
    /// changed byte, a field of another entry, or another field's name.
    #[error("the sealed field does not open under this key, entry and field name")]
    Open,
}

/// The associated data of a field: the context, the entry's hash and the field's name.
fn associated_data(entry_hash: &Hash32, field: &str) -> Vec<u8> {
    let mut ad = Vec::with_capacity(AD_CONTEXT.len() + 1 + Hash32::LEN + field.len());
    ad.extend_from_slice(AD_CONTEXT);
    ad.push(0);
    ad.extend_from_slice(entry_hash.as_bytes());
    ad.extend_from_slice(field.as_bytes());
    ad
}

/// The cipher of a key. The key is always 32 bytes, so building it cannot fail; a failure would be a
/// malformed field in the caller's eyes.
fn cipher(key: &SealKey) -> Result<XChaCha20Poly1305, SealError> {
    XChaCha20Poly1305::new_from_slice(key.0.expose()).map_err(|_| SealError::Malformed)
}

/// Seals `plain` as the field `field` of the entry with hash `entry_hash`: a fresh nonce from `e`, then
/// XChaCha20-Poly1305 with the entry hash and the field name as associated data.
///
/// The sealed field is at most `SEALED_MAX` long, so `plain` is at most `SEALED_MAX` minus the 41 bytes
/// of version, nonce and tag. The brief's signature returns the bytes alone; a failed entropy draw and an
/// over-long field have no honest value to return, so this returns a `Result` (drift row of lane
/// W1-S17c).
///
/// Implements planning/architecture.md section 5.9 (`sealed`) and section 5.17 (encrypted under the
/// account's seal key).
///
/// Caller: the device that writes an entry with a display name (AzWorkspace, the keys lane's
/// glue), the tests.
pub fn seal_field(
    k: &SealKey,
    entry_hash: &Hash32,
    field: &str,
    plain: &[u8],
    e: &mut dyn Entropy,
) -> Result<Vec<u8>, SealError> {
    let total = 1usize
        .checked_add(NONCE_LEN)
        .and_then(|n| n.checked_add(TAG_LEN))
        .and_then(|n| n.checked_add(plain.len()))
        .ok_or(SealError::TooLong)?;
    if ByteCount::of_len(total) > SEALED_MAX {
        return Err(SealError::TooLong);
    }
    let mut nonce = [0u8; NONCE_LEN];
    e.fill(&mut nonce).map_err(SealError::Entropy)?;
    let ciphertext = cipher(k)?
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: &associated_data(entry_hash, field),
            },
        )
        .map_err(|_| SealError::Open)?;
    let mut sealed = Vec::with_capacity(total);
    sealed.push(VERSION);
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Opens a sealed field: the plaintext, in a buffer that is wiped when it is dropped.
///
/// Implements planning/architecture.md section 5.9 (`sealed`) and section 5.17 (without the seal key the
/// fields are unreadable).
///
/// Caller: the devices of members (AzWorkspace shows the names), the tests.
pub fn open_field(
    k: &SealKey,
    entry_hash: &Hash32,
    field: &str,
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SealError> {
    let (version, rest) = sealed.split_first().ok_or(SealError::Malformed)?;
    if *version != VERSION || rest.len() < NONCE_LEN + TAG_LEN {
        return Err(SealError::Malformed);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let plain = cipher(k)?
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &associated_data(entry_hash, field),
            },
        )
        .map_err(|_| SealError::Open)?;
    Ok(Zeroizing::new(plain))
}
