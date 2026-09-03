//! Media keys arriving from OUTSIDE the keyboard stream.
//!
//! On Linux the desktop environment usually grabs the media keys, so
//! `XF86AudioPlay` and friends never reach the application as keysyms - the
//! 9h-i keysym table only sees them when nothing grabbed them. The transport in
//! that case is MPRIS over D-Bus, which arrives on a D-Bus thread rather than
//! in a window's event stream, so it needs a channel like the sensor and HID
//! backends have.
//!
//! What comes out is an ordinary [`VirtualKeyCode`], because that is the
//! contract every other media-key producer already follows: the Win32
//! `WM_APPCOMMAND` arm and the X11/Wayland keysym table both deliver
//! `PlayPause` as a normal key, so an app binding it works unchanged
//! everywhere.

use azul_core::window::VirtualKeyCode;

static PENDING: std::sync::Mutex<Vec<VirtualKeyCode>> = std::sync::Mutex::new(Vec::new());

/// A person cannot press play more often than this between frames; anything
/// beyond it is a stuck sender, and an unbounded queue would grow for the life
/// of the process.
const MAX_PENDING: usize = 64;

/// Park a media key delivered by a platform backend, from any thread.
pub fn push_media_key(key: VirtualKeyCode) {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_PENDING {
        return;
    }
    q.push(key);
}

/// Drain the parked media keys, in arrival order.
pub fn drain_media_keys() -> Vec<VirtualKeyCode> {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_drain_in_arrival_order_and_empty_the_queue() {
        let _ = drain_media_keys();
        push_media_key(VirtualKeyCode::PlayPause);
        push_media_key(VirtualKeyCode::NextTrack);
        let got = drain_media_keys();
        assert_eq!(got, vec![VirtualKeyCode::PlayPause, VirtualKeyCode::NextTrack]);
        assert!(drain_media_keys().is_empty());
    }

    /// A stuck sender must not grow the queue without bound.
    #[test]
    fn the_queue_is_bounded() {
        let _ = drain_media_keys();
        for _ in 0..(MAX_PENDING + 50) {
            push_media_key(VirtualKeyCode::PlayPause);
        }
        assert_eq!(drain_media_keys().len(), MAX_PENDING);
    }
}
