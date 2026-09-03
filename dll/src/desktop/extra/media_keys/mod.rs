//! Media keys that do not arrive through the keyboard.
//!
//! - **Linux**: MPRIS over D-Bus, for the (usual) case where the desktop has
//!   grabbed the media row. Opt-in via `AppConfig::expose_mpris_media_controls`.
//! - **Windows**: nothing needed - `WM_APPCOMMAND` already delivers them.
//! - **macOS**: `MPRemoteCommandCenter`, which needs NO permission (unlike a
//!   CGEventTap) but does require becoming the "now playing" app. Same opt-in
//!   flag as Linux.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

/// Start any out-of-band media-key transport this platform has.
pub fn ensure_started() {
    #[cfg(target_os = "linux")]
    linux::start();
    #[cfg(target_os = "macos")]
    macos::start();
}
