//! The recorder: the one owner of a recording, driven from the window's thread.
//!
//! `Recorder` is a plain value the app keeps in its state (no global): it starts a session, takes the
//! frames of the screen, the camera and the voice and the events of the review as they happen, hands
//! the encoders their work, and sends what they make to the writer thread (`recorder_writer.rs`), which
//! owns every file. The window's thread does the cheap things only: copy the changed rows of the
//! window into the shadow, hand a picture to an encoder (azul's run on their own threads), poll the
//! packets, send them on. A 100 ms timer calls [`Recorder::tick`].
//!
//! It looks after itself: it pauses when the disk would fall under the free space it keeps (and goes on
//! when there is room), when nothing has happened for a while (and goes on at the next input), at a
//! hotkey or a click, and stops at the longest session it may run; everything it does is told to the
//! caller as [`Notice`]s and shown by [`Recorder::indicator`], which is what the always-visible
//! recording indicator draws.
//!
//! Nothing here knows azul: the encoders come through [`CodecFactory`], the clock, the disk probe and
//! the wall clock through [`Deps`], the files through `recorder_fs.rs`.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    recorder_clock::{Clock, CivilTime},
    recorder_codec::{AudioCodec, CodecFactory, RawFrame, VideoCodec},
    recorder_events::{event_channel, Caps, Dropped, EventLog, EventSink},
    recorder_fs::{new_session_dir, open_lines, DiskStore, ManifestFile},
    recorder_manifest::{
        AppInfo, ClockInfo, HostInfo, Manifest, Track, TrackStatus, TranscriptLine, Workspace, EVENTS_FILE, TRANSCRIPT_FILE,
    },
    recorder_policy::{
        disk_state, expected_bytes_per_hour, hours_left, length_state, may_resume_after_quota, DiskState, IdleChange,
        IdleWatch, LengthState, RateMeter,
    },
    recorder_screen::{Due, Pace, ScreenCapture, TapFrame, Tapped},
    recorder_settings::{RecorderSettings, AUDIO_KBPS},
    recorder_track::Kind,
    recorder_writer::{spawn, Cmd, Handle, Status, Writer, WriterConfig},
};

/// What the recorder needs from the machine.
pub struct Deps {
    /// The wall clock now, milliseconds since 1970 UTC.
    pub unix_ms: Box<dyn Fn() -> u64>,
    /// The session clock for a session that begins at the given wall-clock time.
    pub make_clock: Box<dyn Fn(u64) -> Arc<dyn Clock>>,
    /// The free bytes on the disk a path is on (`None`: it cannot be asked).
    pub free_bytes: Box<dyn Fn(&Path) -> Option<u64>>,
    /// Opens the encoders.
    pub factory: Box<dyn CodecFactory>,
}

/// What the system said about a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Perm {
    /// Allowed, or not yet asked (the system asks when the device opens).
    Granted,
    /// Refused: why, in words (`Privacy & Security`).
    Denied(String),
}

/// What a session starts with.
#[derive(Debug, Clone)]
pub struct StartRequest {
    /// Where the sessions go.
    pub recordings_dir: PathBuf,
    pub app: AppInfo,
    pub workspace: Workspace,
    pub host: HostInfo,
    /// The local time's offset from UTC, minutes.
    pub utc_offset_minutes: i32,
    pub camera: Perm,
    pub microphone: Perm,
    /// The devices' names, for the manifest.
    pub camera_device: Option<String>,
    pub microphone_device: Option<String>,
}

/// What a started session is called and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub name: String,
    pub dir: PathBuf,
}

/// What a stopped session leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub name: String,
    pub dir: PathBuf,
    /// The time it was recording, microseconds.
    pub recorded_us: u64,
    /// Bytes in its media files.
    pub media_bytes: u64,
    /// The writer finished in time (every file is finished).
    pub clean: bool,
}

/// Why a recording did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// Another recording is going.
    Busy,
    /// The disk has less free than the recorder keeps.
    DiskFull { free: u64, min_free: u64 },
    /// The session folder or a file of it could not be made: the system's words.
    Io(String),
    /// Nothing was asked for: no screen, no camera, no microphone.
    NothingToRecord,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("a recording is going already"),
            Self::DiskFull { free, min_free } => {
                write!(f, "the disk has {free} bytes free, under the {min_free} the recorder keeps free")
            }
            Self::Io(e) => write!(f, "the session could not be made: {e}"),
            Self::NothingToRecord => f.write_str("the screen, the camera and the microphone are all off in the settings"),
        }
    }
}

impl std::error::Error for StartError {}

/// Why a recording is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// The user paused it (the button, the hotkey).
    User,
    /// Nothing happened for the idle time.
    Idle,
    /// The disk would fall under the free space the recorder keeps.
    Quota,
}

impl PauseReason {
    const fn word(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Idle => "idle",
            Self::Quota => "quota",
        }
    }
}

/// Where the recorder stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording,
    Paused(PauseReason),
}

/// Something the recorder wants the user to know, once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The disk is getting low: free bytes and what is kept.
    DiskLow { free: u64, min_free: u64 },
    /// The disk cannot be asked how much room is left.
    DiskUnknown,
    /// The recording paused itself, or was paused.
    Paused(PauseReason),
    /// It went on.
    Resumed(PauseReason),
    /// The session reaches its longest length in this many microseconds.
    Ending { left_us: u64 },
    /// The session reached its longest length and stopped.
    LengthReached,
    /// A track broke: the words.
    TrackFailed { track: String, why: String },
    /// A track could not start: the words (no encoder, a refused device).
    TrackOff { track: &'static str, why: String },
    /// The manifest could not be written: the words.
    ManifestError(String),
}

/// What the always-visible indicator shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Indicator {
    pub state: State,
    /// The time recorded so far, microseconds.
    pub recorded_us: u64,
    /// The voice's level, 0 to 1 (it falls when nothing comes in).
    pub level: f32,
    /// Bytes in the media files.
    pub media_bytes: u64,
    /// Bytes free on the disk, when it could be asked.
    pub free_bytes: Option<u64>,
    /// Hours left at the current rate before the disk is down to what is kept free.
    pub hours_left: Option<f64>,
    pub disk: DiskState,
    /// Pictures an encoder refused (it was behind).
    pub dropped_frames: u64,
    /// The tracks that are not recording and why: `(track, words)`.
    pub problems: Vec<(String, String)>,
}

/// One track's encoder on the window's thread.
struct VideoPipe {
    dir: &'static str,
    codec: Option<Box<dyn VideoCodec>>,
    size: (u32, u32),
    /// The times of the pictures in the encoder, oldest first: its packets come out in this order.
    pending: VecDeque<u64>,
    dropped: u64,
    off: Option<String>,
    /// When a keyframe was last asked for (the camera: one at least every two seconds).
    last_key_us: Option<u64>,
}

impl VideoPipe {
    const fn new(dir: &'static str) -> Self {
        Self {
            dir,
            codec: None,
            size: (0, 0),
            pending: VecDeque::new(),
            dropped: 0,
            off: None,
            last_key_us: None,
        }
    }

    /// Whether the picture taken at `now_us` should be a keyframe: the first, and every two seconds.
    fn key_due(&mut self, now_us: u64) -> bool {
        let due = self.last_key_us.is_none_or(|t| now_us.saturating_sub(t) >= 2_000_000);
        if due {
            self.last_key_us = Some(now_us);
        }
        due
    }

    /// Sends the packets that are ready to the writer, each with the time of its picture.
    fn drain(&mut self, writer: &Handle) {
        let Some(codec) = self.codec.as_mut() else { return };
        while let Some(au) = codec.poll() {
            let pts = self.pending.pop_front().unwrap_or(0);
            writer.send(Cmd::Video { dir: self.dir, pts_us: pts, annexb: au });
        }
    }

    fn flush(&mut self, writer: &Handle) {
        if let Some(c) = self.codec.as_mut() {
            c.flush();
        }
        self.drain(writer);
    }
}

/// The voice's encoder and its clock.
struct AudioPipe {
    codec: Option<Box<dyn AudioCodec>>,
    /// The session time of the first sample since the pipe was (re)started.
    anchor_us: Option<u64>,
    /// Packets sent since the anchor.
    packets: u64,
    /// Seconds of audio taken since the anchor, microseconds: to find when the input fell behind.
    taken_us: u64,
    level: f32,
    off: Option<String>,
    /// The session time of the last packet sent: the timeline never goes back past it.
    last_pts: Option<u64>,
    /// How far the input was from the session clock at its last chunk, microseconds (positive: behind), and
    /// the most it has been.
    drift_us: i64,
    max_drift_us: u64,
    /// Audio dropped to keep the input from running ahead of the session clock, microseconds.
    dropped_us: u64,
}

/// One packet of Opus is 20 ms.
const PACKET_US: u64 = 20_000;
/// An input that is this far from the session clock is put right: behind, the voice is placed at the
/// time it was said; a little ahead, a chunk is left out.
const DRIFT_US: i64 = 60_000;
/// An input this far ahead of the session clock starts again (never back past the last packet).
const RESYNC_US: i64 = 300_000;

impl AudioPipe {
    const fn new() -> Self {
        Self {
            codec: None,
            anchor_us: None,
            packets: 0,
            taken_us: 0,
            level: 0.0,
            off: None,
            last_pts: None,
            drift_us: 0,
            max_drift_us: 0,
            dropped_us: 0,
        }
    }

    fn drain(&mut self, writer: &Handle) {
        let (Some(codec), Some(anchor)) = (self.codec.as_mut(), self.anchor_us) else {
            return;
        };
        while let Some(packet) = codec.poll() {
            let pts = anchor + self.packets * PACKET_US;
            self.packets += 1;
            self.last_pts = Some(pts);
            writer.send(Cmd::Audio { pts_us: pts, packet });
        }
    }
}

/// A recording in progress.
struct Session {
    name: String,
    dir: PathBuf,
    clock: Arc<dyn Clock>,
    writer: Handle,
    events: EventSink,
    idle: IdleWatch,
    rate: RateMeter,
    screen: ScreenCapture,
    screen_pipe: VideoPipe,
    camera_pipe: VideoPipe,
    audio_pipe: AudioPipe,
    /// What the writer was last seen to have written; only a new failure is told.
    told_failures: usize,
    told_manifest_error: bool,
    told_ending: bool,
    told_disk_low: bool,
    told_disk_unknown: bool,
    last_disk_check_us: u64,
    last_rate_us: u64,
    free_bytes: Option<u64>,
    /// The time recorded before the current run, microseconds.
    recorded_before_us: u64,
    /// When the current run began (while recording).
    run_start_us: u64,
}

/// The recorder.
pub struct Recorder {
    settings: RecorderSettings,
    deps: Deps,
    state: State,
    session: Option<Session>,
}

/// How often the disk is asked, microseconds.
const DISK_CHECK_US: u64 = 5_000_000;
/// How often the writer's byte count goes into the rate meter, microseconds.
const RATE_SAMPLE_US: u64 = 1_000_000;

impl Recorder {
    /// A recorder with these settings, doing nothing yet.
    #[must_use]
    pub fn new(settings: RecorderSettings, deps: Deps) -> Self {
        Self {
            settings,
            deps,
            state: State::Idle,
            session: None,
        }
    }

    /// Where it stands.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// The settings it runs with.
    #[must_use]
    pub const fn settings(&self) -> &RecorderSettings {
        &self.settings
    }

    /// Whether a session is going (recording or paused).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// The session clock's time now; `None` when no session is going.
    #[must_use]
    pub fn now_us(&self) -> Option<u64> {
        self.session.as_ref().map(|s| s.clock.now_us())
    }

    /// The folder of the session that is going.
    #[must_use]
    pub fn session_dir(&self) -> Option<&Path> {
        self.session.as_ref().map(|s| s.dir.as_path())
    }

    // ---- start and stop ----

    /// Starts a session.
    ///
    /// # Errors
    /// Why it did not start: another is going, the disk is too full, the folder could not be made, or
    /// every track is switched off.
    pub fn start(&mut self, req: StartRequest) -> Result<Started, StartError> {
        if self.session.is_some() {
            return Err(StartError::Busy);
        }
        let s = &self.settings;
        if !s.screen && !s.camera && !s.microphone {
            return Err(StartError::NothingToRecord);
        }
        if let DiskState::Full = disk_state((self.deps.free_bytes)(&req.recordings_dir), s.min_free_bytes) {
            let free = (self.deps.free_bytes)(&req.recordings_dir).unwrap_or(0);
            return Err(StartError::DiskFull { free, min_free: s.min_free_bytes });
        }
        let unix_ms = (self.deps.unix_ms)();
        let clock = (self.deps.make_clock)(unix_ms);
        let civil = CivilTime::from_unix_ms(unix_ms);
        let (name, dir) = new_session_dir(&req.recordings_dir, &civil.folder_name()).map_err(|e| StartError::Io(e.to_string()))?;
        let io = |e: std::io::Error| StartError::Io(e.to_string());

        let manifest = Manifest::begin(
            req.app.clone(),
            &name,
            req.workspace.clone(),
            ClockInfo {
                unit: String::from("us"),
                origin_unix_ms: unix_ms,
                origin_iso8601: civil.iso8601(),
                utc_offset_minutes: req.utc_offset_minutes,
            },
            req.host.clone(),
        );
        let transcript = open_lines(&dir.join(TRANSCRIPT_FILE)).map_err(io)?;
        let events_file = open_lines(&dir.join(EVENTS_FILE)).map_err(io)?;
        let (events, rx) = event_channel(Arc::clone(&clock), Caps::default());
        let log = EventLog::new(events_file, rx, 0);
        let writer = Writer::new(
            WriterConfig {
                segment_us: s.segment_us,
                created_unix_s: unix_ms / 1_000,
            },
            manifest,
            Box::new(DiskStore::new(dir.clone())),
            Some(Box::new(transcript)),
        );
        let handle = spawn(writer, Box::new(ManifestFile::new(&dir)), Some(log));
        let now = clock.now_us();
        let mut session = Session {
            name: name.clone(),
            dir: dir.clone(),
            clock: Arc::clone(&clock),
            writer: handle,
            events,
            idle: IdleWatch::new(s.idle_pause_us, now),
            rate: RateMeter::new(120_000_000),
            screen: ScreenCapture::new(Pace::at_fps(s.screen_fps)),
            screen_pipe: VideoPipe::new("screen"),
            camera_pipe: VideoPipe::new("camera"),
            audio_pipe: AudioPipe::new(),
            told_failures: 0,
            told_manifest_error: false,
            told_ending: false,
            told_disk_low: false,
            told_disk_unknown: false,
            last_disk_check_us: now,
            last_rate_us: now,
            free_bytes: (self.deps.free_bytes)(&dir),
            recorded_before_us: 0,
            run_start_us: now,
        };
        // The tracks that are not asked for, or are refused, are off for the whole session, said in the manifest.
        if !s.screen {
            session.screen_pipe.off = Some(String::from("switched off in the settings"));
        }
        if !s.camera {
            session.camera_pipe.off = Some(String::from("switched off in the settings"));
        } else if let Perm::Denied(why) = &req.camera {
            session.camera_pipe.off = Some(why.clone());
            let track = off_track("camera", s.quality.camera().width, s.quality.camera().height, req.camera_device.clone(), TrackStatus::Denied, why);
            session.writer.send(Cmd::AddTrack { dir: "camera", kind: Kind::Video, track: Box::new(track) });
        }
        if !s.microphone {
            session.audio_pipe.off = Some(String::from("switched off in the settings"));
        } else if let Perm::Denied(why) = &req.microphone {
            session.audio_pipe.off = Some(why.clone());
            let track = off_track("audio", 0, 0, req.microphone_device.clone(), TrackStatus::Denied, why);
            session.writer.send(Cmd::AddTrack { dir: "audio", kind: Kind::Opus, track: Box::new(track) });
        } else {
            match self.deps.factory.audio(48_000, 1, AUDIO_KBPS) {
                Ok(codec) => {
                    let track = audio_track(&codec.name(), req.microphone_device.clone());
                    session.writer.send(Cmd::AddTrack { dir: "audio", kind: Kind::Opus, track: Box::new(track) });
                    session.audio_pipe.codec = Some(codec);
                }
                Err(why) => {
                    session.audio_pipe.off = Some(why.clone());
                    let track = off_track("audio", 0, 0, req.microphone_device.clone(), TrackStatus::Failed, &why);
                    session.writer.send(Cmd::AddTrack { dir: "audio", kind: Kind::Opus, track: Box::new(track) });
                }
            }
        }
        let fields = format!(
            "{{\"app\":{},\"version\":{},\"workspace\":{}}}",
            crate::recorder_events::json_string(&req.app.name),
            crate::recorder_events::json_string(&req.app.version),
            crate::recorder_events::json_string(&req.workspace.root)
        );
        let _ = session.events.event_at(now, "session.start", &fields);
        session.writer.send(Cmd::Checkpoint);
        self.session = Some(session);
        self.state = State::Recording;
        Ok(Started { name, dir })
    }

    /// Stops the session: the encoders are flushed, every file is finished and the manifest says
    /// `complete`. Waits (a few seconds at most) for the writer.
    pub fn stop(&mut self, reason: &str) -> Option<Finished> {
        let mut session = self.session.take()?;
        let now = session.clock.now_us();
        let recording = self.state == State::Recording;
        self.state = State::Idle;
        if recording {
            Self::flush_pipes(&mut session);
        }
        let recorded = session.recorded_before_us + if recording { now.saturating_sub(session.run_start_us) } else { 0 };
        let _ = session.events.event_at(now, "session.stop", &format!("{{\"reason\":{}}}", crate::recorder_events::json_string(reason)));
        session.events.flush();
        session.writer.send(Cmd::Stop { t_us: now, ended_unix_ms: (self.deps.unix_ms)() });
        let clean = session.writer.join(Duration::from_secs(10));
        let media_bytes = session.writer.status().media_bytes;
        Some(Finished {
            name: session.name.clone(),
            dir: session.dir.clone(),
            recorded_us: recorded,
            media_bytes,
            clean,
        })
    }

    fn flush_pipes(session: &mut Session) {
        session.screen_pipe.flush(&session.writer);
        session.camera_pipe.flush(&session.writer);
        session.audio_pipe.drain(&session.writer);
    }

    // ---- pause and resume ----

    /// Pauses the recording (the button, the hotkey, or by the recorder's own rules).
    pub fn pause(&mut self, reason: PauseReason) -> Vec<Notice> {
        if self.state != State::Recording {
            return Vec::new();
        }
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        let now = session.clock.now_us();
        Self::flush_pipes(session);
        session.writer.send(Cmd::RunEnd { t_us: now, reason: reason.word() });
        session.recorded_before_us += now.saturating_sub(session.run_start_us);
        let _ = session.events.event_at(now, "session.pause", &format!("{{\"reason\":\"{}\"}}", reason.word()));
        self.state = State::Paused(reason);
        vec![Notice::Paused(reason)]
    }

    /// Goes on after a pause. A recording paused for the disk goes on only if there is room again.
    pub fn resume(&mut self, why: &str) -> Vec<Notice> {
        let State::Paused(reason) = self.state else {
            return Vec::new();
        };
        let min_free = self.settings.min_free_bytes;
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        if reason == PauseReason::Quota {
            let free = (self.deps.free_bytes)(&session.dir);
            if !may_resume_after_quota(free, min_free) {
                return Vec::new();
            }
        }
        let now = session.clock.now_us();
        session.writer.send(Cmd::RunStart { t_us: now });
        session.run_start_us = now;
        session.idle.reset(now);
        session.audio_pipe.anchor_us = None;
        session.audio_pipe.packets = 0;
        session.audio_pipe.taken_us = 0;
        session.screen.mark_changed(now);
        session.screen.request_keyframe();
        session.camera_pipe.last_key_us = None;
        let _ = session.events.event_at(now, "session.resume", &format!("{{\"reason\":{}}}", crate::recorder_events::json_string(why)));
        self.state = State::Recording;
        vec![Notice::Resumed(reason)]
    }

    /// The pause hotkey: pauses a recording, resumes a paused one.
    pub fn toggle_pause(&mut self) -> Vec<Notice> {
        match self.state {
            State::Recording => self.pause(PauseReason::User),
            State::Paused(_) => self.resume("user"),
            State::Idle => Vec::new(),
        }
    }

    /// Something happened (input, speech): keeps the recording awake and wakes an idle pause.
    pub fn activity(&mut self) -> Vec<Notice> {
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        let now = session.clock.now_us();
        if session.idle.activity(now) == IdleChange::Resumed && self.state == State::Paused(PauseReason::Idle) {
            return self.resume("activity");
        }
        Vec::new()
    }

    // ---- what comes in ----

    /// An event of the review, a JSON object as text (see `recorder_events.rs`). Nothing when no
    /// session is going.
    ///
    /// # Errors
    /// Why it was dropped.
    pub fn event(&self, kind: &str, fields: &str) -> Result<(), Dropped> {
        match &self.session {
            Some(s) if self.state != State::Idle => s.events.event(kind, fields),
            _ => Ok(()),
        }
    }

    /// A clone of the sink events go to, for a worker that records from another thread.
    #[must_use]
    pub fn event_sink(&self) -> Option<EventSink> {
        self.session.as_ref().map(|s| s.events.clone())
    }

    /// A line of the transcript.
    pub fn transcript(&mut self, line: TranscriptLine) {
        if let (Some(s), true) = (self.session.as_ref(), self.state != State::Idle) {
            s.writer.send(Cmd::Transcript(Box::new(line)));
        }
    }

    /// The engine showed a frame of the window.
    pub fn tap(&mut self, frame: &TapFrame<'_>) -> Option<Tapped> {
        if self.state != State::Recording {
            return None;
        }
        let session = self.session.as_mut()?;
        if session.screen_pipe.off.is_some() {
            return None;
        }
        let now = session.clock.now_us();
        let outcome = session.screen.tap(frame, now);
        if matches!(outcome, Tapped::Resized | Tapped::Copied { rows: 1.. }) {
            session.idle.activity(now);
        }
        Some(outcome)
    }

    /// A whole frame of the window from the system's own capture (ScreenCaptureKit), for a window on
    /// the GPU path that the engine's tap cannot see; `scale` is the display's device pixels to a point.
    pub fn screen_frame(&mut self, frame: RawFrame, scale: f32) -> Option<Tapped> {
        if self.state != State::Recording {
            return None;
        }
        let session = self.session.as_mut()?;
        if session.screen_pipe.off.is_some() {
            return None;
        }
        let now = session.clock.now_us();
        let outcome = session.screen.push_external(frame, scale, now);
        if matches!(outcome, Tapped::Resized | Tapped::Copied { rows: 1.. }) {
            session.idle.activity(now);
        }
        Some(outcome)
    }

    /// A picture of the camera.
    pub fn camera_frame(&mut self, frame: RawFrame) {
        if self.state != State::Recording {
            return;
        }
        let preset = self.settings.quality.camera();
        let Some(session) = self.session.as_mut() else { return };
        if session.camera_pipe.off.is_some() {
            return;
        }
        if frame.width % 2 != 0 || frame.height % 2 != 0 {
            session.camera_pipe.off = Some(format!("the camera's pictures are {}x{}, an odd size no H.264 file can hold", frame.width, frame.height));
            return;
        }
        let now = session.clock.now_us();
        let size = (frame.width, frame.height);
        if session.camera_pipe.codec.is_none() || session.camera_pipe.size != size {
            Self::reopen(&*self.deps.factory, session, true, size, preset.kbps, "camera", preset.fps);
            session.camera_pipe.last_key_us = None;
        }
        let key = session.camera_pipe.key_due(now);
        Self::submit(session, true, frame, key, now);
    }

    /// Voice from the microphone: interleaved samples of `channels` channels at `sample_rate` Hz.
    pub fn audio_chunk(&mut self, samples: &[f32], sample_rate: u32, channels: u16) {
        let Some(session) = self.session.as_mut() else { return };
        let pipe = &mut session.audio_pipe;
        if self.state != State::Recording || pipe.off.is_some() || sample_rate == 0 {
            // Level only: the meter shows the voice while paused too.
            if self.state != State::Idle {
                let peak = samples.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
                pipe.level = pipe.level.max(peak.min(1.0));
            }
            return;
        }
        let now = session.clock.now_us();
        let frames = samples.len() as u64 / u64::from(channels.max(1));
        let chunk_us = frames * 1_000_000 / u64::from(sample_rate);
        let peak = samples.iter().fold(0.0_f32, |m, v| m.max(v.abs())).min(1.0);
        pipe.level = pipe.level.max(peak);
        // The chunk ended now: its first sample is a chunk's length earlier.
        let first = now.saturating_sub(chunk_us);
        match pipe.anchor_us {
            None => {
                pipe.anchor_us = Some(first);
                pipe.packets = 0;
                pipe.taken_us = 0;
                pipe.drift_us = 0;
            }
            Some(anchor) => {
                // Where the audio's own count says this chunk starts, against where the session clock
                // says it does: the two clocks of one machine drift apart by tens of parts per million,
                // a few hundred milliseconds in an hour. The voice is kept within [`DRIFT_US`] of the
                // session clock, or the exported video's voice and picture part.
                let expected = anchor + pipe.taken_us;
                let drift = i64::try_from(first).unwrap_or(i64::MAX) - i64::try_from(expected).unwrap_or(i64::MAX);
                let floor = pipe.last_pts.map_or(0, |p| p + PACKET_US);
                if drift >= DRIFT_US || drift <= -RESYNC_US {
                    // The input is behind (or far ahead): what is said next is placed where it is said
                    // (never before the last packet: the timeline does not go back).
                    pipe.anchor_us = Some(first.max(floor));
                    pipe.packets = 0;
                    pipe.taken_us = 0;
                    pipe.drift_us = i64::try_from(first).unwrap_or(i64::MAX) - i64::try_from(first.max(floor)).unwrap_or(i64::MAX);
                } else if drift <= -DRIFT_US {
                    // A little ahead: this chunk is left out (20 ms now and then, not heard) and the
                    // session clock catches up with the voice.
                    pipe.dropped_us += chunk_us;
                    pipe.drift_us = drift;
                    pipe.max_drift_us = pipe.max_drift_us.max(drift.unsigned_abs());
                    return;
                } else {
                    pipe.drift_us = drift;
                }
                pipe.max_drift_us = pipe.max_drift_us.max(pipe.drift_us.unsigned_abs());
            }
        }
        if let Some(codec) = pipe.codec.as_mut() {
            codec.submit(samples);
            pipe.taken_us += chunk_us;
            pipe.drain(&session.writer);
        }
    }

    fn reopen(factory: &dyn CodecFactory, session: &mut Session, camera: bool, size: (u32, u32), kbps: u32, dir: &'static str, fps: u32) {
        let scale = if camera { 1.0 } else { session.screen.scale() };
        let source = if camera {
            "camera"
        } else if session.screen.is_external() {
            "screencapturekit"
        } else {
            "frame-tap"
        };
        let writer = &session.writer;
        let pipe = if camera { &mut session.camera_pipe } else { &mut session.screen_pipe };
        pipe.flush(writer);
        pipe.codec = None;
        pipe.pending.clear();
        match factory.video(size.0, size.1, kbps) {
            Ok(codec) => {
                let first = pipe.size == (0, 0);
                let meta = video_track(&codec.name(), codec.hardware(), size, kbps, fps, source, scale);
                pipe.size = size;
                pipe.codec = Some(codec);
                if first {
                    writer.send(Cmd::AddTrack { dir, kind: Kind::Video, track: Box::new(meta) });
                }
            }
            Err(why) => {
                pipe.off = Some(why.clone());
                if pipe.size == (0, 0) {
                    let meta = off_track(dir, size.0, size.1, None, TrackStatus::Failed, &why);
                    writer.send(Cmd::AddTrack { dir, kind: Kind::Video, track: Box::new(meta) });
                } else {
                    writer.send(Cmd::TrackStatus { dir, status: TrackStatus::Failed, note: Some(why) });
                }
            }
        }
    }

    fn submit(session: &mut Session, camera: bool, frame: RawFrame, keyframe: bool, now: u64) -> bool {
        let writer = &session.writer;
        let pipe = if camera { &mut session.camera_pipe } else { &mut session.screen_pipe };
        let Some(codec) = pipe.codec.as_mut() else { return false };
        let taken = codec.submit(frame, keyframe);
        if taken {
            pipe.pending.push_back(now);
        } else {
            pipe.dropped += 1;
        }
        pipe.drain(writer);
        taken
    }

    // ---- the timer ----

    /// Called about every 100 ms: sends the screen's frames that are due, polls the encoders, and looks
    /// after the disk, the idle time, the session's length and the writer.
    pub fn tick(&mut self) -> Vec<Notice> {
        let mut notices = Vec::new();
        let Some(session) = self.session.as_mut() else {
            return notices;
        };
        let now = session.clock.now_us();
        if self.state == State::Recording {
            // The screen.
            if let Some((frame, key)) = session.screen.take_frame(now) {
                let size = (frame.width, frame.height);
                if session.screen_pipe.off.is_none() && (session.screen_pipe.codec.is_none() || session.screen_pipe.size != size) {
                    let kbps = self.settings.quality.screen_kbps(size.0, size.1);
                    Self::reopen(&*self.deps.factory, session, false, size, kbps, "screen", self.settings.screen_fps);
                    if let Some(why) = &session.screen_pipe.off {
                        notices.push(Notice::TrackOff { track: "screen", why: why.clone() });
                    }
                }
                if session.screen_pipe.off.is_none() {
                    let key = key || session.screen_pipe.pending.is_empty();
                    if !Self::submit(session, false, frame, key, now) && session.screen.last_taken() == Due::Change {
                        session.screen.retry();
                    }
                }
            }
            session.screen_pipe.drain(&session.writer);
            session.camera_pipe.drain(&session.writer);
            session.audio_pipe.drain(&session.writer);
            // The idle watch.
            if let IdleChange::Paused { idle_us } = session.idle.check(now) {
                let _ = session.events.event_at(now, "session.idle_pause", &format!("{{\"idle_ms\":{}}}", idle_us / 1_000));
                notices.extend(self.pause(PauseReason::Idle));
            }
        }
        let Some(session) = self.session.as_mut() else {
            return notices;
        };
        // The voice's meter falls, whatever the recording is doing.
        session.audio_pipe.level *= 0.85;
        // The rate.
        if now.saturating_sub(session.last_rate_us) >= RATE_SAMPLE_US {
            session.last_rate_us = now;
            session.rate.record(now, session.writer.status().media_bytes);
        }
        // The disk.
        if now.saturating_sub(session.last_disk_check_us) >= DISK_CHECK_US {
            session.last_disk_check_us = now;
            session.free_bytes = (self.deps.free_bytes)(&session.dir);
            let min_free = self.settings.min_free_bytes;
            match disk_state(session.free_bytes, min_free) {
                DiskState::Unknown => {
                    if !session.told_disk_unknown {
                        session.told_disk_unknown = true;
                        notices.push(Notice::DiskUnknown);
                    }
                }
                DiskState::Low => {
                    if !session.told_disk_low {
                        session.told_disk_low = true;
                        notices.push(Notice::DiskLow { free: session.free_bytes.unwrap_or(0), min_free });
                    }
                }
                DiskState::Full if self.state == State::Recording => {
                    let _ = session.events.event_at(
                        now,
                        "session.quota_pause",
                        &format!("{{\"free_bytes\":{},\"min_free_bytes\":{}}}", session.free_bytes.unwrap_or(0), min_free),
                    );
                    notices.extend(self.pause(PauseReason::Quota));
                }
                DiskState::Full => {}
                DiskState::Fine => session.told_disk_low = false,
            }
            if self.state == State::Paused(PauseReason::Quota) {
                notices.extend(self.resume("room"));
            }
        }
        let Some(session) = self.session.as_mut() else {
            return notices;
        };
        // The length.
        if self.state == State::Recording {
            let recorded = session.recorded_before_us + now.saturating_sub(session.run_start_us);
            match length_state(recorded, self.settings.max_session_us) {
                LengthState::Ending { left_us } if !session.told_ending => {
                    session.told_ending = true;
                    notices.push(Notice::Ending { left_us });
                }
                LengthState::Over => {
                    notices.push(Notice::LengthReached);
                    let _ = self.stop("length");
                    return notices;
                }
                _ => {}
            }
        }
        // The writer.
        let status: Status = session.writer.status();
        for (track, why) in status.failures.iter().skip(session.told_failures) {
            notices.push(Notice::TrackFailed { track: track.clone(), why: why.clone() });
        }
        session.told_failures = status.failures.len();
        if let Some(e) = status.manifest_error {
            if !session.told_manifest_error {
                session.told_manifest_error = true;
                notices.push(Notice::ManifestError(e));
            }
        } else {
            session.told_manifest_error = false;
        }
        notices
    }

    // ---- what the indicator shows ----

    /// What the always-visible indicator shows now.
    #[must_use]
    pub fn indicator(&self) -> Indicator {
        let Some(s) = self.session.as_ref() else {
            return Indicator {
                state: State::Idle,
                recorded_us: 0,
                level: 0.0,
                media_bytes: 0,
                free_bytes: None,
                hours_left: None,
                disk: DiskState::Unknown,
                dropped_frames: 0,
                problems: Vec::new(),
            };
        };
        let now = s.clock.now_us();
        let status = s.writer.status();
        let recorded = s.recorded_before_us + if self.state == State::Recording { now.saturating_sub(s.run_start_us) } else { 0 };
        let per_hour = s
            .rate
            .bytes_per_hour()
            .unwrap_or_else(|| expected_bytes_per_hour(&self.settings, s.screen.window_size()));
        let mut problems: Vec<(String, String)> = Vec::new();
        for (name, off) in [("screen", &s.screen_pipe.off), ("camera", &s.camera_pipe.off), ("audio", &s.audio_pipe.off)] {
            if let Some(why) = off {
                problems.push((name.to_string(), why.clone()));
            }
        }
        problems.extend(status.failures.iter().cloned());
        Indicator {
            state: self.state,
            recorded_us: recorded,
            level: s.audio_pipe.level,
            media_bytes: status.media_bytes,
            free_bytes: s.free_bytes,
            hours_left: hours_left(s.free_bytes, self.settings.min_free_bytes, per_hour),
            disk: disk_state(s.free_bytes, self.settings.min_free_bytes),
            dropped_frames: s.screen_pipe.dropped + s.camera_pipe.dropped,
            problems,
        }
    }

    /// How far the voice's own clock is from the session clock: `(at the last chunk, the most so far)`
    /// in microseconds, positive when the input is behind; `None` before any voice.
    #[must_use]
    pub fn audio_drift(&self) -> Option<(i64, u64)> {
        let p = &self.session.as_ref()?.audio_pipe;
        p.anchor_us.map(|_| (p.drift_us, p.max_drift_us))
    }

    /// What the screen capture has done.
    #[must_use]
    pub fn screen_stats(&self) -> Option<crate::recorder_screen::ScreenStats> {
        self.session.as_ref().map(|s| s.screen.stats())
    }

    /// What the writer has done.
    #[must_use]
    pub fn writer_status(&self) -> Option<Status> {
        self.session.as_ref().map(|s| s.writer.status())
    }
}

fn video_track(encoder: &str, hardware: bool, size: (u32, u32), kbps: u32, fps: u32, source: &str, scale: f32) -> Track {
    Track {
        source: source.to_string(),
        device: None,
        codec: String::from("h264"),
        encoder: encoder.to_string(),
        hardware,
        width: size.0,
        height: size.1,
        scale,
        fps,
        vfr: source != "camera",
        bitrate_kbps: kbps,
        sample_rate: 0,
        channels: 0,
        timescale: crate::recorder_mp4::VIDEO_TIMESCALE,
        status: TrackStatus::Recording,
        note: None,
        segments: Vec::new(),
    }
}

fn audio_track(encoder: &str, device: Option<String>) -> Track {
    Track {
        source: String::from("microphone"),
        device,
        codec: String::from("opus"),
        encoder: encoder.to_string(),
        hardware: false,
        width: 0,
        height: 0,
        scale: 1.0,
        fps: 0,
        vfr: false,
        bitrate_kbps: AUDIO_KBPS,
        sample_rate: 48_000,
        channels: 1,
        timescale: crate::recorder_mp4::OPUS_TIMESCALE,
        status: TrackStatus::Recording,
        note: None,
        segments: Vec::new(),
    }
}

/// A track that is in the manifest to say why it is not recording.
fn off_track(dir: &str, width: u32, height: u32, device: Option<String>, status: TrackStatus, why: &str) -> Track {
    let video = dir != "audio";
    let mut t = if video {
        video_track("none", false, (width.max(2), height.max(2)), 0, 0, if dir == "camera" { "camera" } else { "frame-tap" }, 1.0)
    } else {
        audio_track("none", device.clone())
    };
    t.device = device;
    t.status = status;
    t.note = Some(why.to_string());
    t
}

#[cfg(test)]
#[path = "recorder_test.rs"]
mod recorder_test;
