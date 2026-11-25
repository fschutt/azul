//! The cut list (EDL, schema 1): what an export is asked to make, and the generator that needs no
//! judgement (`identity_edl`).
//!
//! A cut list says WHICH parts of a recorded session go into the video, in which order, and how they
//! look: the screen as the picture, the camera as a picture-in-picture in a corner, the microphone as the
//! sound, a zoom into a rectangle of the screen, captions with the time of every word, chapters. It is
//! the one thing between a recording and a video: the deterministic generators here (and, later, the
//! one that removes filler words, and the editing agent) all write it, and the renderer
//! (`recorder_export.rs`) turns it into ffmpeg commands. `RECORDER.md` describes every field.
//!
//! Times are microseconds on the SESSION clock. A segment never spans the gap between two runs: what was
//! not recorded cannot be shown. Everything is read strictly: a field this schema does not name is an
//! error that names it, so a generator's mistake is found when its output is read, not when a video
//! looks wrong.

use serde::{Deserialize, Serialize};

use crate::recorder_manifest::Manifest;

/// The version of the schema this build writes and reads.
pub const SCHEMA: u32 = 1;
/// The longest video an export may make, microseconds.
pub const MAX_OUTPUT_US: u64 = 24 * 3_600_000_000;
/// The most segments a cut list may have.
pub const MAX_SEGMENTS: usize = 20_000;

/// The size, rate and quality of the video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
}

impl Default for Output {
    /// 1920 x 1080 at 30 frames a second, 8 Mbit/s of picture and 160 kbit/s of sound.
    fn default() -> Self {
        Self { width: 1920, height: 1080, fps: 30, video_kbps: 8_000, audio_kbps: 160 }
    }
}

/// How the screen fills the picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenStyle {
    /// `contain` (all of it, bars where the shapes differ) or `cover` (fills, crops).
    pub fit: String,
}

/// Where the camera's picture-in-picture sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraStyle {
    /// `top_left`, `top_right`, `bottom_left` or `bottom_right`.
    pub corner: String,
    /// Its width as a fraction of the picture's, 0.05 to 0.6.
    pub scale: f64,
    /// Its distance from the corner's two edges, pixels of the picture.
    pub margin: u32,
    /// `rect` (a rounded circle is for a later version).
    pub shape: String,
}

/// How the picture is composed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    /// The colour behind the screen, `#rrggbb`.
    pub background: String,
    pub screen: ScreenStyle,
    pub camera: CameraStyle,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: String::from("#101010"),
            screen: ScreenStyle { fit: String::from("contain") },
            camera: CameraStyle { corner: String::from("bottom_right"), scale: 0.22, margin: 24, shape: String::from("rect") },
        }
    }
}

/// A rectangle of the screen track, in the track's own pixels, that fills the picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zoom {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// The time the picture takes to go from the previous segment's rectangle to this one.
    pub ease_ms: u32,
}

/// What happens to the sound of a segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCut {
    pub gain_db: f64,
    /// The sound of this segment and the one before overlap by this long (the picture cuts).
    pub crossfade_ms: u32,
}

impl Default for AudioCut {
    fn default() -> Self {
        Self { gain_db: 0.0, crossfade_ms: 0 }
    }
}

/// One word of a caption with the time it was said (session clock).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionWord {
    pub w: String,
    pub t0_us: u64,
    pub t1_us: u64,
}

/// The words of a segment, for captions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caption {
    pub text: String,
    pub words: Vec<CaptionWord>,
}

/// One part of the session in the video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdlSegment {
    /// A name for the segment, unique in the list (an intermediate file is named after it).
    pub id: String,
    pub in_us: u64,
    pub out_us: u64,
    /// The camera's picture-in-picture is shown.
    pub camera: bool,
    /// Playing speed: 1.0 is as recorded.
    pub speed: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Zoom>,
    pub audio: AudioCut,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl EdlSegment {
    /// How long the segment is in the video, microseconds.
    #[must_use]
    pub fn duration_us(&self) -> u64 {
        let recorded = self.out_us.saturating_sub(self.in_us) as f64;
        let speed = if self.speed.is_finite() && self.speed > 0.0 { self.speed } else { 1.0 };
        (recorded / speed).round() as u64
    }
}

/// A chapter of the video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chapter {
    /// On the video's own timeline, microseconds from its start.
    pub t_us: u64,
    pub title: String,
}

/// Whether captions are burned in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Captions {
    pub burn: bool,
    pub font_px: u32,
}

impl Default for Captions {
    fn default() -> Self {
        Self { burn: false, font_px: 42 }
    }
}

/// Who made the cut list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    pub name: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The cut list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edl {
    pub schema: u32,
    /// The session's folder name.
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub output: Output,
    pub style: Style,
    pub segments: Vec<EdlSegment>,
    pub chapters: Vec<Chapter>,
    pub captions: Captions,
    pub generator: Generator,
}

/// Why a cut list could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdlError {
    /// Not JSON of this shape (an unknown field is named here).
    Json(String),
    /// It is, but cannot be right for this session.
    Invalid(Vec<String>),
}

impl std::fmt::Display for EdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "the cut list is not valid: {e}"),
            Self::Invalid(p) => write!(f, "the cut list cannot be right for this session: {}", p.join("; ")),
        }
    }
}

impl std::error::Error for EdlError {}

impl Edl {
    /// The cut list as text: pretty-printed, a newline at the end.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// Reads a cut list and checks it against the session it is for.
    ///
    /// # Errors
    /// The text is not a cut list, or says what cannot be for this session.
    pub fn parse(text: &str, manifest: &Manifest) -> Result<Self, EdlError> {
        let edl: Self = serde_json::from_str(text).map_err(|e| EdlError::Json(e.to_string()))?;
        let problems = edl.problems(manifest);
        if problems.is_empty() {
            Ok(edl)
        } else {
            Err(EdlError::Invalid(problems))
        }
    }

    /// The length of the video, microseconds.
    #[must_use]
    pub fn duration_us(&self) -> u64 {
        self.segments.iter().map(EdlSegment::duration_us).sum()
    }

    /// Everything wrong with the cut list for this session, in words; empty when it is sound.
    #[must_use]
    pub fn problems(&self, manifest: &Manifest) -> Vec<String> {
        let mut out = Vec::new();
        if self.schema != SCHEMA {
            out.push(format!("schema {} is not the one this build reads ({SCHEMA})", self.schema));
        }
        if self.session != manifest.session {
            out.push(format!("it is for the session {:?}, not {:?}", self.session, manifest.session));
        }
        let o = &self.output;
        if o.width < 16 || o.height < 16 || o.width > 7_680 || o.height > 4_320 || o.width % 2 != 0 || o.height % 2 != 0 {
            out.push(format!("the video's size {}x{} must be even, from 16x16 to 7680x4320", o.width, o.height));
        }
        if !(1..=120).contains(&o.fps) {
            out.push(format!("{} frames a second is not between 1 and 120", o.fps));
        }
        if !(100..=100_000).contains(&o.video_kbps) || !(16..=512).contains(&o.audio_kbps) {
            out.push(String::from("the bitrates are out of range (100 to 100000 kbit/s of picture, 16 to 512 of sound)"));
        }
        if !is_colour(&self.style.background) {
            out.push(format!("the background {:?} is not #rrggbb", self.style.background));
        }
        if !matches!(self.style.screen.fit.as_str(), "contain" | "cover") {
            out.push(format!("the screen's fit {:?} is not contain or cover", self.style.screen.fit));
        }
        let c = &self.style.camera;
        if !matches!(c.corner.as_str(), "top_left" | "top_right" | "bottom_left" | "bottom_right") {
            out.push(format!("the camera's corner {:?} is not one of the four corners", c.corner));
        }
        if !(0.05..=0.6).contains(&c.scale) || c.shape != "rect" {
            out.push(String::from("the camera's scale must be 0.05 to 0.6 and its shape rect"));
        }
        if self.segments.is_empty() {
            out.push(String::from("there are no segments: nothing to show"));
        }
        if self.segments.len() > MAX_SEGMENTS {
            out.push(format!("{} segments (the most is {MAX_SEGMENTS})", self.segments.len()));
            return out;
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, s) in self.segments.iter().enumerate() {
            self.segment_problems(i, s, manifest, &mut out);
            if !ids.insert(s.id.clone()) {
                out.push(format!("segment {i}: the id {:?} is used twice", s.id));
            }
        }
        if self.duration_us() > MAX_OUTPUT_US {
            out.push(String::from("the video would be longer than 24 hours"));
        }
        let total = self.duration_us();
        let mut previous = 0;
        for (i, ch) in self.chapters.iter().enumerate() {
            if ch.t_us > total || ch.t_us < previous {
                out.push(format!("chapter {i} ({:?}) is out of order or past the end of the video", ch.title));
            }
            previous = ch.t_us;
        }
        out
    }

    fn segment_problems(&self, i: usize, s: &EdlSegment, m: &Manifest, out: &mut Vec<String>) {
        let name = format!("segment {i} ({:?})", s.id);
        if s.id.is_empty() || !s.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') || s.id.len() > 64 {
            out.push(format!("{name}: an id is 1 to 64 letters, digits, - and _"));
        }
        if s.out_us <= s.in_us {
            out.push(format!("{name}: it ends at {} before it begins at {}", s.out_us, s.in_us));
            return;
        }
        // Inside one run: what was not recorded cannot be shown.
        let end = m.end_us();
        let inside = m.runs.iter().any(|r| r.start_us <= s.in_us && s.out_us <= r.end_us.unwrap_or(end));
        if !inside {
            out.push(format!("{name}: {}..{} is not inside one run of the recording", s.in_us, s.out_us));
        }
        if !s.speed.is_finite() || !(0.25..=8.0).contains(&s.speed) {
            out.push(format!("{name}: the speed {} is not between 0.25 and 8", s.speed));
        }
        if let Some(z) = &s.zoom {
            let (sw, sh) = m.tracks.screen.as_ref().map_or((0, 0), |t| (t.width, t.height));
            if z.w == 0 || z.h == 0 || z.x.saturating_add(z.w) > sw || z.y.saturating_add(z.h) > sh {
                out.push(format!("{name}: the zoom {}x{} at {},{} is not inside the screen track's {sw}x{sh}", z.w, z.h, z.x, z.y));
            }
        }
        if !s.audio.gain_db.is_finite() || s.audio.gain_db.abs() > 40.0 {
            out.push(format!("{name}: a gain of {} dB is out of range", s.audio.gain_db));
        }
        if u64::from(s.audio.crossfade_ms) * 1_000 > s.duration_us() / 2 {
            out.push(format!("{name}: a crossfade of {} ms is longer than half the segment", s.audio.crossfade_ms));
        }
        if let Some(c) = &s.caption {
            for (j, w) in c.words.iter().enumerate() {
                if w.t1_us < w.t0_us || w.t0_us < s.in_us || w.t1_us > s.out_us {
                    out.push(format!("{name}: caption word {j} ({:?}) is not inside the segment", w.w));
                }
            }
        }
    }
}

fn is_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The files the review opened and when: `(t_us, file)` from the `nav.file` events of a session, in order,
/// each change of file once (a file opened again at once is not a new chapter).
#[must_use]
pub fn file_changes(events_jsonl: &str) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    for line in events_jsonl.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["k"] != "nav.file" {
            continue;
        }
        let (Some(t), Some(file)) = (v["t"].as_u64(), v["file"].as_str()) else { continue };
        if out.last().is_none_or(|(_, f)| f != file) {
            out.push((t, file.to_string()));
        }
    }
    out
}

/// The cut list that needs no judgement: the recording as it was, one segment for each run, the camera
/// in the corner where it was recorded, the sound as it was, a chapter for each file the review opened.
/// `files` are the file changes of the session ([`file_changes`]).
#[must_use]
pub fn identity_edl(manifest: &Manifest, files: &[(u64, String)]) -> Edl {
    let end = manifest.end_us();
    let has_camera = manifest.tracks.camera.as_ref().is_some_and(|t| !t.segments.is_empty());
    let mut segments = Vec::new();
    for (i, run) in manifest.runs.iter().enumerate() {
        let out_us = run.end_us.unwrap_or(end);
        if out_us <= run.start_us {
            continue;
        }
        segments.push(EdlSegment {
            id: format!("run{}", i + 1),
            in_us: run.start_us,
            out_us,
            camera: has_camera,
            speed: 1.0,
            zoom: None,
            audio: AudioCut::default(),
            caption: None,
            title: None,
        });
    }
    // The chapters: each file change that falls in a segment, on the video's own timeline.
    let mut chapters = Vec::new();
    let mut offset = 0_u64;
    for s in &segments {
        for (t, file) in files {
            if *t >= s.in_us && *t < s.out_us {
                chapters.push(Chapter { t_us: offset + (*t - s.in_us), title: file.clone() });
            }
        }
        offset += s.duration_us();
    }
    // The picture is the screen's shape when there is one, up to full HD.
    let mut output = Output::default();
    if let Some(screen) = manifest.tracks.screen.as_ref() {
        let (w, h) = (screen.width.max(2), screen.height.max(2));
        let scale = (1920.0 / f64::from(w)).min(1080.0 / f64::from(h)).min(1.0);
        output.width = ((f64::from(w) * scale / 2.0).round() as u32 * 2).max(16);
        output.height = ((f64::from(h) * scale / 2.0).round() as u32 * 2).max(16);
    }
    Edl {
        schema: SCHEMA,
        session: manifest.session.clone(),
        title: Some(manifest.workspace.name.clone()),
        output,
        style: Style::default(),
        segments,
        chapters,
        captions: Captions::default(),
        generator: Generator { name: String::from("identity"), version: 1, notes: None },
    }
}

#[cfg(test)]
#[path = "recorder_edl_test.rs"]
mod recorder_edl_test;
