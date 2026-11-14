//! The chunk-list object of a huge file: the sealed repository object that holds the chunk references
//! of a file with more than `INLINE_MAX_CHUNKS` chunks, and the seam that seals it (planning/architecture.md
//! section 5.7 "Pointer v2": "`chunks`: ..., or a chunk-list object for huge files"; build unit S14, lane
//! W1-S14d; DRIFT X-DELTA-W1S14D-3).
//!
//! THE OBJECT is deterministic CBOR, `{1: 1, 2: [chunk reference, ...]}` (key 1 the object's version, key
//! 2 the references in file order, each `[full, raw_len, pack, offset, stored_len]` as in the pointer),
//! sealed by the repository's sealer ([`ObjectSealer`]; INT-S14-1 adapts azul-storage's `meta::Sealer`,
//! XChaCha20-Poly1305 under a subkey of `K_seal`) with the context
//! `"azlin 2026-11 chunk list seal v1" || space_id`, so an object of one space does not open in another.
//! The pointer names it by [`ChunkListRef`]: `name` is the BLAKE3 hash of the sealed bytes (the name the
//! store keeps), `count` the number of references and `total_raw` the sum of their raw lengths; the
//! pointer is signed, so the object a signed pointer names cannot be swapped.
//!
//! WHAT IS CHECKED, IN THIS ORDER, before anything is allocated for the list: the declared `count` is
//! more than `INLINE_MAX_CHUNKS` and at most `MAX_FILE_CHUNKS`; the declared total is possible for that
//! many chunks; the sealed bytes are no longer than a list of that many chunks can be; their BLAKE3 is
//! `name`. Then the sealer opens them (a wrong key, another space, a changed bit: `Seal(Auth)`), the CBOR
//! is read under limits that follow from the declared count, the version is read (a newer one is
//! `NewerList`, never damage), the number of references is `count`, each reference is checked and the
//! raw lengths add up to `total_raw`.
//!
//! MEMORY: a decoded list costs about 70 bytes per chunk in the result and a few times that while the
//! tree is read (1 million chunks, a file of about 1 TiB, is a few hundred MiB for a moment); at the cap
//! `MAX_FILE_CHUNKS` it is a few GiB. The bound is the DECLARED count of a signed pointer, never the size
//! of the bytes a server sends (DRIFT X-DELTA-W1S14D-12).

use azlin_cbor::{decode_secret, encode, IntMap, IntMapBuilder, Limits, SecretValue, Value};
use azlin_keys::SpaceId;
use zeroize::Zeroizing;

use super::error::{PointerError, SealError};
use super::validate::check_list_total;
use super::wire::{ref_from_value, ref_to_value};
use super::{ChunkListRef, INLINE_MAX_CHUNKS};
use crate::ids::ChunkRef;
use crate::limits::{LIST_SEAL, MAX_FILE_CHUNKS};
use crate::units::FileSize;

/// The version of the chunk-list object this build writes and reads.
///
/// Caller: `encode_chunk_list`, `decode_chunk_list`, the tests of a newer list.
pub const LIST_VERSION: u64 = 1;

/// The most bytes a sealer may add to a plaintext: 1 KiB (a key id, a nonce and a tag are about 60). A
/// sealed object longer than its list plus this is refused before it is hashed or opened.
///
/// Caller: INT-S14-1 (the adapter of azul-storage's `Sealer` stays within it), `decode_chunk_list`.
pub const MAX_SEAL_OVERHEAD: usize = 1024;

/// The most bytes one chunk reference takes encoded: the array head (1), `full` (2 + 32), `raw_len`
/// (5), `pack` (1 + 16), `offset` (5) and `stored_len` (5).
const MAX_REF_BYTES: u64 = 67;

/// The bytes before the references: the map head, key 1 with its version, key 2, the array head (up to
/// 5 bytes), with some slack.
const HEADER_BYTES: u64 = 16;

/// The seam to the repository's sealer: authenticated encryption of one object under the space's key, with
/// a context that is authenticated and not stored (INT-S14-1 adapts azul-storage's `meta::Sealer`, whose
/// `seal` and `open` have this shape).
///
/// The contract (DRIFT X-DELTA-W1S14D-14): `open(context, sealed)` returns the plaintext of what `seal(context, plaintext)` made, and
/// an error for anything else: another context, another key, any changed byte; the sealed object is never
/// shorter than its plaintext and adds at most `MAX_SEAL_OVERHEAD` bytes; the plaintext comes back in a
/// wiped-on-drop buffer.
///
/// Seam: INT-S14-1 (the repository's `Sealer` behind this trait); `encode_chunk_list` and
/// `decode_chunk_list` call it.
pub trait ObjectSealer {
    /// `plaintext` encrypted and authenticated together with `context`.
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError>;

    /// The plaintext of what `seal` made with the same `context`; an error when anything differs.
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError>;
}

/// The name of a sealed chunk-list object: the BLAKE3 hash of its bytes, the hash the store keeps and
/// the pointer's [`ChunkListRef::name`].
///
/// Caller: `encode_chunk_list`, `decode_chunk_list`, INT-S14-1 (a fetched object is checked against the
/// name before it is handed to `decode_chunk_list`), the fuzz target `chunk_list_decode`.
pub fn object_name(sealed: &[u8]) -> [u8; 32] {
    *blake3::hash(sealed).as_bytes()
}

/// The context the object is sealed under: the purpose and the space.
fn list_context(space: &SpaceId) -> Vec<u8> {
    [LIST_SEAL.as_str().as_bytes(), space.as_bytes().as_slice()].concat()
}

/// A chunk-list object lists more than `INLINE_MAX_CHUNKS` and at most `MAX_FILE_CHUNKS` chunks.
fn check_count(count: u32) -> Result<(), PointerError> {
    if count <= INLINE_MAX_CHUNKS {
        Err(PointerError::ObjectTooSmall)
    } else if count > MAX_FILE_CHUNKS {
        Err(PointerError::TooManyChunks)
    } else {
        Ok(())
    }
}

/// The limits the plaintext of a list of `count` chunks is read under: room for one more entry than
/// declared (so a list that is one too long is `ListCount`, not a codec error), a depth of 3, strings of
/// at most 32 bytes.
fn list_limits(count: u32) -> Result<Limits, PointerError> {
    let slots = u64::from(count)
        .checked_add(1)
        .ok_or(PointerError::TooManyChunks)?;
    let bytes = slots
        .checked_mul(MAX_REF_BYTES)
        .and_then(|b| b.checked_add(HEADER_BYTES))
        .ok_or(PointerError::TooManyChunks)?;
    let items = slots
        .checked_mul(6)
        .and_then(|i| i.checked_add(8))
        .ok_or(PointerError::TooManyChunks)?;
    Ok(Limits {
        max_input: u32::try_from(bytes).map_err(|_| PointerError::TooManyChunks)?,
        max_depth: 3,
        max_items: count.saturating_add(1),
        max_total_items: u32::try_from(items).map_err(|_| PointerError::TooManyChunks)?,
        max_text: 32,
    })
}

/// The most bytes the sealed object of such a list can have: the plaintext's bound and the sealer's.
fn max_sealed_len(limits: &Limits) -> usize {
    (limits.max_input as usize).saturating_add(MAX_SEAL_OVERHEAD)
}

/// Writes the chunk-list object of a huge file: the references in file order as deterministic CBOR,
/// sealed by `s` under the context of `space`. Returns the sealed bytes (the repository stores them) and
/// the reference the pointer carries (`name` is the BLAKE3 of the sealed bytes). A list of at most
/// `INLINE_MAX_CHUNKS` chunks is `ObjectTooSmall` (it belongs inline), one of more than
/// `MAX_FILE_CHUNKS` is `TooManyChunks`; every reference is checked (`ChunkRef::check`).
///
/// The plaintext (it holds the hashes `full`, which are key material) is built in wiped buffers.
///
/// Caller: INT-S14-1 and INT-S14-2 (the upload pipeline of a file of more than `INLINE_MAX_CHUNKS`
/// chunks).
pub fn encode_chunk_list(
    chunks: &[ChunkRef],
    space: &SpaceId,
    s: &dyn ObjectSealer,
) -> Result<(Vec<u8>, ChunkListRef), PointerError> {
    let count = u32::try_from(chunks.len()).map_err(|_| PointerError::TooManyChunks)?;
    check_count(count)?;
    let mut total = FileSize::ZERO;
    for r in chunks {
        r.check().map_err(PointerError::BadChunk)?;
        total = total
            .checked_add_chunk(r.raw_len)
            .ok_or(PointerError::Overflow)?;
    }
    let limits = list_limits(count)?;

    let mut items = Vec::with_capacity(chunks.len());
    for r in chunks {
        items.push(ref_to_value(r));
    }
    let tree = SecretValue::new(
        IntMapBuilder::new()
            .put(1, Value::Uint(LIST_VERSION))
            .put(2, Value::Array(items))
            .build()?,
    );
    let plaintext = Zeroizing::new(encode(tree.as_value(), &limits)?);
    drop(tree);

    let sealed = s.seal(&list_context(space), &plaintext)?;
    if sealed.len() > max_sealed_len(&limits) {
        return Err(PointerError::ListTooLong);
    }
    let list = ChunkListRef {
        name: object_name(&sealed),
        count,
        total_raw: total,
    };
    Ok((sealed, list))
}

/// Reads the chunk references of a huge file from its sealed object, checked against what the signed
/// pointer declares (`r`). In this order (see the module documentation): the declared count and total
/// (`ObjectTooSmall`, `TooManyChunks`, `ListTotal`), the length of `sealed` (`ListTooLong`), its BLAKE3
/// against `r.name` (`ListName`: a spliced or substituted object), the sealer (`Seal`: another space, a
/// wrong key, a flipped bit), the CBOR and its version (`Cbor`, `Shape`, `NewerList`), the number of
/// references (`ListCount`), each reference (`BadShape`, `BadChunk`) and the sum of the raw lengths
/// (`ListTotal`). Nothing is allocated for the list before the declared count has been checked.
///
/// The vector has its exact capacity: the hashes inside it are key material, and a growth would leave an
/// unwiped copy behind.
///
/// Caller: INT-S14-1 and INT-S14-2 (the download pipeline of a file whose pointer has an `Object` list),
/// S14e2 (through INT-S14-1: the live set of such a file), the fuzz target `chunk_list_decode`.
pub fn decode_chunk_list(
    sealed: &[u8],
    r: &ChunkListRef,
    space: &SpaceId,
    s: &dyn ObjectSealer,
) -> Result<Vec<ChunkRef>, PointerError> {
    check_count(r.count)?;
    check_list_total(r.count, r.total_raw)?;
    let limits = list_limits(r.count)?;
    if sealed.len() > max_sealed_len(&limits) {
        return Err(PointerError::ListTooLong);
    }
    if object_name(sealed) != r.name {
        return Err(PointerError::ListName);
    }
    let plaintext = s.open(&list_context(space), sealed)?;
    if plaintext.len() > limits.max_input as usize {
        return Err(PointerError::ListTooLong);
    }

    let tree = decode_secret(&plaintext, &limits)?;
    let mut m = IntMap::new(tree.as_value())?;
    match m.req_u64(1)? {
        LIST_VERSION => {}
        0 => return Err(PointerError::UnsupportedList(0)),
        newer => {
            return match u8::try_from(newer) {
                Ok(version) => Err(PointerError::NewerList(version)),
                Err(_) => Err(PointerError::BadVersion),
            }
        }
    }
    let entries = m.req_array(2)?;
    m.finish()?;
    if entries.len() != r.count as usize {
        return Err(PointerError::ListCount);
    }

    let mut refs = Vec::with_capacity(entries.len());
    let mut total = FileSize::ZERO;
    for entry in entries {
        let chunk = ref_from_value(entry)?;
        total = total
            .checked_add_chunk(chunk.raw_len)
            .ok_or(PointerError::Overflow)?;
        refs.push(chunk);
    }
    if total != r.total_raw {
        return Err(PointerError::ListTotal);
    }
    Ok(refs)
}
