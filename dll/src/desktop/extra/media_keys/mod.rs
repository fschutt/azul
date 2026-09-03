//! Media keys that do not arrive through the keyboard.
//!
//! - **Linux**: MPRIS over D-Bus, for the (usual) case where the desktop has
//!   grabbed the media row. Opt-in via `AppConfig::expose_mpris_media_controls`.
//! - **Windows**: nothing needed - `WM_APPCOMMAND` already delivers them.
//! - **macOS**: `NSEventTypeSystemDefined` needs a CGEventTap and the
//!   accessibility permission - 9h-i-b.

#[cfg(target_os = "linux")]
pub mod linux;

/// Start any out-of-band media-key transport this platform has.
pub fn ensure_started() {
    #[cfg(target_os = "linux")]
    linux::start();
}
