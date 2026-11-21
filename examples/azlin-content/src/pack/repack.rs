//! Repacking: new packs from blobs that already exist, copied as they are (planning/architecture.md
//! section 5.7 "Garbage collection is the client's ... packs below 50 % live are repacked"; build unit
//! S14, lane W1-S14c, used by the prune of lane S14e2).
//!
//! A repack needs `K_seal` and nothing else: the blobs are ciphertext that is copied byte for byte, so
//! neither `K_chunk` nor any chunk key is touched and no chunk is decrypted, and the pack that held them
//! is not changed (no function of this module takes a pack by `&mut`: a pack is rewritten, never
//! edited). The new packs have new trailers with the new offsets; the caller publishes the new
//! locations to the chunk index before it deletes the old packs (the prune's order of effects).

use azlin_keys::{Entropy, SpaceId};

use super::builder::{AddOutcome, PackBuilder, SealedPack};
use super::{PackError, PackGeometry, Padding, SealKey};
use crate::ids::{PackClass, PackEntry, SealedChunk};

/// New packs that hold the blobs of `live`, in order, copied verbatim, under the default padding
/// policy ([`Padding::Buckets`]: the last pack of the batch is padded to the next bucket). Each item of
/// `live` is a line of the old pack's trailer and the bytes of its blob, read by a ranged read (the
/// line's offset is the OLD one and is not used but for the length check). See [`repack_with`].
/// DRIFT X-DELTA-W1S14C-12: the brief's signature has no padding policy.
///
/// Caller: S14e2 (the prune's repack of the packs below the live floor).
pub fn repack(
    live: &[(PackEntry, &[u8])],
    space: SpaceId,
    class: PackClass,
    k: SealKey,
    g: PackGeometry,
    ent: &mut dyn Entropy,
) -> Result<Vec<SealedPack>, PackError> {
    repack_with(live, space, class, k, g, Padding::default(), ent)
}

/// [`repack`] with the padding policy of the space: every pack but the last is finished with
/// `last_of_batch = false`, the last with `true`. The blobs go into packs in the order of `live`, a
/// pack is closed when the next blob does not fit (`AddOutcome::Full`), and a blob that fits no pack
/// even alone is [`PackError::TooBigForAnyPack`]. A blob whose bytes are not as long as its line says
/// is [`PackError::LengthMismatch`]; one shorter than its framing or longer than the largest blob is
/// [`PackError::Chunk`]; a chunk twice is [`PackError::DuplicateChunk`]. Nothing is returned on an
/// error, and an empty `live` gives no pack.
///
/// Caller: S14e2 (the prune, under the space's policy).
pub fn repack_with(
    live: &[(PackEntry, &[u8])],
    space: SpaceId,
    class: PackClass,
    k: SealKey,
    g: PackGeometry,
    padding: Padding,
    ent: &mut dyn Entropy,
) -> Result<Vec<SealedPack>, PackError> {
    g.validate()?;
    let mut done: Vec<SealedPack> = Vec::new();
    let mut builder = PackBuilder::new(space, class, k.duplicate(), g, padding);
    for (entry, bytes) in live {
        if bytes.len() != entry.length.as_usize() {
            return Err(PackError::LengthMismatch);
        }
        let blob = SealedChunk::from_stored(bytes).map_err(PackError::Chunk)?;
        match builder.add(entry.id, &blob)? {
            AddOutcome::Fits => {}
            AddOutcome::TooBigForAnyPack => return Err(PackError::TooBigForAnyPack),
            AddOutcome::Full => {
                // This pack is closed; the blob starts the next one, and it fits an empty pack or no pack.
                let closed = core::mem::replace(
                    &mut builder,
                    PackBuilder::new(space, class, k.duplicate(), g, padding),
                );
                done.push(closed.finish(false, ent)?);
                match builder.add(entry.id, &blob)? {
                    AddOutcome::Fits => {}
                    AddOutcome::Full | AddOutcome::TooBigForAnyPack => {
                        return Err(PackError::TooBigForAnyPack);
                    }
                }
            }
        }
    }
    if !builder.is_empty() {
        done.push(builder.finish(true, ent)?);
    }
    Ok(done)
}
