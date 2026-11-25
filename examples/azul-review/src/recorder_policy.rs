//! When the recorder pauses itself, and what it says about the disk: the rules, as pure functions
//! of the clock and the numbers they are given.
//!
//! A recording that runs for hours every day for weeks has to look after itself. It pauses when
//! the disk would fall under the free space it was told to keep (and goes on when there is room
//! again), when nothing has happened for a while (and goes on at the next input), and it stops at
//! the longest session it was allowed. It tells the user how much disk is left and for how long, so
//! the panel can show "7.2 GB free, about 11 h left at this rate".

use crate::recorder_settings::{RecorderSettings, GB};

/// How the disk stands against the free space the recorder keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    /// The disk could not be asked (an unknown volume): nothing is paused for it, the panel says so.
    Unknown,
    /// Plenty.
    Fine,
    /// Under half as much again as the recorder keeps free: the user is warned.
    Low,
    /// Under what the recorder keeps free: it pauses.
    Full,
}

/// The extra room a paused recording needs before it goes on again, bytes: not to pause and resume
/// on every few megabytes a cleaning frees.
pub const RESUME_MARGIN: u64 = GB;

/// How the disk stands: `free` bytes against `min_free`.
#[must_use]
pub const fn disk_state(free: Option<u64>, min_free: u64) -> DiskState {
    match free {
        None => DiskState::Unknown,
        Some(f) if f < min_free => DiskState::Full,
        Some(f) if f < min_free + min_free / 2 + GB => DiskState::Low,
        Some(_) => DiskState::Fine,
    }
}

/// Whether a recording paused for the disk may go on: there is room again with a margin.
#[must_use]
pub const fn may_resume_after_quota(free: Option<u64>, min_free: u64) -> bool {
    match free {
        Some(f) => f >= min_free + RESUME_MARGIN,
        None => false,
    }
}

/// How many hours the recorder can go on at `bytes_per_hour` before the disk is down to `min_free`:
/// `None` when it writes nothing (no rate to go by) or the disk is already under.
#[must_use]
pub fn hours_left(free: Option<u64>, min_free: u64, bytes_per_hour: u64) -> Option<f64> {
    let room = free?.checked_sub(min_free)?;
    if bytes_per_hour == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    Some(room as f64 / bytes_per_hour as f64)
}

/// What a recording of these settings is expected to write an hour, bytes, before it has written
/// enough to measure: the camera and the voice at their targets all the time, the screen at a
/// quarter of its target (it is recorded when it changes).
#[must_use]
pub fn expected_bytes_per_hour(settings: &RecorderSettings, screen: (u32, u32)) -> u64 {
    let per_hour = |kbps: u32| u64::from(kbps) * 1_000 / 8 * 3_600;
    let mut total = 0;
    if settings.screen {
        total += per_hour(settings.quality.screen_kbps(screen.0, screen.1)) / 4;
    }
    if settings.camera {
        total += per_hour(settings.quality.camera().kbps);
    }
    if settings.microphone {
        total += per_hour(crate::recorder_settings::AUDIO_KBPS);
    }
    total
}

/// What the recorder has written an hour, from what it wrote in the last minutes.
#[derive(Debug, Clone)]
pub struct RateMeter {
    window_us: u64,
    samples: Vec<(u64, u64)>,
}

/// The least time a measurement must span for its rate to be believed, microseconds.
pub const MIN_SPAN_US: u64 = 30_000_000;

impl RateMeter {
    /// A meter that looks at the last `window_us` microseconds.
    #[must_use]
    pub fn new(window_us: u64) -> Self {
        Self {
            window_us,
            samples: Vec::new(),
        }
    }

    /// Notes that `total_bytes` had been written at `now_us`.
    pub fn record(&mut self, now_us: u64, total_bytes: u64) {
        self.samples.push((now_us, total_bytes));
        let from = now_us.saturating_sub(self.window_us);
        let keep_from = self.samples.iter().rposition(|(t, _)| *t <= from).unwrap_or(0);
        self.samples.drain(..keep_from);
    }

    /// Bytes an hour at the rate of the window; `None` until the window spans [`MIN_SPAN_US`].
    #[must_use]
    pub fn bytes_per_hour(&self) -> Option<u64> {
        let (t0, b0) = *self.samples.first()?;
        let (t1, b1) = *self.samples.last()?;
        let span = t1.saturating_sub(t0);
        if span < MIN_SPAN_US {
            return None;
        }
        let bytes = u128::from(b1.saturating_sub(b0));
        u64::try_from(bytes * 3_600_000_000 / u128::from(span)).ok()
    }
}

/// What the idle watch decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleChange {
    Nothing,
    /// Nothing happened for the idle time: pause.
    Paused { idle_us: u64 },
    /// Something happened while paused for idleness: go on.
    Resumed,
}

/// Watches for a stretch without activity.
#[derive(Debug, Clone, Copy)]
pub struct IdleWatch {
    threshold_us: Option<u64>,
    last_activity_us: u64,
    paused: bool,
}

impl IdleWatch {
    /// A watch that pauses after `threshold_us` without activity (`None`: never), beginning at `now_us`.
    #[must_use]
    pub const fn new(threshold_us: Option<u64>, now_us: u64) -> Self {
        Self {
            threshold_us,
            last_activity_us: now_us,
            paused: false,
        }
    }

    /// Whether the watch has paused the recording.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Something happened at `now_us`: input, speech, a change of the screen.
    pub fn activity(&mut self, now_us: u64) -> IdleChange {
        self.last_activity_us = self.last_activity_us.max(now_us);
        if self.paused {
            self.paused = false;
            IdleChange::Resumed
        } else {
            IdleChange::Nothing
        }
    }

    /// Looks at the clock.
    pub fn check(&mut self, now_us: u64) -> IdleChange {
        let Some(threshold) = self.threshold_us else {
            return IdleChange::Nothing;
        };
        let idle = now_us.saturating_sub(self.last_activity_us);
        if !self.paused && idle >= threshold {
            self.paused = true;
            IdleChange::Paused { idle_us: idle }
        } else {
            IdleChange::Nothing
        }
    }

    /// A pause the user chose, or a resume: the watch starts counting again from `now_us` so a
    /// recording resumed by hand is not paused again at once.
    pub fn reset(&mut self, now_us: u64) {
        self.last_activity_us = now_us;
        self.paused = false;
    }
}

/// How a session stands against the longest it may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthState {
    Fine,
    /// Ten minutes or less are left.
    Ending { left_us: u64 },
    /// The session has reached its length: stop.
    Over,
}

/// How long a session has been recording against the most it may.
#[must_use]
pub const fn length_state(recorded_us: u64, max_us: u64) -> LengthState {
    if recorded_us >= max_us {
        LengthState::Over
    } else if max_us - recorded_us <= 600_000_000 {
        LengthState::Ending { left_us: max_us - recorded_us }
    } else {
        LengthState::Fine
    }
}

#[cfg(test)]
#[path = "recorder_policy_test.rs"]
mod recorder_policy_test;
