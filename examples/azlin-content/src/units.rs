//! Typed units of the content family: the lengths, offsets, sizes and times of chunks, packs and
//! files (planning/architecture.md section 5.7; COMMON rule 7: no raw `u32`, `u64` or `i64` size,
//! offset or time in a public API; build unit S14, lane W1-S14b).
//!
//! | type | what | range |
//! |---|---|---|
//! | [`ChunkLen`] | the length of a chunk's plaintext (`raw_len`) or of its stored blob (`stored_len`, the `length` of a pack entry) | 1 to [`crate::limits::MAX_BLOB`]; never zero |
//! | [`PackLen`] | the length of a pack | any `u32` (a pack is a few MiB; the pack lane checks its own cap) |
//! | [`PackOffset`] | where a blob starts inside a pack | any `u32` |
//! | [`FileSize`] | the size of a file | any `u64` (the pointer lane caps what it accepts) |
//! | [`FileTime`] | a file's modification time: seconds (before 1970 allowed) and nanoseconds | nanoseconds below one second |
//! | [`Percent`] | a share of 100 | 0 to 100 |
//!
//! None of them has a `From<u32>`, `From<u64>` or `Default`: a unit is made by its constructor, which
//! a reviewer can grep, and every sum is checked (`None` on overflow, never a wrap or a clamp).
//!
//! ```
//! use azlin_content::{ChunkLen, PackOffset};
//!
//! let len = ChunkLen::new(1_000).unwrap();
//! let end = PackOffset::new(24).checked_add(len).unwrap();
//! assert_eq!(end.get(), 1_024);
//! assert!(ChunkLen::new(0).is_none()); // an empty chunk does not exist
//! assert!(PackOffset::new(u32::MAX).checked_add(len).is_none()); // no wrap
//! ```
//!
//! A bare number is not a length:
//!
//! ```compile_fail
//! use azlin_content::ChunkLen;
//! let _len: ChunkLen = 1_000u32.into();
//! ```

use core::fmt;

use crate::limits::MAX_BLOB_BYTES;

/// The length of a chunk: of its plaintext (`raw_len`) or of the blob a pack stores (`stored_len`).
/// Always at least 1 (an empty file has an empty chunk list, brief P-5) and at most
/// [`crate::limits::MAX_BLOB`] (the largest chunk plus the blob framing); a `raw_len` is further
/// bounded by [`crate::limits::MAX_CHUNK`] where it is used (`ChunkRef::check`, `open_chunk`).
///
/// Caller: INT-S14-1 (the upload pipeline), S14c (pack entries), S14d (pointer chunks), S14e1
/// (index locations).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkLen(u32);

impl ChunkLen {
    /// The length `bytes`; `None` for 0 and for a length above [`crate::limits::MAX_BLOB`].
    pub const fn new(bytes: u32) -> Option<ChunkLen> {
        if bytes == 0 || bytes > MAX_BLOB_BYTES {
            None
        } else {
            Some(ChunkLen(bytes))
        }
    }

    /// The length of a slice or buffer; `None` for 0, for one above [`crate::limits::MAX_BLOB`]
    /// and for one that does not fit a `u32`.
    pub fn from_len(len: usize) -> Option<ChunkLen> {
        match u32::try_from(len) {
            Ok(bytes) => ChunkLen::new(bytes),
            Err(_) => None,
        }
    }

    /// A length that is known to be in range. Crate-private: the constants of `limits.rs` and
    /// the code that has just checked the bound.
    pub(crate) const fn from_raw(bytes: u32) -> ChunkLen {
        ChunkLen(bytes)
    }

    /// The number of bytes (at least 1).
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The number of bytes as a `usize` (every target of this crate has at least 32 bits).
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Decimal bytes.
impl fmt::Display for ChunkLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ChunkLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkLen({})", self.0)
    }
}

/// The length of a pack in bytes: the sealed object a node stores (planning/architecture.md
/// section 5.7 "Pack (AZP2)"). Any `u32`; the pack lane checks its own geometry.
///
/// Caller: S14c (the builder and the reader), S14e1 (`PackInfo::len`), S14e2 (the repack budget).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackLen(u32);

impl PackLen {
    /// No bytes.
    pub const ZERO: PackLen = PackLen(0);

    /// A pack length of `bytes`.
    pub const fn new(bytes: u32) -> PackLen {
        PackLen(bytes)
    }

    /// The length of a slice or buffer; `None` when it does not fit a `u32`.
    pub fn from_len(len: usize) -> Option<PackLen> {
        match u32::try_from(len) {
            Ok(bytes) => Some(PackLen(bytes)),
            Err(_) => None,
        }
    }

    /// The number of bytes.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The number of bytes as a `usize` (every target of this crate has at least 32 bits).
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }

    /// This length plus `other`; `None` on overflow.
    pub fn checked_add(self, other: PackLen) -> Option<PackLen> {
        self.0.checked_add(other.0).map(PackLen)
    }

    /// This length minus `other`; `None` when `other` is longer.
    pub fn checked_sub(self, other: PackLen) -> Option<PackLen> {
        self.0.checked_sub(other.0).map(PackLen)
    }

    /// This length plus the length of one more chunk blob; `None` on overflow.
    pub fn checked_add_chunk(self, chunk: ChunkLen) -> Option<PackLen> {
        self.0.checked_add(chunk.0).map(PackLen)
    }
}

/// Decimal bytes.
impl fmt::Display for PackLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for PackLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PackLen({})", self.0)
    }
}

/// Where a chunk blob starts inside a pack, in bytes from the first byte of the pack.
///
/// Caller: S14c (trailer lines, `chunk_range`), S14d (pointer chunks), S14e1 (`ChunkLoc`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackOffset(u32);

impl PackOffset {
    /// The start of the pack.
    pub const ZERO: PackOffset = PackOffset(0);

    /// The offset `bytes`.
    pub const fn new(bytes: u32) -> PackOffset {
        PackOffset(bytes)
    }

    /// The number of bytes from the start of the pack.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The offset as a `usize` (every target of this crate has at least 32 bits).
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }

    /// The offset `len` bytes further on; `None` on overflow.
    pub fn checked_add(self, len: ChunkLen) -> Option<PackOffset> {
        self.0.checked_add(len.0).map(PackOffset)
    }
}

/// Decimal bytes.
impl fmt::Display for PackOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for PackOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PackOffset({})", self.0)
    }
}

/// The size of a file in bytes: `size` of a pointer (planning/architecture.md section 5.7
/// "Pointer v2"). Any `u64`; the pointer lane caps what it accepts (rule I2: the declared size
/// bounds every decoder).
///
/// Caller: S14d (the pointer), INT-S14-1 (the upload pipeline's totals).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileSize(u64);

impl FileSize {
    /// An empty file.
    pub const ZERO: FileSize = FileSize(0);

    /// A size of `bytes`.
    pub const fn new(bytes: u64) -> FileSize {
        FileSize(bytes)
    }

    /// The number of bytes.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// This size plus `other`; `None` on overflow.
    pub fn checked_add(self, other: FileSize) -> Option<FileSize> {
        self.0.checked_add(other.0).map(FileSize)
    }

    /// This size plus the plaintext length of one more chunk; `None` on overflow.
    pub fn checked_add_chunk(self, chunk: ChunkLen) -> Option<FileSize> {
        self.0.checked_add(u64::from(chunk.0)).map(FileSize)
    }
}

/// Decimal bytes.
impl fmt::Display for FileSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for FileSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileSize({})", self.0)
    }
}

/// A file's modification time: whole seconds since 1970-01-01 00:00:00 UTC (negative before it) and
/// the nanoseconds past that second, below one second (brief P-4 of S14d: `mtime` may be before
/// 1970). The ordering is the time's.
///
/// ```
/// use azlin_content::FileTime;
///
/// let before = FileTime::new(-1, 999_999_999).unwrap();
/// let after = FileTime::new(0, 0).unwrap();
/// assert!(before < after);
/// assert!(FileTime::new(0, 1_000_000_000).is_none());
/// ```
///
/// Caller: S14d (the pointer's `mtime`), INT-S14-1 (the file's time from the file system).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FileTime {
    secs: i64,
    nanos: u32,
}

impl FileTime {
    /// Nanoseconds in one second: `nanos` is always below this.
    pub const NANOS_PER_SEC: u32 = 1_000_000_000;

    /// 1970-01-01 00:00:00 UTC.
    pub const UNIX_EPOCH: FileTime = FileTime { secs: 0, nanos: 0 };

    /// The time `secs` seconds and `nanos` nanoseconds after the epoch; `None` when `nanos` is
    /// one second or more.
    pub const fn new(secs: i64, nanos: u32) -> Option<FileTime> {
        if nanos >= FileTime::NANOS_PER_SEC {
            None
        } else {
            Some(FileTime { secs, nanos })
        }
    }

    /// The whole seconds since the epoch (negative before it).
    pub const fn secs(self) -> i64 {
        self.secs
    }

    /// The nanoseconds past the second, below [`FileTime::NANOS_PER_SEC`].
    pub const fn nanos(self) -> u32 {
        self.nanos
    }
}

/// A share of one hundred, 0 to 100 (the plan's "per-chunk zstd when it saves 5 %").
///
/// Caller: `limits::COMPRESS_MIN_SAVING`, S14e2 (the live floor of a pack).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percent(u8);

impl Percent {
    /// `percent` per cent; `None` above 100.
    pub const fn new(percent: u8) -> Option<Percent> {
        if percent > 100 {
            None
        } else {
            Some(Percent(percent))
        }
    }

    /// A percentage that is known to be in range. Crate-private: the constants of `limits.rs`.
    pub(crate) const fn from_raw(percent: u8) -> Percent {
        Percent(percent)
    }

    /// The number of per cent, 0 to 100.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// `5%`.
impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

impl fmt::Debug for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Percent({})", self.0)
    }
}
