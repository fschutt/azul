//! The codec byte of a chunk blob, the seam to the compressors and the compression rule
//! (planning/architecture.md section 5.7 pipeline step 3: "per-chunk zstd when it saves 5 % (the
//! existing rule)"; build unit S14, lane W1-S14b).
//!
//! The compressors themselves (zstd, brotli, the dictionaries, the whole-file transforms) are
//! `azlin-compress`'s, and this crate does not depend on it: [`Compressor`] is the seam that
//! INT-S14-1 plugs it into. This crate decides only WHETHER a compressed chunk is kept
//! ([`CompressPolicy`]) and writes the codec into the blob, where the associated data authenticates
//! it.
//!
//! The codec byte table is azul-storage's `crypto/codec.rs`: 0 stored as it is, 1 zstd, 2 brotli,
//! 4 zstd with a shipped dictionary (its id first in the stored bytes); 3 (JPEG XL) is reserved
//! and every other byte is refused, by name.
//!
//! ```
//! use azlin_content::{Codec, CodecError};
//!
//! assert_eq!(Codec::from_byte(1), Ok(Codec::Zstd));
//! assert_eq!(Codec::from_byte(4), Ok(Codec::ZstdDict));
//! assert_eq!(Codec::from_byte(3), Err(CodecError::Unsupported(3)));
//! assert_eq!(Codec::Brotli.byte(), 2);
//! ```

use zeroize::Zeroizing;

use crate::error::CodecError;
use crate::limits::COMPRESS_MIN_SAVING;
use crate::units::{ChunkLen, Percent};

/// The codec byte that is kept free (JPEG XL): never written, refused when read.
///
/// Caller: `Codec::from_byte`, the tests of the refusal by name.
pub const CODEC_JPEG_XL: u8 = 3;

/// How a chunk's stored bytes were made from its plaintext: the byte at the start of a chunk blob.
///
/// Caller: `seal_chunk`, `open_chunk` (INT-S14-1), `ChunkMeta`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Codec {
    /// The plaintext as it is (codec byte 0).
    Stored = 0,
    /// One zstd frame (codec byte 1).
    Zstd = 1,
    /// One brotli stream (codec byte 2).
    Brotli = 2,
    /// One zstd frame with one of the shipped dictionaries, its id first (codec byte 4).
    ZstdDict = 4,
}

impl Codec {
    /// The codec byte.
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// The codec of a codec byte; the reserved 3 and every byte no codec uses are refused, with the
    /// byte.
    pub const fn from_byte(byte: u8) -> Result<Codec, CodecError> {
        match byte {
            0 => Ok(Codec::Stored),
            1 => Ok(Codec::Zstd),
            2 => Ok(Codec::Brotli),
            4 => Ok(Codec::ZstdDict),
            other => Err(CodecError::Unsupported(other)),
        }
    }
}

/// What a compressor hands back: the codec and the compressed stream. The stream is `Zeroizing`: it
/// is derived from a chunk's plaintext (DRIFT X-DELTA-W1S14B-5: the brief wrote `Vec<u8>`; QUALITY
/// rule 8 asks for wiped buffers).
///
/// Caller: INT-S14-1 (azlin-compress's adapter builds it), `seal_chunk`.
pub type Compressed = (Codec, Zeroizing<Vec<u8>>);

/// The seam to the compressors of `azlin-compress` (INT-S14-1 implements it): a chunk is
/// compressed here before it is sealed and decompressed after it is opened.
///
/// Both buffers are `Zeroizing`: they hold the chunk's plaintext or bytes derived from it (DRIFT
/// X-DELTA-W1S14B-5: the brief wrote `Vec<u8>`; QUALITY rule 8 asks for wiped buffers).
///
/// Caller: INT-S14-1 (azlin-compress's segment codecs behind this trait); `seal_chunk` and
/// `open_chunk` call it.
pub trait Compressor {
    /// Compresses one chunk's plaintext. `Ok(None)` says "store it as it is" (incompressible, or
    /// not worth the time); `Ok(Some((codec, bytes)))` gives a codec other than
    /// [`Codec::Stored`] and a stream that `decompress` reads back to exactly the plaintext. The
    /// caller keeps the stream only when it saves enough ([`CompressPolicy`]). A failure is an
    /// `Err`: the seal stops.
    fn compress(&self, plaintext: &[u8]) -> Result<Option<Compressed>, CodecError>;

    /// Decompresses `stored`, which `codec` made. `max_len` is the chunk's declared raw length (the
    /// pointer's, rule I2 "the decoder's `max_decoded_len` is the file's declared size from its
    /// sealed pointer"): an implementation produces at most that many bytes and stops with
    /// [`CodecError::TooLong`] at the first byte past it, whatever the stream claims.
    fn decompress(
        &self,
        codec: Codec,
        stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError>;
}

/// A compressor that never compresses and reads no compressed codec: every chunk is stored as it
/// is (codec 0). For the tests, for a device that does not compress, and for the recompression pass
/// that wants a chunk back as it was.
///
/// Caller: INT-S14-1 (a build without azlin-compress), the tests of this crate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoredOnly;

impl Compressor for StoredOnly {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        codec: Codec,
        _stored: &[u8],
        _max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        Err(CodecError::Unsupported(codec.byte()))
    }
}

/// When a compressed chunk is kept: when it saves at least [`COMPRESS_MIN_SAVING`] (5 %) of the
/// plaintext's length. At exactly 95.0 % of the plaintext the stream is kept; at 95.1 % it is not
/// (the rule of `azlin_compress::segment::worth_it`: `stored * 100 <= raw * 95`).
///
/// ```
/// use azlin_content::{CompressPolicy, Percent};
///
/// let policy = CompressPolicy::plan();
/// assert_eq!(policy.min_saving(), Percent::new(5).unwrap());
/// assert_eq!(policy, CompressPolicy::default());
///
/// let plaintext = vec![0u8; 1000];
/// assert!(policy.keeps(&vec![0u8; 949], &plaintext)); // 94.9 %: kept
/// assert!(policy.keeps(&vec![0u8; 950], &plaintext)); // 95.0 %: kept
/// assert!(!policy.keeps(&vec![0u8; 951], &plaintext)); // 95.1 %: not kept
/// assert!(!policy.keeps(&[], &[])); // nothing is worth keeping instead of nothing
/// ```
///
/// Caller: INT-S14-1 (the upload pipeline passes it to `seal_chunk`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressPolicy {
    min_saving: Percent,
}

impl CompressPolicy {
    /// The plan's rule: a compressed chunk is kept when it saves at least 5 %.
    pub const fn plan() -> CompressPolicy {
        CompressPolicy {
            min_saving: COMPRESS_MIN_SAVING,
        }
    }

    /// The share of the plaintext a compressed chunk has to save to be kept.
    pub const fn min_saving(&self) -> Percent {
        self.min_saving
    }

    /// Whether the compressed stream `stored` is worth keeping instead of `plaintext`: it saves at
    /// least [`CompressPolicy::min_saving`] of the plaintext's length. An empty plaintext is never worth
    /// anything. The comparison is exact (128-bit arithmetic: no rounding and no overflow).
    ///
    /// Caller: `seal_chunk` (INT-S14-1 builds the policy it passes).
    pub fn keeps(&self, stored: &[u8], plaintext: &[u8]) -> bool {
        let keep = 100u128.saturating_sub(u128::from(self.min_saving.get()));
        !plaintext.is_empty()
            && (stored.len() as u128).saturating_mul(100)
                <= (plaintext.len() as u128).saturating_mul(keep)
    }
}

impl Default for CompressPolicy {
    fn default() -> CompressPolicy {
        CompressPolicy::plan()
    }
}
