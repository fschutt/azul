//! POD types for the video-playback surface
//! (SUPER_PLAN_2 §4 Priority 6 + research).
//!
//! Same "dumb widget" architecture as camera/screencap
//! (`azul_layout::widgets::video::VideoWidget`): a background thread decodes
//! the source (vk-video - GPU decode + HTTP-range fetch) and its writeback
//! uploads each frame into the shared GL-texture `ImageRef` + recomposites.
//! Defined here in `azul-core` so the config crosses the FFI without
//! `azul-layout` (or vk-video) as a dependency.
//!
//! Unlike the camera/screencap configs this carries a `source` string, so
//! it's `Clone` but not `Copy`.

use crate::resources::RawImageFormat;
use crate::url::Url;
use azul_css::{AzString, U8Vec};

/// Where a video widget pulls its H.264/MP4 data from — strongly typed so the
/// decode worker matches on it directly (no `RefAny` downcast). Mirrors
/// [`crate::screencap::ScreenCaptureSource`].
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum VideoSource {
    /// An HTTP(S) URL, fetched on the decode thread via an HTTP range request.
    Url(Url),
    /// A local filesystem path.
    File(AzString),
    /// Raw MP4 bytes already in memory.
    Bytes(U8Vec),
}

impl Default for VideoSource {
    fn default() -> Self {
        VideoSource::Url(Url::default())
    }
}

/// Requested video-playback configuration.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoConfig {
    /// Where to load the video from (URL / file path / in-memory bytes).
    pub source: VideoSource,
    /// Seek / scrub position in seconds. Changing it across a relayout makes the
    /// widget's merge callback tell the decode worker to seek (scrubbing
    /// timeline) — the decoder survives relayout like the map's tile cache.
    pub timestamp: f32,
    /// Start playing automatically on mount.
    pub autoplay: bool,
    /// Restart from the beginning when the stream ends.
    pub looping: bool,
    /// Texture format the decoder delivers. `BGRA8` is the portable default;
    /// `Nv12` (a later `RawImageFormat` addition) is the zero-copy path.
    pub output_format: RawImageFormat,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            source: VideoSource::default(),
            timestamp: 0.0,
            autoplay: true,
            looping: false,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl VideoConfig {
    /// A default config playing `source` (autoplay on, no loop, BGRA8, t=0).
    pub fn new(source: VideoSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

/// One captured or decoded frame - tightly-packed RGBA8 pixels
/// (`width * height * 4`). The unit a capture/decode worker produces, the
/// `set_on_frame` hook hands to user code (effects / save / send), and (P8)
/// azul-meet sends over UDP. Defined here (like [`crate::audio::AudioFrame`])
/// so it crosses the FFI without `azul-layout` as a dependency.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly-packed RGBA8 pixel bytes (`width * height * 4`).
    pub bytes: U8Vec,
}

impl VideoFrame {
    /// A frame wrapping `bytes` (tightly-packed RGBA8, `width * height * 4`).
    pub fn new(width: u32, height: u32, bytes: U8Vec) -> Self {
        Self {
            width,
            height,
            bytes,
        }
    }
}

// FFI Option wrapper for a frame-pull hook / accessor. `copy = false` (U8Vec).
impl_option!(VideoFrame, OptionVideoFrame, copy = false, [Clone, Debug]);

// FFI `Vec<VideoFrame>` wrapper — the list a batch decode (`DecodedVideo`,
// `dll::desktop::extra::video_codec::pipeline`) hands back across the C ABI.
// `VideoFrame` derives Debug + Clone + PartialEq, so mirror exactly those Vec
// trait impls (no PartialOrd: `VideoFrame` isn't `PartialOrd`).
impl_vec!(VideoFrame, VideoFrameVec, VideoFrameVecDestructor, VideoFrameVecDestructorType, VideoFrameVecSlice, OptionVideoFrame);
impl_vec_debug!(VideoFrame, VideoFrameVec);
impl_vec_clone!(VideoFrame, VideoFrameVec, VideoFrameVecDestructor);
impl_vec_partialeq!(VideoFrame, VideoFrameVec);
