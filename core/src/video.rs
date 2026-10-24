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

use azul_css::{AzString, U8Vec};

use crate::{resources::RawImageFormat, url::Url};
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// Where a video widget pulls its H.264/MP4 data from — strongly typed so the
/// decode worker matches on it directly (no `RefAny` downcast). Mirrors
/// [`crate::screencap::ScreenCaptureSource`].
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // #[repr(C,u8)] FFI enum: boxing a variant changes the C
                                     // ABI/api.json
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
        Self::Url(Url::default())
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
    /// Start playing as soon as the first frame is decoded. `false` decodes
    /// the first frame and holds it as a poster: the video starts paused,
    /// whatever `paused` says.
    pub autoplay: bool,
    /// Restart from the beginning when the stream ends.
    pub looping: bool,
    /// Hold playback on the current frame. A change of this field across a
    /// relayout pauses (`true`) or resumes (`false`) the running decoder, the
    /// way a changed `timestamp` seeks it. A player with a play button starts
    /// with `autoplay: false, paused: true` and clears `paused` on the first
    /// press.
    pub paused: bool,
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
            paused: false,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl VideoConfig {
    /// A default config playing `source` (autoplay on, no loop, BGRA8, t=0).
    #[must_use]
    pub fn new(source: VideoSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

/// Where a video's pipeline stands, as the widget's `on_status` hook reports
/// it.
#[repr(C)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum VideoPhase {
    /// Downloading, demuxing, or decoding the first frame: nothing to show
    /// yet.
    #[default]
    Loading,
    /// A frame is on screen and the clock is held: the poster before the
    /// first play, or a pause.
    Paused,
    /// The clock runs and the frames follow it.
    Playing,
    /// A video that does not loop reached its end; its last frame stays up.
    Ended,
    /// The pipeline failed. [`VideoStatus::message`] says why.
    Failed,
}

/// What a video's decoder reports: the phase, where the clock stands, how
/// long the video is, and why it failed if it did.
///
/// The decode worker sends one whenever the phase changes, after a seek, and
/// about four times a second while the video plays. `VideoWidget::with_on_status`
/// hands each one to the app, which drives its play button, its time display
/// and its error text from it.
///
/// Field order is by descending alignment (the string, the `f32`s, then the
/// phase): the repo's alignment-order check is a hard error.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStatus {
    /// Why the pipeline failed, in words for the user. Empty unless `phase`
    /// is [`VideoPhase::Failed`].
    pub message: AzString,
    /// Playback position, in seconds from the start.
    pub position_s: f32,
    /// Length in seconds, `0.0` while it is not known yet.
    pub duration_s: f32,
    /// Where the pipeline stands.
    pub phase: VideoPhase,
}

impl Default for VideoStatus {
    fn default() -> Self {
        Self::loading()
    }
}

impl VideoStatus {
    /// Nothing decoded yet: the status a video starts in.
    #[must_use]
    pub const fn loading() -> Self {
        Self::create(VideoPhase::Loading, 0.0, 0.0)
    }

    /// `phase` at `position_s` of a video `duration_s` long, with no message.
    #[must_use]
    pub const fn create(phase: VideoPhase, position_s: f32, duration_s: f32) -> Self {
        Self {
            message: AzString::from_const_str(""),
            position_s,
            duration_s,
            phase,
        }
    }

    /// The pipeline failed; `message` says why, in words for the user.
    #[must_use]
    pub const fn failed(message: AzString) -> Self {
        Self {
            message,
            position_s: 0.0,
            duration_s: 0.0,
            phase: VideoPhase::Failed,
        }
    }

    /// How far through the video the position is, `0.0..=1.0`: `0.0` while
    /// the length is unknown, so a progress bar with no end stays empty.
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration_s > 0.0 {
            let p = self.position_s / self.duration_s;
            if p.is_nan() {
                0.0
            } else {
                p.clamp(0.0, 1.0)
            }
        } else {
            0.0
        }
    }
}

/// One captured or decoded frame: tightly packed pixels in `format`.
///
/// The unit a capture/decode worker produces, the
/// `set_on_frame` hook hands to user code (effects / save / send), and (P8)
/// azul-meet sends over UDP. Defined here (like [`crate::audio::AudioFrame`])
/// so it crosses the FFI without `azul-layout` as a dependency.
///
/// `format` says what `bytes` holds: `RGBA8` or `BGRA8` (`width * height *
/// 4` bytes), or one of the NV12 formats (the Y plane, then the Cb,Cr plane,
/// see `azul_core::resources::Nv12Layout`). A capture delivers what its
/// config's `output_format` asked for where the platform can (NV12 and BGRA8
/// without a conversion on Apple), a decoder what `set_output_format` asked
/// for; RGBA8 is the default.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly packed pixel bytes in `format`.
    pub bytes: U8Vec,
    /// The byte layout of `bytes`.
    pub format: RawImageFormat,
}

impl VideoFrame {
    /// A frame wrapping `bytes` (tightly-packed RGBA8, `width * height * 4`).
    #[must_use]
    pub const fn new(width: u32, height: u32, bytes: U8Vec) -> Self {
        Self::with_format(width, height, bytes, RawImageFormat::RGBA8)
    }

    /// A frame wrapping `bytes` in `format` (see [`VideoFrame::format`]).
    #[must_use]
    pub const fn with_format(
        width: u32,
        height: u32,
        bytes: U8Vec,
        format: RawImageFormat,
    ) -> Self {
        Self {
            width,
            height,
            bytes,
            format,
        }
    }

    /// The byte length a frame of this size and format must have (`None` on
    /// overflow, or for a format frames do not use).
    #[must_use]
    pub fn expected_len(&self) -> Option<usize> {
        let (w, h) = (self.width as usize, self.height as usize);
        if self.format.is_nv12() {
            crate::resources::Nv12Layout::new(w, h).checked_total_len()
        } else {
            match self.format {
                RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => w.checked_mul(h)?.checked_mul(4),
                _ => None,
            }
        }
    }
}

// FFI Option wrapper for a frame-pull hook / accessor. `copy = false` (U8Vec).
impl_option!(VideoFrame, OptionVideoFrame, copy = false, [Clone, Debug]);

/// One CONSUMER of a capture source's frames: a requested output size.
///
/// A camera or a screen share has many consumers of the same captured
/// frame at once — the on-screen preview tile (sized by layout), a remote
/// participant who asked for 500x200, a recorder at full size. The device
/// captures ONCE, at the smallest size that covers every consumer
/// (`azul_layout::image_scale::covering_size`), and every consumer gets its
/// own resample of that one frame (`azul_layout::image_scale::fan_out`): the
/// camera never captures more than the largest consumer needs, and nothing
/// is ever sent bigger than the consumer asked for. Registered on a capture
/// widget with `CameraWidget::with_consumer` /
/// `ScreenCaptureWidget::with_consumer`; each cut frame is handed to the
/// widget's `on_consumer_frame` hook as a [`ConsumerFrame`].
///
/// `id` is caller-chosen and handed back with every cut frame so one hook can
/// serve many consumers ("client Bob" = 7, "the recorder" = 8). Id 0
/// ([`FrameConsumer::PREVIEW_ID`]) is reserved for the widget's own on-screen
/// tile, whose size follows layout.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FrameConsumer {
    /// Caller-chosen id, handed back with every cut frame. 0 is the preview.
    pub id: u32,
    /// Requested output width in px (0 = invalid, the consumer is skipped).
    pub width: u32,
    /// Requested output height in px (0 = invalid, the consumer is skipped).
    pub height: u32,
}

impl FrameConsumer {
    /// The id of the widget's own on-screen preview: its size follows the
    /// laid-out node (device pixels), never the caller.
    pub const PREVIEW_ID: u32 = 0;

    /// A consumer `id` that wants `width` x `height` frames.
    #[must_use]
    pub const fn new(id: u32, width: u32, height: u32) -> Self {
        Self { id, width, height }
    }

    /// `false` for a zero-sized request (skipped by the fan-out) or the
    /// reserved preview id.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.id != Self::PREVIEW_ID && self.width > 0 && self.height > 0
    }
}

impl_vec!(
    FrameConsumer,
    FrameConsumerVec,
    FrameConsumerVecDestructor,
    FrameConsumerVecDestructorType,
    FrameConsumerVecSlice,
    OptionFrameConsumer
);
impl_vec_debug!(FrameConsumer, FrameConsumerVec);
impl_vec_clone!(FrameConsumer, FrameConsumerVec, FrameConsumerVecDestructor);
impl_vec_partialeq!(FrameConsumer, FrameConsumerVec);
impl_vec_eq!(FrameConsumer, FrameConsumerVec);
impl_vec_partialord!(FrameConsumer, FrameConsumerVec);
impl_vec_ord!(FrameConsumer, FrameConsumerVec);
impl_vec_hash!(FrameConsumer, FrameConsumerVec);

impl_option!(
    FrameConsumer,
    OptionFrameConsumer,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// A frame cut to one consumer's requested size: the [`FrameConsumer`] it
/// was cut for (so one hook can route by `consumer.id`) and the resampled
/// RGBA8 pixels (`consumer.width * consumer.height * 4`).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerFrame {
    /// Which consumer this frame was cut for.
    pub consumer: FrameConsumer,
    /// The frame at the consumer's size.
    pub frame: VideoFrame,
}

impl ConsumerFrame {
    /// Pair a cut frame with the consumer it was cut for.
    #[must_use]
    pub const fn new(consumer: FrameConsumer, frame: VideoFrame) -> Self {
        Self { consumer, frame }
    }
}

impl_option!(
    ConsumerFrame,
    OptionConsumerFrame,
    copy = false,
    [Clone, Debug]
);

// FFI `Vec<VideoFrame>` wrapper — the list a batch decode (`DecodedVideo`,
// `dll::desktop::extra::video_codec::pipeline`) hands back across the C ABI.
// `VideoFrame` derives Debug + Clone + PartialEq, so mirror exactly those Vec
// trait impls (no PartialOrd: `VideoFrame` isn't `PartialOrd`).
impl_vec!(
    VideoFrame,
    VideoFrameVec,
    VideoFrameVecDestructor,
    VideoFrameVecDestructorType,
    VideoFrameVecSlice,
    OptionVideoFrame
);
impl_vec_debug!(VideoFrame, VideoFrameVec);
impl_vec_clone!(VideoFrame, VideoFrameVec, VideoFrameVecDestructor);
impl_vec_partialeq!(VideoFrame, VideoFrameVec);

/// One encoded access unit of a video stream (one picture's worth of NAL
/// units), as a demuxer hands it out.
///
/// Annex-B bytes (start-code-prefixed NALs, the parameter sets in front of a
/// keyframe so a decoder can start there), when it is SHOWN, and whether a
/// decoder can start at it. `VideoDecoder::decode` takes `data` as it is.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoChunk {
    /// Presentation time in milliseconds from the start of the stream.
    pub pts_ms: f64,
    /// Annex-B bytes of the access unit.
    pub data: U8Vec,
    /// A keyframe (IDR): decoding can start here.
    pub is_keyframe: bool,
}

// FFI Option wrapper for the demuxer's by-index accessor. `copy = false` (U8Vec).
impl_option!(VideoChunk, OptionVideoChunk, copy = false, [Clone, Debug]);

#[cfg(test)]
#[path = "video_test.rs"]
mod video_test;
