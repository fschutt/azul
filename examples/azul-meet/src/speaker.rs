//! Who speaks: the active speaker of the call, from the levels of the audio each peer sends.
//!
//! Every received 20 ms packet's level ([`level_db`]) goes into [`ActiveSpeaker::observe`]. A
//! packet louder than [`SPEAKING_DB`] is speech; packets less than [`GAP_MS`] apart are one run of
//! speech. A peer that has spoken for [`SWITCH_MS`] in one run takes the stage, unless the one on
//! it is still speaking; the stage stays with the last speaker while everyone is quiet, so it
//! never flickers to nobody, and a cough never takes it. Pure: no azul types, unit-tested here.

use std::collections::BTreeMap;

/// A packet louder than this (dBFS, RMS) is speech. Room noise and a quiet microphone sit well
/// below; a voice at a normal distance well above.
pub const SPEAKING_DB: f32 = -42.0;
/// Loud packets less than this far apart are one run of speech (and the speaking indicator
/// stays on this long after the last one).
pub const GAP_MS: u64 = 400;
/// How long a run of speech must last before its speaker takes the stage.
pub const SWITCH_MS: u64 = 600;
/// The level of silence (and of an empty packet), in dBFS.
pub const SILENCE_DB: f32 = -100.0;

/// The RMS level of 16-bit PCM `samples`, in dBFS ([`SILENCE_DB`] for silence or no samples).
pub fn level_db(samples: &[i16]) -> f32 {
    let _ = samples;
    SILENCE_DB
}

/// One peer's current run of speech: when it began and when it was last heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    started_ms: u64,
    last_ms: u64,
}

/// The call's active speaker (see the module docs).
#[derive(Debug, Default)]
pub struct ActiveSpeaker {
    runs: BTreeMap<u64, Run>,
    current: Option<u64>,
}

impl ActiveSpeaker {
    pub fn new() -> Self {
        ActiveSpeaker::default()
    }

    /// A packet of `peer`'s audio at `level` dBFS arrived at `now_ms`. True when the active
    /// speaker changed.
    pub fn observe(&mut self, peer: u64, level: f32, now_ms: u64) -> bool {
        let _ = (peer, level, now_ms);
        false
    }

    /// The active speaker: the peer on the stage.
    pub fn current(&self) -> Option<u64> {
        self.current
    }

    /// Whether `peer` is speaking right now (the speaking indicator).
    pub fn is_speaking(&self, peer: u64, now_ms: u64) -> bool {
        let _ = (peer, now_ms);
        false
    }

    /// `peer` left the call.
    pub fn forget(&mut self, peer: u64) {
        let _ = peer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: u64 = 1;
    const BEN: u64 = 2;
    const LOUD: f32 = -20.0;
    const QUIET: f32 = -70.0;

    /// `peer` speaks (or is quiet, `level`) from `from_ms` to `to_ms`, a packet every 20 ms.
    /// Returns whether the active speaker changed at any packet.
    fn talk(s: &mut ActiveSpeaker, peer: u64, level: f32, from_ms: u64, to_ms: u64) -> bool {
        let mut changed = false;
        let mut t = from_ms;
        while t <= to_ms {
            changed |= s.observe(peer, level, t);
            t += 20;
        }
        changed
    }

    #[test]
    fn the_level_of_silence_and_of_a_loud_signal() {
        assert_eq!(level_db(&[]), SILENCE_DB);
        assert_eq!(level_db(&[0; 960]), SILENCE_DB);
        let full: Vec<i16> = (0..960).map(|i| if i % 2 == 0 { i16::MAX } else { -i16::MAX }).collect();
        assert!(level_db(&full) > -0.5, "{}", level_db(&full));
        let tenth: Vec<i16> = full.iter().map(|s| s / 10).collect();
        assert!((level_db(&tenth) + 20.0).abs() < 0.5, "{}", level_db(&tenth));
    }

    #[test]
    fn a_peer_who_speaks_long_enough_takes_the_empty_stage_and_a_cough_does_not() {
        let mut s = ActiveSpeaker::new();
        assert!(!talk(&mut s, ADA, LOUD, 0, 300), "a 300 ms cough");
        assert_eq!(s.current(), None);
        assert!(s.is_speaking(ADA, 300));
        assert!(!s.is_speaking(ADA, 300 + GAP_MS + 1));
        assert!(talk(&mut s, BEN, LOUD, 2000, 2000 + SWITCH_MS));
        assert_eq!(s.current(), Some(BEN));
    }

    #[test]
    fn the_stage_stays_with_its_speaker_while_they_speak() {
        let mut s = ActiveSpeaker::new();
        talk(&mut s, ADA, LOUD, 0, 1000);
        assert_eq!(s.current(), Some(ADA));
        // Ben talks over Ada for a while: Ada keeps the stage while she speaks.
        for t in (1020..3000).step_by(20) {
            s.observe(ADA, LOUD, t);
            s.observe(BEN, LOUD, t);
        }
        assert_eq!(s.current(), Some(ADA));
        // Ada stops; Ben goes on and takes the stage.
        talk(&mut s, ADA, QUIET, 3000, 4000);
        talk(&mut s, BEN, LOUD, 3000, 4000);
        assert_eq!(s.current(), Some(BEN));
    }

    #[test]
    fn the_stage_stays_with_the_last_speaker_while_everyone_is_quiet() {
        let mut s = ActiveSpeaker::new();
        talk(&mut s, ADA, LOUD, 0, 1000);
        talk(&mut s, ADA, QUIET, 1000, 10_000);
        talk(&mut s, BEN, QUIET, 1000, 10_000);
        assert_eq!(s.current(), Some(ADA));
    }

    #[test]
    fn a_speaker_who_leaves_leaves_the_stage() {
        let mut s = ActiveSpeaker::new();
        talk(&mut s, ADA, LOUD, 0, 1000);
        s.forget(ADA);
        assert_eq!(s.current(), None);
        assert!(!s.is_speaking(ADA, 1000));
    }
}
