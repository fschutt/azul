//! Platform camera-capture backend registration. The capture seam
//! (`azul_layout::widgets::capture_common`) calls the registered backend;
//! without one, `CameraWidget` shows a test pattern.
//!
//! Linux registers a v4l2 backend (libv4l2 dlopen'd at runtime - no static
//! link, so it cross-compiles and only fails gracefully at runtime if libv4l2
//! is absent). macOS (AVFoundation) / Windows (Media Foundation) / mobile
//! (Camera2) plug in the same way later.

#[cfg(all(target_os = "android", feature = "ndk-sys"))]
mod android;
/// TCC authorization gate shared by the AVFoundation camera + mic backends
/// and the macOS permission manager (`extra/permission/macos.rs`).
#[cfg(all(
    any(target_os = "macos", target_os = "ios"),
    feature = "objc2-av-foundation"
))]
pub mod avf_auth;
#[cfg(all(
    any(target_os = "macos", target_os = "ios"),
    feature = "objc2-av-foundation"
))]
mod avfoundation;
#[cfg(target_os = "linux")]
mod v4l2;
#[cfg(target_os = "windows")]
mod windows;

/// Register the platform camera backend with the capture seam, once. Called
/// from the per-frame layout pass (like [`super::audio::ensure_mic_backend`]),
/// guarded by a `OnceLock`. Linux registers the v4l2 (libv4l2) backend; a no-op
/// elsewhere until a per-OS backend lands (the widget keeps its test pattern).
pub fn ensure_camera_backend() {
    #[cfg(target_os = "linux")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!("[camera] registering v4l2 backend (libv4l2 → RGB24 → RGBA)");
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: v4l2::open,
                    read: v4l2::read,
                    close: v4l2::close,
                    reconfigure: None,
                },
            );
        });
    }
    #[cfg(target_os = "windows")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!(
                "[camera] registering Windows (nokhwa/Media Foundation) backend → RGBA"
            );
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: windows::open,
                    read: windows::read,
                    close: windows::close,
                    reconfigure: None,
                },
            );
        });
    }
    #[cfg(all(
        any(target_os = "macos", target_os = "ios"),
        feature = "objc2-av-foundation"
    ))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!("[camera] registering AVFoundation backend (32-BGRA → RGBA)");
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: avfoundation::open,
                    read: avfoundation::read,
                    close: avfoundation::close,
                    reconfigure: Some(avfoundation::reconfigure),
                },
            );
        });
    }
    #[cfg(all(target_os = "android", feature = "ndk-sys"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            // Registration line for parity with the other arms — this was the
            // only backend that registered without announcing itself.
            crate::plog_info!("[camera] registering Android NDK Camera2 backend → RGBA");
            azul_layout::widgets::capture_common::register_camera_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: android::open,
                    read: android::read,
                    close: android::close,
                    reconfigure: None,
                },
            );
        });
    }
}
