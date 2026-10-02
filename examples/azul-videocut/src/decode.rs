//! Pictures out of MP4 files, on azul's video stack: an `Mp4Demuxer` gives
//! the H.264 access units by index, a `VideoDecoder` (VideoToolbox on macOS,
//! Vulkan Video on x86_64 Linux / Windows) turns them into frames.
//!
//! To show the frame at a time the reader starts the decoder at the
//! keyframe at or before it and decodes forward ([`feed_plan`]); a frame a
//! little further on in the same group of pictures goes on from where the
//! decoder is (playback, a step), so the decoder is restarted only by a
//! jump. The last few frames are kept ([`FrameCache`]).
//!
//! A reader holds a decoder session, which is not moved between threads:
//! each worker job (a scrub, a playback, an export) opens the readers it
//! needs. The FILE BYTES are shared between jobs ([`MediaFiles`]), so a
//! file is read from the disk or the Drive once.

use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    sync::{Arc, Mutex},
};

use azul::{
    image::RawImageFormat,
    vec::U8Vec,
    video::{Mp4Demuxer, VideoDecoder, VideoFrame},
};
use azul_storage::Drive;

use crate::{
    model::{Frame, MediaItem, MediaSource},
    render::{fit_within, Canvas, FrameSource, Generated},
};

/// How many decoded frames a reader keeps.
pub const READER_CACHE: usize = 6;

/// The access units to feed (decode order) to get the frame of unit
/// `target`, whose keyframe is `keyframe`, when the decoder would take unit
/// `next` next (`None`: a fresh decoder): on from `next` when it lies
/// after the keyframe and not past the target, else from the keyframe.
#[must_use]
pub fn feed_plan(next: Option<usize>, keyframe: usize, target: usize) -> Range<usize> {
    match next {
        Some(n) if n > keyframe && n <= target => n..target + 1,
        _ => keyframe..target + 1,
    }
}

/// Sequence frame `frame` at `fps` as milliseconds of media time.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn media_ms(frame: Frame, fps: u32) -> f64 {
    frame as f64 * 1000.0 / f64::from(fps.max(1))
}

/// The last few decoded frames, by access-unit index.
#[derive(Debug, Clone, Default)]
pub struct FrameCache {
    capacity: usize,
    items: VecDeque<(usize, Canvas)>,
}

impl FrameCache {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            items: VecDeque::new(),
        }
    }

    /// Keeps `picture` as unit `index`'s, dropping the oldest when full.
    pub fn put(&mut self, index: usize, picture: Canvas) {
        self.items.retain(|(i, _)| *i != index);
        self.items.push_back((index, picture));
        while self.items.len() > self.capacity {
            self.items.pop_front();
        }
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Canvas> {
        self.items.iter().find(|(i, _)| *i == index).map(|(_, c)| c)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A decoded frame as a picture (RGBA8; BGRA8 is swizzled).
fn canvas_of(frame: VideoFrame) -> Option<Canvas> {
    let (w, h) = (frame.width, frame.height);
    let mut rgba = frame.bytes.as_slice().to_vec();
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return None;
    }
    if matches!(frame.format, RawImageFormat::BGRA8) {
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    Some(Canvas {
        width: w,
        height: h,
        rgba,
    })
}

/// One open MP4: its demuxer, a decoder and where the decoder is.
pub struct ClipReader {
    demuxer: Mp4Demuxer,
    decoder: VideoDecoder,
    /// The access unit the decoder takes next.
    next: Option<usize>,
    cache: FrameCache,
}

impl ClipReader {
    /// A reader of the MP4 in `bytes` handing frames out at `width` x
    /// `height` (0 x 0: the stream's size; the decoder scales).
    pub fn open(bytes: &[u8], width: u32, height: u32) -> Result<Self, String> {
        let demuxer = Mp4Demuxer::create(U8Vec::from_vec(bytes.to_vec()));
        if !demuxer.is_open() {
            return Err(format!("not a readable MP4: {}", demuxer.open_error().as_str()));
        }
        let decoder = VideoDecoder::open(false);
        if !decoder.is_open() {
            return Err(String::from("this machine has no H.264 decoder azul can use"));
        }
        decoder.set_output_format(RawImageFormat::RGBA8);
        decoder.set_output_size(width, height);
        Ok(Self {
            demuxer,
            decoder,
            next: None,
            cache: FrameCache::with_capacity(READER_CACHE),
        })
    }

    /// The stream's size and rate and its length in milliseconds.
    #[must_use]
    pub fn info(&self) -> (u32, u32, f32, f64) {
        (
            self.demuxer.width(),
            self.demuxer.height(),
            self.demuxer.fps(),
            self.demuxer.duration_ms(),
        )
    }

    /// The frame shown at `ms` of media time.
    pub fn picture_at_ms(&mut self, ms: f64) -> Option<Canvas> {
        let target = self.demuxer.frame_at(ms);
        if let Some(c) = self.cache.get(target) {
            return Some(c.clone());
        }
        let keyframe = self.demuxer.keyframe_before(target);
        let mut last = None;
        for i in feed_plan(self.next, keyframe, target) {
            let chunk = self.demuxer.chunk(i).into_option()?;
            self.decoder.decode(chunk.data);
            // Decode is synchronous (VideoToolbox) and hands frames out in
            // decode order: the last frame out is unit i's.
            let mut got = None;
            while let Some(frame) = self.decoder.recv_frame().into_option() {
                got = Some(frame);
            }
            self.next = Some(i + 1);
            if let Some(picture) = got.and_then(canvas_of) {
                if i + READER_CACHE > target {
                    self.cache.put(i, picture.clone());
                }
                last = Some(picture);
            }
        }
        last
    }
}

/// File bytes by media id, shared by every job of the app.
#[derive(Debug, Default)]
pub struct MediaFiles {
    files: Mutex<HashMap<u64, Arc<Vec<u8>>>>,
}

impl MediaFiles {
    /// The bytes of `media`, read once (from its path, or through `drive`).
    pub fn bytes(&self, media: &MediaItem, drive: &dyn Drive) -> Result<Arc<Vec<u8>>, String> {
        if let Some(b) = self.files.lock().ok().and_then(|f| f.get(&media.id).cloned()) {
            return Ok(b);
        }
        let bytes = match &media.source {
            MediaSource::Path { path } => {
                std::fs::read(path).map_err(|e| format!("{path}: {e}"))?
            }
            MediaSource::Stored { key } => drive.get(key).map_err(|e| format!("{key}: {e}"))?,
            MediaSource::Generated { .. } => return Err(String::from("a generated item has no file")),
        };
        let bytes = Arc::new(bytes);
        if let Ok(mut f) = self.files.lock() {
            f.insert(media.id, bytes.clone());
        }
        Ok(bytes)
    }
}

/// Every media item's pictures for one job: generated ones made, files
/// read through a [`ClipReader`] each (opened on first use).
pub struct Library {
    files: Arc<MediaFiles>,
    drive: Arc<dyn Drive>,
    fps: u32,
    readers: HashMap<u64, Result<ClipReader, String>>,
    /// The last failure, for the status bar.
    pub error: Option<String>,
}

impl Library {
    /// A library reading files through `files` / `drive` for a sequence at
    /// `fps`.
    pub fn new(files: Arc<MediaFiles>, drive: Arc<dyn Drive>, fps: u32) -> Self {
        Self {
            files,
            drive,
            fps,
            readers: HashMap::new(),
            error: None,
        }
    }
}

impl FrameSource for Library {
    fn picture(
        &mut self,
        media: &MediaItem,
        frame: Frame,
        max_width: u32,
        max_height: u32,
    ) -> Option<Canvas> {
        if media.is_generated() {
            return Generated.picture(media, frame, max_width, max_height);
        }
        if !self.readers.contains_key(&media.id) {
            let (w, h) = fit_within(media.width, media.height, max_width, max_height);
            let reader = self
                .files
                .bytes(media, self.drive.as_ref())
                .and_then(|bytes| ClipReader::open(&bytes, w, h));
            self.readers.insert(media.id, reader);
        }
        match self.readers.get_mut(&media.id)? {
            Ok(reader) => reader.picture_at_ms(media_ms(frame, self.fps)),
            Err(e) => {
                self.error = Some(format!("{}: {e}", media.name));
                None
            }
        }
    }
}

/// What a probe of an MP4 found: size, rate, length, a thumbnail.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_ms: f64,
    pub thumbnail: Option<Canvas>,
}

/// Opens `bytes` as an MP4 and reads its first frame at `thumb_w` x
/// `thumb_h` for the bin.
pub fn probe(bytes: &[u8], thumb_w: u32, thumb_h: u32) -> Result<Probe, String> {
    let demuxer = Mp4Demuxer::create(U8Vec::from_vec(bytes.to_vec()));
    if !demuxer.is_open() {
        return Err(format!("not a readable MP4: {}", demuxer.open_error().as_str()));
    }
    let (w, h) = fit_within(demuxer.width(), demuxer.height(), thumb_w, thumb_h);
    let thumbnail = ClipReader::open(bytes, w, h)
        .ok()
        .and_then(|mut r| r.picture_at_ms(0.0));
    Ok(Probe {
        width: demuxer.width(),
        height: demuxer.height(),
        fps: demuxer.fps(),
        duration_ms: demuxer.duration_ms(),
        thumbnail,
    })
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
