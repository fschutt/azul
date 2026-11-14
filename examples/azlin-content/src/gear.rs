//! The gear table of the chunker: 256 pseudo-random `u64` values, one per byte value, that the
//! rolling fingerprint adds (planning/architecture.md section 5.7 pipeline step 2 "FastCDC"; brief
//! P-2 and P-3; DRIFT X-DELTA-W1S14B-2; build unit S14, lane W1-S14b).
//!
//! The table decides every cut point of every file for ever (a change re-uploads everything), so it
//! is derived, not written down, and pinned by `tests/vectors/chunker_golden.json`:
//!
//! - [`GearTable::public_v1`]: the BLAKE3 `derive_key`-mode XOF of the context `GEAR_PUBLIC` over no
//!   input, 2,048 bytes read as 256 little-endian `u64`. The same for every space.
//! - [`GearTable::keyed`]: the same XOF of the context `GEAR_KEYED` over a space's `K_chunk`. Two
//!   spaces cut the same file at different places, so a node that sees the lengths of ranged reads
//!   cannot fingerprint a known file (restic and borg key their chunker for the same reason). Equal
//!   chunks inside one space still cut alike, so deduplication inside a space is unchanged. The
//!   integration unit's default is the keyed table (decision X-D-43 of ARCH29 9.13).
//!
//! A keyed table is derived from key material, so it is wiped when dropped, and a `GearTable` has no
//! `Clone`: the chunker shares one behind an `Arc`.
//!
//! ```
//! use azlin_content::gear::GearTable;
//!
//! // The same table every time, and not an all-zero one.
//! let a = GearTable::public_v1();
//! let b = GearTable::public_v1();
//! assert!(a == b);
//! assert_eq!(format!("{a:?}"), "GearTable(***)");
//! ```

use core::fmt;

use blake3::Hasher;
use zeroize::{Zeroize, Zeroizing};

use crate::chunk::KChunk;
use crate::limits::{GEAR_KEYED, GEAR_PUBLIC};

/// The number of entries of a gear table: one per byte value.
///
/// Caller: the tests of the table, INT-WIRE (the contract fixture of the chunker).
pub const GEAR_ENTRIES: usize = 256;

/// The bytes of the XOF that make a table: eight per entry.
const GEAR_BYTES: usize = GEAR_ENTRIES * 8;

/// The 256 values of the gear hash `fp = (fp << 1) + GEAR[byte]`.
///
/// Caller: INT-S14-1 and INT-S14-2 (they pick the table when they build a `Chunker`).
pub struct GearTable([u64; GEAR_ENTRIES]);

impl GearTable {
    /// The public table, the same for every space: the XOF of the context `GEAR_PUBLIC`.
    pub fn public_v1() -> GearTable {
        GearTable::from_hasher(&Hasher::new_derive_key(GEAR_PUBLIC.as_str()))
    }

    /// The table of one space: the XOF of the context `GEAR_KEYED` over its `K_chunk`.
    pub fn keyed(k: &KChunk) -> GearTable {
        let mut hasher = Hasher::new_derive_key(GEAR_KEYED.as_str());
        hasher.update(k.expose());
        GearTable::from_hasher(&hasher)
    }

    /// Reads the 2,048 bytes of the hasher's XOF as 256 little-endian `u64`.
    fn from_hasher(hasher: &Hasher) -> GearTable {
        let mut bytes = Zeroizing::new([0u8; GEAR_BYTES]);
        hasher.finalize_xof().fill(bytes.as_mut_slice());
        let mut table = [0u64; GEAR_ENTRIES];
        let (words, _) = bytes.as_slice().as_chunks::<8>();
        for (entry, word) in table.iter_mut().zip(words) {
            *entry = u64::from_le_bytes(*word);
        }
        GearTable(table)
    }

    /// The gear value of a byte. The table has an entry for each of the 256 values of a byte, so
    /// the lookup is total (`gear_is_total_over_every_byte_value` holds the table to it); the last
    /// arm is the one a `u8` cannot reach.
    #[inline(always)]
    pub(crate) fn gear(&self, byte: u8) -> u64 {
        match self.0.get(usize::from(byte)) {
            Some(value) => *value,
            None => 0,
        }
    }
}

/// Equality of two tables (the tests ask whether two derivations agree).
impl PartialEq for GearTable {
    fn eq(&self, other: &GearTable) -> bool {
        self.0 == other.0
    }
}

impl Eq for GearTable {}

/// Prints `GearTable(***)`: a keyed table is derived from key material.
impl fmt::Debug for GearTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GearTable(***)")
    }
}

impl Drop for GearTable {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_is_total_over_every_byte_value() {
        let table = GearTable::public_v1();
        for byte in 0..=255u8 {
            assert!(table.0.get(usize::from(byte)).is_some(), "byte {byte}");
            assert_eq!(table.gear(byte), table.0[usize::from(byte)]);
        }
        assert_eq!(table.0.len(), 256);
    }
}
