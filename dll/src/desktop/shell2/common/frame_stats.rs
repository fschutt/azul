//! Frame times measured inside the engine: how often a window presents while it is being drawn continuously, and
//! how long the draw takes. Pure: the shell gives it the clock.
//!
//! A phone has no outside source for this on the iOS Simulator (and none that tells the CPU renderer's frames from
//! the GPU's anywhere), so the window reports its own: while frames come one after another a summary line goes
//! to the log once a second, and when a burst ends (the UI went idle) the part of it that was not reported yet:
//!
//! `[render] frames n=62 over_ms=1003 interval_ms median=16.7 p95=17.9 max=33.4 draw_ms median=4.1 p95=7.9`
//!
//! The QA harness reads those lines for the median and the 95th percentile over a scripted scroll. An idle window
//! writes nothing: a gap of more than [`IDLE_GAP_US`] ends a burst, it is not an interval.

/// A gap between two presents longer than this is the UI being idle, not a slow frame.
pub const IDLE_GAP_US: u64 = 250_000;
/// A burst is reported once it is this long.
pub const REPORT_AFTER_US: u64 = 1_000_000;
/// A burst of fewer frames than this says nothing worth a line.
pub const MIN_FRAMES: usize = 8;
/// The most samples one summary keeps (a second of frames at 120 Hz is 120).
pub const MAX_SAMPLES: usize = 1_000;

/// The samples of the burst in progress.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameStats {
    /// When the previous frame was presented, in the shell's clock.
    last_at_us: Option<u64>,
    /// When the burst began.
    burst_start_us: u64,
    /// The time between two presents, in microseconds.
    intervals_us: Vec<u64>,
    /// How long each draw took, in microseconds.
    draws_us: Vec<u64>,
}

/// The nearest-rank percentile (`p` in 0..=100) of the sorted values; 0 for none.
fn percentile(sorted: &[u64], p: u64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p * sorted.len() as u64).div_ceil(100).max(1);
    sorted[(rank as usize - 1).min(sorted.len() - 1)]
}

fn sorted(values: &[u64]) -> Vec<u64> {
    let mut v = values.to_vec();
    v.sort_unstable();
    v
}

/// Milliseconds with one decimal.
fn ms(us: u64) -> String {
    format!("{}.{}", us / 1000, (us % 1000) / 100)
}

impl FrameStats {
    /// No frames yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many frames the burst in progress holds.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.intervals_us.len() + usize::from(self.last_at_us.is_some())
    }

    /// A frame was presented at `at_us` (the shell's clock, microseconds); `draw_us` is how long its draw took, when
    /// it is known. Returns the summary line when this frame completes one: the burst has run for a second, or this
    /// frame comes after an idle gap that ended a burst long enough to report.
    pub fn record(&mut self, at_us: u64, draw_us: Option<u64>) -> Option<String> {
        let mut line = None;
        if let Some(last) = self.last_at_us {
            let gap = at_us.saturating_sub(last);
            if gap > IDLE_GAP_US {
                // the UI went idle: the burst that ended is reported, the next one begins with this frame
                line = self.take_summary(last);
                self.begin(at_us);
            } else if self.intervals_us.len() < MAX_SAMPLES {
                self.intervals_us.push(gap);
            }
        } else {
            self.begin(at_us);
        }
        if let Some(d) = draw_us {
            if self.draws_us.len() < MAX_SAMPLES {
                self.draws_us.push(d);
            }
        }
        self.last_at_us = Some(at_us);
        if line.is_none() && at_us.saturating_sub(self.burst_start_us) >= REPORT_AFTER_US {
            line = self.take_summary(at_us);
            // the next window of the same burst starts with this frame
            self.begin(at_us);
            self.last_at_us = Some(at_us);
        }
        line
    }

    fn begin(&mut self, at_us: u64) {
        self.burst_start_us = at_us;
        self.intervals_us.clear();
        self.draws_us.clear();
        self.last_at_us = Some(at_us);
    }

    /// The summary of what was collected, if there are enough frames; the samples are cleared either way.
    fn take_summary(&mut self, end_us: u64) -> Option<String> {
        let enough = self.intervals_us.len() + 1 >= MIN_FRAMES;
        let intervals = sorted(&self.intervals_us);
        let draws = sorted(&self.draws_us);
        let frames = self.intervals_us.len() + 1;
        let over = end_us.saturating_sub(self.burst_start_us);
        self.intervals_us.clear();
        self.draws_us.clear();
        if !enough {
            return None;
        }
        let mut line = format!(
            "[render] frames n={frames} over_ms={} interval_ms median={} p95={} max={}",
            over / 1000,
            ms(percentile(&intervals, 50)),
            ms(percentile(&intervals, 95)),
            ms(intervals.last().copied().unwrap_or(0)),
        );
        if !draws.is_empty() {
            line.push_str(&format!(
                " draw_ms median={} p95={}",
                ms(percentile(&draws, 50)),
                ms(percentile(&draws, 95))
            ));
        }
        Some(line)
    }
}

#[cfg(test)]
#[path = "frame_stats_test.rs"]
mod frame_stats_test;
