//! Numbers somebody else wrote: what an object, a header or a length may be before this device
//! spends memory or time on it (ARCH28 RF50 / AR5b-5; the read side of RF48's `get_limited`).
//!
//! A [`Limit`] is a named maximum. The name is what the error says ("a key file may be at most
//! 65536 bytes"), so a refusal tells which rule held, not only that a number was too big.
//! [`Declared`] is a number somebody else wrote (a header, an index entry, a chunk size): it can
//! be compared with a [`Limit`], turned into a position or a span of a slice the reader HAS, or
//! into a first reservation of at most [`PREALLOC`] - and into nothing else, so a size read from
//! bytes cannot size an allocation or an index by accident. [`ByteReader`] takes bytes by
//! [`Declared`] counts and cannot run past its end. [`named`] holds the limits the formats of
//! this crate use.

use std::{fmt, ops::Range};

use crate::{Drive, DriveError};

/// What a number read from a header, an index entry or a chunk size may make this device reserve up
/// front. What is really read grows the buffer after that.
pub const PREALLOC: usize = 1 << 20;

/// A named maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit {
    /// What the limit is for, as an error names it: `"a key file"`.
    pub name: &'static str,
    /// The most bytes.
    pub max: u64,
}

impl Limit {
    #[must_use]
    pub const fn new(name: &'static str, max: u64) -> Limit {
        Limit { name, max }
    }

    /// `got` when it is within the limit, else the error that names the limit.
    ///
    /// # Errors
    ///
    /// [`LimitError`] when `got` is more than [`Limit::max`].
    pub fn within(self, got: u64) -> Result<u64, LimitError> {
        if got > self.max {
            Err(LimitError { limit: self, got })
        } else {
            Ok(got)
        }
    }

    /// [`Limit::within`] for the object `key` of a drive: the refusal is a [`DriveError`] that
    /// names the key and the limit.
    ///
    /// # Errors
    ///
    /// [`DriveError::Protocol`] when `got` is more than [`Limit::max`].
    pub fn check_object(self, key: &str, got: u64) -> Result<u64, DriveError> {
        self.within(got).map_err(|e| e.of_object(key))
    }

    /// The object `key` of `bucket` when it is at most this big: [`Drive::get_limited`] with the
    /// refusal for size made ONE error whatever kind of drive answers. A drive that checks after
    /// the read says `Protocol`; a bucket whose transport refuses the answer on its way says a
    /// transport error whose text is not for sorting. So an answer that failed is asked once more
    /// as a HEAD, which says the size as a number: an object found over the limit is
    /// [`DriveError::Corrupt`] (a file nobody who holds the key wrote), anything else is the
    /// failure it was (a network that went is not "a file that is too big").
    ///
    /// # Errors
    ///
    /// `NotFound` for no object, `Corrupt` for one over the limit, else the drive's own.
    pub fn read_from(self, bucket: &dyn Drive, key: &str) -> Result<Vec<u8>, DriveError> {
        match bucket.get_limited(key, self) {
            Ok(bytes) => Ok(bytes),
            Err(
                e @ (DriveError::Protocol(_)
                | DriveError::Transport(_)
                | DriveError::Service(_)
                | DriveError::Io(_)),
            ) => match bucket.head(key) {
                Ok(info) if info.size > self.max => Err(DriveError::Corrupt {
                    key: key.to_string(),
                    reason: LimitError {
                        limit: self,
                        got: info.size,
                    }
                    .to_string(),
                }),
                // A HEAD that failed, or found the object small, says nothing about its size.
                _ => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    /// The limit for an object whose HEAD said `head_size`: that size - an answer past it is not the
    /// object the HEAD saw (a storage that streams for ever, a body swapped for a bigger one) - or
    /// `ceiling` when the HEAD said nothing (a missing Content-Length reads as 0) or more than that.
    #[must_use]
    pub const fn as_the_head_said(name: &'static str, head_size: u64, ceiling: u64) -> Limit {
        Limit::new(
            name,
            if head_size == 0 || head_size > ceiling {
                ceiling
            } else {
                head_size
            },
        )
    }

    /// [`Drive::get_limited`] for a drive that cannot hand its transport the limit (a service's SDK,
    /// a database): the size first - a stat reads no byte - and the object read only when it is
    /// within the limit, its length checked once more after the read (it may have grown since).
    /// A drive that cannot say the size leaves the read to decide, as it did.
    ///
    /// # Errors
    ///
    /// `NotFound` for no object, [`DriveError::Protocol`] for one over the limit, else the drive's own.
    pub fn get_after_looking(self, drive: &dyn Drive, key: &str) -> Result<Vec<u8>, DriveError> {
        match drive.head(key) {
            Ok(info) => {
                self.check_object(key, info.size)?;
            }
            Err(e @ DriveError::NotFound { .. }) => return Err(e),
            Err(_) => {}
        }
        let bytes = drive.get(key)?;
        self.check_object(key, bytes.len() as u64)?;
        Ok(bytes)
    }
}

/// A length past its [`Limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError {
    pub limit: Limit,
    /// The length that was declared or found.
    pub got: u64,
}

impl LimitError {
    /// As the error of a drive call on `key`.
    #[must_use]
    pub fn of_object(self, key: &str) -> DriveError {
        DriveError::Protocol(format!("\"{key}\": {self}"))
    }
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} may be at most {} bytes, this one is {}",
            self.limit.name, self.limit.max, self.got
        )
    }
}

impl std::error::Error for LimitError {}

impl From<LimitError> for DriveError {
    fn from(e: LimitError) -> Self {
        DriveError::Protocol(e.to_string())
    }
}

/// A number somebody else wrote - a size, a count or an offset read from a header, an index entry
/// or a chunk line. It has no `Add`, no conversion to `usize` and no `Index`: the only things
/// to do with it are to compare it with a [`Limit`] ([`Declared::within`]), to place it in a slice
/// the reader holds ([`Declared::index_in`], [`Declared::span_in`]) and to reserve at most
/// [`PREALLOC`] bytes for it ([`Declared::prealloc`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Declared(u64);

impl Declared {
    /// `n`, as somebody else's word.
    #[must_use]
    pub const fn new(n: u64) -> Declared {
        Declared(n)
    }

    /// The number when it is within `limit`.
    ///
    /// # Errors
    ///
    /// [`LimitError`] naming the limit.
    pub fn within(self, limit: Limit) -> Result<u64, LimitError> {
        limit.within(self.0)
    }

    /// How much to reserve up front for this many bytes still to come: at most [`PREALLOC`].
    #[must_use]
    pub fn prealloc(self) -> usize {
        usize::try_from(self.0).map_or(PREALLOC, |n| n.min(PREALLOC))
    }

    /// The number as a position in a slice of `len` elements (`0..len`).
    ///
    /// # Errors
    ///
    /// [`LimitError`] when it points past the slice.
    pub fn index_in(self, len: usize) -> Result<usize, LimitError> {
        match usize::try_from(self.0) {
            Ok(at) if at < len => Ok(at),
            _ => Err(LimitError {
                limit: Limit::new("a position inside the data it points into", len as u64),
                got: self.0,
            }),
        }
    }

    /// The span that starts here and is `length` long, inside a slice of `total` elements - the
    /// addition checked, so an offset and a length that wrap are an error, not a span.
    ///
    /// # Errors
    ///
    /// [`LimitError`] when the span does not lie in the slice.
    pub fn span_in(self, length: Declared, total: usize) -> Result<Range<usize>, LimitError> {
        let end = self.0.checked_add(length.0);
        let (Ok(start), Some(Ok(end))) = (usize::try_from(self.0), end.map(usize::try_from)) else {
            return Err(LimitError {
                limit: Limit::new("a span inside the data it points into", total as u64),
                got: u64::MAX,
            });
        };
        if end > total {
            return Err(LimitError {
                limit: Limit::new("a span inside the data it points into", total as u64),
                got: self.0.saturating_add(length.0),
            });
        }
        Ok(start..end)
    }
}

/// The reader ran out of bytes: it wanted `wanted` and had `had`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortRead {
    pub wanted: u64,
    pub had: usize,
}

impl fmt::Display for ShortRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "it broke off: {} bytes were wanted and {} were there",
            self.wanted, self.had
        )
    }
}

impl std::error::Error for ShortRead {}

/// Bytes taken by [`Declared`] counts. It never indexes past its end and never adds: a size read
/// from the bytes that is more than there is gives a [`ShortRead`], whatever the number.
#[derive(Clone, Copy, Debug)]
pub struct ByteReader<'a> {
    rest: &'a [u8],
}

impl<'a> ByteReader<'a> {
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> ByteReader<'a> {
        ByteReader { rest: bytes }
    }

    /// The bytes not taken yet.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.rest
    }

    /// Whether every byte was taken.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next `n` bytes.
    ///
    /// # Errors
    ///
    /// [`ShortRead`] when fewer are left (nothing is taken then).
    pub fn take(&mut self, n: Declared) -> Result<&'a [u8], ShortRead> {
        let short = ShortRead {
            wanted: n.0,
            had: self.rest.len(),
        };
        let count = usize::try_from(n.0).map_err(|_| short)?;
        let (taken, rest) = self.rest.split_at_checked(count).ok_or(short)?;
        self.rest = rest;
        Ok(taken)
    }

    /// The bytes up to the next CRLF (not included), the CRLF taken.
    ///
    /// # Errors
    ///
    /// [`ShortRead`] when there is no CRLF left.
    pub fn line(&mut self) -> Result<&'a [u8], ShortRead> {
        let end = self
            .rest
            .windows(2)
            .position(|pair| pair == b"\r\n")
            .ok_or(ShortRead {
                wanted: self.rest.len() as u64 + 1,
                had: self.rest.len(),
            })?;
        let line = self.take(Declared::new(end as u64))?;
        self.take(Declared::new(2))?;
        Ok(line)
    }
}

/// The limits the formats of this crate read their sizes against. Each is the largest value this
/// software WRITES with some room (the work order's interim caps, revisited when real drives are
/// measured): a file or an object over it is somebody else's doing.
pub mod named {
    use super::Limit;

    /// A key file of `.azlin/keys/`: a member's wrap, the recovery wrap, a further recovery code's.
    pub const KEY_FILE: Limit = Limit::new("a key file", 64 << 10);
    /// The key chain file (`_chain.key`): up to 256 links.
    pub const KEY_CHAIN: Limit = Limit::new("a key chain file", 256 << 10);
    /// The rotation journal (`_rotation.key`): ids and a phase.
    pub const JOURNAL: Limit = Limit::new("a rotation journal", 64 << 10);
    /// An object of the drive index (a pack, a pointer blob, a checkpoint): interim, until real
    /// drives are measured.
    pub const INDEX_OBJECT: Limit = Limit::new("an object of the drive index", 256 << 20);
    /// A share's manifest.
    pub const SHARE_MANIFEST: Limit = Limit::new("a share manifest", 16 << 20);
    /// The most a sync record or a sync index may weigh when its HEAD said nothing: about 500 MB of
    /// JSON for a folder of two million files, with room.
    pub const SYNC_OBJECT_CEILING: u64 = 4 << 30;
    /// One chunk of an HTTP/1.1 answer: the largest body any answer of the nodes holds.
    pub const HTTP_CHUNK: Limit = Limit::new("a chunk of an answer", 256 << 20);
    /// The pages of ONE listing a caller follows before it takes the listing for endless: at the
    /// default 1000 keys a page that is 100 million keys, many times the objects of the largest
    /// drive the index describes (a pack index holds about 6 million). A bucket that never ends a
    /// listing (a hostile one, a broken one) must not keep a caller reading and growing for ever
    /// (X-DELTA-AZDRIVE-KEYS-22).
    pub const LISTING_PAGES: usize = 100_000;
    /// The most entries one listing keeps before it is refused: more than the drive the index
    /// describes (a pack index holds about 6 million), and a thousand-entry page for the page cap
    /// would be a hundred million of them - gigabytes - before [`LISTING_PAGES`] stopped it
    /// (X-DELTA-AZDRIVE-KEYS-22).
    pub const LISTING_ENTRIES: usize = 10_000_000;
    /// The pages of the root-level listing that decides whether a bucket holds a file of the drive
    /// at all, 8 entries a page: 128 entries at the root are many times the bookkeeping of a new
    /// drive. A listing that is not over by then says nothing of an empty bucket.
    pub const ROOT_SCAN_PAGES: usize = 16;
}
