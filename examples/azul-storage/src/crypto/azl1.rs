//! The AZL1 object: one version of one file, compressed where that pays and encrypted, as it
//! lies in the bucket under its random key.
//!
//! ```text
//! [header, 4096 bytes]                                 all integers little endian
//!     0   4  magic "AZL1"
//!     4   2  version (1; 2 for a transformed file, see below)
//!     6   2  header flags (0)
//!     8   4  header length (4096)
//!    12   4  segment size: plaintext bytes per segment (1 MiB)
//!    16  16  object id (the random id of the bucket key)
//!    32  19  nonce prefix of the segments' STREAM (random)
//!    51   5  zero
//!    56  32  key commitment: BLAKE3 keyed hash, keyed with the file key, of
//!            "AZL1 key commitment v1"
//!            -- bytes 0..88 are the associated data of the info, of every segment and of
//!               the trailer: none of them opens under another header --
//!    88  88  the file key wrapped with the drive key (key id 16, nonce 24, sealed key + tag 48)
//!   176  24  info nonce (random)
//!   200  76  the info, sealed: plaintext size u64, segment count u64, object length u64,
//!            BLAKE3 of the plaintext (32), flags u32 (bit 0: a segment is compressed); + tag
//!   276  24  trailer nonce (random)
//!   300      zero up to 4096 (version 1)
//!   version 2 only:
//!   300  24  transform nonce (random)
//!   324  64  the transform block, sealed with the info key (associated data: bytes 0..88 and
//!            "AZL1 transform v2"): transform id u8, its version u8, 6 zero bytes, the
//!            transformed length u64, BLAKE3 of the transformed stream (32); + tag
//!   388      zero up to 4096
//! [segments]
//!     segment i: XChaCha20-Poly1305 of (codec byte || data) - codec 0 stored, 1 zstd,
//!     2 brotli, 4 zstd with a shipped dictionary (see `codec`) -, the nonce being the prefix, i as
//!     a 32-bit big-endian counter and a last-segment flag (aead's STREAM, `StreamBE32`): a
//!     segment moved to another place does not open, nor does a cut after any segment but
//!     the last. Every object has at least one segment (an empty file: one empty last one).
//! [trailer]
//!     the segments' sealed lengths (u32 each), sealed; then the trailer's own length (u64)
//! ```
//!
//! What the bucket shows of an object is its length and the public header: no name, no
//! plaintext size, no hash of the plaintext (they are sealed in the info).
//!
//! Version 2 holds a file that went through a whole-file transform of azlin-compress (Lepton
//! for a JPEG, the rebuilt deflate streams of a PDF, Office, ZIP, PNG or gzip file): its
//! segments carry the TRANSFORMED stream, cut and sealed exactly as a version 1 file's bytes,
//! and the transform block says which transform and what the stream is. The info's size and
//! BLAKE3 stay the original file's (what the index, the quota and the final check use); its
//! segment count is the transformed stream's. A reader decrypts the stream, checks its BLAKE3,
//! runs the inverse transform (never past the sealed size) and checks the original's BLAKE3.
//! Transforms run only on files up to 64 MiB, so a version 2 file is decoded whole even for a
//! ranged read. Every object without a transform stays version 1, byte for byte.
//!
//! Keys: the file key is 256 random bits, made for this object only (a rewrite is a new object
//! with a new key). Three keys are derived from it with BLAKE3's `derive_key`, each with a
//! context of its own: the segments' key, the info's key and the trailer's key. The info and
//! the trailer are sealed once each, under random nonces stored in the header.
//!
//! Order of checks when an object is opened: the magic, version and layout of the header; the
//! object id it names; the KEY COMMITMENT (before anything is decrypted: XChaCha20-Poly1305 is
//! not key-committing, and a file shared under several keys must not open to different
//! contents per key); the info; the trailer, whose lengths must add up to the object. Then each
//! segment authenticates on its own: a ranged read decrypts only the segments it covers, and a
//! read of the whole file also checks the BLAKE3 of the plaintext.
//!
//! Streaming: [`Azl1Writer`] keeps one segment in memory whatever the file's size; it writes
//! the segments and the trailer as they come and hands the header back at the end, for the
//! front of the object ([`encrypt_stream`] seeks back and writes it). [`OpenObject`] reads the
//! header and the trailer (two reads), then segments on demand ([`OpenObject::read_range`],
//! [`Azl1Reader`]).

use std::{
    fmt,
    io::{self, Read, Seek, SeekFrom, Write},
};

use chacha20poly1305::{
    aead::{
        stream::{NewStream, Nonce as StreamNonce, StreamBE32, StreamPrimitive},
        Aead, Payload,
    },
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use zeroize::{Zeroize, Zeroizing};

use azlin_compress::{
    transform::{self, Forward},
    Kind, Limits,
};
/// A whole-file transform, as a version 2 object names it.
pub use azlin_compress::TransformId;

use super::{
    codec::{self, Codec, Compression, Encoded, Encoder},
    random_bytes, CryptoError, DriveKey, FileKey, ObjectId, WrappedKey, NONCE_LEN,
    OBJECT_ID_LEN, TAG_LEN,
};

/// The first four bytes of every object.
pub const MAGIC: [u8; 4] = *b"AZL1";
/// The format version of a file stored without a whole-file transform.
pub const VERSION: u16 = 1;
/// The format version of a transformed file (the transform block in the header).
pub const VERSION_TRANSFORMED: u16 = 2;
/// Bytes of the header.
pub const HEADER_LEN: usize = 4096;
/// Plaintext bytes per segment, unless a writer is told otherwise.
pub const DEFAULT_SEGMENT_SIZE: u32 = 1 << 20;
/// The smallest and the largest segment size a header may name.
pub const MIN_SEGMENT_SIZE: u32 = 1 << 10;
pub const MAX_SEGMENT_SIZE: u32 = 1 << 24;

/// The STREAM nonce prefix: 24 nonce bytes less the 32-bit counter and the last-segment flag.
pub const PREFIX_LEN: usize = 19;
/// Bytes of the sealed info's plaintext.
const INFO_LEN: usize = 60;
/// Bytes of the key commitment.
const COMMITMENT_LEN: usize = 32;
/// A segment's bytes beyond its data: the codec byte and the tag.
pub const SEGMENT_OVERHEAD: u64 = 1 + TAG_LEN as u64;
/// Bytes of one trailer entry (a segment's sealed length).
const TRAILER_ENTRY: usize = 4;
/// Bytes of the trailer's length at the very end.
const LENGTH_FIELD: u64 = 8;
/// Info flag: at least one segment is compressed.
const FLAG_COMPRESSED: u32 = 1;
/// Bytes of the sealed transform block's plaintext (version 2).
const TRANSFORM_LEN: usize = 48;

// The header's fields: where they start.
const OFF_VERSION: usize = 4;
const OFF_FLAGS: usize = 6;
const OFF_HEADER_LEN: usize = 8;
const OFF_SEGMENT_SIZE: usize = 12;
const OFF_OBJECT_ID: usize = 16;
const OFF_PREFIX: usize = 32;
const OFF_COMMITMENT: usize = 56;
/// Bytes 0..88: the associated data of everything sealed with the file's keys.
const CONTEXT_LEN: usize = 88;
const OFF_WRAP: usize = 88;
const OFF_INFO_NONCE: usize = 176;
const OFF_INFO: usize = 200;
const OFF_TRAILER_NONCE: usize = 276;
/// Version 1: everything from here to the header's end is zero.
const HEADER_USED: usize = 300;
/// Version 2: the transform block's nonce and sealed bytes, then zero to the header's end.
const OFF_TRANSFORM_NONCE: usize = 300;
const OFF_TRANSFORM: usize = 324;
const HEADER_USED_TRANSFORMED: usize = OFF_TRANSFORM + TRANSFORM_LEN + TAG_LEN;

/// BLAKE3 `derive_key` contexts of the keys derived from a file key (one key, one purpose).
const SEGMENT_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 segment key";
const INFO_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 header info key";
const TRAILER_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 trailer key";
/// What the key commitment hashes, keyed with the file key.
const COMMITMENT_INPUT: &[u8] = b"AZL1 key commitment v1";
/// Appended to the header context in the transform block's associated data: the block and the
/// info share the info key, and neither opens in the other's place.
const TRANSFORM_AAD: &[u8] = b"AZL1 transform v2";

/// The segments' cipher: aead's STREAM over XChaCha20-Poly1305 (nonce = prefix || counter ||
/// last-segment flag).
type SegmentStream = StreamBE32<XChaCha20Poly1305>;
/// The STREAM's nonce prefix as the cipher takes it (19 bytes).
type Prefix = StreamNonce<XChaCha20Poly1305, SegmentStream>;

fn damaged(why: impl Into<String>) -> CryptoError {
    CryptoError::Damaged(why.into())
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn le_u64(bytes: &[u8]) -> u64 {
    let mut out = [0u8; 8];
    out.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(out)
}

/// The `N` bytes of a slice of exactly `N`.
fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], CryptoError> {
    bytes
        .try_into()
        .map_err(|_| damaged("a header field has the wrong length"))
}

/// A segment size this build writes and reads.
fn check_segment_size(segment_size: u32) -> Result<(), CryptoError> {
    if (MIN_SEGMENT_SIZE..=MAX_SEGMENT_SIZE).contains(&segment_size) {
        Ok(())
    } else {
        Err(CryptoError::Unsupported(format!(
            "segments of {segment_size} bytes"
        )))
    }
}

/// How many segments `size` plaintext bytes make: at least one, so an empty file is one
/// empty last segment and every object ends its STREAM with a last segment.
#[must_use]
pub fn segment_count(size: u64, segment_size: u32) -> u64 {
    size.div_ceil(u64::from(segment_size)).max(1)
}

/// The sealed trailer's bytes for `count` segments (without the length field after it).
fn trailer_len(count: u64) -> u64 {
    count * TRAILER_ENTRY as u64 + TAG_LEN as u64
}

/// The bytes of an object of `plaintext_size` bytes whose every segment is stored
/// ([`Compression::Never`]): the header, each segment's plaintext with its codec byte and tag,
/// the trailer and its length. A pack's size is planned with it before it is written.
#[must_use]
pub fn stored_object_len(plaintext_size: u64, segment_size: u32) -> u64 {
    let count = segment_count(plaintext_size, segment_size);
    HEADER_LEN as u64 + plaintext_size + count * SEGMENT_OVERHEAD + trailer_len(count) + LENGTH_FIELD
}

/// The key commitment of `file_key`: BLAKE3 in keyed mode, keyed with the file key. Finding
/// two keys with one commitment is finding a BLAKE3 collision.
fn commitment_of(file_key: &FileKey) -> blake3::Hash {
    blake3::keyed_hash(file_key.as_bytes(), COMMITMENT_INPUT)
}

/// The keys of one object, derived from its file key. The ciphers wipe their keys when
/// dropped, and the derived bytes are wiped as soon as the ciphers have them.
struct ObjectKeys {
    segments: SegmentStream,
    info: XChaCha20Poly1305,
    trailer: XChaCha20Poly1305,
}

impl ObjectKeys {
    fn derive(file_key: &FileKey, prefix: &[u8; PREFIX_LEN]) -> ObjectKeys {
        let subkey =
            |context: &str| Zeroizing::new(blake3::derive_key(context, file_key.as_bytes()));
        let segments = subkey(SEGMENT_KEY_CONTEXT);
        let info = subkey(INFO_KEY_CONTEXT);
        let trailer = subkey(TRAILER_KEY_CONTEXT);
        ObjectKeys {
            segments: SegmentStream::from_aead(
                XChaCha20Poly1305::new(Key::from_slice(&segments[..])),
                Prefix::from_slice(prefix),
            ),
            info: XChaCha20Poly1305::new(Key::from_slice(&info[..])),
            trailer: XChaCha20Poly1305::new(Key::from_slice(&trailer[..])),
        }
    }
}

/// What the header seals about the file.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Info {
    /// The file's bytes (in version 2 the original's, before the transform).
    plaintext_size: u64,
    segment_count: u64,
    object_len: u64,
    /// BLAKE3 of the file (in version 2 the original's).
    blake3: [u8; 32],
    flags: u32,
    /// The bytes the segments hold: the file in version 1, the transformed stream in version
    /// 2 (from the transform block; not part of the sealed info).
    stream_len: u64,
}

impl Info {
    fn to_bytes(self) -> [u8; INFO_LEN] {
        let mut out = [0u8; INFO_LEN];
        out[0..8].copy_from_slice(&self.plaintext_size.to_le_bytes());
        out[8..16].copy_from_slice(&self.segment_count.to_le_bytes());
        out[16..24].copy_from_slice(&self.object_len.to_le_bytes());
        out[24..56].copy_from_slice(&self.blake3);
        out[56..60].copy_from_slice(&self.flags.to_le_bytes());
        out
    }

    fn parse(bytes: &[u8]) -> Result<Info, CryptoError> {
        if bytes.len() != INFO_LEN {
            return Err(damaged("the header info has the wrong length"));
        }
        let plaintext_size = le_u64(&bytes[0..8]);
        Ok(Info {
            plaintext_size,
            segment_count: le_u64(&bytes[8..16]),
            object_len: le_u64(&bytes[16..24]),
            blake3: array(&bytes[24..56])?,
            flags: le_u32(&bytes[56..60]),
            stream_len: plaintext_size,
        })
    }

    /// Whether the info describes an object this build can read with `segment_size`.
    fn check(&self, segment_size: u32) -> Result<(), CryptoError> {
        if self.flags & !FLAG_COMPRESSED != 0 {
            return Err(CryptoError::Unsupported(String::from(
                "info flags of a newer version",
            )));
        }
        if self.segment_count > u64::from(u32::MAX)
            || self.segment_count != segment_count(self.stream_len, segment_size)
        {
            return Err(damaged("the segment count does not fit the plaintext size"));
        }
        let least = HEADER_LEN as u64
            + self.segment_count * SEGMENT_OVERHEAD
            + trailer_len(self.segment_count)
            + LENGTH_FIELD;
        if self.object_len < least {
            return Err(damaged("the object length is too small for its segments"));
        }
        Ok(())
    }

    /// The plaintext bytes of segment `index`: the segment size, less in the last one.
    fn segment_plain_len(&self, segment_size: u64, index: u64) -> u64 {
        if index + 1 < self.segment_count {
            segment_size
        } else {
            self.stream_len - (self.segment_count - 1) * segment_size
        }
    }
}

/// What a version 2 header seals about the transform: which one, and the stream it made.
#[derive(Clone, Copy, PartialEq, Eq)]
struct TransformBlock {
    id: TransformId,
    /// The transformed stream's bytes: what the segments hold.
    transformed_len: u64,
    /// BLAKE3 of the transformed stream.
    blake3: [u8; 32],
}

impl TransformBlock {
    fn to_bytes(self) -> [u8; TRANSFORM_LEN] {
        let mut out = [0u8; TRANSFORM_LEN];
        let (id, version) = self.id.bytes();
        out[0] = id;
        out[1] = version;
        out[8..16].copy_from_slice(&self.transformed_len.to_le_bytes());
        out[16..48].copy_from_slice(&self.blake3);
        out
    }

    /// The block of a file of `plaintext_size` bytes: a transform this build reads, a stream
    /// no longer than that transform can make of such a file.
    fn parse(bytes: &[u8], plaintext_size: u64) -> Result<TransformBlock, CryptoError> {
        if bytes.len() != TRANSFORM_LEN {
            return Err(damaged("the transform block has the wrong length"));
        }
        let id = TransformId::from_bytes(bytes[0], bytes[1])?;
        if id == TransformId::None || bytes[2..8].iter().any(|b| *b != 0) {
            return Err(CryptoError::Unsupported(String::from(
                "a transform block of a newer version",
            )));
        }
        // Transforms run on files up to this size only: a larger one is a newer writer's.
        if plaintext_size > Limits::default().max_transform_input {
            return Err(CryptoError::Unsupported(String::from(
                "a transformed file larger than this version transforms",
            )));
        }
        let transformed_len = le_u64(&bytes[8..16]);
        if transformed_len > transform::max_transformed_len(id, plaintext_size) {
            return Err(damaged("the transformed stream is longer than its file allows"));
        }
        Ok(TransformBlock {
            id,
            transformed_len,
            blake3: array(&bytes[16..48])?,
        })
    }
}

/// The associated data of the transform block: the header context, then [`TRANSFORM_AAD`].
fn transform_aad(context: &[u8; CONTEXT_LEN]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(CONTEXT_LEN + TRANSFORM_AAD.len());
    aad.extend_from_slice(context);
    aad.extend_from_slice(TRANSFORM_AAD);
    aad
}

/// Bytes 0..88 of a header of format `version`.
fn header_context(
    version: u16,
    segment_size: u32,
    object_id: &ObjectId,
    prefix: &[u8; PREFIX_LEN],
    commitment: &blake3::Hash,
) -> [u8; CONTEXT_LEN] {
    let mut context = [0u8; CONTEXT_LEN];
    context[..MAGIC.len()].copy_from_slice(&MAGIC);
    context[OFF_VERSION..OFF_VERSION + 2].copy_from_slice(&version.to_le_bytes());
    // The header flags (bytes 6..8) stay zero in versions 1 and 2.
    context[OFF_HEADER_LEN..OFF_HEADER_LEN + 4].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
    context[OFF_SEGMENT_SIZE..OFF_SEGMENT_SIZE + 4].copy_from_slice(&segment_size.to_le_bytes());
    context[OFF_OBJECT_ID..OFF_OBJECT_ID + OBJECT_ID_LEN].copy_from_slice(&object_id.0);
    context[OFF_PREFIX..OFF_PREFIX + PREFIX_LEN].copy_from_slice(prefix);
    context[OFF_COMMITMENT..OFF_COMMITMENT + COMMITMENT_LEN].copy_from_slice(commitment.as_bytes());
    context
}

/// How an object is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Plaintext bytes per segment, [`MIN_SEGMENT_SIZE`] to [`MAX_SEGMENT_SIZE`] (1 MiB by
    /// default).
    pub segment_size: u32,
    pub compression: Compression,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            segment_size: DEFAULT_SEGMENT_SIZE,
            compression: Compression::Auto,
        }
    }
}

/// What a writer knows of a file before its first byte; the transform and the codecs use it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileHint {
    /// The name extension ("jpg"), the detection's tie-break.
    pub extension: Option<String>,
    /// The file's bytes, when known: the codecs' time budget grows with them, and a stream
    /// known to be larger than a transform takes is not read into memory for one.
    pub len: Option<u64>,
}

impl FileHint {
    /// The hint of the drive path `path` (its extension), `len` bytes long when known.
    #[must_use]
    pub fn for_path(path: &str, len: Option<u64>) -> FileHint {
        FileHint {
            extension: codec::extension_of(path).map(str::to_ascii_lowercase),
            len,
        }
    }
}

/// What a finished object is: what the drive's index keeps of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSummary {
    pub object_id: ObjectId,
    /// The file's bytes (its size before compression and transform).
    pub plaintext_size: u64,
    /// The object's bytes in the bucket (compressed and encrypted: what the quota counts).
    pub object_len: u64,
    /// The segments of the stored stream (in version 2: of the transformed stream).
    pub segment_count: u64,
    /// BLAKE3 of the plaintext.
    pub blake3: [u8; 32],
    /// Whether the object holds fewer bytes than the file: a segment is compressed, or the
    /// file was transformed.
    pub compressed: bool,
    /// The whole-file transform the object holds (a version 2 object); `None`: version 1.
    pub transform: Option<TransformId>,
    /// The file key wrapped with the drive key: the same wrap as in the header.
    pub wrapped_key: WrappedKey,
}

/// A finished [`Azl1Writer`]: its sink (holding the object from byte [`HEADER_LEN`] on), the
/// header for bytes `0..HEADER_LEN`, and what the object is.
pub struct Finished<W> {
    pub sink: W,
    pub header: Box<[u8; HEADER_LEN]>,
    pub summary: ObjectSummary,
}

/// The original file of a transformed stream: what a version 2 info seals.
#[derive(Clone, Copy)]
struct Original {
    id: TransformId,
    len: u64,
    blake3: [u8; 32],
}

/// A transformed stream about to be written: its file, the kind the codecs treat it as, and
/// its length.
#[derive(Clone, Copy)]
struct Transforming {
    original: Original,
    kind: Kind,
    stream_len: u64,
}

/// Writes one new object, a segment at a time, whatever the file's size: give it the
/// plaintext through `io::Write`, then [`Azl1Writer::finish`]. The sink receives the segments
/// and the trailer (the object from byte [`HEADER_LEN`] on); the header comes back at the end,
/// for the object's first [`HEADER_LEN`] bytes. A writer that failed refuses further writes.
/// It writes version 1 objects; the whole-file transforms are [`encrypt`]'s and
/// [`encrypt_stream`]'s, which need the whole file.
pub struct Azl1Writer<W: Write> {
    sink: W,
    object_id: ObjectId,
    wrapped_key: WrappedKey,
    keys: ObjectKeys,
    context: [u8; CONTEXT_LEN],
    segment_size: usize,
    /// The plaintext of the segment being filled (never more than one segment; wiped).
    pending: Zeroizing<Vec<u8>>,
    /// The sealed length of every segment written.
    lengths: Vec<u32>,
    hasher: blake3::Hasher,
    /// Bytes given to the writer: the file, or its transformed stream.
    plaintext_size: u64,
    /// Bytes handed to the sink.
    written: u64,
    encoder: Encoder,
    compressed: bool,
    failed: bool,
    /// The file a version 2 object's stream was made from.
    original: Option<Original>,
}

impl<W: Write> Azl1Writer<W> {
    /// A writer of the new object `object_id` into `sink`: a fresh random file key (wrapped
    /// with `drive_key` at once and then kept only as its derived keys), a fresh nonce prefix.
    pub fn new(
        sink: W,
        object_id: ObjectId,
        drive_key: &DriveKey,
        options: &WriteOptions,
    ) -> Result<Azl1Writer<W>, CryptoError> {
        Azl1Writer::start(sink, object_id, drive_key, options, &FileHint::default(), None)
    }

    /// [`Azl1Writer::new`] with what is known of the file; with `transforming`, the writer of
    /// a version 2 object whose stream is the transformed file.
    fn start(
        sink: W,
        object_id: ObjectId,
        drive_key: &DriveKey,
        options: &WriteOptions,
        hint: &FileHint,
        transforming: Option<Transforming>,
    ) -> Result<Azl1Writer<W>, CryptoError> {
        check_segment_size(options.segment_size)?;
        let file_key = FileKey::generate()?;
        let mut prefix = [0u8; PREFIX_LEN];
        random_bytes(&mut prefix)?;
        let wrapped_key = drive_key.wrap_file_key(&file_key, &object_id)?;
        let version = if transforming.is_some() {
            VERSION_TRANSFORMED
        } else {
            VERSION
        };
        let context = header_context(
            version,
            options.segment_size,
            &object_id,
            &prefix,
            &commitment_of(&file_key),
        );
        let segment_size = options.segment_size as usize;
        let codec_options = options
            .compression
            .options(options.segment_size, hint.extension.as_deref());
        let encoder = match &transforming {
            Some(t) => Encoder::new(codec_options, Some(t.kind), Some(t.stream_len)),
            None => Encoder::new(codec_options, None, hint.len),
        };
        Ok(Azl1Writer {
            sink,
            object_id,
            wrapped_key,
            keys: ObjectKeys::derive(&file_key, &prefix),
            context,
            segment_size,
            pending: Zeroizing::new(Vec::with_capacity(segment_size)),
            lengths: Vec::new(),
            hasher: blake3::Hasher::new(),
            plaintext_size: 0,
            written: 0,
            encoder,
            compressed: false,
            failed: false,
            original: transforming.map(|t| t.original),
        })
    }

    /// Seals the pending segment (the last one when `last`) and hands it to the sink.
    fn seal_pending(&mut self, last: bool) -> Result<(), CryptoError> {
        let index = u32::try_from(self.lengths.len())
            .ok()
            .filter(|index| *index < u32::MAX)
            .ok_or_else(|| CryptoError::Unsupported(String::from("over 4 billion segments")))?;
        let encoded = self.encoder.encode(&self.pending)?;
        let data: &[u8] = match &encoded {
            Encoded::Stored => &self.pending,
            Encoded::Compressed { data, .. } => data,
        };
        // The capacity holds the tag too: the buffer never moves (and never leaves a copy).
        let mut segment =
            Zeroizing::new(Vec::with_capacity(data.len() + SEGMENT_OVERHEAD as usize));
        segment.push(encoded.codec().byte());
        segment.extend_from_slice(data);
        // Nonce: the prefix, `index` (32 bits, big endian) and the last-segment flag.
        self.keys
            .segments
            .encrypt_in_place(index, last, &self.context, &mut *segment)
            .map_err(|_| damaged("the cipher refused a segment"))?;
        let sealed_len = u32::try_from(segment.len())
            .map_err(|_| CryptoError::Unsupported(String::from("a segment over 4 GiB")))?;
        self.sink.write_all(&segment)?;
        self.lengths.push(sealed_len);
        self.written += segment.len() as u64;
        if encoded.compressed().is_some() {
            self.compressed = true;
        }
        self.pending.zeroize();
        Ok(())
    }

    /// The header of the finished object (with the transform block of a version 2 one).
    fn header(
        &self,
        info: Info,
        trailer_nonce: &[u8; NONCE_LEN],
        block: Option<TransformBlock>,
    ) -> Result<Box<[u8; HEADER_LEN]>, CryptoError> {
        let mut info_nonce = [0u8; NONCE_LEN];
        random_bytes(&mut info_nonce)?;
        let sealed_info = self
            .keys
            .info
            .encrypt(
                XNonce::from_slice(&info_nonce),
                Payload {
                    msg: &info.to_bytes(),
                    aad: &self.context,
                },
            )
            .map_err(|_| damaged("the cipher refused the header info"))?;
        if sealed_info.len() != INFO_LEN + TAG_LEN {
            return Err(damaged("the sealed header info has the wrong length"));
        }
        let mut header = Box::new([0u8; HEADER_LEN]);
        header[..CONTEXT_LEN].copy_from_slice(&self.context);
        header[OFF_WRAP..OFF_WRAP + WrappedKey::LEN].copy_from_slice(&self.wrapped_key.to_bytes());
        header[OFF_INFO_NONCE..OFF_INFO_NONCE + NONCE_LEN].copy_from_slice(&info_nonce);
        header[OFF_INFO..OFF_INFO + INFO_LEN + TAG_LEN].copy_from_slice(&sealed_info);
        header[OFF_TRAILER_NONCE..OFF_TRAILER_NONCE + NONCE_LEN].copy_from_slice(trailer_nonce);
        if let Some(block) = block {
            let mut nonce = [0u8; NONCE_LEN];
            random_bytes(&mut nonce)?;
            let sealed = self
                .keys
                .info
                .encrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: &block.to_bytes(),
                        aad: &transform_aad(&self.context),
                    },
                )
                .map_err(|_| damaged("the cipher refused the transform block"))?;
            if sealed.len() != TRANSFORM_LEN + TAG_LEN {
                return Err(damaged("the sealed transform block has the wrong length"));
            }
            header[OFF_TRANSFORM_NONCE..OFF_TRANSFORM_NONCE + NONCE_LEN].copy_from_slice(&nonce);
            header[OFF_TRANSFORM..HEADER_USED_TRANSFORMED].copy_from_slice(&sealed);
        }
        Ok(header)
    }

    /// Seals the last segment (an empty one for an empty file), writes the trailer and its
    /// length, and returns the header for the object's first [`HEADER_LEN`] bytes.
    pub fn finish(mut self) -> Result<Finished<W>, CryptoError> {
        if self.failed {
            return Err(CryptoError::Io(String::from(
                "the object writer failed before",
            )));
        }
        self.seal_pending(true)?;
        let segment_count = self.lengths.len() as u64;
        let mut table = Vec::with_capacity(self.lengths.len() * TRAILER_ENTRY);
        for len in &self.lengths {
            table.extend_from_slice(&len.to_le_bytes());
        }
        let mut trailer_nonce = [0u8; NONCE_LEN];
        random_bytes(&mut trailer_nonce)?;
        let trailer = self
            .keys
            .trailer
            .encrypt(
                XNonce::from_slice(&trailer_nonce),
                Payload {
                    msg: &table,
                    aad: &self.context,
                },
            )
            .map_err(|_| damaged("the cipher refused the trailer"))?;
        self.sink.write_all(&trailer)?;
        self.sink.write_all(&(trailer.len() as u64).to_le_bytes())?;
        self.sink.flush()?;
        let stream_hash = *self.hasher.finalize().as_bytes();
        // Version 2: the info names the original file, the block the stream the segments hold.
        let (plaintext_size, blake3, block) = match self.original {
            Some(original) => (
                original.len,
                original.blake3,
                Some(TransformBlock {
                    id: original.id,
                    transformed_len: self.plaintext_size,
                    blake3: stream_hash,
                }),
            ),
            None => (self.plaintext_size, stream_hash, None),
        };
        let info = Info {
            plaintext_size,
            segment_count,
            object_len: HEADER_LEN as u64 + self.written + trailer.len() as u64 + LENGTH_FIELD,
            blake3,
            flags: if self.compressed { FLAG_COMPRESSED } else { 0 },
            stream_len: self.plaintext_size,
        };
        let header = self.header(info, &trailer_nonce, block)?;
        let summary = ObjectSummary {
            object_id: self.object_id,
            plaintext_size: info.plaintext_size,
            object_len: info.object_len,
            segment_count,
            blake3: info.blake3,
            compressed: self.compressed || block.is_some(),
            transform: block.map(|block| block.id),
            wrapped_key: self.wrapped_key.clone(),
        };
        Ok(Finished {
            sink: self.sink,
            header,
            summary,
        })
    }
}

impl<W: Write> Write for Azl1Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("the object writer failed before"));
        }
        let mut rest = buf;
        while !rest.is_empty() {
            if self.pending.len() == self.segment_size {
                // More bytes follow, so the full segment is not the last one.
                if let Err(e) = self.seal_pending(false) {
                    self.failed = true;
                    return Err(e.into());
                }
            }
            let take = (self.segment_size - self.pending.len()).min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            self.hasher.update(&rest[..take]);
            self.plaintext_size += take as u64;
            rest = &rest[take..];
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

/// The verified whole-file transform of `file` under `options`, when its kind has one that
/// pays (azlin-compress decides, and has decoded it and compared it with `file` already).
fn transform_of(
    file: &[u8],
    options: &WriteOptions,
    hint: &FileHint,
) -> Option<(transform::Transformed, Transforming)> {
    let codec_options = options
        .compression
        .options(options.segment_size, hint.extension.as_deref())?;
    let start = &file[..file.len().min(codec::SNIFF_LEN)];
    let plan = azlin_compress::plan(start, file.len() as u64, &codec_options);
    if plan.transform.is_none() {
        return None;
    }
    let forward = {
        let _slot = TransformSlot::take();
        transform::forward(file, plan.kind, &codec_options)
    };
    match forward {
        Forward::Applied(transformed) if transformed.id != TransformId::None => {
            let transforming = Transforming {
                original: Original {
                    id: transformed.id,
                    len: file.len() as u64,
                    blake3: *blake3::hash(file).as_bytes(),
                },
                kind: transform::codec_kind(plan.kind, transformed.id),
                stream_len: transformed.bytes.len() as u64,
            };
            Some((transformed, transforming))
        }
        Forward::Applied(_) | Forward::NotApplied(_) => None,
    }
}

/// `stream` sealed into a new object in memory (`transforming`: as the transformed stream of a
/// version 2 object).
fn seal_in_memory(
    stream: &[u8],
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
    hint: &FileHint,
    transforming: Option<Transforming>,
) -> Result<(Vec<u8>, ObjectSummary), CryptoError> {
    let mut sink = vec![0u8; HEADER_LEN];
    sink.reserve(stream.len() + stream.len() / 4096 + 1024);
    let mut writer = Azl1Writer::start(sink, object_id, drive_key, options, hint, transforming)?;
    writer.write_all(stream)?;
    let Finished {
        mut sink,
        header,
        summary,
    } = writer.finish()?;
    sink[..HEADER_LEN].copy_from_slice(&header[..]);
    Ok((sink, summary))
}

/// A new object of `plaintext` in memory, its file key wrapped with `drive_key`.
pub fn encrypt(
    plaintext: &[u8],
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
) -> Result<(Vec<u8>, ObjectSummary), CryptoError> {
    encrypt_as(plaintext, object_id, drive_key, options, &FileHint::default())
}

/// [`encrypt`] with what is known of the file (its name's extension). A JPEG, PDF, Office,
/// ZIP, PNG or gzip file whose transform pays becomes a version 2 object.
pub fn encrypt_as(
    plaintext: &[u8],
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
    hint: &FileHint,
) -> Result<(Vec<u8>, ObjectSummary), CryptoError> {
    let hint = FileHint {
        len: Some(plaintext.len() as u64),
        ..hint.clone()
    };
    match transform_of(plaintext, options, &hint) {
        Some((transformed, transforming)) => seal_in_memory(
            &transformed.bytes,
            object_id,
            drive_key,
            options,
            &hint,
            Some(transforming),
        ),
        None => seal_in_memory(plaintext, object_id, drive_key, options, &hint, None),
    }
}

/// Streams `source` into a new object at the sink's position: room for the header first, the
/// segments and the trailer as the source is read, then the header over the room. One
/// segment in memory, whatever the size; the sink is left at the object's end.
pub fn encrypt_stream<R: Read + ?Sized, W: Write + Seek>(
    source: &mut R,
    sink: &mut W,
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
) -> Result<ObjectSummary, CryptoError> {
    encrypt_stream_as(source, sink, object_id, drive_key, options, &FileHint::default())
}

/// [`encrypt_stream`] with what is known of the file. A file whose first bytes name a kind with
/// a whole-file transform (a JPEG, PDF, Office, ZIP, PNG or gzip file) is read into memory when
/// it ends within the transforms' 64 MiB, and becomes a version 2 object when its transform
/// pays; every other file streams with one segment in memory.
pub fn encrypt_stream_as<R: Read + ?Sized, W: Write + Seek>(
    source: &mut R,
    sink: &mut W,
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
    hint: &FileHint,
) -> Result<ObjectSummary, CryptoError> {
    let start = sink.stream_position()?;
    sink.write_all(&[0u8; HEADER_LEN])?;
    let mut hint = hint.clone();
    let mut head: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
    let mut transformed = None;
    if let Some(codec_options) = options
        .compression
        .options(options.segment_size, hint.extension.as_deref())
    {
        (&mut *source)
            .take(codec::SNIFF_LEN as u64)
            .read_to_end(&mut head)?;
        let max = codec_options.limits.max_transform_input;
        // An unknown length is tried: the stream is read until it ends or passes the cap.
        let len = hint.len.unwrap_or(max);
        if azlin_compress::plan(&head, len, &codec_options).transform.is_some() {
            let rest = (max + 1).saturating_sub(head.len() as u64);
            (&mut *source).take(rest).read_to_end(&mut head)?;
            if head.len() as u64 <= max {
                // The whole file is in memory.
                hint.len = Some(head.len() as u64);
                transformed = transform_of(&head, options, &hint);
            }
        }
    }
    let finished = match transformed {
        Some((transformed, transforming)) => {
            let mut writer = Azl1Writer::start(
                &mut *sink,
                object_id,
                drive_key,
                options,
                &hint,
                Some(transforming),
            )?;
            writer.write_all(&transformed.bytes)?;
            writer.finish()?
        }
        None => {
            let mut writer =
                Azl1Writer::start(&mut *sink, object_id, drive_key, options, &hint, None)?;
            writer.write_all(&head)?;
            io::copy(source, &mut writer)?;
            writer.finish()?
        }
    };
    let Finished {
        header, summary, ..
    } = finished;
    sink.seek(SeekFrom::Start(start))?;
    sink.write_all(&header[..])?;
    sink.seek(SeekFrom::Start(start + summary.object_len))?;
    sink.flush()?;
    Ok(summary)
}

/// The bytes of an object, by range: a slice in memory, a drive's object (ranged GETs).
pub trait ObjectSource {
    /// A failed read: [`CryptoError`] for bytes in memory; for a drive, its own error or the
    /// format's.
    type Error: From<CryptoError>;
    /// Exactly `len` bytes from `offset` of the object.
    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, Self::Error>;
}

impl ObjectSource for [u8] {
    type Error = CryptoError;

    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, CryptoError> {
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= self.len() as u64)
            .ok_or_else(|| damaged("the object is cut short"))?;
        Ok(self[offset as usize..end as usize].to_vec())
    }
}

impl ObjectSource for Vec<u8> {
    type Error = CryptoError;

    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, CryptoError> {
        self.as_slice().read_at(offset, len)
    }
}

impl<T: ObjectSource + ?Sized> ObjectSource for &T {
    type Error = T::Error;

    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, T::Error> {
        (**self).read_at(offset, len)
    }
}

/// The public part of an object's header: what anyone holding the object can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicHeader {
    /// Plaintext bytes per segment.
    pub segment_size: u32,
    /// The object the header names.
    pub object_id: ObjectId,
    /// The file key, wrapped with the drive key that wrote the object.
    pub wrapped_key: WrappedKey,
    /// The format version: [`VERSION`], or [`VERSION_TRANSFORMED`] for a transformed file.
    pub version: u16,
    context: [u8; CONTEXT_LEN],
    prefix: [u8; PREFIX_LEN],
    commitment: [u8; COMMITMENT_LEN],
    info_nonce: [u8; NONCE_LEN],
    sealed_info: [u8; INFO_LEN + TAG_LEN],
    trailer_nonce: [u8; NONCE_LEN],
    /// Version 2: the transform block's nonce and its sealed bytes.
    sealed_transform: Option<([u8; NONCE_LEN], [u8; TRANSFORM_LEN + TAG_LEN])>,
}

impl PublicHeader {
    /// The nonce prefix of the object's segments: with the file key, what opens a stored-only
    /// object's segments without its header ([`StoredSegments`]).
    #[must_use]
    pub fn prefix(&self) -> [u8; PREFIX_LEN] {
        self.prefix
    }

    /// The header in the first [`HEADER_LEN`] bytes of `bytes`: its magic, version, length and
    /// segment size checked, its unused bytes zero.
    pub fn parse(bytes: &[u8]) -> Result<PublicHeader, CryptoError> {
        let Some(bytes) = bytes.get(..HEADER_LEN) else {
            return Err(damaged("the object is shorter than an AZL1 header"));
        };
        if bytes[..MAGIC.len()] != MAGIC[..] {
            return Err(damaged("not an AZL1 object"));
        }
        let version = le_u16(&bytes[OFF_VERSION..]);
        let used = match version {
            VERSION => HEADER_USED,
            VERSION_TRANSFORMED => HEADER_USED_TRANSFORMED,
            other => return Err(CryptoError::Unsupported(format!("AZL1 version {other}"))),
        };
        if le_u16(&bytes[OFF_FLAGS..]) != 0 {
            return Err(CryptoError::Unsupported(String::from(
                "header flags of a newer version",
            )));
        }
        let header_len = le_u32(&bytes[OFF_HEADER_LEN..]);
        if header_len as usize != HEADER_LEN {
            return Err(CryptoError::Unsupported(format!(
                "a header of {header_len} bytes"
            )));
        }
        let segment_size = le_u32(&bytes[OFF_SEGMENT_SIZE..]);
        check_segment_size(segment_size)?;
        let unused = bytes[OFF_PREFIX + PREFIX_LEN..OFF_COMMITMENT]
            .iter()
            .chain(&bytes[used..]);
        if unused.copied().any(|b| b != 0) {
            return Err(damaged("the header's unused bytes are not zero"));
        }
        let wrapped_key = WrappedKey::from_bytes(&bytes[OFF_WRAP..OFF_WRAP + WrappedKey::LEN])
            .ok_or_else(|| damaged("the wrapped file key"))?;
        let sealed_transform = if version == VERSION_TRANSFORMED {
            Some((
                array(&bytes[OFF_TRANSFORM_NONCE..OFF_TRANSFORM_NONCE + NONCE_LEN])?,
                array(&bytes[OFF_TRANSFORM..HEADER_USED_TRANSFORMED])?,
            ))
        } else {
            None
        };
        Ok(PublicHeader {
            segment_size,
            object_id: ObjectId(array(&bytes[OFF_OBJECT_ID..OFF_OBJECT_ID + OBJECT_ID_LEN])?),
            wrapped_key,
            version,
            sealed_transform,
            context: array(&bytes[..CONTEXT_LEN])?,
            prefix: array(&bytes[OFF_PREFIX..OFF_PREFIX + PREFIX_LEN])?,
            commitment: array(&bytes[OFF_COMMITMENT..OFF_COMMITMENT + COMMITMENT_LEN])?,
            info_nonce: array(&bytes[OFF_INFO_NONCE..OFF_INFO_NONCE + NONCE_LEN])?,
            sealed_info: array(&bytes[OFF_INFO..OFF_INFO + INFO_LEN + TAG_LEN])?,
            trailer_nonce: array(&bytes[OFF_TRAILER_NONCE..OFF_TRAILER_NONCE + NONCE_LEN])?,
        })
    }

    /// The commitment, the keys, the info and (version 2) the transform block, for
    /// `object_id` under `file_key`.
    fn open(
        &self,
        object_id: &ObjectId,
        file_key: &FileKey,
    ) -> Result<(ObjectKeys, Info, Option<TransformBlock>), CryptoError> {
        if self.object_id != *object_id {
            return Err(damaged("the header names another object"));
        }
        // The key commitment first (constant time): nothing is decrypted under a key the
        // object does not commit to.
        if commitment_of(file_key) != self.commitment {
            return Err(CryptoError::KeyMismatch);
        }
        let keys = ObjectKeys::derive(file_key, &self.prefix);
        let info = keys
            .info
            .decrypt(
                XNonce::from_slice(&self.info_nonce),
                Payload {
                    msg: &self.sealed_info,
                    aad: &self.context,
                },
            )
            .map_err(|_| damaged("the header does not authenticate"))?;
        let mut info = Info::parse(&info)?;
        let block = match &self.sealed_transform {
            None => None,
            Some((nonce, sealed)) => {
                let plain = keys
                    .info
                    .decrypt(
                        XNonce::from_slice(nonce),
                        Payload {
                            msg: sealed,
                            aad: &transform_aad(&self.context),
                        },
                    )
                    .map_err(|_| damaged("the transform block does not authenticate"))?;
                let block = TransformBlock::parse(&plain, info.plaintext_size)?;
                info.stream_len = block.transformed_len;
                Some(block)
            }
        };
        info.check(self.segment_size)?;
        Ok((keys, info, block))
    }

    /// Where every segment starts, then where the trailer starts, from the object's tail (its
    /// last `trailer + 8` bytes, read from `tail_start`).
    fn segment_offsets(
        &self,
        keys: &ObjectKeys,
        info: &Info,
        tail: &[u8],
        tail_start: u64,
    ) -> Result<Vec<u64>, CryptoError> {
        let trailer = trailer_len(info.segment_count) as usize;
        if tail.len() != trailer + LENGTH_FIELD as usize {
            return Err(damaged("the object is cut short"));
        }
        if le_u64(&tail[trailer..]) != trailer as u64 {
            return Err(damaged("the trailer's length does not match the header"));
        }
        let table = keys
            .trailer
            .decrypt(
                XNonce::from_slice(&self.trailer_nonce),
                Payload {
                    msg: &tail[..trailer],
                    aad: &self.context,
                },
            )
            .map_err(|_| damaged("the trailer does not authenticate"))?;
        let segment_size = u64::from(self.segment_size);
        let mut offsets = Vec::with_capacity(table.len() / TRAILER_ENTRY + 1);
        let mut at = HEADER_LEN as u64;
        for (index, entry) in table.chunks_exact(TRAILER_ENTRY).enumerate() {
            let len = u64::from(le_u32(entry));
            let plain = info.segment_plain_len(segment_size, index as u64);
            if !(SEGMENT_OVERHEAD..=plain + SEGMENT_OVERHEAD).contains(&len) {
                return Err(damaged(format!("segment {index} has an impossible length")));
            }
            offsets.push(at);
            at += len;
        }
        offsets.push(at);
        if offsets.len() as u64 != info.segment_count + 1 || at != tail_start {
            return Err(damaged("the segment lengths do not add up to the object"));
        }
        Ok(offsets)
    }
}

/// An object opened with its file key: header, commitment, info and trailer checked. Small;
/// keep it to read more ranges of the object without reading its header and trailer again.
pub struct OpenObject {
    object_id: ObjectId,
    keys: ObjectKeys,
    context: [u8; CONTEXT_LEN],
    segment_size: u64,
    info: Info,
    /// Version 2: the transform the segments' stream goes back through.
    transform: Option<TransformBlock>,
    /// Where each segment starts in the object, then where the trailer starts.
    offsets: Vec<u64>,
}

impl fmt::Debug for OpenObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenObject")
            .field("object_id", &self.object_id)
            .field("segment_size", &self.segment_size)
            .field("segment_count", &self.info.segment_count)
            .field("object_len", &self.info.object_len)
            .finish_non_exhaustive()
    }
}

impl OpenObject {
    /// Reads the header and the tail of `object_id` from `source` (two reads) and opens them
    /// with `file_key` (from the drive's index, or a share).
    pub fn open<S: ObjectSource + ?Sized>(
        source: &S,
        object_id: &ObjectId,
        file_key: &FileKey,
    ) -> Result<OpenObject, S::Error> {
        let header = PublicHeader::parse(&source.read_at(0, HEADER_LEN as u64)?)?;
        OpenObject::with_header(source, &header, object_id, file_key)
    }

    /// The same with the file key from the object's own header, unwrapped by `drive_key`.
    pub fn open_with_drive_key<S: ObjectSource + ?Sized>(
        source: &S,
        object_id: &ObjectId,
        drive_key: &DriveKey,
    ) -> Result<OpenObject, S::Error> {
        let header = PublicHeader::parse(&source.read_at(0, HEADER_LEN as u64)?)?;
        let file_key = drive_key.unwrap_file_key(&header.wrapped_key, object_id)?;
        OpenObject::with_header(source, &header, object_id, &file_key)
    }

    /// With the header read already: the commitment, the info, then the tail (one read).
    pub fn with_header<S: ObjectSource + ?Sized>(
        source: &S,
        header: &PublicHeader,
        object_id: &ObjectId,
        file_key: &FileKey,
    ) -> Result<OpenObject, S::Error> {
        let (keys, info, transform) = header.open(object_id, file_key)?;
        let trailer = trailer_len(info.segment_count);
        // `Info::check` made sure the object holds the trailer and its length.
        let tail_start = info.object_len - LENGTH_FIELD - trailer;
        let tail = source.read_at(tail_start, trailer + LENGTH_FIELD)?;
        let offsets = header.segment_offsets(&keys, &info, &tail, tail_start)?;
        Ok(OpenObject {
            object_id: *object_id,
            keys,
            context: header.context,
            segment_size: u64::from(header.segment_size),
            info,
            transform,
            offsets,
        })
    }

    #[must_use]
    pub fn object_id(&self) -> ObjectId {
        self.object_id
    }

    /// The file's bytes.
    #[must_use]
    pub fn plaintext_size(&self) -> u64 {
        self.info.plaintext_size
    }

    /// The object's bytes in the bucket.
    #[must_use]
    pub fn object_len(&self) -> u64 {
        self.info.object_len
    }

    #[must_use]
    pub fn segment_count(&self) -> u64 {
        self.info.segment_count
    }

    /// BLAKE3 of the plaintext.
    #[must_use]
    pub fn blake3(&self) -> [u8; 32] {
        self.info.blake3
    }

    /// Whether a segment is compressed.
    #[must_use]
    pub fn compressed(&self) -> bool {
        self.info.flags & FLAG_COMPRESSED != 0
    }

    /// The whole-file transform of a version 2 object; `None` for version 1.
    #[must_use]
    pub fn transform(&self) -> Option<TransformId> {
        self.transform.map(|block| block.id)
    }

    /// Where segment `index` lies in the object: its offset and its sealed length.
    #[must_use]
    pub fn segment_span(&self, index: u64) -> (u64, u64) {
        let i = index as usize;
        (self.offsets[i], self.offsets[i + 1] - self.offsets[i])
    }

    /// The plaintext of segment `index` from its sealed bytes.
    fn open_segment(&self, index: u64, sealed: Vec<u8>) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let last = index + 1 == self.info.segment_count;
        let expected = self.info.segment_plain_len(self.segment_size, index);
        open_sealed_segment(&self.keys, &self.context, index, last, expected, sealed)
    }

    /// The plaintext of segments `first..=last`, whose sealed bytes are `sealed` (they start
    /// at segment `first`); also hashed into `hasher` when one is given.
    fn decrypt_segments(
        &self,
        sealed: &[u8],
        first: u64,
        last: u64,
        mut hasher: Option<&mut blake3::Hasher>,
        mut each: impl FnMut(u64, &[u8]),
    ) -> Result<(), CryptoError> {
        let base = self.offsets[first as usize];
        let expected = self.offsets[last as usize + 1] - base;
        if sealed.len() as u64 != expected {
            return Err(damaged("the object is cut short"));
        }
        for index in first..=last {
            let (offset, len) = self.segment_span(index);
            let from = (offset - base) as usize;
            let plain = self.open_segment(index, sealed[from..from + len as usize].to_vec())?;
            if let Some(hasher) = hasher.as_deref_mut() {
                hasher.update(plain.as_slice());
            }
            each(index, plain.as_slice());
        }
        Ok(())
    }

    /// The plaintext bytes `start..=end` (an `end` past the file means to its end; a `start`
    /// past it gives nothing): the segments covering them, in ONE read of the source. A version
    /// 2 object (a transformed file, at most 64 MiB) is read and decoded whole for any range.
    pub fn read_range<S: ObjectSource + ?Sized>(
        &self,
        source: &S,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, S::Error> {
        let size = self.info.plaintext_size;
        if start >= size || end < start {
            return Ok(Vec::new());
        }
        let end = end.min(size - 1);
        if self.transform.is_some() {
            let whole = Zeroizing::new(self.read_all(source)?);
            return Ok(whole[start as usize..=end as usize].to_vec());
        }
        let first = start / self.segment_size;
        let last = end / self.segment_size;
        let from = self.offsets[first as usize];
        let sealed = source.read_at(from, self.offsets[last as usize + 1] - from)?;
        let mut out = Vec::with_capacity(usize::try_from(end - start + 1).unwrap_or(0));
        self.decrypt_segments(&sealed, first, last, None, |index, plain| {
            let segment_start = index * self.segment_size;
            let lo = (start.max(segment_start) - segment_start) as usize;
            let hi = (end.min(segment_start + plain.len() as u64 - 1) - segment_start) as usize;
            out.extend_from_slice(&plain[lo..=hi]);
        })?;
        Ok(out)
    }

    /// The whole plaintext (one read of every segment), checked against its BLAKE3.
    pub fn read_all<S: ObjectSource + ?Sized>(&self, source: &S) -> Result<Vec<u8>, S::Error> {
        let from = HEADER_LEN as u64;
        let sealed = source.read_at(from, self.offsets[self.offsets.len() - 1] - from)?;
        Ok(self.decrypt_body(&sealed)?)
    }

    /// The whole plaintext from the object's segments (`body`: from byte [`HEADER_LEN`] to
    /// the trailer), checked against its BLAKE3. In version 2 the segments give the
    /// transformed stream, checked against the transform block's BLAKE3 and decoded, never
    /// past the sealed size.
    fn decrypt_body(&self, body: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut stream = Zeroizing::new(Vec::with_capacity(
            usize::try_from(self.info.stream_len).unwrap_or(0),
        ));
        let mut hasher = blake3::Hasher::new();
        self.decrypt_segments(
            body,
            0,
            self.info.segment_count - 1,
            Some(&mut hasher),
            |_, plain| stream.extend_from_slice(plain),
        )?;
        let Some(block) = self.transform else {
            if hasher.finalize() != self.info.blake3 {
                return Err(damaged("the plaintext does not match its hash"));
            }
            return Ok(std::mem::take(&mut *stream));
        };
        if hasher.finalize() != block.blake3 {
            return Err(damaged("the transformed stream does not match its hash"));
        }
        let limits = Limits {
            max_decoded_len: self.info.plaintext_size,
            ..Limits::default()
        };
        let mut out = Vec::with_capacity(usize::try_from(self.info.plaintext_size).unwrap_or(0));
        {
            let _slot = TransformSlot::take();
            transform::inverse(block.id, &mut stream.as_slice(), &mut out, &limits, decode_threads())?;
        }
        if out.len() as u64 != self.info.plaintext_size
            || blake3::hash(&out) != self.info.blake3
        {
            out.zeroize();
            return Err(damaged("the decoded file does not match its hash"));
        }
        Ok(out)
    }

    /// A reader of the plaintext from `source`, a segment at a time (a version 2 object: read
    /// and decoded whole at the first read).
    pub fn into_reader<S: ObjectSource>(self, source: S) -> Azl1Reader<S> {
        Azl1Reader {
            object: self,
            source,
            next: 0,
            current: Zeroizing::new(Vec::new()),
            at: 0,
            hasher: blake3::Hasher::new(),
            checked: false,
        }
    }
}

/// The plaintext of segment `index` (the STREAM's last one when `last`) from its sealed bytes:
/// decrypted in place, its codec byte read, decompressed to exactly `expected` bytes.
fn open_sealed_segment(
    keys: &ObjectKeys,
    context: &[u8; CONTEXT_LEN],
    index: u64,
    last: bool,
    expected: u64,
    sealed: Vec<u8>,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let position =
        u32::try_from(index).map_err(|_| damaged("a segment index past the STREAM's counter"))?;
    let mut buffer = Zeroizing::new(sealed);
    // Nonce: the prefix, `position` (32 bits, big endian) and the last-segment flag.
    keys.segments
        .decrypt_in_place(position, last, context, &mut *buffer)
        .map_err(|_| {
            damaged(format!(
                "segment {index} does not authenticate (changed, moved or cut off)"
            ))
        })?;
    let expected =
        usize::try_from(expected).map_err(|_| damaged("a segment larger than memory"))?;
    let Some(&codec_byte) = buffer.first() else {
        return Err(damaged(format!("segment {index} is empty")));
    };
    if codec::codec_of(codec_byte)? == Codec::Stored {
        if buffer.len() != expected + 1 {
            return Err(damaged(format!("segment {index} has the wrong length")));
        }
        // Drop the codec byte in place (the plaintext never leaves the wiped buffer).
        buffer.copy_within(1.., 0);
        buffer.truncate(expected);
        return Ok(buffer);
    }
    codec::decode_segment(codec_byte, &buffer[1..], expected)
}

/// How many whole-file transforms (encode or decode) run at once in this process: each may
/// hold up to 150 MB (its file, its stream, Lepton's working set), and a sync moves several
/// files at once.
const PARALLEL_TRANSFORMS: usize = 2;
static TRANSFORMS_RUNNING: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static TRANSFORM_DONE: std::sync::Condvar = std::sync::Condvar::new();

/// One of the [`PARALLEL_TRANSFORMS`] slots, given back when dropped.
struct TransformSlot;

impl TransformSlot {
    /// Waits for a free slot.
    fn take() -> TransformSlot {
        let mut running = TRANSFORMS_RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *running >= PARALLEL_TRANSFORMS {
            running = TRANSFORM_DONE
                .wait(running)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *running += 1;
        TransformSlot
    }
}

impl Drop for TransformSlot {
    fn drop(&mut self) {
        let mut running = TRANSFORMS_RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *running = running.saturating_sub(1);
        TRANSFORM_DONE.notify_one();
    }
}

/// The threads Lepton may use to decode a JPEG: the computer's, at most four.
fn decode_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(4))
}

/// The segments of an object whose every segment is stored (written with
/// [`Compression::Never`]), opened from what a drive's index keeps of it: its id, its file key,
/// its segment size, its plaintext size and the nonce prefix of its segments. A stored segment
/// has a fixed sealed length, so where any plaintext range lies in the object is known without
/// the header or the trailer: a pack's member is ONE ranged read. The key commitment is part of
/// every segment's associated data, so a segment written under another key does not open; the
/// index's BLAKE3 of what is read checks the rest.
pub struct StoredSegments {
    /// The object's keys, derived from its file key and nonce prefix.
    keys: ObjectKeys,
    /// Header bytes 0..88 as the writer made them: every segment's associated data.
    context: [u8; CONTEXT_LEN],
    /// Plaintext bytes per segment.
    segment_size: u64,
    /// The object's plaintext bytes: where the last segment ends.
    plaintext_size: u64,
    /// The object's segments: the last one carries the STREAM's last-segment flag.
    segment_count: u64,
}

impl fmt::Debug for StoredSegments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSegments")
            .field("segment_size", &self.segment_size)
            .field("plaintext_size", &self.plaintext_size)
            .finish_non_exhaustive()
    }
}

impl StoredSegments {
    /// The segments of `object_id` under `file_key`, `prefix` being the nonce prefix its header
    /// names ([`PublicHeader::prefix`]).
    pub fn new(
        object_id: &ObjectId,
        file_key: &FileKey,
        prefix: &[u8; PREFIX_LEN],
        segment_size: u32,
        plaintext_size: u64,
    ) -> Result<StoredSegments, CryptoError> {
        check_segment_size(segment_size)?;
        let context =
            header_context(VERSION, segment_size, object_id, prefix, &commitment_of(file_key));
        Ok(StoredSegments {
            keys: ObjectKeys::derive(file_key, prefix),
            context,
            segment_size: u64::from(segment_size),
            plaintext_size,
            segment_count: segment_count(plaintext_size, segment_size),
        })
    }

    /// The first and the last segment covering plaintext bytes `start..start + len`, which
    /// must be at least one byte inside the plaintext.
    fn covering(&self, start: u64, len: u64) -> Result<(u64, u64), CryptoError> {
        let end = start
            .checked_add(len)
            .filter(|end| len > 0 && *end <= self.plaintext_size)
            .ok_or_else(|| damaged("a range outside the object"))?;
        Ok((start / self.segment_size, (end - 1) / self.segment_size))
    }

    /// Where segment `index` starts in the object, and its sealed length.
    fn sealed_span(&self, index: u64) -> (u64, u64) {
        let plain = if index + 1 < self.segment_count {
            self.segment_size
        } else {
            self.plaintext_size - (self.segment_count - 1) * self.segment_size
        };
        let full = self.segment_size + SEGMENT_OVERHEAD;
        (HEADER_LEN as u64 + index * full, plain + SEGMENT_OVERHEAD)
    }

    /// The bytes of the object to read for plaintext bytes `start..start + len`: their offset
    /// and their length (whole segments).
    pub fn span(&self, start: u64, len: u64) -> Result<(u64, u64), CryptoError> {
        let (first, last) = self.covering(start, len)?;
        let (from, _) = self.sealed_span(first);
        let (at, sealed) = self.sealed_span(last);
        Ok((from, at + sealed - from))
    }

    /// The plaintext bytes `start..start + len` from `sealed`: exactly the bytes
    /// [`StoredSegments::span`] names.
    pub fn open(
        &self,
        start: u64,
        len: u64,
        sealed: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let (first, last) = self.covering(start, len)?;
        let (base, total) = self.span(start, len)?;
        if sealed.len() as u64 != total {
            return Err(damaged("the object is cut short"));
        }
        let mut out = Zeroizing::new(Vec::with_capacity(usize::try_from(len).unwrap_or(0)));
        for index in first..=last {
            let (offset, sealed_len) = self.sealed_span(index);
            let from = (offset - base) as usize;
            let plain_len = sealed_len - SEGMENT_OVERHEAD;
            let plain = open_sealed_segment(
                &self.keys,
                &self.context,
                index,
                index + 1 == self.segment_count,
                plain_len,
                sealed[from..from + sealed_len as usize].to_vec(),
            )?;
            let segment_start = index * self.segment_size;
            let lo = start.max(segment_start) - segment_start;
            let hi = (start + len).min(segment_start + plain_len) - segment_start;
            out.extend_from_slice(&plain[lo as usize..hi as usize]);
        }
        Ok(out)
    }
}

/// The plaintext of a whole object in memory, opened with its file key. The object must be
/// exactly as long as its header says.
pub fn decrypt(object: &[u8], object_id: &ObjectId, file_key: &FileKey) -> Result<Vec<u8>, CryptoError> {
    let opened = OpenObject::open(object, object_id, file_key)?;
    if opened.object_len() != object.len() as u64 {
        return Err(damaged("the object is longer than its header says"));
    }
    let tail_start = opened.offsets[opened.offsets.len() - 1] as usize;
    opened.decrypt_body(&object[HEADER_LEN..tail_start])
}

/// The plaintext of an object as `io::Read`: one segment in memory at a time (wiped as the
/// next one comes), the BLAKE3 of the whole checked at the end.
pub struct Azl1Reader<S: ObjectSource> {
    object: OpenObject,
    source: S,
    /// The next segment to read.
    next: u64,
    current: Zeroizing<Vec<u8>>,
    /// How much of `current` was handed out.
    at: usize,
    hasher: blake3::Hasher,
    checked: bool,
}

impl<S: ObjectSource> Azl1Reader<S> {
    /// The file's bytes.
    #[must_use]
    pub fn plaintext_size(&self) -> u64 {
        self.object.plaintext_size()
    }
}

impl<S> Read for Azl1Reader<S>
where
    S: ObjectSource,
    S::Error: Into<io::Error>,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.at == self.current.len() {
            if self.object.transform.is_some() {
                if self.checked {
                    return Ok(0);
                }
                // A transformed file (at most 64 MiB) is read and decoded whole, its stream's
                // and its own BLAKE3 checked.
                self.checked = true;
                let from = HEADER_LEN as u64;
                let body_len = self.object.offsets[self.object.offsets.len() - 1] - from;
                let body = match self.source.read_at(from, body_len) {
                    Ok(body) => body,
                    Err(e) => return Err(e.into()),
                };
                self.current = Zeroizing::new(self.object.decrypt_body(&body)?);
                self.at = 0;
                self.next = self.object.info.segment_count;
                continue;
            }
            if self.next == self.object.info.segment_count {
                if !self.checked {
                    self.checked = true;
                    if self.hasher.finalize() != self.object.info.blake3 {
                        return Err(damaged("the plaintext does not match its hash").into());
                    }
                }
                return Ok(0);
            }
            let index = self.next;
            let (offset, len) = self.object.segment_span(index);
            let sealed = match self.source.read_at(offset, len) {
                Ok(sealed) => sealed,
                Err(e) => return Err(e.into()),
            };
            if sealed.len() as u64 != len {
                return Err(damaged("the object is cut short").into());
            }
            let plain = self.object.open_segment(index, sealed)?;
            self.hasher.update(&plain);
            self.current = plain;
            self.at = 0;
            self.next += 1;
        }
        let n = buf.len().min(self.current.len() - self.at);
        buf[..n].copy_from_slice(&self.current[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
