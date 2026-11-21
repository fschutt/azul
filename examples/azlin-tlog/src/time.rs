//! The two time types of the log: a point in time and a length of time (planning/architecture.md section 1.4
//! rule 1: a clock is a parameter, never read here).
//!
//! This crate defines its own two newtypes on purpose: a verifier of the transparency log (a node, the
//! updater of an app, the browser page) must not depend on the keys crate (drift row X-DELTA-W1S45C-8). Every
//! time the crate needs (the time of a cosignature, "now" of a check) is passed in as a [`UnixTime`]; no call
//! reads the system clock, so every result is a function of its arguments.

/// A point in time: whole seconds since 1970-01-01T00:00:00Z (UTC), as a cosignature carries it (C2SP
/// `tlog-cosignature`: "a time represented as a POSIX timestamp").
///
/// Caller: `policy::Policy::check` (the future-time rule), `cosign::Cosigner::cosign` (the signed time),
/// every verifier of a bundle (the `now` of `bundle::verify_bundle`). Seam: the waiting period of S48 reads
/// [`Logged::logged_since`](crate::Logged::logged_since).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UnixTime(u64);

impl UnixTime {
    /// The time `secs` seconds after the epoch.
    pub const fn from_secs(secs: u64) -> UnixTime {
        UnixTime(secs)
    }

    /// The seconds since the epoch.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// This time plus `span`; `None` when the sum does not fit (a time is never wrapped or clamped).
    /// Seam: first caller is S48's waiting period (`logged_since.plus(72 hours) <= now`, 24 hours for a
    /// security fix).
    pub fn plus(self, span: Span) -> Option<UnixTime> {
        self.0.checked_add(span.0).map(UnixTime)
    }

    /// The time from `earlier` to this one; `None` when `earlier` is after this time.
    pub fn since(self, earlier: UnixTime) -> Option<Span> {
        self.0.checked_sub(earlier.0).map(Span)
    }
}

/// A length of time in whole seconds.
///
/// Caller: `policy::Policy` (the `max_future` slack for a witness whose clock runs ahead), the waiting period
/// of S48 (the 72 and 24 hours of architecture 2.20 point 3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Span(u64);

impl Span {
    /// A span of `secs` seconds.
    pub const fn from_secs(secs: u64) -> Span {
        Span(secs)
    }

    /// The seconds in the span.
    pub const fn get(self) -> u64 {
        self.0
    }
}
