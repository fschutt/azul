//! The numbers of the content family, each written once: the chunk sizes of the pipeline, the
//! compression rule, the layout of a chunk blob, the pack sizes, the cap on a file's chunks, and the
//! table of every BLAKE3 `derive_key` context of the family (planning/architecture.md section 5.7;
//! section 1.4 rule 4: "Everything has a limit, stated up front"; build unit S14, lane W1-S14b).
//!
//! No later lane edits this file: the contexts of the pack and pointer lanes are declared here up
//! front, and a constant a later lane needs lives in its own module and is named in its report.
//!
//! | constant | value | where the plan says it |
//! |---|---|---|
//! | [`MIN_CHUNK`] | 256 KiB | 5.7 pipeline step 2: "minimum 256 KiB" |
//! | [`AVG_CHUNK`] | 1 MiB | "average 1 MiB" |
//! | [`MAX_CHUNK`] | 4 MiB | "maximum 4 MiB" (a 4 MiB chunk and a 4 MiB pack: DRIFT W1S14B-4, S14c's P-2) |
//! | [`COMPRESS_MIN_SAVING`] | 5 per cent | step 3: "per-chunk zstd when it saves 5 % (the existing rule)" |
//! | [`PACK_TARGET`] | 4 MiB | step 5: "packing into 4 MiB packs" |
//! | [`PACK_PAD_STEP`] | 256 KiB | 5.15 M4: "the last one padded to 256 KiB steps" |
//! | [`PACK_REPO_PAD`] | 4 KiB | 5.15 M4: "repository objects to 4 KiB" |
//!
//! [`NORMAL_CHUNK`] and [`MAX_FILE_CHUNKS`] are this lane's provisional numbers (DRIFT
//! X-DELTA-W1S14B-6 and -7): the first is where the chunker's strict mask gives way to the loose
//! one, the second the largest chunk count a file may declare.

use azlin_keys::Secret;
use blake3::Hasher;

use crate::units::{ChunkLen, PackLen, Percent};

// ---------------------------------------------------------------------------------------------
// The chunk sizes of the pipeline (planning/architecture.md section 5.7)
// ---------------------------------------------------------------------------------------------

/// The shortest chunk the chunker cuts, except the last chunk of a stream: 256 KiB. A file of at
/// most this many bytes is one chunk.
///
/// Caller: `Chunker` (INT-S14-1, INT-S14-2), S14c (the pack geometry).
pub const MIN_CHUNK: ChunkLen = ChunkLen::from_raw(256 * 1024);

/// The average chunk of the pipeline: 1 MiB. The chunker's normal size ([`NORMAL_CHUNK`]) is set so
/// that the expected chunk length is this number.
///
/// Caller: `Chunker`, the measurements of the test suite.
pub const AVG_CHUNK: ChunkLen = ChunkLen::from_raw(1024 * 1024);

/// The longest chunk the chunker cuts, and the longest plaintext `seal_chunk` accepts: 4 MiB.
///
/// Caller: `Chunker`, `seal_chunk`, `open_chunk`, `ChunkRef::check`.
pub const MAX_CHUNK: ChunkLen = ChunkLen::from_raw(4 * 1024 * 1024);

/// The chunk length at which the chunker's strict mask gives way to the loose one (FastCDC
/// normalized chunking): 848 KiB. DRIFT X-DELTA-W1S14B-6: the plan gives the three sizes and no
/// normal size; this one makes the expected chunk length 1.0046 MiB in the model of
/// `tests/chunker_distribution.rs`.
///
/// Caller: `Chunker`.
pub const NORMAL_CHUNK: ChunkLen = ChunkLen::from_raw(848 * 1024);

/// A chunk is stored compressed only when that saves at least this share of its length
/// (planning/architecture.md section 5.7: "per-chunk zstd when it saves 5 % (the existing rule)";
/// the rule of `azlin_compress::segment::worth_it`).
///
/// Caller: `CompressPolicy` (INT-S14-1).
pub const COMPRESS_MIN_SAVING: Percent = Percent::from_raw(5);

/// The most chunks one file may declare: 2^24, the bound under which a chunk list is decoded
/// (a file of 16 TiB at the average chunk, 4 TiB at the shortest). DRIFT X-DELTA-W1S14B-7: the
/// plan gives no cap (section 1.4 rule 4 asks for one).
///
/// Caller: S14d (the pointer and its chunk-list object), S14e1 (index limits).
pub const MAX_FILE_CHUNKS: u32 = 1 << 24;

// ---------------------------------------------------------------------------------------------
// The layout of a chunk blob (brief P-1: `codec (1) || nonce (24) || ciphertext || tag (16)`)
// ---------------------------------------------------------------------------------------------

/// The bytes of the codec at the start of a chunk blob.
///
/// Caller: `chunk::blob` (the layout), S14c (it stores blobs verbatim).
pub const BLOB_CODEC_LEN: usize = 1;

/// The bytes of the synthetic nonce of XChaCha20-Poly1305 in a chunk blob.
///
/// Caller: `chunk::blob`.
pub const BLOB_NONCE_LEN: usize = 24;

/// The bytes of the Poly1305 tag at the end of a chunk blob.
///
/// Caller: `chunk::blob`.
pub const BLOB_TAG_LEN: usize = 16;

/// What a chunk blob adds to its stored bytes: the codec, the nonce and the tag (41 bytes).
///
/// Caller: `chunk::blob`, S14c (padding arithmetic).
pub const BLOB_OVERHEAD: usize = BLOB_CODEC_LEN + BLOB_NONCE_LEN + BLOB_TAG_LEN;

/// The largest blob in bytes: a stored chunk of [`MAX_CHUNK`] bytes and the framing.
pub(crate) const MAX_BLOB_BYTES: u32 = 4 * 1024 * 1024 + 41;

/// The largest chunk blob a pack stores: [`MAX_CHUNK`] stored bytes and [`BLOB_OVERHEAD`]. Every
/// `stored_len` is at most this (stored bytes are never longer than the plaintext: a chunk that
/// does not compress is stored as it is).
///
/// Caller: `ChunkLen::new` (its bound), `SealedChunk::from_stored`, S14c (`hard_max`).
pub const MAX_BLOB: ChunkLen = ChunkLen::from_raw(MAX_BLOB_BYTES);

// ---------------------------------------------------------------------------------------------
// The pack sizes (planning/architecture.md sections 5.7 and 5.15 M4)
// ---------------------------------------------------------------------------------------------

/// The size a pack is filled to: 4 MiB, one stripe (section 2.12).
///
/// Caller: S14c (`pack::TARGET` is this value).
pub const PACK_TARGET: PackLen = PackLen::new(4 * 1024 * 1024);

/// The steps the last pack of an upload is padded to under the `buckets` policy: 256 KiB.
///
/// Caller: S14c (`pack::PAD_STEP` is this value).
pub const PACK_PAD_STEP: PackLen = PackLen::new(256 * 1024);

/// The step a repository-class pack is padded to: 4 KiB.
///
/// Caller: S14c (`pack::REPO_PAD` is this value).
pub const PACK_REPO_PAD: PackLen = PackLen::new(4 * 1024);

// ---------------------------------------------------------------------------------------------
// The registry of derive contexts of the content family
// ---------------------------------------------------------------------------------------------

/// One BLAKE3 `derive_key` context string of the content family, with the name of its constant.
/// Its constructor is crate-private, so a context exists only because it is written in the
/// `contexts!` table below, and the table and [`CONTEXTS`] cannot drift apart. The keys family
/// (`azlin_keys::derive::CONTEXTS`) has its own table; the test `c20_contexts_are_disjoint_from_azlin_keys`
/// holds the two apart. A context is never edited: a changed purpose is a new context with the next
/// `v<N>`.
///
/// Caller: `Full::chunk_id`, `Full::chunk_key` (the chunk contexts), `GearTable` (the gear
/// contexts), S14c (`K_PACK_TRAILER`, `PACK_PADDING`), S14d (`LIST_SEAL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentContext {
    name: &'static str,
    text: &'static str,
}

impl ContentContext {
    /// A context. Crate-private: a context is an entry of the table below, nothing else.
    const fn new(name: &'static str, text: &'static str) -> ContentContext {
        ContentContext { name, text }
    }

    /// The context string handed to BLAKE3.
    pub const fn as_str(&self) -> &'static str {
        self.text
    }

    /// The name of the constant (`CHUNK_ID`), for tables and known-answer files.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The key derived from `key` under this context: BLAKE3 `derive_key`, one key for one
    /// purpose. The 32 bytes are written straight into the returned [`Secret`], so no unwiped
    /// copy of the key is made on the way (the hasher's own state is not wiped: DRIFT
    /// X-DELTA-W1S14B-9, the manifest has no `zeroize` feature of blake3).
    ///
    /// Caller: `Full::chunk_key` (the chunk key), S14c (the trailer key and the padding key of a
    /// pack).
    pub fn derive(&self, key: &Secret<32>) -> Secret<32> {
        let mut hasher = Hasher::new_derive_key(self.text);
        hasher.update(key.expose());
        let mut derived = Secret::from_bytes([0u8; 32]);
        hasher.finalize_xof().fill(derived.expose_mut());
        derived
    }

    /// The 32 bytes BLAKE3 `derive_key` gives for public `material` under this context: an id, never
    /// a key.
    ///
    /// Caller: S14d (the name of a chunk-list object), S14c (ids derived from public bytes).
    pub fn derive_public(&self, material: &[u8]) -> [u8; 32] {
        blake3::derive_key(self.text, material)
    }
}

/// Declares the contexts: one constant each, and `CONTEXTS` listing every one in this order.
macro_rules! contexts {
    ($( $(#[$meta:meta])* $name:ident = $text:literal; )+) => {
        $(
            $(#[$meta])*
            pub const $name: ContentContext = ContentContext::new(stringify!($name), $text);
        )+

        /// Every derive context of the content family, in the order of the table.
        ///
        /// Caller: the test `c20_contexts_are_disjoint_from_azlin_keys`, INT-WIRE (the contract
        /// fixtures).
        pub const CONTEXTS: &[ContentContext] = &[$( $name ),+];
    };
}

contexts! {
    /// The context of a chunk id: `chunk_id = derive_key("azlin chunk id v1", full)[0..16]`.
    /// The string is the plan's own (planning/architecture.md section 5.7 "Chunk (AZL2)").
    ///
    /// Caller: `Full::chunk_id`.
    CHUNK_ID = "azlin chunk id v1";

    /// The context of a chunk key: `chunk_key = derive_key("azlin chunk key v1", full)`. A separate
    /// context from [`CHUNK_ID`], so the id reveals nothing of the key (section 5.7). The string is
    /// the plan's own.
    ///
    /// Caller: `Full::chunk_key`.
    CHUNK_KEY = "azlin chunk key v1";

    /// The context of the public gear table of the chunker (brief P-2): the XOF of this context,
    /// the same for every space.
    ///
    /// Caller: `GearTable::public_v1`.
    GEAR_PUBLIC = "azlin 2026-11 fastcdc gear v1";

    /// The context of the keyed gear table of the chunker (brief P-3, decision X-D-43): the XOF of
    /// this context over `K_chunk`, so the cut points of a space are not those of any other.
    ///
    /// Caller: `GearTable::keyed`.
    GEAR_KEYED = "azlin 2026-11 fastcdc gear keyed v1";

    /// The key a pack's trailer is sealed under, derived from `K_seal` (planning/architecture.md
    /// section 5.7 "Pack (AZP2)": "a trailer sealed with `K_seal`"); lane W1-S14c.
    ///
    /// Caller: S14c (`SealedPack` and `read_trailer`).
    K_PACK_TRAILER = "azlin 2026-11 pack trailer key v1";

    /// The key of the keystream that pads a pack (so the padding passes the node's randomness
    /// test and a resent pack hashes the same); lane W1-S14c.
    ///
    /// Caller: S14c (the padding of a pack).
    PACK_PADDING = "azlin 2026-11 pack padding key v1";

    /// The context under which the chunk-list object of a huge file is sealed (planning/architecture.md
    /// section 5.7 "or a chunk-list object for huge files"); lane W1-S14d.
    ///
    /// Caller: S14d (`encode_chunk_list`, `decode_chunk_list`).
    LIST_SEAL = "azlin 2026-11 chunk list seal v1";
}
