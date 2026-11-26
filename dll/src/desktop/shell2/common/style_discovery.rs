//! What the start of an app asks of the system and of subprocesses, as pure decisions with their tests: whether a
//! command-line fallback may answer an accessibility flag, and the line `App::create` logs for each of its steps
//! (`AZ_LOG=debug`), so that the next number of the start is read and not guessed.

use azul_css::dynamic_selector::BoolCondition;

/// Whether the command line may answer an accessibility flag (reduced motion, increased contrast): only where the native
/// query did not run (`native_answered` false) and left the flag at its default. A native `False` is an answer: asking
/// `defaults read` again costs a process (7 ms on a Mac, on the window's thread) for the answer the system already gave.
#[must_use]
pub fn cli_fallback_wanted(native_answered: bool, native: BoolCondition) -> bool {
    !native_answered && native == BoolCondition::False
}

/// The line of a step of `App::create`.
#[must_use]
pub fn step_line(step: &str, micros: u64) -> String {
    format!("[app] App::create: {step} {micros} us")
}

/// The clock of `App::create`'s steps: each is measured from the one before it.
#[derive(Debug, Clone, Copy)]
pub struct CreateSteps {
    start_us: u64,
    last_us: u64,
}

impl CreateSteps {
    /// The steps begin at `now_us` (microseconds of any clock that does not go back).
    #[must_use]
    pub const fn at(now_us: u64) -> Self {
        Self { start_us: now_us, last_us: now_us }
    }

    /// The step `step` ended at `now_us`: its line (the time since the step before; a clock that did not move or went
    /// back is no time).
    pub fn lap_at(&mut self, step: &str, now_us: u64) -> String {
        let line = step_line(step, now_us.saturating_sub(self.last_us));
        self.last_us = self.last_us.max(now_us);
        line
    }

    /// The whole time from the start to `now_us`.
    #[must_use]
    pub const fn total_at(&self, now_us: u64) -> u64 {
        now_us.saturating_sub(self.start_us)
    }
}

#[cfg(test)]
#[path = "style_discovery_test.rs"]
mod style_discovery_test;
