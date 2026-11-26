//! Keeping a window out of screen captures (`WindowFlags::exclude_from_capture`): the one decision every shell
//! makes about it, and the values the platforms take. Pure: no window, no system call.
//!
//! macOS sets the window's sharing type, Windows its display affinity; X11 and Wayland give a program no
//! way to do it, so their shells do not call anything.

use azul_core::window::WindowFlags;

/// What a shell tells the system after the flags of a window changed: `None` when nothing is to be told,
/// `Some(on)` when the window is to be kept out of captures (`on`) or shown to them again (`!on`).
///
/// `previous` is `None` for a window that has just been created: one that does not ask is left as the system
/// made it, one that asks is told once.
#[must_use]
pub fn exclusion_to_apply(previous: Option<&WindowFlags>, current: &WindowFlags) -> Option<bool> {
    match previous {
        None => current.exclude_from_capture.then_some(true),
        Some(before) if before.exclude_from_capture != current.exclude_from_capture => Some(current.exclude_from_capture),
        Some(_) => None,
    }
}

/// The macOS `NSWindowSharingType` of a window that is `excluded` from captures: `NSWindowSharingNone` (0) when it
/// is, else `NSWindowSharingReadOnly` (1), the system's default.
#[must_use]
pub const fn macos_sharing_type(excluded: bool) -> usize {
    if excluded {
        0
    } else {
        1
    }
}

/// The Windows `SetWindowDisplayAffinity` value for a window that is `excluded` from captures:
/// `WDA_EXCLUDEFROMCAPTURE` (0x11) when it is, else `WDA_NONE` (0).
#[must_use]
pub const fn windows_display_affinity(excluded: bool) -> u32 {
    if excluded {
        0x11
    } else {
        0
    }
}

#[cfg(test)]
#[path = "capture_exclusion_test.rs"]
mod capture_exclusion_test;
