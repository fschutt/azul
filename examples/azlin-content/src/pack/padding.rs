//! Padding: the bytes between the body and the trailer that hide the exact size of the last pack of an
//! upload (planning/architecture.md section 5.15 M4: "4 MiB packs, the last one padded to 256 KiB
//! steps; repository objects to 4 KiB; policy `padding` = none, buckets (the default) or full";
//! section 5.7 "The last pack of an upload is padded to a size bucket"; build unit S14, lane W1-S14c).
//!
//! The rules ([`padded_len`]), all of them DRIFT X-DELTA-W1S14C-9 (the brief's P-5 and C7 do not say
//! every case):
//!
//! | policy | a pack finished with `last_of_batch = false` | the last pack of a batch |
//! |---|---|---|
//! | `None` | unchanged | unchanged |
//! | `Buckets` (the default) | unchanged | up to the next multiple of `pad_step` (256 KiB), of `REPO_PAD` (4 KiB) for a repository pack |
//! | `Full` | unchanged | up to `target` (4 MiB) |
//!
//! A pack that is already above `target` (a lone oversize chunk, brief P-2) is padded within
//! `hard_max`: under `Buckets` to the next step but never above `hard_max`, under `Full` to
//! `hard_max`, so that the oversize packs form one bucket of their own. A pack is never padded past
//! the cap that applies to it, so a geometry whose `target` is not a multiple of `pad_step` ends at
//! `target`, not above it.
//!
//! THE PADDING BYTES (DRIFT X-DELTA-W1S14C-13): the brief says "the XChaCha20 keystream of
//! derive(PACK_PADDING, K_seal) under the trailer's nonce". Concretely: the key is
//! `PACK_PADDING.derive(K_seal)`, the nonce is the 24 bytes of the trailer's nonce, and the bytes are
//! the ciphertext of `pad_len` zero bytes under XChaCha20-Poly1305 with empty associated data, the tag
//! dropped; that is the XChaCha20 keystream from block counter 1 on (the AEAD takes block 0 for its
//! Poly1305 key). The bytes look random (the node's randomness test passes them), are a function of
//! the key and the trailer's nonce (so two packs of one space never repeat padding), and are checked by
//! `rebuild_entries`, which has the key.

use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use super::{PackError, PackGeometry, Padding, SealKey, REPO_PAD};
use crate::ids::PackClass;
use crate::limits::PACK_PADDING;
use crate::units::PackLen;

/// `len` rounded up to a multiple of `step`; [`PackError::BadGeometry`] for a step of zero or a
/// result that does not fit a `u32`.
fn round_up(len: PackLen, step: PackLen) -> Result<PackLen, PackError> {
    if step.get() == 0 {
        return Err(PackError::BadGeometry);
    }
    len.get()
        .div_ceil(step.get())
        .checked_mul(step.get())
        .map(PackLen::new)
        .ok_or(PackError::BadGeometry)
}

/// The length a pack of `unpadded` bytes (header, body and trailer) has after padding, under the
/// rules in the table of the module documentation. A pack above `hard_max` is
/// [`PackError::TooBigForAnyPack`]; the result is never below `unpadded` and never above the cap
/// that applies (`target`, or `hard_max` for a pack already above `target`).
pub(super) fn padded_len(
    unpadded: PackLen,
    class: PackClass,
    policy: Padding,
    last_of_batch: bool,
    g: &PackGeometry,
) -> Result<PackLen, PackError> {
    if unpadded > g.hard_max {
        return Err(PackError::TooBigForAnyPack);
    }
    if !last_of_batch {
        return Ok(unpadded);
    }
    let cap = if unpadded <= g.target {
        g.target
    } else {
        g.hard_max
    };
    let padded = match policy {
        Padding::None => unpadded,
        Padding::Buckets => {
            let step = if class == PackClass::Repository {
                REPO_PAD
            } else {
                g.pad_step
            };
            round_up(unpadded, step)?.min(cap)
        }
        Padding::Full => cap,
    };
    Ok(padded)
}

/// Overwrites `out` with the padding bytes of the key and the nonce: the XChaCha20 keystream described
/// in the module documentation. An empty `out` is left alone.
pub(super) fn keystream(key: &SealKey, nonce: &[u8; 24], out: &mut [u8]) -> Result<(), PackError> {
    out.fill(0);
    if out.is_empty() {
        return Ok(());
    }
    let padding_key = PACK_PADDING.derive(&key.key);
    // The key is 32 bytes and the nonce 24 by their array types: `from_slice` cannot see a wrong length.
    let cipher = XChaCha20Poly1305::new(Key::from_slice(padding_key.expose()));
    // The tag is the Poly1305 of the keystream: of no use, dropped. The cipher refuses only a buffer
    // past its own limit (2^38 bytes): a length no pack has.
    let _tag = cipher
        .encrypt_in_place_detached(XNonce::from_slice(nonce), b"", out)
        .map_err(|_| PackError::TooLong)?;
    Ok(())
}
