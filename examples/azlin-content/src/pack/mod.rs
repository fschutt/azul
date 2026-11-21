//! AZP2, the pack format: a short header, the chunk blobs back to back, padding, and a trailer sealed
//! with `K_seal` that lists `(chunk_id, offset, length)` (planning/architecture.md section 5.7 "Pack
//! (AZP2)"; section 2.5 "Nodes check what they can"; section 5.15 M4 "4 MiB packs, the last one padded
//! to 256 KiB steps; repository objects to 4 KiB; policy `padding` = none, buckets (the default) or
//! full"; build unit S14, lane W1-S14c). The server sees an opaque object of about 4 MiB, and a lost
//! chunk index can be rebuilt from the trailers.
//!
//! ```text
//!  0     24         the header (header.rs): "AZP2", version 1, class, reserved 0,
//!                    key_epoch, chunk_count, body_len, pad_len (little-endian)
//! 24     body_len   the chunk blobs, back to back, each exactly as seal_chunk made it
//!                    then pad_len bytes of padding (padding.rs): a keystream, never zeros
//!                    then the trailer to the end (trailer.rs):
//!                      nonce (24) || XChaCha20-Poly1305(K_PACK_TRAILER.derive(K_seal),
//!                        CBOR { 1: [[chunk_id, offset, length], ...], 2: BLAKE3(body) }) || tag (16)
//! ```
//!
//! | file | what |
//! |---|---|
//! | `header.rs` | [`PackHeader`]: parse, serialise, the ranges of the trailer and the body |
//! | `trailer.rs` | the sealed list of lines: sealing, opening, the exact sizes (crate-private) |
//! | `padding.rs` | the padding policy and the padding bytes (crate-private) |
//! | `builder.rs` | [`PackBuilder`], [`AddOutcome`], [`SealedPack`] |
//! | `reader.rs` | [`read_trailer`], [`chunk_range`], [`rebuild_entries`] |
//! | `repack.rs` | [`repack()`], [`repack_with()`]: new packs from existing blobs, verbatim |
//! | `sealed_check.rs` | the node's key-less checks: [`check_header`], [`randomness_check`], [`check_new_pack`] |
//! | `names.rs` | [`pack_name`], [`pack_id`], [`storage_key`] |
//!
//! The pack sizes ([`TARGET`], [`PAD_STEP`], [`REPO_PAD`]) are the numbers of `limits.rs`
//! (DRIFT X-DELTA-W1S14B-12); [`HARD_MAX`] and [`MAX_CHUNKS`] are this lane's (DRIFT
//! X-DELTA-W1S14C-1 and -11): a 4 MiB chunk and the framing do not fit a 4 MiB pack, so a chunk that
//! does not fit gets a pack of its own up to `HARD_MAX`, and no reader accepts a pack above it or one
//! with more than `MAX_CHUNKS` chunks.
//!
//! ```
//! use azlin_content::pack::{rebuild_entries, AddOutcome, PackBuilder, PackGeometry, Padding, SealKey};
//! use azlin_content::{seal_chunk, CompressPolicy, KChunk, PackClass, StoredOnly};
//! use azlin_keys::{Epoch, Secret, SeededEntropy, SpaceId};
//!
//! let space = SpaceId::from_bytes([7; 16]);
//! let k_chunk = KChunk::from_secret(Secret::from_bytes([1; 32]));
//! let seal = || SealKey { epoch: Epoch::FIRST, key: Secret::from_bytes([2; 32]) };
//!
//! let mut builder =
//!     PackBuilder::new(space, PackClass::Content, seal(), PackGeometry::default(), Padding::None);
//! let (meta, blob) =
//!     seal_chunk(&space, &k_chunk, b"one chunk of one file", &StoredOnly, CompressPolicy::plan())
//!         .unwrap();
//! assert_eq!(builder.add(meta.id, &blob).unwrap(), AddOutcome::Fits);
//! let pack = builder.finish(true, &mut SeededEntropy::from_u64(1)).unwrap();
//!
//! // The trailer lists where the blob sits; the lines are rebuilt from the bytes of the pack alone.
//! assert_eq!(rebuild_entries(pack.bytes(), &space, &seal()).unwrap(), pack.entries());
//! ```

use core::fmt;

use azlin_keys::{EntropyError, Epoch, Secret};

use crate::error::ChunkError;
use crate::limits::{PACK_PAD_STEP, PACK_REPO_PAD, PACK_TARGET};
use crate::units::PackLen;

pub mod builder;
pub mod header;
pub mod names;
mod padding;
pub mod reader;
pub mod repack;
pub mod sealed_check;
mod trailer;

pub use crate::ids::PackEntry;
pub use builder::{AddOutcome, PackBuilder, SealedPack};
pub use header::PackHeader;
pub use names::{pack_id, pack_name, storage_key};
pub use reader::{chunk_range, read_trailer, rebuild_entries};
pub use repack::{repack, repack_with};
pub use sealed_check::{
    check_header, check_new_pack, randomness_check, NotSealed, CHI_SQUARE_MAX, MIN_SAMPLE,
    SAMPLE_LEN,
};

/// The size a pack is filled to: 4 MiB, one stripe (planning/architecture.md section 5.7 step 5, section
/// 5.15 M4). The value is `limits::PACK_TARGET` (DRIFT X-DELTA-W1S14B-12).
///
/// Caller: INT-S14-1 and INT-S14-2 (the upload pipeline's geometry), `PackGeometry::default`.
pub const TARGET: PackLen = PACK_TARGET;

/// The steps the last pack of an upload is padded to under `Padding::Buckets`: 256 KiB (section 5.15
/// M4). The value is `limits::PACK_PAD_STEP`.
///
/// Caller: `PackGeometry::default`.
pub const PAD_STEP: PackLen = PACK_PAD_STEP;

/// The step a repository-class pack is padded to under `Padding::Buckets`: 4 KiB (section 5.15 M4
/// "repository objects to 4 KiB"). The value is `limits::PACK_REPO_PAD`.
///
/// Caller: the padding of a repository pack (`builder.rs`), INT-S14-1.
pub const REPO_PAD: PackLen = PACK_REPO_PAD;

/// The longest pack there is: 4 MiB and 64 KiB (brief P-2, DRIFT X-DELTA-W1S14C-1). A chunk of 4 MiB
/// and the framing of a pack (the header, the blob's own 41 bytes, the trailer) do not fit one 4 MiB
/// pack, so such a chunk gets a pack of its own, which spans two stripes (section 2.12 allows objects
/// of several stripes). Neither the builder nor the readers accept a pack above it.
///
/// Caller: `PackGeometry::default`, `PackHeader::parse` (the reader's limit), INT-S14-4 (the node's
/// limit of a `pack_put`).
pub const HARD_MAX: PackLen = PackLen::new(4 * 1024 * 1024 + 64 * 1024);

/// The most chunks a pack holds: 16,384. It is the default of `PackGeometry::max_chunks`, the largest
/// value that field may have, and the most a reader accepts in a header or a trailer (section 1.4
/// rule 4: everything has a limit, stated up front; DRIFT X-DELTA-W1S14C-11).
///
/// Caller: `PackGeometry`, `PackHeader::parse`, the trailer's decoder.
pub const MAX_CHUNKS: u32 = 16_384;

/// The padding policy of an account or a space (planning/architecture.md section 5.15 M4: policy
/// `padding` = none, buckets (the default) or full). The rules are the table in `padding.rs`
/// (DRIFT X-DELTA-W1S14C-9).
///
/// Caller: INT-S14-1 (the policy `padding` of the space, read from the account's policy, 5.10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Padding {
    /// No padding: the pack has the size its chunks make.
    None,
    /// The last pack of an upload is padded to the next bucket: `pad_step` (256 KiB), or `REPO_PAD`
    /// (4 KiB) for a repository pack. The default.
    #[default]
    Buckets,
    /// The last pack of an upload is padded to `target` (4 MiB): every pack the node sees but the
    /// ones that were closed because they were full has the same size. DRIFT X-DELTA-W1S14C-9: the
    /// brief's P-5 says "every pack" and C7 says a pack finished with `last_of_batch = false` is
    /// never padded; the second is taken.
    Full,
}

/// The sizes a pack builder works to (brief section 4): `target` is the size a pack is filled to,
/// `hard_max` the most a lone oversize chunk's pack may have, `pad_step` the step of the buckets and
/// `max_chunks` the most lines a trailer has.
///
/// Caller: INT-S14-1 (the default, or a smaller one for repository packs), `repack`, the tests (small
/// geometries make small packs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackGeometry {
    /// The size a pack is filled to: [`TARGET`] by default.
    pub target: PackLen,
    /// The most a pack may have: [`HARD_MAX`] by default. Only a lone chunk that does not fit within
    /// `target` makes a pack longer than `target`.
    pub hard_max: PackLen,
    /// The step of the buckets: [`PAD_STEP`] by default.
    pub pad_step: PackLen,
    /// The most chunks in a pack: [`MAX_CHUNKS`] by default and at most.
    pub max_chunks: u32,
}

impl PackGeometry {
    /// 4 MiB, 4 MiB and 64 KiB, 256 KiB, 16,384.
    ///
    /// Caller: `Default`, INT-S14-1.
    pub const DEFAULT: PackGeometry = PackGeometry {
        target: TARGET,
        hard_max: HARD_MAX,
        pad_step: PAD_STEP,
        max_chunks: MAX_CHUNKS,
    };

    /// Whether the numbers can make a pack at all: a step and a target above zero, `hard_max` at least
    /// `target` and at most [`HARD_MAX`], and `max_chunks` from 1 to [`MAX_CHUNKS`]; otherwise
    /// [`PackError::BadGeometry`]. The builder checks it at every `add` and `finish`.
    ///
    /// Caller: `PackBuilder` (checks it), `repack`, INT-S14-1 (checks a geometry it was configured with).
    pub fn validate(&self) -> Result<(), PackError> {
        let sound = self.pad_step.get() > 0
            && self.target.get() > 0
            && self.hard_max >= self.target
            && self.hard_max <= HARD_MAX
            && self.max_chunks >= 1
            && self.max_chunks <= MAX_CHUNKS;
        if sound {
            Ok(())
        } else {
            Err(PackError::BadGeometry)
        }
    }
}

impl Default for PackGeometry {
    fn default() -> PackGeometry {
        PackGeometry::DEFAULT
    }
}

/// `K_seal` and the epoch that names it: the key a pack's trailer is sealed with, and the epoch the
/// header records so that a reader after a rotation picks the right key (brief P-1). The key is key
/// material: no `Clone`, a redacted `Debug`, wiped on drop; a second owner is [`SealKey::duplicate`].
///
/// Caller: INT-S14-1 and INT-S14-3 (built from the space key's `K_seal` and epoch, S15c1), S14e2
/// (the prune repacks with it), the tests.
#[derive(Debug)]
pub struct SealKey {
    /// The epoch of the space key that `key` is derived from.
    pub epoch: Epoch,
    /// `K_seal` of that epoch.
    pub key: Secret<32>,
}

impl SealKey {
    /// A second owner of the same key: the only way to copy a `SealKey`.
    ///
    /// Caller: `repack` (every pack it builds has its own builder), INT-S14-1 (a builder per pack).
    pub fn duplicate(&self) -> SealKey {
        SealKey {
            epoch: self.epoch,
            key: self.key.duplicate(),
        }
    }
}

/// Why a pack could not be built, read or opened. One small `Copy` enum, no strings, no data that
/// could carry a secret: a caller maps each to its own status.
///
/// Caller: INT-S14-1 to INT-S14-3 (the upload and download pipelines map each to a status and a
/// retry), S14e1 and S14e2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackError {
    /// Fewer than 24 bytes where a header is read.
    TooShort,
    /// The first four bytes are not `AZP2`.
    BadMagic,
    /// The version byte is not 1: a newer pack is refused by name and never parsed.
    UnsupportedVersion(u8),
    /// The class byte is none of 0, 1, 2.
    BadClass(u8),
    /// The two reserved bytes are not zero.
    ReservedNotZero,
    /// The key epoch of the header is 0: epochs start at 1.
    BadEpoch,
    /// The header says no chunks: a pack has at least one.
    NoChunks,
    /// More chunks than [`MAX_CHUNKS`].
    TooManyChunks,
    /// The pack is longer than [`HARD_MAX`].
    TooLong,
    /// The sizes of the header do not fit each other or the declared length of the pack: the body
    /// cannot hold the chunks, the padding and the trailer do not fill the rest.
    SizesInconsistent,
    /// `finish` of a builder that holds no blob.
    EmptyPack,
    /// A chunk id twice in one pack.
    DuplicateChunk,
    /// A [`PackGeometry`] that cannot make a pack.
    BadGeometry,
    /// A blob that does not fit a pack even alone, within `hard_max`.
    TooBigForAnyPack,
    /// The trailer does not authenticate: a wrong `K_seal`, space or epoch, a trailer of another pack,
    /// or a flipped bit in the header, the body, the padding or the trailer. One word for all of them.
    Auth,
    /// The trailer authenticates but is not what a builder writes: not the format, a count that is
    /// not the header's, lines that are not back to back, a blob shorter than its framing.
    BadTrailer,
    /// A line of the trailer, or of a pointer, that does not lie inside the body of its pack.
    EntryOutsideBody,
    /// The bytes of a blob are not as long as the line that describes them.
    LengthMismatch,
    /// A blob that is no chunk blob (shorter than its framing, or longer than the largest blob).
    Chunk(ChunkError),
    /// The entropy source gave no nonce: no pack is made.
    Entropy(EntropyError),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::TooShort => f.write_str("fewer than 24 bytes for a pack header"),
            PackError::BadMagic => f.write_str("the pack does not start with the AZP2 magic"),
            PackError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "the pack is of version {v}, which this build does not read"
                )
            }
            PackError::BadClass(c) => write!(f, "the pack's class byte {c} is no class"),
            PackError::ReservedNotZero => {
                f.write_str("the reserved bytes of the header are not zero")
            }
            PackError::BadEpoch => f.write_str("the key epoch of the header is 0"),
            PackError::NoChunks => f.write_str("the header says the pack holds no chunk"),
            PackError::TooManyChunks => f.write_str("the pack holds more chunks than the limit"),
            PackError::TooLong => f.write_str("the pack is longer than the limit"),
            PackError::SizesInconsistent => {
                f.write_str("the sizes in the header do not fit each other or the pack")
            }
            PackError::EmptyPack => f.write_str("a pack needs at least one chunk"),
            PackError::DuplicateChunk => f.write_str("a chunk twice in one pack"),
            PackError::BadGeometry => f.write_str("the pack geometry cannot make a pack"),
            PackError::TooBigForAnyPack => f.write_str("the blob does not fit any pack"),
            PackError::Auth => f.write_str("the pack does not authenticate"),
            PackError::BadTrailer => f.write_str("the trailer is not what a builder writes"),
            PackError::EntryOutsideBody => f.write_str("the entry lies outside the pack's body"),
            PackError::LengthMismatch => f.write_str("the blob is not as long as its entry says"),
            PackError::Chunk(e) => fmt::Display::fmt(e, f),
            PackError::Entropy(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for PackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PackError::Chunk(e) => Some(e),
            PackError::Entropy(e) => Some(e),
            _ => None,
        }
    }
}
