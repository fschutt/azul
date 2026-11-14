//! The shared vocabulary of the content family: the ids and names of chunks and packs, the secret
//! hash of a chunk, the pack classes, the blob a pack stores and the three records the later lanes
//! pass between them (planning/architecture.md section 5.7; build unit S14, lane W1-S14b).
//!
//! | type | what | secret? |
//! |---|---|---|
//! | [`ChunkId`] | 16 bytes: `derive_key("azlin chunk id v1", full)[0..16]` | no |
//! | [`Full`] | 32 bytes: `keyed-BLAKE3(K_chunk, plaintext)`, the chunk's secret name and the input of its key | YES: key material |
//! | [`PackId`], [`PackName`] | 16 and 32 bytes: the id of a pack and its name between devices | no |
//! | [`PackClass`] | `content`, `derived`, `repository` | no |
//! | [`SealedChunk`] | the blob a pack stores: `codec (1) \|\| nonce (24) \|\| ciphertext \|\| tag (16)` | no (ciphertext) |
//! | [`PackEntry`] | one line of a pack trailer: `(chunk_id, offset, length)` | no |
//! | [`PackSummary`] | what the chunk index needs of a pack | no |
//! | [`ChunkRef`] | one `chunks` entry of pointer v2: `(full, raw length, pack, offset, stored length)` | YES: holds a [`Full`] |
//!
//! The ids print and parse as lowercase hex (32 digits for a chunk or pack id, 64 for a name),
//! strictly: no uppercase, no whitespace, no other length. A [`Full`] holds key material, so it is
//! wiped when dropped, prints as `Full(***)`, compares in constant time, and has no `Clone`: a second
//! copy is [`Full::duplicate`].
//!
//! ```
//! use azlin_content::{ChunkId, Full};
//!
//! let id = ChunkId::from_bytes([0xab; 16]);
//! assert_eq!(id.to_string(), "abababababababababababababababab");
//! assert_eq!(id.to_string().parse::<ChunkId>(), Ok(id));
//!
//! let full = Full::from_bytes([7; 32]);
//! assert_eq!(format!("{full:?}"), "Full(***)");
//! assert!(full == full.duplicate());
//! ```
//!
//! There is no `Clone`, `Display` or `Default` for a [`Full`]; each of these does not compile:
//!
//! ```compile_fail
//! use azlin_content::Full;
//! let a = Full::from_bytes([7; 32]);
//! let _b = a.clone();
//! ```
//!
//! ```compile_fail
//! use azlin_content::Full;
//! let a = Full::from_bytes([7; 32]);
//! let _text = format!("{a}");
//! ```
//!
//! ```compile_fail
//! use azlin_content::Full;
//! let _a = Full::default();
//! ```

use core::fmt;
use core::str::FromStr;

use azlin_keys::{IdError, Secret};
use blake3::Hasher;

use crate::error::ChunkError;
use crate::limits::{BLOB_OVERHEAD, CHUNK_ID, MAX_CHUNK};
use crate::units::{ChunkLen, PackLen, PackOffset};

/// Writes `bytes` as lowercase hex (two digits per byte).
fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

/// The value of a lowercase hex digit; `None` for anything else (uppercase included).
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Strict hex: exactly `2 * N` lowercase hex digits, nothing else.
fn parse_hex<const N: usize>(text: &str) -> Result<[u8; N], IdError> {
    let digits = text.as_bytes();
    if digits.len() != N.saturating_mul(2) {
        return Err(IdError::Length);
    }
    let mut out = [0u8; N];
    for (byte, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
        let (high, low) = match pair {
            [high, low] => (nibble(*high), nibble(*low)),
            _ => return Err(IdError::Length),
        };
        match (high, low) {
            (Some(high), Some(low)) => *byte = (high << 4) | low,
            _ => return Err(IdError::BadDigit),
        }
    }
    Ok(out)
}

/// One plain byte-array id type: bytes, lowercase hex `Display`, strict `FromStr`, ordering and
/// hashing (the index keys its maps by them), no `Default`.
macro_rules! bytes_id {
    ($(#[$meta:meta])* $name:ident, $len:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $len]);

        impl $name {
            /// The number of bytes of the id.
            pub const LEN: usize = $len;

            /// The id with these bytes.
            pub const fn from_bytes(bytes: [u8; $len]) -> $name {
                $name(bytes)
            }

            /// The bytes of the id.
            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }
        }

        /// Lowercase hex, two digits per byte.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(f, &self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self)
            }
        }

        /// Strict: exactly twice as many lowercase hex digits as bytes.
        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<$name, IdError> {
                parse_hex::<$len>(text).map($name)
            }
        }
    };
}

bytes_id! {
    /// The id of a chunk inside one space: `derive_key("azlin chunk id v1", full)[0..16]`
    /// (planning/architecture.md section 5.7). It differs between spaces (the `full` it comes from
    /// is keyed by the space's `K_chunk`) and reveals nothing of the chunk key (a separate
    /// context). It is in the associated data of the chunk, in pack trailers and in the chunk index.
    ///
    /// Caller: every content lane (S14c trailers, S14e1 the index, S14e2 live sets, INT-S14-1 the
    /// upload pipeline).
    ChunkId, 16
}

bytes_id! {
    /// The id of a pack: 16 bytes, the first half of its [`PackName`] (S14c's P-3), the part of its
    /// storage key `s/<space>/p/<2 hex>/<32 hex>`.
    ///
    /// Caller: S14c (`SealedPack`), S14d (`ChunkRef::pack`), S14e1 (`PackInfo`), S14e2 (the prune).
    PackId, 16
}

bytes_id! {
    /// The name of a pack between devices: the BLAKE3 hash of its ciphertext (planning/architecture.md
    /// section 5.7 "The chunk index ... plus each pack's ciphertext BLAKE3 (the same hash the store
    /// keeps as the stripe's, and the pack's name between devices, 5.13)").
    ///
    /// Caller: S14c (`pack_name`), S14e1 (`PackInfo::name`).
    PackName, 32
}

/// The secret hash of a chunk: `full = keyed-BLAKE3(K_chunk, plaintext)` (planning/architecture.md
/// section 5.7 "Chunk (AZL2)"). It is the chunk's name inside a pointer and the input of the chunk's
/// id and key, so it is KEY MATERIAL: whoever holds it can open the chunk (a share carries it, a
/// pointer lists it). A `Full` is wiped when dropped, prints as `Full(***)`, compares in constant
/// time and has no `Clone`, `Display` or `Default`; a second copy is [`Full::duplicate`], a
/// greppable call.
///
/// Caller: `identity`, `seal_chunk`, `open_chunk`, S14d (the pointer's chunk list: decoded with
/// `azlin_cbor::decode_secret`, R17), S16 (a share carries it).
pub struct Full(Secret<32>);

impl Full {
    /// The number of bytes of a `full` hash.
    pub const LEN: usize = 32;

    /// A `Full` holding this secret.
    pub fn from_secret(secret: Secret<32>) -> Full {
        Full(secret)
    }

    /// A `Full` holding these bytes. The caller's own array is not wiped by this call: prefer
    /// [`Full::from_slice`] where the bytes are decoded.
    pub fn from_bytes(bytes: [u8; 32]) -> Full {
        Full(Secret::from_bytes(bytes))
    }

    /// A `Full` from a slice of exactly 32 bytes; `None` for any other length.
    pub fn from_slice(bytes: &[u8]) -> Option<Full> {
        Secret::from_slice(bytes).map(Full)
    }

    /// The bytes, for the one place that needs them (writing a pointer, a share). Never pass them
    /// to a log or an error.
    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose()
    }

    /// Whether the two hashes are equal, in constant time.
    pub fn ct_eq(&self, other: &Full) -> bool {
        self.0.ct_eq(&other.0)
    }

    /// A second owner of the same bytes: the only way to copy a `Full`.
    pub fn duplicate(&self) -> Full {
        Full(self.0.duplicate())
    }

    /// The id of this chunk: `derive_key("azlin chunk id v1", full)[0..16]`.
    pub fn chunk_id(&self) -> ChunkId {
        let mut hasher = Hasher::new_derive_key(CHUNK_ID.as_str());
        hasher.update(self.0.expose());
        let mut id = [0u8; ChunkId::LEN];
        hasher.finalize_xof().fill(&mut id);
        ChunkId(id)
    }
}

/// Equality in constant time ([`Full::ct_eq`]); it lets a structure that holds a `Full` derive
/// `PartialEq` (DRIFT X-DELTA-W1S14B-10).
impl PartialEq for Full {
    fn eq(&self, other: &Full) -> bool {
        self.ct_eq(other)
    }
}

impl Eq for Full {}

/// Prints `Full(***)`: never a byte.
impl fmt::Debug for Full {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Full(***)")
    }
}

/// The class of a pack (planning/architecture.md section 5.7 "Pack classes: `content`, `derived`,
/// `repository`"): what its chunks are. The class byte is in the pack header.
///
/// Caller: S14c (the header and the builder), S14e1 (`PackInfo::class`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PackClass {
    /// File content: the chunks of files (class byte 0).
    Content = 0,
    /// Derived records: previews, extracts, embeddings (class byte 1).
    Derived = 1,
    /// The repository's own objects (class byte 2).
    Repository = 2,
}

impl PackClass {
    /// The three classes, in the order of their class bytes.
    pub const ALL: [PackClass; 3] = [
        PackClass::Content,
        PackClass::Derived,
        PackClass::Repository,
    ];

    /// The class byte.
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// The class of a class byte; `None` for a byte no class uses.
    pub const fn from_byte(byte: u8) -> Option<PackClass> {
        match byte {
            0 => Some(PackClass::Content),
            1 => Some(PackClass::Derived),
            2 => Some(PackClass::Repository),
            _ => None,
        }
    }
}

/// The blob a pack stores for one chunk: `codec (1) || nonce (24) || ciphertext || tag (16)`, made by
/// `seal_chunk` (the layout is brief P-1, kept behind `chunk::blob`). Its bytes are ciphertext: public
/// data, so it can be cloned and compared freely. A pack copies it verbatim and never edits it.
///
/// A `SealedChunk` exists only with a length from the framing's 41 bytes up to the largest blob
/// ([`crate::limits::MAX_BLOB`]); whether it authenticates is `open_chunk`'s question.
///
/// Caller: INT-S14-1 (the upload pipeline), S14c (`PackBuilder::add`, `repack`).
#[derive(Clone, PartialEq, Eq)]
pub struct SealedChunk {
    bytes: Vec<u8>,
    len: ChunkLen,
}

impl SealedChunk {
    /// A blob from owned bytes, shape-checked: at least the framing and at most the largest blob.
    pub(crate) fn from_vec(bytes: Vec<u8>) -> Result<SealedChunk, ChunkError> {
        if bytes.len() < BLOB_OVERHEAD {
            return Err(ChunkError::Truncated);
        }
        match ChunkLen::from_len(bytes.len()) {
            Some(len) => Ok(SealedChunk { bytes, len }),
            None => Err(ChunkError::TooLong),
        }
    }

    /// A blob read back from a pack (a ranged read of one entry, or the bytes of a repack),
    /// shape-checked: [`ChunkError::Truncated`] below the framing's 41 bytes,
    /// [`ChunkError::TooLong`] above the largest blob. The codec byte is not interpreted and nothing
    /// is authenticated: that is `open_chunk`'s. DRIFT X-DELTA-W1S14B-8: the brief's
    /// `from_blob_unchecked` under a `test-support` feature the final manifest does not have.
    ///
    /// Caller: S14c (`repack` and the tests of the builder), INT-S14-1 (a blob fetched from a pack).
    pub fn from_stored(bytes: &[u8]) -> Result<SealedChunk, ChunkError> {
        SealedChunk::from_vec(bytes.to_vec())
    }

    /// The bytes of the blob.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The blob's bytes, by value.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The length of the blob as a pack length: what it adds to a pack.
    pub fn len(&self) -> PackLen {
        PackLen::new(self.len.get())
    }

    /// The length of the blob as a chunk length: the `stored_len` of its pointer entry and the
    /// `length` of its pack entry.
    pub fn stored_len(&self) -> ChunkLen {
        self.len
    }
}

/// Prints the length, never the bytes.
impl fmt::Debug for SealedChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SealedChunk({} bytes)", self.bytes.len())
    }
}

/// One line of a pack trailer (planning/architecture.md section 5.7: "a trailer sealed with `K_seal`
/// listing `(chunk_id, offset, length)`"): where the blob of one chunk sits inside the pack. Defined
/// here, in the skeleton, so that the index and the prune compile against it; S14c re-exports it from
/// `pack`.
///
/// Caller: S14c (the trailer, `rebuild_entries`), S14e1 (`rebuild_from_trailers`), S14e2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PackEntry {
    /// The chunk the blob holds.
    pub id: ChunkId,
    /// Where the blob starts, from the first byte of the pack.
    pub offset: PackOffset,
    /// The length of the blob (the chunk's `stored_len`).
    pub length: ChunkLen,
}

impl PackEntry {
    /// The offset just past the blob; `None` on overflow.
    pub fn end(&self) -> Option<PackOffset> {
        self.offset.checked_add(self.length)
    }
}

/// What the chunk index needs of a finished pack: S14c's `SealedPack::summary()` produces it, S14e1's
/// `ChunkIndex::add_pack` takes it.
///
/// Caller: S14c (`SealedPack::summary`), S14e1 (`add_pack`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PackSummary {
    /// The id of the pack.
    pub id: PackId,
    /// The BLAKE3 hash of the pack's ciphertext.
    pub name: PackName,
    /// The length of the pack in bytes.
    pub len: PackLen,
    /// The class of the pack.
    pub class: PackClass,
    /// The chunks the pack holds, in the order of its trailer.
    pub entries: Vec<PackEntry>,
}

/// One `chunks` entry of a pointer v2 (planning/architecture.md section 5.7 "Pointer v2": `(full
/// hash, raw length, pack, offset, stored length)`): where one chunk of a file sits and how long it is
/// before and after compression. It holds a [`Full`], so it is key material: no `Clone`, a redacted
/// `Debug`, a second copy is [`ChunkRef::duplicate`].
///
/// Caller: S14d (the pointer and the chunk-list object), INT-S14-1 (built from a `ChunkMeta` after
/// packing), S14e2 (the live set).
#[derive(Debug, PartialEq, Eq)]
pub struct ChunkRef {
    /// The chunk's secret hash.
    pub full: Full,
    /// The length of the plaintext.
    pub raw_len: ChunkLen,
    /// The pack that holds the blob.
    pub pack: PackId,
    /// Where the blob starts in the pack.
    pub offset: PackOffset,
    /// The length of the blob in the pack.
    pub stored_len: ChunkLen,
}

impl ChunkRef {
    /// A second owner of the same reference: the only way to copy one.
    pub fn duplicate(&self) -> ChunkRef {
        ChunkRef {
            full: self.full.duplicate(),
            raw_len: self.raw_len,
            pack: self.pack,
            offset: self.offset,
            stored_len: self.stored_len,
        }
    }

    /// The id of the chunk.
    pub fn id(&self) -> ChunkId {
        self.full.chunk_id()
    }

    /// The offset just past the blob; `None` on overflow.
    pub fn end(&self) -> Option<PackOffset> {
        self.offset.checked_add(self.stored_len)
    }

    /// Whether the numbers can describe a chunk at all: the plaintext is at most
    /// [`MAX_CHUNK`] ([`ChunkError::TooLong`]), the blob holds its framing and at least one stored
    /// byte ([`ChunkError::Truncated`]), the stored bytes are not longer than the plaintext
    /// ([`ChunkError::LengthMismatch`]: a chunk that does not compress is stored as it is) and the blob
    /// ends inside a `u32` ([`ChunkError::TooLong`]). Nothing here says the blob exists.
    pub fn check(&self) -> Result<(), ChunkError> {
        if self.raw_len > MAX_CHUNK {
            return Err(ChunkError::TooLong);
        }
        let stored = self.stored_len.as_usize();
        if stored <= BLOB_OVERHEAD {
            return Err(ChunkError::Truncated);
        }
        if stored > self.raw_len.as_usize().saturating_add(BLOB_OVERHEAD) {
            return Err(ChunkError::LengthMismatch);
        }
        if self.end().is_none() {
            return Err(ChunkError::TooLong);
        }
        Ok(())
    }
}
