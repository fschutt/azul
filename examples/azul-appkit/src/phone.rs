//! What a phone is, as every app of the kit asks it: one answer, so that no app guesses its own.
//!
//! The room the system keeps at the window's edges (a camera cutout, the home indicator, the system
//! bars) is the engine's: the root of the DOM is laid out in the window minus the safe area, so an
//! app pads nothing for it (`WindowFlags::extend_into_safe_area` opts out).

use azul::callbacks::LayoutCallbackInfo;

/// A window narrower than this is a phone's, on a touch platform, px.
pub const PHONE_MAX_WIDTH: f32 = 600.0;
/// A window shorter than this is a phone's too (a phone on its side), px.
pub const PHONE_MAX_HEIGHT: f32 = 500.0;

/// Whether a window of `width` x `height` px is a phone's: a touch platform (`touch`) and a
/// window narrower than [`PHONE_MAX_WIDTH`] or shorter than [`PHONE_MAX_HEIGHT`] (a phone on its
/// side is short, a tablet is neither).
#[must_use]
pub fn is_phone_window(touch: bool, width: f32, height: f32) -> bool {
    touch && (width < PHONE_MAX_WIDTH || height < PHONE_MAX_HEIGHT)
}

/// [`is_phone_window`] for this build's platform and the window the layout callback is asked for.
/// The size goes through the callback's RECORDED queries, so a resize that flips the answer builds
/// the window again (reading the size itself would leave the DOM stale across exactly that
/// resize); a desktop asks nothing, so no resize of its window rebuilds for this.
#[must_use]
pub fn phone_window(info: &LayoutCallbackInfo) -> bool {
    if !cfg!(any(target_os = "android", target_os = "ios")) {
        return false;
    }
    // Both are asked every time, so both are recorded.
    let narrow = info.window_width_less_than(PHONE_MAX_WIDTH);
    let short = info.window_height_less_than(PHONE_MAX_HEIGHT);
    narrow || short
}

#[cfg(test)]
#[path = "phone_test.rs"]
mod phone_test;
