//! The clock of a shell that re-reads the user's accessibility preferences on its own schedule.
//!
//! A platform that notifies (a registry broadcast, a workspace notification) is read when it says
//! so; one that does not (the Android settings no native app hears, iOS's `UIAccessibility` flags
//! when no observer class is registered, the macOS workspace flags) is read every so often while
//! the window is in front. The rule is the pure [`PreferencePoll`]; this holds the shell's start
//! time, so the shell asks `due()` without keeping a clock of its own.

use std::time::{Duration, Instant};

use azul_css::accessibility::PreferencePoll;

/// When to re-read the preferences next.
#[derive(Debug, Clone, Copy)]
pub struct PreferenceWatch {
    started: Instant,
    poll: PreferencePoll,
}

/// `duration` in whole milliseconds, saturating.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl PreferenceWatch {
    /// A watch that is due now and then every `interval`.
    #[must_use]
    pub fn every(interval: Duration) -> Self {
        Self {
            started: Instant::now(),
            poll: PreferencePoll::every(millis(interval)),
        }
    }

    /// Whether the preferences are to be read now.
    pub fn due(&mut self) -> bool {
        self.due_at(Instant::now())
    }

    /// Whether the preferences are to be read at `now`.
    pub fn due_at(&mut self, now: Instant) -> bool {
        self.poll
            .due(millis(now.saturating_duration_since(self.started)))
    }

    /// The next [`Self::due`] answers `true`: a notification or a resume said they moved.
    pub fn ask_again_now(&mut self) {
        self.poll.ask_again_now();
    }
}

#[cfg(test)]
#[path = "preference_watch_test.rs"]
mod preference_watch_test;
