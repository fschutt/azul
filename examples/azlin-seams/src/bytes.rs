//! `ByteCount`: the one size type of the engine's pure crates (planning/architecture.md section
//! 1.4 rule 4 "Everything has a limit, stated up front"; section 5.13 "a bot appending all day"
//! and the limits table of section 2.5; build unit S17, lane W1-S17a).
//!
//! A size is a number of bytes and nothing else: there is no `From<u64>` and no `Add`/`Sub`/`Mul`
//! operator. Every operation is checked (`None` on overflow or underflow) or saturating by its
//! name, so a quota, a cap or a counter never wraps and never panics. The constructors for KiB, MiB
//! and GiB take a `u32`, so they cannot overflow (the largest, `gib(u32::MAX)`, is below 2^62) and
//! are `const fn`: `const HEAD_MAX: ByteCount = ByteCount::kib(4);`.

use core::fmt;

/// A number of bytes.
///
/// Implements planning/architecture.md section 1.4 rule 4 (limits are stated up front, in a type
/// that cannot wrap).
///
/// Caller: S16a (`HEAD_MAX`, log caps), S16b (`cap_bytes`, `charge`), S16c (`bytes_cap`, daily
/// caps), S16f (request caps), S17b (`RequestPolicy::max_bytes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteCount(u64);

impl ByteCount {
    /// No bytes.
    pub const ZERO: ByteCount = ByteCount(0);

    /// The largest size (2^64 - 1 bytes).
    pub const MAX: ByteCount = ByteCount(u64::MAX);

    /// `bytes` bytes.
    pub const fn new(bytes: u64) -> ByteCount {
        ByteCount(bytes)
    }

    /// `kib` KiB (1,024 bytes each).
    pub const fn kib(kib: u32) -> ByteCount {
        ByteCount((kib as u64) << 10)
    }

    /// `mib` MiB (1,048,576 bytes each).
    pub const fn mib(mib: u32) -> ByteCount {
        ByteCount((mib as u64) << 20)
    }

    /// `gib` GiB (1,073,741,824 bytes each).
    pub const fn gib(gib: u32) -> ByteCount {
        ByteCount((gib as u64) << 30)
    }

    /// The length of a buffer.
    pub fn of_len(len: usize) -> ByteCount {
        // `usize` fits `u64` on every supported target (the assertion in lib.rs).
        ByteCount(len as u64)
    }

    /// The number of bytes.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Whether this is no bytes at all.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// This size plus `other`; `None` on overflow.
    pub fn checked_add(self, other: ByteCount) -> Option<ByteCount> {
        if mutant!("seams_byte_count_wraps") {
            return Some(ByteCount(self.0.wrapping_add(other.0)));
        }
        self.0.checked_add(other.0).map(ByteCount)
    }

    /// This size minus `other`; `None` when `other` is larger.
    pub fn checked_sub(self, other: ByteCount) -> Option<ByteCount> {
        self.0.checked_sub(other.0).map(ByteCount)
    }

    /// This size `factor` times; `None` on overflow.
    pub fn checked_mul(self, factor: u64) -> Option<ByteCount> {
        self.0.checked_mul(factor).map(ByteCount)
    }

    /// This size plus `other`, at most [`ByteCount::MAX`].
    pub fn saturating_add(self, other: ByteCount) -> ByteCount {
        ByteCount(self.0.saturating_add(other.0))
    }

    /// This size minus `other`, at least [`ByteCount::ZERO`].
    pub fn saturating_sub(self, other: ByteCount) -> ByteCount {
        ByteCount(self.0.saturating_sub(other.0))
    }
}

/// `1024 B`: the number of bytes and the unit, nothing rounded.
impl fmt::Display for ByteCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} B", self.0)
    }
}
