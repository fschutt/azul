//! AzDrive's lossless compression, on the client only: the server stores ciphertext and never
//! sees a plaintext byte, so whatever saves space has to happen here, before encryption. Fewer
//! bytes also make the upload itself shorter.
//!
//! A file goes through three steps, each of which may decline:
//! 1. **detect** ([`detect`]): its kind from its first bytes, the extension only as a tie-break;
//! 2. **transform** ([`transform`]): JPEG images (the file itself, or pictures inside a PDF, an
//!    Office or ZIP file, a camera RAW file) are recompressed with Lepton, about a fifth smaller
//!    and bit-exact; the deflate streams of PDF, ZIP and Office files, PNG images and gzip files
//!    become their plain text plus the few bytes that rebuild them exactly. Every transform is
//!    decoded and compared with the input before it is kept;
//! 3. **segments** ([`segment`]): the result is cut into 1 MiB segments, each stored or
//!    compressed on its own with zstd (the level the time budget allows), brotli (text, when it
//!    wins), or zstd with a shipped dictionary (small text files). Dense kinds (video, HEIC,
//!    archives) are stored without a trial.
//!
//! [`container`] writes all of it into one self-describing AZC1 container; AZL1 (azul-storage's
//! encrypted object) uses the segment layer and the transform directly, with its own framing.
//! [`plan`] says up front what will happen (whether the caller must hold the whole file in
//! memory for a transform), and [`estimate`] guesses the saving cheaply for the UI.
//!
//! Everything is synchronous and `Send`, touches no network and keeps no global state. Every
//! decoder checks before it allocates and returns an error, never panics, on any input.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod container;
pub mod detect;
pub mod dict;
mod error;
pub mod member;
mod options;
pub mod segment;
pub mod transform;

pub use container::{decode, decode_to, encode, Encoded, Report, StreamEncoder};
pub use detect::{detect, Kind};
pub use error::Error;
pub use member::{decode_member, encode_member, Member, MEMBER_CONTAINER};
pub use options::{
    Budget, Limits, Options, DEFAULT_SEGMENT_SIZE, MAX_JPEG_PIXELS, MAX_SEGMENT_SIZE, MIN_SEGMENT_SIZE,
};
pub use transform::TransformId;

/// How the codec stage will treat a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecPath {
    /// Stored without a trial (a dense kind).
    Skip,
    /// zstd after a trial on the first segment.
    Trial,
    /// zstd or brotli, whichever is smaller on the first segment.
    Text,
    /// Like [`CodecPath::Text`], with the kind's shipped dictionary as a third candidate.
    TextWithDictionary,
}

/// What [`encode`] will do with a file, decided from its first bytes and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub kind: Kind,
    /// The whole-file transform that will be tried (the caller must have the whole file).
    pub transform: Option<TransformId>,
    /// What the codec stage does with the transformed stream (with the file itself when the
    /// transform does not run, a dense kind is stored).
    pub codecs: CodecPath,
}

/// What [`encode`] will do with a file of `len` bytes that starts with `start`.
#[must_use]
pub fn plan(start: &[u8], len: u64, options: &Options) -> Plan {
    let kind = detect(&start[..start.len().min(detect::SNIFF_LEN)], options.extension.as_deref());
    let transform = transform::wanted(kind, len, options);
    // What the codecs see: the transformed stream when the transform runs.
    let codec_kind = transform.map_or(kind, |id| transform::codec_kind(kind, id));
    let codecs = if codec_kind.is_dense() {
        CodecPath::Skip
    } else if codec_kind.is_text() {
        if options.dictionaries
            && len <= segment::DICTIONARY_MAX_LEN
            && dict::for_kind(codec_kind).is_some()
        {
            CodecPath::TextWithDictionary
        } else {
            CodecPath::Text
        }
    } else {
        CodecPath::Trial
    };
    Plan {
        kind,
        transform,
        codecs,
    }
}

/// A cheap guess of what a file will take once encoded, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate {
    pub kind: Kind,
    pub original_len: u64,
    /// The most likely encoded length, and a range around it.
    pub expected_len: u64,
    pub low_len: u64,
    pub high_len: u64,
    /// Whether the guess compressed part of the file (`false`: a typical figure for the kind).
    pub measured: bool,
}

/// The sample `estimate` compresses at most.
pub const ESTIMATE_SAMPLE: usize = 256 * 1024;

/// What a file of `total_len` bytes, whose first bytes are `sample`, will take: a typical
/// figure for JPEG (Lepton saves 8 to 31 %, 21 % on average in our measurements) and dense
/// kinds (nothing), otherwise zstd level 3 on up to [`ESTIMATE_SAMPLE`] bytes of the sample.
/// Takes about a millisecond.
#[must_use]
pub fn estimate(sample: &[u8], total_len: u64, options: &Options) -> Estimate {
    let kind = detect(&sample[..sample.len().min(detect::SNIFF_LEN)], options.extension.as_deref());
    let whole = |len: u64| Estimate {
        kind,
        original_len: total_len,
        expected_len: len,
        low_len: len,
        high_len: len,
        measured: false,
    };
    let part = |permille: u64| total_len / 1000 * permille + total_len % 1000 * permille / 1000;
    if kind == Kind::Jpeg {
        if transform::wanted(kind, total_len, options).is_none() {
            return whole(total_len);
        }
        return Estimate {
            expected_len: part(790),
            low_len: part(690),
            high_len: part(920),
            ..whole(total_len)
        };
    }
    if kind.is_dense() || sample.is_empty() || total_len == 0 {
        return whole(total_len);
    }
    let sample = &sample[..sample.len().min(ESTIMATE_SAMPLE)];
    let compressed = zstd::bulk::compress(sample, 3).map_or(sample.len(), |c| c.len());
    if !segment::worth_it(compressed, sample.len()) {
        return whole(total_len);
    }
    let permille = (compressed as u64 * 1000).div_ceil(sample.len() as u64);
    // Higher levels, brotli and the transforms do better than level 3 on a sample; the start
    // of a file also says little about its rest.
    Estimate {
        expected_len: part(permille),
        low_len: part(permille * 85 / 100),
        high_len: part((permille * 110 / 100).min(1000)),
        measured: true,
        ..whole(total_len)
    }
}
