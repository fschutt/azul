//! Rate limiting with "newest wins": the pen state leaves at most 30 times a second and a gesture update at most 60
//! times a second, and a value that waits for its turn is REPLACED by a newer one (never queued behind it): a slow
//! link, or a busy second, drops the values in between and always ends on the latest.

use crate::link::Millis;

/// Holds the newest value and lets it out at most every `1 / hz` seconds.
#[derive(Debug, Clone)]
pub struct Pacer<T> {
    min_interval_ms: u64,
    last_sent: Option<Millis>,
    pending: Option<T>,
    dropped: u64,
}

impl<T> Pacer<T> {
    /// A pacer that lets a value out at most `hz` times a second (`hz` of 0 is 1).
    #[must_use]
    pub fn new(hz: u64) -> Self {
        let hz = hz.max(1);
        Pacer { min_interval_ms: 1000_u64.div_ceil(hz), last_sent: None, pending: None, dropped: 0 }
    }

    /// The least time between two values, in milliseconds.
    #[must_use]
    pub fn min_interval_ms(&self) -> u64 {
        self.min_interval_ms
    }

    /// Offers a value: it waits for its turn and replaces one that is waiting.
    pub fn offer(&mut self, value: T) {
        if self.pending.replace(value).is_some() {
            self.dropped += 1;
        }
    }

    /// The waiting value, if its turn has come.
    pub fn take(&mut self, now: Millis) -> Option<T> {
        self.pending.as_ref()?;
        let due = self.last_sent.map_or(0, |last| last.saturating_add(self.min_interval_ms));
        if now >= due {
            self.last_sent = Some(now);
            self.pending.take()
        } else {
            None
        }
    }

    /// The waiting value now, whatever its turn (before a frame that must not be overtaken, like the end of a
    /// gesture): it counts as sent at `now`.
    pub fn flush(&mut self, now: Millis) -> Option<T> {
        let value = self.pending.take()?;
        self.last_sent = Some(now);
        Some(value)
    }

    /// When the waiting value may go (`None`: nothing waits).
    #[must_use]
    pub fn next_due(&self, now: Millis) -> Option<Millis> {
        self.pending.as_ref()?;
        let due = self.last_sent.map_or(0, |last| last.saturating_add(self.min_interval_ms));
        Some(due.max(now))
    }

    /// Whether a value waits.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Forgets the waiting value.
    pub fn clear(&mut self) {
        self.pending = None;
    }

    /// How many values a newer one replaced before they went.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
#[path = "pace_test.rs"]
mod pace_test;
