//! UUID string generation - the intended mint for MARKER strings.
//!
//! Markers (`Dom::with_marker` + `CallbackInfo::get_node_id_by_marker`) are
//! app-chosen strings created at `layout()` time; what makes them work is that
//! they collide with *nothing* - not another widget, not another window, not a
//! hard-coded string in some library. A UUID gives that guarantee without any
//! coordination, so the pattern is:
//!
//! ```rust,ignore
//! // once, at layout() time (store it wherever the driving callback looks):
//! let marker = Uuid::short();
//! let bar = ProgressBar::create(0.0).dom().with_marker(Some(marker.clone()).into());
//! ```
//!
//! [`Uuid::v4`] returns the canonical hyphenated form (36 chars);
//! [`Uuid::short`] returns the same 128 bits as a 22-char flickrBase58 string
//! (the `short-uuid` encoding) - shorter, no hyphens, URL/log friendly.
//!
//! # These ids are DETERMINISTIC, not random
//!
//! Every id is a pure function of a process-local counter, so run the same
//! program twice and you get the same sequence of ids. That is all a marker
//! needs - it has to be unique among the strings alive in ONE process, and it
//! is: the mixing function is a bijection, so 2^63 mints collide zero times,
//! exactly rather than probabilistically.
//!
//! What it is NOT: unpredictable, or unique across processes and machines. Do
//! not use these to identify a client to a server, as a security token, or as
//! a database key that two processes might mint independently. The crate
//! carries no randomness source on purpose - `uuid`'s `v4` needs one, and on
//! `wasm32-unknown-unknown` there isn't one without pulling in `getrandom`'s
//! JS shim, which broke every wasm consumer of azul-layout 0.0.15.

use core::sync::atomic::{AtomicU64, Ordering};

use azul_css::AzString;

/// flickrBase58 — base58 without the look-alikes `0`, `O`, `I` and `l`.
const BASE58: &[u8; 58] = b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";

/// Mint tick. Every id is a pure function of its tick, so the sequence is the
/// same on every run — deterministic, not random.
static TICK: AtomicU64 = AtomicU64::new(0);

/// The splitmix64 finalizer. Every step (xor-shift-right, multiply by an odd
/// constant) is invertible, so the whole function is a BIJECTION on `u64`:
/// distinct ticks give distinct outputs, which is what makes the no-collision
/// guarantee below exact rather than probabilistic.
const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 128 fresh bits: one tick spread over two disjoint `mix64` inputs.
///
/// `hi` alone is `mix64` of an injective function of the tick, so two mints
/// can only repeat once the tick wraps at 2^63.
fn next_bits() -> [u8; 16] {
    let tick = TICK.fetch_add(1, Ordering::Relaxed);
    let hi = mix64(tick.wrapping_mul(2));
    let lo = mix64(tick.wrapping_mul(2).wrapping_add(1));
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&hi.to_be_bytes());
    out[8..].copy_from_slice(&lo.to_be_bytes());
    // RFC 4122: version 4 in the high nibble of byte 6, variant 0b10 in the
    // top two bits of byte 8. Stamped after the mix so the shape is exact.
    out[6] = (out[6] & 0x0F) | 0x40;
    out[8] = (out[8] & 0x3F) | 0x80;
    out
}

/// Static-method namespace for UUID string generation ([`Uuid::v4`],
/// [`Uuid::short`]). The struct only exists so the FFI layer can hang static
/// methods off it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // FFI/api.json static-namespace placeholder field
pub struct Uuid {
    pub _reserved: u8,
}

impl Default for Uuid {
    fn default() -> Self {
        Self::new()
    }
}

impl Uuid {
    /// Returns a zero-initialised handle. The struct only exists so the FFI
    /// layer can hang static methods off it.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// A fresh version-4-shaped UUID in canonical hyphenated lowercase form,
    /// e.g. `"550e8400-e29b-41d4-a716-446655440000"` (36 characters).
    ///
    /// Deterministic, not random - the value depends only on how many ids this
    /// process has already minted. See the module docs before using one as
    /// anything but a marker.
    #[must_use]
    pub fn v4() -> AzString {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let b = next_bits();
        let mut out = String::with_capacity(36);
        for (i, byte) in b.iter().enumerate() {
            // 4-2-2-2-6 bytes, so the hyphens fall after bytes 4, 6, 8 and 10.
            if matches!(i, 4 | 6 | 8 | 10) {
                out.push('-');
            }
            out.push(HEX[usize::from(byte >> 4)] as char);
            out.push(HEX[usize::from(byte & 0x0F)] as char);
        }
        out.into()
    }

    /// A fresh version-4-shaped UUID as a 22-character flickrBase58 string,
    /// e.g. `"mhvXdrZT4jP5T8vBxuvm75"` - the same encoding `short-uuid` used to
    /// print. The compact spelling for markers, log lines and URLs.
    ///
    /// Deterministic, not random - see [`Uuid::v4`] and the module docs.
    #[must_use]
    pub fn short() -> AzString {
        // Big-endian base58 of the same 128 bits, left-padded to the fixed 22
        // characters `short-uuid` emits (a value with leading zero bytes still
        // has to spell 22 characters or the width stops being a contract).
        let mut n = u128::from_be_bytes(next_bits());
        let mut buf = [BASE58[0]; 22];
        let mut i = 22;
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = BASE58[(n % 58) as usize];
            n /= 58;
        }
        String::from_utf8_lossy(&buf).into_owned().into()
    }
}

#[cfg(test)]
mod uuid_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v4_is_canonical_hyphenated_lowercase() {
        for _ in 0..64 {
            let s = Uuid::v4();
            let s = s.as_str();
            assert_eq!(s.len(), 36, "canonical UUID is 36 chars: {s}");
            for (i, c) in s.char_indices() {
                if matches!(i, 8 | 13 | 18 | 23) {
                    assert_eq!(c, '-', "hyphen expected at {i}: {s}");
                } else {
                    assert!(
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
                        "lowercase hex expected at {i}: {s}",
                    );
                }
            }
            assert_eq!(&s[14..15], "4", "version nibble must say v4: {s}");
        }
    }

    #[test]
    fn short_is_22_chars_of_flickr_base58() {
        // flickrBase58: no 0 / O / I / l - the look-alike characters.
        for _ in 0..64 {
            let s = Uuid::short();
            let s = s.as_str();
            assert_eq!(s.len(), 22, "short UUID is 22 chars: {s}");
            for c in s.chars() {
                assert!(c.is_ascii_alphanumeric(), "base58 is alphanumeric: {s}");
                assert!(
                    !matches!(c, '0' | 'O' | 'I' | 'l'),
                    "flickrBase58 excludes look-alikes, got {c:?} in {s}",
                );
            }
        }
    }

    #[test]
    fn minted_ids_do_not_collide() {
        // The whole point of the module: 10k mints, 10k distinct strings.
        let mut seen = HashSet::new();
        for _ in 0..5_000 {
            assert!(seen.insert(Uuid::v4().as_str().to_string()));
            assert!(seen.insert(Uuid::short().as_str().to_string()));
        }
    }

    #[test]
    fn the_namespace_handle_is_inert() {
        assert_eq!(Uuid::new(), Uuid::default());
        assert_eq!(Uuid::new()._reserved, 0);
    }
}
