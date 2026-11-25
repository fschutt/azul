//! The files of a recorded session and what the manifest says about them (schema 1).
//!
//! ```text
//! <data root>/review/recordings/<date-time>/
//!     manifest.json        the clock, the tracks and their segments, the runs, the diagnostics
//!     events.jsonl         what the review did, one JSON object a line (recorder_events.rs)
//!     transcript.jsonl     what was said, with the time of every word
//!     screen/0001.mp4 ...  H.264, AzReview's own frames, a new frame when the screen changed
//!     camera/0001.mp4 ...  H.264, the camera
//!     audio/0001.mp4  ...  Opus, the microphone
//! ```
//!
//! Every time in these files is microseconds on the session clock (`recorder_clock.rs`): the
//! manifest says where that clock's origin is on the wall clock. A track is a list of SEGMENT
//! files (a few minutes each), each a fragmented MP4 that starts at a keyframe and has its own
//! start time on the session clock, so a recording that was killed loses at most the last
//! seconds, and an exporter seeks into the one segment it needs.
//!
//! The manifest is rewritten (atomically) whenever a segment opens or closes and when the
//! recording starts, pauses and stops; reading it ([`Manifest::parse`]) checks it strictly
//! ([`Manifest::problems`]), because an editing agent and the exporter act on what it says.
//! `RECORDER.md` describes every field.

use serde::{Deserialize, Serialize};

/// The version of the schema this build writes and reads.
pub const SCHEMA: u32 = 1;
/// The manifest's file name.
pub const MANIFEST_FILE: &str = "manifest.json";
/// The events' file name.
pub const EVENTS_FILE: &str = "events.jsonl";
/// The transcript's file name.
pub const TRANSCRIPT_FILE: &str = "transcript.jsonl";
/// The folders of the three tracks.
pub static TRACK_DIRS: [&str; 3] = ["screen", "camera", "audio"];
/// The longest list of diagnostics a manifest keeps.
pub const MAX_DIAGNOSTICS: usize = 500;

/// How a session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// The recorder is writing it, or was killed while it did and has not looked at it since.
    Recording,
    /// It was stopped properly.
    Complete,
    /// It was found cut off and its files were cut back to the last whole fragment.
    Recovered,
}

/// How a track stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackStatus {
    /// It is being written.
    Recording,
    /// It ended with the session (or was switched off for the rest of it).
    Stopped,
    /// It was not asked for.
    Off,
    /// The system refused the device (camera and microphone permissions).
    Denied,
    /// It broke: `note` says how.
    Failed,
}

/// Which app made the recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// What was being reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// The folder (a path on the machine that recorded).
    pub root: String,
    /// Its name, for a title.
    pub name: String,
}

/// Where the session clock's origin is on the wall clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockInfo {
    /// Always `"us"`: microseconds.
    pub unit: String,
    /// The origin, milliseconds since 1970-01-01T00:00:00Z.
    pub origin_unix_ms: u64,
    /// The same, as text.
    pub origin_iso8601: String,
    /// What the machine's local time was ahead of UTC at the origin, minutes.
    pub utc_offset_minutes: i32,
}

/// The machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
}

/// One segment file of a track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// The file, relative to the session folder (`screen/0001.mp4`).
    pub file: String,
    /// The session-clock time of the segment's first sample.
    pub start_us: u64,
    /// The session-clock time of the end of its last sample; absent while it is being written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_us: Option<u64>,
    /// The file's size.
    pub bytes: u64,
    /// Samples (video frames, audio packets) in it.
    pub samples: u64,
    /// It was closed properly.
    pub complete: bool,
    /// It was found cut off and cut back.
    #[serde(default, skip_serializing_if = "is_false")]
    pub recovered: bool,
    /// The picture's size in this segment (video), when it differs from the track's: the window
    /// was resized, which starts a new segment. 0: the track's.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height: u32,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
fn is_false(v: &bool) -> bool {
    !*v
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// One track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Where its pictures or sound come from: `frame-tap` (AzReview's own frames),
    /// `screencapturekit`, `camera`, `microphone`, `mock`.
    pub source: String,
    /// The device's name, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// `h264` or `opus`.
    pub codec: String,
    /// What encodes it (`VideoToolbox`, `AudioToolbox`).
    pub encoder: String,
    /// The encoder runs in hardware.
    pub hardware: bool,
    /// The picture's size in pixels; 0 for sound.
    pub width: u32,
    pub height: u32,
    /// Device pixels to a point at the time of recording (the screen track: 2.0 on a Retina display).
    pub scale: f32,
    /// The frame rate the encoder was told; a screen track is variable-rate, frames come when the
    /// screen changed.
    pub fps: u32,
    pub vfr: bool,
    /// The encoder's target, kbit/s.
    pub bitrate_kbps: u32,
    /// Sound: samples a second a channel, and channels; 0 for pictures.
    pub sample_rate: u32,
    pub channels: u8,
    /// The files' ticks a second.
    pub timescale: u32,
    pub status: TrackStatus,
    /// Why the track is `off`, `denied` or `failed`, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub segments: Vec<Segment>,
}

impl Track {
    /// The time the track's first sample is at, on the session clock.
    #[must_use]
    pub fn start_us(&self) -> Option<u64> {
        self.segments.first().map(|s| s.start_us)
    }

    /// The time the track's last sample ends, when it is known.
    #[must_use]
    pub fn end_us(&self) -> Option<u64> {
        self.segments.iter().rev().find_map(|s| s.end_us)
    }

    /// Bytes in all the track's segments.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.segments.iter().map(|s| s.bytes).sum()
    }
}

/// The three tracks; one that was not asked for is absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Tracks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Track>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<Track>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<Track>,
}

impl Tracks {
    /// `(folder name, track)` for the tracks that exist.
    #[must_use]
    pub fn present(&self) -> Vec<(&'static str, &Track)> {
        [("screen", &self.screen), ("camera", &self.camera), ("audio", &self.audio)]
            .into_iter()
            .filter_map(|(name, t)| t.as_ref().map(|t| (name, t)))
            .collect()
    }
}

/// A stretch of the session in which the recorder was recording: between a start or a resume and a
/// pause or the stop. What lies between two runs was not recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub start_us: u64,
    /// Absent while the run goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_us: Option<u64>,
    /// Why it ended: `pause`, `idle`, `quota`, `stop`, `crash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
}

/// A file of lines beside the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub file: String,
    /// Lines written.
    pub count: u64,
    pub bytes: u64,
    /// Lines that were dropped over the caps (events only).
    #[serde(default)]
    pub dropped: u64,
}

/// Something the recorder wants a reader of the session to know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub t_us: u64,
    /// `info`, `warn` or `error`.
    pub level: String,
    pub text: String,
}

/// The manifest of a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`SCHEMA`].
    pub schema: u32,
    pub app: AppInfo,
    /// The session's folder name (`2026-11-26_10-00-00`).
    pub session: String,
    pub state: SessionState,
    pub workspace: Workspace,
    pub clock: ClockInfo,
    pub host: HostInfo,
    pub tracks: Tracks,
    pub runs: Vec<Run>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<FileInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<FileInfo>,
    /// When the recording stopped, milliseconds since 1970 UTC; absent while it goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_unix_ms: Option<u64>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// Why a manifest could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The text is not JSON of this shape.
    Json(String),
    /// It is, but what it says cannot be right.
    Invalid(Vec<String>),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "the manifest is not valid: {e}"),
            Self::Invalid(p) => write!(f, "the manifest says something that cannot be: {}", p.join("; ")),
        }
    }
}

impl std::error::Error for ManifestError {}

/// The name of segment `index` (from 1) of the track in folder `dir`: `screen/0001.mp4`.
#[must_use]
pub fn segment_file(dir: &str, index: u32) -> String {
    format!("{dir}/{index:04}.mp4")
}

/// Whether `path` is a plain relative path inside a session folder: no `..`, no absolute path, no
/// backslash, no empty part, no control character.
#[must_use]
pub fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

impl Manifest {
    /// A manifest for a session that begins now: no tracks yet, one run that has begun.
    #[must_use]
    pub fn begin(
        app: AppInfo,
        session: &str,
        workspace: Workspace,
        clock: ClockInfo,
        host: HostInfo,
    ) -> Self {
        Self {
            schema: SCHEMA,
            app,
            session: session.to_string(),
            state: SessionState::Recording,
            workspace,
            clock,
            host,
            tracks: Tracks::default(),
            runs: vec![Run {
                start_us: 0,
                end_us: None,
                end_reason: None,
            }],
            events: None,
            transcript: None,
            ended_unix_ms: None,
            diagnostics: Vec::new(),
        }
    }

    /// The manifest as the file holds it: pretty-printed, a newline at the end.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// Reads a manifest and checks it.
    ///
    /// # Errors
    /// The text is not a manifest, or says what cannot be.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(text).map_err(|e| ManifestError::Json(e.to_string()))?;
        let problems = manifest.problems();
        if problems.is_empty() {
            Ok(manifest)
        } else {
            Err(ManifestError::Invalid(problems))
        }
    }

    /// Everything wrong with what the manifest says, in words; empty when it is sound.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.schema != SCHEMA {
            out.push(format!("schema {} is not the one this build reads ({SCHEMA})", self.schema));
        }
        if !is_safe_relative(&self.session) || self.session.contains('/') {
            out.push(format!("the session name {:?} is not a plain folder name", self.session));
        }
        if self.clock.unit != "us" {
            out.push(format!("the clock's unit is {:?}, not \"us\"", self.clock.unit));
        }
        if self.diagnostics.len() > MAX_DIAGNOSTICS {
            out.push(format!("{} diagnostics (the most is {MAX_DIAGNOSTICS})", self.diagnostics.len()));
        }
        self.run_problems(&mut out);
        for (dir, track) in self.tracks.present() {
            track_problems(dir, track, &mut out);
        }
        for (what, info) in [("events", &self.events), ("transcript", &self.transcript)] {
            if let Some(info) = info {
                if !is_safe_relative(&info.file) {
                    out.push(format!("the {what} file {:?} is not a plain relative path", info.file));
                }
            }
        }
        out
    }

    fn run_problems(&self, out: &mut Vec<String>) {
        let mut previous_end = 0_u64;
        for (i, run) in self.runs.iter().enumerate() {
            if run.start_us < previous_end {
                out.push(format!("run {i} starts at {} before the run before it ends ({previous_end})", run.start_us));
            }
            match run.end_us {
                Some(end) if end < run.start_us => {
                    out.push(format!("run {i} ends at {end} before it starts ({})", run.start_us));
                }
                Some(end) => previous_end = end,
                None if i + 1 != self.runs.len() => out.push(format!("run {i} has no end but is not the last")),
                None => {}
            }
        }
    }

    /// The end of the session on its clock: the latest end of a run or a segment.
    #[must_use]
    pub fn end_us(&self) -> u64 {
        let runs = self.runs.iter().filter_map(|r| r.end_us);
        let segments = self.tracks.present().into_iter().filter_map(|(_, t)| t.end_us());
        runs.chain(segments).max().unwrap_or(0)
    }

    /// Bytes in every segment of every track.
    #[must_use]
    pub fn media_bytes(&self) -> u64 {
        self.tracks.present().into_iter().map(|(_, t)| t.bytes()).sum()
    }

    /// The time the session was recording: the length of its runs (an open run counts to
    /// `now_us`).
    #[must_use]
    pub fn recorded_us(&self, now_us: u64) -> u64 {
        self.runs
            .iter()
            .map(|r| r.end_us.unwrap_or(now_us).saturating_sub(r.start_us))
            .sum()
    }

    /// Adds a diagnostic, keeping the list under its cap (the oldest `info` goes first, then the
    /// oldest of anything).
    pub fn note(&mut self, t_us: u64, level: &str, text: &str) {
        if self.diagnostics.len() >= MAX_DIAGNOSTICS {
            let at = self.diagnostics.iter().position(|d| d.level == "info").unwrap_or(0);
            self.diagnostics.remove(at);
        }
        self.diagnostics.push(Diagnostic {
            t_us,
            level: level.to_string(),
            text: text.chars().take(500).collect(),
        });
    }
}

fn track_problems(dir: &str, track: &Track, out: &mut Vec<String>) {
    let video = dir != "audio";
    let (codec, name) = if video { ("h264", "video") } else { ("opus", "audio") };
    if track.codec != codec {
        out.push(format!("the {dir} track's codec is {:?}, a {name} track holds {codec}", track.codec));
    }
    if video && (track.width == 0 || track.height == 0) {
        out.push(format!("the {dir} track has no picture size"));
    }
    if !video && (track.sample_rate == 0 || !(1..=2).contains(&track.channels)) {
        out.push(format!(
            "the {dir} track has {} Hz x {} channels: it must be one or two channels at a rate",
            track.sample_rate, track.channels
        ));
    }
    if track.timescale == 0 {
        out.push(format!("the {dir} track has no timescale"));
    }
    if track.bitrate_kbps > 200_000 {
        out.push(format!("the {dir} track's bitrate of {} kbit/s cannot be", track.bitrate_kbps));
    }
    let mut previous_end: Option<u64> = None;
    let mut seen = std::collections::BTreeSet::new();
    for (i, seg) in track.segments.iter().enumerate() {
        if !is_safe_relative(&seg.file) || !seg.file.starts_with(&format!("{dir}/")) {
            out.push(format!("segment {i} of {dir}: {:?} is not a file in the {dir} folder", seg.file));
        }
        if !seen.insert(seg.file.clone()) {
            out.push(format!("segment {i} of {dir}: {:?} is listed twice", seg.file));
        }
        if let Some(prev) = previous_end {
            if seg.start_us < prev {
                out.push(format!("segment {i} of {dir} starts at {} before the one before it ends ({prev})", seg.start_us));
            }
        }
        match seg.end_us {
            Some(end) if end < seg.start_us => {
                out.push(format!("segment {i} of {dir} ends at {end} before it starts ({})", seg.start_us));
            }
            Some(end) => previous_end = Some(end),
            None if i + 1 != track.segments.len() => {
                out.push(format!("segment {i} of {dir} has no end but is not the last"));
            }
            None => {}
        }
    }
}

// ---------------------------------------------------------------------------
// The transcript
// ---------------------------------------------------------------------------

/// One word of the transcript with the time it was said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub w: String,
    pub t0_us: u64,
    pub t1_us: u64,
    /// How sure the recogniser was, 0 to 1, when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<f32>,
}

/// A stretch of speech: one line of `transcript.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptLine {
    pub t0_us: u64,
    pub t1_us: u64,
    pub text: String,
    /// A BCP 47 tag.
    pub lang: String,
    /// The recogniser (`native`, `whistle`, `mock`).
    pub engine: String,
    /// The conversation it belongs to, when it started one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conv: Option<u64>,
    pub words: Vec<Word>,
}

impl TranscriptLine {
    /// The line as the file holds it, without the newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// What is wrong with the line, in words; empty when it is sound.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.t1_us < self.t0_us {
            out.push(format!("the line ends at {} before it starts at {}", self.t1_us, self.t0_us));
        }
        let mut previous_start = self.t0_us;
        for (i, w) in self.words.iter().enumerate() {
            if w.t1_us < w.t0_us {
                out.push(format!("word {i} ({:?}) ends before it starts", w.w));
            }
            if w.t0_us < previous_start {
                out.push(format!("word {i} ({:?}) starts before the word before it", w.w));
            }
            if let Some(p) = w.p {
                if !(0.0..=1.0).contains(&p) {
                    out.push(format!("word {i} ({:?}) has a probability of {p}", w.w));
                }
            }
            previous_start = previous_start.max(w.t0_us);
        }
        out
    }
}

/// The lines of a transcript file. A line that cannot be read (the last one of a recording that
/// was killed is often cut off) is left out and named in the second value.
#[must_use]
pub fn parse_transcript(text: &str) -> (Vec<TranscriptLine>, Vec<String>) {
    let mut lines = Vec::new();
    let mut problems = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TranscriptLine>(raw) {
            Ok(line) => {
                let p = line.problems();
                if p.is_empty() {
                    lines.push(line);
                } else {
                    problems.push(format!("line {}: {}", i + 1, p.join("; ")));
                }
            }
            Err(e) => problems.push(format!("line {}: {e}", i + 1)),
        }
    }
    (lines, problems)
}

#[cfg(test)]
#[path = "recorder_manifest_test.rs"]
mod recorder_manifest_test;
