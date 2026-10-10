//! The AZL1 object: one version of one file, compressed where that pays and encrypted, as it
//! lies in the bucket under its random key.
//!
//! ```text
//! [header, 4096 bytes]                                 all integers little endian
//!     0   4  magic "AZL1"
//!     4   2  version (1)
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
//!   300      zero up to 4096
//! [segments]
//!     segment i: XChaCha20-Poly1305 of (codec byte || data) - codec 0 stored, 1 zstd,
//!     2 brotli (see `codec`) -, the nonce being the prefix, i as
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

use super::{
    codec::{self, Codec, Compression, Encoded, Encoder},
    random_bytes, CryptoError, DriveKey, FileKey, ObjectId, WrappedKey, NONCE_LEN,
    OBJECT_ID_LEN, TAG_LEN,
};

/// The first four bytes of every object.
pub const MAGIC: [u8; 4] = *b"AZL1";
/// The format version this build writes and reads.
pub const VERSION: u16 = 1;
/// Bytes of the header.
pub const HEADER_LEN: usize = 4096;
/// Plaintext bytes per segment, unless a writer is told otherwise.
pub const DEFAULT_SEGMENT_SIZE: u32 = 1 << 20;
/// The smallest and the largest segment size a header may name.
pub const MIN_SEGMENT_SIZE: u32 = 1 << 10;
pub const MAX_SEGMENT_SIZE: u32 = 1 << 24;

/// The STREAM nonce prefix: 24 nonce bytes less the 32-bit counter and the last-segment flag.
const PREFIX_LEN: usize = 19;
/// Bytes of the sealed info's plaintext.
const INFO_LEN: usize = 60;
/// Bytes of the key commitment.
const COMMITMENT_LEN: usize = 32;
/// A segment's bytes beyond its data: the codec byte and the tag.
const SEGMENT_OVERHEAD: u64 = 1 + TAG_LEN as u64;
/// Bytes of one trailer entry (a segment's sealed length).
const TRAILER_ENTRY: usize = 4;
/// Bytes of the trailer's length at the very end.
const LENGTH_FIELD: u64 = 8;
/// Info flag: at least one segment is compressed.
const FLAG_COMPRESSED: u32 = 1;

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
/// Everything from here to the header's end is zero.
const HEADER_USED: usize = 300;

/// BLAKE3 `derive_key` contexts of the keys derived from a file key (one key, one purpose).
const SEGMENT_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 segment key";
const INFO_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 header info key";
const TRAILER_KEY_CONTEXT: &str = "Azlin 2026-10-08 AZL1 trailer key";
/// What the key commitment hashes, keyed with the file key.
const COMMITMENT_INPUT: &[u8] = b"AZL1 key commitment v1";

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
    plaintext_size: u64,
    segment_count: u64,
    object_len: u64,
    blake3: [u8; 32],
    flags: u32,
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
        Ok(Info {
            plaintext_size: le_u64(&bytes[0..8]),
            segment_count: le_u64(&bytes[8..16]),
            object_len: le_u64(&bytes[16..24]),
            blake3: array(&bytes[24..56])?,
            flags: le_u32(&bytes[56..60]),
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
            || self.segment_count != segment_count(self.plaintext_size, segment_size)
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
            self.plaintext_size - (self.segment_count - 1) * segment_size
        }
    }
}

/// Bytes 0..88 of a header.
fn header_context(
    segment_size: u32,
    object_id: &ObjectId,
    prefix: &[u8; PREFIX_LEN],
    commitment: &blake3::Hash,
) -> [u8; CONTEXT_LEN] {
    let mut context = [0u8; CONTEXT_LEN];
    context[..MAGIC.len()].copy_from_slice(&MAGIC);
    context[OFF_VERSION..OFF_VERSION + 2].copy_from_slice(&VERSION.to_le_bytes());
    // The header flags (bytes 6..8) stay zero in version 1.
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

/// What a finished object is: what the drive's index keeps of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSummary {
    pub object_id: ObjectId,
    /// The file's bytes (its size before compression).
    pub plaintext_size: u64,
    /// The object's bytes in the bucket (compressed and encrypted: what the quota counts).
    pub object_len: u64,
    pub segment_count: u64,
    /// BLAKE3 of the plaintext.
    pub blake3: [u8; 32],
    /// Whether a segment is compressed.
    pub compressed: bool,
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

/// Writes one new object, a segment at a time, whatever the file's size: give it the
/// plaintext through `io::Write`, then [`Azl1Writer::finish`]. The sink receives the segments
/// and the trailer (the object from byte [`HEADER_LEN`] on); the header comes back at the end,
/// for the object's first [`HEADER_LEN`] bytes. A writer that failed refuses further writes.
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
    plaintext_size: u64,
    /// Bytes handed to the sink.
    written: u64,
    encoder: Encoder,
    compressed: bool,
    failed: bool,
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
        check_segment_size(options.segment_size)?;
        let file_key = FileKey::generate()?;
        let mut prefix = [0u8; PREFIX_LEN];
        random_bytes(&mut prefix)?;
        let wrapped_key = drive_key.wrap_file_key(&file_key, &object_id)?;
        let context = header_context(
            options.segment_size,
            &object_id,
            &prefix,
            &commitment_of(&file_key),
        );
        let segment_size = options.segment_size as usize;
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
            encoder: Encoder::new(options.compression),
            compressed: false,
            failed: false,
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
            Encoded::Zstd(frame) | Encoded::Brotli(frame) => frame,
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

    /// The header of the finished object.
    fn header(
        &self,
        info: Info,
        trailer_nonce: &[u8; NONCE_LEN],
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
        let info = Info {
            plaintext_size: self.plaintext_size,
            segment_count,
            object_len: HEADER_LEN as u64 + self.written + trailer.len() as u64 + LENGTH_FIELD,
            blake3: *self.hasher.finalize().as_bytes(),
            flags: if self.compressed { FLAG_COMPRESSED } else { 0 },
        };
        let header = self.header(info, &trailer_nonce)?;
        let summary = ObjectSummary {
            object_id: self.object_id,
            plaintext_size: info.plaintext_size,
            object_len: info.object_len,
            segment_count,
            blake3: info.blake3,
            compressed: self.compressed,
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

/// A new object of `plaintext` in memory, its file key wrapped with `drive_key`.
pub fn encrypt(
    plaintext: &[u8],
    object_id: ObjectId,
    drive_key: &DriveKey,
    options: &WriteOptions,
) -> Result<(Vec<u8>, ObjectSummary), CryptoError> {
    let mut sink = vec![0u8; HEADER_LEN];
    sink.reserve(plaintext.len() + plaintext.len() / 4096 + 1024);
    let mut writer = Azl1Writer::new(sink, object_id, drive_key, options)?;
    writer.write_all(plaintext)?;
    let Finished {
        mut sink,
        header,
        summary,
    } = writer.finish()?;
    sink[..HEADER_LEN].copy_from_slice(&header[..]);
    Ok((sink, summary))
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
    let start = sink.stream_position()?;
    sink.write_all(&[0u8; HEADER_LEN])?;
    let mut writer = Azl1Writer::new(&mut *sink, object_id, drive_key, options)?;
    io::copy(source, &mut writer)?;
    let Finished {
        header, summary, ..
    } = writer.finish()?;
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
    context: [u8; CONTEXT_LEN],
    prefix: [u8; PREFIX_LEN],
    commitment: [u8; COMMITMENT_LEN],
    info_nonce: [u8; NONCE_LEN],
    sealed_info: [u8; INFO_LEN + TAG_LEN],
    trailer_nonce: [u8; NONCE_LEN],
}

impl PublicHeader {
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
        if version != VERSION {
            return Err(CryptoError::Unsupported(format!("AZL1 version {version}")));
        }
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
            .chain(&bytes[HEADER_USED..]);
        if unused.copied().any(|b| b != 0) {
            return Err(damaged("the header's unused bytes are not zero"));
        }
        let wrapped_key = WrappedKey::from_bytes(&bytes[OFF_WRAP..OFF_WRAP + WrappedKey::LEN])
            .ok_or_else(|| damaged("the wrapped file key"))?;
        Ok(PublicHeader {
            segment_size,
            object_id: ObjectId(array(&bytes[OFF_OBJECT_ID..OFF_OBJECT_ID + OBJECT_ID_LEN])?),
            wrapped_key,
            context: array(&bytes[..CONTEXT_LEN])?,
            prefix: array(&bytes[OFF_PREFIX..OFF_PREFIX + PREFIX_LEN])?,
            commitment: array(&bytes[OFF_COMMITMENT..OFF_COMMITMENT + COMMITMENT_LEN])?,
            info_nonce: array(&bytes[OFF_INFO_NONCE..OFF_INFO_NONCE + NONCE_LEN])?,
            sealed_info: array(&bytes[OFF_INFO..OFF_INFO + INFO_LEN + TAG_LEN])?,
            trailer_nonce: array(&bytes[OFF_TRAILER_NONCE..OFF_TRAILER_NONCE + NONCE_LEN])?,
        })
    }

    /// The commitment, the keys and the info, for `object_id` under `file_key`.
    fn open(
        &self,
        object_id: &ObjectId,
        file_key: &FileKey,
    ) -> Result<(ObjectKeys, Info), CryptoError> {
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
        let info = Info::parse(&info)?;
        info.check(self.segment_size)?;
        Ok((keys, info))
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
        let (keys, info) = header.open(object_id, file_key)?;
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

    /// Where segment `index` lies in the object: its offset and its sealed length.
    #[must_use]
    pub fn segment_span(&self, index: u64) -> (u64, u64) {
        let i = index as usize;
        (self.offsets[i], self.offsets[i + 1] - self.offsets[i])
    }

    /// The plaintext of segment `index` from its sealed bytes.
    fn open_segment(&self, index: u64, sealed: Vec<u8>) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let position = u32::try_from(index)
            .map_err(|_| damaged("a segment index past the STREAM's counter"))?;
        let last = index + 1 == self.info.segment_count;
        let mut buffer = Zeroizing::new(sealed);
        // Nonce: the prefix, `position` (32 bits, big endian) and the last-segment flag.
        self.keys
            .segments
            .decrypt_in_place(position, last, &self.context, &mut *buffer)
            .map_err(|_| {
                damaged(format!(
                    "segment {index} does not authenticate (changed, moved or cut off)"
                ))
            })?;
        let expected = usize::try_from(self.info.segment_plain_len(self.segment_size, index))
            .map_err(|_| damaged("a segment larger than memory"))?;
        let Some(&codec_byte) = buffer.first() else {
            return Err(damaged(format!("segment {index} is empty")));
        };
        match Codec::from_byte(codec_byte)? {
            Codec::Stored => {
                if buffer.len() != expected + 1 {
                    return Err(damaged(format!("segment {index} has the wrong length")));
                }
                // Drop the codec byte in place (the plaintext never leaves the wiped buffer).
                buffer.copy_within(1.., 0);
                buffer.truncate(expected);
                Ok(buffer)
            }
            Codec::Zstd => codec::decompress(&buffer[1..], expected),
            Codec::Brotli => codec::decompress_brotli(&buffer[1..], expected),
        }
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
    /// past it gives nothing): the segments covering them, in ONE read of the source.
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
    /// the trailer), checked against its BLAKE3.
    fn decrypt_body(&self, body: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut out = Vec::with_capacity(usize::try_from(self.info.plaintext_size).unwrap_or(0));
        let mut hasher = blake3::Hasher::new();
        self.decrypt_segments(
            body,
            0,
            self.info.segment_count - 1,
            Some(&mut hasher),
            |_, plain| out.extend_from_slice(plain),
        )?;
        if hasher.finalize() != self.info.blake3 {
            return Err(damaged("the plaintext does not match its hash"));
        }
        Ok(out)
    }

    /// A reader of the plaintext from `source`, a segment at a time.
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
