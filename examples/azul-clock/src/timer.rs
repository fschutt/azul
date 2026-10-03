//! A countdown timer: idle, running (until an instant), paused (with the
//! time left) or finished.
//!
//! Everything is derived from the wall clock in milliseconds since 1970,
//! never from counting ticks (the plan's TimerModel): a slow frame never
//! loses time, and a running timer survives a restart of the app as the
//! instant it ends - which is also when the OS notification for it is
//! scheduled (`schedule.rs`), so it rings while AzClock is closed.
//!
//! The file of a timer (`clock/timers/<uuid>.json`) is this struct as JSON.

use serde::{Deserialize, Serialize};

use crate::tone::Sound;

/// One minute in milliseconds (+1 min).
pub const MINUTE_MS: i64 = 60_000;
/// The presets, in minutes.
pub const PRESETS_MIN: [u32; 5] = [1, 3, 5, 10, 25];
/// The longest timer: 99 hours.
pub const MAX_MS: i64 = 99 * 60 * MINUTE_MS;

/// Where a timer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum TimerState {
    /// Set, not started: the whole length is left.
    Idle,
    /// Counting down; done at `ends_at` (ms since 1970).
    Running { ends_at: i64 },
    /// Stopped with `remaining_ms` left.
    Paused { remaining_ms: i64 },
    /// Done at `at` (ms since 1970); it rings until it is dismissed.
    Finished { at: i64 },
}

/// One timer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountdownTimer {
    /// The file's name (a UUID).
    pub id: String,
    /// "Tea" (may be empty).
    #[serde(default)]
    pub label: String,
    /// The length it was set to; +1 min adds to it.
    pub duration_ms: i64,
    pub state: TimerState,
    #[serde(default)]
    pub sound: Sound,
}

impl CountdownTimer {
    /// An idle timer of `duration_ms`.
    #[must_use]
    pub fn new(id: &str, label: &str, duration_ms: i64) -> CountdownTimer {
        CountdownTimer {
            id: id.to_string(),
            label: label.to_string(),
            duration_ms: duration_ms.clamp(0, MAX_MS),
            state: TimerState::Idle,
            sound: Sound::Beep,
        }
    }

    /// Start (or resume, or start again when finished) at `now`.
    pub fn start(&mut self, now: i64) {
        let _ = now;
    }

    /// Pause at `now`, keeping the time left (a timer whose time is up
    /// finishes instead).
    pub fn pause(&mut self, now: i64) {
        let _ = now;
    }

    /// Back to the whole length, idle.
    pub fn reset(&mut self) {}

    /// +1 min (or any `ms`): a running timer ends later, a paused one has
    /// more left, an idle one is longer, a finished one starts again with
    /// just that time. The length grows with it (the ring's "of 11:00").
    pub fn add(&mut self, now: i64, ms: i64) {
        let _ = (now, ms);
    }

    /// The time left at `now` (0 when done).
    #[must_use]
    pub fn remaining_ms(&self, now: i64) -> i64 {
        let _ = now;
        0
    }

    /// How much of the ring is left at `now`: 1.0 at the start, 0.0 when done.
    #[must_use]
    pub fn fraction_left(&self, now: i64) -> f32 {
        let _ = now;
        0.0
    }

    /// Call on every tick: `true` when the timer finished now (once).
    pub fn tick(&mut self, now: i64) -> bool {
        let _ = now;
        false
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.state, TimerState::Running { .. })
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        matches!(self.state, TimerState::Finished { .. })
    }

    /// When it ends, while it runs.
    #[must_use]
    pub fn ends_at(&self) -> Option<i64> {
        match self.state {
            TimerState::Running { ends_at } => Some(ends_at),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_790_000_000_000;
    const MIN: i64 = MINUTE_MS;

    fn ten() -> CountdownTimer {
        CountdownTimer::new("t", "Tea", 10 * MIN)
    }

    #[test]
    fn a_started_timer_counts_down_by_the_wall_clock() {
        let mut t = ten();
        assert_eq!(t.remaining_ms(T0), 10 * MIN, "idle: the whole length");
        t.start(T0);
        assert_eq!(t.ends_at(), Some(T0 + 10 * MIN));
        assert_eq!(t.remaining_ms(T0 + 2 * MIN), 8 * MIN);
        assert_eq!(t.remaining_ms(T0 + 10 * MIN + 5_000), 0);
        t.start(T0 + MIN);
        assert_eq!(t.ends_at(), Some(T0 + 10 * MIN), "starting a running timer changes nothing");
    }

    #[test]
    fn pausing_freezes_the_time_left_and_resuming_goes_on_from_it() {
        let mut t = ten();
        t.start(T0);
        t.pause(T0 + 3 * MIN);
        assert_eq!(t.state, TimerState::Paused { remaining_ms: 7 * MIN });
        assert_eq!(t.remaining_ms(T0 + 20 * MIN), 7 * MIN, "paused: time stands still");
        t.start(T0 + 20 * MIN);
        assert_eq!(t.ends_at(), Some(T0 + 27 * MIN));
    }

    #[test]
    fn a_timer_finishes_exactly_once() {
        let mut t = ten();
        t.start(T0);
        assert!(!t.tick(T0 + 9 * MIN));
        assert!(t.tick(T0 + 10 * MIN + 300), "a late tick finishes it");
        assert_eq!(t.state, TimerState::Finished { at: T0 + 10 * MIN }, "at its end, not at the tick");
        assert!(!t.tick(T0 + 11 * MIN), "once");
        assert!(t.is_finished());
        // Pausing a timer whose time is up finishes it.
        let mut late = ten();
        late.start(T0);
        late.pause(T0 + 12 * MIN);
        assert!(late.is_finished());
    }

    #[test]
    fn plus_one_minute_extends_a_running_a_paused_an_idle_and_a_finished_timer() {
        let mut running = ten();
        running.start(T0);
        running.add(T0 + MIN, MIN);
        assert_eq!(running.ends_at(), Some(T0 + 11 * MIN));
        assert_eq!(running.duration_ms, 11 * MIN, "the ring's length grows too");

        let mut paused = ten();
        paused.start(T0);
        paused.pause(T0 + 4 * MIN);
        paused.add(T0 + 5 * MIN, MIN);
        assert_eq!(paused.state, TimerState::Paused { remaining_ms: 7 * MIN });

        let mut idle = ten();
        idle.add(T0, MIN);
        assert_eq!((idle.duration_ms, idle.state), (11 * MIN, TimerState::Idle));

        let mut done = ten();
        done.start(T0);
        assert!(done.tick(T0 + 10 * MIN));
        done.add(T0 + 10 * MIN + 30_000, MIN);
        assert_eq!(done.ends_at(), Some(T0 + 11 * MIN + 30_000), "one more minute from now");
        assert_eq!(done.duration_ms, MIN);
    }

    #[test]
    fn reset_brings_back_the_whole_length() {
        let mut t = ten();
        t.start(T0);
        t.pause(T0 + 2 * MIN);
        t.reset();
        assert_eq!(t.state, TimerState::Idle);
        assert_eq!(t.remaining_ms(T0 + 30 * MIN), 10 * MIN);
    }

    #[test]
    fn the_ring_drains_with_the_time_left() {
        let mut t = ten();
        assert_eq!(t.fraction_left(T0), 1.0);
        t.start(T0);
        assert!((t.fraction_left(T0 + 5 * MIN) - 0.5).abs() < 1e-6);
        assert!((t.fraction_left(T0 + 9 * MIN) - 0.1).abs() < 1e-6);
        t.tick(T0 + 10 * MIN);
        assert_eq!(t.fraction_left(T0 + 10 * MIN), 0.0);
        assert_eq!(CountdownTimer::new("z", "", 0).fraction_left(T0), 0.0, "no length: empty");
    }

    #[test]
    fn a_slow_frame_never_loses_time() {
        let mut t = ten();
        t.start(T0);
        // Ticks at uneven moments: the time left is always the end minus now.
        for now in [T0 + 16, T0 + 1_500, T0 + 61_234, T0 + 300_001] {
            assert!(!t.tick(now));
            assert_eq!(t.remaining_ms(now), T0 + 10 * MIN - now);
        }
    }

    #[test]
    fn a_running_timer_survives_a_restart_and_one_that_ended_meanwhile_finishes_at_its_end() {
        let mut t = ten();
        t.start(T0);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"state\":\"running\""), "{json}");
        let mut back: CountdownTimer = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        // AzClock was closed for an hour.
        assert!(back.tick(T0 + 60 * MIN));
        assert_eq!(back.state, TimerState::Finished { at: T0 + 10 * MIN });
        let idle: CountdownTimer =
            serde_json::from_str(r#"{"id":"x","duration_ms":60000,"state":{"state":"idle"}}"#).unwrap();
        assert_eq!(idle.state, TimerState::Idle);
    }
}
