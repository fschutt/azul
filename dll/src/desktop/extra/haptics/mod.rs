//! Platform haptics that do not belong to a window.
//!
//! The window-owned paths (macOS `NSHapticFeedbackManager`, Android
//! `performHapticFeedback`, iOS `UIFeedbackGenerator`) live in their shells.
//! This is for actuators reached through a device API instead - today the
//! Surface Pen on Windows.

#[cfg(target_os = "windows")]
pub mod windows;
