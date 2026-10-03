//! Playing audio files - the `AudioPlayer` handle.
//!
//! An app that plays music hands the player FILES (a path, or the bytes) and asks it what is
//! heard; it never touches samples. The player decodes on its own thread
//! ([`super::decode::FileSource`]), converts to the output's format (channels remixed, the rate
//! resampled), feeds an `AudioSink` a short lead ahead of the listener (so a pause or a seek is
//! heard at once: the sink pauses in place and drops its queue on a seek), and plays queued tracks
//! GAPLESSLY: the next file's first sample follows the last one's last sample in the same output
//! stream, with no reopen and no silence.
//!
//! What the player reports ([`AudioPlayerState`]) is what the listener HEARS - the track and the
//! position at the output's play head, not at the decoder (with a gapless queue the decoder is
//! already in the next track while the end of the last one is still queued) - plus the level of
//! the audio being heard, for a level meter.
//!
//! The arithmetic is [`super::playback`]'s; [`PlayerCore`] is the player's state machine, a pure
//! step function (`pump`) over a [`PcmOutput`] so it is tested without a device; the handle runs
//! it on a thread.

use std::collections::VecDeque;

use super::playback::{
    apply_gain, chunk_peaks, remix, LevelHistory, LinearResampler, Rechunker, TrackClock,
};

/// What an [`AudioPlayer`](self) is doing, as the listener hears it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioPlayerState {
    /// Where the listener is in the track heard now, in seconds.
    pub position_s: f64,
    /// The length of the track heard now, in seconds (0 when unknown).
    pub duration_s: f64,
    /// The id of the track heard now (what `load_*` / `queue_*` returned); 0 = none.
    pub track: u64,
    /// The id of the last track that could not be opened (0 = none): its reason is
    /// `AudioPlayer::error_message`.
    pub failed_track: u64,
    /// The player's volume, `0.0..=1.0`.
    pub volume: f32,
    /// The peak level (`0.0..=1.0`) of the left channel of the audio heard now (a meter's input).
    pub peak_left: f32,
    /// The peak level of the right channel.
    pub peak_right: f32,
    /// Tracks queued after the one being decoded.
    pub queued_tracks: u32,
    /// Playback runs (not paused, something to play, an output open).
    pub playing: bool,
    /// Everything loaded and queued has been heard to its end.
    pub finished: bool,
    /// An audio output opened: false in a headless run without the synthetic sink, and on a
    /// machine without an output device (`AudioPlayer::error_message` says why).
    pub has_output: bool,
}

/// Decoded audio the player plays: a file, or (in tests) a made-up signal.
pub(crate) trait PcmSource {
    /// Samples per second per channel of what `next_samples` hands out.
    fn rate(&self) -> u32;
    /// Channels of what `next_samples` hands out.
    fn channels(&self) -> u16;
    /// The length in seconds (0 when unknown).
    fn duration_s(&self) -> f64;
    /// The next samples (interleaved), `None` at the end.
    fn next_samples(&mut self) -> Option<Vec<f32>>;
    /// Goes to `seconds`; the media time reached, `None` when it cannot seek.
    fn seek(&mut self, seconds: f64) -> Option<f64>;
}

impl PcmSource for super::decode::FileSource {
    fn rate(&self) -> u32 {
        super::decode::FileSource::rate(self)
    }
    fn channels(&self) -> u16 {
        super::decode::FileSource::channels(self)
    }
    fn duration_s(&self) -> f64 {
        self.info().duration_s
    }
    fn next_samples(&mut self) -> Option<Vec<f32>> {
        super::decode::FileSource::next_samples(self)
    }
    fn seek(&mut self, seconds: f64) -> Option<f64> {
        super::decode::FileSource::seek(self, seconds)
    }
}

/// Where the player's audio goes: an `AudioSink`, or (in tests) a stand-in.
pub(crate) trait PcmOutput {
    /// The output's sample rate.
    fn rate(&self) -> u32;
    /// The output's channels.
    fn channels(&self) -> u16;
    /// Hands interleaved samples in the output's format over; false when it did not take them.
    fn try_play(&self, samples: &[f32]) -> bool;
    /// Frames taken and not heard yet.
    fn queued_frames(&self) -> u64;
    /// Frames heard so far.
    fn samples_played(&self) -> u64;
    /// Holds playback in place (false: cannot).
    fn pause(&self) -> bool;
    /// Plays on.
    fn resume(&self);
    /// Drops what is queued (false: cannot).
    fn clear(&self) -> bool;
}

impl PcmOutput for super::AudioSink {
    fn rate(&self) -> u32 {
        self.config().sample_rate
    }
    fn channels(&self) -> u16 {
        self.config().channels
    }
    fn try_play(&self, samples: &[f32]) -> bool {
        let config = self.config();
        super::AudioSink::try_play(
            self,
            azul_core::audio::AudioFrame {
                sample_rate: config.sample_rate,
                channels: config.channels,
                samples: azul_css::F32Vec::from_vec(samples.to_vec()),
            },
        )
    }
    fn queued_frames(&self) -> u64 {
        super::AudioSink::queued_frames(self)
    }
    fn samples_played(&self) -> u64 {
        super::AudioSink::samples_played(self)
    }
    fn pause(&self) -> bool {
        super::AudioSink::pause(self)
    }
    fn resume(&self) {
        super::AudioSink::resume(self);
    }
    fn clear(&self) -> bool {
        super::AudioSink::clear(self)
    }
}

/// The player's state machine: the track being decoded and the queue after it, the conversion
/// to the output's format, and the clock of what is heard. [`pump`](Self::pump) feeds the
/// output up to the lead; everything else is a command.
pub(crate) struct PlayerCore {
    /// The track being decoded (id, source).
    current: Option<(u64, Box<dyn PcmSource>)>,
    /// The tracks after it, in order.
    queue: VecDeque<(u64, Box<dyn PcmSource>)>,
    /// The resampler of the track being decoded, for its rate, when it differs from the output's.
    resampler: Option<(u32, LinearResampler)>,
    chunker: Rechunker,
    /// A chunk the output did not take, offered again first.
    held: Option<Vec<f32>>,
    clock: TrackClock,
    levels: LevelHistory,
    /// The volume asked for, and the gain the last chunk ended at (the ramp's start).
    volume: f32,
    gain: f32,
    paused: bool,
    /// Frames handed to the output (taken back off by a clear).
    written: u64,
    lead_frames: u64,
    out_rate: u32,
    out_channels: u16,
    /// The lengths of the last tracks (the one heard may be before the one decoded).
    durations: VecDeque<(u64, f64)>,
}

/// Track lengths a player remembers (the heard track trails the decoded one by one at most).
const DURATIONS_KEPT: usize = 8;

impl PlayerCore {
    /// A player for an output at `out_rate` x `out_channels`, writing chunks of `chunk_frames`
    /// and keeping at most `lead_frames` queued ahead of the listener.
    pub(crate) fn new(
        out_rate: u32,
        out_channels: u16,
        chunk_frames: usize,
        lead_frames: u64,
    ) -> Self {
        Self {
            current: None,
            queue: VecDeque::new(),
            resampler: None,
            chunker: Rechunker::new(out_channels, chunk_frames),
            held: None,
            clock: TrackClock::default(),
            levels: LevelHistory::default(),
            volume: 1.0,
            gain: 1.0,
            paused: false,
            written: 0,
            lead_frames: lead_frames.max(1),
            out_rate: out_rate.max(1),
            out_channels: out_channels.max(1),
            durations: VecDeque::new(),
        }
    }

    fn remember_duration(&mut self, id: u64, duration_s: f64) {
        self.durations.retain(|(i, _)| *i != id);
        self.durations.push_back((id, duration_s));
        while self.durations.len() > DURATIONS_KEPT {
            self.durations.pop_front();
        }
    }

    /// Drops what the output has queued and everything in flight here; the next frame written
    /// starts a new stretch.
    fn restart_output(&mut self, out: &dyn PcmOutput) {
        let _ = out.clear();
        self.written = out.samples_played() + out.queued_frames();
        self.chunker.clear();
        self.held = None;
        self.resampler = None;
        self.clock.clear();
        self.levels.clear();
    }

    /// Plays `source` (track `id`) now: what was playing and queued is dropped.
    pub(crate) fn load(&mut self, id: u64, source: Box<dyn PcmSource>, out: &dyn PcmOutput) {
        self.queue.clear();
        self.start_now(id, source, out);
    }

    /// `source` becomes the track being decoded, heard from the next frame written.
    fn start_now(&mut self, id: u64, source: Box<dyn PcmSource>, out: &dyn PcmOutput) {
        self.restart_output(out);
        self.remember_duration(id, source.duration_s());
        self.clock.begin(self.written, id, 0.0, self.out_rate);
        self.current = Some((id, source));
    }

    /// Plays `source` (track `id`) after the queue, gaplessly.
    pub(crate) fn enqueue(&mut self, id: u64, source: Box<dyn PcmSource>) {
        if self.current.is_none() && self.queue.is_empty() {
            // Nothing is being decoded (all ran out, or nothing was loaded): this one follows
            // whatever the output still has to play.
            let start = self.written
                + self
                    .held
                    .as_ref()
                    .map_or(0, |h| (h.len() / usize::from(self.out_channels)) as u64)
                + self.chunker.pending_frames() as u64;
            self.remember_duration(id, source.duration_s());
            self.clock.begin(start, id, 0.0, self.out_rate);
            self.resampler = None;
            self.current = Some((id, source));
        } else {
            self.queue.push_back((id, source));
        }
    }

    /// Drops the queued tracks (the one being decoded plays on).
    pub(crate) fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Ends the track being decoded and goes on with the next queued one now.
    pub(crate) fn skip(&mut self, out: &dyn PcmOutput) {
        match self.queue.pop_front() {
            Some((id, source)) => self.start_now(id, source, out),
            None => self.stop(out),
        }
    }

    /// Goes to `seconds` in the track being decoded; what was queued is dropped.
    pub(crate) fn seek(&mut self, seconds: f64, out: &dyn PcmOutput) {
        let Some((id, mut source)) = self.current.take() else {
            return;
        };
        let reached = source.seek(seconds);
        self.restart_output(out);
        self.clock
            .begin(self.written, id, reached.unwrap_or(0.0), self.out_rate);
        self.current = Some((id, source));
    }

    /// Holds (`true`) or resumes playback.
    pub(crate) fn set_paused(&mut self, paused: bool, out: &dyn PcmOutput) {
        self.paused = paused;
        if paused {
            let _ = out.pause();
        } else {
            out.resume();
        }
    }

    /// The volume, `0.0..=1.0` (ramped in over the next chunk).
    pub(crate) fn set_volume(&mut self, volume: f32) {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    /// Drops everything: nothing plays, nothing is queued.
    pub(crate) fn stop(&mut self, out: &dyn PcmOutput) {
        self.queue.clear();
        self.current = None;
        self.restart_output(out);
    }

    /// The next chunk in the output's format, the gain applied; `None` when nothing is left.
    fn next_chunk(&mut self) -> Option<Vec<f32>> {
        loop {
            if let Some(chunk) = self.chunker.pop() {
                return Some(self.finish_chunk(chunk));
            }
            let Some((_, source)) = self.current.as_mut() else {
                let tail = self.chunker.flush()?;
                return Some(self.finish_chunk(tail));
            };
            match source.next_samples() {
                Some(samples) => {
                    let (rate, channels) = (source.rate(), source.channels());
                    let samples = self.convert(&samples, rate, channels);
                    self.chunker.push(&samples);
                }
                None => {
                    // The track ran out: the next queued one starts at the very next frame.
                    self.resampler = None;
                    match self.queue.pop_front() {
                        Some((id, next)) => {
                            let start = self.written + self.chunker.pending_frames() as u64;
                            self.remember_duration(id, next.duration_s());
                            self.clock.begin(start, id, 0.0, self.out_rate);
                            self.current = Some((id, next));
                        }
                        None => self.current = None,
                    }
                }
            }
        }
    }

    /// `samples` (interleaved, `channels` at `rate`) in the output's channels and rate.
    fn convert(&mut self, samples: &[f32], rate: u32, channels: u16) -> Vec<f32> {
        let remixed = remix(samples, channels, self.out_channels);
        if rate == self.out_rate || rate == 0 {
            return remixed;
        }
        let fits = matches!(&self.resampler, Some((r, _)) if *r == rate);
        if !fits {
            self.resampler = Some((
                rate,
                LinearResampler::new(rate, self.out_rate, self.out_channels),
            ));
        }
        match self.resampler.as_mut() {
            Some((_, resampler)) => resampler.process(&remixed),
            None => remixed,
        }
    }

    /// The gain applied to a chunk: from where the last chunk ended to the volume asked for.
    fn finish_chunk(&mut self, mut chunk: Vec<f32>) -> Vec<f32> {
        let (from, to) = (self.gain, self.volume);
        if (from - 1.0).abs() > f32::EPSILON || (to - 1.0).abs() > f32::EPSILON {
            apply_gain(&mut chunk, self.out_channels, from, to);
        }
        self.gain = to;
        chunk
    }

    /// Feeds the output up to the lead. Returns the frames written.
    pub(crate) fn pump(&mut self, out: &dyn PcmOutput) -> u64 {
        if self.paused {
            return 0;
        }
        let channels = usize::from(self.out_channels);
        let mut wrote = 0u64;
        // At most one lead's worth a call, so an output that cannot tell its queue (and does
        // not block) is never flooded.
        while out.queued_frames() < self.lead_frames && wrote < self.lead_frames {
            let chunk = match self.held.take() {
                Some(chunk) => chunk,
                None => match self.next_chunk() {
                    Some(chunk) => chunk,
                    None => break,
                },
            };
            let frames = (chunk.len() / channels) as u64;
            if out.try_play(&chunk) {
                self.levels
                    .push(self.written, chunk_peaks(&chunk, self.out_channels));
                self.written += frames;
                wrote += frames;
            } else {
                self.held = Some(chunk);
                break;
            }
        }
        wrote
    }

    /// What the listener hears now.
    pub(crate) fn state(&mut self, out: &dyn PcmOutput) -> AudioPlayerState {
        let played = out.samples_played();
        let (track, position_s) = self.clock.at(played).unwrap_or((0, 0.0));
        let duration_s = self
            .durations
            .iter()
            .find(|(id, _)| *id == track)
            .map_or(0.0, |(_, d)| *d);
        let position_s = if duration_s > 0.0 {
            position_s.min(duration_s)
        } else {
            position_s
        };
        let (peak_left, peak_right) = self.levels.at(played, self.written);
        let decoding = self.current.is_some()
            || !self.queue.is_empty()
            || self.held.is_some()
            || self.chunker.pending_frames() > 0;
        let finished = track != 0 && !decoding && played >= self.written;
        AudioPlayerState {
            position_s,
            duration_s,
            track,
            failed_track: 0,
            volume: self.volume,
            peak_left,
            peak_right,
            queued_tracks: u32::try_from(self.queue.len()).unwrap_or(u32::MAX),
            playing: !self.paused && track != 0 && !finished,
            finished,
            has_output: true,
        }
    }
}

#[cfg(test)]
pub(crate) mod player_tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A stand-in output: takes up to `capacity` queued frames, plays what the test says
    /// ([`FakeOutput::advance`]), records everything it was given.
    pub(crate) struct FakeOutput {
        pub rate: u32,
        pub channels: u16,
        pub capacity: u64,
        pub written: RefCell<Vec<f32>>,
        pub queued: Cell<u64>,
        pub played: Cell<u64>,
        pub clears: Cell<u32>,
        pub paused: Cell<bool>,
    }

    impl FakeOutput {
        pub(crate) fn new(rate: u32, channels: u16, capacity: u64) -> Self {
            Self {
                rate,
                channels,
                capacity,
                written: RefCell::new(Vec::new()),
                queued: Cell::new(0),
                played: Cell::new(0),
                clears: Cell::new(0),
                paused: Cell::new(false),
            }
        }

        /// The listener hears `frames` more (at most what is queued).
        pub(crate) fn advance(&self, frames: u64) {
            let n = frames.min(self.queued.get());
            self.queued.set(self.queued.get() - n);
            self.played.set(self.played.get() + n);
        }

        /// The first channel of every frame written, in order.
        pub(crate) fn left(&self) -> Vec<f32> {
            self.written
                .borrow()
                .chunks(usize::from(self.channels))
                .map(|f| f[0])
                .collect()
        }
    }

    impl PcmOutput for FakeOutput {
        fn rate(&self) -> u32 {
            self.rate
        }
        fn channels(&self) -> u16 {
            self.channels
        }
        fn try_play(&self, samples: &[f32]) -> bool {
            let frames = (samples.len() / usize::from(self.channels)) as u64;
            if self.queued.get() + frames > self.capacity {
                return false;
            }
            self.written.borrow_mut().extend_from_slice(samples);
            self.queued.set(self.queued.get() + frames);
            true
        }
        fn queued_frames(&self) -> u64 {
            self.queued.get()
        }
        fn samples_played(&self) -> u64 {
            self.played.get()
        }
        fn pause(&self) -> bool {
            self.paused.set(true);
            true
        }
        fn resume(&self) {
            self.paused.set(false);
        }
        fn clear(&self) -> bool {
            self.queued.set(0);
            self.clears.set(self.clears.get() + 1);
            true
        }
    }

    /// A made-up track: `frames` frames at `rate`, mono, frame `i` worth `base + i / 100000`,
    /// handed out 64 frames at a time (so the player must re-chunk).
    pub(crate) struct Ramp {
        pub rate: u32,
        pub frames: usize,
        pub base: f32,
        pub pos: usize,
    }

    impl Ramp {
        pub(crate) fn boxed(rate: u32, frames: usize, base: f32) -> Box<dyn PcmSource> {
            Box::new(Ramp {
                rate,
                frames,
                base,
                pos: 0,
            })
        }
    }

    impl PcmSource for Ramp {
        fn rate(&self) -> u32 {
            self.rate
        }
        fn channels(&self) -> u16 {
            1
        }
        fn duration_s(&self) -> f64 {
            self.frames as f64 / f64::from(self.rate)
        }
        fn next_samples(&mut self) -> Option<Vec<f32>> {
            if self.pos >= self.frames {
                return None;
            }
            let end = (self.pos + 64).min(self.frames);
            let out = (self.pos..end)
                .map(|i| self.base + i as f32 / 100_000.0)
                .collect();
            self.pos = end;
            Some(out)
        }
        fn seek(&mut self, seconds: f64) -> Option<f64> {
            self.pos = ((seconds * f64::from(self.rate)).round() as usize).min(self.frames);
            Some(self.pos as f64 / f64::from(self.rate))
        }
    }

    /// Pumps and plays `step` frames at a time until the player says finished (at most `max`
    /// rounds).
    fn play_out(core: &mut PlayerCore, out: &FakeOutput, step: u64, max: usize) {
        for _ in 0..max {
            core.pump(out);
            if core.state(out).finished {
                return;
            }
            out.advance(step);
        }
        panic!("the player never finished");
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_loaded_track_plays_to_its_end_and_the_player_says_finished() {
        let out = FakeOutput::new(1000, 2, 10_000);
        let mut core = PlayerCore::new(1000, 2, 100, 300);
        core.load(7, Ramp::boxed(1000, 1000, 0.0), &out);
        core.pump(&out);
        assert_eq!(out.queued.get(), 300, "a lead of 300 frames, no more");
        let s = core.state(&out);
        assert_eq!(s.track, 7);
        assert!(s.playing && !s.finished);
        assert!(close(s.duration_s, 1.0));
        play_out(&mut core, &out, 100, 100);
        let left = out.left();
        assert_eq!(left.len(), 1000, "every frame was written once");
        assert!(left
            .iter()
            .enumerate()
            .all(|(i, v)| (*v - i as f32 / 100_000.0).abs() < 1e-6));
        // Mono went to both channels.
        assert!(out.written.borrow().chunks(2).all(|f| f[0] == f[1]));
        let s = core.state(&out);
        assert!(s.finished && !s.playing);
        assert!(close(s.position_s, 1.0), "{}", s.position_s);
    }

    #[test]
    fn the_next_queued_track_follows_without_a_gap_and_the_state_names_the_one_heard() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.load(1, Ramp::boxed(1000, 250, 0.1), &out);
        core.enqueue(2, Ramp::boxed(1000, 250, 0.2));
        assert_eq!(core.state(&out).queued_tracks, 1);
        core.pump(&out);
        // 300 frames written: all of track 1 and the first 50 of track 2 - no gap between.
        let left = out.left();
        assert_eq!(left.len(), 300);
        assert!((left[249] - (0.1 + 249.0 / 100_000.0)).abs() < 1e-6);
        assert!(
            (left[250] - 0.2).abs() < 1e-6,
            "track 2 starts at the next frame"
        );
        // The listener is at frame 200: still track 1.
        out.advance(200);
        let s = core.state(&out);
        assert_eq!(s.track, 1);
        assert!(close(s.position_s, 0.2));
        // At frame 300: 50 frames into track 2.
        out.advance(100);
        let s = core.state(&out);
        assert_eq!(s.track, 2);
        assert!(close(s.position_s, 0.05), "{}", s.position_s);
        play_out(&mut core, &out, 100, 100);
        assert_eq!(out.left().len(), 500, "500 frames, end to end");
    }

    #[test]
    fn a_seek_drops_what_was_queued_and_plays_from_the_target() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.load(1, Ramp::boxed(1000, 2000, 0.0), &out);
        core.pump(&out);
        out.advance(100);
        // (The load dropped the output's queue too: a new track never plays after old audio.)
        let clears = out.clears.get();
        core.seek(1.5, &out);
        assert_eq!(
            out.clears.get(),
            clears + 1,
            "the queue of the old position is dropped"
        );
        assert!(close(core.state(&out).position_s, 1.5));
        let before = out.left().len();
        core.pump(&out);
        let left = out.left();
        assert!(
            (left[before] - 1500.0 / 100_000.0).abs() < 1e-6,
            "the first frame after the seek is frame 1500"
        );
        out.advance(100);
        assert!(close(core.state(&out).position_s, 1.6));
    }

    #[test]
    fn paused_the_player_writes_nothing_and_the_position_holds() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.load(1, Ramp::boxed(1000, 2000, 0.0), &out);
        core.pump(&out);
        out.advance(100);
        core.set_paused(true, &out);
        assert!(out.paused.get(), "the output holds in place");
        let written = out.left().len();
        out.advance(0);
        assert_eq!(core.pump(&out), 0);
        assert_eq!(out.left().len(), written);
        let s = core.state(&out);
        assert!(!s.playing && !s.finished);
        assert!(close(s.position_s, 0.1));
        core.set_paused(false, &out);
        assert!(!out.paused.get());
        assert!(core.state(&out).playing);
    }

    #[test]
    fn a_track_at_another_rate_is_resampled_to_the_output_rate_gaplessly() {
        let out = FakeOutput::new(1000, 1, 100_000);
        let mut core = PlayerCore::new(1000, 1, 100, 100_000);
        core.load(1, Ramp::boxed(1000, 500, 0.1), &out);
        // Half a second at 500 Hz: half a second at 1000 Hz once resampled.
        core.enqueue(2, Ramp::boxed(500, 250, 0.2));
        play_out(&mut core, &out, 10_000, 10);
        let n = out.left().len();
        assert!((995..=1000).contains(&n), "one second of output: {n}");
        let s = core.state(&out);
        assert_eq!(s.track, 2);
    }

    #[test]
    fn a_volume_change_ramps_in_over_the_next_chunk() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 100);
        // A constant 0.5 (the ramp's step is far below the test's tolerance).
        core.load(1, Ramp::boxed(1000, 1000, 0.5), &out);
        core.pump(&out);
        core.set_volume(0.5);
        assert!((core.state(&out).volume - 0.5).abs() < 1e-6);
        out.advance(100);
        core.pump(&out);
        out.advance(100);
        core.pump(&out);
        let left = out.left();
        assert!((left[99] - 0.501).abs() < 0.01, "full volume before");
        assert!(
            left[100] > left[199],
            "the change ramps over the next chunk"
        );
        assert!(
            (left[199] - 0.25).abs() < 0.01,
            "at the new volume by its end"
        );
        assert!((left[250] - 0.25).abs() < 0.01, "and stays there");
    }

    #[test]
    fn the_level_reported_is_that_of_the_audio_being_heard() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.load(1, Ramp::boxed(1000, 100, 0.1), &out);
        core.enqueue(2, Ramp::boxed(1000, 1000, 0.8));
        core.pump(&out);
        // Written: 0.1 then 0.8; heard: the first chunk (0.1).
        let s = core.state(&out);
        assert!((s.peak_left - 0.1).abs() < 0.01, "{}", s.peak_left);
        out.advance(150);
        let s = core.state(&out);
        assert!((s.peak_left - 0.8).abs() < 0.02, "{}", s.peak_left);
    }

    #[test]
    fn skip_goes_on_with_the_next_track_now_and_stop_drops_everything() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.load(1, Ramp::boxed(1000, 5000, 0.1), &out);
        core.enqueue(2, Ramp::boxed(1000, 5000, 0.2));
        core.pump(&out);
        out.advance(100);
        core.skip(&out);
        core.pump(&out);
        out.advance(1);
        let s = core.state(&out);
        assert_eq!(s.track, 2);
        assert_eq!(s.queued_tracks, 0);
        core.stop(&out);
        core.pump(&out);
        let s = core.state(&out);
        assert_eq!(s.track, 0);
        assert!(!s.playing);
    }
}
