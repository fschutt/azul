//! The chunk format AZL2: the secret key of a space's chunks, the identity of a chunk, and the
//! sealing and opening of its blob (planning/architecture.md section 5.7 "Chunk (AZL2)"; build unit
//! S14, lane W1-S14b).
//!
//! - `full = keyed-BLAKE3(K_chunk, plaintext)`, `chunk_id = derive_key("azlin chunk id v1",
//!   full)[0..16]` and `chunk_key = derive_key("azlin chunk key v1", full)`: separate contexts, so
//!   the id reveals nothing of the key input ([`identity`]).
//! - The blob is `codec (1) || nonce (24) || ciphertext || tag (16)` ([`blob`]; DRIFT
//!   X-DELTA-W1S14B-3). The ciphertext is XChaCha20-Poly1305 under `chunk_key` with a SYNTHETIC
//!   nonce, `nonce = keyed-BLAKE3(chunk_key, codec || stored bytes)[0..24]`, and the associated data
//!   `"AZL2" || space_id || chunk_id || codec` binds the blob to its format, its space, its chunk and
//!   its codec ([`seal_chunk`], [`open_chunk`]). A zero nonce would reuse the keystream: the
//!   compressed bytes differ between devices and recompression passes (the codec is chosen by a time
//!   budget) while the key stays the same; the synthetic nonce changes with every change of the
//!   stored bytes.
//! - Equal chunks inside one space have the same `full`, id and key, so they are stored once; the ids
//!   of two spaces differ, so the server cannot tell whether two accounts hold the same file.
//! - Nothing is drawn from an entropy source: sealing is a pure function (same inputs, same bytes).
//!
//! ```
//! use azlin_content::{identity, open_chunk, seal_chunk, CompressPolicy, KChunk, OpenAs, StoredOnly};
//! use azlin_keys::{Secret, SpaceId};
//!
//! let space = SpaceId::from_bytes([7; 16]);
//! let k = KChunk::from_secret(Secret::from_bytes([1; 32]));
//! let plaintext = b"the bytes of one chunk";
//!
//! let (meta, blob) = seal_chunk(&space, &k, plaintext, &StoredOnly, CompressPolicy::plan()).unwrap();
//! let (full, id) = identity(&k, plaintext);
//! assert!(meta.full == full && meta.id == id);
//!
//! let opened = open_chunk(
//!     &space,
//!     &meta.full,
//!     &meta.id,
//!     blob.as_bytes(),
//!     meta.raw_len,
//!     OpenAs::Member(&k),
//!     &StoredOnly,
//! )
//! .unwrap();
//! assert_eq!(&opened[..], &plaintext[..]);
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
//!
//! `seal_chunk` has no entropy parameter; its signature is exactly this (a compile-time check):
//!
//! ```
//! use azlin_content::{seal_chunk, ChunkError, ChunkMeta, CompressPolicy, Compressor, KChunk, SealedChunk};
//! use azlin_keys::SpaceId;
//!
//! let _signature: fn(
//!     &SpaceId,
//!     &KChunk,
//!     &[u8],
//!     &dyn Compressor,
//!     CompressPolicy,
//! ) -> Result<(ChunkMeta, SealedChunk), ChunkError> = seal_chunk;
//! ```
//!
//! `open_chunk` hands the plaintext back in a wiped-on-drop buffer (a compile-time check):
//!
//! ```
//! use azlin_content::{open_chunk, ChunkError, ChunkId, ChunkLen, Compressor, Full, OpenAs};
//! use azlin_keys::SpaceId;
//! use zeroize::Zeroizing;
//!
//! let _signature: fn(
//!     &SpaceId,
//!     &Full,
//!     &ChunkId,
//!     &[u8],
//!     ChunkLen,
//!     OpenAs<'_>,
//!     &dyn Compressor,
//! ) -> Result<Zeroizing<Vec<u8>>, ChunkError> = open_chunk;
//! ```

use core::fmt;

use azlin_keys::{Secret, SpaceId};
use blake3::Hasher;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Key, Tag, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::codec::{Codec, CompressPolicy, Compressor};
use crate::error::{ChunkError, CodecError};
use crate::ids::{ChunkId, ChunkRef, Full, PackId, SealedChunk};
use crate::limits::{BLOB_CODEC_LEN, BLOB_NONCE_LEN, BLOB_OVERHEAD, MAX_BLOB, MAX_CHUNK};
use crate::units::{ChunkLen, PackOffset};

/// The format tag at the start of the associated data of every chunk.
const AAD_MAGIC: &[u8] = b"AZL2";

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

/// The layout of a chunk blob (brief P-1; DRIFT X-DELTA-W1S14B-3), behind one place: `codec (1 byte)
/// || nonce (24) || ciphertext || tag (16)`. The codec byte is authenticated by the associated data;
/// the nonce is synthetic. The constants are the layout's (and are pinned by `c16`).
pub mod blob {
    use crate::error::ChunkError;
    use crate::limits::{BLOB_CODEC_LEN, BLOB_NONCE_LEN, BLOB_OVERHEAD, BLOB_TAG_LEN};

    /// The bytes of the codec at the start of a blob.
    ///
    /// Caller: the tests of the layout (C16).
    pub const CODEC_LEN: usize = BLOB_CODEC_LEN;

    /// The bytes of the synthetic nonce after the codec byte.
    ///
    /// Caller: the tests of the layout (C16).
    pub const NONCE_LEN: usize = BLOB_NONCE_LEN;

    /// The bytes of the Poly1305 tag at the end of a blob.
    ///
    /// Caller: the tests of the layout (C16).
    pub const TAG_LEN: usize = BLOB_TAG_LEN;

    /// What a blob adds to its stored bytes: 41 bytes. It is also the shortest blob (no stored
    /// bytes), the bound under which `open_chunk` answers `Truncated`.
    ///
    /// Caller: the tests of the layout (C16), S14c (padding arithmetic).
    pub const OVERHEAD: usize = BLOB_OVERHEAD;

    /// The parts of a blob, borrowed from its bytes.
    pub(crate) struct Parts<'a> {
        /// The codec byte as written: not yet authenticated, not yet interpreted.
        pub(crate) codec: u8,
        /// The synthetic nonce.
        pub(crate) nonce: &'a [u8; NONCE_LEN],
        /// The ciphertext: as long as the stored bytes.
        pub(crate) ciphertext: &'a [u8],
        /// The Poly1305 tag.
        pub(crate) tag: &'a [u8; TAG_LEN],
    }

    /// Splits a blob into its parts; a blob shorter than its framing is `Truncated`.
    pub(crate) fn decode(blob: &[u8]) -> Result<Parts<'_>, ChunkError> {
        let (&codec, rest) = blob.split_first().ok_or(ChunkError::Truncated)?;
        let (nonce, rest) = rest
            .split_first_chunk::<NONCE_LEN>()
            .ok_or(ChunkError::Truncated)?;
        let (ciphertext, tag) = rest
            .split_last_chunk::<TAG_LEN>()
            .ok_or(ChunkError::Truncated)?;
        Ok(Parts {
            codec,
            nonce,
            ciphertext,
            tag,
        })
    }
}

/// The associated data of a chunk: `"AZL2" || space_id || chunk_id || codec` (planning/architecture.md
/// section 5.7).
fn aad(space: &SpaceId, id: &ChunkId, codec_byte: u8) -> Vec<u8> {
    [
        AAD_MAGIC,
        space.as_bytes().as_slice(),
        id.as_bytes().as_slice(),
        &[codec_byte],
    ]
    .concat()
}

/// Seals stored bytes under the chunk named by `full` and `id`: the step of `seal_chunk` after the
/// codec was chosen, public for the recompression pass (a chunk re-sealed under another codec keeps
/// its `full`, id and key) and for the known-answer vectors. `stored` is what `codec` made of the
/// plaintext (the plaintext itself for [`Codec::Stored`]); it is at least one byte and at most
/// [`MAX_CHUNK`]. Deterministic: the nonce is `keyed-BLAKE3(chunk_key, codec || stored)[0..24]`, so
/// the same inputs give the same blob, and a change of the stored bytes changes the nonce.
///
/// Caller: INT-S14-1 (the recompression pass), `seal_chunk`, the vectors of this crate.
pub fn seal_stored(
    space: &SpaceId,
    full: &Full,
    id: &ChunkId,
    codec: Codec,
    stored: &[u8],
) -> Result<SealedChunk, ChunkError> {
    if stored.is_empty() {
        return Err(ChunkError::EmptyChunk);
    }
    if stored.len() > MAX_CHUNK.as_usize() {
        return Err(ChunkError::TooLong);
    }
    let key = full.chunk_key();
    let codec_byte = codec.byte();

    // nonce = keyed-BLAKE3(chunk_key, codec || stored bytes)[0..24]: the first 24 bytes of the hash
    // are the first 24 bytes of its XOF.
    let mut hasher = Hasher::new_keyed(key.expose());
    hasher.update(&[codec_byte]);
    hasher.update(stored);
    let mut nonce = [0u8; BLOB_NONCE_LEN];
    hasher.finalize_xof().fill(&mut nonce);

    // blob = codec || nonce || stored (encrypted in place) || tag. The vector has its final capacity
    // from the start, so it never reallocates with the plaintext in it; it is wiped if anything fails.
    let mut blob = Zeroizing::new(Vec::with_capacity(
        BLOB_OVERHEAD.saturating_add(stored.len()),
    ));
    blob.push(codec_byte);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(stored);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let body = blob
        .get_mut(BLOB_CODEC_LEN + BLOB_NONCE_LEN..)
        .ok_or(ChunkError::Truncated)?;
    // The cipher refuses only a buffer past its own limit (2^38 bytes): a length this format never has.
    let tag = cipher
        .encrypt_in_place_detached(
            XNonce::from_slice(&nonce),
            &aad(space, id, codec_byte),
            body,
        )
        .map_err(|_| ChunkError::TooLong)?;
    blob.extend_from_slice(&tag);
    SealedChunk::from_vec(core::mem::take(&mut *blob))
}

/// Seals one chunk: the identity of the plaintext, the codec (the compressor's when it saves enough,
/// else the plaintext as it is), and the blob. Returns what the pipeline records ([`ChunkMeta`]) and the
/// blob a pack stores. Draws NO randomness: the same inputs give the same bytes (the nonce is
/// synthetic). A plaintext of no bytes is `EmptyChunk` (an empty file has an empty chunk list), one of
/// more than [`MAX_CHUNK`] bytes is `TooLong`; a compressor that fails stops the seal.
///
/// Caller: INT-S14-1 (azul-storage's upload pipeline and azcloud-kit's sync engine).
pub fn seal_chunk(
    space: &SpaceId,
    k: &KChunk,
    plaintext: &[u8],
    c: &dyn Compressor,
    p: CompressPolicy,
) -> Result<(ChunkMeta, SealedChunk), ChunkError> {
    if plaintext.is_empty() {
        return Err(ChunkError::EmptyChunk);
    }
    let raw_len = match ChunkLen::from_len(plaintext.len()) {
        Some(len) if len <= MAX_CHUNK => len,
        _ => return Err(ChunkError::TooLong),
    };
    let (full, id) = identity(k, plaintext);

    // The compressor's stream is kept only when it saves enough (the 5 % rule of the plan).
    let kept = match c.compress(plaintext)? {
        Some((codec, stored)) if codec != Codec::Stored => {
            if stored.is_empty() {
                return Err(ChunkError::Codec(CodecError::Damaged));
            }
            if p.keeps(&stored, plaintext) {
                Some((codec, stored))
            } else {
                None
            }
        }
        _ => None,
    };
    let (codec, sealed) = match &kept {
        Some((codec, stored)) => (*codec, seal_stored(space, &full, &id, *codec, stored)?),
        None => (
            Codec::Stored,
            seal_stored(space, &full, &id, Codec::Stored, plaintext)?,
        ),
    };
    let meta = ChunkMeta {
        full,
        id,
        raw_len,
        stored_len: sealed.stored_len(),
        codec,
    };
    Ok((meta, sealed))
}

/// Opens one chunk: checks the blob against the associated data, decompresses it within the declared
/// length, and checks what it can of the content. In this order, so that nothing but `Truncated`,
/// `TooLong`, `LengthMismatch` (stored bytes longer than `raw_len`) and `Auth` can come out of bytes a
/// server made up:
///
/// 1. `raw_len` above [`MAX_CHUNK`] or a blob above [`MAX_BLOB`] is `TooLong`; a blob shorter than its
///    framing is `Truncated`; more stored bytes than `raw_len` is `LengthMismatch` (DRIFT
///    X-DELTA-W1S14B-13: nothing is allocated for a blob that cannot be the declared chunk);
/// 2. the authenticated decryption under `chunk_key = derive_key("azlin chunk key v1", full)` and the
///    associated data `"AZL2" || space_id || chunk_id || codec byte` as read: another space, another
///    chunk id, another codec byte, another key or any flipped bit is `Auth`, whatever the part;
/// 3. only now is the codec byte read: one this build does not read is `BadCodec`;
/// 4. the id must be the id of `full` (`IdMismatch`; brief P-4);
/// 5. the stored bytes are decompressed by `c`, which may produce at most `raw_len` bytes (rule I2: the
///    pointer's declared size bounds every decoder); the result must be exactly `raw_len` bytes
///    (`LengthMismatch`);
/// 6. a member (`OpenAs::Member`) checks that the plaintext hashes to `full` under its `K_chunk`
///    (`ContentMismatch`); a share recipient has no `K_chunk` and relies on the file-level BLAKE3 of
///    the pointer.
///
/// The plaintext comes back in a `Zeroizing` buffer of exactly `raw_len` bytes; every buffer that held
/// stored bytes or plaintext is wiped on every path.
///
/// Caller: INT-S14-1 (azul-storage's download pipeline and azcloud-kit's sync engine), S16 (a share
/// recipient opens with `OpenAs::Share`).
pub fn open_chunk(
    space: &SpaceId,
    full: &Full,
    id: &ChunkId,
    sealed: &[u8],
    raw_len: ChunkLen,
    who: OpenAs<'_>,
    c: &dyn Compressor,
) -> Result<Zeroizing<Vec<u8>>, ChunkError> {
    // 1. What the declared and the given lengths allow, before anything is allocated.
    if raw_len > MAX_CHUNK || sealed.len() > MAX_BLOB.as_usize() {
        return Err(ChunkError::TooLong);
    }
    let parts = blob::decode(sealed)?;
    if parts.ciphertext.len() > raw_len.as_usize() {
        return Err(ChunkError::LengthMismatch);
    }

    // 2. Authenticate. The codec byte is used here only as the byte the associated data names.
    let key = full.chunk_key();
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let mut stored = Zeroizing::new(parts.ciphertext.to_vec());
    cipher
        .decrypt_in_place_detached(
            XNonce::from_slice(parts.nonce),
            &aad(space, id, parts.codec),
            stored.as_mut_slice(),
            Tag::from_slice(parts.tag),
        )
        .map_err(|_| ChunkError::Auth)?;

    // 3. The blob is authentic: its codec byte means something now.
    let codec = Codec::from_byte(parts.codec).map_err(|_| ChunkError::BadCodec(parts.codec))?;

    // 4. The id names this `full`.
    if *id != full.chunk_id() {
        return Err(ChunkError::IdMismatch);
    }

    // 5. The plaintext, within the declared length.
    let plaintext = match codec {
        Codec::Stored => stored,
        other => c.decompress(other, &stored, raw_len)?,
    };
    if plaintext.len() != raw_len.as_usize() {
        return Err(ChunkError::LengthMismatch);
    }

    // 6. A member can check the content against `full`.
    if let OpenAs::Member(k) = who {
        let (check, _) = identity(k, &plaintext);
        if !check.ct_eq(full) {
            return Err(ChunkError::ContentMismatch);
        }
    }
    Ok(plaintext)
}
