//! `Hash32`: the 32-byte hash value of the engine's chains, heads, checkpoints and commitments,
//! and `hash32`, the one way this crate hashes under a domain string (planning/architecture.md
//! section 5.9 "The team log and the space chains": every entry is hash-chained through `prev`;
//! section 2.5 "Signed structures, each with its own domain string"; build unit S17, lane
//! W1-S17a).
//!
//! - `Hash32` is 32 opaque bytes: `Copy`, ordered and hashable as bytes (it can be a map key), with
//!   NO `Default` (an all-zero hash is [`Hash32::ZERO`], named: the `prev` of a genesis entry).
//! - Its text form is 64 lowercase hex digits and [`FromStr`] accepts nothing else (no uppercase,
//!   no whitespace, no `0x`, no other length), so a hash has exactly one spelling.
//! - `hash32(domain, parts)` is BLAKE3 in its `derive_key` mode with `domain` as the context, over
//!   the parts with a LENGTH FRAME in front of each (`len as u64, little-endian`, then the bytes), so
//!   `["ab", "c"]` and `["a", "bc"]` are two different inputs, and so are `[]` and `[""]`. Two
//!   domains give two unrelated functions. The byte layout is pinned by
//!   `hash32_is_domain_separated_and_displays_lowercase_hex`, which re-derives it with
//!   `blake3::derive_key`.
//!
//! The byte-exact canonical encoding of a team-log or space-chain entry and the preimage of its
//! signature are azlin-cbor's and azlin-contract's (a later glue unit, CHAIN-GLUE, makes the entry
//! hashes with them): `hash32` is the generic helper for the hashes this engine makes of its own
//! data (state hashes, commitments, capability hashes).

use core::fmt;
use core::str::FromStr;

use azlin_keys::IdError;

/// A 32-byte hash value.
///
/// Implements planning/architecture.md section 5.9 (`prev` and the head hash of a chain, the
/// checkpoint's state hash).
///
/// Caller: azlin-chain (`ChainEntry::prev`/`hash`, `Head`), S17c (`TeamState::state_hash`), S16a
/// (`state_hash`, value hashes), S16b (`key_id`, capability hashes), S16d, S17d, S17f, S17g.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash32([u8; 32]);

impl Hash32 {
    /// The number of bytes of a hash.
    pub const LEN: usize = 32;

    /// The hash of 32 zero bytes: the `prev` of a genesis entry (it is a name for "no previous
    /// entry", not the output of any hashing).
    pub const ZERO: Hash32 = Hash32([0u8; 32]);

    /// The hash with these bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Hash32 {
        Hash32(bytes)
    }

    /// The bytes of the hash.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Whether this is [`Hash32::ZERO`].
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

/// 64 lowercase hex digits.
impl fmt::Display for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash32({self})")
    }
}

/// The value of a lowercase hex digit; `None` for anything else (uppercase included).
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Strict: exactly 64 lowercase hex digits. The error is `IdError::Length` for any other length
/// (the empty text included) and `IdError::BadDigit` for a character that is not a lowercase hex
/// digit.
impl FromStr for Hash32 {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Hash32, IdError> {
        let digits = text.as_bytes();
        if digits.len() != Hash32::LEN * 2 {
            return Err(IdError::Length);
        }
        let mut out = [0u8; 32];
        for (byte, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
            let (high, low) = match pair {
                [high, low] => (nibble(*high), nibble(*low)),
                _ => return Err(IdError::Length),
            };
            match (high, low) {
                (Some(high), Some(low)) => *byte = (high << 4) | low,
                _ => return Err(IdError::BadDigit),
            }
        }
        Ok(Hash32(out))
    }
}

/// The hash of `parts` under `domain`: BLAKE3 `derive_key` with `domain` as the context string, then
/// each part preceded by its length (`u64`, little-endian). Deterministic; the same parts under
/// another domain, or the same bytes cut into other parts, give another hash.
///
/// A domain is a `const` of the crate that owns the hash (lowercase ASCII such as
/// `azlin-state-v1`); this function does not validate it.
///
/// Implements planning/architecture.md section 5.9 (hash chains) and section 2.5 (a domain string
/// per structure).
///
/// Caller: azlin-chain's tests and checkpoints; S17c (`state_hash`, entry hashes through the glue),
/// S16a/S16b (state and key commitments), S9a (registry digest).
pub fn hash32(domain: &str, parts: &[&[u8]]) -> Hash32 {
    let mut hasher = if mutant!("seams_hash32_ignores_domain") {
        blake3::Hasher::new()
    } else {
        blake3::Hasher::new_derive_key(domain)
    };
    for part in parts {
        if !mutant!("seams_hash32_no_length_frame") {
            // `usize` fits `u64` on every supported target (the assertion in lib.rs).
            hasher.update(&(part.len() as u64).to_le_bytes());
        }
        hasher.update(part);
    }
    Hash32(*hasher.finalize().as_bytes())
}
