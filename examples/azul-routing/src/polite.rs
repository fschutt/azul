//! How AzMaps asks a public service without flooding it: its requests a gap apart, and after a
//! failure (a refusal, a busy server, no answer) a pause that doubles with every failure in a
//! row, up to a cap. While a service pauses, callers answer from their cache, or say the
//! service is busy - they do not ask again. One answer ends the pause.
//!
//! The gate is shared by every worker of a session that asks the same service.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// The longest pause after failures in a row.
pub const MAX_BACKOFF: Duration = Duration::from_secs(120);

/// The pause after the first failure.
pub const FIRST_BACKOFF: Duration = Duration::from_secs(2);

/// What a request may do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Go ahead.
    Go,
    /// The caller gave up while it waited.
    Stopped,
    /// The service pauses after failures for this much longer: answer from the cache.
    BackingOff(Duration),
}

#[derive(Debug, Default)]
struct GateState {
    /// When the last request went out.
    last_sent: Option<Instant>,
    /// Failures in a row.
    failures: u32,
    /// Until when the service pauses.
    paused_until: Option<Instant>,
}

/// The gate of one service.
#[derive(Debug)]
pub struct Polite {
    /// The least time between two requests.
    gap: Duration,
    state: Mutex<GateState>,
}

impl Polite {
    /// A gate that keeps requests `gap` apart.
    #[must_use]
    pub fn new(gap: Duration) -> Self {
        Self {
            gap,
            state: Mutex::new(GateState::default()),
        }
    }

    /// Waits for this request's turn - asking `stop` while it waits - and takes it.
    pub fn turn(&self, stop: &mut dyn FnMut() -> bool) -> Turn {
        loop {
            if stop() {
                return Turn::Stopped;
            }
            let wait = {
                let Ok(mut state) = self.state.lock() else {
                    return Turn::Go;
                };
                let now = Instant::now();
                if let Some(until) = state.paused_until.filter(|until| *until > now) {
                    return Turn::BackingOff(until - now);
                }
                let wait = state
                    .last_sent
                    .map_or(Duration::ZERO, |at| self.gap.saturating_sub(at.elapsed()));
                if wait.is_zero() {
                    state.last_sent = Some(now);
                    return Turn::Go;
                }
                wait
            };
            // Short sleeps, so a caller that gives up is let go soon.
            std::thread::sleep(wait.min(Duration::from_millis(50)));
        }
    }

    /// The service answered: the pause ends.
    pub fn answered(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failures = 0;
            state.paused_until = None;
        }
    }

    /// The service failed: the next pause is twice the last one.
    pub fn failed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failures = state.failures.saturating_add(1);
            state.paused_until = Some(Instant::now() + backoff(state.failures));
        }
    }

    /// How long the service still pauses, if it does.
    #[must_use]
    pub fn paused_for(&self) -> Option<Duration> {
        let state = self.state.lock().ok()?;
        let now = Instant::now();
        state.paused_until.filter(|until| *until > now).map(|until| until - now)
    }
}

/// The pause after `failures` failures in a row: 2 s, 4 s, 8 s, ... up to [`MAX_BACKOFF`].
#[must_use]
pub fn backoff(failures: u32) -> Duration {
    if failures == 0 {
        return Duration::ZERO;
    }
    let doublings = (failures - 1).min(16);
    FIRST_BACKOFF.saturating_mul(1u32 << doublings).min(MAX_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pause_doubles_with_every_failure_up_to_the_cap() {
        assert_eq!(backoff(0), Duration::ZERO);
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(2), Duration::from_secs(4));
        assert_eq!(backoff(3), Duration::from_secs(8));
        assert_eq!(backoff(20), MAX_BACKOFF);
        assert_eq!(backoff(u32::MAX), MAX_BACKOFF);
    }

    #[test]
    fn two_requests_go_a_gap_apart() {
        let gate = Polite::new(Duration::from_millis(150));
        let started = Instant::now();
        assert_eq!(gate.turn(&mut || false), Turn::Go);
        assert_eq!(gate.turn(&mut || false), Turn::Go);
        assert!(started.elapsed() >= Duration::from_millis(150), "{:?}", started.elapsed());
    }

    #[test]
    fn a_caller_that_gives_up_while_it_waits_sends_nothing() {
        let gate = Polite::new(Duration::from_secs(5));
        assert_eq!(gate.turn(&mut || false), Turn::Go);
        let started = Instant::now();
        assert_eq!(gate.turn(&mut || true), Turn::Stopped);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn after_a_failure_the_service_pauses_and_an_answer_ends_the_pause() {
        let gate = Polite::new(Duration::ZERO);
        assert_eq!(gate.paused_for(), None);
        gate.failed();
        let Turn::BackingOff(left) = gate.turn(&mut || false) else {
            panic!("a failed service is not asked again at once");
        };
        assert!(left <= FIRST_BACKOFF && left > Duration::from_millis(1500), "{left:?}");
        assert!(gate.paused_for().is_some());
        gate.failed();
        assert!(gate.paused_for().expect("paused") > FIRST_BACKOFF, "the second pause is longer");
        gate.answered();
        assert_eq!(gate.paused_for(), None);
        assert_eq!(gate.turn(&mut || false), Turn::Go);
    }
}
