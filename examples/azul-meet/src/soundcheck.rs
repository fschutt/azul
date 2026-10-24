//! The sound check before a meeting, in the waiting room and in Settings > Audio & Video.
//!
//! "Test speakers" plays a short chime through the speaker. "Record a test clip" records a few
//! seconds of the microphone and plays them straight back, so a person hears what the others
//! will hear. This file holds the rules only, free of azul types: the app feeds it the
//! microphone's chunks and its clock, and plays the packets it hands out on the playout thread.

use crate::audio;

/// How long the test clip records.
pub const CLIP_MS: u64 = 3000;
/// How long the speaker test's chime sounds.
pub const CHIME_MS: u64 = 1200;
/// Silent packets after what plays. The sink closes when its queue runs dry, and without this
/// tail the last packet would still sit in the device's buffer when it does.
pub const TAIL_PACKETS: usize = 10;
/// A clip whose loudest sample stays under this is silence: a muted or unplugged microphone, or
/// one the system does not let AzMeet use (it then hands out zeros).
const SILENCE_PEAK: f32 = 0.003;
/// The chime's two notes, one after the other.
const CHIME_NOTES_HZ: [f64; 2] = [660.0, 880.0];
/// Each note fades in and out over this long, so it starts and ends without a click.
const FADE_MS: u64 = 20;

/// What the sound check does now. Times are on the app's clock, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// The microphone is recorded since `since`.
    Recording { since: u64 },
    /// Something plays until `until`: the clip just recorded (`clip`), or the chime.
    Playing { until: u64, clip: bool },
}

/// How the last check ended: the line under the buttons says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No check ran yet, or one runs now.
    None,
    PlayedClip,
    PlayedChime,
    /// The clip held only silence, so nothing was played back.
    Quiet,
    /// Not one chunk came from the microphone while recording.
    NoMicrophone,
}

/// What [`SoundCheck::tick`] asks of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    Nothing,
    /// Play these 20 ms packets of mono 16-bit PCM at this rate, one after the other.
    Play(u32, Vec<Vec<i16>>),
    /// The check is over: the window shows the buttons and the outcome again.
    Ended,
}

/// The sound check's state, one per app (`MeetState`).
#[derive(Debug, Clone)]
pub struct SoundCheck {
    phase: Phase,
    /// The clip recorded so far, mono at `rate`; at most [`CLIP_MS`] of it.
    clip: Vec<f32>,
    /// The rate of the clip's first chunk; chunks at another rate are skipped (there is no
    /// resampler, and mixing rates would play the clip at the wrong pitch).
    rate: Option<u32>,
    outcome: Outcome,
}

impl Default for SoundCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundCheck {
    pub fn new() -> Self {
        SoundCheck {
            phase: Phase::Idle,
            clip: Vec::new(),
            rate: None,
            outcome: Outcome::None,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The microphone is being recorded: the app keeps it open even while it is muted.
    pub fn records(&self) -> bool {
        matches!(self.phase, Phase::Recording { .. })
    }

    /// Whole seconds the recording still runs at `now`, rounded up (the button counts down).
    pub fn seconds_left(&self, now: u64) -> u64 {
        match self.phase {
            Phase::Recording { since } => {
                let left = (since + CLIP_MS).saturating_sub(now);
                left.div_ceil(1000)
            }
            _ => 0,
        }
    }

    /// Starts recording the test clip at `now`, in place of any check that runs.
    pub fn start_recording(&mut self, now: u64) {
        self.clip.clear();
        self.rate = None;
        self.outcome = Outcome::None;
        self.phase = Phase::Recording { since: now };
    }

    /// Starts the speaker test at `now`, in place of any check that runs: the chime's packets
    /// at `rate`, to be played one after the other.
    pub fn start_chime(&mut self, rate: u32, now: u64) -> Vec<Vec<i16>> {
        self.clip.clear();
        self.rate = None;
        self.outcome = Outcome::None;
        let packets = packets(rate, &chime(rate));
        self.phase = Phase::Playing {
            until: now + playing_ms(&packets),
            clip: false,
        };
        packets
    }

    /// Stops whatever runs; the outcome line stays empty.
    pub fn cancel(&mut self) {
        self.clip.clear();
        self.rate = None;
        self.outcome = Outcome::None;
        self.phase = Phase::Idle;
    }

    /// A chunk from the microphone (`channels` interleaved): kept while recording, mixed to
    /// mono, up to [`CLIP_MS`] of it.
    pub fn feed(&mut self, rate: u32, channels: u16, samples: &[f32]) {
        if !self.records() || rate == 0 {
            return;
        }
        if *self.rate.get_or_insert(rate) != rate {
            return;
        }
        let room = usize::try_from(CLIP_MS * u64::from(rate) / 1000)
            .unwrap_or(usize::MAX)
            .saturating_sub(self.clip.len());
        let mono = audio::mix_to_mono(channels, samples);
        self.clip.extend(mono.into_iter().take(room));
    }

    /// The clock moved to `now`: a finished recording turns into its playback (or ends, when
    /// there is nothing to play), a finished playback ends.
    pub fn tick(&mut self, now: u64) -> Tick {
        match self.phase {
            Phase::Idle => Tick::Nothing,
            Phase::Recording { since } if now.saturating_sub(since) < CLIP_MS => Tick::Nothing,
            Phase::Recording { .. } => {
                let clip = std::mem::take(&mut self.clip);
                let rate = self.rate.take();
                let peak = clip.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
                match rate {
                    Some(rate) if !clip.is_empty() && peak >= SILENCE_PEAK => {
                        let packets = packets(rate, &clip);
                        self.phase = Phase::Playing {
                            until: now + playing_ms(&packets),
                            clip: true,
                        };
                        Tick::Play(rate, packets)
                    }
                    _ => {
                        self.outcome = if clip.is_empty() {
                            Outcome::NoMicrophone
                        } else {
                            Outcome::Quiet
                        };
                        self.phase = Phase::Idle;
                        Tick::Ended
                    }
                }
            }
            Phase::Playing { until, .. } if now < until => Tick::Nothing,
            Phase::Playing { clip, .. } => {
                self.outcome = if clip {
                    Outcome::PlayedClip
                } else {
                    Outcome::PlayedChime
                };
                self.phase = Phase::Idle;
                Tick::Ended
            }
        }
    }
}

/// How long `packets` take to play.
fn playing_ms(packets: &[Vec<i16>]) -> u64 {
    packets.len() as u64 * u64::from(audio::PACKET_MS)
}

/// `samples` (mono) cut into 20 ms packets of 16-bit PCM, the last one filled up with silence,
/// then [`TAIL_PACKETS`] of silence.
fn packets(rate: u32, samples: &[f32]) -> Vec<Vec<i16>> {
    let size = audio::samples_per_packet(rate);
    let mut out: Vec<Vec<i16>> = samples
        .chunks(size)
        .map(|chunk| {
            let mut packet: Vec<i16> = chunk.iter().map(|s| audio::to_pcm16(*s)).collect();
            packet.resize(size, 0);
            packet
        })
        .collect();
    out.extend(std::iter::repeat(vec![0; size]).take(TAIL_PACKETS));
    out
}

/// The speaker test's sound at `rate`: two notes, each half of [`CHIME_MS`], each fading in and
/// out, at the test tone's level.
fn chime(rate: u32) -> Vec<f32> {
    let rate_f = f64::from(rate);
    let total = CHIME_MS * u64::from(rate) / 1000;
    let note_len = (total / CHIME_NOTES_HZ.len() as u64).max(1);
    let fade = (FADE_MS * u64::from(rate) / 1000).max(1);
    (0..total)
        .map(|i| {
            let note = ((i / note_len) as usize).min(CHIME_NOTES_HZ.len() - 1);
            let at = i % note_len;
            let edge = at.min(note_len - 1 - at);
            let gain = (edge as f64 / fade as f64).min(1.0);
            let phase = at as f64 * CHIME_NOTES_HZ[note] / rate_f;
            ((phase * std::f64::consts::TAU).sin() * gain) as f32 * audio::TONE_AMPLITUDE
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio;

    const RATE: u32 = 48_000;

    /// Feeds `ms` of a constant `value` in 20 ms chunks, as the microphone widget sends them.
    fn feed_ms(check: &mut SoundCheck, channels: u16, value: f32, ms: u64) {
        let chunk = audio::samples_per_packet(RATE) * usize::from(channels);
        for _ in 0..ms / 20 {
            check.feed(RATE, channels, &vec![value; chunk]);
        }
    }

    #[test]
    fn a_test_clip_records_three_seconds_and_then_plays_them_back() {
        let mut check = SoundCheck::new();
        check.start_recording(1_000);
        assert!(check.records());
        assert_eq!(check.seconds_left(1_000), 3);
        feed_ms(&mut check, 1, 0.1, 3_500);
        assert_eq!(check.tick(3_999), Tick::Nothing, "the clip is not over yet");
        let Tick::Play(rate, packets) = check.tick(4_000) else {
            panic!("the clip plays once it is recorded");
        };
        assert_eq!(rate, RATE);
        let clip_packets = (CLIP_MS / u64::from(audio::PACKET_MS)) as usize;
        assert_eq!(packets.len(), clip_packets + TAIL_PACKETS, "three seconds, then silence");
        assert!(packets[0].iter().all(|s| *s == audio::to_pcm16(0.1)));
        assert!(packets[clip_packets].iter().all(|s| *s == 0));
        let Phase::Playing { until, clip: true } = check.phase() else {
            panic!("the clip plays");
        };
        assert_eq!(check.tick(until - 1), Tick::Nothing);
        assert_eq!(check.tick(until), Tick::Ended);
        assert_eq!(check.phase(), Phase::Idle);
        assert_eq!(check.outcome(), Outcome::PlayedClip);
    }

    #[test]
    fn a_test_clip_of_only_silence_says_the_microphone_heard_nothing() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        feed_ms(&mut check, 1, 0.0, 3_000);
        assert_eq!(check.tick(CLIP_MS), Tick::Ended, "nothing worth playing back");
        assert_eq!(check.outcome(), Outcome::Quiet);
        assert_eq!(check.phase(), Phase::Idle);
    }

    #[test]
    fn a_recording_without_a_microphone_ends_by_itself() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        assert_eq!(check.tick(CLIP_MS), Tick::Ended);
        assert_eq!(check.outcome(), Outcome::NoMicrophone);
        assert!(!check.records());
    }

    #[test]
    fn the_speaker_test_plays_a_short_chime_and_ends_by_itself() {
        let mut check = SoundCheck::new();
        let packets = check.start_chime(RATE, 100);
        let chime_packets = (CHIME_MS / u64::from(audio::PACKET_MS)) as usize;
        assert_eq!(packets.len(), chime_packets + TAIL_PACKETS);
        let loudest = packets.iter().flatten().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        assert!(loudest > 0, "the chime is heard");
        assert!(
            loudest <= audio::to_pcm16(audio::TONE_AMPLITUDE).unsigned_abs(),
            "and no louder than the test tone"
        );
        assert_eq!(packets[0][0], 0, "it fades in");
        let Phase::Playing { until, clip: false } = check.phase() else {
            panic!("the chime plays");
        };
        assert_eq!(until, 100 + packets.len() as u64 * u64::from(audio::PACKET_MS));
        assert_eq!(check.tick(until - 1), Tick::Nothing);
        assert_eq!(check.tick(until), Tick::Ended);
        assert_eq!(check.outcome(), Outcome::PlayedChime);
    }

    #[test]
    fn a_new_check_replaces_the_one_running() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        feed_ms(&mut check, 1, 0.1, 1_000);
        let _ = check.start_chime(RATE, 1_000);
        assert!(!check.records(), "the chime stopped the recording");
        check.start_recording(1_100);
        assert!(check.records());
        assert_eq!(check.tick(1_100 + CLIP_MS), Tick::Ended, "the old clip is gone");
        assert_eq!(check.outcome(), Outcome::NoMicrophone);
        check.start_recording(9_000);
        check.cancel();
        assert_eq!(check.phase(), Phase::Idle);
    }

    #[test]
    fn a_stereo_microphone_is_mixed_to_mono_and_a_chunk_at_another_rate_is_skipped() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        let chunk = audio::samples_per_packet(RATE);
        let stereo: Vec<f32> = (0..chunk).flat_map(|_| [0.2, 0.0]).collect();
        for _ in 0..CLIP_MS / 20 {
            check.feed(RATE, 2, &stereo);
            check.feed(44_100, 1, &[0.9; 882]);
        }
        let Tick::Play(rate, packets) = check.tick(CLIP_MS) else {
            panic!("the clip plays");
        };
        assert_eq!(rate, RATE);
        assert!(packets[0].iter().all(|s| *s == audio::to_pcm16(0.1)));
    }
}
