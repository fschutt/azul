//! The chunk format AZL2: the secret key of a space's chunks, the identity of a chunk, and the
//! description of a sealed one (planning/architecture.md section 5.7 "Chunk (AZL2)"; build unit
//! S14, lane W1-S14b).
//!
//! - `full = keyed-BLAKE3(K_chunk, plaintext)`, `chunk_id = derive_key("azlin chunk id v1",
//!   full)[0..16]` and `chunk_key = derive_key("azlin chunk key v1", full)`: separate contexts, so
//!   the id reveals nothing of the key input ([`identity`]).
//! - Equal chunks inside one space have the same `full`, id and key, so they are stored once; the
//!   ids of two spaces differ, so the server cannot tell whether two accounts hold the same file.
//!
//! ```
//! use azlin_content::{identity, KChunk};
//! use azlin_keys::Secret;
//!
//! let a = KChunk::from_secret(Secret::from_bytes([1; 32]));
//! let b = KChunk::from_secret(Secret::from_bytes([2; 32]));
//! let (full_a, id_a) = identity(&a, b"the same bytes");
//! let (full_a2, id_a2) = identity(&a, b"the same bytes");
//! let (_full_b, id_b) = identity(&b, b"the same bytes");
//! assert!(full_a == full_a2 && id_a == id_a2); // one space: one id
//! assert_ne!(id_a, id_b); // two spaces: two ids
//! assert_eq!(id_a, full_a.chunk_id());
//! ```
//!
//! A `KChunk` is key material: no `Clone`, no `Display`, no `Default`, and it prints as
//! `KChunk(***)`:
//!
//! ```compile_fail
//! use azlin_content::KChunk;
//! use azlin_keys::Secret;
//! let k = KChunk::from_secret(Secret::from_bytes([1; 32]));
//! let _copy = k.clone();
//! ```

use core::fmt;

use azlin_keys::Secret;
use blake3::Hasher;

use crate::codec::Codec;
use crate::ids::{ChunkId, ChunkRef, Full, PackId};
use crate::units::{ChunkLen, PackOffset};

/// `K_chunk`: the key of a space's chunk ids and chunk keys, the space's dedup key (planning/architecture.md
/// section 5.5 "Space key, per epoch ... gives `K_seal` ... and `K_chunk`", and "Dedup key epochs:
/// `K_chunk` rotates only on an explicit re-key space"). Derived by azlin-keys
/// (`SpaceKey::k_chunk`, S15c1); here only a typed wrapper, so a chunk function cannot be handed a
/// `K_seal` or any other 32 bytes by mistake. DRIFT X-DELTA-W1S14B-1: whether `K_chunk` rotates with
/// the space epoch is S15c1's question.
///
/// Caller: INT-S14-1 (the upload pipeline builds one from the space key), `identity`,
/// `seal_chunk`, `open_chunk`, `GearTable::keyed`.
pub struct KChunk(Secret<32>);

impl KChunk {
    /// A `K_chunk` holding this secret.
    pub fn from_secret(secret: Secret<32>) -> KChunk {
        KChunk(secret)
    }

    /// The bytes, for the BLAKE3 calls of this crate.
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose()
    }
}

/// Prints `KChunk(***)`: never a byte.
impl fmt::Debug for KChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KChunk(***)")
    }
}

/// What `seal_chunk` tells the pipeline about the chunk it sealed: the chunk's secret hash and id,
/// its plaintext length, the length of the blob (what the pack and the pointer record) and the codec
/// that was chosen. It holds a [`Full`], so it is key material: no `Clone`, a redacted `Debug`, a second
/// copy is [`ChunkMeta::duplicate`].
///
/// Caller: INT-S14-1 (the upload pipeline turns it into a [`ChunkRef`] once the chunk has a place in a
/// pack), S14e1 (`plan_upload` takes a slice of them).
#[derive(Debug, PartialEq, Eq)]
pub struct ChunkMeta {
    /// The chunk's secret hash.
    pub full: Full,
    /// The chunk's id (the id of `full`).
    pub id: ChunkId,
    /// The length of the plaintext.
    pub raw_len: ChunkLen,
    /// The length of the blob: the codec byte, the nonce, the stored bytes and the tag.
    pub stored_len: ChunkLen,
    /// How the stored bytes were made from the plaintext.
    pub codec: Codec,
}

impl ChunkMeta {
    /// A second owner of the same description: the only way to copy one.
    pub fn duplicate(&self) -> ChunkMeta {
        ChunkMeta {
            full: self.full.duplicate(),
            id: self.id,
            raw_len: self.raw_len,
            stored_len: self.stored_len,
            codec: self.codec,
        }
    }

    /// The pointer entry of this chunk once its blob sits at `offset` in the pack `pack`.
    ///
    /// Caller: INT-S14-1 (after the pack builder has placed the blob).
    pub fn into_ref(self, pack: PackId, offset: PackOffset) -> ChunkRef {
        ChunkRef {
            full: self.full,
            raw_len: self.raw_len,
            pack,
            offset,
            stored_len: self.stored_len,
        }
    }
}

/// Who opens a chunk. A member of the space holds `K_chunk`, so `open_chunk` can check that the
/// plaintext hashes to the chunk's `full`; a share recipient holds `full` (the share carries it) but
/// not `K_chunk` (diagram of planning/architecture.md section 5.5: share to chunk) and can check
/// only that the id is the id of that `full`: the file-level BLAKE3 of the pointer is the check there.
///
/// Caller: INT-S14-1 (the download pipeline: `Member`), S16 (the share path: `Share`).
#[derive(Clone, Copy, Debug)]
pub enum OpenAs<'a> {
    /// A member of the space, with its `K_chunk`.
    Member(&'a KChunk),
    /// A share recipient: `full` and nothing else.
    Share,
}

/// The identity of a plaintext chunk under a space's `K_chunk`: `full = keyed-BLAKE3(K_chunk,
/// plaintext)` and `chunk_id = derive_key("azlin chunk id v1", full)[0..16]`. Pure and deterministic:
/// the same space and the same bytes give the same pair; nothing is drawn from an entropy source. An
/// empty plaintext has an identity too (the hash of nothing); it is `seal_chunk` that refuses it.
///
/// Caller: INT-S14-1 (the dedup lookup before sealing), `seal_chunk`.
pub fn identity(k: &KChunk, plaintext: &[u8]) -> (Full, ChunkId) {
    let mut hasher = Hasher::new_keyed(k.expose());
    hasher.update(plaintext);
    let mut hash = Secret::from_bytes([0u8; 32]);
    hasher.finalize_xof().fill(hash.expose_mut());
    let full = Full::from_secret(hash);
    let id = full.chunk_id();
    (full, id)
}
