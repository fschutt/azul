//! The sealed trailer of an AZP2 pack: the list `(chunk_id, offset, length)` of the blobs in the body,
//! sealed with `K_seal` (planning/architecture.md section 5.7 "Pack (AZP2)": "a trailer sealed with
//! `K_seal` listing `(chunk_id, offset, length)` ... a lost chunk index can be rebuilt from the
//! trailers"; build unit S14, lane W1-S14c, brief P-1).
//!
//! ```text
//! trailer = nonce (24, drawn from the Entropy seam)
//!        || XChaCha20-Poly1305(K = K_PACK_TRAILER.derive(K_seal), plaintext) || tag (16)
//! plaintext = CBOR { 1: [[chunk_id (16 bytes), offset, length], ...], 2: BLAKE3(body) (32 bytes) }
//! associated data = "AZP2" || space_id (16) || the 24 header bytes
//! ```
//!
//! DRIFT X-DELTA-W1S14C-6: the brief's associated data has no space id, but C5 ("wrong space id") and
//! `read_trailer(.., space, ..)` need one: it is in. DRIFT X-DELTA-W1S14C-7: the brief puts
//! `BLAKE3(body)` into the associated data, which a reader that holds only the header and the trailer
//! (the ranged read the brief wants from `read_trailer`) cannot compute: the hash is key 2 of the
//! sealed plaintext instead, so the trailer authenticates on its own and `rebuild_entries` (the whole
//! pack in hand) checks the body against the sealed hash. Either way the trailer is bound to its pack,
//! its class, its epoch, its space and its content.
//!
//! The plaintext is deterministic strict CBOR (`azlin-cbor`, rule R6): an entry is an array of three,
//! the lines are in the order of the body, and the reader refuses what a builder never writes: a
//! count other than the header's, an offset that is not where the last blob ended (the first at 24, the
//! last blob ending at the end of the body), a blob shorter than its framing, a chunk twice, a field of
//! another type. A key the reader does not know is ignored unless `crit` lists it (R18 does not apply:
//! this module never copies or re-signs a trailer).

use std::collections::BTreeSet;

use azlin_cbor::{decode, encode, IntMap, IntMapBuilder, Limits, Value};
use azlin_keys::SpaceId;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Key, Tag, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use super::header::{PackHeader, MAGIC};
use super::{PackError, SealKey, MAX_CHUNKS};
use crate::ids::{ChunkId, PackEntry};
use crate::limits::{BLOB_OVERHEAD, K_PACK_TRAILER};
use crate::units::{ChunkLen, PackLen, PackOffset};

/// The bytes of the nonce at the start of a trailer.
pub(super) const NONCE_LEN: usize = 24;

/// The bytes of the Poly1305 tag at the end of a trailer.
pub(super) const TAG_LEN: usize = 16;

/// The bytes of the header, as the first offset of the body.
const HEADER_BYTES: u64 = 24;

/// The bytes of a line of the trailer that no number changes: the array head, the head of the byte
/// string and the 16 bytes of the chunk id.
const ENTRY_FIXED: usize = 1 + 1 + 16;

/// The bytes of the plaintext that no line changes: the map head, key 1, key 2, the head of the
/// 32-byte hash and the hash. The array head of the lines is added by the count.
const PLAIN_FIXED: usize = 1 + 1 + 1 + 2 + 32;

/// The bytes of a trailer that neither the lines nor their count change: the nonce, the tag and the
/// fixed part of the plaintext, as a `u64` for the size arithmetic of the header check.
const TRAILER_FIXED: u64 = (NONCE_LEN + TAG_LEN + PLAIN_FIXED) as u64;

/// The least a line of the trailer takes: an offset of at least 24 and a length of at least 41 both
/// take two bytes.
const ENTRY_SHORTEST: u64 = 22;

/// The most a line of the trailer takes: an offset and a length that both need four bytes after the
/// head byte.
const ENTRY_LONGEST: u64 = 28;

/// What the plaintext is decoded under: depth 3 (the map, the lines, one line), at most
/// [`MAX_CHUNKS`] items in a container, at most four items per line and five more in all, no string
/// above the 32 bytes of the hash, and no more input than the longest trailer holds
/// (`PLAIN_FIXED` + 3 + 28 bytes per line). Every limit is a refusal point, never a capacity
/// (planning/architecture.md section 1.4 rule 4).
const PLAIN_LIMITS: Limits = Limits {
    max_input: 512 * 1024,
    max_depth: 3,
    max_items: MAX_CHUNKS,
    max_total_items: 4 * MAX_CHUNKS + 16,
    max_text: 32,
};

/// The bytes a CBOR head takes for the unsigned integer `n`: the initial byte alone below 24, then
/// one, two, four or eight more.
fn uint_len(n: u64) -> usize {
    if n < 24 {
        1
    } else if n < 256 {
        2
    } else if n < 65_536 {
        3
    } else if n < 4_294_967_296 {
        5
    } else {
        9
    }
}

/// The bytes one line `[id, offset, length]` of the trailer takes.
pub(super) fn entry_len(offset: u32, length: u32) -> usize {
    ENTRY_FIXED
        .saturating_add(uint_len(u64::from(offset)))
        .saturating_add(uint_len(u64::from(length)))
}

/// The bytes of a whole trailer of `count` lines that take `entries_len` bytes together: the nonce,
/// the sealed plaintext and the tag. It is exact: the pack builder decides when a pack is full by it,
/// and the pack it makes has a trailer of exactly this length.
pub(super) fn trailer_len(count: usize, entries_len: usize) -> usize {
    NONCE_LEN
        .saturating_add(TAG_LEN)
        .saturating_add(PLAIN_FIXED)
        .saturating_add(uint_len(count as u64))
        .saturating_add(entries_len)
}

/// The shortest and the longest a trailer of `count` lines can be: what a header claims is checked
/// against, without any key, by the node and by the reader.
pub(super) fn trailer_bounds(count: u32) -> (u64, u64) {
    let lines = u64::from(count);
    let fixed = TRAILER_FIXED.saturating_add(uint_len(lines) as u64);
    (
        fixed.saturating_add(lines.saturating_mul(ENTRY_SHORTEST)),
        fixed.saturating_add(lines.saturating_mul(ENTRY_LONGEST)),
    )
}

/// The associated data of a trailer: `"AZP2" || space_id || the 24 header bytes`.
fn associated_data(space: &SpaceId, header: &PackHeader) -> Vec<u8> {
    [
        MAGIC.as_slice(),
        space.as_bytes().as_slice(),
        header.to_bytes().as_slice(),
    ]
    .concat()
}

/// The plaintext of a trailer: the lines and the hash of the body, as deterministic CBOR.
pub(super) fn encode_plain(
    entries: &[PackEntry],
    body_hash: &[u8; 32],
) -> Result<Vec<u8>, PackError> {
    let mut lines = Vec::with_capacity(entries.len());
    for entry in entries {
        lines.push(Value::Array(vec![
            Value::Bytes(entry.id.as_bytes().to_vec()),
            Value::Uint(u64::from(entry.offset.get())),
            Value::Uint(u64::from(entry.length.get())),
        ]));
    }
    let value = IntMapBuilder::new()
        .put(1, Value::Array(lines))
        .put(2, Value::Bytes(body_hash.to_vec()))
        .build()
        .map_err(|_| PackError::BadTrailer)?;
    encode(&value, &PLAIN_LIMITS).map_err(|_| PackError::TooManyChunks)
}

/// Seals `plain` into a trailer: `nonce || ciphertext || tag`, under the key derived from `K_seal`
/// and the associated data of `space` and `header`.
pub(super) fn seal(
    key: &SealKey,
    space: &SpaceId,
    header: &PackHeader,
    plain: &[u8],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, PackError> {
    let trailer_key = K_PACK_TRAILER.derive(&key.key);
    // The key is 32 bytes and the nonce 24 by their array types: `from_slice` cannot see a wrong length.
    let cipher = XChaCha20Poly1305::new(Key::from_slice(trailer_key.expose()));
    let mut out = Zeroizing::new(Vec::with_capacity(
        NONCE_LEN
            .saturating_add(plain.len())
            .saturating_add(TAG_LEN),
    ));
    out.extend_from_slice(nonce);
    out.extend_from_slice(plain);
    let sealed = out.get_mut(NONCE_LEN..).ok_or(PackError::BadTrailer)?;
    // The cipher refuses only a buffer past its own limit (2^38 bytes): a length no trailer has.
    let tag = cipher
        .encrypt_in_place_detached(
            XNonce::from_slice(nonce),
            &associated_data(space, header),
            sealed,
        )
        .map_err(|_| PackError::TooLong)?;
    out.extend_from_slice(&tag);
    Ok(core::mem::take(&mut *out))
}

/// What an authenticated trailer says: the lines, and the hash of the body the builder sealed.
pub(super) struct Opened {
    /// The `(chunk_id, offset, length)` lines, in the order of the body, checked against the header.
    pub(super) entries: Vec<PackEntry>,
    /// The BLAKE3 hash of the body, as sealed.
    pub(super) body_hash: [u8; 32],
}

/// Opens the trailer `trailer` of the pack that `header` describes. In this order: the sizes (the
/// header against the length of the trailer: [`PackError::SizesInconsistent`] and its siblings), the
/// key's epoch against the header's ([`PackError::Auth`]), the authenticated decryption under the
/// associated data of `space` and `header` ([`PackError::Auth`]: a wrong key, space or epoch, another
/// pack's trailer, a flipped bit), and only then the plaintext: strict CBOR, then the lines against the
/// header ([`PackError::BadTrailer`], [`PackError::DuplicateChunk`]). Nothing is allocated for a claim
/// before the claim has been checked against the bytes in hand and a constant.
pub(super) fn open(
    key: &SealKey,
    space: &SpaceId,
    header: &PackHeader,
    trailer: &[u8],
) -> Result<Opened, PackError> {
    // The length the pack must have for this header and this trailer, checked like a node would.
    let trailer_len = u64::try_from(trailer.len()).map_err(|_| PackError::TooLong)?;
    let total = HEADER_BYTES
        .checked_add(u64::from(header.body_len.get()))
        .and_then(|n| n.checked_add(u64::from(header.pad_len.get())))
        .and_then(|n| n.checked_add(trailer_len))
        .ok_or(PackError::TooLong)?;
    let total = PackLen::new(u32::try_from(total).map_err(|_| PackError::TooLong)?);
    header.check_sizes(total)?;

    if header.key_epoch != key.epoch {
        return Err(PackError::Auth);
    }
    let (nonce, rest) = trailer
        .split_first_chunk::<NONCE_LEN>()
        .ok_or(PackError::SizesInconsistent)?;
    let (ciphertext, tag) = rest
        .split_last_chunk::<TAG_LEN>()
        .ok_or(PackError::SizesInconsistent)?;
    let trailer_key = K_PACK_TRAILER.derive(&key.key);
    // The key, nonce and tag are 32, 24 and 16 bytes by their array types (see `seal`).
    let cipher = XChaCha20Poly1305::new(Key::from_slice(trailer_key.expose()));
    let mut plain = Zeroizing::new(ciphertext.to_vec());
    cipher
        .decrypt_in_place_detached(
            XNonce::from_slice(nonce),
            &associated_data(space, header),
            plain.as_mut_slice(),
            Tag::from_slice(tag),
        )
        .map_err(|_| PackError::Auth)?;
    parse_plain(header, &plain)
}

/// Reads the authenticated plaintext of a trailer: the shape, then every line against the header.
fn parse_plain(header: &PackHeader, plain: &[u8]) -> Result<Opened, PackError> {
    let value = decode(plain, &PLAIN_LIMITS).map_err(|_| PackError::BadTrailer)?;
    let mut map = IntMap::new(&value).map_err(|_| PackError::BadTrailer)?;
    let lines = map.req_array(1).map_err(|_| PackError::BadTrailer)?;
    let body_hash = map
        .req_bytes_n::<32>(2)
        .map_err(|_| PackError::BadTrailer)?;
    // Unknown keys are ignored, a key `crit` lists that this reader does not know is refused.
    map.finish().map_err(|_| PackError::BadTrailer)?;

    if u64::try_from(lines.len()).map_err(|_| PackError::BadTrailer)?
        != u64::from(header.chunk_count)
    {
        return Err(PackError::BadTrailer);
    }
    let mut entries = Vec::with_capacity(lines.len());
    let mut seen: BTreeSet<[u8; 16]> = BTreeSet::new();
    let mut next = HEADER_BYTES;
    for line in lines {
        let fields = line.as_array().ok_or(PackError::BadTrailer)?;
        let [id, offset, length] = fields else {
            return Err(PackError::BadTrailer);
        };
        let id = <[u8; 16]>::try_from(id.as_bytes().ok_or(PackError::BadTrailer)?)
            .map_err(|_| PackError::BadTrailer)?;
        let offset = offset.as_uint().ok_or(PackError::BadTrailer)?;
        let length = length.as_uint().ok_or(PackError::BadTrailer)?;
        // Back to back: each blob starts where the last one ended, the first right after the header.
        if offset != next {
            return Err(PackError::BadTrailer);
        }
        let blob_len = u32::try_from(length)
            .map_err(|_| PackError::BadTrailer)
            .and_then(|n| ChunkLen::new(n).ok_or(PackError::BadTrailer))?;
        if blob_len.as_usize() < BLOB_OVERHEAD {
            return Err(PackError::BadTrailer);
        }
        let offset32 = u32::try_from(offset).map_err(|_| PackError::BadTrailer)?;
        next = offset.checked_add(length).ok_or(PackError::BadTrailer)?;
        if !seen.insert(id) {
            return Err(PackError::DuplicateChunk);
        }
        entries.push(PackEntry {
            id: ChunkId::from_bytes(id),
            offset: PackOffset::new(offset32),
            length: blob_len,
        });
    }
    // The last blob ends exactly where the body ends.
    let body_end = HEADER_BYTES.saturating_add(u64::from(header.body_len.get()));
    if next != body_end {
        return Err(PackError::BadTrailer);
    }
    Ok(Opened { entries, body_hash })
}
