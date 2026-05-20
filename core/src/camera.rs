//! POD types for the camera-capture surface
//! (SUPER_PLAN_2 §4 Priority 6 + research/01).
//!
//! Camera frames are GPU textures, not scalar samples, so the stateful side
//! is heavier than the sensors': `azul_layout::managers::camera` owns a
//! `CameraStream` per capture, each holding a shared `ImageRef` texture the
//! capture thread writes into (zero-copy - clones see new bytes via the
//! `ImageRef` `Arc`). A `CameraPreview` node renders that texture and, by
//! appearing in the DOM, declares "I need the camera" to the permission
//! layer (research/01 §"permission-as-DOM").
//!
//! Defined here in `azul-core` so the config / id / status types cross the
//! FFI without `azul-layout` (or AVFoundation / Camera2) as a dependency -
//! these are what an app passes to `start_camera` and reads back from a
//! stream. The `Nv12` zero-copy output format is a `RawImageFormat` addition
//! deferred to the backend tick; configs default to `BGRA8`.

use crate::resources::RawImageFormat;

/// Identifies one camera capture stream - assigned by `start_camera`, used
/// to read the stream back (`get_camera_frame`) and to stop / pause / flip it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaptureStreamId {
    pub id: u64,
}

/// Which physical camera to open.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CameraFacing {
    /// User-facing (selfie) camera.
    Front,
    /// World-facing (rear) camera.
    Back,
    /// An external / USB camera (desktop webcams report here).
    External,
}

/// Lifecycle of a capture stream.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StreamState {
    /// Opening the device / negotiating the format.
    Starting,
    /// Delivering frames.
    Running,
    /// Temporarily suspended (app backgrounded, `pause_camera`).
    Paused,
    /// Stopped by the app (`stop_camera`) or torn down.
    Stopped,
    /// Failed - see the stream's [`CaptureErrorCode`].
    Error,
}

/// Rotation / mirroring the capture needs relative to the display (the
/// sensor's native orientation rarely matches the UI's).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaptureOrientation {
    /// Upright (0°).
    Up,
    /// Upside down (180°).
    Down,
    /// Rotated 90° counter-clockwise.
    Left,
    /// Rotated 90° clockwise.
    Right,
    /// Horizontally mirrored (typical for the front camera).
    Mirror,
}

/// Why a capture stream failed ([`StreamState::Error`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaptureErrorCode {
    /// The user denied (or hasn't granted) camera permission.
    PermissionDenied,
    /// No camera matched the requested [`CameraFacing`].
    DeviceUnavailable,
    /// The device disappeared mid-capture (unplugged / claimed).
    DeviceLost,
    /// The requested format / resolution isn't supported.
    Unsupported,
    /// A platform error not covered above.
    Internal,
}

/// Requested capture configuration - the input to `start_camera`. Zero
/// `width`/`height`/`fps` mean "let the backend pick its default".
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
    /// Which camera to open.
    pub facing: CameraFacing,
    /// Preferred frame width in px (0 = backend default).
    pub width: u32,
    /// Preferred frame height in px (0 = backend default).
    pub height: u32,
    /// Preferred frame rate (0 = backend default).
    pub fps: u32,
    /// Texture format the backend should deliver. `BGRA8` is the portable
    /// default; `Nv12` (a later `RawImageFormat` addition) is the zero-copy
    /// path on platforms that produce it natively.
    pub output_format: RawImageFormat,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            facing: CameraFacing::Back,
            width: 0,
            height: 0,
            fps: 0,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl CameraConfig {
    /// A default config for the given `facing` (backend-chosen size/fps,
    /// `BGRA8`).
    pub fn new(facing: CameraFacing) -> Self {
        Self {
            facing,
            ..Self::default()
        }
    }
}

/// Runtime stats for a capture stream - surfaced for HUD / debugging.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureStats {
    /// Measured delivery rate (frames/s), smoothed by the backend.
    pub measured_fps: f32,
    /// Frames delivered to the texture since the stream started.
    pub frames_delivered: u64,
    /// Frames the backend dropped (couldn't keep up / late).
    pub frames_dropped: u64,
}

impl Default for CaptureStats {
    fn default() -> Self {
        Self {
            measured_fps: 0.0,
            frames_delivered: 0,
            frames_dropped: 0,
        }
    }
}
