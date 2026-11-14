//! The seams of planning/architecture.md section 1.4 rule 1 ("I/O is a parameter. Every
//! component takes its network, disk, clock and randomness as an interface"): randomness
//! ([`Entropy`]), time ([`Clock`], [`UnixTime`], [`Span`]) and the one view of a space chain that
//! the key rules need ([`SpaceChainView`]). Build unit S15, lane W1-S15a.
//!
//! Production passes the real source and tests pass a simulated one driven by a seed, so a failing
//! seed replays exactly. Under the feature `test-support` the crate offers `SeededEntropy`,
//! `FailingEntropy`, `ManualClock` and `MapChainView`; the real clock is the caller's (this
//! crate reads no clock, file or network: `std::time` is not used here), the real randomness is
//! `OsEntropy` under the feature `os-rng`.
//!
//! UNKNOWN IS A STATE: a failed entropy draw is an [`EntropyError`] and the caller stops; an
//! unreadable clock is a [`ClockError`], never the time 0; an unreadable chain is a [`ChainError`],
//! never "not authorized".
//!
//! ```
//! use azlin_keys::{Span, UnixTime};
//!
//! let issued = UnixTime::from_secs(1_800_000_000);
//! let expires = issued.checked_add(Span::days(1)).unwrap();
//! assert_eq!(expires.as_secs(), 1_800_086_400);
//! assert_eq!(UnixTime::from_secs(u64::MAX).checked_add(Span::secs(1)), None); // checked: no wrap
//! ```
//!
//! There is no implicit conversion from a bare number to a time or a span:
//!
//! ```compile_fail
//! use azlin_keys::Span;
//! let _span: Span = 86_400u64.into();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::UnixTime;
//! let _time: UnixTime = 1_800_000_000u64.into();
//! ```

pub use crate::error::{ChainError, ClockError, EntropyError};
use crate::ids::{ChainSeq, DeviceId, Epoch, KeyId, SpaceId};

#[cfg(any(test, feature = "test-support"))]
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(any(test, feature = "test-support"))]
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------
// Time: UnixTime and Span (whole seconds, checked arithmetic, no raw numbers in an API)
// ---------------------------------------------------------------------------------------------

/// A point in time: whole seconds since 1970-01-01 00:00:00 UTC. Arithmetic is checked (`None` on
/// overflow) and there is no `From<u64>`: a time is made with [`UnixTime::from_secs`], which a
/// reviewer can grep.
///
/// Caller: every lane that stamps, expires or compares (S15c2 `FreshHead`, S9 registry stamps,
/// S17 the team log, `VerifyAt::Time`); azlin-seams names it `Stamp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixTime(u64);

impl UnixTime {
    /// 1970-01-01 00:00:00 UTC.
    pub const EPOCH: UnixTime = UnixTime(0);

    /// The time `secs` seconds after the epoch.
    pub const fn from_secs(secs: u64) -> UnixTime {
        UnixTime(secs)
    }

    /// Seconds since the epoch.
    pub const fn as_secs(&self) -> u64 {
        self.0
    }

    /// This time plus `by`; `None` on overflow.
    pub fn checked_add(self, by: Span) -> Option<UnixTime> {
        self.0.checked_add(by.0).map(UnixTime)
    }

    /// This time minus `by`; `None` when that is before the epoch.
    pub fn checked_sub(self, by: Span) -> Option<UnixTime> {
        self.0.checked_sub(by.0).map(UnixTime)
    }

    /// The span from `earlier` to this time; `None` when `earlier` is later than this time.
    pub fn elapsed_since(self, earlier: UnixTime) -> Option<Span> {
        self.0.checked_sub(earlier.0).map(Span)
    }
}

/// A length of time: whole seconds. The constructors for minutes, hours and days take a `u32`, so
/// they cannot overflow; the arithmetic between spans is checked.
///
/// Caller: every lane that bounds a time (S15c2 `FreshHead`'s maximum age, S9 windows, S17
/// holds); azlin-seams re-exports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span(u64);

impl Span {
    /// No time at all.
    pub const ZERO: Span = Span(0);

    /// `secs` seconds.
    pub const fn secs(secs: u64) -> Span {
        Span(secs)
    }

    /// `minutes` minutes (60 seconds each).
    pub const fn minutes(minutes: u32) -> Span {
        Span(minutes as u64 * 60)
    }

    /// `hours` hours (3,600 seconds each).
    pub const fn hours(hours: u32) -> Span {
        Span(hours as u64 * 3_600)
    }

    /// `days` days (86,400 seconds each, no calendar).
    pub const fn days(days: u32) -> Span {
        Span(days as u64 * 86_400)
    }

    /// The span in seconds.
    pub const fn as_secs(&self) -> u64 {
        self.0
    }

    /// This span plus `other`; `None` on overflow.
    pub fn checked_add(self, other: Span) -> Option<Span> {
        self.0.checked_add(other.0).map(Span)
    }

    /// This span minus `other`; `None` when `other` is longer.
    pub fn checked_sub(self, other: Span) -> Option<Span> {
        self.0.checked_sub(other.0).map(Span)
    }

    /// This span `factor` times; `None` on overflow.
    pub fn checked_mul(self, factor: u32) -> Option<Span> {
        self.0.checked_mul(u64::from(factor)).map(Span)
    }
}

// ---------------------------------------------------------------------------------------------
// Randomness
// ---------------------------------------------------------------------------------------------

/// A source of random bytes, a parameter of every generator (planning/architecture.md section 1.4
/// rule 1). A draw either fills the whole buffer or fails; after an `Err` the buffer holds NO
/// usable bytes, and the caller returns the error: no function of this crate (or of a dependent)
/// goes on with zeroes.
///
/// Caller: every generator (ids, keys, nonces, ephemeral keys; S15b-S15e, S14, S9, S16, S17).
pub trait Entropy {
    /// Fills `out` completely with fresh random bytes, or fails.
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError>;
}

/// The operating system's random source (`getrandom`), under the feature `os-rng`. An operating
/// system failure is [`EntropyError::Os`], never zero bytes.
///
/// Caller: the production composition roots (INT-S15-1 in azul-storage, the services); not built
/// for wasm32 without `getrandom`'s `js` feature, so the browser build uses
/// `default-features = false` and brings its own `Entropy`.
#[cfg(feature = "os-rng")]
#[derive(Clone, Copy, Debug, Default)]
pub struct OsEntropy;

#[cfg(feature = "os-rng")]
impl Entropy for OsEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        getrandom::getrandom(out).map_err(|_| EntropyError::Os)
    }
}

/// A deterministic stream of bytes from a seed (the BLAKE3 extendable output of the keyed hash of
/// the seed), for tests that must replay: the same seed gives the same stream however it is cut
/// into draws. It counts the bytes drawn. The stream is pinned by `tests/vectors/seeded_entropy.json`
/// because the known-answer vectors of the other lanes are made with it: it never changes.
///
/// Under the feature `test-support`; never part of a production build.
///
/// Seam: tests of every lane (a dev-dependency feature of azlin-content and the key lanes).
#[cfg(any(test, feature = "test-support"))]
pub struct SeededEntropy {
    stream: blake3::OutputReader,
    drawn: usize,
}

#[cfg(any(test, feature = "test-support"))]
impl SeededEntropy {
    /// The stream of this 32-byte seed.
    pub fn new(seed: [u8; 32]) -> SeededEntropy {
        let mut hasher = blake3::Hasher::new_keyed(&seed);
        hasher.update(b"azlin seeded entropy v1");
        SeededEntropy {
            stream: hasher.finalize_xof(),
            drawn: 0,
        }
    }

    /// The stream of a small seed: the 8 bytes little-endian, the rest zero.
    pub fn from_u64(seed: u64) -> SeededEntropy {
        let mut bytes = [0u8; 32];
        for (dst, src) in bytes.iter_mut().zip(seed.to_le_bytes()) {
            *dst = src;
        }
        SeededEntropy::new(bytes)
    }

    /// How many bytes have been drawn so far.
    pub fn drawn(&self) -> usize {
        self.drawn
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Entropy for SeededEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        let drawn = self
            .drawn
            .checked_add(out.len())
            .ok_or(EntropyError::Exhausted)?;
        self.stream.fill(out);
        self.drawn = drawn;
        Ok(())
    }
}

/// Prints the count, never the stream.
#[cfg(any(test, feature = "test-support"))]
impl core::fmt::Debug for SeededEntropy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SeededEntropy {{ drawn: {} }}", self.drawn)
    }
}

/// An entropy source that serves a fixed number of bytes and then fails, for the tests of "no
/// operation continues with zeroes": the first draw that does not fit fails with
/// [`EntropyError::Exhausted`] and takes nothing, and every later draw fails too.
///
/// Under the feature `test-support`.
///
/// Seam: tests of every generator in every lane.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct FailingEntropy {
    inner: SeededEntropy,
    left: usize,
    failed: bool,
}

#[cfg(any(test, feature = "test-support"))]
impl FailingEntropy {
    /// A source that serves `good_bytes` bytes (from a fixed seed) and then fails;
    /// `FailingEntropy::after(0)` fails at the first draw.
    pub fn after(good_bytes: usize) -> FailingEntropy {
        FailingEntropy {
            inner: SeededEntropy::from_u64(0),
            left: good_bytes,
            failed: false,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Entropy for FailingEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        if self.failed {
            return Err(EntropyError::Exhausted);
        }
        match self.left.checked_sub(out.len()) {
            Some(left) => {
                self.inner.fill(out)?;
                self.left = left;
                Ok(())
            }
            None => {
                self.failed = true;
                Err(EntropyError::Exhausted)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Time source
// ---------------------------------------------------------------------------------------------

/// A source of the current time, a parameter of everything that stamps, expires or compares
/// (planning/architecture.md section 1.4 rule 1). An unreadable clock is an error, never 0.
///
/// Caller: S15c2 (`FreshHead::check`), S9/S17 stamps, every expiry and age check.
pub trait Clock {
    /// The current time, or why it cannot be read.
    fn now(&self) -> Result<UnixTime, ClockError>;
}

/// The seam the production clock plugs into: a [`Clock`] over a function of the caller's own,
/// which reads ITS system clock and maps a failure to a [`ClockError`] (this crate reads no
/// clock: `std::time` is not used here).
///
/// Seam: the composition roots of the apps and services (INT-S15-1); azlin-seams may provide a
/// real clock on top of it. DRIFT X-DELTA-W1S15A-6: the brief names this type and gives it no
/// shape; this is the provisional reading.
#[derive(Clone, Copy, Debug)]
pub struct SystemClockSeam<F> {
    read: F,
}

impl<F> SystemClockSeam<F>
where
    F: Fn() -> Result<UnixTime, ClockError>,
{
    /// A clock that asks `read` for the time at every call.
    pub fn new(read: F) -> SystemClockSeam<F> {
        SystemClockSeam { read }
    }
}

impl<F> Clock for SystemClockSeam<F>
where
    F: Fn() -> Result<UnixTime, ClockError>,
{
    fn now(&self) -> Result<UnixTime, ClockError> {
        (self.read)()
    }
}

/// A clock a test sets by hand: `set`, `advance`, `jump_back` and `fail`. It takes `&self` (the
/// code under test holds it as a `&dyn Clock` while the test moves it), so it is built of atomics.
///
/// Under the feature `test-support`.
///
/// Seam: tests of every lane that reads a clock.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct ManualClock {
    secs: AtomicU64,
    failing: AtomicBool,
}

#[cfg(any(test, feature = "test-support"))]
impl ManualClock {
    /// A clock that reads `at`.
    pub fn new(at: UnixTime) -> ManualClock {
        ManualClock {
            secs: AtomicU64::new(at.0),
            failing: AtomicBool::new(false),
        }
    }

    /// Sets the time.
    pub fn set(&self, at: UnixTime) {
        self.secs.store(at.0, Ordering::SeqCst);
    }

    /// Moves the clock forward by `by` and returns the new time; `None` (and no change) when that
    /// would overflow.
    pub fn advance(&self, by: Span) -> Option<UnixTime> {
        match self
            .secs
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| {
                now.checked_add(by.0)
            }) {
            Ok(before) => before.checked_add(by.0).map(UnixTime),
            Err(_) => None,
        }
    }

    /// Moves the clock back by `by` (a jump of the real clock) and returns the new time; `None`
    /// (and no change) when that would be before the epoch.
    pub fn jump_back(&self, by: Span) -> Option<UnixTime> {
        match self
            .secs
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| {
                now.checked_sub(by.0)
            }) {
            Ok(before) => before.checked_sub(by.0).map(UnixTime),
            Err(_) => None,
        }
    }

    /// While `failing` is true the clock cannot be read: `now` answers
    /// [`ClockError::Unreadable`].
    pub fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Clock for ManualClock {
    fn now(&self) -> Result<UnixTime, ClockError> {
        if self.failing.load(Ordering::SeqCst) {
            Err(ClockError::Unreadable)
        } else {
            Ok(UnixTime(self.secs.load(Ordering::SeqCst)))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The space chain, as the key rules see it (P-S15a-5 / C-14: moved here from S15c2)
// ---------------------------------------------------------------------------------------------

/// What a chain entry is that commits an epoch's key id: the first epoch of a space
/// (`space_create`) or a later one (`space_rekey`) (planning/architecture.md section 5.9).
///
/// Caller: S16b (the chain entries), S15c2 (`authorize_epoch`'s rule 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommitKind {
    /// The `space_create` entry: commits the first epoch.
    Create,
    /// A `space_rekey` entry: commits a later epoch.
    Rekey,
}

/// A signed space chain entry's commitment to the key of one epoch: "an epoch counts only if a
/// signed `space_rekey` (or `space_create`) chain entry from an authorized device commits to its
/// key id" (planning/architecture.md section 5.5).
///
/// Caller: S16b (`SpaceChains` builds them from verified entries), S15c2 (`authorize_epoch`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Commitment {
    /// The space the entry belongs to.
    pub space: SpaceId,
    /// The epoch whose key the entry commits to.
    pub epoch: Epoch,
    /// The key id the entry commits to (`SpaceKey::key_id` of S15c1).
    pub key_id: KeyId,
    /// The device that signed the entry.
    pub signer: DeviceId,
    /// The position of the entry in the chain.
    pub seq: ChainSeq,
    /// Whether the entry creates the space or rekeys it.
    pub kind: CommitKind,
}

/// The verified space chain, as far as the key rules read it. `Ok(None)` ("the chain has no
/// commitment for this epoch") and `Err` ("the chain cannot be read") are different answers: an
/// unreadable chain is Unknown, never "not authorized" and never "no commitment".
///
/// Seam: S16b's `SpaceChains` implements it over the verified `azlin-space-chain-v1` entries;
/// S15c2's `authorize_epoch` and `RekeyPlan::is_stale` call it.
pub trait SpaceChainView {
    /// The commitment of `space`'s chain to the key of `epoch`; `Ok(None)` when the readable chain
    /// holds none.
    fn commitment(&self, space: &SpaceId, epoch: Epoch) -> Result<Option<Commitment>, ChainError>;

    /// Whether `device` is an authorized signer of `space` at chain position `at`.
    fn is_authorized_signer(
        &self,
        space: &SpaceId,
        device: &DeviceId,
        at: ChainSeq,
    ) -> Result<bool, ChainError>;
}

/// The windows `[from, until)` (`until` exclusive, `None`: for ever) in which a device is an
/// authorized signer of a space.
#[cfg(any(test, feature = "test-support"))]
type SignerWindows = Vec<(ChainSeq, Option<ChainSeq>)>;

/// A [`SpaceChainView`] a test fills by hand: commitments, the windows in which a device is an
/// authorized signer, and a switch that makes every answer an error.
///
/// Under the feature `test-support`.
///
/// Seam: tests of S15c2 (`authorize_epoch`, `RekeyPlan::is_stale`) and S16b.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Default)]
pub struct MapChainView {
    commitments: BTreeMap<(SpaceId, Epoch), Commitment>,
    signers: BTreeMap<(SpaceId, DeviceId), SignerWindows>,
    failing: Option<ChainError>,
}

#[cfg(any(test, feature = "test-support"))]
impl MapChainView {
    /// An empty chain view: no commitments, no signers.
    pub fn new() -> MapChainView {
        MapChainView::default()
    }

    /// Adds a commitment (replacing one of the same space and epoch).
    pub fn commit(&mut self, commitment: Commitment) {
        self.commitments
            .insert((commitment.space, commitment.epoch), commitment);
    }

    /// Makes `device` an authorized signer of `space` from position `from` (inclusive) until
    /// position `until` (exclusive; `None`: for ever).
    pub fn authorize(
        &mut self,
        space: SpaceId,
        device: DeviceId,
        from: ChainSeq,
        until: Option<ChainSeq>,
    ) {
        self.signers
            .entry((space, device))
            .or_default()
            .push((from, until));
    }

    /// With `Some(error)` every answer is that error (an unreadable chain); `None` ends it.
    pub fn fail_with(&mut self, error: Option<ChainError>) {
        self.failing = error;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl SpaceChainView for MapChainView {
    fn commitment(&self, space: &SpaceId, epoch: Epoch) -> Result<Option<Commitment>, ChainError> {
        if let Some(error) = self.failing {
            return Err(error);
        }
        Ok(self.commitments.get(&(*space, epoch)).copied())
    }

    fn is_authorized_signer(
        &self,
        space: &SpaceId,
        device: &DeviceId,
        at: ChainSeq,
    ) -> Result<bool, ChainError> {
        if let Some(error) = self.failing {
            return Err(error);
        }
        let windows = match self.signers.get(&(*space, *device)) {
            Some(windows) => windows,
            None => return Ok(false),
        };
        Ok(windows
            .iter()
            .any(|&(from, until)| from <= at && until.is_none_or(|until| at < until)))
    }
}
