//! The writing side of a recording: the tracks' segment files, the manifest, the transcript, and the
//! runs, behind one owner ([`Writer`]) that a single thread drives ([`spawn`]).
//!
//! The recorder's main thread never touches a file. It sends the encoded samples, the transcript
//! lines and the changes of state to the writer as [`Cmd`] messages; the writer appends them to the
//! segment files ([`crate::recorder_track`]), keeps the manifest true, and rewrites it atomically
//! whenever a segment opens or closes or the state changes. The rules are in [`Writer`], which is
//! plain and tested with files in memory; the thread around it ([`spawn`]) only moves messages.
//!
//! A pause closes every open segment (a file holds only time that was recorded, so a hold of the
//! last picture never spans the pause) and the resume opens new ones at keyframes. A track whose
//! file fails is marked `failed` in the manifest with the system's words and the others go on.
//! Stopping finishes every file and the manifest (`state: complete`); a channel that closes
//! without a stop (the app was torn down) finishes them the same way, at the last time it heard of.

use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

use crate::{
    recorder_events::EventLog,
    recorder_manifest::{
        FileInfo, Manifest, Run, SessionState, Track, TrackStatus, TranscriptLine, TRACK_DIRS,
    },
    recorder_mp4::Colour,
    recorder_track::{Change, Kind, SegmentStore, TrackWriter},
};

/// What the writer is told once, when the session begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterConfig {
    /// How long a segment file runs, microseconds.
    pub segment_us: u64,
    /// The wall-clock time the files' headers say, seconds since 1970 UTC.
    pub created_unix_s: u64,
}

/// What the writer says back, for the recorder's log and indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// A track's file failed: the words, and the track is `failed` from now on.
    TrackFailed { track: &'static str, why: String },
    /// A segment file was opened.
    SegmentOpened { track: &'static str, file: String, start_us: u64 },
    /// A segment file was closed properly.
    SegmentClosed { track: &'static str, file: String, end_us: u64 },
}

/// The owner of a session's files.
pub struct Writer {
    cfg: WriterConfig,
    manifest: Manifest,
    store: Box<dyn SegmentStore + Send>,
    tracks: BTreeMap<&'static str, TrackWriter>,
    transcript: Option<Box<dyn Write + Send>>,
    transcript_lines: u64,
    transcript_bytes: u64,
    dirty: bool,
}

impl Writer {
    /// A writer for a session whose manifest is `manifest` (it has begun: one open run), writing
    /// its segments through `store` and its transcript lines to `transcript`.
    #[must_use]
    pub fn new(
        cfg: WriterConfig,
        manifest: Manifest,
        store: Box<dyn SegmentStore + Send>,
        transcript: Option<Box<dyn Write + Send>>,
    ) -> Self {
        Self {
            cfg,
            manifest,
            store,
            tracks: BTreeMap::new(),
            transcript,
            transcript_lines: 0,
            transcript_bytes: 0,
            dirty: true,
        }
    }

    /// Adds the track in folder `dir` (`screen`, `camera` or `audio`), described by `track`.
    ///
    /// # Errors
    /// The folder is none of the three, or the track is there already.
    pub fn add_track(&mut self, dir: &str, kind: Kind, track: Track) -> Result<(), String> {
        let Some(&name) = TRACK_DIRS.iter().find(|d| **d == dir) else {
            return Err(format!("{dir:?} is none of the track folders"));
        };
        if self.tracks.contains_key(name) {
            return Err(format!("the {name} track is there already"));
        }
        let slot = match name {
            "screen" => &mut self.manifest.tracks.screen,
            "camera" => &mut self.manifest.tracks.camera,
            _ => &mut self.manifest.tracks.audio,
        };
        // The screen is RGB made YCbCr as Rec. 709; a camera as HD is 709 and below it 601.
        let colour = if name == "screen" || track.height >= 720 { Colour::bt709() } else { Colour::bt601() };
        *slot = Some(track);
        let writer = TrackWriter::new(name, kind, self.cfg.segment_us, 1, self.cfg.created_unix_s).with_default_colour(colour);
        self.tracks.insert(name, writer);
        self.dirty = true;
        Ok(())
    }

    fn track_mut(&mut self, dir: &str) -> Option<&mut Track> {
        match dir {
            "screen" => self.manifest.tracks.screen.as_mut(),
            "camera" => self.manifest.tracks.camera.as_mut(),
            "audio" => self.manifest.tracks.audio.as_mut(),
            _ => None,
        }
    }

    /// Takes one encoded picture (Annex-B) of the track `dir` seen at `pts_us`.
    pub fn video(&mut self, dir: &'static str, pts_us: u64, annexb: &[u8]) -> Vec<Note> {
        let Some(tw) = self.tracks.get_mut(dir) else {
            return Vec::new();
        };
        let result = tw.push_video(self.store.as_mut(), pts_us, annexb);
        self.after(dir, result)
    }

    /// Takes one Opus packet whose first sample is at `pts_us`.
    pub fn audio(&mut self, pts_us: u64, packet: Vec<u8>) -> Vec<Note> {
        let Some(tw) = self.tracks.get_mut("audio") else {
            return Vec::new();
        };
        let result = tw.push_audio(self.store.as_mut(), pts_us, packet);
        self.after("audio", result)
    }

    fn after(&mut self, dir: &'static str, result: Result<Vec<Change>, crate::recorder_track::TrackError>) -> Vec<Note> {
        let mut notes = Vec::new();
        match result {
            Ok(changes) => {
                for c in changes {
                    self.dirty = true;
                    notes.push(match c {
                        Change::Opened(s) => Note::SegmentOpened { track: dir, file: s.file, start_us: s.start_us },
                        Change::Closed(s) => Note::SegmentClosed { track: dir, file: s.file, end_us: s.end_us.unwrap_or(s.start_us) },
                    });
                }
            }
            Err(e) => {
                let why = e.to_string();
                if let Some(t) = self.track_mut(dir) {
                    t.status = TrackStatus::Failed;
                    t.note = Some(why.clone());
                }
                self.dirty = true;
                notes.push(Note::TrackFailed { track: dir, why });
            }
        }
        // A track that broke while writing a fragment reports it on its next sample.
        if let Some(why) = self.tracks.get(dir).and_then(|t| t.failure()).map(str::to_string) {
            let already = self.track_mut(dir).is_some_and(|t| t.status == TrackStatus::Failed);
            if !already {
                if let Some(t) = self.track_mut(dir) {
                    t.status = TrackStatus::Failed;
                    t.note = Some(why.clone());
                }
                self.dirty = true;
                notes.push(Note::TrackFailed { track: dir, why });
            }
        }
        notes
    }

    /// Sets what the manifest says about a track: it was denied, switched off, or stopped.
    pub fn track_status(&mut self, dir: &str, status: TrackStatus, note: Option<String>) {
        if let Some(t) = self.track_mut(dir) {
            t.status = status;
            t.note = note;
            self.dirty = true;
        }
    }

    /// Adds one line to the transcript file and counts it.
    pub fn transcript(&mut self, line: &TranscriptLine) {
        let Some(out) = self.transcript.as_mut() else {
            return;
        };
        let mut text = line.to_json();
        text.push('\n');
        if out.write_all(text.as_bytes()).is_ok() {
            self.transcript_lines += 1;
            self.transcript_bytes += text.len() as u64;
            self.dirty = true;
        }
    }

    /// Flushes the transcript file.
    pub fn flush(&mut self) {
        if let Some(out) = self.transcript.as_mut() {
            let _ = out.flush();
        }
    }

    /// The recording paused at `t_us`: every open segment is closed there and the run ends.
    pub fn run_end(&mut self, t_us: u64, reason: &str) -> Vec<Note> {
        let notes = self.finish_tracks(t_us);
        if let Some(run) = self.manifest.runs.last_mut().filter(|r| r.end_us.is_none()) {
            run.end_us = Some(t_us);
            run.end_reason = Some(reason.to_string());
        }
        self.dirty = true;
        notes
    }

    /// The recording went on at `t_us`: a new run begins.
    pub fn run_start(&mut self, t_us: u64) {
        if self.manifest.runs.last().is_none_or(|r| r.end_us.is_some()) {
            self.manifest.runs.push(Run { start_us: t_us, end_us: None, end_reason: None });
            self.dirty = true;
        }
    }

    /// Adds a diagnostic to the manifest.
    pub fn diagnostic(&mut self, t_us: u64, level: &str, text: &str) {
        self.manifest.note(t_us, level, text);
        self.dirty = true;
    }

    /// Says what the events file holds.
    pub fn set_events_info(&mut self, info: FileInfo) {
        if self.manifest.events.as_ref() != Some(&info) {
            self.manifest.events = Some(info);
            self.dirty = true;
        }
    }

    fn finish_tracks(&mut self, t_us: u64) -> Vec<Note> {
        let mut notes = Vec::new();
        let names: Vec<&'static str> = self.tracks.keys().copied().collect();
        for name in names {
            let result = match self.tracks.get_mut(name) {
                Some(tw) => tw.finish(t_us),
                None => continue,
            };
            notes.extend(self.after(name, result));
        }
        notes
    }

    /// The recording stopped at `t_us` (`ended_unix_ms` on the wall clock, when it is known): every
    /// file is finished and the manifest says `complete`.
    pub fn stop(&mut self, t_us: u64, ended_unix_ms: Option<u64>) -> Vec<Note> {
        let notes = self.run_end(t_us, "stop");
        for t in [&mut self.manifest.tracks.screen, &mut self.manifest.tracks.camera, &mut self.manifest.tracks.audio]
            .into_iter()
            .flatten()
        {
            if t.status == TrackStatus::Recording {
                t.status = TrackStatus::Stopped;
            }
        }
        self.flush();
        self.manifest.state = SessionState::Complete;
        self.manifest.ended_unix_ms = ended_unix_ms;
        self.dirty = true;
        notes
    }

    /// Bytes in every segment file so far.
    #[must_use]
    pub fn media_bytes(&self) -> u64 {
        self.tracks.values().map(TrackWriter::bytes).sum()
    }

    /// Bytes in the segments of one track so far.
    #[must_use]
    pub fn track_bytes(&self, dir: &str) -> u64 {
        self.tracks.get(dir).map_or(0, TrackWriter::bytes)
    }

    /// The manifest as it stands: the segments of every track as its writer has them now, the
    /// transcript's size, the sizes a segment shares with its track left out.
    pub fn manifest(&mut self) -> &Manifest {
        for (name, tw) in &self.tracks {
            let mut segments = tw.segments();
            let slot = match *name {
                "screen" => self.manifest.tracks.screen.as_mut(),
                "camera" => self.manifest.tracks.camera.as_mut(),
                _ => self.manifest.tracks.audio.as_mut(),
            };
            if let Some(track) = slot {
                for s in &mut segments {
                    if (s.width, s.height) == (track.width, track.height) {
                        s.width = 0;
                        s.height = 0;
                    }
                }
                track.segments = segments;
            }
        }
        if self.transcript.is_some() {
            self.manifest.transcript = Some(FileInfo {
                file: crate::recorder_manifest::TRANSCRIPT_FILE.to_string(),
                count: self.transcript_lines,
                bytes: self.transcript_bytes,
                dropped: 0,
            });
        }
        &self.manifest
    }

    /// Whether the manifest changed since it was last written; clears the mark.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

// ---------------------------------------------------------------------------
// The thread
// ---------------------------------------------------------------------------

/// What the main thread tells the writer.
pub enum Cmd {
    AddTrack { dir: &'static str, kind: Kind, track: Box<Track> },
    Video { dir: &'static str, pts_us: u64, annexb: Vec<u8> },
    Audio { pts_us: u64, packet: Vec<u8> },
    TrackStatus { dir: &'static str, status: TrackStatus, note: Option<String> },
    Transcript(Box<TranscriptLine>),
    RunEnd { t_us: u64, reason: &'static str },
    RunStart { t_us: u64 },
    Diagnostic { t_us: u64, level: &'static str, text: String },
    /// Write the manifest now.
    Checkpoint,
    Stop { t_us: u64, ended_unix_ms: u64 },
}

/// What the writer shows the main thread, without a round trip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Bytes in the segment files.
    pub media_bytes: u64,
    /// Bytes per track.
    pub screen_bytes: u64,
    pub camera_bytes: u64,
    pub audio_bytes: u64,
    /// Lines in the events and the transcript files.
    pub event_lines: u64,
    pub transcript_lines: u64,
    /// Tracks that failed, with the words.
    pub failures: Vec<(String, String)>,
    /// The manifest could not be written: the words.
    pub manifest_error: Option<String>,
    /// The writer has finished (stopped).
    pub done: bool,
}

/// Where the manifest goes: atomically, to a file (or to memory in a test).
pub trait ManifestSink: Send {
    /// Replaces the manifest with `text`.
    ///
    /// # Errors
    /// The manifest could not be written.
    fn write(&mut self, text: &str) -> std::io::Result<()>;
}

/// The writer thread's handle.
pub struct Handle {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    join: Option<JoinHandle<()>>,
}

impl Handle {
    /// Sends a command; `false` when the writer is gone.
    pub fn send(&self, cmd: Cmd) -> bool {
        self.tx.send(cmd).is_ok()
    }

    /// What the writer has done, as of its last look.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Waits for the writer to finish (after a [`Cmd::Stop`]) for at most `timeout`; whether it did.
    pub fn join(&mut self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if self.status().done {
                if let Some(j) = self.join.take() {
                    let _ = j.join();
                }
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

impl Drop for Handle {
    /// A recorder that is dropped without a stop: the writer finishes the files at the last time
    /// it heard of (its channel closes), and this waits a moment for it.
    fn drop(&mut self) {
        let (dead, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.tx, dead));
        let _ = self.join(Duration::from_secs(3));
    }
}

/// Starts the writer thread. `events` is the events file's log (its sink was given to the recorder),
/// `manifest_sink` the atomic manifest file.
#[must_use]
pub fn spawn<W: Write + Send + 'static>(
    mut writer: Writer,
    mut sink: Box<dyn ManifestSink>,
    mut events: Option<EventLog<W>>,
) -> Handle {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let status = Arc::new(Mutex::new(Status::default()));
    let shared = Arc::clone(&status);
    let join = std::thread::Builder::new()
        .name(String::from("azreview-recorder-writer"))
        .spawn(move || {
            let mut last_t = 0_u64;
            let mut stopped = false;
            let publish = |writer: &mut Writer, events: &Option<EventLog<W>>, manifest_error: Option<String>, done: bool, failures: Vec<(String, String)>| {
                if let Ok(mut s) = shared.lock() {
                    s.media_bytes = writer.media_bytes();
                    s.screen_bytes = writer.track_bytes("screen");
                    s.camera_bytes = writer.track_bytes("camera");
                    s.audio_bytes = writer.track_bytes("audio");
                    s.event_lines = events.as_ref().map_or(0, EventLog::lines);
                    s.transcript_lines = writer.transcript_lines;
                    s.failures = failures;
                    s.manifest_error = manifest_error;
                    s.done = done;
                }
            };
            let mut failures: Vec<(String, String)> = Vec::new();
            let mut manifest_error: Option<String> = None;
            loop {
                let cmd = rx.recv_timeout(Duration::from_millis(100));
                let mut checkpoint = false;
                match cmd {
                    Ok(cmd) => {
                        let notes = match cmd {
                            Cmd::AddTrack { dir, kind, track } => {
                                if let Err(e) = writer.add_track(dir, kind, *track) {
                                    writer.diagnostic(last_t, "error", &e);
                                }
                                Vec::new()
                            }
                            Cmd::Video { dir, pts_us, annexb } => {
                                last_t = last_t.max(pts_us);
                                writer.video(dir, pts_us, &annexb)
                            }
                            Cmd::Audio { pts_us, packet } => {
                                last_t = last_t.max(pts_us);
                                writer.audio(pts_us, packet)
                            }
                            Cmd::TrackStatus { dir, status, note } => {
                                writer.track_status(dir, status, note);
                                Vec::new()
                            }
                            Cmd::Transcript(line) => {
                                last_t = last_t.max(line.t1_us);
                                writer.transcript(&line);
                                Vec::new()
                            }
                            Cmd::RunEnd { t_us, reason } => {
                                last_t = last_t.max(t_us);
                                writer.run_end(t_us, reason)
                            }
                            Cmd::RunStart { t_us } => {
                                last_t = last_t.max(t_us);
                                writer.run_start(t_us);
                                Vec::new()
                            }
                            Cmd::Diagnostic { t_us, level, text } => {
                                writer.diagnostic(t_us, level, &text);
                                Vec::new()
                            }
                            Cmd::Checkpoint => {
                                checkpoint = true;
                                Vec::new()
                            }
                            Cmd::Stop { t_us, ended_unix_ms } => {
                                last_t = last_t.max(t_us);
                                stopped = true;
                                writer.stop(t_us, Some(ended_unix_ms))
                            }
                        };
                        for n in notes {
                            if let Note::TrackFailed { track, why } = n {
                                failures.push((track.to_string(), why));
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        // The recorder went away without a stop: finish at the last time heard of.
                        if !stopped {
                            stopped = true;
                            writer.diagnostic(last_t, "warn", "the recorder was dropped without a stop: the files were finished where they were");
                            let _ = writer.stop(last_t, None);
                        }
                    }
                }
                if let Some(log) = events.as_mut() {
                    let _ = log.pump();
                    let lines = log.lines();
                    writer.set_events_info(FileInfo {
                        file: crate::recorder_manifest::EVENTS_FILE.to_string(),
                        count: lines,
                        bytes: log.bytes(),
                        dropped: 0,
                    });
                }
                if checkpoint || stopped || writer.take_dirty() {
                    let text = writer.manifest().to_json();
                    manifest_error = sink.write(&text).err().map(|e| e.to_string());
                }
                publish(&mut writer, &events, manifest_error.clone(), false, failures.clone());
                if stopped {
                    if let Some(log) = events.as_mut() {
                        let _ = log.pump();
                        let _ = log.flush();
                    }
                    publish(&mut writer, &events, manifest_error.clone(), true, failures.clone());
                    break;
                }
            }
        })
        .ok();
    Handle { tx, status, join }
}

#[cfg(test)]
#[path = "recorder_writer_test.rs"]
mod recorder_writer_test;
