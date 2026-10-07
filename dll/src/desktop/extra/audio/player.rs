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

pub use azul_core::audio::AudioPlayerState;

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

    /// Makes `source` (track `id`) the track heard next, from `position_s`, and HOLDS: nothing
    /// is heard until [`set_paused`](Self::set_paused)`(false)`; what was playing and queued is
    /// dropped.
    pub(crate) fn preload(
        &mut self,
        id: u64,
        source: Box<dyn PcmSource>,
        position_s: f64,
        out: &dyn PcmOutput,
    ) {
        let _ = position_s;
        self.load(id, source, out);
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
            buffered_s: 0.0,
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

// ==== The AudioPlayer handle: PlayerCore on a thread, feeding an AudioSink ====

/// Opens the output a player plays on: the platform's (`AudioSink::open`), or a test's.
pub(crate) type OpenOutput = fn(azul_core::audio::AudioConfig) -> super::AudioSink;

/// A music player: hand it audio files, it decodes and plays them on its own thread through an
/// `AudioSink` - gapless from one queued file to the next - and says what is heard
/// ([`get_state`](Self::get_state)). Every call returns at once; a file opens on the player's
/// thread (one that does not open shows up as `AudioPlayerState::failed_track` and
/// [`error_message`](Self::error_message)). Dropping the handle stops playback.
#[repr(C)]
pub struct AudioPlayer {
    /// Opaque pointer to the engine-side state (null when closed).
    pub ptr: *mut core::ffi::c_void,
    /// Whether this handle owns (and on drop stops) the player.
    pub run_destructor: bool,
}

impl Clone for AudioPlayer {
    fn clone(&self) -> Self {
        // Non-owning shallow copy (the FFI handle convention): only the original stops it.
        AudioPlayer {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        AudioPlayer {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.close();
    }
}

/// Where a track's audio comes from.
enum SourceSpec {
    Path(String),
    Bytes(Vec<u8>, String),
}

impl SourceSpec {
    fn open(self) -> Result<super::decode::FileSource, String> {
        match self {
            SourceSpec::Path(path) => super::decode::FileSource::open_path(&path),
            SourceSpec::Bytes(bytes, extension) => {
                super::decode::FileSource::open_bytes(bytes, &extension)
            }
        }
    }
}

/// What the handle asks the player's thread to do.
enum Command {
    Load(u64, SourceSpec),
    Queue(u64, SourceSpec),
    ClearQueue,
    Play,
    Pause,
    Toggle,
    Stop,
    Seek(f64),
    Skip,
    Volume(f32),
}

/// What the handle and the player's thread share.
#[derive(Default)]
struct Shared {
    commands: VecDeque<Command>,
    state: AudioPlayerState,
    error: Option<String>,
    quit: bool,
}

type SharedRef = std::sync::Arc<(std::sync::Mutex<Shared>, std::sync::Condvar)>;

fn lock(shared: &SharedRef) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Engine-side state behind an open [`AudioPlayer`].
struct PlayerInner {
    shared: SharedRef,
    thread: Option<std::thread::JoinHandle<()>>,
    next_id: std::sync::atomic::AtomicU64,
}

/// Frames a player writes at once, and how far ahead of the listener it stays (seconds).
const CHUNK_FRAMES: usize = 2048;
const LEAD_S: f64 = 0.25;
/// How long the player's thread sleeps between feeds while playing, and while idle.
const TICK_PLAYING_MS: u64 = 10;
const TICK_IDLE_MS: u64 = 100;

/// The player's output: the sink, the state machine on it.
struct Output {
    sink: super::AudioSink,
    core: PlayerCore,
}

/// Opens an output for a first source at `rate`: two channels at its rate, else at 48 kHz (the
/// player resamples), else none and why.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn open_output(open: OpenOutput, rate: u32) -> Result<Output, String> {
    let mut why = String::new();
    for rate in [rate, 48_000] {
        if rate == 0 {
            continue;
        }
        let sink = open(azul_core::audio::AudioConfig {
            sample_rate: rate,
            channels: 2,
        });
        if sink.is_open() {
            let lead = (f64::from(rate) * LEAD_S) as u64;
            return Ok(Output {
                core: PlayerCore::new(rate, 2, CHUNK_FRAMES, lead),
                sink,
            });
        }
        if let azul_css::OptionString::Some(reason) = sink.error_message() {
            why = reason.as_str().to_string();
        }
    }
    Err(format!("no audio output: {why}"))
}

/// The player's thread: commands in, the output fed, the state out.
fn player_thread(shared: SharedRef, open: OpenOutput) {
    let mut output: Option<Output> = None;
    let mut error: Option<String> = None;
    let mut failed_track = 0u64;
    let mut volume = 1.0f32;
    let mut paused = false;
    loop {
        let commands: Vec<Command> = {
            let mut guard = lock(&shared);
            if guard.quit {
                return;
            }
            guard.commands.drain(..).collect()
        };
        for command in commands {
            match command {
                Command::Load(id, spec) | Command::Queue(id, spec) if output.is_none() => {
                    // The first file decides the output's rate.
                    match spec.open() {
                        Ok(source) => match open_output(open, source.rate()) {
                            Ok(mut out) => {
                                out.core.set_volume(volume);
                                out.core.load(id, Box::new(source), &out.sink);
                                paused = false;
                                output = Some(out);
                            }
                            Err(why) => {
                                failed_track = id;
                                error = Some(why);
                            }
                        },
                        Err(why) => {
                            failed_track = id;
                            error = Some(why);
                        }
                    }
                }
                Command::Load(id, spec) => match spec.open() {
                    Ok(source) => {
                        if let Some(out) = output.as_mut() {
                            out.core.load(id, Box::new(source), &out.sink);
                            out.core.set_paused(false, &out.sink);
                            paused = false;
                        }
                    }
                    Err(why) => {
                        failed_track = id;
                        error = Some(why);
                    }
                },
                Command::Queue(id, spec) => match spec.open() {
                    Ok(source) => {
                        if let Some(out) = output.as_mut() {
                            out.core.enqueue(id, Box::new(source));
                        }
                    }
                    Err(why) => {
                        failed_track = id;
                        error = Some(why);
                    }
                },
                Command::Volume(v) => {
                    volume = if v.is_finite() {
                        v.clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    if let Some(out) = output.as_mut() {
                        out.core.set_volume(volume);
                    }
                }
                Command::Play | Command::Pause | Command::Toggle => {
                    paused = match command {
                        Command::Play => false,
                        Command::Pause => true,
                        _ => !paused,
                    };
                    if let Some(out) = output.as_mut() {
                        out.core.set_paused(paused, &out.sink);
                    }
                }
                Command::ClearQueue => {
                    if let Some(out) = output.as_mut() {
                        out.core.clear_queue();
                    }
                }
                Command::Stop => {
                    if let Some(out) = output.as_mut() {
                        out.core.stop(&out.sink);
                    }
                }
                Command::Seek(seconds) => {
                    if let Some(out) = output.as_mut() {
                        out.core.seek(seconds, &out.sink);
                    }
                }
                Command::Skip => {
                    if let Some(out) = output.as_mut() {
                        out.core.skip(&out.sink);
                    }
                }
            }
        }
        let mut state = match output.as_mut() {
            Some(out) => {
                out.core.pump(&out.sink);
                out.core.state(&out.sink)
            }
            None => AudioPlayerState::default(),
        };
        state.failed_track = failed_track;
        state.volume = volume;
        state.has_output = output.is_some();
        let tick = if state.playing {
            TICK_PLAYING_MS
        } else {
            TICK_IDLE_MS
        };
        let mut guard = lock(&shared);
        guard.state = state;
        guard.error.clone_from(&error);
        if guard.commands.is_empty() && !guard.quit {
            let _ = shared
                .1
                .wait_timeout(guard, std::time::Duration::from_millis(tick));
        }
    }
}

impl AudioPlayer {
    /// A player on the platform's audio output (opened with the first file). In a headless run
    /// it plays on the synthetic sink when one was asked for (`AZ_SYNTHETIC_DEVICES=audio_sink`),
    /// else it has no output (`AudioPlayerState::has_output` false, `error_message` says why).
    pub fn create() -> AudioPlayer {
        Self::create_with(super::AudioSink::open)
    }

    /// A player on the outputs `open` makes.
    pub(crate) fn create_with(open: OpenOutput) -> AudioPlayer {
        let shared: SharedRef = std::sync::Arc::new((
            std::sync::Mutex::new(Shared {
                state: AudioPlayerState {
                    volume: 1.0,
                    ..AudioPlayerState::default()
                },
                ..Shared::default()
            }),
            std::sync::Condvar::new(),
        ));
        let for_thread = shared.clone();
        let thread = std::thread::Builder::new()
            .name(String::from("azul-audio-player"))
            .spawn(move || player_thread(for_thread, open));
        let thread = match thread {
            Ok(t) => Some(t),
            Err(e) => {
                lock(&shared).error = Some(format!("the player's thread did not start: {e}"));
                None
            }
        };
        AudioPlayer {
            ptr: Box::into_raw(Box::new(PlayerInner {
                shared,
                thread,
                next_id: std::sync::atomic::AtomicU64::new(1),
            })) as *mut core::ffi::c_void,
            run_destructor: true,
        }
    }

    fn inner(&self) -> Option<&PlayerInner> {
        unsafe { (self.ptr as *const PlayerInner).as_ref() }
    }

    /// Hands `command` to the player's thread and wakes it.
    fn send(&self, command: Command) {
        if let Some(inner) = self.inner() {
            lock(&inner.shared).commands.push_back(command);
            inner.shared.1.notify_all();
        }
    }

    /// A new track id, and the command made of it.
    fn send_track(&self, make: impl FnOnce(u64) -> Command) -> u64 {
        let Some(inner) = self.inner() else {
            return 0;
        };
        let id = inner
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.send(make(id));
        id
    }

    /// Plays the audio file at `path` now (what was playing and queued is dropped). Returns the
    /// track's id (what `AudioPlayerState::track` names while it is heard); 0 when closed.
    pub fn load_file(&self, path: azul_css::AzString) -> u64 {
        let path = path.as_str().to_string();
        self.send_track(|id| Command::Load(id, SourceSpec::Path(path)))
    }

    /// Plays an audio file from its `bytes` now (`extension`: "mp3", "flac", ... - a hint, may
    /// be empty). Returns the track's id; 0 when closed.
    pub fn load_bytes(&self, bytes: azul_css::U8Vec, extension: azul_css::AzString) -> u64 {
        let spec = SourceSpec::Bytes(bytes.as_ref().to_vec(), extension.as_str().to_string());
        self.send_track(|id| Command::Load(id, spec))
    }

    /// Gets the audio file at `path` ready to play from `position_s` and HOLDS: the file and the
    /// output are opened and the first samples decoded on the player's thread, nothing is heard
    /// until [`play`](Self::play) - which then starts at once (a video's sound that must start
    /// with its first picture). What was playing and queued is dropped. Ready when
    /// `AudioPlayerState::buffered_s` is above zero for this id. Returns the track's id; 0 when
    /// closed.
    pub fn preload_file(&self, path: azul_css::AzString, position_s: f64) -> u64 {
        let _ = position_s;
        self.load_file(path)
    }

    /// Plays the audio file at `path` after the queued ones, gaplessly. Returns its id.
    pub fn queue_file(&self, path: azul_css::AzString) -> u64 {
        let path = path.as_str().to_string();
        self.send_track(|id| Command::Queue(id, SourceSpec::Path(path)))
    }

    /// Plays an audio file from its `bytes` after the queued ones, gaplessly. Returns its id.
    pub fn queue_bytes(&self, bytes: azul_css::U8Vec, extension: azul_css::AzString) -> u64 {
        let spec = SourceSpec::Bytes(bytes.as_ref().to_vec(), extension.as_str().to_string());
        self.send_track(|id| Command::Queue(id, spec))
    }

    /// Drops the queued files (the one playing plays on).
    pub fn clear_queue(&self) {
        self.send(Command::ClearQueue);
    }

    /// Plays on (after `pause`).
    pub fn play(&self) {
        self.send(Command::Play);
    }

    /// Holds playback where it is.
    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    /// Plays if paused, pauses if playing.
    pub fn toggle(&self) {
        self.send(Command::Toggle);
    }

    /// Stops: nothing plays, nothing is queued.
    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    /// Goes to `position_s` seconds in the file playing.
    pub fn seek(&self, position_s: f64) {
        self.send(Command::Seek(position_s));
    }

    /// Goes on with the next queued file now.
    pub fn skip(&self) {
        self.send(Command::Skip);
    }

    /// The volume, `0.0` (silent) to `1.0` (as decoded).
    pub fn set_volume(&self, volume: f32) {
        self.send(Command::Volume(volume));
    }

    /// What the listener hears now: the track, the position in it, its length, the level, and
    /// whether it plays, finished, or has no output.
    pub fn get_state(&self) -> AudioPlayerState {
        self.inner()
            .map_or_else(AudioPlayerState::default, |i| lock(&i.shared).state)
    }

    /// Why the last file did not open, or why there is no audio output; `None` when all is well.
    pub fn error_message(&self) -> azul_css::OptionString {
        match self.inner().and_then(|i| lock(&i.shared).error.clone()) {
            Some(why) => azul_css::OptionString::Some(azul_css::AzString::from(why)),
            None => azul_css::OptionString::None,
        }
    }

    /// Stops playback and ends the player's thread. (Dropping the handle does this too.)
    pub fn close(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            let mut inner = unsafe { Box::from_raw(self.ptr as *mut PlayerInner) };
            lock(&inner.shared).quit = true;
            inner.shared.1.notify_all();
            if let Some(thread) = inner.thread.take() {
                let _ = thread.join();
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

#[cfg(all(test, feature = "audio-decode"))]
mod handle_tests {
    use std::time::{Duration, Instant};

    use azul_core::audio::AudioConfig;
    use azul_css::{AzString, U8Vec};
    use azul_layout::request::mock::MockDevice;

    use super::{AudioPlayer, AudioPlayerState};
    use crate::desktop::extra::audio::{decode::fixtures, AudioSink};

    /// The headless synthetic output: plays in real time, hears nothing.
    fn synthetic(config: AudioConfig) -> AudioSink {
        AudioSink::open_as(config, MockDevice::Synthetic)
    }

    /// `seconds` of a 8 kHz mono tone, as a WAV file.
    fn tone(seconds: f64) -> U8Vec {
        let frames = (seconds * 8000.0) as usize;
        U8Vec::from_vec(fixtures::wav(8000, 1, frames, |i, _| {
            fixtures::sine16(i, 8000, 440.0, 0.5)
        }))
    }

    /// The player's state once `done` holds (or after `limit`).
    fn wait(
        player: &AudioPlayer,
        limit: Duration,
        done: impl Fn(&AudioPlayerState) -> bool,
    ) -> AudioPlayerState {
        let start = Instant::now();
        loop {
            let s = player.get_state();
            if done(&s) || start.elapsed() > limit {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_player_plays_a_file_to_its_end_on_its_own_thread() {
        let player = AudioPlayer::create_with(synthetic);
        assert_eq!(player.get_state().track, 0, "nothing loaded yet");
        let id = player.load_bytes(tone(0.3), AzString::from("wav"));
        assert!(id > 0);
        let playing = wait(&player, Duration::from_secs(2), |s| s.playing);
        assert_eq!(playing.track, id);
        assert!(playing.has_output);
        assert!((playing.duration_s - 0.3).abs() < 1e-3);
        let s = wait(&player, Duration::from_secs(3), |s| s.finished);
        assert!(s.finished, "{s:?}");
        assert_eq!(s.track, id);
        assert!((s.position_s - 0.3).abs() < 0.02, "{s:?}");
    }

    #[test]
    fn a_queued_file_plays_after_the_first_and_the_state_follows_it() {
        let player = AudioPlayer::create_with(synthetic);
        let first = player.load_bytes(tone(0.2), AzString::from("wav"));
        let second = player.queue_bytes(tone(0.2), AzString::from("wav"));
        assert!(second > first);
        let s = wait(&player, Duration::from_secs(3), |s| s.track == second);
        assert_eq!(s.track, second, "the second file is heard after the first");
        let s = wait(&player, Duration::from_secs(3), |s| s.finished);
        assert!(s.finished && s.track == second, "{s:?}");
    }

    #[test]
    fn a_file_that_does_not_open_is_reported_with_its_id_and_reason() {
        let player = AudioPlayer::create_with(synthetic);
        let id = player.load_file(AzString::from("/nonexistent/missing-song.mp3"));
        let s = wait(&player, Duration::from_secs(2), |s| s.failed_track == id);
        assert_eq!(s.failed_track, id);
        let why = player.error_message().into_option().expect("a reason");
        assert!(
            why.as_str().contains("missing-song.mp3"),
            "{}",
            why.as_str()
        );
    }

    #[test]
    fn a_preloaded_file_is_ready_and_silent_until_play_then_starts_where_it_was_preloaded() {
        let player = AudioPlayer::create_with(synthetic);
        let path = std::env::temp_dir().join(format!(
            "azul-player-preload-{}.wav",
            std::process::id()
        ));
        let wav: Vec<u8> = tone(3.0).as_ref().to_vec();
        std::fs::write(&path, wav).expect("the test file is written");
        let id = player.preload_file(AzString::from(path.to_string_lossy().into_owned()), 1.0);
        assert!(id > 0);
        let ready = wait(&player, Duration::from_secs(2), |s| {
            s.track == id && s.buffered_s > 0.0
        });
        assert!(ready.buffered_s > 0.0, "ready to start: {ready:?}");
        assert!(!ready.playing, "held: {ready:?}");
        std::thread::sleep(Duration::from_millis(200));
        let held = player.get_state();
        assert!(!held.playing, "{held:?}");
        assert!(
            (held.position_s - 1.0).abs() < 0.01,
            "nothing is heard while held: {held:?}"
        );
        player.play();
        let s = wait(&player, Duration::from_secs(2), |s| s.position_s > 1.05);
        assert!(s.playing && s.position_s > 1.05, "{s:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_pause_holds_the_position_and_a_seek_moves_it() {
        let player = AudioPlayer::create_with(synthetic);
        player.load_bytes(tone(3.0), AzString::from("wav"));
        wait(&player, Duration::from_secs(2), |s| s.position_s > 0.1);
        player.pause();
        let held = wait(&player, Duration::from_secs(1), |s| !s.playing);
        assert!(!held.playing);
        std::thread::sleep(Duration::from_millis(100));
        assert!((player.get_state().position_s - held.position_s).abs() < 1e-6);
        player.seek(2.0);
        let s = wait(&player, Duration::from_secs(1), |s| {
            (s.position_s - 2.0).abs() < 1e-3
        });
        assert!((s.position_s - 2.0).abs() < 1e-3, "{s:?}");
        player.play();
        let s = wait(&player, Duration::from_secs(1), |s| s.position_s > 2.05);
        assert!(s.playing && s.position_s > 2.05, "{s:?}");
        player.set_volume(0.25);
        let s = wait(&player, Duration::from_secs(1), |s| {
            (s.volume - 0.25).abs() < 1e-6
        });
        assert!((s.volume - 0.25).abs() < 1e-6);
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
        /// The output plays again after a clear (AVFoundation's player node: `stop`, `play`).
        pub clear_resumes: Cell<bool>,
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
                clear_resumes: Cell::new(false),
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
            if self.clear_resumes.get() {
                self.paused.set(false);
            }
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
    fn a_preloaded_track_is_decoded_ahead_and_heard_only_after_play() {
        let out = FakeOutput::new(1000, 1, 10_000);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.preload(1, Ramp::boxed(1000, 2000, 0.0), 0.5, &out);
        core.pump(&out);
        core.pump(&out);
        assert!(out.paused.get(), "the output holds");
        assert!(
            out.left().is_empty(),
            "nothing reaches the output while held"
        );
        let s = core.state(&out);
        assert_eq!(s.track, 1);
        assert!(!s.playing && !s.finished, "{s:?}");
        assert!(
            close(s.position_s, 0.5),
            "held at the preload position: {}",
            s.position_s
        );
        assert!(
            s.buffered_s >= 0.1 - 1e-9,
            "a chunk is decoded ahead, ready to start: {}",
            s.buffered_s
        );
        core.set_paused(false, &out);
        core.pump(&out);
        assert!(!out.paused.get());
        let left = out.left();
        assert_eq!(left.len(), 300, "play fills the lead at once");
        assert!(
            (left[0] - 500.0 / 100_000.0).abs() < 1e-6,
            "the first frame heard is the preload position's: {}",
            left[0]
        );
        assert!(
            left.windows(2)
                .all(|w| (w[1] - w[0] - 1.0 / 100_000.0).abs() < 1e-6),
            "no frame lost or repeated between the chunk decoded ahead and the rest"
        );
        out.advance(100);
        let s = core.state(&out);
        assert!(s.playing);
        assert!(close(s.position_s, 0.6), "{}", s.position_s);
    }

    #[test]
    fn a_seek_while_held_decodes_ahead_from_the_target_and_the_output_stays_held() {
        let out = FakeOutput::new(1000, 1, 10_000);
        // An output whose clear plays it again (AVFoundation's player node does).
        out.clear_resumes.set(true);
        let mut core = PlayerCore::new(1000, 1, 100, 300);
        core.preload(1, Ramp::boxed(1000, 2000, 0.0), 0.0, &out);
        assert!(out.paused.get(), "held after the preload's clear");
        core.pump(&out);
        core.seek(1.5, &out);
        assert!(out.paused.get(), "a seek while held stays held");
        core.pump(&out);
        assert!(out.left().is_empty(), "nothing heard while held");
        let s = core.state(&out);
        assert!(close(s.position_s, 1.5), "{}", s.position_s);
        assert!(s.buffered_s > 0.0, "decoded ahead from the target");
        core.set_paused(false, &out);
        core.pump(&out);
        let left = out.left();
        assert!(
            (left[0] - 1500.0 / 100_000.0).abs() < 1e-6,
            "the first frame heard is the target's: {}",
            left[0]
        );
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
