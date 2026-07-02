//! Screen-capture backends for `ScreenCaptureWidget` (the `azul-screenshare`
//! demo). Registers a per-OS [`CaptureVTable`] via `register_screen_backend`
//! so the widget pulls REAL frames instead of its moving-band test pattern.
//!
//! Linux: xdg-desktop-portal **ScreenCast** (the same dialog-driven flow OBS
//! and browsers use — works on KDE/GNOME, X11 and Wayland) handing off to a
//! **PipeWire** video stream (dlopen'd `libpipewire-0.3.so.0`, no link-time
//! dependency). macOS: **ScreenCaptureKit**, dlopen'd at runtime (macOS 12.3+,
//! no link-time dependency — older systems keep the test pattern). Windows
//! (DXGI duplication) is a follow-up.

#[cfg(target_os = "linux")]
mod dmabuf;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(target_os = "macos", feature = "objc2-av-foundation"))]
mod macos;

/// Idempotently register the platform screen-capture backend. Called from the
/// per-frame layout pass next to `ensure_camera_backend` / `ensure_mic_backend`.
pub fn ensure_screen_backend() {
    #[cfg(target_os = "linux")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!(
                "[screencap] registering xdg-desktop-portal ScreenCast + PipeWire backend"
            );
            azul_layout::widgets::capture_common::register_screen_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: linux::open,
                    read: linux::read,
                    close: linux::close,
                },
            );
        });
    }
    #[cfg(all(target_os = "macos", feature = "objc2-av-foundation"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!(
                "[screencap] registering ScreenCaptureKit backend (runtime-loaded, macOS 12.3+)"
            );
            azul_layout::widgets::capture_common::register_screen_backend(
                azul_layout::widgets::capture_common::CaptureVTable {
                    open: macos::open,
                    read: macos::read,
                    close: macos::close,
                },
            );
        });
    }
}
