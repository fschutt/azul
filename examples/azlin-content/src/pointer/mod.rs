//! The signed pointer v2: one per file version, in the repository, naming the file's size, times,
//! mode, plaintext BLAKE3, transform, chunks (or a chunk-list object for huge files), previews, streams,
//! extended attributes, labels and derived records, SIGNED by its author device
//! (planning/architecture.md section 5.7 "Pointer v2"; rule M2 of section 6.10: "pointers against their
//! author's signature"; build unit S14, lane W1-S14d).
//!
//! | module | what |
//! |---|---|
//! | `wire` | the deterministic CBOR codec: `PointerV2::to_cbor`, `PointerV2::from_cbor` |
//! | `sign` | `PointerV2::sign`, `PointerV2::verify`, the witness [`Verified`] |
//! | `validate` | the limits, the ranges, the sums (`PointerV2::validate`, `check_consistency`) |
//! | `chunk_list` | the sealed chunk-list object of a huge file, the seam [`ObjectSealer`] |
//! | `error` | [`PointerError`], [`SealError`] |
//!
//! # The wire form
//!
//! A pointer is ONE map with unsigned-integer keys (the format of section 5.1 principle 6: versioned,
//! extensible, deterministic CBOR through `azlin-cbor`, unknown fields ignored, must-understand fields
//! listed in `crit`):
//!
//! | key | field | value |
//! |---|---|---|
//! | 0 | `crit` | ascending array of the keys the reader must understand (absent when empty) |
//! | 1 | `v` | the unsigned integer 2 |
//! | 2 | `size` | unsigned integer: the original's bytes |
//! | 3 | `mtime` | `[seconds, nanoseconds]`: seconds an integer (before 1970 negative), nanoseconds below 10^9 |
//! | 4 | `mode` | unsigned integer below 4096: the permission bits |
//! | 5 | `blake3` | 32 bytes: plain BLAKE3 of the ORIGINAL file |
//! | 6 | `transform` | `[id, out_len, params]`; absent: no transform |
//! | 7 | `chunks` | an array of chunk references, or the chunk-list reference `{1: name, 2: count, 3: total_raw}` |
//! | 8 | `author_device` | 16 bytes |
//! | 9 | `previews` | array of `[kind, w, h, [chunk references]]`; absent when empty |
//! | 10 | `streams` | array of `[kind, w, h, bandwidth, [chunk references]]`; absent when empty |
//! | 11 | `xattrs` | array of `[name, value]` (text, bytes); absent when empty |
//! | 12 | `labels` | array of text; absent when empty |
//! | 13 | `derived` | array of `[extractor, model_id, model_version, chunk reference]`; absent when empty |
//! | 14 | `sig` | 64 bytes; absent: unsigned |
//! | 15 | `created_at` | unsigned integer: seconds since 1970 |
//! | 16 and up | unknown | kept as they are, so a copy re-encodes byte for byte |
//!
//! A chunk reference is `[full (32 bytes), raw_len, pack (16 bytes), offset, stored_len]`
//! (section 5.7: `(full hash, raw length, pack, offset, stored length)`). Nested records have a fixed
//! number of elements: a record that grows is a new MAJOR version (`Decoded::Newer`), a field that
//! grows the pointer is a new top-level key (DRIFT X-DELTA-W1S14D-9: the plan's "unknown fields
//! ignored" is read for the top level; a chunk reference has no room for fields of its own).
//!
//! RULE FOR WRITERS OF A NEWER MINOR VERSION: a field that names chunks, packs or objects MUST be
//! listed in `crit` (the prune of S14e2 decides what is live from the chunks a pointer names, and a
//! reader that skipped such a field would call them unreferenced).
//!
//! # What the signature covers
//!
//! `sig` is Ed25519 over `"azlin-pointer-v2" || 0x00 || space_id (16) || the canonical CBOR of the pointer
//! WITHOUT key 14` (the one signing rule of ARCH29 9.13: `azlin_keys::sign::signing_input`), so a pointer
//! signed for one space is refused in another and every field is covered: the unknown ones, `crit` and
//! the chunk references too. `verify` finds the author's key through
//! `DeviceDirectory::verify_key(author_device, at)`; the instant `at` is a parameter because a device
//! may be revoked later (DRIFT X-DELTA-W1S14D-2: verify at the repository log entry's chain position,
//! `VerifyAt::ChainSeq`, not at `created_at`, which a revoked device can backdate).
//!
//! # Key material
//!
//! The chunk hashes `full` are KEY MATERIAL (a chunk's id and key derive from them): they are read with
//! `azlin_cbor::decode_secret` and handed to `Full::from_slice` (rule R17), `to_cbor` returns a
//! `Zeroizing` buffer, nothing here has `Clone` (a copy is `PointerV2::duplicate`), and `Debug` prints
//! no hash and no attribute value. Build a `Vec<ChunkRef>` with its exact capacity
//! (`Vec::with_capacity`): a `push` beyond the capacity copies the hashes to a new allocation and
//! leaves the old one unwiped (DRIFT X-DELTA-W1S14D-13).
//!
//! ```compile_fail
//! use azlin_content::pointer::PointerV2;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<PointerV2>();
//! ```
//!
//! ```compile_fail
//! use azlin_content::pointer::ChunkList;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<ChunkList>();
//! ```
//!
//! `to_cbor` hands the bytes back in a wiped-on-drop buffer (a compile-time check):
//!
//! ```
//! use azlin_content::pointer::{PointerError, PointerV2};
//! use zeroize::Zeroizing;
//!
//! let _signature: fn(&PointerV2) -> Result<Zeroizing<Vec<u8>>, PointerError> = PointerV2::to_cbor;
//! ```

use core::fmt;

use azlin_cbor::{SecretValue, Value};
use azlin_keys::{DeviceId, Signature, UnixTime};

use crate::ids::ChunkRef;
use crate::units::{FileSize, FileTime};

mod chunk_list;
mod error;
mod sign;
mod validate;
mod wire;

pub use chunk_list::{
    decode_chunk_list, encode_chunk_list, object_name, ObjectSealer, LIST_VERSION,
    MAX_SEAL_OVERHEAD,
};
pub use error::{Limit, Part, PointerError, SealError};
pub use sign::Verified;

// ---------------------------------------------------------------------------------------------
// The limits of a pointer (section 1.4 rule 4: "Everything has a limit, stated up front")
// ---------------------------------------------------------------------------------------------

/// The version number this build writes in key 1 and reads.
///
/// Caller: `PointerV2::to_cbor`, `PointerV2::from_cbor`, INT-S14-1 (the writer's own version check).
pub const VERSION: u8 = 2;

/// The highest key this build knows: the keys 1 to 15 are the fields above; a key from 16 up is
/// unknown and kept.
///
/// Caller: `PointerV2::validate` (the check of `crit`), `UnknownFields`.
pub const LAST_KNOWN_KEY: u64 = 15;

/// The most chunk references a pointer holds inline: 1,024 (a pointer of about 100 KiB at most). A
/// file with more chunks has a chunk-list object, and a file with at most this many has none, so each
/// pointer has ONE canonical encoding (DRIFT X-DELTA-W1S14D-3, P-2: the plan says "or a chunk-list
/// object for huge files" and gives no threshold).
///
/// Caller: INT-S14-1 (the writer chooses `Inline` or `Object` by it), `PointerV2::validate`,
/// `encode_chunk_list`.
pub const INLINE_MAX_CHUNKS: u32 = 1024;

/// `INLINE_MAX_CHUNKS` as a length of a `Vec` (every target of this crate has at least 32 bits).
const INLINE_MAX: usize = INLINE_MAX_CHUNKS as usize;

/// The most previews of one pointer: 64 (DRIFT X-DELTA-W1S14D-4, P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_PREVIEWS: usize = 64;

/// The most streams (renditions) of one pointer: 32 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_STREAMS: usize = 32;

/// The most extended attributes of one pointer: 64 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_XATTRS: usize = 64;

/// The most labels of one pointer: 32 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_LABELS: usize = 32;

/// The most derived records of one pointer: 64 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_DERIVED: usize = 64;

/// The longest text field (extractor, model id, model version) in bytes: 256 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_TEXT: usize = 256;

/// The longest label in bytes: 128 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_LABEL: usize = 128;

/// The longest extended-attribute name in bytes: 255 (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_XATTR_NAME: usize = 255;

/// The longest extended-attribute value in bytes: 64 KiB (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_XATTR_VALUE: usize = 64 * 1024;

/// The most bytes of all extended-attribute names and values together: 256 KiB (P-3).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_XATTR_TOTAL: usize = 256 * 1024;

/// The most chunk references of one preview: 16 (DRIFT X-DELTA-W1S14D-7: the brief's P-3 limits the
/// number of previews and not their chunks).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_PREVIEW_CHUNKS: usize = 16;

/// The most chunk references of one stream: 1,024, as many as a file's inline list (DRIFT
/// X-DELTA-W1S14D-7).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_STREAM_CHUNKS: usize = 1024;

/// The most bytes the parameters of a transform may encode to: 4 KiB (DRIFT X-DELTA-W1S14D-7).
///
/// Caller: `PointerV2::validate`, INT-S14-1 (azlin-compress's transforms).
pub const MAX_TRANSFORM_PARAMS: usize = 4096;

/// The largest width or height of a preview or a stream: 65,535 (DRIFT X-DELTA-W1S14D-7).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_DIMENSION: u32 = 65_535;

/// The largest file (and the largest transformed stream and chunk list) a pointer may declare: 2^50
/// bytes (check C12: rule I2's declared size bounds every decoder, so it is bounded itself; DRIFT
/// X-DELTA-W1S14D-7, -12: the chunk cap makes 2^46 the largest size a valid pointer can have).
///
/// Caller: `PointerV2::validate`, `encode_chunk_list`, INT-S14-1 (the writer's refusal of a file too
/// large for a pointer).
pub const MAX_FILE_SIZE: FileSize = FileSize::new(1 << 50);

/// The permission bits a `mode` may carry: the low twelve (`0o7777`). The apply step of section 6.10
/// (S10: no setuid from a remote entry) refuses the special bits; the pointer only carries them
/// (DRIFT X-DELTA-W1S14D-5, P-4).
///
/// Caller: `PointerV2::validate`, INT-S14-1 (the apply step).
pub const MODE_MASK: u32 = 0o7777;

/// The earliest `mtime` in seconds since 1970: 0000-01-01 00:00:00 UTC (DRIFT X-DELTA-W1S14D-7: a time
/// outside the years 0000 to 9999 would make a peer's conversion into a calendar or a `SystemTime`
/// fail).
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MIN_TIME_SECS: i64 = -62_167_219_200;

/// The latest `mtime` and `created_at` in seconds since 1970: 9999-12-31 23:59:59 UTC.
///
/// Caller: `PointerV2::validate`, INT-S14-1.
pub const MAX_TIME_SECS: u64 = 253_402_300_799;

// ---------------------------------------------------------------------------------------------
// The types
// ---------------------------------------------------------------------------------------------

/// The whole-file transform of a pointer (section 5.7 pipeline step 1: JPEG, PDF, Office through
/// azlin-compress): its id, the length of the stream it makes (which the chunks add up to) and its
/// parameters, opaque here.
///
/// Caller: INT-S14-1 (azlin-compress's adapter builds and reads it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformRef {
    /// The transform's id in azlin-compress's table.
    pub id: u16,
    /// The length of the transformed stream: the raw lengths of the chunks add up to it.
    pub out_len: FileSize,
    /// The transform's parameters: any CBOR value that encodes to at most `MAX_TRANSFORM_PARAMS`
    /// bytes. This crate does not read it.
    pub params: Value,
}

/// A reference to the chunk-list object of a huge file: the object's name (the BLAKE3 of its sealed
/// bytes, the name the store keeps), the number of chunks it lists and the sum of their raw lengths.
///
/// Caller: INT-S14-1 (`encode_chunk_list` makes it, the repository stores the object under its
/// `name`), `decode_chunk_list`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkListRef {
    /// The BLAKE3 hash of the sealed object.
    pub name: [u8; 32],
    /// The number of chunks the object lists: more than `INLINE_MAX_CHUNKS`, at most
    /// `MAX_FILE_CHUNKS`.
    pub count: u32,
    /// The sum of the raw lengths of the listed chunks.
    pub total_raw: FileSize,
}

/// The chunks of a file: inline in the pointer, or in a sealed chunk-list object. The canonical rule
/// (P-2): `Object` if and only if the file has more than `INLINE_MAX_CHUNKS` chunks. An empty file has
/// `Inline` with no chunks.
///
/// The hashes `full` inside are key material: no `Clone`, a second copy is
/// [`ChunkList::duplicate`].
///
/// Caller: INT-S14-1 (writes and reads pointers), S14e2 (the live set, through
/// [`PointerV2::chunk_refs`]).
#[derive(PartialEq, Eq)]
pub enum ChunkList {
    /// The chunk references themselves, at most `INLINE_MAX_CHUNKS`.
    Inline(Vec<ChunkRef>),
    /// A reference to the sealed chunk-list object.
    Object(ChunkListRef),
}

impl ChunkList {
    /// A second owner of the same list: the only way to copy one.
    ///
    /// Caller: `PointerV2::duplicate`, INT-S14-1.
    pub fn duplicate(&self) -> ChunkList {
        match self {
            ChunkList::Inline(refs) => ChunkList::Inline(duplicate_refs(refs)),
            ChunkList::Object(r) => ChunkList::Object(*r),
        }
    }
}

/// Prints the number of inline chunks, never a hash.
impl fmt::Debug for ChunkList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChunkList::Inline(refs) => write!(f, "Inline({} chunks)", refs.len()),
            ChunkList::Object(r) => write!(f, "Object({r:?})"),
        }
    }
}

/// A copy of a slice of chunk references, in a vector of exactly the right capacity (no growth that
/// would copy the hashes to a second allocation).
fn duplicate_refs(refs: &[ChunkRef]) -> Vec<ChunkRef> {
    let mut out = Vec::with_capacity(refs.len());
    for r in refs {
        out.push(r.duplicate());
    }
    out
}

/// One preview of a file (section 5.7: `previews (kind, w, h, chunk refs)`): a small rendition for a
/// file list, kept in a `derived`-class pack. `kind` is the apps' (an image, a poster frame, a
/// waveform).
///
/// Caller: INT-S14-1, the apps' preview pipeline.
#[derive(Debug, PartialEq, Eq)]
pub struct Preview {
    /// What the preview is (the apps' code).
    pub kind: u8,
    /// Width in pixels, 0 to `MAX_DIMENSION`.
    pub w: u32,
    /// Height in pixels, 0 to `MAX_DIMENSION`.
    pub h: u32,
    /// The chunks of the preview: at least one, at most `MAX_PREVIEW_CHUNKS`.
    pub chunks: Vec<ChunkRef>,
}

impl Preview {
    /// A second owner of the same preview.
    ///
    /// Caller: `PointerV2::duplicate`.
    pub fn duplicate(&self) -> Preview {
        Preview {
            kind: self.kind,
            w: self.w,
            h: self.h,
            chunks: duplicate_refs(&self.chunks),
        }
    }
}

/// One rendition of a media file (section 5.7: `streams (HLS-style renditions)`; section 5.12: "the
/// smaller renditions in a file's pointer serve slow lines"). DRIFT X-DELTA-W1S14D-6: the plan names
/// the field and no content; this is the preview's record plus the rendition's bandwidth.
///
/// Caller: INT-S14-1, the apps' media players.
#[derive(Debug, PartialEq, Eq)]
pub struct StreamRef {
    /// What the rendition is (the apps' code: video, audio, ...).
    pub kind: u8,
    /// Width in pixels (0: none), at most `MAX_DIMENSION`.
    pub w: u32,
    /// Height in pixels (0: none), at most `MAX_DIMENSION`.
    pub h: u32,
    /// The rendition's declared bandwidth in bits per second, for the player's choice of a line.
    pub bandwidth: u32,
    /// The chunks of the rendition: at least one, at most `MAX_STREAM_CHUNKS`.
    pub chunks: Vec<ChunkRef>,
}

impl StreamRef {
    /// A second owner of the same stream.
    ///
    /// Caller: `PointerV2::duplicate`.
    pub fn duplicate(&self) -> StreamRef {
        StreamRef {
            kind: self.kind,
            w: self.w,
            h: self.h,
            bandwidth: self.bandwidth,
            chunks: duplicate_refs(&self.chunks),
        }
    }
}

/// One derived record of a file (sections 5.7 and 5.16: `derived (extractor, model_id, model_version,
/// record ref)`): search data that syncs without the file and can be dropped and recomputed. The
/// record is a chunk in a `derived`-class pack (P-5).
///
/// Caller: INT-S14-1, the apps' on-device search (section 6.7).
#[derive(Debug, PartialEq, Eq)]
pub struct DerivedRef {
    /// The extractor that made the record (text, at most `MAX_TEXT` bytes).
    pub extractor: String,
    /// The model's id (text, at most `MAX_TEXT` bytes; may be empty).
    pub model_id: String,
    /// The model's version (text, at most `MAX_TEXT` bytes; may be empty).
    pub model_version: String,
    /// The chunk that holds the record.
    pub record: ChunkRef,
}

impl DerivedRef {
    /// A second owner of the same record reference.
    ///
    /// Caller: `PointerV2::duplicate`.
    pub fn duplicate(&self) -> DerivedRef {
        DerivedRef {
            extractor: self.extractor.clone(),
            model_id: self.model_id.clone(),
            model_version: self.model_version.clone(),
            record: self.record.duplicate(),
        }
    }
}

/// The fields of a NEWER minor version of the pointer, kept as they were read (keys 16 and up, strictly
/// ascending) so that a copy still verifies: "ignore unknown fields" and a signature over the bytes are
/// compatible only when a copy re-encodes them byte for byte (check C9). Their values are wiped when
/// this is dropped (rule R17: a newer field may carry key material this build cannot recognise).
///
/// Caller: `PointerV2::from_cbor` (fills it), `PointerV2::to_cbor` (writes it back), the tests of a
/// newer minor version.
#[derive(PartialEq, Eq)]
pub struct UnknownFields(Vec<(u64, Value)>);

impl UnknownFields {
    /// No unknown fields.
    ///
    /// Caller: INT-S14-1 (every pointer this build makes), `PointerV2::from_cbor`.
    pub fn none() -> UnknownFields {
        UnknownFields(Vec::new())
    }

    /// These fields: the keys must be 16 or more and strictly ascending, else
    /// [`PointerError::BadUnknown`] (the values are wiped).
    ///
    /// Caller: `PointerV2::from_cbor`, the tests of a newer minor version.
    pub fn from_entries(entries: Vec<(u64, Value)>) -> Result<UnknownFields, PointerError> {
        let mut valid = true;
        let mut previous: Option<u64> = None;
        for (key, _) in &entries {
            let ascending = previous.is_none_or(|last| last < *key);
            if *key <= LAST_KNOWN_KEY || !ascending {
                valid = false;
                break;
            }
            previous = Some(*key);
        }
        if !valid {
            wipe_entries(entries);
            return Err(PointerError::BadUnknown);
        }
        Ok(UnknownFields(entries))
    }

    /// The fields in key order.
    ///
    /// Caller: `PointerV2::to_cbor`, the tests.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &Value)> + '_ {
        self.0.iter().map(|(key, value)| (*key, value))
    }

    /// The number of fields.
    ///
    /// Caller: the tests.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    ///
    /// Caller: `PointerV2::to_cbor`, the tests.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A second owner of the same fields.
    ///
    /// Caller: `PointerV2::duplicate`.
    pub fn duplicate(&self) -> UnknownFields {
        UnknownFields(self.0.clone())
    }
}

impl Drop for UnknownFields {
    fn drop(&mut self) {
        wipe_entries(core::mem::take(&mut self.0));
    }
}

/// Prints the number of fields, never a value.
impl fmt::Debug for UnknownFields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UnknownFields({} keys)", self.0.len())
    }
}

/// Wipes every byte and text buffer of the entries: the tree goes through `SecretValue`, whose drop is
/// the one wipe of azlin-cbor.
fn wipe_entries(entries: Vec<(u64, Value)>) {
    let leaves: Vec<Value> = entries.into_iter().map(|(_, value)| value).collect();
    drop(SecretValue::new(Value::Array(leaves)));
}

/// One pointer v2 (section 5.7 "Pointer v2"): everything a device needs to rebuild one version of a
/// file, signed by the device that made it. Written and read by [`PointerV2::to_cbor`] and
/// [`PointerV2::from_cbor`], signed and verified by [`PointerV2::sign`] and [`PointerV2::verify`].
///
/// The chunk hashes inside are key material: no `Clone` (a copy is [`PointerV2::duplicate`]), a
/// redacted `Debug`.
///
/// Caller: INT-S14-1 (azul-storage writes and reads pointers), INT-S14-2 (azcloud-kit's sync engine),
/// INT-S16 (log records carry signed pointers).
#[derive(PartialEq, Eq)]
pub struct PointerV2 {
    /// The size of the original file in bytes (key 2), at most `MAX_FILE_SIZE`.
    pub size: FileSize,
    /// The modification time (key 3): seconds (before 1970 allowed) and nanoseconds.
    pub mtime: FileTime,
    /// The permission bits (key 4), `MODE_MASK` at most. The apply step refuses setuid and the like
    /// (section 6.10 S10), not this crate.
    pub mode: u32,
    /// The plain BLAKE3 of the ORIGINAL file (key 5): the file-level check that also covers share
    /// recipients, who hold `full` but not `K_chunk`.
    pub blake3: [u8; 32],
    /// The whole-file transform (key 6), `None` when the chunks are the file itself.
    pub transform: Option<TransformRef>,
    /// The chunks (key 7): the raw lengths add up to the transformed length (`transform.out_len`) or,
    /// without a transform, to `size`.
    pub chunks: ChunkList,
    /// The device that made this pointer and signed it (key 8).
    pub author_device: DeviceId,
    /// The author's signature (key 14), `None` until [`PointerV2::sign`].
    pub sig: Option<Signature>,
    /// The previews (key 9).
    pub previews: Vec<Preview>,
    /// The streams (key 10).
    pub streams: Vec<StreamRef>,
    /// The extended attributes (key 11): unique names of 1 to 255 bytes without NUL, in the order
    /// they are written.
    pub xattrs: Vec<(String, Vec<u8>)>,
    /// The labels (key 12): texts of 1 to 128 bytes without NUL.
    pub labels: Vec<String>,
    /// The derived records (key 13).
    pub derived: Vec<DerivedRef>,
    /// When the pointer was made (key 15), seconds since 1970. The device's own clock: it proves
    /// nothing of WHEN (see the module documentation: verify at the log entry's position).
    pub created_at: UnixTime,
    /// The fields of a newer minor version (keys 16 and up), kept so a copy still verifies (C9).
    pub unknown: UnknownFields,
    /// The keys the reader must understand (key 0), kept so a copy keeps its must-understand markers
    /// (rule R18): ascending, each from 1 to 15. This build writes none; a newer writer may list a
    /// known key, and a listed key this build does not know refuses the pointer.
    pub crit: Vec<u64>,
}

impl PointerV2 {
    /// A second owner of the same pointer: the only way to copy one (every chunk reference through
    /// `ChunkRef::duplicate`, the unknown fields and `crit` included, so the copy encodes to the same
    /// bytes and still verifies).
    ///
    /// Caller: INT-S14-1 (a new version is made from the previous pointer), the tests.
    pub fn duplicate(&self) -> PointerV2 {
        PointerV2 {
            size: self.size,
            mtime: self.mtime,
            mode: self.mode,
            blake3: self.blake3,
            transform: self.transform.clone(),
            chunks: self.chunks.duplicate(),
            author_device: self.author_device,
            sig: self.sig,
            previews: self.previews.iter().map(Preview::duplicate).collect(),
            streams: self.streams.iter().map(StreamRef::duplicate).collect(),
            xattrs: self.xattrs.clone(),
            labels: self.labels.clone(),
            derived: self.derived.iter().map(DerivedRef::duplicate).collect(),
            created_at: self.created_at,
            unknown: self.unknown.duplicate(),
            crit: self.crit.clone(),
        }
    }

    /// Every chunk reference the pointer holds itself: the inline chunks, the chunks of the previews and
    /// of the streams, the derived records. The chunks of an `Object` list are in the object
    /// (`decode_chunk_list`), not here. This is the one place that names every kind of reference, so a
    /// reader that decides what is live (the prune of S14e2) cannot forget one.
    ///
    /// Caller: S14e2 (the live set, through INT-S14-1's `VersionSource`), `PointerV2::validate`.
    pub fn chunk_refs(&self) -> impl Iterator<Item = &ChunkRef> + '_ {
        let inline: &[ChunkRef] = match &self.chunks {
            ChunkList::Inline(refs) => refs.as_slice(),
            ChunkList::Object(_) => &[],
        };
        inline.iter().chain(self.side_refs())
    }

    /// The chunk references of the previews, the streams and the derived records.
    fn side_refs(&self) -> impl Iterator<Item = &ChunkRef> + '_ {
        self.previews
            .iter()
            .flat_map(|p| p.chunks.iter())
            .chain(self.streams.iter().flat_map(|s| s.chunks.iter()))
            .chain(self.derived.iter().map(|d| &d.record))
    }
}

/// Prints the sizes and the counts, never a chunk hash, an attribute value or the file's BLAKE3.
impl fmt::Debug for PointerV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointerV2")
            .field("size", &self.size)
            .field("mtime", &self.mtime)
            .field("mode", &format_args!("{:#o}", self.mode))
            .field("blake3", &"***")
            .field("transform", &self.transform.as_ref().map(|t| t.id))
            .field("chunks", &self.chunks)
            .field("author_device", &self.author_device)
            .field("signed", &self.sig.is_some())
            .field("previews", &self.previews.len())
            .field("streams", &self.streams.len())
            .field("xattrs", &self.xattrs.len())
            .field("labels", &self.labels.len())
            .field("derived", &self.derived.len())
            .field("created_at", &self.created_at)
            .field("unknown", &self.unknown)
            .field("crit", &self.crit)
            .finish()
    }
}

/// What reading a pointer's bytes gives: a pointer of this build's format, or the word that a newer
/// format wrote it. A newer pointer is NEVER damage: the caller shows "update the app to open it" and
/// changes nothing (the idea of `azul-storage`'s `meta/pointer.rs::newer_format`).
///
/// The pointer is boxed (DRIFT X-DELTA-W1S14D-8: a pointer is about 400 bytes and the other variant
/// one): `Decoded::Current(p)` gives a `Box<PointerV2>`, which dereferences to the pointer (`*p` moves
/// it out).
///
/// A reader that decides what is LIVE from the chunks a pointer names (the prune of S14e2) must take a
/// `Newer` pointer, like any pointer it cannot read, as "cannot tell" and delete nothing: this build
/// knows nothing of what that pointer names (a writer's field that names chunks is listed in `crit`,
/// so a minor version of the format is refused here instead of being read without it).
///
/// Caller: INT-S14-1 and INT-S14-2 (every read of a pointer).
#[derive(Debug)]
pub enum Decoded {
    /// A pointer of this build's format (version 2), valid and in its one canonical encoding. Its
    /// signature is NOT checked: that is [`PointerV2::verify`].
    Current(Box<PointerV2>),
    /// A pointer of a newer format: its version number. Nothing else of it was read.
    Newer(u8),
}
