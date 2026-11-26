//! How steadily the window's timer runs after a recording starts: the first tick's distance from the start, the
//! longest gap between two ticks and how many there were. A window whose thread is busy for a while (a device that is
//! opened on it, a layout, a system call) shows as a long gap, so the demo says these numbers at the end of a run and
//! a run of the real devices can be judged by them. Plain Rust: the times are given, in microseconds of any clock that
//! does not go back.

/// The timer's steadiness since a recording began.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickStats {
    begun_us: Option<u64>,
    last_us: u64,
    /// The ticks since the start.
    pub count: u32,
    /// The first tick's distance from the start, microseconds (0 before it).
    pub first_us: u64,
    /// The longest gap between two ticks, the start counting as the tick before the first, microseconds.
    pub max_gap_us: u64,
}

impl TickStats {
    /// A recording begins now: what was counted before is gone.
    pub fn begin(&mut self, now_us: u64) {
        *self = Self { begun_us: Some(now_us), last_us: now_us, ..Self::default() };
    }

    /// The timer ran now. A tick before any start is not counted.
    pub fn tick(&mut self, now_us: u64) {
        let Some(begun) = self.begun_us else { return };
        let gap = now_us.saturating_sub(self.last_us);
        if self.count == 0 {
            self.first_us = now_us.saturating_sub(begun);
        }
        self.count = self.count.saturating_add(1);
        self.max_gap_us = self.max_gap_us.max(gap);
        self.last_us = now_us.max(self.last_us);
    }

    /// `count=69 first_ms=830 max_gap_ms=410`: what a script reads.
    #[must_use]
    pub fn line(&self) -> String {
        format!("count={} first_ms={} max_gap_ms={}", self.count, self.first_us / 1_000, self.max_gap_us / 1_000)
    }
}

#[cfg(test)]
#[path = "recorder_ticks_test.rs"]
mod recorder_ticks_test;
