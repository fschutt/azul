//! The session clock: ONE time base for every track of a recording.
//!
//! A recording has several tracks (the screen, the camera, the microphone) and a list of events
//! (the pen, the navigation, the conversations). To cut them together later they all say "when" in
//! the same unit on the same origin: microseconds since the recording began, read from one
//! monotonic clock ([`Clock::now_us`]). The wall-clock time of that origin is kept once, in the
//! manifest ([`SessionClock::origin_unix_ms`]), so a moment of the recording can be told in the
//! real time it happened in (a git commit time, a file's date).
//!
//! The clock is a trait so a test (and the headless e2e) moves it by hand ([`MockClock`]); the
//! recorder never reads a system clock anywhere else. Plain `std`, no azul.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

/// What tells the time of a recording: microseconds since its origin.
///
/// Every implementation is monotonic (never goes back) and can be read from any thread: the
/// events are stamped where they happen, which is not always the thread that writes them.
pub trait Clock: Send + Sync {
    /// Microseconds since the origin of the session.
    fn now_us(&self) -> u64;
}

/// The real clock: monotonic, from an [`Instant`] taken when the session starts.
#[derive(Debug, Clone)]
pub struct SessionClock {
    origin: Instant,
    origin_unix_ms: u64,
}

impl SessionClock {
    /// A clock whose origin is now. `origin_unix_ms` is what the wall clock says at this moment
    /// (milliseconds since 1970 UTC): the caller reads it, so this module reads no system clock
    /// but the monotonic one.
    #[must_use]
    pub fn start(origin_unix_ms: u64) -> Self {
        Self {
            origin: Instant::now(),
            origin_unix_ms,
        }
    }

    /// The wall-clock time of the origin, milliseconds since 1970 UTC.
    #[must_use]
    pub fn origin_unix_ms(&self) -> u64 {
        self.origin_unix_ms
    }
}

impl Clock for SessionClock {
    fn now_us(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// A clock a test moves: it stands still until told otherwise. Clones share the one time, so the
/// test keeps a handle while the recorder holds another.
#[derive(Debug, Clone, Default)]
pub struct MockClock {
    us: Arc<AtomicU64>,
}

impl MockClock {
    /// A mock clock at `us` microseconds.
    #[must_use]
    pub fn at(us: u64) -> Self {
        Self {
            us: Arc::new(AtomicU64::new(us)),
        }
    }

    /// Moves the time to `us`. A time before the current one is ignored: the clock is monotonic.
    pub fn set_us(&self, us: u64) {
        self.us.fetch_max(us, Ordering::SeqCst);
    }

    /// Moves the time forward by `ms` milliseconds.
    pub fn advance_ms(&self, ms: u64) {
        self.us.fetch_add(ms.saturating_mul(1_000), Ordering::SeqCst);
    }

    /// Moves the time forward by `us` microseconds.
    pub fn advance_us(&self, us: u64) {
        self.us.fetch_add(us, Ordering::SeqCst);
    }
}

impl Clock for MockClock {
    fn now_us(&self) -> u64 {
        self.us.load(Ordering::SeqCst)
    }
}

/// A calendar moment in UTC, as the folder of a session and the manifest name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl CivilTime {
    /// The UTC calendar time of `unix_ms` (milliseconds since 1970-01-01T00:00:00Z).
    #[must_use]
    pub fn from_unix_ms(unix_ms: u64) -> Self {
        let secs = unix_ms / 1_000;
        let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX / 2);
        let rem = u32::try_from(secs % 86_400).unwrap_or(0);
        // Howard Hinnant's days-to-civil: the proleptic Gregorian calendar, no table.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
        let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year,
            month,
            day,
            hour: rem / 3_600,
            minute: rem % 3_600 / 60,
            second: rem % 60,
        }
    }

    /// `2026-11-26T10:00:00Z`: the manifest's way to say it.
    #[must_use]
    pub fn iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `2026-11-26_10-00-00`: a name that sorts by time and is a valid folder name everywhere
    /// (no colon). UTC, so two sessions of one machine never collide across a change of the local
    /// time zone; the manifest says the local offset.
    #[must_use]
    pub fn folder_name(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// `1:02:03` or `2:03` for `us` microseconds: the recorder's elapsed time as the indicator shows it.
#[must_use]
pub fn clock_text(us: u64) -> String {
    let total = us / 1_000_000;
    let (h, m, s) = (total / 3_600, total % 3_600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
#[path = "recorder_clock_test.rs"]
mod recorder_clock_test;
