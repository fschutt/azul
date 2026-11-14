//! The checks a NODE can run on a new pack without any key (planning/architecture.md section 2.5
//! "Nodes check what they can. Every pack must have the AZP2 header (magic, version, class, sizes). The
//! node also tests a few kilobytes of each new pack for randomness: real ciphertext is indistinguishable
//! from random bytes, while plaintext compresses. A pack that fails is refused with `not_sealed`. The
//! check catches mistakes, not malice: a determined client can still store weakly encrypted data. That
//! can only ever hurt its own account."; build unit S14, lane W1-S14c, brief P-4).
//!
//! S6's `pack_put` handler calls [`check_new_pack`] on the first bytes it holds and maps every
//! [`NotSealed`] to the status `not_sealed`; the client calls the same functions before an upload.
//!
//! # The randomness test
//!
//! A byte histogram of the sample, and Pearson's chi-square statistic against the uniform
//! distribution: with `n` sample bytes and `c_v` bytes of value `v`,
//! `X2 = sum_v (c_v - n/256)^2 / (n/256) = 256 * S / n - n` where `S = sum_v c_v^2`. The sample is the
//! first [`SAMPLE_LEN`] (4096) bytes after the header, or all the bytes after it when the pack is
//! shorter; a pack with fewer than [`MIN_SAMPLE`] (512) such bytes cannot be tested and is accepted on
//! the header check alone (DRIFT X-DELTA-W1S14C-10). The test is done in integers: the sample is refused
//! when `256 * S > n * (n + T)` with the integer `T` = [`CHI_SQUARE_MAX`], which is `X2 > T` with no
//! rounding.
//!
//! THE THRESHOLD. For random bytes `X2` is chi-square with 255 degrees of freedom (mean 255, standard
//! deviation 22.6). The upper tail of that law is `1e-12` at `T0 = 447.5143` (recomputed by the test
//! `c12_randomness_check_threshold_is_derived` with an incomplete-gamma routine of its own). That alone
//! would refuse real packs, though: a blob starts with its CLEAR codec byte (`codec (1) || nonce (24) ||
//! ciphertext || tag (16)`, brief P-1 of S14b), and a pack of tiny files holds one every 42 bytes, so a
//! sample of 4096 bytes can hold `F = ceil(4096 / 41) = 100` bytes that are all the same value `c` (the
//! codec 0 of a stored chunk): the count of value `c` is then `F` above its mean, and `X2` is about
//! `255 + F^2 / E` (`E = 4096 / 256 = 16`), 870 on average and above 1,100 only in a few of 20,000
//! simulated samples. The threshold must hold for that case too, and does so by the triangle
//! inequality in the Euclidean norm: `X2` is the squared norm of the vector `(c_v - E) / sqrt(E)`, the
//! framing bytes add at most `F / sqrt(E)` to its norm and their lower count of random bytes at most
//! `F / (16 * sqrt(E))` more, so
//!
//! ```text
//! T = (sqrt(T0) + F / sqrt(E) * (1 + 1/16))^2 = (21.155 + 26.5625)^2 = 2276.92  ->  CHI_SQUARE_MAX = 2277
//! ```
//!
//! No legitimate pack scores above it, and the false-refusal rate on random data is the chi-square tail
//! at 2277, far below `1e-12`; the price is that data close to uniform is not caught. What it catches is
//! what the plan asks: zero fill (`X2` about 1,044,000), one repeated byte, text (English about 77,000,
//! JSON), a header of a known file type followed by ASCII, ASCII-only data (95 symbols: about 7,200), any
//! data that uses at most 160 byte values (2,712), and a random block of up to about 450 bytes repeated
//! (`X2` is about `255 * 4096 / block`). It does not catch compressed data (a zstd stream is random to a
//! histogram: documented, "the check catches mistakes, not malice") nor a longer random block repeated.

use super::header::PackHeader;
use super::PackError;
use crate::units::PackLen;

/// The bytes of the sample the randomness test reads at most: the first 4096 bytes after the header.
///
/// Caller: `check_new_pack` and `randomness_check` (INT-S14-4: S6 holds this many bytes of a new pack
/// before it calls), the tests of the check.
pub const SAMPLE_LEN: usize = 4096;

/// The fewest bytes of sample the test can judge: 512. A pack with fewer bytes after the header is
/// accepted on the header check alone, and `randomness_check` itself says
/// [`NotSealed::TooShort`].
///
/// Caller: `check_new_pack` and `randomness_check`, the tests of the check.
pub const MIN_SAMPLE: usize = 512;

/// The threshold of the chi-square statistic: a sample is refused when `X2 > CHI_SQUARE_MAX`, derived
/// in the module documentation (the chi-square(255) tail of `1e-12`, widened by what the clear codec
/// bytes of up to 100 blobs in the sample can add).
///
/// Caller: `randomness_check`, the test `c12_randomness_check_threshold_is_derived`.
pub const CHI_SQUARE_MAX: u32 = 2277;

/// Why a node refuses a new pack with the status `not_sealed` (planning/architecture.md section 2.5).
/// The variants say what failed for the log and the tests; the status is the same for all of them.
///
/// Caller: INT-S14-4 (S6's `pack_put` and `pack_commit` map every variant to `not_sealed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NotSealed {
    /// The first four bytes are not `AZP2`.
    BadMagic,
    /// The version byte is not 1: a pack of a newer format, never parsed.
    UnsupportedVersion(u8),
    /// The class byte is none of 0 content, 1 derived, 2 repository.
    BadClass(u8),
    /// Any other field or size of the header is out of range or does not fit the declared length of
    /// the pack: the reserved bytes, the epoch, the count of chunks, the sizes of the body, the padding
    /// and the trailer against the declared length.
    SizesInconsistent,
    /// The bytes after the header are not random: plaintext, zero fill, repeated blocks.
    NotRandom,
    /// Fewer bytes than the check needs: fewer than 24 for the header, or fewer than the sample the
    /// declared length makes (at most 4096 bytes after the header) for the randomness test; or, for
    /// `randomness_check` itself, a sample under 512 bytes.
    TooShort,
}

impl core::fmt::Display for NotSealed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NotSealed::BadMagic => f.write_str("the pack does not start with the AZP2 magic"),
            NotSealed::UnsupportedVersion(v) => {
                write!(
                    f,
                    "the pack is of version {v}, which this node does not read"
                )
            }
            NotSealed::BadClass(c) => write!(f, "the pack's class byte {c} is no class"),
            NotSealed::SizesInconsistent => {
                f.write_str("the pack's header does not fit its declared length")
            }
            NotSealed::NotRandom => f.write_str("the bytes of the pack are not random"),
            NotSealed::TooShort => f.write_str("too few bytes to check the pack"),
        }
    }
}

impl std::error::Error for NotSealed {}

/// What the node's header check says of a refused header: the three header facts the node can name,
/// and `SizesInconsistent` for every other field or size.
fn not_sealed(e: PackError) -> NotSealed {
    match e {
        PackError::TooShort => NotSealed::TooShort,
        PackError::BadMagic => NotSealed::BadMagic,
        PackError::UnsupportedVersion(v) => NotSealed::UnsupportedVersion(v),
        PackError::BadClass(c) => NotSealed::BadClass(c),
        PackError::ReservedNotZero
        | PackError::BadEpoch
        | PackError::NoChunks
        | PackError::TooManyChunks
        | PackError::TooLong
        | PackError::SizesInconsistent
        | PackError::EmptyPack
        | PackError::DuplicateChunk
        | PackError::BadGeometry
        | PackError::TooBigForAnyPack
        | PackError::Auth
        | PackError::BadTrailer
        | PackError::EntryOutsideBody
        | PackError::LengthMismatch
        | PackError::Chunk(_)
        | PackError::Entropy(_) => NotSealed::SizesInconsistent,
    }
}

/// The AZP2 header check of section 2.5, with no key: the magic, the version, the class byte and the
/// sizes against the length the store declares for the object (a `total_len` above
/// [`super::HARD_MAX`], a body that cannot hold the declared number of chunks, a trailer that is too
/// short or too long for it, a count of zero or above the cap, non-zero reserved bytes, an epoch of
/// 0). `prefix` holds at least the first 24 bytes of the pack; more are ignored.
///
/// Caller: INT-S14-4 (S6's `pack_put`), `check_new_pack`, the client before an upload.
pub fn check_header(prefix: &[u8], total_len: PackLen) -> Result<PackHeader, NotSealed> {
    PackHeader::parse(prefix, total_len).map_err(not_sealed)
}

/// The randomness test of section 2.5 on `sample`, the first bytes after the header of a new pack: at
/// most the first [`SAMPLE_LEN`] bytes are read. The statistic and its threshold are derived in the
/// module documentation. A sample of fewer than [`MIN_SAMPLE`] bytes is [`NotSealed::TooShort`]: there
/// is too little to judge; [`check_new_pack`] does not call this function for such a pack.
///
/// Caller: INT-S14-4 (S6's `pack_put` through `check_new_pack`), the client before an upload.
pub fn randomness_check(sample: &[u8]) -> Result<(), NotSealed> {
    let sample = sample.get(..SAMPLE_LEN).unwrap_or(sample);
    if sample.len() < MIN_SAMPLE {
        return Err(NotSealed::TooShort);
    }
    let mut counts = [0u32; 256];
    for &byte in sample {
        if let Some(count) = counts.get_mut(usize::from(byte)) {
            *count = count.saturating_add(1);
        }
    }
    // `sample` is at most SAMPLE_LEN bytes, so every number below is far from overflow.
    let n = sample.len() as u64;
    let sum_of_squares: u64 = counts.iter().map(|&c| u64::from(c) * u64::from(c)).sum();
    // X2 = 256 * S / n - n is above T exactly when 256 * S is above n * (n + T).
    if sum_of_squares.saturating_mul(256) > n.saturating_mul(n + u64::from(CHI_SQUARE_MAX)) {
        return Err(NotSealed::NotRandom);
    }
    Ok(())
}

/// Both checks on the first bytes the node holds of a new pack of `total_len` bytes: the header check,
/// then the randomness test of the sample after the header (`min(total_len - 24, 4096)` bytes).
/// `first` must hold that many bytes after the header, or [`NotSealed::TooShort`] says the caller has
/// not received enough yet; a pack whose declared length leaves fewer than 512 bytes after the header
/// is accepted on the header check alone (too short to test: DRIFT X-DELTA-W1S14C-10). On success the
/// parsed header is returned.
///
/// Caller: INT-S14-4 (S6's `pack_put` and `pack_commit`: `not_sealed` on any `Err`).
pub fn check_new_pack(first: &[u8], total_len: PackLen) -> Result<PackHeader, NotSealed> {
    let header = check_header(first, total_len)?;
    // The header check passed, so `total_len` is above the header's own 24 bytes.
    let after_header = total_len.as_usize().saturating_sub(PackHeader::LEN);
    let wanted = after_header.min(SAMPLE_LEN);
    if wanted < MIN_SAMPLE {
        return Ok(header);
    }
    let end = PackHeader::LEN.saturating_add(wanted);
    let sample = first.get(PackHeader::LEN..end).ok_or(NotSealed::TooShort)?;
    randomness_check(sample)?;
    Ok(header)
}
