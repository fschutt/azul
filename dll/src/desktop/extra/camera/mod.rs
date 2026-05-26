//! Platform camera-capture backend registration. The capture seam
//! (`azul_layout::widgets::capture_common`) calls the registered backend;
//! without one, `CameraWidget` shows a test pattern.
//!
//! Linux registers a v4l2 backend (libv4l2 dlopen'd at runtime - no static
//! link, so it cross-compiles and only fails gracefully at runtime if libv4l2
//! is absent). macOS (AVFoundation) / Windows (Media Foundation) / mobile
//! (Camera2) plug in the same way later.

#[cfg(target_os = "linux")]
mod v4l2;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "objc2-av-foundation"))]
mod avfoundation;
#[cfg(all(target_os = "android", feature = "ndk-sys"))]
mod android;

/// Register the platform camera backend with the capture seam, once. Called
/// from the per-frame layout pass (like [`super::audio::ensure_mic_backend`]),
/// guarded by a `OnceLock`. Linux registers the v4l2 (libv4l2) backend; a no-op
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
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "objc2-av-foundation"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: avfoundation::open,
                    read: avfoundation::read,
                    close: avfoundation::close,
                },
            );
        });
    }
    #[cfg(all(target_os = "android", feature = "ndk-sys"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: android::open,
                    read: android::read,
                    close: android::close,
                },
            );
        });
    }
}
