//! Packs: a batch of git objects as one immutable object in the bucket
//! (`wal/<name>.pack`), sealed in chunks, and its sealed index (`.idx`).
//!
//! # Format (version 1)
//!
//! - **Records:** the objects in the order of their ids, each `kind (u8, git's pack type
//!   number) | body length (u32 LE) | body`.
//! - **Chunks:** the records cut into chunks of about 64 KiB (a record never spans two
//!   chunks; a bigger one is a chunk of its own). Chunk `i` of `n` is sealed on its own
//!   with the context `"<pack key>#chunk <i>/<n>"`, so a chunk cannot move within the
//!   pack, into another pack, and the pack cannot lose its tail unnoticed. The `.pack`
//!   object is the sealed chunks one after the other: a device can fetch one chunk with
//!   a ranged GET (C6).
//! - **Index:** `"AZMI" | version u8 = 1 | 3 zero bytes | chunk count u32 | object count
//!   u32`, then per chunk `offset in the .pack (u64) | sealed length (u32)`, then per
//!   object, ascending by id, `id (32) | chunk (u32) | offset of its record in the
//!   chunk (u32) | body length (u32) | kind (u8)`; little-endian, sealed whole with the
//!   context `"<idx key>"`.
//! - **Name:** 64 hex digits of the sealer's keyed hash of `"azul-storage meta pack v1"`
//!   and the sorted ids: the same objects make the same name on every device holding
//!   the key, and the name says nothing about them to anyone else.
//!
//! Every object read out of a pack is checked against its id.

use std::collections::BTreeMap;

use super::{
    hex, keys,
    objects::{Kind, ObjectId, Objects},
    MetaError, Sealer,
};
use crate::ByteRange;

/// Records are cut into chunks of about this many bytes.
pub const CHUNK_TARGET: usize = 64 * 1024;

const IDX_MAGIC: &[u8; 4] = b"AZMI";
const IDX_VERSION: u8 = 1;
const IDX_HEADER: usize = 16;
const IDX_CHUNK: usize = 12;
const IDX_ENTRY: usize = 32 + 4 + 4 + 4 + 1;
const RECORD_HEADER: usize = 5;
const NAME_LABEL: &[u8] = b"azul-storage meta pack v1";

/// The context chunk `index` of `count` of the pack at `pack_key` is sealed with.
#[must_use]
pub fn chunk_context(pack_key: &str, index: u32, count: u32) -> String {
    format!("{pack_key}#chunk {index}/{count}")
}

fn corrupt(key: &str, reason: impl Into<String>) -> MetaError {
    MetaError::Corrupt {
        key: key.to_string(),
        reason: reason.into(),
    }
}

fn sealed_error(key: &str, e: &super::SealError) -> MetaError {
    MetaError::Sealed {
        key: key.to_string(),
        reason: e.reason.clone(),
    }
}

/// Collects objects for one pack.
#[derive(Debug, Clone, Default)]
pub struct PackWriter {
    objects: BTreeMap<ObjectId, (Kind, Vec<u8>)>,
}

/// A pack ready for the bucket: its name, the sealed `.pack` and `.idx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedPack {
    /// 64 hex digits: a keyed hash of the ids.
    pub name: String,
    /// The `.pack` object.
    pub pack: Vec<u8>,
    /// The `.idx` object.
    pub idx: Vec<u8>,
    /// How many objects it holds.
    pub objects: u32,
}

impl PackWriter {
    #[must_use]
    pub fn new() -> Self {
        PackWriter::default()
    }

    /// Adds an object (twice is once).
    pub fn add(&mut self, kind: Kind, body: Vec<u8>) -> ObjectId {
        let id = ObjectId::of(kind, &body);
        self.objects.entry(id).or_insert((kind, body));
        id
    }

    /// Adds the object `id` of `objects`; [`MetaError::MissingObject`] when it has none.
    pub fn add_from(&mut self, objects: &Objects, id: &ObjectId) -> Result<(), MetaError> {
        let (kind, body) = objects
            .get(id)
            .ok_or_else(|| MetaError::MissingObject { id: id.to_hex() })?;
        self.objects.entry(*id).or_insert_with(|| (kind, body.to_vec()));
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The pack's name under `sealer`'s key.
    #[must_use]
    pub fn name(&self, sealer: &dyn Sealer) -> String {
        let mut input = Vec::with_capacity(NAME_LABEL.len() + self.objects.len() * 32);
        input.extend_from_slice(NAME_LABEL);
        for id in self.objects.keys() {
            input.extend_from_slice(&id.0);
        }
        hex(&sealer.name_hash(&input))
    }

    /// Seals the pack. An empty pack is refused (a publish without objects has none).
    pub fn seal(&self, sealer: &dyn Sealer) -> Result<SealedPack, MetaError> {
        let name = self.name(sealer);
        let pack_key = keys::pack(&name);
        let idx_key = keys::idx(&name);
        if self.objects.is_empty() {
            return Err(corrupt(&pack_key, "a pack without objects"));
        }
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        let mut current: Vec<u8> = Vec::new();
        let mut entries: Vec<(ObjectId, u32, u32, u32, Kind)> = Vec::with_capacity(self.objects.len());
        for (id, (kind, body)) in &self.objects {
            let len = u32::try_from(body.len())
                .map_err(|_| corrupt(&pack_key, "an object of 4 GiB or more"))?;
            if !current.is_empty() && current.len() + RECORD_HEADER + body.len() > CHUNK_TARGET {
                chunks.push(std::mem::take(&mut current));
            }
            let chunk = chunks.len() as u32;
            let offset = current.len() as u32;
            current.push(kind.code());
            current.extend_from_slice(&len.to_le_bytes());
            current.extend_from_slice(body);
            entries.push((*id, chunk, offset, len, *kind));
        }
        chunks.push(current);
        let count = chunks.len() as u32;
        let mut pack = Vec::new();
        let mut table = Vec::with_capacity(chunks.len());
        for (index, chunk) in chunks.iter().enumerate() {
            let context = chunk_context(&pack_key, index as u32, count);
            let sealed = sealer
                .seal(context.as_bytes(), chunk)
                .map_err(|e| sealed_error(&pack_key, &e))?;
            table.push((pack.len() as u64, sealed.len() as u32));
            pack.extend_from_slice(&sealed);
        }
        let mut plain =
            Vec::with_capacity(IDX_HEADER + table.len() * IDX_CHUNK + entries.len() * IDX_ENTRY);
        plain.extend_from_slice(IDX_MAGIC);
        plain.push(IDX_VERSION);
        plain.extend_from_slice(&[0, 0, 0]);
        plain.extend_from_slice(&count.to_le_bytes());
        plain.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (offset, len) in &table {
            plain.extend_from_slice(&offset.to_le_bytes());
            plain.extend_from_slice(&len.to_le_bytes());
        }
        for (id, chunk, offset, len, kind) in &entries {
            plain.extend_from_slice(&id.0);
            plain.extend_from_slice(&chunk.to_le_bytes());
            plain.extend_from_slice(&offset.to_le_bytes());
            plain.extend_from_slice(&len.to_le_bytes());
            plain.push(kind.code());
        }
        let idx = sealer
            .seal(idx_key.as_bytes(), &plain)
            .map_err(|e| sealed_error(&idx_key, &e))?;
        Ok(SealedPack {
            name,
            pack,
            idx,
            objects: entries.len() as u32,
        })
    }
}

/// Where an object is in a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdxEntry {
    pub id: ObjectId,
    pub chunk: u32,
    /// Where its record starts in the chunk's plaintext.
    pub offset: u32,
    /// The body's length.
    pub len: u32,
    pub kind: Kind,
}

/// A pack's opened index: where every object is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackIndex {
    name: String,
    /// Per chunk: offset in the `.pack` and sealed length.
    chunks: Vec<(u64, u32)>,
    /// Ascending by id.
    entries: Vec<IdxEntry>,
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(word)
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(word)
}

impl PackIndex {
    /// Opens and checks the `.idx` of the pack called `name`.
    pub fn open(sealer: &dyn Sealer, name: &str, sealed: &[u8]) -> Result<PackIndex, MetaError> {
        let key = keys::idx(name);
        let plain = sealer
            .open(key.as_bytes(), sealed)
            .map_err(|e| sealed_error(&key, &e))?;
        if plain.len() < IDX_HEADER || &plain[..4] != IDX_MAGIC {
            return Err(corrupt(&key, "not a pack index"));
        }
        if plain[4] != IDX_VERSION {
            return Err(corrupt(&key, format!("pack index version {}", plain[4])));
        }
        let count = u32_at(&plain, 8) as usize;
        let objects = u32_at(&plain, 12) as usize;
        let expected = IDX_HEADER as u64 + count as u64 * IDX_CHUNK as u64 + objects as u64 * IDX_ENTRY as u64;
        if plain.len() as u64 != expected || count == 0 {
            return Err(corrupt(&key, "a pack index of the wrong length"));
        }
        let mut chunks = Vec::with_capacity(count);
        let mut next = 0u64;
        for i in 0..count {
            let at = IDX_HEADER + i * IDX_CHUNK;
            let (offset, len) = (u64_at(&plain, at), u32_at(&plain, at + 8));
            if offset != next {
                return Err(corrupt(&key, "pack chunks that are not contiguous"));
            }
            next = offset + u64::from(len);
            chunks.push((offset, len));
        }
        let mut entries: Vec<IdxEntry> = Vec::with_capacity(objects);
        let table = IDX_HEADER + count * IDX_CHUNK;
        for i in 0..objects {
            let at = table + i * IDX_ENTRY;
            let mut id = [0u8; 32];
            id.copy_from_slice(&plain[at..at + 32]);
            let entry = IdxEntry {
                id: ObjectId(id),
                chunk: u32_at(&plain, at + 32),
                offset: u32_at(&plain, at + 36),
                len: u32_at(&plain, at + 40),
                kind: Kind::from_code(plain[at + 44])
                    .ok_or_else(|| corrupt(&key, "an object of an unknown kind"))?,
            };
            if entry.chunk as usize >= count {
                return Err(corrupt(&key, "an object in a chunk the pack does not have"));
            }
            if entries.last().is_some_and(|last| last.id >= entry.id) {
                return Err(corrupt(&key, "pack index entries out of order"));
            }
            entries.push(entry);
        }
        Ok(PackIndex {
            name: name.to_string(),
            chunks,
            entries,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Every object's place, ascending by id.
    #[must_use]
    pub fn entries(&self) -> &[IdxEntry] {
        &self.entries
    }

    /// Where the object `id` is, when the pack has it.
    #[must_use]
    pub fn find(&self, id: &ObjectId) -> Option<&IdxEntry> {
        self.entries
            .binary_search_by(|e| e.id.cmp(id))
            .ok()
            .map(|at| &self.entries[at])
    }

    #[must_use]
    pub fn chunk_count(&self) -> u32 {
        self.chunks.len() as u32
    }

    /// The `.pack`'s length.
    #[must_use]
    pub fn pack_len(&self) -> u64 {
        self.chunks
            .last()
            .map_or(0, |(offset, len)| offset + u64::from(*len))
    }

    /// The bytes of chunk `chunk` in the `.pack`, for a ranged read.
    #[must_use]
    pub fn chunk_range(&self, chunk: u32) -> Option<ByteRange> {
        let (offset, len) = *self.chunks.get(chunk as usize)?;
        Some(ByteRange::new(offset, Some(offset + u64::from(len) - 1)))
    }

    /// The plaintext of chunk `chunk` from its sealed bytes.
    pub fn open_chunk(
        &self,
        sealer: &dyn Sealer,
        chunk: u32,
        sealed: &[u8],
    ) -> Result<Vec<u8>, MetaError> {
        let key = keys::pack(&self.name);
        let context = chunk_context(&key, chunk, self.chunk_count());
        sealer
            .open(context.as_bytes(), sealed)
            .map_err(|e| sealed_error(&key, &e))
    }

    /// The body of `entry` out of its opened chunk, checked against its id.
    pub fn object_in_chunk(&self, entry: &IdxEntry, chunk: &[u8]) -> Result<Vec<u8>, MetaError> {
        let key = keys::pack(&self.name);
        let at = entry.offset as usize;
        let end = at + RECORD_HEADER + entry.len as usize;
        if end > chunk.len() {
            return Err(corrupt(&key, "an object past the end of its chunk"));
        }
        if chunk[at] != entry.kind.code() || u32_at(chunk, at + 1) != entry.len {
            return Err(corrupt(&key, "an object whose record disagrees with the index"));
        }
        let body = chunk[at + RECORD_HEADER..end].to_vec();
        if ObjectId::of(entry.kind, &body) != entry.id {
            return Err(corrupt(&key, "an object whose bytes do not have its id"));
        }
        Ok(body)
    }

    /// Every object of the sealed `.pack`, checked, into `objects`.
    pub fn read_into(
        &self,
        sealer: &dyn Sealer,
        sealed_pack: &[u8],
        objects: &mut Objects,
    ) -> Result<(), MetaError> {
        let key = keys::pack(&self.name);
        if sealed_pack.len() as u64 != self.pack_len() {
            return Err(corrupt(&key, "a pack of the wrong length"));
        }
        let mut opened: Vec<Vec<u8>> = Vec::with_capacity(self.chunks.len());
        for (index, (offset, len)) in self.chunks.iter().enumerate() {
            let start = *offset as usize;
            let sealed = &sealed_pack[start..start + *len as usize];
            opened.push(self.open_chunk(sealer, index as u32, sealed)?);
        }
        for entry in &self.entries {
            let body = self.object_in_chunk(entry, &opened[entry.chunk as usize])?;
            objects.insert_checked(entry.id, entry.kind, body)?;
        }
        Ok(())
    }
}
