//! The record stream both transforms write: the file from its first byte to its last, as
//! records (integers little endian):
//!
//! ```text
//! 0x00 literal   u32 length, the bytes as they are
//! 0x01 lepton    u32 length of the JPEG, u16 width, u16 height, u32 length of the Lepton data,
//!                the Lepton data
//! 0x02 deflate   u32 length of the deflate stream, u32 length of its reconstruction data, that
//!                data, u32 length of the plain text's records, the plain text as records
//!                (literal and lepton only, closed by its own 0xFF)
//! 0x03 png idat  u16 number of IDAT chunks, u32 data length of each, u32 length of the joined
//!                data's records, the joined chunk data as records (literal, deflate; closed by
//!                0xFF); decoding writes every chunk back with its length, type and CRC-32
//! 0xFF end       nothing may follow
//! ```
//!
//! Transform 1 (JPEG) writes and reads tags 0x00 and 0x01 only; transform 2 (containers) all
//! four. Decoding and the encoder's verify share this one parser: what the verify accepted is
//! what a decode does later.

use std::{
    io::{Cursor, Read, Write},
    panic::{catch_unwind, AssertUnwindSafe},
};

use zeroize::Zeroizing;

use super::{lepton, scan::JpegSpan};
use crate::{
    error::{damaged, Error},
    options::Limits,
};

pub(crate) const TAG_LITERAL: u8 = 0x00;
pub(crate) const TAG_LEPTON: u8 = 0x01;
pub(crate) const TAG_DEFLATE: u8 = 0x02;
pub(crate) const TAG_PNG_IDAT: u8 = 0x03;
pub(crate) const TAG_END: u8 = 0xFF;

/// The largest plain text one deflate record may hold. A decoder holds it whole while it
/// rebuilds the stream, so this bounds a decode's memory whatever the record claims.
pub(crate) const MAX_DEFLATE_PLAIN: u64 = 64 << 20;
/// The most plain text one byte of deflate data can stand for (a 258-byte match in under two
/// bits, RFC 1951).
pub(crate) const MAX_DEFLATE_EXPANSION: u64 = 1032;
/// The longest deflate stream a record may rebuild: twice its plain text plus 64 KiB. Every
/// stream an encoder writes in practice fits; one that does not is left as a literal.
const MAX_STREAM_PER_PLAIN: u64 = 2;
const MAX_STREAM_SLACK: u64 = 64 * 1024;
/// The most IDAT chunks one PNG record may hold.
pub(crate) const MAX_IDAT_CHUNKS: usize = 1 << 16;
/// The most bytes a nested record stream or a reconstruction may take: its plain text's cap
/// plus the records' own bytes. A longer length is damage, refused before anything is read.
const MAX_NESTED: usize = (MAX_DEFLATE_PLAIN + MAX_DEFLATE_PLAIN / 64 + 4096) as usize;

fn nested_len(len: u32) -> Result<usize, Error> {
    let len = len as usize;
    if len > MAX_NESTED {
        return Err(damaged("a nested record length over its cap"));
    }
    Ok(len)
}
/// Literal records are copied in pieces of this size.
const COPY_CHUNK: usize = 64 * 1024;

/// One record, while a forward transform builds the stream.
pub(crate) enum Piece<'a> {
    Literal(&'a [u8]),
    Lepton {
        span: JpegSpan,
        data: Vec<u8>,
    },
    /// A deflate stream of `stream_len` bytes, its reconstruction data and its plain text,
    /// already written as records (literal and Lepton).
    Deflate {
        stream_len: u32,
        reconstruction: Vec<u8>,
        plain_records: Zeroizing<Vec<u8>>,
        /// The plain text's length (for the memory and time accounting only).
        plain_len: u64,
    },
    /// A PNG's IDAT chunks: their data lengths and the joined data as records.
    PngIdat {
        chunk_lens: Vec<u32>,
        joined_records: Zeroizing<Vec<u8>>,
        /// The bytes the chunks take in the file (lengths, types, data, CRCs).
        file_len: u64,
        /// The deflate stream inside and its plain text's length (for the stats only).
        stream_len: u64,
        plain_len: u64,
    },
}

impl Piece<'_> {
    /// Whether this record transforms something (everything but a literal).
    pub fn is_transform(&self) -> bool {
        !matches!(self, Piece::Literal(_))
    }

    /// The bytes of the file this record stands for.
    pub fn file_len(&self) -> u64 {
        match self {
            Piece::Literal(bytes) => bytes.len() as u64,
            Piece::Lepton { span, .. } => span.len() as u64,
            Piece::Deflate { stream_len, .. } => u64::from(*stream_len),
            Piece::PngIdat { file_len, .. } => *file_len,
        }
    }

    /// The bytes this record takes in the stream.
    pub fn stream_len(&self) -> usize {
        match self {
            Piece::Literal(bytes) => 5 + bytes.len(),
            Piece::Lepton { data, .. } => 13 + data.len(),
            Piece::Deflate {
                reconstruction,
                plain_records,
                ..
            } => 13 + reconstruction.len() + plain_records.len(),
            Piece::PngIdat {
                chunk_lens,
                joined_records,
                ..
            } => 7 + 4 * chunk_lens.len() + joined_records.len(),
        }
    }
}

fn len32(len: usize) -> Result<[u8; 4], Error> {
    u32::try_from(len)
        .map(u32::to_le_bytes)
        .map_err(|_| Error::Limit(String::from("a record over 4 GiB")))
}

/// The records of `pieces`, closed by an end record.
pub(crate) fn write_records(pieces: &[Piece<'_>]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let len = 1 + pieces.iter().map(Piece::stream_len).sum::<usize>();
    let mut out = Zeroizing::new(Vec::with_capacity(len));
    for piece in pieces {
        match piece {
            Piece::Literal(bytes) => {
                out.push(TAG_LITERAL);
                out.extend_from_slice(&len32(bytes.len())?);
                out.extend_from_slice(bytes);
            }
            Piece::Lepton { span, data } => {
                out.push(TAG_LEPTON);
                out.extend_from_slice(&len32(span.len())?);
                out.extend_from_slice(&span.width.to_le_bytes());
                out.extend_from_slice(&span.height.to_le_bytes());
                out.extend_from_slice(&len32(data.len())?);
                out.extend_from_slice(data);
            }
            Piece::Deflate {
                stream_len,
                reconstruction,
                plain_records,
                ..
            } => {
                out.push(TAG_DEFLATE);
                out.extend_from_slice(&stream_len.to_le_bytes());
                out.extend_from_slice(&len32(reconstruction.len())?);
                out.extend_from_slice(reconstruction);
                out.extend_from_slice(&len32(plain_records.len())?);
                out.extend_from_slice(plain_records);
            }
            Piece::PngIdat {
                chunk_lens,
                joined_records,
                ..
            } => {
                out.push(TAG_PNG_IDAT);
                let count = u16::try_from(chunk_lens.len())
                    .map_err(|_| Error::Limit(String::from("over 65535 IDAT chunks")))?;
                out.extend_from_slice(&count.to_le_bytes());
                for len in chunk_lens {
                    out.extend_from_slice(&len.to_le_bytes());
                }
                out.extend_from_slice(&len32(joined_records.len())?);
                out.extend_from_slice(joined_records);
            }
        }
    }
    out.push(TAG_END);
    Ok(out)
}

/// Where decoded records go: a writer (a real decode) or a comparison with the input (the
/// verify of an encode). `index` counts the top-level records that transform something.
pub(crate) trait RecordSink {
    /// The next bytes of a literal record.
    fn literal(&mut self, bytes: &[u8]) -> Result<(), Error>;
    /// The bytes a transforming record decoded to, or why it did not; `file_len` is what it
    /// stands for in the file.
    fn rebuilt(
        &mut self,
        index: usize,
        file_len: u64,
        decoded: Result<Zeroizing<Vec<u8>>, Error>,
    ) -> Result<(), Error>;
}

/// A real decode: every byte to the writer, the first error stops it.
pub(crate) struct WriterSink<'a, W: Write> {
    pub writer: &'a mut W,
}

impl<W: Write> RecordSink for WriterSink<'_, W> {
    fn literal(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.writer.write_all(bytes).map_err(Error::from)
    }

    fn rebuilt(
        &mut self,
        _index: usize,
        _file_len: u64,
        decoded: Result<Zeroizing<Vec<u8>>, Error>,
    ) -> Result<(), Error> {
        self.writer.write_all(&decoded?).map_err(Error::from)
    }
}

/// A decode into memory (the plain text of a deflate record, the joined IDAT data), which
/// stops at `max` bytes.
struct BufferSink<'a> {
    out: &'a mut Zeroizing<Vec<u8>>,
    max: u64,
}

impl BufferSink<'_> {
    fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.out.len() as u64 + bytes.len() as u64 > self.max {
            return Err(Error::Limit(String::from(
                "a record's plain text is larger than its cap",
            )));
        }
        self.out.extend_from_slice(bytes);
        Ok(())
    }
}

impl RecordSink for BufferSink<'_> {
    fn literal(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.push(bytes)
    }

    fn rebuilt(
        &mut self,
        _index: usize,
        _file_len: u64,
        decoded: Result<Zeroizing<Vec<u8>>, Error>,
    ) -> Result<(), Error> {
        self.push(&decoded?)
    }
}

/// The verify: compares with the input and notes which transforming records went wrong (a
/// record that failed still moves the position on by its length, so the records after it are
/// judged on their own).
pub(crate) struct CompareSink<'a> {
    pub expected: &'a [u8],
    pub at: usize,
    /// Whether every literal matched.
    pub same: bool,
    /// The transforming records that failed or gave other bytes.
    pub bad: Vec<usize>,
}

impl<'a> CompareSink<'a> {
    pub fn new(expected: &'a [u8]) -> CompareSink<'a> {
        CompareSink {
            expected,
            at: 0,
            same: true,
            bad: Vec::new(),
        }
    }

    fn matches(&self, bytes: &[u8]) -> bool {
        self.expected
            .get(self.at..self.at.saturating_add(bytes.len()))
            .is_some_and(|expected| expected == bytes)
    }

    /// Whether the stream decoded to exactly the input.
    pub fn complete(&self) -> bool {
        self.same && self.bad.is_empty() && self.at == self.expected.len()
    }
}

impl RecordSink for CompareSink<'_> {
    fn literal(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if !self.matches(bytes) {
            self.same = false;
        }
        self.at = self.at.saturating_add(bytes.len());
        Ok(())
    }

    fn rebuilt(
        &mut self,
        index: usize,
        file_len: u64,
        decoded: Result<Zeroizing<Vec<u8>>, Error>,
    ) -> Result<(), Error> {
        if !decoded.is_ok_and(|bytes| bytes.len() as u64 == file_len && self.matches(&bytes)) {
            self.bad.push(index);
        }
        self.at = self.at.saturating_add(usize::try_from(file_len).unwrap_or(usize::MAX));
        Ok(())
    }
}

/// Which records a stream may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Allowed {
    /// Transform 1, and the plain text of a deflate record: literal and Lepton.
    Jpeg,
    /// Transform 2 at the top level: every record.
    Containers,
    /// The joined data of a PNG record: literal and deflate.
    Idat,
}

fn read_array<const N: usize>(reader: &mut impl Read) -> Result<[u8; N], Error> {
    let mut bytes = [0u8; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| damaged("a transform record is cut off"))?;
    Ok(bytes)
}

fn read_u32(reader: &mut impl Read) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(read_array(reader)?))
}

/// Exactly `len` bytes of `reader`, read in steps so that a damaged length cannot allocate
/// more than the stream holds.
fn read_exactly(reader: &mut impl Read, len: usize) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut out = Zeroizing::new(Vec::with_capacity(len.min(1 << 20)));
    let read = reader
        .take(len as u64)
        .read_to_end(&mut out)
        .map_err(Error::from)?;
    if read != len {
        return Err(damaged("a transform record is cut off"));
    }
    Ok(out)
}

fn check_len(written: u64, more: u64, limits: &Limits) -> Result<u64, Error> {
    let total = written.saturating_add(more);
    if total > limits.max_decoded_len {
        return Err(Error::Limit(String::from(
            "the decoded file would be larger than the cap",
        )));
    }
    Ok(total)
}

/// The records in `bytes` (a nested stream) decoded into memory, at most `max` bytes.
fn decode_nested(
    bytes: &[u8],
    allowed: Allowed,
    max: u64,
    limits: &Limits,
    threads: usize,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut out = Zeroizing::new(Vec::new());
    let mut nested_limits = *limits;
    nested_limits.max_decoded_len = max;
    decode_records(
        &mut Cursor::new(bytes),
        &mut BufferSink { out: &mut out, max },
        allowed,
        &nested_limits,
        threads,
    )?;
    Ok(out)
}

/// The deflate stream of a deflate record: its plain text decoded from its records, then the
/// stream rebuilt from the plain text and the reconstruction data.
fn rebuild_deflate(
    stream_len: u32,
    reconstruction: &[u8],
    plain_records: &[u8],
    limits: &Limits,
    threads: usize,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    // The format's cap, not the caller's memory option: a file written with a larger memory
    // cap must still open on a device with a smaller one.
    let plain = decode_nested(plain_records, Allowed::Jpeg, MAX_DEFLATE_PLAIN, limits, threads)?;
    // Deflate never needs more than about twice its plain text (a 15-bit code per byte, plus
    // block headers): a longer claim is damage, refused before the rebuild sizes its output.
    if u64::from(stream_len) > MAX_STREAM_PER_PLAIN * plain.len() as u64 + MAX_STREAM_SLACK {
        return Err(damaged("a deflate record longer than its plain text allows"));
    }
    // The rebuild runs vendored code over our own (authenticated) bytes; a panic there on
    // damaged bytes still only means "this record does not decode".
    let rebuilt = catch_unwind(AssertUnwindSafe(|| {
        azlin_preflate::recreate_whole_deflate_stream(&plain, reconstruction, stream_len as usize)
    }));
    let stream = match rebuilt {
        Ok(Ok(stream)) => Zeroizing::new(stream),
        Ok(Err(e)) => return Err(damaged(format!("a deflate record does not rebuild: {e}"))),
        Err(_) => return Err(damaged("a deflate record does not rebuild (it panicked)")),
    };
    if stream.len() as u64 != u64::from(stream_len) {
        return Err(damaged(
            "a deflate record rebuilds to another length than it says",
        ));
    }
    Ok(stream)
}

/// A PNG record's chunks: the joined data decoded from its records, cut by the chunk lengths,
/// each chunk written with its length, type and CRC-32.
fn rebuild_idat(chunk_lens: &[u32], joined_records: &[u8], limits: &Limits, threads: usize) -> Result<Zeroizing<Vec<u8>>, Error> {
    let total: u64 = chunk_lens.iter().map(|&l| u64::from(l)).sum();
    if total > MAX_DEFLATE_PLAIN {
        return Err(Error::Limit(String::from("IDAT data over the cap")));
    }
    let joined = decode_nested(joined_records, Allowed::Idat, total, limits, threads)?;
    if joined.len() as u64 != total {
        return Err(damaged("a PNG record's data does not fill its chunks"));
    }
    let mut out = Zeroizing::new(Vec::with_capacity(joined.len() + 12 * chunk_lens.len()));
    let mut at = 0usize;
    for &len in chunk_lens {
        let data = &joined[at..at + len as usize];
        at += len as usize;
        let mut crc = crc32fast::Hasher::new();
        crc.update(b"IDAT");
        crc.update(data);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(b"IDAT");
        out.extend_from_slice(data);
        out.extend_from_slice(&crc.finalize().to_be_bytes());
    }
    Ok(out)
}

/// Decodes the records of `reader` into `sink`; returns the bytes decoded.
pub(crate) fn decode_records<R: Read>(
    reader: &mut R,
    sink: &mut dyn RecordSink,
    allowed: Allowed,
    limits: &Limits,
    threads: usize,
) -> Result<u64, Error> {
    let mut written = 0u64;
    let mut index = 0usize;
    let mut buffer = Zeroizing::new(vec![0u8; COPY_CHUNK]);
    loop {
        let [tag] = read_array::<1>(reader)
            .map_err(|_| damaged("the transform has no end record"))?;
        let refused = match (allowed, tag) {
            (_, TAG_END | TAG_LITERAL) => false,
            (Allowed::Jpeg | Allowed::Containers, TAG_LEPTON) => false,
            (Allowed::Containers | Allowed::Idat, TAG_DEFLATE) => false,
            (Allowed::Containers, TAG_PNG_IDAT) => false,
            _ => true,
        };
        if refused {
            return Err(match tag {
                TAG_LEPTON | TAG_DEFLATE | TAG_PNG_IDAT => damaged(format!(
                    "a transform record of type {tag} where it may not be"
                )),
                other => Error::Unsupported(format!("transform record type {other}")),
            });
        }
        match tag {
            TAG_END => {
                let mut more = [0u8; 1];
                if reader.read(&mut more).map_err(Error::from)? != 0 {
                    return Err(damaged("bytes after the transform's end record"));
                }
                return Ok(written);
            }
            TAG_LITERAL => {
                let len = u64::from(read_u32(reader)?);
                written = check_len(written, len, limits)?;
                let mut left = len;
                while left > 0 {
                    let step = left.min(COPY_CHUNK as u64) as usize;
                    reader
                        .read_exact(&mut buffer[..step])
                        .map_err(|_| damaged("a transform record is cut off"))?;
                    sink.literal(&buffer[..step])?;
                    left -= step as u64;
                }
            }
            TAG_LEPTON => {
                let jpeg_len = read_u32(reader)? as usize;
                let width = u16::from_le_bytes(read_array(reader)?);
                let height = u16::from_le_bytes(read_array(reader)?);
                let lepton_len = read_u32(reader)? as usize;
                written = check_len(written, jpeg_len as u64, limits)?;
                if u64::from(width) * u64::from(height) > limits.max_jpeg_pixels {
                    return Err(Error::Limit(format!(
                        "a {width} x {height} JPEG (over the pixel cap)"
                    )));
                }
                let data = read_exactly(reader, lepton_len)?;
                let decoded = lepton::decode(&data, jpeg_len, width, height, threads);
                sink.rebuilt(index, jpeg_len as u64, decoded)?;
                index += 1;
            }
            TAG_DEFLATE => {
                let stream_len = read_u32(reader)?;
                let reconstruction_len = nested_len(read_u32(reader)?)?;
                written = check_len(written, u64::from(stream_len), limits)?;
                let reconstruction = read_exactly(reader, reconstruction_len)?;
                let plain_len = nested_len(read_u32(reader)?)?;
                let plain_records = read_exactly(reader, plain_len)?;
                let decoded = rebuild_deflate(stream_len, &reconstruction, &plain_records, limits, threads);
                sink.rebuilt(index, u64::from(stream_len), decoded)?;
                index += 1;
            }
            TAG_PNG_IDAT => {
                let count = usize::from(u16::from_le_bytes(read_array(reader)?));
                if count == 0 || count > MAX_IDAT_CHUNKS {
                    return Err(damaged("a PNG record without chunks"));
                }
                let mut chunk_lens = Vec::with_capacity(count);
                for _ in 0..count {
                    chunk_lens.push(read_u32(reader)?);
                }
                let file_len: u64 = chunk_lens.iter().map(|&l| u64::from(l) + 12).sum();
                written = check_len(written, file_len, limits)?;
                let joined_len = nested_len(read_u32(reader)?)?;
                let joined_records = read_exactly(reader, joined_len)?;
                let decoded = rebuild_idat(&chunk_lens, &joined_records, limits, threads);
                sink.rebuilt(index, file_len, decoded)?;
                index += 1;
            }
            _ => return Err(Error::Unsupported(format!("transform record type {tag}"))),
        }
    }
}

/// Verifies `pieces` against `input` by writing and decoding them; a transforming record that
/// does not give back its bytes becomes a literal and the stream is verified again (once).
/// Returns the verified stream, or why there is none.
pub(crate) fn verified<'a>(
    input: &'a [u8],
    mut pieces: Vec<Piece<'a>>,
    allowed: Allowed,
    limits: &Limits,
    threads: usize,
    mut on_dropped: impl FnMut(&Piece<'a>),
) -> Result<(Zeroizing<Vec<u8>>, Vec<Piece<'a>>), String> {
    for _round in 0..2 {
        if !pieces.iter().any(Piece::is_transform) {
            return Err(String::from("nothing left to transform"));
        }
        let stream = write_records(&pieces).map_err(|e| e.to_string())?;
        let mut check = CompareSink::new(input);
        let decoded = decode_records(&mut Cursor::new(&stream[..]), &mut check, allowed, limits, threads);
        if decoded.is_ok() && check.complete() {
            return Ok((stream, pieces));
        }
        if check.bad.is_empty() {
            return Err(String::from("the transform does not decode to the file"));
        }
        // Each bad record goes back to the literal bytes it stands for.
        let mut at = 0usize;
        let mut index = 0usize;
        for piece in &mut pieces {
            let len = piece.file_len() as usize;
            if piece.is_transform() {
                if check.bad.contains(&index) {
                    on_dropped(piece);
                    *piece = Piece::Literal(&input[at..at + len]);
                }
                index += 1;
            }
            at += len;
        }
    }
    Err(String::from("the transform does not decode to the file"))
}
