//! The errors of the pointer module: one small `Copy` enum per kind of failure, no strings as control
//! flow and no data that could carry a secret (the variants hold key numbers, limit names and the
//! errors of the layers below), so a caller maps each to its own status and a `Display` can be
//! logged as it is (planning/architecture.md section 5.7 "Pointer v2"; build unit S14, lane W1-S14d).
//!
//! UNKNOWN IS A STATE (section 1.4 rule 11): a directory that cannot answer, a signature that does
//! not verify, a sealer that refuses and an object that is not the one the pointer names are errors
//! that stop the operation, never a default and never "valid". A pointer of a NEWER format is not an
//! error at all: `PointerV2::from_cbor` answers `Decoded::Newer(version)` (the idea of
//! `azul-storage`'s `meta/pointer.rs::newer_format`: a newer pointer is never damage).

use core::fmt;

use azlin_cbor::{DecodeError, EncodeError, MapError, ShapeError};

use crate::error::ChunkError;

/// A nested record of the pointer, named where its shape is wrong (a wrong type or a wrong number
/// of elements inside it). The top-level keys are named by `ShapeError` (key numbers).
///
/// Caller: INT-S14-1 (messages), the tests of the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The `mtime` pair `[seconds, nanoseconds]`.
    Mtime,
    /// The `transform` triple `[id, out_len, params]`.
    Transform,
    /// The `chunks` value: neither a list of chunk references nor a chunk-list reference.
    Chunks,
    /// One chunk reference `[full, raw_len, pack, offset, stored_len]`.
    ChunkRef,
    /// The chunk-list reference `{1: name, 2: count, 3: total_raw}`.
    ListRef,
    /// One preview `[kind, w, h, chunks]`.
    Preview,
    /// One stream `[kind, w, h, bandwidth, chunks]`.
    Stream,
    /// One extended attribute `[name, value]`.
    Xattr,
    /// One label (a text string).
    Label,
    /// One derived record `[extractor, model_id, model_version, record]`.
    Derived,
}

impl Part {
    /// The word for the part in messages.
    ///
    /// Caller: `PointerError`'s `Display`.
    pub const fn name(self) -> &'static str {
        match self {
            Part::Mtime => "modification time",
            Part::Transform => "transform",
            Part::Chunks => "chunk list",
            Part::ChunkRef => "chunk reference",
            Part::ListRef => "chunk-list reference",
            Part::Preview => "preview",
            Part::Stream => "stream",
            Part::Xattr => "extended attribute",
            Part::Label => "label",
            Part::Derived => "derived record",
        }
    }
}

/// A limit of section 1.4 rule 4 that a pointer went over (DRIFT X-DELTA-W1S14D-4: the plan gives
/// none, the brief's P-3 and this lane's additions name them; the numbers are constants of
/// `pointer`).
///
/// Caller: INT-S14-1 (messages), the tests of the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limit {
    /// More previews than `MAX_PREVIEWS`.
    Previews,
    /// More streams than `MAX_STREAMS`.
    Streams,
    /// More extended attributes than `MAX_XATTRS`.
    Xattrs,
    /// More labels than `MAX_LABELS`.
    Labels,
    /// More derived records than `MAX_DERIVED`.
    Derived,
    /// A preview with more chunk references than `MAX_PREVIEW_CHUNKS`.
    PreviewChunks,
    /// A stream with more chunk references than `MAX_STREAM_CHUNKS`.
    StreamChunks,
    /// A text field (extractor, model id, model version) longer than `MAX_TEXT`.
    Text,
    /// A label longer than `MAX_LABEL`.
    Label,
    /// An extended-attribute name longer than `MAX_XATTR_NAME`.
    XattrName,
    /// An extended-attribute value longer than `MAX_XATTR_VALUE`.
    XattrValue,
    /// Names and values of all extended attributes together longer than `MAX_XATTR_TOTAL`.
    XattrTotal,
    /// Transform parameters that encode to more than `MAX_TRANSFORM_PARAMS` bytes.
    TransformParams,
}

impl Limit {
    /// The word for the limit in messages.
    ///
    /// Caller: `PointerError`'s `Display`.
    pub const fn name(self) -> &'static str {
        match self {
            Limit::Previews => "previews",
            Limit::Streams => "streams",
            Limit::Xattrs => "extended attributes",
            Limit::Labels => "labels",
            Limit::Derived => "derived records",
            Limit::PreviewChunks => "chunks of a preview",
            Limit::StreamChunks => "chunks of a stream",
            Limit::Text => "a text field",
            Limit::Label => "a label",
            Limit::XattrName => "an attribute name",
            Limit::XattrValue => "an attribute value",
            Limit::XattrTotal => "all attributes together",
            Limit::TransformParams => "the transform parameters",
        }
    }
}

/// Why a sealer refused to seal or to open a chunk-list object: the seam `ObjectSealer`
/// (INT-S14-1 adapts azul-storage's `meta::Sealer`, whose own error is a sentence).
///
/// Caller: `ObjectSealer` implementations (INT-S14-1, the test doubles), `PointerError::Seal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SealError {
    /// The object does not authenticate: another key, another context, any changed byte. One word for
    /// all of them, so the refusal says nothing of which part failed.
    Auth,
    /// The plaintext or the sealed object is longer than the sealer takes.
    TooLong,
    /// The sealer could not do its work (no key at hand, a failed store, a failed entropy draw).
    Failed,
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SealError::Auth => "the object does not authenticate",
            SealError::TooLong => "the object is longer than the sealer takes",
            SealError::Failed => "the sealer could not do its work",
        })
    }
}

impl std::error::Error for SealError {}

/// Why a pointer or a chunk-list object cannot be written, read, signed or verified.
///
/// Caller: INT-S14-1 and INT-S14-2 map each variant to a status and a retry (a damaged pointer is not
/// a newer one, an unavailable directory is not an unknown author), INT-S16 (log records that carry
/// signed pointers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerError {
    /// The strict CBOR codec refused the bytes (truncated, trailing bytes, a non-shortest integer, an
    /// unsorted or duplicate map key, a float or a tag, a limit of the caller's `Limits`).
    Cbor(DecodeError),
    /// A key of the pointer (or of a chunk-list object) is missing, has the wrong type or length, or
    /// `crit` is malformed or names a key this build does not understand.
    Shape(ShapeError),
    /// Writing hit a limit of the CBOR codec (`Limits::OBJECT`).
    Encode(EncodeError),
    /// The map of a struct could not be built (a duplicate key: a programming error of this module,
    /// returned instead of a panic).
    Map(MapError),
    /// A nested record has the wrong shape: the wrong type or the wrong number of elements.
    BadShape(Part),
    /// The version is 0 or 1: formats this build does not read.
    UnsupportedVersion(u8),
    /// The version number is above 255 (a newer version is `Decoded::Newer`, but its number must fit a
    /// byte).
    BadVersion,
    /// The bytes are valid CBOR with valid fields but not the one encoding of their pointer (an empty
    /// list written as an empty array, a `null` for an absent field, or any difference the re-encoding
    /// shows). The signature covers canonical bytes only.
    NotCanonical,
    /// `mode` has a bit above the low twelve (the permission bits).
    BadMode,
    /// A time (`mtime`, `created_at`) outside the years 0000 to 9999, or nanoseconds of a second or
    /// more.
    BadTime,
    /// `size`, `transform.out_len` or a list's `total_raw` above 2^50 bytes.
    SizeTooLarge,
    /// A preview or stream dimension above 65,535.
    BadDimension,
    /// The `unknown` fields are not unsigned-integer keys from 16 up, strictly ascending.
    BadUnknown,
    /// `crit` lists key 0, a key above 15, or is not strictly ascending.
    BadCrit,
    /// A chunk-list reference or object declares more than `MAX_FILE_CHUNKS` chunks.
    TooManyChunks,
    /// More than `INLINE_MAX_CHUNKS` chunk references inline: such a file has a chunk-list object.
    TooManyInline,
    /// A chunk-list object for `INLINE_MAX_CHUNKS` chunks or fewer: such a file has them inline.
    ObjectTooSmall,
    /// A chunk reference whose numbers cannot describe a chunk (S14b's `ChunkRef::check`).
    BadChunk(ChunkError),
    /// The raw lengths of the chunks do not add up to the declared length (`size`, or the transform's
    /// `out_len`).
    SizeMismatch,
    /// A sum or a product of lengths does not fit 64 bits.
    Overflow,
    /// A collection or text over its limit.
    OverLimit(Limit),
    /// A list that must not be empty (the chunks of a preview or of a stream).
    EmptyList(Part),
    /// An extended-attribute name that is empty or holds a NUL byte.
    BadXattrName,
    /// Two extended attributes of the same name.
    DuplicateXattr,
    /// A label or a text field that is empty where text is required, or holds a NUL byte.
    BadText,
    /// The pointer has no signature.
    Unsigned,
    /// The signature does not verify: another space, another author key, any changed field.
    BadSignature,
    /// The device directory knows no device of the pointer's `author_device`.
    UnknownAuthor,
    /// The author's key is not valid yet at the instant asked about.
    AuthorNotYetActive,
    /// The author's key is revoked at the instant asked about.
    AuthorRevoked,
    /// The device directory could not answer (never "unknown device", never "valid").
    DirectoryUnavailable,
    /// The sealer refused the chunk-list object.
    Seal(SealError),
    /// The sealed chunk-list object is not the one the pointer names (its BLAKE3 is not `name`).
    ListName,
    /// The chunk-list object holds another number of chunks than the reference declares.
    ListCount,
    /// The chunk lengths of the object do not add up to the reference's `total_raw`, or the declared
    /// total cannot belong to that many chunks.
    ListTotal,
    /// The sealed object or its plaintext is longer than a list of that many chunks can be.
    ListTooLong,
    /// The chunk-list object is of a newer format (its version is above 1): update to read it.
    NewerList(u8),
    /// The chunk-list object's version is 0.
    UnsupportedList(u8),
}

impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PointerError::Cbor(e) => write!(f, "the pointer is not valid deterministic CBOR: {e}"),
            PointerError::Shape(e) => write!(f, "the pointer has the wrong shape: {e}"),
            PointerError::Encode(e) => write!(f, "the pointer cannot be written: {e}"),
            PointerError::Map(e) => write!(f, "the pointer's map cannot be built: {e}"),
            PointerError::BadShape(part) => write!(f, "the pointer's {} is malformed", part.name()),
            PointerError::UnsupportedVersion(v) => {
                write!(f, "pointer version {v} is not read by this build")
            }
            PointerError::BadVersion => f.write_str("the pointer's version is above 255"),
            PointerError::NotCanonical => {
                f.write_str("the pointer is not the one canonical encoding of its content")
            }
            PointerError::BadMode => f.write_str("the file mode has bits above the low twelve"),
            PointerError::BadTime => f.write_str("a time is outside the years 0000 to 9999"),
            PointerError::SizeTooLarge => f.write_str("a declared size is above 2^50 bytes"),
            PointerError::BadDimension => f.write_str("a dimension is above 65,535"),
            PointerError::BadUnknown => {
                f.write_str("the unknown fields are not ascending keys from 16 up")
            }
            PointerError::BadCrit => f.write_str("`crit` is not an ascending list of known keys"),
            PointerError::TooManyChunks => f.write_str("the file declares too many chunks"),
            PointerError::TooManyInline => {
                f.write_str("too many chunks inline: the file needs a chunk-list object")
            }
            PointerError::ObjectTooSmall => {
                f.write_str("a chunk-list object for so few chunks: they belong inline")
            }
            PointerError::BadChunk(e) => write!(f, "a chunk reference is refused: {e}"),
            PointerError::SizeMismatch => {
                f.write_str("the chunk lengths do not add up to the declared length")
            }
            PointerError::Overflow => f.write_str("a length does not fit 64 bits"),
            PointerError::OverLimit(limit) => write!(f, "too many or too long: {}", limit.name()),
            PointerError::EmptyList(part) => write!(f, "a {} has no chunks", part.name()),
            PointerError::BadXattrName => {
                f.write_str("an attribute name is empty or holds a NUL byte")
            }
            PointerError::DuplicateXattr => f.write_str("two attributes have the same name"),
            PointerError::BadText => f.write_str("a text is empty or holds a NUL byte"),
            PointerError::Unsigned => f.write_str("the pointer has no signature"),
            PointerError::BadSignature => f.write_str("the pointer's signature does not verify"),
            PointerError::UnknownAuthor => f.write_str("the pointer's author device is unknown"),
            PointerError::AuthorNotYetActive => f.write_str("the author's key is not active yet"),
            PointerError::AuthorRevoked => f.write_str("the author's key is revoked"),
            PointerError::DirectoryUnavailable => {
                f.write_str("the device directory is unavailable")
            }
            PointerError::Seal(e) => {
                write!(f, "the chunk-list object cannot be sealed or opened: {e}")
            }
            PointerError::ListName => {
                f.write_str("the chunk-list object is not the one the pointer names")
            }
            PointerError::ListCount => {
                f.write_str("the chunk-list object holds another number of chunks than declared")
            }
            PointerError::ListTotal => {
                f.write_str("the chunk-list object's lengths do not match the declared total")
            }
            PointerError::ListTooLong => {
                f.write_str("the chunk-list object is longer than its chunk count allows")
            }
            PointerError::NewerList(v) => write!(
                f,
                "the chunk-list object is of a newer format ({v}): update the app to open it"
            ),
            PointerError::UnsupportedList(v) => {
                write!(f, "chunk-list object version {v} is not read by this build")
            }
        }
    }
}

impl std::error::Error for PointerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PointerError::Cbor(e) => Some(e),
            PointerError::Shape(e) => Some(e),
            PointerError::Encode(e) => Some(e),
            PointerError::Map(e) => Some(e),
            PointerError::BadChunk(e) => Some(e),
            PointerError::Seal(e) => Some(e),
            _ => None,
        }
    }
}

impl From<DecodeError> for PointerError {
    fn from(e: DecodeError) -> PointerError {
        PointerError::Cbor(e)
    }
}

impl From<ShapeError> for PointerError {
    fn from(e: ShapeError) -> PointerError {
        PointerError::Shape(e)
    }
}

impl From<EncodeError> for PointerError {
    fn from(e: EncodeError) -> PointerError {
        PointerError::Encode(e)
    }
}

impl From<MapError> for PointerError {
    fn from(e: MapError) -> PointerError {
        PointerError::Map(e)
    }
}

impl From<SealError> for PointerError {
    fn from(e: SealError) -> PointerError {
        PointerError::Seal(e)
    }
}
