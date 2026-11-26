//! POD types for the screen-capture surface
//! (SUPER_PLAN_2 §4 Priority 6 + research/01).
//!
//! Symmetric to the camera surface: screen capture is a "dumb widget"
//! (`azul_layout::widgets::screencap::ScreenCaptureWidget`) that owns a
//! background capture thread + a GL-texture `ImageRef`, identical to the
//! camera widget — only the *source* differs (a display / window instead of
//! a camera). Defined here in `azul-core` so the config types cross the FFI
//! without `azul-layout` (or ScreenCaptureKit / MediaProjection / PipeWire)
//! as a dependency.
//!
//! Reuses the camera surface's generic capture status types
//! ([`crate::camera::StreamState`], `CaptureStats`, `CaptureStreamId`,
//! `CaptureErrorCode`) — those are capture-agnostic.

use crate::resources::RawImageFormat;

/// What to capture.
#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenCaptureSource {
    /// The primary display (the default).
    #[default]
    PrimaryDisplay,
    /// A specific display by index (0-based).
    Display(u32),
    /// A specific window by its platform id / handle.
    Window(u64),
    /// A specific display by the system's own id (macOS: its `CGDirectDisplayID`; `0` is the
    /// primary display). Unlike an index it names the same display while others come and go, and
    /// the id of a display is what a list of displays (`Monitor`) and a setting can keep.
    DisplayId(u32),
}

/// Requested screen-capture configuration — the input to the screencap
/// widget. Zero `fps` means "let the backend pick its default".
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenCaptureConfig {
    /// What to capture (display / window).
    pub source: ScreenCaptureSource,
    /// Preferred frame rate (0 = backend default).
    pub fps: u32,
    /// Texture format the backend should deliver. `BGRA8` is the portable
    /// default; `Nv12` (a later `RawImageFormat` addition) is the zero-copy
    /// path on platforms that produce it natively.
    pub output_format: RawImageFormat,
    /// Whether the cursor is in the picture. Default: yes.
    pub show_cursor: bool,
    /// Whether this app's own windows are in the picture. Default: no - a shared desktop that shows
    /// the sharing app loops (every repaint of the tile is a change of the screen); an app that
    /// records ITS OWN review from the display wants them in.
    pub include_own_windows: bool,
}

impl Default for ScreenCaptureConfig {
    fn default() -> Self {
        Self {
            source: ScreenCaptureSource::PrimaryDisplay,
            fps: 0,
            output_format: RawImageFormat::BGRA8,
            show_cursor: true,
            include_own_windows: false,
        }
    }
}

impl ScreenCaptureConfig {
    /// A default config for the given `source` (backend-chosen fps, `BGRA8`).
    #[must_use]
    pub fn new(source: ScreenCaptureSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

#[cfg(test)]
#[path = "screencap_test.rs"]
mod screencap_test;
