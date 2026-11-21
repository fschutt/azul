//! The AEAD layer of every envelope: XChaCha20-Poly1305 under a 32-byte key, a 24-byte nonce
//! drawn from the entropy seam, and the associated data the caller passes (planning/architecture.md
//! section 5.5, "`nonce`, `ct` | XChaCha20-Poly1305"; build unit S15, lane W1-S15b).
//!
//! Algorithms 1 and 2 reach this layer through their own key derivations; lane W1-S15d reuses it
//! for algorithms 3, 4 and 5 (through `seal_with_key`, which also builds the associated data
//! and the signature, or directly). The nonce is ALWAYS drawn here, from the seam passed in, so a
//! key is never used with a nonce the caller chose; a wrap that fails to draw one fails and
//! returns nothing (UNKNOWN IS A STATE: no function goes on with zeroes).

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use super::error::{OpenError, WrapError};
use super::wire::MAX_PLAINTEXT;
use crate::seam::Entropy;
use crate::secret::Secret;

/// The length of the XChaCha20-Poly1305 nonce, in bytes.
///
/// Caller: `seal_under`, `open_under`, the wire codec.
pub const NONCE_LEN: usize = 24;

/// The length of the Poly1305 tag that ends every ciphertext, in bytes.
///
/// Caller: the wire codec (the shortest `ct` is one tag), `open_under`.
pub const TAG_LEN: usize = 16;

/// Seals `plaintext` under `key` with `header_ad` as the associated data and a fresh 24-byte
/// nonce drawn from `ent`; returns the nonce and the ciphertext (the tag is its last 16 bytes).
/// On an entropy failure nothing is returned. The plaintext may be empty (the ciphertext is then
/// one tag) and at most `MAX_PLAINTEXT` bytes.
///
/// The draw is exactly 24 bytes, once, and it comes AFTER every other draw of a construction
/// (the ephemeral key, the ML-KEM message): the order is part of the known-answer vectors.
///
/// Caller: `seal_with_key` (this crate: every algorithm), W1-S15d for algorithms 3, 4 and 5
/// when it builds the associated data itself.
pub fn seal_under(
    key: &Secret<32>,
    header_ad: &[u8],
    plaintext: &[u8],
    ent: &mut dyn Entropy,
) -> Result<([u8; NONCE_LEN], Vec<u8>), WrapError> {
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(WrapError::PlaintextTooLarge);
    }
    let mut nonce = [0u8; NONCE_LEN];
    ent.fill(&mut nonce).map_err(WrapError::Entropy)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: header_ad,
            },
        )
        .map_err(|_| WrapError::Cipher)?;
    Ok((nonce, ct))
}

/// Opens `ct` (ciphertext and tag) under `key` with `header_ad` and `nonce`; the plaintext comes
/// back in a `Zeroizing` buffer. Any failure (a wrong key, a changed header, nonce or
/// ciphertext, a ciphertext shorter than a tag) is [`OpenError::Auth`], one word: the error says
/// nothing about which part was wrong.
///
/// Caller: `open`, `open_with_key` (this crate), W1-S15d for algorithms 3, 4 and 5 when it
/// builds the associated data itself.
pub fn open_under(
    key: &Secret<32>,
    header_ad: &[u8],
    nonce: &[u8; NONCE_LEN],
    ct: &[u8],
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    if ct.len() > MAX_PLAINTEXT + TAG_LEN {
        return Err(OpenError::Auth);
    }
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: header_ad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| OpenError::Auth)
}
