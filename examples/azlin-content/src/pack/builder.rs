//! Building a pack: chunk blobs in, a sealed pack out (planning/architecture.md section 5.7 "Pack
//! (AZP2): a short header and the chunks, then a trailer sealed with `K_seal`", "packing into 4 MiB
//! packs"; build unit S14, lane W1-S14c).
//!
//! A pack is rewritten, never edited: [`PackBuilder::finish`] consumes the builder and returns a
//! [`SealedPack`] that has no way to add, remove or change a chunk (each of these does not compile):
//!
//! ```compile_fail,E0599
//! use azlin_content::pack::SealedPack;
//! use azlin_content::{ChunkId, SealedChunk};
//!
//! fn append(pack: &mut SealedPack, id: ChunkId, blob: &SealedChunk) {
//!     let _ = pack.add(id, blob); // a finished pack has no `add`
//! }
//! ```
//!
//! ```compile_fail,E0382
//! use azlin_content::pack::{PackBuilder, PackGeometry, Padding, SealKey};
//! use azlin_content::PackClass;
//! use azlin_keys::{Epoch, Secret, SeededEntropy, SpaceId};
//!
//! let seal = SealKey { epoch: Epoch::FIRST, key: Secret::from_bytes([2; 32]) };
//! let builder = PackBuilder::new(
//!     SpaceId::from_bytes([7; 16]),
//!     PackClass::Content,
//!     seal,
//!     PackGeometry::default(),
//!     Padding::None,
//! );
//! let _ = builder.finish(true, &mut SeededEntropy::from_u64(1));
//! let _ = builder.finish(true, &mut SeededEntropy::from_u64(1)); // the builder was consumed
//! ```
//!
//! # What fills a pack
//!
//! [`PackBuilder::add`] puts a blob at the end of the body unless the pack it would make is above
//! `target` bytes (the header, the blobs and the exact trailer for the lines so far plus this one) or
//! has more than `max_chunks` lines: then it answers [`AddOutcome::Full`] and changes nothing, and the
//! caller finishes this builder and starts another. A blob that does not fit even an empty pack within
//! `target` gets a pack of its own, up to `hard_max` (brief P-2: the largest chunk is 4 MiB and a
//! 4 MiB pack has no room for its framing; section 2.12 allows objects of several stripes); a blob that
//! does not fit within `hard_max` is [`AddOutcome::TooBigForAnyPack`]. Blobs are written in the order
//! they are added, and the trailer lists them in that order.

use std::collections::BTreeSet;

use azlin_keys::{Entropy, SpaceId};

use super::header::PackHeader;
use super::names::{pack_id, pack_name};
use super::padding::{keystream, padded_len};
use super::trailer::{encode_plain, entry_len, seal, trailer_len, NONCE_LEN};
use super::{PackError, PackGeometry, Padding, SealKey};
use crate::ids::{ChunkId, PackClass, PackEntry, PackId, PackName, PackSummary, SealedChunk};
use crate::units::{PackLen, PackOffset};

/// What [`PackBuilder::add`] did with a blob.
///
/// Caller: INT-S14-1 (the upload pipeline finishes the builder on `Full` and starts the next one),
/// `repack`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddOutcome {
    /// The blob was added at the end of the body.
    Fits,
    /// The blob was NOT added: the pack would pass `target` bytes or `max_chunks` lines. The caller
    /// finishes this builder and adds the blob to a new one.
    Full,
    /// The blob was NOT added: even alone in a pack it would pass `hard_max` bytes.
    TooBigForAnyPack,
}

/// A pack that is being filled: the blobs added so far, back to back after a reserved header, and the
/// lines of the trailer. Consumed by [`PackBuilder::finish`].
///
/// Caller: INT-S14-1 (azul-storage's upload pipeline and azcloud-kit's sync engine pack the sealed
/// chunks of a batch), `repack` (S14e2's prune).
pub struct PackBuilder {
    space: SpaceId,
    class: PackClass,
    key: SealKey,
    geometry: PackGeometry,
    padding: Padding,
    /// The reserved header (24 zero bytes until `finish`), then the blobs.
    bytes: Vec<u8>,
    entries: Vec<PackEntry>,
    /// The bytes the lines of the trailer take, added up.
    entries_len: usize,
    ids: BTreeSet<ChunkId>,
}

impl core::fmt::Debug for PackBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PackBuilder({:?}, {} chunks, {} bytes)",
            self.class,
            self.entries.len(),
            self.bytes.len()
        )
    }
}

impl PackBuilder {
    /// A builder for a pack of `class` in `space`, whose trailer is sealed with `k_seal`, filled
    /// according to `geometry` and padded by `padding` at [`PackBuilder::finish`]. An invalid geometry
    /// ([`PackGeometry::validate`]) is reported by `add` and `finish`, not here.
    ///
    /// Caller: INT-S14-1 (one builder per pack), `repack`.
    pub fn new(
        space: SpaceId,
        class: PackClass,
        k_seal: SealKey,
        geometry: PackGeometry,
        padding: Padding,
    ) -> PackBuilder {
        PackBuilder {
            space,
            class,
            key: k_seal,
            geometry,
            padding,
            bytes: vec![0u8; PackHeader::LEN],
            entries: Vec::new(),
            entries_len: 0,
            ids: BTreeSet::new(),
        }
    }

    /// Whether no blob was added yet.
    ///
    /// Caller: INT-S14-1 (a builder that stays empty is dropped, not finished: `finish` of an empty
    /// builder is [`PackError::EmptyPack`]), `repack`.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many blobs were added.
    ///
    /// Caller: INT-S14-1 (progress and the batch's bookkeeping), the tests of the builder.
    pub fn chunk_count(&self) -> usize {
        self.entries.len()
    }

    /// Adds the blob `blob` of the chunk `id` at the end of the body, copied as it is: the pack never
    /// reads or changes a blob. [`AddOutcome::Fits`] when it was added; [`AddOutcome::Full`] when
    /// the pack would then pass `target` bytes or `max_chunks` lines (nothing was added: finish this
    /// builder, start another); [`AddOutcome::TooBigForAnyPack`] when the blob alone would pass
    /// `hard_max`. A blob that alone passes `target` but fits `hard_max` is added to an EMPTY builder
    /// (a pack of its own, brief P-2) and is [`AddOutcome::Full`] for a builder that holds anything. A
    /// chunk added twice to one pack is [`PackError::DuplicateChunk`] (the trailer would be
    /// ambiguous); an invalid geometry is [`PackError::BadGeometry`].
    ///
    /// Caller: INT-S14-1 (the upload pipeline), `repack`.
    pub fn add(&mut self, id: ChunkId, blob: &SealedChunk) -> Result<AddOutcome, PackError> {
        self.geometry.validate()?;
        if self.ids.contains(&id) {
            return Err(PackError::DuplicateChunk);
        }
        let length = blob.stored_len();
        let start = u32::try_from(self.bytes.len()).map_err(|_| PackError::TooLong)?;
        let end = start.checked_add(length.get()).ok_or(PackError::TooLong)?;
        let line = entry_len(start, length.get());

        // Alone in a pack of its own: the header at 24, the blob, the one line.
        let alone = PackLen::from_len(
            PackHeader::LEN
                .saturating_add(length.as_usize())
                .saturating_add(trailer_len(
                    1,
                    entry_len(PackHeader::LEN as u32, length.get()),
                )),
        )
        .ok_or(PackError::TooLong)?;
        if alone > self.geometry.hard_max {
            return Ok(AddOutcome::TooBigForAnyPack);
        }

        if !self.entries.is_empty() {
            // The pack as it would be with this blob: the header, every blob and the trailer.
            let lines = self.entries.len().saturating_add(1);
            let would_be = usize::try_from(end)
                .map_err(|_| PackError::TooLong)?
                .saturating_add(trailer_len(lines, self.entries_len.saturating_add(line)));
            let would_be = PackLen::from_len(would_be).ok_or(PackError::TooLong)?;
            let too_many = u64::try_from(lines).map_err(|_| PackError::TooLong)?
                > u64::from(self.geometry.max_chunks);
            if too_many || would_be > self.geometry.target {
                return Ok(AddOutcome::Full);
            }
        }

        if self.entries.is_empty() {
            // The first blob: room for a whole pack at once (the geometry is valid, so `target` is
            // at most the largest pack), so the body is not copied on every growth.
            self.bytes.reserve(self.geometry.target.as_usize());
        }
        self.bytes.extend_from_slice(blob.as_bytes());
        self.entries.push(PackEntry {
            id,
            offset: PackOffset::new(start),
            length,
        });
        self.entries_len = self.entries_len.saturating_add(line);
        self.ids.insert(id);
        Ok(AddOutcome::Fits)
    }

    /// Seals the pack: the padding the policy asks for (`last_of_batch` says whether this is the last
    /// pack of the upload: see the table in `padding.rs`), the trailer sealed with `K_seal` under a
    /// nonce drawn from `ent` (drawn LAST: a failed draw is [`PackError::Entropy`] and makes no pack),
    /// and the header. The result holds the bytes, the lines, the name and the id. The same blobs, key
    /// and nonce give the same bytes. A builder that holds no blob is [`PackError::EmptyPack`].
    ///
    /// Caller: INT-S14-1 (finishes a builder when `add` says `Full` and at the end of a batch),
    /// `repack`.
    pub fn finish(
        self,
        last_of_batch: bool,
        ent: &mut dyn Entropy,
    ) -> Result<SealedPack, PackError> {
        self.geometry.validate()?;
        if self.entries.is_empty() {
            return Err(PackError::EmptyPack);
        }
        let PackBuilder {
            space,
            class,
            key,
            geometry,
            padding,
            mut bytes,
            entries,
            ..
        } = self;

        let chunk_count = u32::try_from(entries.len()).map_err(|_| PackError::TooManyChunks)?;
        let body_len = bytes
            .len()
            .checked_sub(PackHeader::LEN)
            .and_then(PackLen::from_len)
            .ok_or(PackError::SizesInconsistent)?;
        let body = bytes
            .get(PackHeader::LEN..)
            .ok_or(PackError::SizesInconsistent)?;
        let body_hash = *blake3::hash(body).as_bytes();

        // The sizes: the trailer is as long as its plaintext, so the padding can be decided before
        // anything is sealed.
        let plain = encode_plain(&entries, &body_hash)?;
        let trailer_bytes = NONCE_LEN
            .saturating_add(plain.len())
            .saturating_add(super::trailer::TAG_LEN);
        let unpadded = PackLen::from_len(bytes.len().saturating_add(trailer_bytes))
            .ok_or(PackError::TooLong)?;
        let padded = padded_len(unpadded, class, padding, last_of_batch, &geometry)?;
        let pad_len = padded
            .checked_sub(unpadded)
            .ok_or(PackError::SizesInconsistent)?;
        let header = PackHeader {
            class,
            key_epoch: key.epoch,
            chunk_count,
            body_len,
            pad_len,
        };

        // Everything that can fail is behind us but the draw and the two ciphers.
        let mut nonce = [0u8; NONCE_LEN];
        ent.fill(&mut nonce).map_err(PackError::Entropy)?;

        bytes
            .get_mut(..PackHeader::LEN)
            .ok_or(PackError::SizesInconsistent)?
            .copy_from_slice(&header.to_bytes());
        let pad_start = bytes.len();
        bytes.resize(pad_start.saturating_add(pad_len.as_usize()), 0);
        keystream(
            &key,
            &nonce,
            bytes
                .get_mut(pad_start..)
                .ok_or(PackError::SizesInconsistent)?,
        )?;
        bytes.extend_from_slice(&seal(&key, &space, &header, &plain, &nonce)?);

        // The first `add` made room for a whole pack: a short pack gives the rest back (the bytes are
        // ciphertext, so the copy this may make leaves nothing to wipe).
        bytes.shrink_to_fit();
        let len = PackLen::from_len(bytes.len()).ok_or(PackError::TooLong)?;
        let name = pack_name(&bytes);
        Ok(SealedPack {
            id: pack_id(&name),
            name,
            len,
            class,
            bytes,
            entries,
        })
    }
}

/// A finished pack: its bytes, the lines of its trailer, its name and its id. It has no way to be
/// changed (see the module documentation): a pack is rewritten, never edited.
///
/// Caller: INT-S14-1 (uploaded as it is; the id gives the storage key), S14e1 (`summary` for
/// `add_pack`), S14e2 (`PackStore::put_pack`).
#[derive(Clone, PartialEq, Eq)]
pub struct SealedPack {
    bytes: Vec<u8>,
    entries: Vec<PackEntry>,
    id: PackId,
    name: PackName,
    len: PackLen,
    class: PackClass,
}

impl SealedPack {
    /// The bytes of the pack, ciphertext all of it (header, blobs, padding, trailer): what the store
    /// holds, and what [`SealedPack::name`] is the hash of.
    ///
    /// Caller: INT-S14-1 (the upload), the tests of the format.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The lines of the trailer, in the order of the body: where each blob sits.
    ///
    /// Caller: INT-S14-1 (the pointers' `(pack, offset, stored length)`), S14e1 (`add_pack`).
    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }

    /// The id of the pack: the first 16 bytes of its name.
    ///
    /// Caller: INT-S14-1 (the storage key and the pointers' `pack`), S14d (`ChunkRef::pack`).
    pub fn id(&self) -> PackId {
        self.id
    }

    /// The name of the pack: the BLAKE3 hash of its bytes.
    ///
    /// Caller: S14e1 (the hash the chunk index records), INT-S14-3 (checked after a download).
    pub fn name(&self) -> PackName {
        self.name
    }

    /// The class of the pack.
    ///
    /// Caller: S14e1 (`PackInfo::class`), INT-S14-1.
    pub fn class(&self) -> PackClass {
        self.class
    }

    /// The length of the pack in bytes.
    ///
    /// Caller: S14e1 (`PackInfo::len`), S14e2 (the repack budget), INT-S14-1.
    pub fn len(&self) -> PackLen {
        self.len
    }

    /// Whether the pack has no bytes: never, a pack always has its header and trailer.
    ///
    /// Caller: the tests of the builder.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// What the chunk index needs of this pack: the id, the name, the length, the class and the lines.
    ///
    /// Caller: S14e1 (`ChunkIndex::add_pack`).
    pub fn summary(&self) -> PackSummary {
        PackSummary {
            id: self.id,
            name: self.name,
            len: self.len,
            class: self.class,
            entries: self.entries.clone(),
        }
    }

    /// The bytes of the pack, by value.
    ///
    /// Caller: INT-S14-1 (hands the bytes to the store without a copy).
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Prints the id, the length and the number of chunks, never the bytes.
impl core::fmt::Debug for SealedPack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SealedPack({}, {:?}, {} chunks, {} bytes)",
            self.id,
            self.class,
            self.entries.len(),
            self.len
        )
    }
}
