//! One track's segment files: opening them, rotating them, noticing a new picture size, and the
//! manifest's view of them.
//!
//! A [`TrackWriter`] takes the track's encoded samples with their times on the session clock and
//! writes them as a sequence of segment files (`screen/0001.mp4`, `0002.mp4`, ...), each a
//! fragmented MP4 from [`crate::recorder_mp4`]. A segment starts at a keyframe (video) or at a
//! packet (audio); it is closed and the next one opened at the first keyframe once it has run for
//! the segment length, and at once when the encoder's parameter sets change (the window was
//! resized: a file has one picture size). Every fragment is appended and synced when it is whole,
//! so a kill loses the fragment in progress and nothing before it.
//!
//! The files themselves are behind [`SegmentStore`], so the rules are tested on a map in memory.
//! A write that fails marks the track as failed (the others go on); the writer says how in words.

use std::{fmt, io};

use crate::{
    recorder_manifest::{segment_file, Segment},
    recorder_mp4::{
        annexb_to_avcc, Colour, FragmentWriter, Limits, MuxError, OpusConfig, TrackConfig, VideoConfig,
    },
};

/// Where segment files go: the session folder on disk, or a map in a test.
pub trait SegmentStore {
    /// Creates the file `rel_path` (relative to the session folder; the folders on the way are made
    /// when they are not there) and fails if it exists already: a file of an earlier run is never
    /// overwritten.
    ///
    /// # Errors
    /// The file could not be created.
    fn create(&mut self, rel_path: &str) -> io::Result<Box<dyn SegmentFile>>;
}

/// A segment file that is being written. `Send`: the writer thread owns it.
pub trait SegmentFile: Send {
    /// Appends `bytes`.
    ///
    /// # Errors
    /// The bytes could not be written.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Makes what was appended durable.
    ///
    /// # Errors
    /// The file could not be synced.
    fn sync(&mut self) -> io::Result<()>;

    /// The file's length so far.
    fn len(&self) -> u64;
}

/// What a track holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// H.264 samples, as Annex-B access units.
    Video,
    /// Opus packets.
    Opus,
}

/// What happened to the segment files by a sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A segment was opened (the manifest lists it).
    Opened(Segment),
    /// A segment was closed properly (the manifest's entry is complete).
    Closed(Segment),
}

/// Why a track could not take a sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackError {
    /// A file could not be created or written: the words of the system.
    Io(String),
    /// The muxer refused a sample.
    Mux(MuxError),
    /// The picture's description could not be read from a keyframe.
    Config(String),
}

impl fmt::Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "writing the recording failed: {e}"),
            Self::Mux(e) => write!(f, "a sample was refused: {e}"),
            Self::Config(e) => write!(f, "the picture's description could not be read: {e}"),
        }
    }
}

impl std::error::Error for TrackError {}

impl From<MuxError> for TrackError {
    fn from(e: MuxError) -> Self {
        Self::Mux(e)
    }
}

impl From<io::Error> for TrackError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

struct Open {
    writer: FragmentWriter,
    file: Box<dyn SegmentFile>,
    info: Segment,
    /// The parameter sets the segment was opened with (video): a keyframe with others rotates.
    config_key: Vec<u8>,
}

/// The writer of one track's segment files.
pub struct TrackWriter {
    dir: &'static str,
    kind: Kind,
    segment_us: u64,
    limits: Limits,
    created_unix_s: u64,
    next_index: u32,
    open: Option<Open>,
    done: Vec<Segment>,
    dropped_before_keyframe: u64,
    failed: Option<String>,
    default_colour: Option<Colour>,
}

impl TrackWriter {
    /// A writer for the track in folder `dir`, whose first segment is numbered `first_index` (1, or
    /// the next number after the files an earlier run left). `segment_us` is how long a segment runs.
    #[must_use]
    pub fn new(dir: &'static str, kind: Kind, segment_us: u64, first_index: u32, created_unix_s: u64) -> Self {
        Self {
            dir,
            kind,
            segment_us,
            limits: Limits::default(),
            created_unix_s,
            next_index: first_index.max(1),
            open: None,
            done: Vec::new(),
            dropped_before_keyframe: 0,
            failed: None,
            default_colour: None,
        }
    }

    /// The colour tags to write when the encoder's stream has none (video).
    #[must_use]
    pub const fn with_default_colour(mut self, colour: Colour) -> Self {
        self.default_colour = Some(colour);
        self
    }

    /// Whether the track broke: why, in words. A broken track takes nothing more.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// Video samples dropped because no keyframe had begun a segment yet.
    #[must_use]
    pub const fn dropped_before_keyframe(&self) -> u64 {
        self.dropped_before_keyframe
    }

    /// Whether a segment is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The segments so far, the open one last as it stands now (no end, not complete).
    #[must_use]
    pub fn segments(&self) -> Vec<Segment> {
        let mut out = self.done.clone();
        if let Some(open) = &self.open {
            let mut info = open.info.clone();
            info.bytes = open.file.len();
            info.samples = open.writer.stats().samples;
            out.push(info);
        }
        out
    }

    /// Bytes in the track's files so far.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.done.iter().map(|s| s.bytes).sum::<u64>() + self.open.as_ref().map_or(0, |o| o.file.len())
    }

    /// The time the open segment began, on the session clock.
    #[must_use]
    pub fn open_since_us(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.info.start_us)
    }

    /// Takes one video access unit (Annex-B) seen at `pts_us` on the session clock.
    ///
    /// # Errors
    /// A file could not be written, or the keyframe's description could not be read. The track is
    /// then failed ([`Self::failure`]).
    pub fn push_video(&mut self, store: &mut dyn SegmentStore, pts_us: u64, annexb: &[u8]) -> Result<Vec<Change>, TrackError> {
        if self.kind != Kind::Video {
            return Err(TrackError::Config(String::from("this track holds sound, not pictures")));
        }
        if self.failed.is_some() {
            return Ok(Vec::new());
        }
        let result = self.video(store, pts_us, annexb);
        if let Err(e) = &result {
            self.failed = Some(e.to_string());
        }
        result
    }

    fn video(&mut self, store: &mut dyn SegmentStore, pts_us: u64, annexb: &[u8]) -> Result<Vec<Change>, TrackError> {
        let sample = annexb_to_avcc(annexb);
        if sample.bytes.is_empty() {
            return Ok(Vec::new());
        }
        let mut changes = Vec::new();
        if sample.keyframe {
            let key: Vec<u8> = [sample.sps.as_deref().unwrap_or(&[]), sample.pps.as_deref().unwrap_or(&[])].concat();
            let rotate = self.open.as_ref().is_some_and(|o| {
                pts_us.saturating_sub(o.info.start_us) >= self.segment_us || (!key.is_empty() && key != o.config_key)
            });
            if rotate {
                changes.extend(self.close(pts_us)?);
            }
            if self.open.is_none() && !key.is_empty() {
                let mut config = VideoConfig::from_keyframe(annexb).map_err(TrackError::Config)?;
                if let Some(colour) = self.default_colour {
                    config = config.with_default_colour(colour);
                }
                let (w, h) = (config.info.width, config.info.height);
                changes.push(self.open_segment(store, pts_us, TrackConfig::Video(config), key, (w, h))?);
            }
        }
        let Some(open) = self.open.as_mut() else {
            self.dropped_before_keyframe += 1;
            return Ok(changes);
        };
        let rel = pts_us.saturating_sub(open.info.start_us);
        let fragment = open.writer.push(rel, sample.bytes, sample.keyframe)?;
        Self::append(open, &fragment)?;
        Ok(changes)
    }

    /// Takes one Opus packet whose first sample is at `pts_us` on the session clock.
    ///
    /// # Errors
    /// As [`Self::push_video`].
    pub fn push_audio(&mut self, store: &mut dyn SegmentStore, pts_us: u64, packet: Vec<u8>) -> Result<Vec<Change>, TrackError> {
        if self.kind != Kind::Opus {
            return Err(TrackError::Config(String::from("this track holds pictures, not sound")));
        }
        if self.failed.is_some() {
            return Ok(Vec::new());
        }
        let result = self.audio(store, pts_us, packet);
        if let Err(e) = &result {
            self.failed = Some(e.to_string());
        }
        result
    }

    fn audio(&mut self, store: &mut dyn SegmentStore, pts_us: u64, packet: Vec<u8>) -> Result<Vec<Change>, TrackError> {
        let mut changes = Vec::new();
        if self.open.as_ref().is_some_and(|o| pts_us.saturating_sub(o.info.start_us) >= self.segment_us) {
            changes.extend(self.close(pts_us)?);
        }
        if self.open.is_none() {
            changes.push(self.open_segment(store, pts_us, TrackConfig::Opus(OpusConfig::mono_48k()), Vec::new(), (0, 0))?);
        }
        if let Some(open) = self.open.as_mut() {
            let rel = pts_us.saturating_sub(open.info.start_us);
            let fragment = open.writer.push(rel, packet, true)?;
            Self::append(open, &fragment)?;
        }
        Ok(changes)
    }

    /// Closes the open segment at `end_us` (the recording stopped, or paused).
    ///
    /// # Errors
    /// The file could not be written.
    pub fn finish(&mut self, end_us: u64) -> Result<Vec<Change>, TrackError> {
        if self.failed.is_some() {
            return Ok(Vec::new());
        }
        let result = self.close(end_us);
        if let Err(e) = &result {
            self.failed = Some(e.to_string());
        }
        result.map(|c| c.into_iter().collect())
    }

    fn open_segment(
        &mut self,
        store: &mut dyn SegmentStore,
        start_us: u64,
        config: TrackConfig,
        config_key: Vec<u8>,
        size: (u32, u32),
    ) -> Result<Change, TrackError> {
        let name = segment_file(self.dir, self.next_index);
        let mut file = store.create(&name)?;
        let (writer, head) = FragmentWriter::new(config, self.created_unix_s, self.limits);
        file.append(&head)?;
        file.sync()?;
        self.next_index += 1;
        let info = Segment {
            file: name,
            start_us,
            end_us: None,
            bytes: file.len(),
            samples: 0,
            complete: false,
            recovered: false,
            width: size.0,
            height: size.1,
        };
        let change = Change::Opened(info.clone());
        self.open = Some(Open { writer, file, info, config_key });
        Ok(change)
    }

    fn close(&mut self, end_us: u64) -> Result<Option<Change>, TrackError> {
        let Some(mut open) = self.open.take() else {
            return Ok(None);
        };
        let rel_end = end_us.saturating_sub(open.info.start_us);
        let tail = open.writer.finish(rel_end);
        let appended = open.file.append(&tail).and_then(|()| open.file.sync());
        let mut info = open.info;
        info.bytes = open.file.len();
        info.samples = open.writer.stats().samples;
        if let Err(e) = appended {
            // What was written stays listed, not complete: a scan will find where it is whole.
            self.done.push(info);
            return Err(e.into());
        }
        info.end_us = Some(info.start_us + rel_end);
        info.complete = true;
        self.done.push(info.clone());
        Ok(Some(Change::Closed(info)))
    }

    /// Appends a fragment the muxer handed out and syncs it; keeps the open segment's size.
    fn append(open: &mut Open, fragment: &[u8]) -> Result<(), TrackError> {
        if fragment.is_empty() {
            return Ok(());
        }
        open.file.append(fragment)?;
        open.file.sync()?;
        open.info.bytes = open.file.len();
        Ok(())
    }
}

#[cfg(test)]
#[path = "recorder_track_test.rs"]
mod recorder_track_test;
