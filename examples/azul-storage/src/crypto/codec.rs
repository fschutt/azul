//! What an AZL1 segment's codec byte says, and how a file is compressed before it is sealed.
//! The work is azlin-compress's; this module ties it to AZL1 and to the drive's errors.
//!
//! - The codec byte opens every segment's sealed bytes: 0 = stored as it is, 1 = zstd (one
//!   frame, any level), 2 = brotli (one stream), 4 = zstd with one of the shipped dictionaries
//!   (its id first). 3 (JPEG XL) is kept free; it and any other byte are refused by name.
//! - Compression is on by default ([`Compression::Auto`]): the file's kind comes from its first
//!   bytes (the extension only breaks ties), a JPEG, PDF, Office, ZIP, PNG or gzip file up to
//!   64 MiB goes through its whole-file transform first (an AZL1 version 2 object, see
//!   [`crate::crypto::azl1`]), and each segment is then stored, or compressed with the codec the
//!   time budget allows when that saves at least 5 %. Dense kinds (video, HEIC, archives) are
//!   stored without a trial.
//! - The recompression pass ([`crate::recompress`], an idle computer on mains power) writes a
//!   file again with [`Compression::Idle`]: the same steps with the highest levels.
//!
//! Compression happens before encryption, so a segment's stored size says how well it
//! compressed: what an observer of the bucket learns about a file beyond its rough size.

use azlin_compress::{
    detect,
    segment::{decode_segment as decode, Segment, SegmentEncoder},
    Kind, Options,
};
use zeroize::Zeroizing;

pub use azlin_compress::segment::{
    brotli_window_bits, worth_it, Codec, CODEC_JPEG_XL, MIN_SAVING_PERCENT, RETRY_EVERY,
};

use super::CryptoError;

/// The codec byte of a brotli segment.
pub const CODEC_BROTLI: u8 = 2;
/// The codec byte of a zstd segment with a shipped dictionary.
pub const CODEC_ZSTD_DICT: u8 = 4;
/// The bytes the file's kind is detected from.
pub const SNIFF_LEN: usize = azlin_compress::detect::SNIFF_LEN;

impl From<azlin_compress::Error> for CryptoError {
    fn from(e: azlin_compress::Error) -> CryptoError {
        match e {
            azlin_compress::Error::Unsupported(what) => CryptoError::Unsupported(what),
            azlin_compress::Error::Io(why) => CryptoError::Io(why),
            // A decoder over a limit or refused by its codec read bytes that are not what a
            // writer of this format makes.
            other => CryptoError::Damaged(other.to_string()),
        }
    }
}

/// How a file is compressed when it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// Upload: the transform and the segment codecs that keep up with a transfer.
    #[default]
    Auto,
    /// Every segment stored as it is, no transform (packs, the tests' exact sizes).
    Never,
    /// The recompression pass: the transform and the highest levels the idle budget allows.
    Idle,
}

impl Compression {
    /// azlin-compress's options for a file with this name extension; `None` for
    /// [`Compression::Never`]. The segment size is AZL1's, so the budgets see real segments.
    #[must_use]
    pub fn options(self, segment_size: u32, extension: Option<&str>) -> Option<Options> {
        let mut options = match self {
            Compression::Never => return None,
            Compression::Auto => Options::upload(),
            Compression::Idle => Options::idle(),
        };
        options.segment_size = segment_size;
        Some(match extension {
            Some(extension) => options.with_extension(extension),
            None => options,
        })
    }
}

/// The codec of a codec byte, refused by name when this build does not read it.
pub fn codec_of(byte: u8) -> Result<Codec, CryptoError> {
    Ok(Codec::from_byte(byte)?)
}

/// The plaintext of a segment from its codec byte and its stored bytes: exactly `expected`
/// bytes, or `Damaged` (decoding stops past them: a segment cannot inflate past its size).
pub fn decode_segment(
    codec_byte: u8,
    data: &[u8],
    expected: usize,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    Ok(decode(codec_byte, data, expected)?)
}

/// The name extension of a drive path ("jpg" of "photos/a.JPG"), the detection's tie-break.
/// A dot file without another dot ("notes/.profile") has none.
#[must_use]
pub fn extension_of(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty() && extension.len() <= 10).then_some(extension)
}

/// A segment as it goes into its object: stored, or its codec and stored bytes.
pub(crate) enum Encoded {
    Stored,
    Compressed {
        codec: Codec,
        data: Zeroizing<Vec<u8>>,
    },
}

impl Encoded {
    pub(crate) fn codec(&self) -> Codec {
        match self {
            Encoded::Stored => Codec::Stored,
            Encoded::Compressed { codec, .. } => *codec,
        }
    }

    /// The compressed bytes; `None` for a stored segment.
    pub(crate) fn compressed(&self) -> Option<&[u8]> {
        match self {
            Encoded::Stored => None,
            Encoded::Compressed { data, .. } => Some(data.as_slice()),
        }
    }
}

enum State {
    /// Nothing is compressed.
    Off,
    /// The first segment names the kind (unless the writer knew it) and makes the encoder.
    Waiting {
        options: Options,
        kind: Option<Kind>,
        total_len: Option<u64>,
    },
    Encoding(Box<SegmentEncoder>),
}

/// One file's segments, in order: azlin-compress's segment encoder, made at the first segment
/// (its first bytes say the kind).
pub(crate) struct Encoder {
    state: State,
}

impl Encoder {
    /// The encoder of a file under `options` (`None`: everything stored). `kind` is the kind the
    /// codecs treat the bytes as when the writer knows it (a transformed stream); `total_len`
    /// the length of what is written, when known (the time budget grows with it).
    pub(crate) fn new(options: Option<Options>, kind: Option<Kind>, total_len: Option<u64>) -> Encoder {
        Encoder {
            state: match options {
                None => State::Off,
                Some(options) => State::Waiting {
                    options,
                    kind,
                    total_len,
                },
            },
        }
    }

    /// How the segment `raw` is stored. A compressed segment was decoded and compared with
    /// `raw` already; one that did not give `raw` back is stored.
    pub(crate) fn encode(&mut self, raw: &[u8]) -> Result<Encoded, CryptoError> {
        self.state = match std::mem::replace(&mut self.state, State::Off) {
            State::Waiting {
                options,
                kind,
                total_len,
            } => {
                let kind = kind.unwrap_or_else(|| {
                    detect(&raw[..raw.len().min(SNIFF_LEN)], options.extension.as_deref())
                });
                State::Encoding(Box::new(SegmentEncoder::new(kind, total_len, &options)))
            }
            other => other,
        };
        let State::Encoding(encoder) = &mut self.state else {
            return Ok(Encoded::Stored);
        };
        Ok(match encoder.encode(raw)? {
            Segment::Stored => Encoded::Stored,
            Segment::Compressed { codec, data } => Encoded::Compressed { codec, data },
        })
    }
}
