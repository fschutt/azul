//! How often the pump looks at the network: every [`BUSY_MS`] while media flows (audio frames
//! arrive every 20 ms, video every 33-66 ms), every [`IDLE_MS`] once nothing has flowed for
//! [`QUIET_AFTER_MS`] - a call with every microphone muted and every camera off then wakes the
//! app four times a second instead of sixty-six. The pump's timer has a fixed interval, so a pace
//! change re-arms it. Pure: no azul types, unit-tested here.

/// The pump's interval while media flows.
pub const BUSY_MS: u64 = 15;
/// The pump's interval in a quiet call (a chat message or a peer's first media waits at most
/// this long).
pub const IDLE_MS: u64 = 250;
/// How long nothing must flow before the pump slows down.
pub const QUIET_AFTER_MS: u64 = 1000;

/// The pump's pace (see the module docs).
#[derive(Debug)]
pub struct PumpPace {
    interval_ms: u64,
    last_busy_ms: Option<u64>,
}

impl Default for PumpPace {
    fn default() -> Self {
        PumpPace {
            interval_ms: BUSY_MS,
            last_busy_ms: None,
        }
    }
}

impl PumpPace {
    pub fn new() -> Self {
        PumpPace::default()
    }

    /// The interval the pump runs at now.
    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    /// A pump ran at `now_ms`; `busy` says whether media flowed or is due (something sent or
    /// received, a microphone or camera on, a decoder or the playout working). The new interval
    /// when the pump's timer must be re-armed, else `None`.
    pub fn after_pump(&mut self, busy: bool, now_ms: u64) -> Option<u64> {
        // The first pump counts as busy: a call starts fast.
        let last_busy = *self.last_busy_ms.get_or_insert(now_ms);
        if busy {
            self.last_busy_ms = Some(now_ms);
        }
        let quiet = !busy && now_ms.saturating_sub(last_busy) >= QUIET_AFTER_MS;
        let wanted = if quiet { IDLE_MS } else { BUSY_MS };
        if wanted == self.interval_ms {
            return None;
        }
        self.interval_ms = wanted;
        Some(wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pump_starts_fast_and_stays_fast_while_media_flows() {
        let mut pace = PumpPace::new();
        assert_eq!(pace.interval_ms(), BUSY_MS);
        for t in (0..5000).step_by(15) {
            assert_eq!(pace.after_pump(true, t), None, "{t} ms");
        }
        assert_eq!(pace.interval_ms(), BUSY_MS);
    }

    #[test]
    fn a_quiet_call_slows_the_pump_once_and_media_speeds_it_up_once() {
        let mut pace = PumpPace::new();
        assert_eq!(pace.after_pump(true, 0), None);
        // Quiet, but not for long yet.
        assert_eq!(pace.after_pump(false, 500), None);
        assert_eq!(pace.after_pump(false, QUIET_AFTER_MS - 1), None);
        assert_eq!(pace.after_pump(false, QUIET_AFTER_MS), Some(IDLE_MS));
        assert_eq!(pace.interval_ms(), IDLE_MS);
        assert_eq!(pace.after_pump(false, QUIET_AFTER_MS + IDLE_MS), None, "re-armed once");
        assert_eq!(pace.after_pump(true, 9000), Some(BUSY_MS));
        assert_eq!(pace.after_pump(true, 9015), None);
    }

    #[test]
    fn a_call_that_starts_quiet_slows_down_after_the_quiet_spell() {
        let mut pace = PumpPace::new();
        assert_eq!(pace.after_pump(false, 0), None);
        assert_eq!(pace.after_pump(false, QUIET_AFTER_MS), Some(IDLE_MS));
    }
}
