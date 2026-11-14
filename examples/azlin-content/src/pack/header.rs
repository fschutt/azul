//! The 24-byte header of an AZP2 pack (planning/architecture.md section 5.7 "Pack (AZP2)": "a short
//! header"; section 2.5 "Every pack must have the AZP2 header (magic, version, class, sizes)"; build
//! unit S14, lane W1-S14c, brief P-1).
//!
//! ```text
//!  0  4  magic "AZP2"
//!  4  1  version = 1
//!  5  1  class: 0 content, 1 derived, 2 repository
//!  6  2  reserved = 0 (non-zero is refused)
//!  8  4  key_epoch: the epoch of the K_seal that sealed the trailer (at least 1)
//! 12  4  chunk_count: the number of chunk blobs, 1 to MAX_CHUNKS
//! 16  4  body_len: the bytes of the chunk blobs, back to back
//! 20  4  pad_len: the bytes of padding after the body
//! ```
//!
//! All numbers are little-endian. The trailer is whatever remains: `trailer_len = total_len - 24 -
//! body_len - pad_len` is a consistency check, not a stored field. The header is parsed by a node
//! that holds no key (the AZP2 header check of section 2.5, [`crate::pack::check_header`]) and by the
//! client; it is strict: every field that is not what a writer of this version writes is refused with
//! its own typed error, and a version this build does not read is [`PackError::UnsupportedVersion`]
//! before any other field is interpreted (a newer header is never misparsed).

use core::ops::Range;

use azlin_keys::Epoch;

use super::trailer::trailer_bounds;
use super::{PackError, HARD_MAX, MAX_CHUNKS};
use crate::ids::PackClass;
use crate::limits::{BLOB_OVERHEAD, MAX_BLOB};
use crate::units::{PackLen, PackOffset};

/// The magic: the first four bytes of every pack.
///
/// Caller: `PackHeader::parse` and `to_bytes`, the tests of the format.
pub const MAGIC: [u8; 4] = *b"AZP2";

/// The version this build writes and reads: 1. Another version is refused by name, never parsed.
///
/// Caller: `PackHeader::parse` and `to_bytes`, the tests of the format.
pub const VERSION: u8 = 1;

/// The bytes of the header as a `u64`, for the size arithmetic of the checks.
const HEADER_BYTES: u64 = 24;

/// The least a blob can hold: its framing (the codec byte, the nonce and the tag), 41 bytes.
const MIN_BLOB: u64 = BLOB_OVERHEAD as u64;

/// The parsed header of a pack. The fields are public so that a reader can look at them, and a
/// header made by hand is checked by the functions that take it ([`PackHeader::trailer_range`],
/// `read_trailer`): only [`PackHeader::parse`] and the pack builder make headers that are known to be
/// consistent with a pack.
///
/// Caller: INT-S14-1 and INT-S14-3 (the client reads the header of a pack by a ranged read before it
/// reads the trailer), INT-S14-4 (S6's `pack_put` through `check_header`), `read_trailer`,
/// `chunk_range`, S14e1 (the pack's class and sizes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackHeader {
    /// What the chunks are: content, derived or repository.
    pub class: PackClass,
    /// The epoch of the `K_seal` that sealed the trailer: a reader after a rotation picks the key
    /// of this epoch.
    pub key_epoch: Epoch,
    /// The number of chunk blobs in the body.
    pub chunk_count: u32,
    /// The bytes of the chunk blobs.
    pub body_len: PackLen,
    /// The bytes of padding between the body and the trailer.
    pub pad_len: PackLen,
}

impl PackHeader {
    /// The length of the header in bytes: 24.
    ///
    /// Caller: INT-S14-1 and INT-S14-3 (the size of the first ranged read of a pack), the readers
    /// and the builder of this module.
    pub const LEN: usize = 24;

    /// Reads the header from the first bytes of a pack of `total_len` bytes (the length the store
    /// declares for the object). Nothing is allocated, and nothing but the first 24 bytes and the
    /// declared length is read. In this order, each with its own error: fewer than 24 bytes
    /// ([`PackError::TooShort`]), the magic, the version, the class byte, the reserved bytes, the
    /// epoch (0 does not exist), then the sizes ([`PackHeader::trailer_range`]'s checks).
    ///
    /// Caller: INT-S14-1 and INT-S14-3 (the client), `check_header` (INT-S14-4, the node),
    /// `rebuild_entries`.
    pub fn parse(first_bytes: &[u8], total_len: PackLen) -> Result<PackHeader, PackError> {
        let Some(head) = first_bytes.first_chunk::<24>() else {
            return Err(PackError::TooShort);
        };
        let [m0, m1, m2, m3, version, class, r0, r1, e0, e1, e2, e3, c0, c1, c2, c3, b0, b1, b2, b3, p0, p1, p2, p3] =
            *head;
        if [m0, m1, m2, m3] != MAGIC {
            return Err(PackError::BadMagic);
        }
        if version != VERSION {
            return Err(PackError::UnsupportedVersion(version));
        }
        let class = PackClass::from_byte(class).ok_or(PackError::BadClass(class))?;
        if r0 != 0 || r1 != 0 {
            return Err(PackError::ReservedNotZero);
        }
        let key_epoch =
            Epoch::new(u32::from_le_bytes([e0, e1, e2, e3])).ok_or(PackError::BadEpoch)?;
        let header = PackHeader {
            class,
            key_epoch,
            chunk_count: u32::from_le_bytes([c0, c1, c2, c3]),
            body_len: PackLen::new(u32::from_le_bytes([b0, b1, b2, b3])),
            pad_len: PackLen::new(u32::from_le_bytes([p0, p1, p2, p3])),
        };
        header.check_sizes(total_len)?;
        Ok(header)
    }

    /// The 24 bytes of the header, as [`PackHeader::parse`] reads them. They are also what the
    /// associated data of the trailer holds (`"AZP2" || space_id || these bytes`), so the trailer is
    /// bound to the class, the epoch, the count and the sizes.
    ///
    /// Caller: the builder, the trailer's associated data, the tests and fuzz entries of the format.
    pub fn to_bytes(self) -> [u8; 24] {
        let [m0, m1, m2, m3] = MAGIC;
        let [e0, e1, e2, e3] = self.key_epoch.get().to_le_bytes();
        let [c0, c1, c2, c3] = self.chunk_count.to_le_bytes();
        let [b0, b1, b2, b3] = self.body_len.get().to_le_bytes();
        let [p0, p1, p2, p3] = self.pad_len.get().to_le_bytes();
        [
            m0,
            m1,
            m2,
            m3,
            VERSION,
            self.class.byte(),
            0,
            0,
            e0,
            e1,
            e2,
            e3,
            c0,
            c1,
            c2,
            c3,
            b0,
            b1,
            b2,
            b3,
            p0,
            p1,
            p2,
            p3,
        ]
    }

    /// Where the trailer sits in a pack of `total_len` bytes: from the end of the padding to the
    /// end, so that a client can read it by a ranged read after it has read the header. The sizes
    /// are checked against each other first: a pack above [`HARD_MAX`] is [`PackError::TooLong`];
    /// no chunk, or more than [`MAX_CHUNKS`], is [`PackError::NoChunks`] or
    /// [`PackError::TooManyChunks`]; a body that cannot hold `chunk_count` blobs (each at least its
    /// framing, at most [`MAX_BLOB`]), and a trailer that is shorter or longer than `chunk_count`
    /// lines can make (including a header that claims more bytes than the pack has), are
    /// [`PackError::SizesInconsistent`]. DRIFT X-DELTA-W1S14C-8: the brief's `Range<u32>` is a
    /// range of [`PackOffset`].
    ///
    /// Caller: INT-S14-1 and INT-S14-3 (the ranged read of the trailer), `rebuild_entries`.
    pub fn trailer_range(&self, total_len: PackLen) -> Result<Range<PackOffset>, PackError> {
        let range = self.check_sizes(total_len)?;
        Ok(PackOffset::new(range.start)..PackOffset::new(range.end))
    }

    /// Where the chunk blobs sit: from the end of the header to the end of the body.
    ///
    /// Caller: `chunk_range` (INT-S14-3: a chunk's bytes by a ranged read), `rebuild_entries`.
    pub fn body_range(&self) -> Result<Range<PackOffset>, PackError> {
        let start = HEADER_BYTES;
        let end = start
            .checked_add(u64::from(self.body_len.get()))
            .ok_or(PackError::SizesInconsistent)?;
        let end = u32::try_from(end).map_err(|_| PackError::SizesInconsistent)?;
        Ok(PackOffset::new(PackHeader::LEN as u32)..PackOffset::new(end))
    }

    /// The consistency of the header's sizes with each other and with the declared length of the
    /// pack; the trailer's byte range `start..total` when they agree.
    pub(super) fn check_sizes(&self, total_len: PackLen) -> Result<Range<u32>, PackError> {
        if self.chunk_count == 0 {
            return Err(PackError::NoChunks);
        }
        if self.chunk_count > MAX_CHUNKS {
            return Err(PackError::TooManyChunks);
        }
        if total_len > HARD_MAX {
            return Err(PackError::TooLong);
        }
        // `chunk_count` is at most MAX_CHUNKS here, so none of these products can overflow.
        let count = u64::from(self.chunk_count);
        let body = u64::from(self.body_len.get());
        let min_body = count.saturating_mul(MIN_BLOB);
        let max_body = count.saturating_mul(u64::from(MAX_BLOB.get()));
        if !(min_body..=max_body).contains(&body) {
            return Err(PackError::SizesInconsistent);
        }
        let start = HEADER_BYTES
            .saturating_add(body)
            .saturating_add(u64::from(self.pad_len.get()));
        let total = u64::from(total_len.get());
        let trailer = total
            .checked_sub(start)
            .ok_or(PackError::SizesInconsistent)?;
        let (shortest, longest) = trailer_bounds(self.chunk_count);
        if !(shortest..=longest).contains(&trailer) {
            return Err(PackError::SizesInconsistent);
        }
        // `start` is below `total`, which is at most HARD_MAX: it fits a u32.
        let start = u32::try_from(start).map_err(|_| PackError::SizesInconsistent)?;
        Ok(start..total_len.get())
    }
}
