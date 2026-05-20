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
use azul_css::{AzString, U8Vec};

/// Requested video-playback configuration.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoConfig {
    /// Source URL or file path (decoded via vk-video + HTTP-range fetch).
    pub source: AzString,
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
            source: AzString::from_const_str(""),
            autoplay: true,
            looping: false,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl VideoConfig {
    /// A default config playing `source` (autoplay on, no loop, BGRA8).
    pub fn new(source: AzString) -> Self {
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
