//! The AZC1 container: one file, transformed and compressed, self-describing.
//!
//! ```text
//! [header]                                   all integers little endian
//!    0  4  magic "AZC1"
//!    4  1  format version (1)
//!    5  1  flags (0; a set bit is a newer version's)
//!    6  1  transform id (0 none, 1 JPEG images, 2 containers: deflate streams and JPEG images)
//!    7  1  transform version (0 for none, 1 for the others)
//!    8  4  segment size: transformed bytes per segment (1 KiB to 16 MiB)
//!   12  2  length of the transform's parameters (0 in version 1), then the parameters
//! [segments], each:
//!    0  1  codec byte (see `segment`; 0xFF ends the segments)
//!    1  4  its transformed bytes: the segment size, less in the last segment, never 0
//!    5  4  its stored bytes: equal for codec 0, fewer for the others
//!    9     the stored bytes
//! [end]
//!    0  1  0xFF
//!    1  8  transformed length (the sum of the segments' bytes)
//!    9  8  original length
//!   17 32  BLAKE3 of the original
//! ```
//!
//! The segments come first and the lengths and the hash last, so a stream can be written in
//! one pass without knowing its length. A decoder checks everything it reads before it
//! allocates for it, refuses what a newer version wrote by name, and checks the length and the
//! BLAKE3 of the whole file at the end: a transform's decoder can never hand out other bytes
//! than the ones that went in.

use std::{
    io::{self, Cursor, Read, Write},
    time::{Duration, Instant},
};

use zeroize::Zeroizing;

use crate::{
    detect::{detect, Kind, SNIFF_LEN},
    error::{damaged, Error},
    options::{Options, MAX_SEGMENT_SIZE, MIN_SEGMENT_SIZE},
    segment::{decode_segment, CodecStats, Segment, SegmentEncoder},
    transform::{self, DeflateStats, Forward, JpegStats, TransformId},
};

/// The first four bytes of every container.
pub const MAGIC: [u8; 4] = *b"AZC1";
/// The format version this build writes and reads.
pub const FORMAT_VERSION: u8 = 1;
/// The codec byte that ends the segments.
const END: u8 = 0xFF;
/// Bytes of the fixed header (before the parameters).
const HEADER_LEN: usize = 14;
/// Bytes of a segment's frame before its data.
const FRAME_LEN: usize = 9;
/// Bytes of the end record after its 0xFF.
const TRAILER_LEN: usize = 48;

/// What an encode did: for the UI ("Compression saved you ...") and for the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub kind: Kind,
    pub transform: TransformId,
    /// Why the transform this kind wants did not run (`None` when it ran, or none applies).
    pub transform_skipped: Option<String>,
    pub jpeg: JpegStats,
    pub deflate: DeflateStats,
    pub codec: CodecStats,
    pub original_len: u64,
    pub transformed_len: u64,
    /// The container's bytes.
    pub encoded_len: u64,
    pub transform_time: Duration,
    pub codec_time: Duration,
}

/// An encoded file.
#[derive(Debug)]
pub struct Encoded {
    pub bytes: Zeroizing<Vec<u8>>,
    pub report: Report,
}

fn check_segment_size(segment_size: u32) -> Result<(), Error> {
    if (MIN_SEGMENT_SIZE..=MAX_SEGMENT_SIZE).contains(&segment_size) {
        Ok(())
    } else {
        Err(Error::Limit(format!("a segment size of {segment_size} bytes")))
    }
}

fn header(transform: TransformId, segment_size: u32) -> [u8; HEADER_LEN] {
    let (id, version) = transform.bytes();
    let mut out = [0u8; HEADER_LEN];
    out[..4].copy_from_slice(&MAGIC);
    out[4] = FORMAT_VERSION;
    out[5] = 0;
    out[6] = id;
    out[7] = version;
    out[8..12].copy_from_slice(&segment_size.to_le_bytes());
    // 12..14: no parameters in version 1.
    out
}

/// Appends one segment's frame and bytes to `out`.
fn write_frame(out: &mut impl Write, segment: &Segment, raw: &[u8]) -> Result<u64, Error> {
    let data: &[u8] = match segment {
        Segment::Stored => raw,
        Segment::Compressed { data, .. } => data,
    };
    let plain_len = u32::try_from(raw.len()).map_err(|_| Error::Limit(String::from("a segment over 4 GiB")))?;
    let data_len = u32::try_from(data.len()).map_err(|_| Error::Limit(String::from("a segment over 4 GiB")))?;
    let mut frame = [0u8; FRAME_LEN];
    frame[0] = segment.codec().byte();
    frame[1..5].copy_from_slice(&plain_len.to_le_bytes());
    frame[5..9].copy_from_slice(&data_len.to_le_bytes());
    out.write_all(&frame)?;
    out.write_all(data)?;
    Ok((FRAME_LEN + data.len()) as u64)
}

fn trailer(transformed_len: u64, original_len: u64, hash: &blake3::Hash) -> [u8; 1 + TRAILER_LEN] {
    let mut out = [0u8; 1 + TRAILER_LEN];
    out[0] = END;
    out[1..9].copy_from_slice(&transformed_len.to_le_bytes());
    out[9..17].copy_from_slice(&original_len.to_le_bytes());
    out[17..49].copy_from_slice(hash.as_bytes());
    out
}

/// Encodes `input` under `options`: detection, the transform its kind wants (verified, else
/// none), then the segments.
pub fn encode(input: &[u8], options: &Options) -> Result<Encoded, Error> {
    check_segment_size(options.segment_size)?;
    let started = Instant::now();
    let kind = detect(&input[..input.len().min(SNIFF_LEN)], options.extension.as_deref());
    let wanted = transform::wanted(kind, input.len() as u64, options);
    let (transform, transformed, jpeg, deflate, skipped) = match transform::forward(input, kind, options) {
        Forward::Applied(t) => (t.id, Some(t.bytes), t.jpeg, t.deflate, None),
        Forward::NotApplied(why) => (
            TransformId::None,
            None,
            JpegStats::default(),
            DeflateStats::default(),
            wanted.map(|_| why),
        ),
    };
    let transform_time = started.elapsed();
    let codec_started = Instant::now();
    let stream: &[u8] = transformed.as_deref().map_or(input, |bytes| &bytes[..]);
    let mut out = Zeroizing::new(Vec::with_capacity(stream.len() / 2 + 64));
    out.extend_from_slice(&header(transform, options.segment_size));
    let codec_kind = transform::codec_kind(kind, transform);
    let mut segments = SegmentEncoder::new(codec_kind, Some(stream.len() as u64), options);
    for raw in stream.chunks(options.segment_size as usize) {
        let segment = segments.encode(raw)?;
        write_frame(&mut *out, &segment, raw)?;
    }
    out.extend_from_slice(&trailer(
        stream.len() as u64,
        input.len() as u64,
        &blake3::hash(input),
    ));
    let report = Report {
        kind,
        transform,
        transform_skipped: skipped,
        jpeg,
        deflate,
        codec: segments.stats(),
        original_len: input.len() as u64,
        transformed_len: stream.len() as u64,
        encoded_len: out.len() as u64,
        transform_time,
        codec_time: codec_started.elapsed(),
    };
    Ok(Encoded { bytes: out, report })
}

/// Encodes a stream without holding it whole: no transform (they need the whole file), one
/// segment in memory at a time. Write the file into it, then [`StreamEncoder::finish`].
pub struct StreamEncoder<W: Write> {
    sink: W,
    options: Options,
    /// The file's length when the caller knows it: the codec budget is planned on it.
    total_hint: Option<u64>,
    /// Made when the first segment is complete (its bytes decide the kind).
    encoder: Option<SegmentEncoder>,
    kind: Kind,
    /// The segment being filled (wiped).
    pending: Zeroizing<Vec<u8>>,
    hasher: blake3::Hasher,
    original_len: u64,
    /// The container's bytes written so far.
    written: u64,
    started: Instant,
    /// A failed write leaves the container unusable; later writes are refused.
    failed: bool,
}

impl<W: Write> StreamEncoder<W> {
    /// An encoder into `sink` (the header is written at once). `total_len` is the file's
    /// length if known.
    pub fn new(mut sink: W, options: &Options, total_len: Option<u64>) -> Result<StreamEncoder<W>, Error> {
        check_segment_size(options.segment_size)?;
        let head = header(TransformId::None, options.segment_size);
        sink.write_all(&head)?;
        Ok(StreamEncoder {
            sink,
            options: options.clone(),
            total_hint: total_len,
            encoder: None,
            kind: Kind::Binary,
            pending: Zeroizing::new(Vec::with_capacity(options.segment_size as usize)),
            hasher: blake3::Hasher::new(),
            original_len: 0,
            written: HEADER_LEN as u64,
            started: Instant::now(),
            failed: false,
        })
    }

    fn seal_pending(&mut self) -> Result<(), Error> {
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.encoder.is_none() {
            let start = &self.pending[..self.pending.len().min(SNIFF_LEN)];
            self.kind = detect(start, self.options.extension.as_deref());
            self.encoder = Some(SegmentEncoder::new(self.kind, self.total_hint, &self.options));
        }
        let Some(encoder) = self.encoder.as_mut() else {
            return Err(Error::Codec(String::from("no segment encoder")));
        };
        let segment = encoder.encode(&self.pending)?;
        self.written += write_frame(&mut self.sink, &segment, &self.pending)?;
        self.pending.clear();
        Ok(())
    }

    /// Seals the last segment and writes the end; returns the sink and what was done.
    pub fn finish(mut self) -> Result<(W, Report), Error> {
        if self.failed {
            return Err(Error::Io(String::from("the encoder failed before")));
        }
        self.seal_pending()?;
        let end = trailer(self.original_len, self.original_len, &self.hasher.finalize());
        self.sink.write_all(&end)?;
        self.sink.flush()?;
        self.written += end.len() as u64;
        let report = Report {
            kind: self.kind,
            transform: TransformId::None,
            transform_skipped: None,
            jpeg: JpegStats::default(),
            deflate: DeflateStats::default(),
            codec: self.encoder.as_ref().map(SegmentEncoder::stats).unwrap_or_default(),
            original_len: self.original_len,
            transformed_len: self.original_len,
            encoded_len: self.written,
            transform_time: Duration::ZERO,
            codec_time: self.started.elapsed(),
        };
        Ok((self.sink, report))
    }
}

impl<W: Write> Write for StreamEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("the encoder failed before"));
        }
        let segment_size = self.options.segment_size as usize;
        let mut rest = buf;
        while !rest.is_empty() {
            if self.pending.len() == segment_size {
                if let Err(e) = self.seal_pending() {
                    self.failed = true;
                    return Err(io::Error::other(e));
                }
            }
            let take = (segment_size - self.pending.len()).min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            self.hasher.update(&rest[..take]);
            self.original_len += take as u64;
            rest = &rest[take..];
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

/// A container's header, as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub transform: TransformId,
    pub segment_size: u32,
}

fn read_exact_or(reader: &mut impl Read, buf: &mut [u8], what: &str) -> Result<(), Error> {
    reader.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => damaged(format!("{what} is cut off")),
        _ => Error::from(e),
    })
}

/// Reads and checks a container's header.
pub fn read_header(reader: &mut impl Read) -> Result<Header, Error> {
    let mut head = [0u8; HEADER_LEN];
    read_exact_or(reader, &mut head, "the header")?;
    if head[..4] != MAGIC {
        return Err(damaged("not an AZC1 container"));
    }
    if head[4] != FORMAT_VERSION {
        return Err(Error::Unsupported(format!("container format version {}", head[4])));
    }
    if head[5] != 0 {
        return Err(Error::Unsupported(format!("container flags {:#04x}", head[5])));
    }
    let transform = TransformId::from_bytes(head[6], head[7])?;
    let segment_size = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    check_segment_size(segment_size).map_err(|_| damaged("the segment size is out of range"))?;
    let params = u16::from_le_bytes([head[12], head[13]]);
    if params != 0 {
        return Err(Error::Unsupported(String::from("transform parameters")));
    }
    Ok(Header {
        transform,
        segment_size,
    })
}

/// What the end record says.
struct Trailer {
    transformed_len: u64,
    original_len: u64,
    blake3: [u8; 32],
}

/// The transformed stream of a container, segment by segment: a `Read` that the transform's
/// decoder pulls from. An error is kept in `error` (the transform sees only an `io::Error`).
struct SegmentReader<'a, R: Read> {
    inner: &'a mut R,
    segment_size: u32,
    /// The decoded segment being read, and how far.
    current: Zeroizing<Vec<u8>>,
    pos: usize,
    /// Transformed bytes handed out so far, and the most there may be.
    total: u64,
    max_total: u64,
    /// The segment before was shorter than the segment size: only the end may follow.
    short_seen: bool,
    trailer: Option<Trailer>,
    error: Option<Error>,
}

impl<R: Read> SegmentReader<'_, R> {
    /// Decodes the next segment into `current`; `false` at the end record.
    fn next_segment(&mut self) -> Result<bool, Error> {
        let mut codec = [0u8; 1];
        read_exact_or(self.inner, &mut codec, "the segments")?;
        if codec[0] == END {
            let mut end = [0u8; TRAILER_LEN];
            read_exact_or(self.inner, &mut end, "the end record")?;
            self.trailer = Some(Trailer {
                transformed_len: u64::from_le_bytes(end[0..8].try_into().map_err(|_| damaged("end record"))?),
                original_len: u64::from_le_bytes(end[8..16].try_into().map_err(|_| damaged("end record"))?),
                blake3: end[16..48].try_into().map_err(|_| damaged("end record"))?,
            });
            return Ok(false);
        }
        if self.short_seen {
            return Err(damaged("a segment after a short one"));
        }
        let mut lengths = [0u8; 8];
        read_exact_or(self.inner, &mut lengths, "a segment")?;
        let plain_len = u32::from_le_bytes([lengths[0], lengths[1], lengths[2], lengths[3]]);
        let data_len = u32::from_le_bytes([lengths[4], lengths[5], lengths[6], lengths[7]]);
        if plain_len == 0 || plain_len > self.segment_size {
            return Err(damaged("a segment's length is out of range"));
        }
        // Stored data is exactly the segment; compressed data is smaller (else it was stored).
        let stored = codec[0] == 0;
        if (stored && data_len != plain_len) || (!stored && data_len >= plain_len) {
            return Err(damaged("a segment's stored length does not fit its codec"));
        }
        self.total += u64::from(plain_len);
        if self.total > self.max_total {
            return Err(Error::Limit(String::from(
                "the transformed stream is larger than the cap",
            )));
        }
        if plain_len < self.segment_size {
            self.short_seen = true;
        }
        let mut data = Zeroizing::new(vec![0u8; data_len as usize]);
        read_exact_or(self.inner, &mut data, "a segment")?;
        self.current = decode_segment(codec[0], &data, plain_len as usize)?;
        self.pos = 0;
        Ok(true)
    }
}

impl<R: Read> Read for SegmentReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.pos == self.current.len() {
            if self.trailer.is_some() {
                return Ok(0);
            }
            match self.next_segment() {
                Ok(true) => {}
                Ok(false) => return Ok(0),
                Err(e) => {
                    let message = e.to_string();
                    self.error = Some(e);
                    return Err(io::Error::other(message));
                }
            }
        }
        let n = buf.len().min(self.current.len() - self.pos);
        buf[..n].copy_from_slice(&self.current[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// A writer that hashes and counts what passes through and refuses more than `max` bytes.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: blake3::Hasher,
    written: u64,
    max: u64,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written.saturating_add(buf.len() as u64) > self.max {
            return Err(io::Error::other("the decoded file would be larger than the cap"));
        }
        self.inner.write_all(buf)?;
        self.hasher.update(buf);
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Decodes the container of `reader` into `writer`; returns the file's length. On an error the
/// writer may have received part of the file: throw it away.
pub fn decode_to<R: Read, W: Write>(mut reader: R, writer: W, options: &Options) -> Result<u64, Error> {
    let limits = &options.limits;
    let header = read_header(&mut reader)?;
    let mut segments = SegmentReader {
        inner: &mut reader,
        segment_size: header.segment_size,
        current: Zeroizing::new(Vec::new()),
        pos: 0,
        total: 0,
        max_total: transform::max_transformed_len(header.transform, limits.max_decoded_len),
        short_seen: false,
        trailer: None,
        error: None,
    };
    let mut out = HashingWriter {
        inner: writer,
        hasher: blake3::Hasher::new(),
        written: 0,
        max: limits.max_decoded_len,
    };
    let result = transform::inverse(header.transform, &mut segments, &mut out, limits, options.threads);
    if let Some(e) = segments.error.take() {
        return Err(e);
    }
    let written = result?;
    // The transform's own end must be the end of the segments.
    let mut probe = [0u8; 1];
    let more = segments.read(&mut probe);
    if let Some(e) = segments.error.take() {
        return Err(e);
    }
    if more? != 0 {
        return Err(damaged("transformed bytes after the transform's end"));
    }
    let Some(trailer) = segments.trailer.take() else {
        return Err(damaged("the end record is missing"));
    };
    if trailer.transformed_len != segments.total {
        return Err(damaged("the transformed length does not match the segments"));
    }
    if trailer.original_len != written || written != out.written {
        return Err(damaged("the file has another length than the container says"));
    }
    if out.hasher.finalize().as_bytes() != &trailer.blake3 {
        return Err(damaged("the file's BLAKE3 does not match"));
    }
    if reader.read(&mut probe)? != 0 {
        return Err(damaged("bytes after the end record"));
    }
    Ok(written)
}

/// Decodes a container held in memory.
pub fn decode(container: &[u8], options: &Options) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut out = Zeroizing::new(Vec::with_capacity(container.len().saturating_mul(2).min(64 << 20)));
    decode_to(Cursor::new(container), &mut *out, options)?;
    Ok(out)
}
