//! The content-defined chunker of the pipeline: FastCDC with normalized chunking over a gear hash
//! (planning/architecture.md section 5.7 pipeline step 2: "FastCDC over the transformed stream
//! (average 1 MiB, minimum 256 KiB, maximum 4 MiB; a file under 256 KiB is one chunk)"; brief P-2;
//! DRIFT X-DELTA-W1S14B-2 and -6; build unit S14, lane W1-S14b).
//!
//! # The algorithm, and why each number is what it is
//!
//! The variant is the normalized-chunking FastCDC of Xia et al. (USENIX ATC 2016) at normalization
//! level 2, over the gear hash `fp = (fp << 1) + GEAR[byte]` on a `u64` (so the fingerprint is a
//! function of the last 64 bytes and nothing else):
//!
//! 1. **Cut-point skipping.** No cut is tested before the chunk is [`MIN_CHUNK`] (256 KiB) long. The
//!    bytes before the last 64 of that stretch are not even hashed; the last 63 of them are hashed
//!    and not tested, so that the first test, at exactly `MIN_CHUNK`, already sees a full 64-byte
//!    window (a decision is a function of the window and of the chunk's length, never of how the
//!    chunk began).
//! 2. **Strict, then loose.** Below [`NORMAL_CHUNK`] (848 KiB) a cut is taken when the top 22 bits of
//!    the fingerprint are zero ([`MASK_STRICT`], probability 2^-22 per byte); from `NORMAL_CHUNK` on,
//!    when the top 18 bits are ([`MASK_LOOSE`], 2^-18 per byte). The loose mask is a subset of the
//!    strict one. The two probabilities concentrate the chunk lengths around the normal size, and
//!    `NORMAL_CHUNK` is set so that the expected length is the plan's 1 MiB average
//!    (`tests/chunker_distribution.rs` recomputes the model and measures the real thing).
//! 3. **Forced cut.** A chunk is cut at [`MAX_CHUNK`] (4 MiB) whatever the content says.
//! 4. **A short rest is one chunk.** A stream of at most `MIN_CHUNK` bytes is a single chunk, and the
//!    last chunk of any stream is whatever is left (1 byte up to `MAX_CHUNK`).
//!
//! Only `u64` arithmetic is in the hash, so the same boundaries come out on 32-bit and 64-bit
//! targets; the boundaries of fixed inputs are pinned in `tests/vectors/chunker_golden.json`.
//!
//! # Two entry points, one rule
//!
//! [`Chunker::cut`] cuts a slice that is in memory; [`Chunker::stream`] cuts a stream of any size
//! with memory bounded by [`MAX_CHUNK`] whatever the file size: the same scanner runs under both, so
//! the chunks are the same bytes whatever the sizes of the pieces fed in.
//!
//! ```
//! use azlin_content::{Chunker, GearTable};
//!
//! let data: Vec<u8> = (0..600_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
//! let chunker = Chunker::new(GearTable::public_v1());
//!
//! // In memory: contiguous, non-empty ranges that cover the input.
//! let ranges = chunker.cut(&data);
//! assert_eq!(ranges.first().map(|r| r.start), Some(0));
//! assert_eq!(ranges.last().map(|r| r.end), Some(data.len()));
//!
//! // As a stream, in pieces of 100,000 bytes: the same chunks.
//! let mut stream = chunker.stream();
//! let mut chunks = Vec::new();
//! for piece in data.chunks(100_000) {
//!     stream.feed(piece, &mut chunks);
//! }
//! stream.finish(&mut chunks);
//! let streamed: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
//! let in_memory: Vec<&[u8]> = ranges.iter().map(|r| &data[r.clone()]).collect();
//! assert_eq!(streamed, in_memory);
//! ```

use core::ops::Range;
use std::sync::Arc;

use zeroize::{Zeroize, Zeroizing};

use crate::gear::GearTable;
use crate::limits::{MAX_CHUNK, MIN_CHUNK, NORMAL_CHUNK};

/// The shortest chunk, as a slice length.
const MIN: usize = MIN_CHUNK.as_usize();
/// Where the strict mask gives way to the loose one, as a chunk length.
const NORMAL: usize = NORMAL_CHUNK.as_usize();
/// The longest chunk, as a slice length.
const MAX: usize = MAX_CHUNK.as_usize();

/// The bytes of history the fingerprint holds: a `u64` shifted left once per byte forgets a byte
/// after 64 steps.
///
/// Caller: the tests of the chunker (they check the first test of a chunk sees this many bytes).
pub const WINDOW: usize = 64;

/// The strict mask: the top 22 bits of the fingerprint (normalization level 2: 20 + 2 bits).
///
/// Caller: the tests of the chunker, INT-WIRE (the contract fixture of the chunker).
pub const MASK_STRICT: u64 = 0xFFFF_FC00_0000_0000;

/// The loose mask: the top 18 bits of the fingerprint (20 - 2 bits), a subset of [`MASK_STRICT`].
///
/// Caller: the tests of the chunker, INT-WIRE (the contract fixture of the chunker).
pub const MASK_LOOSE: u64 = 0xFFFF_C000_0000_0000;

/// The index of the first byte of a chunk that is hashed: the 64 bytes before the first cut test.
const HASH_FROM: usize = MIN - WINDOW;

/// The size the stream chunker's buffer starts at (it doubles, up to [`MAX_CHUNK`]).
const FIRST_BUFFER: usize = 64 * 1024;

/// The scanner of one chunk: the fingerprint and the number of bytes seen. The one place the cut
/// rule is written; the one-shot and the streaming chunker both run it.
struct Scan {
    fp: u64,
    len: usize,
}

impl Scan {
    fn new() -> Scan {
        Scan { fp: 0, len: 0 }
    }

    /// Starts the next chunk.
    fn reset(&mut self) {
        self.fp = 0;
        self.len = 0;
    }

    /// Feeds `bytes`, the next bytes of the current chunk. Returns `Some(n)` when the chunk ends
    /// after the first `n` bytes of `bytes` (`n >= 1`), `None` when it goes on past them all.
    fn advance(&mut self, table: &GearTable, bytes: &[u8]) -> Option<usize> {
        // The bytes before the window of the first test are neither hashed nor tested.
        let skip = HASH_FROM.saturating_sub(self.len).min(bytes.len());
        self.len = self.len.saturating_add(skip);
        for (index, &byte) in bytes.iter().enumerate().skip(skip) {
            self.fp = (self.fp << 1).wrapping_add(table.gear(byte));
            self.len = self.len.saturating_add(1);
            if self.len >= MIN {
                let mask = if self.len < NORMAL {
                    MASK_STRICT
                } else {
                    MASK_LOOSE
                };
                if (self.fp & mask) == 0 {
                    return Some(index.saturating_add(1));
                }
            }
            if self.len >= MAX {
                return Some(index.saturating_add(1));
            }
        }
        None
    }
}

/// The chunker: a gear table and the plan's sizes. Cheap to clone (the table is shared).
///
/// Caller: INT-S14-1 (azul-storage's upload pipeline), INT-S14-2 (azcloud-kit's sync engine).
#[derive(Clone, Debug)]
pub struct Chunker {
    table: Arc<GearTable>,
}

impl Chunker {
    /// A chunker over this table. There is no default table: the caller chooses
    /// [`GearTable::public_v1`] or [`GearTable::keyed`] (the integration unit's default is the
    /// keyed one, decision X-D-43 of ARCH29 9.13).
    pub fn new(table: GearTable) -> Chunker {
        Chunker {
            table: Arc::new(table),
        }
    }

    /// Cuts a slice that is in memory into chunks: contiguous, non-empty ranges that cover it, each
    /// between [`MIN_CHUNK`] and [`MAX_CHUNK`] bytes long except the last, which may be as short as
    /// 1 byte. An empty slice has no chunks; a slice of at most `MIN_CHUNK` bytes is one.
    pub fn cut(&self, data: &[u8]) -> Vec<Range<usize>> {
        let mut ranges = Vec::with_capacity(data.len() / MIN + 1);
        let mut scan = Scan::new();
        let mut start = 0usize;
        while let Some(rest) = data.get(start..).filter(|rest| !rest.is_empty()) {
            scan.reset();
            let len = scan.advance(&self.table, rest).unwrap_or(rest.len());
            let end = start.saturating_add(len);
            ranges.push(start..end);
            start = end;
        }
        ranges
    }

    /// Starts a stream: bytes go in with [`StreamChunker::feed`], whole chunks come out. Memory is
    /// bounded by [`MAX_CHUNK`] whatever the size of the file.
    pub fn stream(&self) -> StreamChunker {
        StreamChunker {
            table: Arc::clone(&self.table),
            buf: Zeroizing::new(Vec::new()),
            scan: Scan::new(),
        }
    }
}

/// A chunker fed in pieces: it holds the chunk being cut (at most [`MAX_CHUNK`] bytes) and hands out
/// each chunk as soon as its cut is found. The buffer and the chunks are `Zeroizing`: a chunk is a
/// piece of the user's file (DRIFT X-DELTA-W1S14B-5). The buffer is allocated in doubling steps that
/// copy and wipe, never by `Vec`'s own growth, so no unwiped copy of the plaintext is left in freed
/// memory.
///
/// Caller: INT-S14-1 (the upload pipeline reads a file piece by piece), INT-S14-2.
pub struct StreamChunker {
    table: Arc<GearTable>,
    buf: Zeroizing<Vec<u8>>,
    scan: Scan,
}

impl StreamChunker {
    /// Feeds the next bytes of the stream. Every chunk whose cut falls inside them is pushed onto
    /// `out`, in order; the bytes after the last cut stay in the buffer for the next call.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Zeroizing<Vec<u8>>>) {
        let mut rest = bytes;
        while !rest.is_empty() {
            // The current chunk takes at most what is left of the maximum.
            let room = MAX.saturating_sub(self.buf.len());
            let take = room.min(rest.len());
            // `take <= rest.len()` by the `min` above, so the split cannot fail.
            let (piece, later) = rest.split_at(take);
            self.make_room(piece.len());
            match self.scan.advance(&self.table, piece) {
                Some(n) => {
                    // The chunk ends after `n <= piece.len()` bytes of this piece.
                    let (head, _) = piece.split_at(n);
                    self.buf.extend_from_slice(head);
                    self.emit(out);
                    rest = rest.split_at(n).1;
                }
                None => {
                    self.buf.extend_from_slice(piece);
                    rest = later;
                }
            }
        }
    }

    /// Ends the stream: the chunk being cut, if any, is the last chunk (it may be as short as one
    /// byte). An empty stream has no chunks.
    pub fn finish(mut self, out: &mut Vec<Zeroizing<Vec<u8>>>) {
        if !self.buf.is_empty() {
            self.emit(out);
        }
    }

    /// The bytes of the current, unfinished chunk the stream holds (at most [`MAX_CHUNK`]).
    ///
    /// Caller: the memory test C6 of this crate, INT-S14-1 (its memory budget).
    pub fn buffered_len(&self) -> usize {
        self.buf.len()
    }

    /// The capacity of the stream's buffer: never more than [`MAX_CHUNK`], whatever was fed.
    ///
    /// Caller: the memory test C6 of this crate, INT-S14-1 (its memory budget).
    pub fn buffered_capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// Makes room for `extra` more bytes in the buffer without a reallocation that would leave the
    /// plaintext behind: a bigger buffer is allocated, the bytes are copied, the old buffer is wiped.
    /// `buf.len() + extra` is at most `MAX` (the caller took at most what the chunk has room for).
    fn make_room(&mut self, extra: usize) {
        let need = self.buf.len().saturating_add(extra);
        if need <= self.buf.capacity() {
            return;
        }
        let doubled = self.buf.capacity().saturating_mul(2).max(FIRST_BUFFER);
        let capacity = doubled.min(MAX).max(need);
        let mut bigger = Zeroizing::new(Vec::with_capacity(capacity));
        bigger.extend_from_slice(&self.buf);
        self.buf.as_mut_slice().zeroize();
        self.buf = bigger;
    }

    /// Hands the buffered chunk to `out` as a chunk of its own (exactly its length), wipes the
    /// buffer and starts the next chunk.
    fn emit(&mut self, out: &mut Vec<Zeroizing<Vec<u8>>>) {
        out.push(Zeroizing::new(self.buf.to_vec()));
        self.buf.as_mut_slice().zeroize();
        self.buf.clear();
        self.scan.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_masks_are_the_documented_ones() {
        assert_eq!(MASK_STRICT.count_ones(), 22);
        assert_eq!(MASK_LOOSE.count_ones(), 18);
        assert_eq!(
            MASK_STRICT & MASK_LOOSE,
            MASK_LOOSE,
            "the loose mask is a subset"
        );
        assert_eq!(MASK_STRICT.leading_ones(), 22, "the top bits");
        assert_eq!(MASK_LOOSE.leading_ones(), 18, "the top bits");
        assert_eq!(
            WINDOW,
            u64::BITS as usize,
            "a u64 forgets a byte after 64 shifts"
        );
        assert_eq!(HASH_FROM + WINDOW, MIN);
    }
}
