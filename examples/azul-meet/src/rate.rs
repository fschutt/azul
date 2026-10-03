//! The sender's rate control, free of azul types so `cargo test -p AzMeet` checks it without a
//! network or an encoder: how many kilobits a second each rendition's H.264 encoder may spend.
//!
//! # Why
//!
//! H.264 packets ride reliable iroh messages (`video_wire.rs`): a link that carries fewer bits
//! than the encoder makes queues them, every picture arrives later, and once a receiver has
//! [`MAX_IN_FLIGHT`] packets unacknowledged the sender pauses it until a keyframe - the picture
//! freezes. An encoder that spends less before that happens keeps the picture moving, at a lower
//! quality, instead (`VideoEncoder::set_bitrate`, applied from the next frame, no restart).
//!
//! # Signal
//!
//! Each receiver acknowledges every `ACK_EVERY` packets (`video_wire::SendWindow`): the packets
//! sent and not acknowledged yet (`in_flight`) are the stream's queue on that link plus one round
//! trip. A link that keeps up holds a handful (an acknowledgement batch, plus the frames of one
//! round trip); a link that does not keep up holds more and more. One encoder serves every
//! receiver of a rendition, so the WORST receiver decides; the most any of them had in flight
//! since the last decision counts (acknowledgements come in batches, so a single look could land
//! right after one).
//!
//! # Rule (additive increase, multiplicative decrease - the shape of WebRTC's loss-based control)
//!
//! Every [`DECIDE_EVERY_MS`], by the most packets any receiver had in flight since the last
//! decision:
//! - at least [`DECREASE_AT`]: the rate drops to [`DECREASE_PERCENT`] of itself (not below the
//!   floor), and does not rise again for [`HOLD_AFTER_DECREASE_MS`] (the encoder's queue and the
//!   link's need that long to drain);
//! - at most [`INCREASE_BELOW`], and no hold: it rises by [`INCREASE_STEP_PERCENT`] (at least
//!   [`MIN_STEP_KBPS`]) up to the rendition's ladder rate - about ten percent a second;
//! - in between: it stays.
//!
//! The ceiling is the rendition's ladder rate (`IrohLoadBalancer::rendition_kbps`), where every
//! stream starts. The floor is [`FLOOR_PERCENT`] of it (at least [`MIN_KBPS`]): below that the
//! picture is mush, and the send window's pause is the better answer.
//!
//! The uplink estimate (`routes::CapacityEstimator`) is NOT a ceiling: it is the larger of what
//! was sent and what a congestion window allows, so a rate lowered for one slow link would lower
//! what is sent, then the estimate, then the ceiling of every stream - a spiral down. Each link's
//! own queue is the signal; a saturated uplink fills every link's queue at once.

use crate::video_wire::MAX_IN_FLIGHT;

/// How often the controller decides (ms).
pub const DECIDE_EVERY_MS: u64 = 500;
/// Packets in flight to one receiver at which the rate drops: half the send window.
pub const DECREASE_AT: usize = MAX_IN_FLIGHT / 2;
/// Packets in flight to every receiver at or under which the rate may rise: a third of the send
/// window (an acknowledgement batch and a round trip of frames fit).
pub const INCREASE_BELOW: usize = MAX_IN_FLIGHT / 3;
/// What a decrease leaves of the rate, in percent.
pub const DECREASE_PERCENT: u64 = 70;
/// How long after a decrease the rate does not rise (ms).
pub const HOLD_AFTER_DECREASE_MS: u64 = 2000;
/// One increase, in percent of the rate.
pub const INCREASE_STEP_PERCENT: u64 = 5;
/// The smallest increase (kbit/s).
pub const MIN_STEP_KBPS: u32 = 10;
/// The floor, in percent of the rendition's ladder rate.
pub const FLOOR_PERCENT: u64 = 25;
/// The lowest floor (kbit/s): what VideoToolbox's rate control takes at least.
pub const MIN_KBPS: u32 = 64;

/// The rate of one rendition's encoder (see the module docs).
#[derive(Debug, Clone)]
pub struct RateControl {
    ladder_kbps: u32,
    rate_kbps: u32,
    /// The most packets any receiver had in flight since the last decision.
    worst_in_flight: usize,
    decided_at_ms: Option<u64>,
    /// No increase before this time (ms): set by a decrease.
    hold_until_ms: u64,
}

impl RateControl {
    /// A stream at `ladder_kbps`, the rendition's rate (where it starts and its ceiling).
    pub fn new(ladder_kbps: u32) -> Self {
        RateControl {
            ladder_kbps,
            rate_kbps: ladder_kbps,
            worst_in_flight: 0,
            decided_at_ms: None,
            hold_until_ms: 0,
        }
    }

    /// What the encoder spends now (kbit/s): what a re-opened encoder opens with.
    pub fn rate_kbps(&self) -> u32 {
        self.rate_kbps
    }

    /// The rendition's ladder rate, the ceiling.
    pub fn ladder_kbps(&self) -> u32 {
        self.ladder_kbps
    }

    /// The lowest rate the controller goes to.
    pub fn floor_kbps(&self) -> u32 {
        let share = u64::from(self.ladder_kbps) * FLOOR_PERCENT / 100;
        u32::try_from(share)
            .unwrap_or(u32::MAX)
            .max(MIN_KBPS)
            .min(self.ladder_kbps)
    }

    /// One receiver of the stream has `in_flight` packets sent and not acknowledged yet.
    pub fn observe(&mut self, in_flight: usize) {
        self.worst_in_flight = self.worst_in_flight.max(in_flight);
    }

    /// Every pump, at `now_ms`: the new rate when the encoder must be told one, else `None`.
    pub fn tick(&mut self, now_ms: u64) -> Option<u32> {
        // RED stub: the rate never moves.
        let _ = now_ms;
        None
    }

    /// "600 of 1000 kbps": for the statistics.
    pub fn label(&self) -> String {
        format!("{} of {} kbps", self.rate_kbps, self.ladder_kbps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pump's interval while media flows (`pace::BUSY_MS`).
    const PUMP_MS: u64 = 15;

    /// Pumps from `from_ms` to before `to_ms`: one receiver with `in_flight` packets each pump,
    /// then the tick. Every rate the encoder was told, with its time.
    fn pump(rc: &mut RateControl, from_ms: u64, to_ms: u64, in_flight: usize) -> Vec<(u64, u32)> {
        let mut told = Vec::new();
        let mut t = from_ms;
        while t < to_ms {
            rc.observe(in_flight);
            if let Some(kbps) = rc.tick(t) {
                told.push((t, kbps));
            }
            t += PUMP_MS;
        }
        told
    }

    #[test]
    fn a_link_that_keeps_up_keeps_the_ladder_rate_and_the_encoder_hears_nothing() {
        let mut rc = RateControl::new(1000);
        let mut told = Vec::new();
        // Acknowledgements in batches: in flight rises to six, falls to one, again and again.
        for (i, t) in (0..10_000).step_by(PUMP_MS as usize).enumerate() {
            rc.observe(1 + i % 6);
            if let Some(kbps) = rc.tick(t) {
                told.push(kbps);
            }
        }
        assert!(told.is_empty(), "told {told:?}");
        assert_eq!(rc.rate_kbps(), 1000);
    }

    #[test]
    fn a_link_that_backs_up_lowers_the_rate_each_decision_while_it_does() {
        let mut rc = RateControl::new(1000);
        let told = pump(&mut rc, 0, 1600, DECREASE_AT + 2);
        let rates: Vec<u32> = told.iter().map(|(_, kbps)| *kbps).collect();
        assert_eq!(rates, vec![700, 490, 343], "told {told:?}");
        assert_eq!(rc.rate_kbps(), 343);
        // A decision every half second, not every pump.
        for pair in told.windows(2) {
            assert!(pair[1].0 - pair[0].0 >= DECIDE_EVERY_MS, "told {told:?}");
        }
    }

    #[test]
    fn the_worst_receiver_decides() {
        let mut rc = RateControl::new(1000);
        let mut told = Vec::new();
        for t in (0..600).step_by(PUMP_MS as usize) {
            rc.observe(2); // a peer that keeps up
            rc.observe(DECREASE_AT); // one that does not
            told.extend(rc.tick(t));
        }
        assert_eq!(told, vec![700]);
    }

    #[test]
    fn a_backlog_that_cleared_between_two_decisions_still_counts() {
        let mut rc = RateControl::new(1000);
        assert_eq!(rc.tick(0), None);
        rc.observe(MAX_IN_FLIGHT);
        rc.observe(0); // the acknowledgements came in right after
        assert_eq!(rc.tick(DECIDE_EVERY_MS), Some(700));
    }

    #[test]
    fn the_rate_never_falls_below_the_floor() {
        let mut rc = RateControl::new(1000);
        let told = pump(&mut rc, 0, 20_000, MAX_IN_FLIGHT);
        assert_eq!(rc.floor_kbps(), 250);
        assert_eq!(rc.rate_kbps(), 250);
        assert_eq!(told.last().map(|(_, kbps)| *kbps), Some(250));
        assert!(told.iter().all(|(_, kbps)| *kbps >= 250), "told {told:?}");
        // Once on the floor, the encoder hears nothing more.
        let more = pump(&mut rc, 20_000, 25_000, MAX_IN_FLIGHT);
        assert!(more.is_empty(), "told {more:?}");
    }

    #[test]
    fn after_a_drop_the_rate_waits_then_climbs_back_to_the_ladder_about_ten_percent_a_second() {
        let mut rc = RateControl::new(1000);
        // Backed up through the decision at 1020 ms (the pumps are 15 ms apart), so nothing
        // observed after it lingers into the next one.
        let down = pump(&mut rc, 0, 1021, DECREASE_AT);
        let (dropped_at, low) = *down.last().expect("a decrease");
        assert_eq!((dropped_at, low), (1020, 490));
        let up = pump(&mut rc, 1035, 30_000, 3);
        assert!(!up.is_empty(), "the rate never rose again");
        assert!(
            up[0].0 >= dropped_at + HOLD_AFTER_DECREASE_MS,
            "rose at {} ms, {} ms after the drop: told {up:?}",
            up[0].0,
            up[0].0 - dropped_at
        );
        let mut before = low;
        for (t, kbps) in &up {
            let step = (u64::from(before) * INCREASE_STEP_PERCENT / 100) as u32;
            assert!(*kbps > before, "{t} ms: {kbps} after {before}");
            assert!(
                *kbps <= before + step.max(MIN_STEP_KBPS),
                "{t} ms: {before} -> {kbps} is more than one step"
            );
            before = *kbps;
        }
        assert_eq!(rc.rate_kbps(), 1000, "back at the ladder rate");
        assert_eq!(
            up.last().map(|(_, kbps)| *kbps),
            Some(1000),
            "never past it"
        );
        // Back from half the ladder rate in more than five seconds, not at once.
        let climb_ms = up.last().map(|(t, _)| *t).unwrap_or(0) - up[0].0;
        assert!(climb_ms >= 5000, "climbed back in {climb_ms} ms");
    }

    #[test]
    fn a_link_neither_backing_up_nor_clear_holds_the_rate() {
        let mut rc = RateControl::new(1000);
        // Backed up through the decision at 510 ms.
        let _ = pump(&mut rc, 0, 511, DECREASE_AT);
        assert_eq!(rc.rate_kbps(), 700);
        // Between the two marks, long after the hold: no change either way.
        let told = pump(&mut rc, 525, 10_000, (INCREASE_BELOW + DECREASE_AT) / 2);
        assert!(told.is_empty(), "told {told:?}");
        assert_eq!(rc.rate_kbps(), 700);
    }

    #[test]
    fn the_statistics_say_the_rate_and_the_ladder_rate() {
        let mut rc = RateControl::new(1000);
        assert_eq!(rc.label(), "1000 of 1000 kbps");
        let _ = pump(&mut rc, 0, 600, MAX_IN_FLIGHT);
        assert_eq!(rc.label(), "700 of 1000 kbps");
    }
}
