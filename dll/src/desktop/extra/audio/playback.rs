//! The arithmetic of playing decoded audio - pure, every target, tested without a device.
//!
//! A decoder hands out packets of whatever size its codec makes (1152 frames of MP3, 1024 of AAC,
//! 4096 of FLAC, ...), at the FILE's rate and channel count. An output wants steady chunks at ITS
//! rate and channel count, never more queued than a short lead (so a pause or a seek is heard at
//! once), and a player wants to know which track the listener HEARS and where in it - which is
//! not the track it is decoding: with gapless playback the next track is decoded while the end of
//! the last one is still queued in the device.
//!
//! - [`Rechunker`]: packets of any size in, chunks of a fixed frame count out.
//! - [`LinearResampler`]: one rate to another, streaming (state carried across packets).
//! - [`remix`]: one channel count to another (mono doubled, stereo averaged to mono).
//! - [`apply_gain`]: the volume, ramped over a chunk so a change does not click.
//! - [`chunk_peaks`] and [`LevelHistory`]: the level of what is heard now (a meter's input).
//! - [`TrackClock`]: which track and which media time the listener hears, from the output's
//!   frame count (gapless boundaries and seeks).
//! - [`RealTimeClock`]: how many frames a device without a clock of its own (the headless
//!   synthetic sink) has played by now - the wall clock, paused and restarted.

use std::collections::VecDeque;

/// Packets of interleaved samples of any size in, chunks of exactly `frames` sample frames out
/// (the last one shorter, from [`Rechunker::flush`]).
#[derive(Debug, Clone)]
pub(crate) struct Rechunker {
    channels: usize,
    frames: usize,
    pending: Vec<f32>,
}

impl Rechunker {
    /// Chunks of `frames` frames of `channels` interleaved samples (both at least 1).
    pub(crate) fn new(channels: u16, frames: usize) -> Self {
        let _ = (channels, frames);
        Self {
            channels: 1,
            frames: 1,
            pending: Vec::new(),
        }
    }

    /// Appends a packet (interleaved; a trailing partial frame is dropped).
    pub(crate) fn push(&mut self, samples: &[f32]) {
        let _ = samples;
    }

    /// The next whole chunk, if one is complete.
    pub(crate) fn pop(&mut self) -> Option<Vec<f32>> {
        None
    }

    /// Whatever is left (a partial chunk), at the end of the stream.
    pub(crate) fn flush(&mut self) -> Option<Vec<f32>> {
        None
    }

    /// Drops what is pending (a seek).
    pub(crate) fn clear(&mut self) {}

    /// Frames waiting for the next chunk.
    pub(crate) fn pending_frames(&self) -> usize {
        0
    }
}

/// Converts interleaved audio from one sample rate to another by linear interpolation, carrying
/// the last frame and the fractional position across calls, so a stream cut into packets comes
/// out the same as in one piece. Good for speech and for a rate the device refuses; a band-limited
/// resampler (rubato) is the quality step for music.
#[derive(Debug, Clone)]
pub(crate) struct LinearResampler {
    from: u32,
    to: u32,
    channels: usize,
    /// Where the next output frame lies, in input frames from the start of the next packet
    /// (in `-1.0..0.0`: between the previous packet's last frame and the next packet's first).
    t: f64,
    /// The previous packet's last frame (empty before the first packet).
    prev: Vec<f32>,
}

impl LinearResampler {
    /// A resampler from `from` Hz to `to` Hz for `channels` interleaved channels.
    pub(crate) fn new(from: u32, to: u32, channels: u16) -> Self {
        let _ = (from, to, channels);
        Self {
            from: 1,
            to: 1,
            channels: 1,
            t: 0.0,
            prev: Vec::new(),
        }
    }

    /// The rates are the same: [`process`](Self::process) hands its input back.
    pub(crate) fn is_identity(&self) -> bool {
        false
    }

    /// The output rate.
    pub(crate) fn output_rate(&self) -> u32 {
        self.to
    }

    /// The next packet (interleaved, at the input rate) at the output rate.
    pub(crate) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let _ = input;
        Vec::new()
    }

    /// Forgets the carried frame (a seek: the next packet does not continue the last one).
    pub(crate) fn reset(&mut self) {}
}

/// `samples` (interleaved, `from` channels) as `to` channels: one channel goes to every output
/// channel, many go to one as their average, otherwise the first `to` channels are kept (missing
/// ones are silent).
pub(crate) fn remix(samples: &[f32], from: u16, to: u16) -> Vec<f32> {
    let _ = (samples, from, to);
    Vec::new()
}

/// Scales `samples` (interleaved, `channels`) by a gain that moves linearly from `from` (the
/// first frame) to `to` (the last), so a volume change does not click. Clamps to `-1.0..=1.0`.
pub(crate) fn apply_gain(samples: &mut [f32], channels: u16, from: f32, to: f32) {
    let _ = (samples, channels, from, to);
}

/// The peak magnitude (`0.0..=1.0`) of the first two channels of `samples` (interleaved,
/// `channels`); mono reports its one channel twice.
pub(crate) fn chunk_peaks(samples: &[f32], channels: u16) -> (f32, f32) {
    let _ = (samples, channels);
    (0.0, 0.0)
}

/// The peaks of the chunks handed to an output, by the output frame each chunk starts at, so a
/// meter shows the level of what is HEARD (the output plays a lead behind the writes).
#[derive(Debug, Clone, Default)]
pub(crate) struct LevelHistory {
    chunks: VecDeque<(u64, f32, f32)>,
}

/// Chunks a [`LevelHistory`] keeps at most (about three seconds of 2048-frame chunks).
const LEVEL_HISTORY_MAX: usize = 64;

impl LevelHistory {
    /// The chunk starting at output frame `start` peaked at `peaks`.
    pub(crate) fn push(&mut self, start: u64, peaks: (f32, f32)) {
        let _ = (start, peaks);
    }

    /// The peaks of the chunk playing at output frame `played` (silence before the first and
    /// once `played` is past `written`, the frames ever written); older chunks are dropped.
    pub(crate) fn at(&mut self, played: u64, written: u64) -> (f32, f32) {
        let _ = (played, written);
        (0.0, 0.0)
    }

    /// Forgets every chunk (a seek drops what was queued).
    pub(crate) fn clear(&mut self) {}
}

/// One stretch of output: from output frame `start` on, track `track` plays from media time
/// `media_s` at `rate` frames a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub start: u64,
    pub media_s: f64,
    pub track: u64,
    pub rate: u32,
}

/// Which track the listener hears and where in it, from the output's frame count: a new
/// [`Segment`] begins at every track change (gapless: in the middle of what is queued) and at
/// every seek.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrackClock {
    segments: VecDeque<Segment>,
}

impl TrackClock {
    /// From output frame `start` on, `track` plays from `media_s` at `rate`.
    pub(crate) fn begin(&mut self, start: u64, track: u64, media_s: f64, rate: u32) {
        let _ = (start, track, media_s, rate);
    }

    /// Forgets every segment (a load or a seek starts over).
    pub(crate) fn clear(&mut self) {}

    /// The track heard at output frame `played` and the media time in it (`None` before any
    /// segment). Segments wholly before the one playing are dropped.
    pub(crate) fn at(&mut self, played: u64) -> Option<(u64, f64)> {
        let _ = played;
        None
    }

    /// The segment that starts last (the track being decoded).
    pub(crate) fn last(&self) -> Option<Segment> {
        None
    }
}

/// The frames a device without its own clock has played by now: the wall clock since it last
/// started, never more than it was given. Times are seconds on any monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RealTimeClock {
    rate: u32,
    /// Frames played when the clock last (re)started.
    base_frames: u64,
    /// When it last (re)started; `None` while stopped (paused, or nothing given yet).
    base_s: Option<f64>,
    paused: bool,
}

impl RealTimeClock {
    /// A stopped clock at `rate` frames a second.
    pub(crate) fn new(rate: u32) -> Self {
        let _ = rate;
        Self {
            rate: 1,
            base_frames: 0,
            base_s: None,
            paused: false,
        }
    }

    /// Frames played by `now_s`, of the `taken` frames given so far.
    pub(crate) fn played(&self, taken: u64, now_s: f64) -> u64 {
        let _ = (taken, now_s);
        0
    }

    /// More frames arrive at `now_s` (`taken_before` were given until now): a clock that ran
    /// dry (played everything) or never ran starts again from here.
    pub(crate) fn on_take(&mut self, taken_before: u64, now_s: f64) {
        let _ = (taken_before, now_s);
    }

    /// Holds the position at `now_s`.
    pub(crate) fn pause(&mut self, taken: u64, now_s: f64) {
        let _ = (taken, now_s);
    }

    /// Runs on from `now_s`.
    pub(crate) fn resume(&mut self, now_s: f64) {
        let _ = now_s;
    }

    /// What was queued is gone: `taken` (what remains given) is all played.
    pub(crate) fn clear(&mut self, taken: u64) {
        let _ = taken;
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;

    fn sine(freq: f64, rate: u32, frames: usize, channels: u16) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * channels as usize);
        for i in 0..frames {
            let v = (2.0 * core::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() as f32;
            for _ in 0..channels {
                out.push(v * 0.5);
            }
        }
        out
    }

    #[test]
    fn packets_of_any_size_come_out_as_chunks_of_the_asked_size_and_nothing_is_lost() {
        let mut chunker = Rechunker::new(2, 1024);
        let mut fed = Vec::new();
        let mut out = Vec::new();
        // MP3-sized packets, then a few odd ones.
        let sizes = [1152usize, 1152, 576, 4096, 17, 1, 2000];
        let mut n = 0.0f32;
        for frames in sizes {
            let packet: Vec<f32> = (0..frames * 2)
                .map(|_| {
                    n += 1.0;
                    n
                })
                .collect();
            fed.extend_from_slice(&packet);
            chunker.push(&packet);
            while let Some(chunk) = chunker.pop() {
                assert_eq!(
                    chunk.len(),
                    1024 * 2,
                    "a whole chunk is 1024 frames of 2 channels"
                );
                out.extend(chunk);
            }
        }
        let total: usize = sizes.iter().sum();
        assert_eq!(chunker.pending_frames(), total % 1024);
        let rest = chunker.flush().expect("the tail comes out at the end");
        assert_eq!(rest.len(), (total % 1024) * 2);
        out.extend(rest);
        assert_eq!(out, fed, "every sample, in order");
        assert!(chunker.flush().is_none(), "nothing twice");
    }

    #[test]
    fn a_cleared_chunker_forgets_what_was_pending() {
        let mut chunker = Rechunker::new(1, 100);
        chunker.push(&[0.5; 70]);
        assert_eq!(chunker.pending_frames(), 70);
        chunker.clear();
        assert_eq!(chunker.pending_frames(), 0);
        assert!(chunker.flush().is_none());
    }

    #[test]
    fn a_resampler_between_equal_rates_hands_its_input_back() {
        let mut r = LinearResampler::new(48_000, 48_000, 2);
        assert!(r.is_identity());
        let input = sine(440.0, 48_000, 480, 2);
        assert_eq!(r.process(&input), input);
    }

    #[test]
    fn a_second_of_44_1_khz_becomes_a_second_of_48_khz_and_the_tone_stays_the_same() {
        let mut r = LinearResampler::new(44_100, 48_000, 1);
        assert!(!r.is_identity());
        assert_eq!(r.output_rate(), 48_000);
        let input = sine(441.0, 44_100, 44_100, 1);
        // In packets of 1152 frames, like an MP3.
        let mut out = Vec::new();
        for packet in input.chunks(1152) {
            out.extend(r.process(packet));
        }
        assert!(
            (47_995..=48_000).contains(&out.len()),
            "a second in is a second out: {} frames",
            out.len()
        );
        // The same 441 Hz tone, sampled at 48 kHz (linear interpolation is off by < 1e-3 here).
        for (k, v) in out.iter().enumerate() {
            let want = 0.5 * (2.0 * core::f64::consts::PI * 441.0 * k as f64 / 48_000.0).sin();
            assert!(
                (f64::from(*v) - want).abs() < 2e-3,
                "frame {k}: {v} vs {want}"
            );
        }
    }

    #[test]
    fn a_stream_resampled_in_packets_equals_the_stream_resampled_whole() {
        let input = sine(1000.0, 48_000, 4800, 2);
        let mut whole = LinearResampler::new(48_000, 44_100, 2);
        let a = whole.process(&input);
        let mut parts = LinearResampler::new(48_000, 44_100, 2);
        let mut b = Vec::new();
        for packet in input.chunks(2 * 333) {
            b.extend(parts.process(packet));
        }
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-5);
        }
    }

    #[test]
    fn mono_is_doubled_to_stereo_and_stereo_is_averaged_to_mono() {
        assert_eq!(remix(&[0.1, 0.2], 1, 2), vec![0.1, 0.1, 0.2, 0.2]);
        assert_eq!(remix(&[0.2, 0.4, -1.0, 1.0], 2, 1), vec![0.3, 0.0]);
        assert_eq!(remix(&[0.5, 0.25], 2, 2), vec![0.5, 0.25]);
        // 5.1 to stereo keeps the front pair.
        assert_eq!(remix(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 6, 2), vec![0.1, 0.2]);
        // A partial frame at the end is dropped.
        assert_eq!(remix(&[0.1, 0.2, 0.3], 2, 2), vec![0.1, 0.2]);
    }

    #[test]
    fn a_volume_change_ramps_over_the_chunk_instead_of_jumping() {
        let mut s = vec![1.0f32; 2 * 5];
        apply_gain(&mut s, 2, 1.0, 0.0);
        // Frame by frame from 1.0 to 0.0, both channels alike.
        let frames: Vec<f32> = s.chunks(2).map(|f| f[0]).collect();
        assert_eq!(frames, vec![1.0, 0.75, 0.5, 0.25, 0.0]);
        assert!(s.chunks(2).all(|f| f[0] == f[1]));
        let mut loud = vec![0.8f32; 4];
        apply_gain(&mut loud, 1, 2.0, 2.0);
        assert_eq!(loud, vec![1.0; 4], "clamped to full scale");
    }

    #[test]
    fn the_peaks_are_per_channel_and_mono_reports_its_channel_twice() {
        assert_eq!(chunk_peaks(&[0.1, -0.9, 0.5, 0.2], 2), (0.5, 0.9));
        assert_eq!(chunk_peaks(&[0.1, -0.7, 0.3], 1), (0.7, 0.7));
        assert_eq!(chunk_peaks(&[], 2), (0.0, 0.0));
        assert_eq!(
            chunk_peaks(&[2.0, 0.0], 2),
            (1.0, 0.0),
            "clamped to full scale"
        );
    }

    #[test]
    fn the_level_shown_is_the_level_of_the_chunk_being_heard_not_the_one_being_written() {
        let mut h = LevelHistory::default();
        h.push(0, (0.1, 0.1));
        h.push(1000, (0.5, 0.4));
        h.push(2000, (0.9, 0.8));
        // Written up to 3000; the output plays frame 1500: the second chunk.
        assert_eq!(h.at(1500, 3000), (0.5, 0.4));
        assert_eq!(h.at(2999, 3000), (0.9, 0.8));
        // Past what was written: silence.
        assert_eq!(h.at(3000, 3000), (0.0, 0.0));
        h.clear();
        assert_eq!(h.at(2500, 3000), (0.0, 0.0));
    }

    #[test]
    fn the_clock_follows_the_heard_track_across_a_gapless_boundary() {
        let mut clock = TrackClock::default();
        assert_eq!(clock.at(0), None);
        // Track 1 from 0 s; track 2 begins at output frame 44_100 (one second later).
        clock.begin(0, 1, 0.0, 44_100);
        clock.begin(44_100, 2, 0.0, 44_100);
        let (track, t) = clock.at(22_050).unwrap();
        assert_eq!(track, 1);
        assert!((t - 0.5).abs() < 1e-9);
        let (track, t) = clock.at(44_100 + 11_025).unwrap();
        assert_eq!(track, 2);
        assert!((t - 0.25).abs() < 1e-9);
        assert_eq!(clock.last().map(|s| s.track), Some(2));
    }

    #[test]
    fn after_a_seek_the_clock_counts_from_the_seek_target() {
        let mut clock = TrackClock::default();
        clock.begin(0, 1, 0.0, 48_000);
        clock.clear();
        clock.begin(96_000, 1, 30.0, 48_000);
        let (track, t) = clock.at(96_000 + 24_000).unwrap();
        assert_eq!(track, 1);
        assert!((t - 30.5).abs() < 1e-9);
        // Before the segment (the device still plays what it had): its start.
        let (_, t) = clock.at(90_000).unwrap();
        assert!((t - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_device_without_a_clock_plays_in_real_time_pauses_and_never_plays_more_than_it_got() {
        let mut c = RealTimeClock::new(1000);
        assert_eq!(c.played(0, 0.0), 0);
        // 500 frames given at t=1.0: half a second of audio.
        c.on_take(0, 1.0);
        assert_eq!(c.played(500, 1.25), 250);
        assert_eq!(c.played(500, 9.0), 500, "never more than given");
        // It ran dry; 300 more at t=10: the clock restarts at 500.
        c.on_take(500, 10.0);
        assert_eq!(c.played(800, 10.1), 600);
        // Paused at 10.1: the position holds.
        c.pause(800, 10.1);
        assert_eq!(c.played(800, 20.0), 600);
        c.resume(20.0);
        assert_eq!(c.played(800, 20.1), 700);
        // A clear: what was queued is gone, the rest counts as played.
        c.clear(700);
        assert_eq!(c.played(700, 25.0), 700);
    }
}
