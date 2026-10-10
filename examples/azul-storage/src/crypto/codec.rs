//! What an AZL1 segment's codec byte says, and when a file is worth compressing.
//!
//! - The codec byte opens every segment's sealed bytes: 0 = stored as it is, 1 = zstd (one
//!   frame, any level), 2 = brotli (one stream; the recompression pass writes it). 3 (JPEG XL)
//!   is kept for a later version; this one refuses it by name.
//! - On upload a segment is compressed with zstd level 3 and kept compressed only when that
//!   saves at least 5 %.
//! - The recompression pass ([`crate::recompress`], an idle computer on mains power) writes a
//!   file again with [`Compression::Recode`]: every segment brotli at quality 11 (text) or zstd
//!   at level 19 (everything else), kept per segment when that saves 5 % over storing it.
//! - A file that is compressed already is not tried at all: the first segment's magic number
//!   says so (JPEG, PNG, GIF, WebP, the ISO media files - MP4, MOV, HEIC, AVIF -, ZIP and with
//!   it DOCX / XLSX / ODT / EPUB, gzip, zstd, xz, bzip2, 7z, RAR, PDF, ...). Otherwise the
//!   first segment is compressed as a trial; when that saves less than 5 %, the file's other
//!   segments are not tried either.
//!
//! Compression happens before encryption, so a segment's stored size says how well it
//! compressed: what an observer of the bucket learns about a file beyond its rough size.

use zeroize::Zeroizing;

use super::CryptoError;

/// The zstd level of the upload pass (fast enough not to slow a transfer down).
pub const ZSTD_LEVEL: i32 = 3;
/// A segment stays compressed only when that saves at least this many percent.
pub const MIN_SAVING_PERCENT: u64 = 5;
/// The zstd level of the recompression pass.
pub const ZSTD_MAX_LEVEL: i32 = 19;
/// The brotli quality of the recompression pass (the slowest, the smallest).
pub const BROTLI_QUALITY: u32 = 11;
/// The smallest and the largest brotli window (log2 of its bytes): a segment's window is the
/// smallest that holds the whole segment (1 MiB segments: 20), so a reader never needs more
/// memory than the segment.
pub const BROTLI_MIN_WINDOW_BITS: u32 = 16;
pub const BROTLI_MAX_WINDOW_BITS: u32 = 24;
/// The codec byte of a brotli segment.
pub const CODEC_BROTLI: u8 = 2;
/// The codec byte kept for a JPEG XL segment (not written or read by this version).
pub const CODEC_JPEG_XL: u8 = 3;

/// How a segment's bytes are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// As they are.
    Stored,
    /// One zstd frame.
    Zstd,
    /// One brotli stream.
    Brotli,
}

impl Codec {
    /// The codec byte.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Codec::Stored => 0,
            Codec::Zstd => 1,
            Codec::Brotli => CODEC_BROTLI,
        }
    }

    /// The codec of a codec byte.
    pub fn from_byte(byte: u8) -> Result<Codec, CryptoError> {
        match byte {
            0 => Ok(Codec::Stored),
            1 => Ok(Codec::Zstd),
            CODEC_BROTLI => Ok(Codec::Brotli),
            CODEC_JPEG_XL => Err(CryptoError::Unsupported(String::from(
                "a JPEG XL segment (written by a newer version)",
            ))),
            other => Err(CryptoError::Unsupported(format!("segment codec {other}"))),
        }
    }
}

/// Compression on upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// zstd where it saves 5 %, as the module documentation says.
    #[default]
    Auto,
    /// Every segment stored as it is (data known to be random, the tests' exact sizes).
    Never,
    /// The recompression pass: every segment tried with `Recoding` (no trial on the first
    /// segment: the pass chose the file), kept when it saves 5 %.
    Recode(Recoding),
}

/// What the recompression pass writes a file's segments with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recoding {
    /// brotli at quality 11: text, markup, source code, JSON, CSV, uncompressed documents.
    Brotli,
    /// zstd at level 19: everything else that is not compressed already.
    ZstdMax,
}

/// Whether `compressed` bytes are worth keeping instead of `raw` ones: at least 5 % fewer.
#[must_use]
pub fn worth_it(compressed: usize, raw: usize) -> bool {
    raw > 0 && (compressed as u64) * 100 <= (raw as u64) * (100 - MIN_SAVING_PERCENT)
}

/// The magic numbers of formats that are compressed already.
const COMPRESSED_MAGIC: &[&[u8]] = &[
    b"\xFF\xD8\xFF",                     // JPEG
    b"\x89PNG\r\n\x1A\n",                // PNG
    b"GIF87a",                           // GIF
    b"GIF89a",                           // GIF
    b"PK\x03\x04",                       // ZIP: DOCX, XLSX, PPTX, ODT, EPUB, JAR, APK
    b"PK\x05\x06",                       // an empty ZIP
    b"PK\x07\x08",                       // a spanned ZIP
    b"\x1F\x8B",                         // gzip
    b"\x28\xB5\x2F\xFD",                 // zstd
    b"\xFD7zXZ\x00",                     // xz
    b"BZh",                              // bzip2
    b"7z\xBC\xAF\x27\x1C",               // 7z
    b"Rar!\x1A\x07",                     // RAR
    b"%PDF-",                            // PDF (its streams are deflated)
    b"\x04\x22\x4D\x18",                 // LZ4
    b"OggS",                             // Ogg: Vorbis, Opus, Theora
    b"fLaC",                             // FLAC
    b"ID3",                              // MP3 with an ID3 tag
    b"\x1A\x45\xDF\xA3",                 // Matroska, WebM
    b"\xFF\x0A",                         // JPEG XL codestream
    b"\x00\x00\x00\x0CJXL \x0D\x0A\x87\x0A", // JPEG XL container
    b"wOFF",                             // WOFF
    b"wOF2",                             // WOFF2
    b"MSCF",                             // CAB
];

/// Whether `start` (a file's first bytes) is a format that is compressed already.
#[must_use]
pub fn looks_compressed(start: &[u8]) -> bool {
    if COMPRESSED_MAGIC.iter().any(|magic| start.starts_with(magic)) {
        return true;
    }
    // The ISO base media files (MP4, MOV, M4A, HEIC, AVIF, 3GP): a box size, then `ftyp`.
    if start.len() >= 12 && &start[4..8] == b"ftyp" {
        return true;
    }
    // RIFF: WebP and AVI are compressed, WAV is not.
    start.len() >= 12
        && start.starts_with(b"RIFF")
        && matches!(&start[8..12], b"WEBP" | b"AVI ")
}

/// A segment as it goes into its object: stored, or the zstd frame or brotli stream of it.
pub(crate) enum Encoded {
    Stored,
    Zstd(Zeroizing<Vec<u8>>),
    Brotli(Zeroizing<Vec<u8>>),
}

impl Encoded {
    pub(crate) fn codec(&self) -> Codec {
        match self {
            Encoded::Stored => Codec::Stored,
            Encoded::Zstd(_) => Codec::Zstd,
            Encoded::Brotli(_) => Codec::Brotli,
        }
    }

    /// The compressed bytes; `None` for a stored segment.
    pub(crate) fn compressed(&self) -> Option<&[u8]> {
        match self {
            Encoded::Stored => None,
            Encoded::Zstd(bytes) | Encoded::Brotli(bytes) => Some(bytes.as_slice()),
        }
    }
}

enum State {
    /// The first segment decides.
    Undecided,
    /// Every segment is tried.
    On(zstd::bulk::Compressor<'static>),
    /// No segment is tried.
    Off,
    /// Every segment is recompressed (the zstd compressor made on the first use).
    Recode(Recoding, Option<zstd::bulk::Compressor<'static>>),
}

/// One file's compression: the first segment decides whether the others are tried.
pub(crate) struct Encoder {
    state: State,
}

fn zstd_failed(e: std::io::Error) -> CryptoError {
    CryptoError::Io(format!("zstd: {e}"))
}

impl Encoder {
    pub(crate) fn new(compression: Compression) -> Encoder {
        Encoder {
            state: match compression {
                Compression::Auto => State::Undecided,
                Compression::Never => State::Off,
                Compression::Recode(recoding) => State::Recode(recoding, None),
            },
        }
    }

    /// How the segment `raw` is stored.
    pub(crate) fn encode(&mut self, raw: &[u8]) -> Result<Encoded, CryptoError> {
        if let State::Recode(recoding, compressor) = &mut self.state {
            return recode(*recoding, compressor, raw);
        }
        if let State::Undecided = self.state {
            if looks_compressed(raw) {
                self.state = State::Off;
                return Ok(Encoded::Stored);
            }
            self.state = State::On(zstd::bulk::Compressor::new(ZSTD_LEVEL).map_err(zstd_failed)?);
            let trial = self.compress(raw)?;
            if let Encoded::Stored = trial {
                // The trial saved less than 5 %: the rest of the file is not tried.
                self.state = State::Off;
            }
            return Ok(trial);
        }
        self.compress(raw)
    }

    fn compress(&mut self, raw: &[u8]) -> Result<Encoded, CryptoError> {
        let State::On(compressor) = &mut self.state else {
            return Ok(Encoded::Stored);
        };
        if raw.is_empty() {
            return Ok(Encoded::Stored);
        }
        let compressed = Zeroizing::new(compressor.compress(raw).map_err(zstd_failed)?);
        if worth_it(compressed.len(), raw.len()) {
            Ok(Encoded::Zstd(compressed))
        } else {
            Ok(Encoded::Stored)
        }
    }
}

/// The smallest brotli window that holds `len` bytes, within the bounds above.
#[must_use]
pub fn brotli_window_bits(len: usize) -> u32 {
    let mut bits = BROTLI_MIN_WINDOW_BITS;
    while bits < BROTLI_MAX_WINDOW_BITS && (1usize << bits) < len {
        bits += 1;
    }
    bits
}

/// `raw` brotli-compressed at [`BROTLI_QUALITY`] (one stream).
fn brotli_compress(raw: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    use std::io::Write;
    let mut out = Zeroizing::new(Vec::with_capacity(raw.len() / 2));
    let mut writer = brotli::CompressorWriter::new(
        &mut *out,
        64 * 1024,
        BROTLI_QUALITY,
        brotli_window_bits(raw.len()),
    );
    writer
        .write_all(raw)
        .map_err(|e| CryptoError::Io(format!("brotli: {e}")))?;
    // `into_inner` finishes the stream (into `out`, which needs no I/O to fail).
    let _ = writer.into_inner();
    Ok(out)
}

/// One segment of the recompression pass: `recoding`'s bytes when they save 5 %, else stored.
fn recode(
    recoding: Recoding,
    compressor: &mut Option<zstd::bulk::Compressor<'static>>,
    raw: &[u8],
) -> Result<Encoded, CryptoError> {
    if raw.is_empty() {
        return Ok(Encoded::Stored);
    }
    let encoded = match recoding {
        Recoding::Brotli => Encoded::Brotli(brotli_compress(raw)?),
        Recoding::ZstdMax => {
            if compressor.is_none() {
                *compressor =
                    Some(zstd::bulk::Compressor::new(ZSTD_MAX_LEVEL).map_err(zstd_failed)?);
            }
            let Some(compressor) = compressor.as_mut() else {
                return Ok(Encoded::Stored);
            };
            Encoded::Zstd(Zeroizing::new(compressor.compress(raw).map_err(zstd_failed)?))
        }
    };
    let keep = encoded
        .compressed()
        .is_some_and(|bytes| worth_it(bytes.len(), raw.len()));
    Ok(if keep { encoded } else { Encoded::Stored })
}

/// The plaintext of a brotli segment, which must be exactly `expected` bytes (reading stops
/// one byte past it: a segment cannot inflate past its size).
pub(crate) fn decompress_brotli(
    data: &[u8],
    expected: usize,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    use std::io::Read;
    let mut plain = Zeroizing::new(Vec::with_capacity(expected));
    brotli::Decompressor::new(data, 64 * 1024)
        .take(expected as u64 + 1)
        .read_to_end(&mut plain)
        .map_err(|_| CryptoError::Damaged(String::from("a segment does not decompress")))?;
    if plain.len() != expected {
        return Err(CryptoError::Damaged(String::from(
            "a segment decompresses to another length than the file's",
        )));
    }
    Ok(plain)
}

/// The plaintext of a zstd segment, which must be exactly `expected` bytes (more is refused
/// while decompressing: a segment cannot inflate past its size).
pub(crate) fn decompress(data: &[u8], expected: usize) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let plain = zstd::bulk::decompress(data, expected)
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Damaged(String::from("a segment does not decompress")))?;
    if plain.len() != expected {
        return Err(CryptoError::Damaged(String::from(
            "a segment decompresses to another length than the file's",
        )));
    }
    Ok(plain)
}
