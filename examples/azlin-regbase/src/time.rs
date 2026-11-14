//! The registry's two kinds of time (planning/architecture.md section 2.4 "Time is the leader's
//! stamp, checked" and section 5.3 "**Times are day starts** (D37) except where a column says
//! otherwise, and the R3 schema gate checks it"; build unit S9, lane W1-S9a).
//!
//! - [`LeaderTime`]: the time the leader writes into an entry. It is `azlin_seams::Stamp` (which is
//!   `azlin_keys::UnixTime`: whole seconds since 1970, checked arithmetic, no negative time); the
//!   NAME is kept in every signature so that a typed clock later is a change of this file alone.
//!   Inside an apply it is `ApplyCtx::now`; there is no other clock.
//! - [`DayStart`]: a [`LeaderTime`] truncated to the start of its day (86,400-second days, no
//!   calendar). It is the ONLY type of a registry time column that is a day start, and it has no
//!   constructor from a raw integer: [`DayStart::of`] truncates a time and
//!   [`DayStart::from_start`] refuses a time that is not a day start.
//! - [`LeaderClock`]: the leader's stamp as a `azlin_keys::Clock`, for code that takes a clock
//!   parameter and runs inside an apply.
//!
//! ```compile_fail
//! // a day start cannot be made from a raw number
//! let _ = azlin_regbase::DayStart::from(86_400u64);
//! ```
//!
//! ```compile_fail
//! // and its field is private
//! let t = azlin_regbase::LeaderTime::from_secs(5);
//! let _ = azlin_regbase::DayStart(t);
//! ```

use azlin_keys::{Clock, ClockError, Span, UnixTime};
use azlin_seams::Stamp;

/// The time the leader stamped into the entry being applied (planning/architecture.md section 2.4):
/// monotone in the log, and the only time an apply may read.
///
/// Caller: `ApplyCtx::now`, every exact time column (`reg_device.hold_until`, a key's window,
/// a credential's `issued_at` and `expires_at`).
pub type LeaderTime = Stamp;

/// The length of a registry day in seconds (no calendar, no leap seconds).
const DAY_LEN: u64 = Span::days(1).as_secs();

/// A leader stamp as a clock: it always reads the time it was made with. An apply has no clock
/// of its own (planning/architecture.md section 1.4 rule 1); a function that asks for an
/// `azlin_keys::Clock` inside an apply is given this one, so it reads the entry's stamp and
/// nothing else.
///
/// Seam: S9g (`check_connection` runs azlin-creds' verifiers at the leader's stamp) and S9i (the
/// dispatcher builds one per entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaderClock {
    stamp: LeaderTime,
}

impl LeaderClock {
    /// The clock that reads `stamp`.
    pub const fn at(stamp: LeaderTime) -> LeaderClock {
        LeaderClock { stamp }
    }

    /// The stamp this clock reads.
    pub const fn stamp(&self) -> LeaderTime {
        self.stamp
    }
}

impl Clock for LeaderClock {
    fn now(&self) -> Result<UnixTime, ClockError> {
        Ok(self.stamp)
    }
}

/// The start of a day: a [`LeaderTime`] that is a multiple of 86,400 seconds. "Times are day
/// starts (D37)": the plan keeps a customer's activity at the granularity of a day, so every time
/// column of the registry is a `DayStart` except the ones the plan says are exact (`hold_until`).
///
/// Ordered by time; no `Default`; the field is private.
///
/// Implements planning/architecture.md section 5.3 (D37) and section 5.15 R3 (the schema gate
/// refuses an exact time where a day start is required).
///
/// Caller: every day-start column of azlin-registry's rows; S9b (`last_used`, expiry), S9c, S9d
/// (period, ban, notices).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DayStart(LeaderTime);

impl DayStart {
    /// The start of the day `t` is in: `t` truncated to a multiple of 86,400 seconds, so
    /// `of(t).start() <= t < of(t).start() + 1 day`.
    pub fn of(t: LeaderTime) -> DayStart {
        let secs = t.as_secs();
        DayStart(UnixTime::from_secs(secs - secs % DAY_LEN))
    }

    /// `t` as a day start when it is one (a multiple of 86,400 seconds), else `None`. For the
    /// code that reads a stored column back: a stored value that is not a day start is
    /// corruption, never rounded.
    pub fn from_start(t: LeaderTime) -> Option<DayStart> {
        if t.as_secs().is_multiple_of(DAY_LEN) {
            Some(DayStart(t))
        } else {
            None
        }
    }

    /// The first second of the day.
    pub const fn start(self) -> LeaderTime {
        self.0
    }

    /// This day start `d` later. `d` must be a whole number of days: a span that is not (and an
    /// overflow) is `None`, never rounded.
    pub fn plus_days(self, d: Span) -> Option<DayStart> {
        if !d.as_secs().is_multiple_of(DAY_LEN) {
            return None;
        }
        self.0.checked_add(d).map(DayStart)
    }

    /// This day start `d` earlier. `d` must be a whole number of days; a span that is not, or one
    /// that reaches before 1970, is `None`.
    pub fn minus_days(self, d: Span) -> Option<DayStart> {
        if !d.as_secs().is_multiple_of(DAY_LEN) {
            return None;
        }
        self.0.checked_sub(d).map(DayStart)
    }

    /// The span from `earlier` to this day (a whole number of days); `None` when `earlier` is
    /// later.
    pub fn days_since(self, earlier: DayStart) -> Option<Span> {
        self.0.elapsed_since(earlier.0)
    }

    /// The next day's start; `None` at the largest representable day.
    pub fn next(self) -> Option<DayStart> {
        self.plus_days(Span::days(1))
    }
}
