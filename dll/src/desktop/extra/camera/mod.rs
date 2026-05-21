//! Platform camera-capture backend registration. The capture seam
//! (`azul_layout::widgets::capture_common`) calls the registered backend;
//! without one, `CameraWidget` shows a test pattern.
//!
//! Linux registers a v4l2 backend (via `rscam` - libc ioctls, cross-compiles).
//! macOS (AVFoundation) / Windows (Media Foundation) / mobile (Camera2) plug in
//! the same way later.

#[cfg(target_os = "linux")]
mod v4l2;
#[cfg(target_os = "windows")]
mod windows;

/// Register the platform camera backend with the capture seam, once. Called
/// from the per-frame layout pass (like [`super::audio::ensure_mic_backend`]),
/// guarded by a `OnceLock`. Linux registers the v4l2 (`rscam`) backend; a no-op
/// elsewhere until a per-OS backend lands (the widget keeps its test pattern).
pub fn ensure_camera_backend() {
    #[cfg(target_os = "linux")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: v4l2::open,
                    read: v4l2::read,
                    close: v4l2::close,
                },
            );
        });
    }
    #[cfg(target_os = "windows")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: windows::open,
                    read: windows::read,
                    close: windows::close,
                },
            );
        });
    }
}
