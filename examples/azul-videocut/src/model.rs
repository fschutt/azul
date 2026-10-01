//! The edit model: a project holds media items and one sequence of tracks
//! of clips. Every change of the sequence is an [`Edit`] applied through
//! [`Project::edit`], which keeps the sequence as it was before for
//! [`Project::undo`] (the command and a memento of what it changed - the
//! sequence is small, a copy is cheaper than an inverse per command and can
//! never drift from it).
//!
//! Times are whole FRAMES at the sequence's rate (`Sequence::fps`): a clip
//! plays `length` frames from `start`, showing its media from frame
//! `source_in` on. A media item's length is counted in sequence frames too,
//! so a 29.97 fps file in a 25 fps sequence is read by time, frame by frame
//! of the sequence.
//!
//! No azul types here: the model is plain data with serde, unit-tested
//! without a window.

use serde::{Deserialize, Serialize};

/// A frame index at the sequence's rate.
pub type Frame = i64;

/// The most edits kept for undo.
pub const HISTORY_LIMIT: usize = 200;

/// What a track holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    /// Pictures (V1, V2, ...): the top one wins.
    Video,
    /// Sound (A1, A2, ...).
    Audio,
}

/// A picture the app makes itself (the sample project, a title card, the
/// fallback when a machine has no H.264 encoder to make sample files).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// SMPTE-like colour bars with a moving marker.
    Bars,
    /// One colour.
    Matte { rgb: [u8; 3] },
    /// One colour with a light bar sweeping across it and the frame number
    /// as blocks: motion you can see while scrubbing.
    Sweep { rgb: [u8; 3] },
}

/// Where a media item's pictures come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediaSource {
    /// A file on this computer, referenced by its path.
    Path { path: String },
    /// A file in the project's folder (`videocut/<uuid>/media/<name>`), read
    /// through the Drive.
    Stored { key: String },
    /// A picture the app makes.
    Generated { pattern: Pattern },
}

/// One item of the media bin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    /// The project's id for it.
    pub id: u64,
    /// The bin's name ("pier.mp4").
    pub name: String,
    /// Where its pictures come from.
    pub source: MediaSource,
    /// Its length in sequence frames.
    pub frames: Frame,
    /// Its picture size.
    pub width: u32,
    pub height: u32,
    /// Its own frame rate (the file's), for the bin.
    pub fps: f64,
    /// "H.264", "generated", ...
    pub codec: String,
    /// It has pictures (goes on a video track).
    #[serde(default = "yes")]
    pub has_video: bool,
    /// It has sound (goes on an audio track).
    #[serde(default)]
    pub has_audio: bool,
}

fn yes() -> bool {
    true
}

impl MediaItem {
    /// A generated picture `frames` frames long at `width` x `height`.
    #[must_use]
    pub fn generated(name: &str, pattern: Pattern, frames: Frame, width: u32, height: u32) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            source: MediaSource::Generated { pattern },
            frames: frames.max(1),
            width,
            height,
            fps: 0.0,
            codec: String::from("generated"),
            has_video: true,
            has_audio: false,
        }
    }

    /// Made by the app (no file behind it).
    #[must_use]
    pub fn is_generated(&self) -> bool {
        matches!(self.source, MediaSource::Generated { .. })
    }
}

/// A clip's picture controls (Premiere's Motion and Opacity).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    /// Moves the picture right, in sequence pixels.
    pub x: f32,
    /// Moves the picture down, in sequence pixels.
    pub y: f32,
    /// 1.0: the picture fits the frame.
    pub scale: f32,
    /// 0.0 (invisible) .. 1.0.
    pub opacity: f32,
    /// The share of the picture cut away at each edge, 0.0 .. 1.0.
    pub crop_left: f32,
    pub crop_right: f32,
    pub crop_top: f32,
    pub crop_bottom: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            opacity: 1.0,
            crop_left: 0.0,
            crop_right: 0.0,
            crop_top: 0.0,
            crop_bottom: 0.0,
        }
    }
}

/// A transition at a clip's head, from the clip before it on its track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// The outgoing picture fades into the incoming one.
    CrossDissolve,
    /// The outgoing picture fades to black, the incoming one from black.
    DipToBlack,
}

/// A transition over a clip's first `frames` frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    pub frames: Frame,
}

/// A span of a track playing a span of a media item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    /// The media item it plays.
    pub media: u64,
    /// Where it starts on the timeline.
    pub start: Frame,
    /// The media frame it starts with.
    pub source_in: Frame,
    /// How many frames it plays (at least one).
    pub length: Frame,
    #[serde(default)]
    pub effects: Effects,
    #[serde(default)]
    pub transition: Option<Transition>,
    /// A disabled clip does not play (Premiere's Shift+E).
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Clip {
    /// One past its last frame.
    #[must_use]
    pub fn end(&self) -> Frame {
        self.start + self.length
    }

    /// Whether it plays timeline frame `f`.
    #[must_use]
    pub fn contains(&self, f: Frame) -> bool {
        self.start <= f && f < self.end()
    }

    /// The media frame it shows at timeline frame `f`.
    #[must_use]
    pub fn source_frame(&self, f: Frame) -> Frame {
        self.source_in + (f - self.start)
    }
}

/// A track: its clips in time order, never overlapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
    /// Video: hidden (the eye is off); audio: muted.
    #[serde(default)]
    pub hidden: bool,
    /// Locked: no edit touches it.
    #[serde(default)]
    pub locked: bool,
}

impl Track {
    fn new(id: u64, name: &str, kind: TrackKind) -> Self {
        Self {
            id,
            name: name.to_string(),
            kind,
            clips: Vec::new(),
            hidden: false,
            locked: false,
        }
    }

    /// The clip playing frame `f`.
    #[must_use]
    pub fn clip_at(&self, f: Frame) -> Option<&Clip> {
        self.clips.iter().find(|c| c.contains(f))
    }

    fn sort(&mut self) {
        self.clips.sort_by_key(|c| c.start);
    }
}

/// The sequence: its format and its tracks, V1 first (bottom), the audio
/// tracks after the video ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Frames per second.
    pub fps: u32,
    pub tracks: Vec<Track>,
}

impl Sequence {
    /// One past the last frame of the last clip; 0 when empty.
    #[must_use]
    pub fn end(&self) -> Frame {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(Clip::end))
            .max()
            .unwrap_or(0)
    }

    /// (track, index) of clip `id`.
    #[must_use]
    pub fn find_clip(&self, id: u64) -> Option<(usize, usize)> {
        self.tracks.iter().enumerate().find_map(|(t, track)| {
            track.clips.iter().position(|c| c.id == id).map(|i| (t, i))
        })
    }

    /// Clip `id`.
    #[must_use]
    pub fn clip(&self, id: u64) -> Option<&Clip> {
        self.find_clip(id).map(|(t, i)| &self.tracks[t].clips[i])
    }

    /// (media, media frame) shown on `track` at timeline frame `f`.
    #[must_use]
    pub fn source_frame_at(&self, track: usize, f: Frame) -> Option<(u64, Frame)> {
        let c = self.tracks.get(track)?.clip_at(f)?;
        Some((c.media, c.source_frame(f)))
    }

    /// The video tracks' indices, bottom (V1) first.
    #[must_use]
    pub fn video_tracks(&self) -> Vec<usize> {
        (0..self.tracks.len())
            .filter(|t| self.tracks[*t].kind == TrackKind::Video)
            .collect()
    }

    /// Every clip edge, ascending, each once (the playhead's Up / Down).
    #[must_use]
    pub fn edit_points(&self) -> Vec<Frame> {
        let mut points = vec![0];
        for t in &self.tracks {
            for c in &t.clips {
                points.push(c.start);
                points.push(c.end());
            }
        }
        points.sort_unstable();
        points.dedup();
        points
    }
}

/// The source monitor's marks on a media item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceMarks {
    pub media: u64,
    /// The first frame to use (I).
    pub mark_in: Option<Frame>,
    /// The last frame to use (O), inclusive.
    pub mark_out: Option<Frame>,
    /// The source monitor's playhead.
    pub position: Frame,
}

/// Which edge of a clip a trim moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

/// A change of the sequence: what the editor's tools and keys do.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Put `clip` at `at` on `track`, pushing everything from `at` on to the
    /// right (a clip spanning `at` is cut there first).
    Insert { track: usize, at: Frame, clip: Clip },
    /// Put `clip` at `at` on `track` over whatever lies there.
    Overwrite { track: usize, at: Frame, clip: Clip },
    /// Cut the clip under `at` in two - on one track, or on every unlocked
    /// track (`None`).
    Razor { track: Option<usize>, at: Frame },
    /// Remove a clip, leaving a gap.
    Lift { clip: u64 },
    /// Remove a clip and close the gap.
    RippleDelete { clip: u64 },
    /// Put a clip at `start` on `track` (over whatever lies there).
    Move { clip: u64, track: usize, start: Frame },
    /// Move a clip's in or out point to `at`, within its media and its
    /// neighbours.
    Trim { clip: u64, edge: Edge, at: Frame },
    /// Move a clip's in or out point and the clips after it with it.
    RippleTrim { clip: u64, edge: Edge, at: Frame },
    /// Play other frames of the media in the same place.
    Slip { clip: u64, delta: Frame },
    /// A clip's picture controls.
    SetEffects { clip: u64, effects: Effects },
    /// A clip's head transition.
    SetTransition { clip: u64, transition: Option<Transition> },
    /// A clip plays or not.
    SetEnabled { clip: u64, enabled: bool },
    /// A track's eye (video) or mute (audio).
    SetTrackHidden { track: usize, hidden: bool },
    /// A track's lock.
    SetTrackLocked { track: usize, locked: bool },
}

impl Edit {
    /// The edit's name for the Undo / Redo commands.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Edit::Insert { .. } => "Insert",
            Edit::Overwrite { .. } => "Overwrite",
            Edit::Razor { .. } => "Razor",
            Edit::Lift { .. } => "Lift",
            Edit::RippleDelete { .. } => "Ripple delete",
            Edit::Move { .. } => "Move",
            Edit::Trim { .. } => "Trim",
            Edit::RippleTrim { .. } => "Ripple trim",
            Edit::Slip { .. } => "Slip",
            Edit::SetEffects { .. } => "Effects",
            Edit::SetTransition { .. } => "Transition",
            Edit::SetEnabled { .. } => "Enable",
            Edit::SetTrackHidden { .. } => "Track visibility",
            Edit::SetTrackLocked { .. } => "Track lock",
        }
    }
}

/// Why an edit was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    NoSuchClip,
    NoSuchTrack,
    TrackLocked,
    /// A picture on an audio track, or sound on a video track.
    WrongTrackKind,
    /// The edit would change nothing (a cut on a cut).
    NothingToDo,
}

impl EditError {
    /// The message for the status bar.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            EditError::NoSuchClip => "That clip is gone.",
            EditError::NoSuchTrack => "That track does not exist.",
            EditError::TrackLocked => "The track is locked.",
            EditError::WrongTrackKind => "That clip cannot go on this kind of track.",
            EditError::NothingToDo => "Nothing to change.",
        }
    }
}

/// The undo and redo stacks: each entry is an edit's name and the sequence
/// before it (undo) or after it (redo).
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<(&'static str, Sequence)>,
    redo: Vec<(&'static str, Sequence)>,
}

/// The project: the media bin and the sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// The project's uuid: its folder `videocut/<id>/`.
    pub id: String,
    pub name: String,
    pub media: Vec<MediaItem>,
    pub sequence: Sequence,
    /// The next id for a media item, a track or a clip.
    pub next_id: u64,
    #[serde(skip)]
    history: History,
}

impl Project {
    /// An empty project: a `width` x `height` sequence at `fps` with V1..V3
    /// over A1..A3.
    #[must_use]
    pub fn create(id: String, name: String, width: u32, height: u32, fps: u32) -> Self {
        let tracks = vec![
            Track::new(1, "V1", TrackKind::Video),
            Track::new(2, "V2", TrackKind::Video),
            Track::new(3, "V3", TrackKind::Video),
            Track::new(4, "A1", TrackKind::Audio),
            Track::new(5, "A2", TrackKind::Audio),
            Track::new(6, "A3", TrackKind::Audio),
        ];
        Self {
            id,
            sequence: Sequence {
                name: format!("{name} - Sequence 1"),
                width,
                height,
                fps: fps.max(1),
                tracks,
            },
            name,
            media: Vec::new(),
            next_id: 7,
            history: History::default(),
        }
    }

    /// A fresh id.
    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Puts `item` in the bin under a fresh id and returns it.
    pub fn add_media(&mut self, mut item: MediaItem) -> u64 {
        item.id = self.new_id();
        let id = item.id;
        self.media.push(item);
        id
    }

    /// Media item `id`.
    #[must_use]
    pub fn media(&self, id: u64) -> Option<&MediaItem> {
        self.media.iter().find(|m| m.id == id)
    }

    /// A media item's length in frames (very long when it is unknown).
    fn media_frames(&self, id: u64) -> Frame {
        self.media(id).map_or(Frame::MAX / 4, |m| m.frames)
    }

    /// A new clip of `length` frames of `media` from its frame `source_in`.
    pub fn clip_from_media(&mut self, media: u64, source_in: Frame, length: Frame) -> Clip {
        Clip {
            id: self.new_id(),
            media,
            start: 0,
            source_in: source_in.max(0),
            length: length.max(1),
            effects: Effects::default(),
            transition: None,
            enabled: true,
        }
    }

    /// A new clip of the source monitor's marked range (the whole media
    /// without marks); `None` when the out mark lies before the in mark.
    pub fn clip_from_marks(&mut self, marks: &SourceMarks) -> Option<Clip> {
        let frames = self.media(marks.media)?.frames;
        let last = (frames - 1).max(0);
        let from = marks.mark_in.unwrap_or(0).clamp(0, last);
        let to = marks.mark_out.unwrap_or(last).clamp(0, last);
        if to < from {
            return None;
        }
        Some(self.clip_from_media(marks.media, from, to - from + 1))
    }

    /// Applies `edit`; an edit that changes the sequence can be undone. A
    /// refused edit leaves everything as it was.
    pub fn edit(&mut self, edit: Edit) -> Result<(), EditError> {
        let before = self.sequence.clone();
        let label = edit.label();
        match self.apply(edit) {
            Ok(()) => {
                if self.sequence != before {
                    self.history.undo.push((label, before));
                    if self.history.undo.len() > HISTORY_LIMIT {
                        self.history.undo.remove(0);
                    }
                    self.history.redo.clear();
                }
                Ok(())
            }
            Err(e) => {
                self.sequence = before;
                Err(e)
            }
        }
    }

    /// Undoes the last edit; `false` when there is none.
    pub fn undo(&mut self) -> bool {
        let Some((label, before)) = self.history.undo.pop() else {
            return false;
        };
        let now = core::mem::replace(&mut self.sequence, before);
        self.history.redo.push((label, now));
        true
    }

    /// Redoes the last undone edit; `false` when there is none.
    pub fn redo(&mut self) -> bool {
        let Some((label, after)) = self.history.redo.pop() else {
            return false;
        };
        let now = core::mem::replace(&mut self.sequence, after);
        self.history.undo.push((label, now));
        true
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Forgets every edit (a project as it was loaded or made).
    pub fn clear_history(&mut self) {
        self.history = History::default();
    }

    /// The name of the edit Undo would undo.
    #[must_use]
    pub fn undo_label(&self) -> Option<&'static str> {
        self.history.undo.last().map(|(l, _)| *l)
    }

    /// The name of the edit Redo would redo.
    #[must_use]
    pub fn redo_label(&self) -> Option<&'static str> {
        self.history.redo.last().map(|(l, _)| *l)
    }

    /// The project as `project.json`.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// A project from its `project.json`.
    pub fn from_json(json: &str) -> Result<Project, String> {
        serde_json::from_str(json).map_err(|e| format!("project.json: {e}"))
    }

    // ---- applying edits ----

    /// Track `track`, if it exists and is not locked.
    fn writable(&self, track: usize) -> Result<(), EditError> {
        match self.sequence.tracks.get(track) {
            None => Err(EditError::NoSuchTrack),
            Some(t) if t.locked => Err(EditError::TrackLocked),
            Some(_) => Ok(()),
        }
    }

    /// Whether `clip`'s media may go on `track`.
    fn fits(&self, track: usize, clip: &Clip) -> Result<(), EditError> {
        let kind = self.sequence.tracks.get(track).ok_or(EditError::NoSuchTrack)?.kind;
        let ok = self.media(clip.media).map_or(true, |m| match kind {
            TrackKind::Video => m.has_video,
            TrackKind::Audio => m.has_audio,
        });
        if ok {
            Ok(())
        } else {
            Err(EditError::WrongTrackKind)
        }
    }

    /// The clip with `id`, on a writable track.
    fn writable_clip(&self, id: u64) -> Result<(usize, usize), EditError> {
        let (t, i) = self.sequence.find_clip(id).ok_or(EditError::NoSuchClip)?;
        self.writable(t)?;
        Ok((t, i))
    }

    /// Empties `from..to` of `track`: clips inside go, clips across an edge
    /// are cut there (a clip across both becomes two).
    fn clear_range(&mut self, track: usize, from: Frame, to: Frame) {
        let clips = core::mem::take(&mut self.sequence.tracks[track].clips);
        let mut out = Vec::with_capacity(clips.len() + 1);
        for c in clips {
            if c.end() <= from || c.start >= to {
                out.push(c);
                continue;
            }
            let keeps_left = c.start < from;
            if keeps_left {
                let mut left = c.clone();
                left.length = from - c.start;
                out.push(left);
            }
            if c.end() > to {
                let id = if keeps_left { self.new_id() } else { c.id };
                out.push(Clip {
                    id,
                    start: to,
                    source_in: c.source_in + (to - c.start),
                    length: c.end() - to,
                    transition: None,
                    ..c.clone()
                });
            }
        }
        self.sequence.tracks[track].clips = out;
        self.sequence.tracks[track].sort();
    }

    /// Cuts the clip of `track` that spans `at` in two; `false` when no clip
    /// spans it.
    fn split_at(&mut self, track: usize, at: Frame) -> bool {
        let Some(i) = self.sequence.tracks[track]
            .clips
            .iter()
            .position(|c| c.start < at && at < c.end())
        else {
            return false;
        };
        let id = self.new_id();
        let clips = &mut self.sequence.tracks[track].clips;
        let c = clips[i].clone();
        clips[i].length = at - c.start;
        clips.insert(
            i + 1,
            Clip {
                id,
                start: at,
                source_in: c.source_in + (at - c.start),
                length: c.end() - at,
                transition: None,
                ..c
            },
        );
        true
    }

    fn apply(&mut self, edit: Edit) -> Result<(), EditError> {
        match edit {
            Edit::Overwrite { track, at, mut clip } => {
                self.writable(track)?;
                self.fits(track, &clip)?;
                clip.start = at.max(0);
                self.clear_range(track, clip.start, clip.end());
                self.sequence.tracks[track].clips.push(clip);
                self.sequence.tracks[track].sort();
            }
            Edit::Insert { track, at, mut clip } => {
                self.writable(track)?;
                self.fits(track, &clip)?;
                let at = at.max(0);
                self.split_at(track, at);
                for c in &mut self.sequence.tracks[track].clips {
                    if c.start >= at {
                        c.start += clip.length;
                    }
                }
                clip.start = at;
                self.sequence.tracks[track].clips.push(clip);
                self.sequence.tracks[track].sort();
            }
            Edit::Razor { track, at } => {
                let tracks: Vec<usize> = match track {
                    Some(t) => {
                        self.writable(t)?;
                        vec![t]
                    }
                    None => (0..self.sequence.tracks.len())
                        .filter(|t| !self.sequence.tracks[*t].locked)
                        .collect(),
                };
                let mut cut = false;
                for t in tracks {
                    cut |= self.split_at(t, at);
                }
                if !cut {
                    return Err(EditError::NothingToDo);
                }
            }
            Edit::Lift { clip } => {
                let (t, i) = self.writable_clip(clip)?;
                self.sequence.tracks[t].clips.remove(i);
            }
            Edit::RippleDelete { clip } => {
                let (t, i) = self.writable_clip(clip)?;
                let gone = self.sequence.tracks[t].clips.remove(i);
                for c in &mut self.sequence.tracks[t].clips {
                    if c.start >= gone.end() {
                        c.start -= gone.length;
                    }
                }
            }
            Edit::Move { clip, track, start } => {
                let (t, i) = self.writable_clip(clip)?;
                self.writable(track)?;
                if self.sequence.tracks[t].kind != self.sequence.tracks[track].kind {
                    return Err(EditError::WrongTrackKind);
                }
                let mut c = self.sequence.tracks[t].clips.remove(i);
                c.start = start.max(0);
                self.clear_range(track, c.start, c.end());
                self.sequence.tracks[track].clips.push(c);
                self.sequence.tracks[track].sort();
            }
            Edit::Trim { clip, edge, at } => self.trim(clip, edge, at, false)?,
            Edit::RippleTrim { clip, edge, at } => self.trim(clip, edge, at, true)?,
            Edit::Slip { clip, delta } => {
                let (t, i) = self.writable_clip(clip)?;
                let c = &self.sequence.tracks[t].clips[i];
                let most = (self.media_frames(c.media) - c.length).max(0);
                let source_in = (c.source_in + delta).clamp(0, most);
                self.sequence.tracks[t].clips[i].source_in = source_in;
            }
            Edit::SetEffects { clip, effects } => {
                let (t, i) = self.writable_clip(clip)?;
                self.sequence.tracks[t].clips[i].effects = effects;
            }
            Edit::SetTransition { clip, transition } => {
                let (t, i) = self.writable_clip(clip)?;
                let length = self.sequence.tracks[t].clips[i].length;
                self.sequence.tracks[t].clips[i].transition = transition.map(|mut tr| {
                    tr.frames = tr.frames.clamp(1, length.max(1));
                    tr
                });
            }
            Edit::SetEnabled { clip, enabled } => {
                let (t, i) = self.writable_clip(clip)?;
                self.sequence.tracks[t].clips[i].enabled = enabled;
            }
            Edit::SetTrackHidden { track, hidden } => {
                self.sequence
                    .tracks
                    .get_mut(track)
                    .ok_or(EditError::NoSuchTrack)?
                    .hidden = hidden;
            }
            Edit::SetTrackLocked { track, locked } => {
                self.sequence
                    .tracks
                    .get_mut(track)
                    .ok_or(EditError::NoSuchTrack)?
                    .locked = locked;
            }
        }
        Ok(())
    }

    /// A trim of clip `id`'s `edge` to `at`. A plain trim stays between the
    /// media's ends and the neighbouring clips; a ripple trim moves the
    /// clips after the edit by as much as the clip changed.
    fn trim(&mut self, id: u64, edge: Edge, at: Frame, ripple: bool) -> Result<(), EditError> {
        let (t, i) = self.writable_clip(id)?;
        let media_frames = self.media_frames(self.sequence.tracks[t].clips[i].media);
        let clips = &mut self.sequence.tracks[t].clips;
        let prev_end = if i > 0 { clips[i - 1].end() } else { 0 };
        let next_start = clips.get(i + 1).map(|c| c.start);
        let c = clips[i].clone();
        let old_end = c.end();
        match (edge, ripple) {
            (Edge::Start, false) => {
                let earliest = (c.start - c.source_in).max(prev_end);
                let latest = c.end() - 1;
                let new_start = at.clamp(earliest.min(latest), latest);
                let delta = new_start - c.start;
                let clip = &mut clips[i];
                clip.start = new_start;
                clip.source_in += delta;
                clip.length -= delta;
            }
            (Edge::Start, true) => {
                // The clip stays where it is and starts on other media
                // frames; what follows moves by the change of length.
                let delta = (at - c.start).clamp(-c.source_in, c.length - 1);
                let clip = &mut clips[i];
                clip.source_in += delta;
                clip.length -= delta;
                for other in clips.iter_mut() {
                    if other.id != id && other.start >= old_end {
                        other.start -= delta;
                    }
                }
            }
            (Edge::End, false) => {
                let latest = (c.start + (media_frames - c.source_in))
                    .min(next_start.unwrap_or(Frame::MAX));
                let earliest = c.start + 1;
                let new_end = at.clamp(earliest, latest.max(earliest));
                clips[i].length = new_end - c.start;
            }
            (Edge::End, true) => {
                let latest = c.start + (media_frames - c.source_in);
                let earliest = c.start + 1;
                let new_end = at.clamp(earliest, latest.max(earliest));
                let delta = new_end - old_end;
                clips[i].length = new_end - c.start;
                for other in clips.iter_mut() {
                    if other.id != id && other.start >= old_end {
                        other.start += delta;
                    }
                }
            }
        }
        clips.sort_by_key(|c| c.start);
        Ok(())
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
