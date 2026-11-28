//! The room the system keeps for itself at the window's four edges (a camera cutout, the home
//! indicator, the system bars) and what a phone is, as every app of the kit asks them: one
//! answer, so that no app guesses its own.
//!
//! On a desktop the insets are zero and nothing is padded.

use azul::{callbacks::LayoutCallbackInfo, option::OptionPixelValue};

/// A window narrower than this is a phone's, on a touch platform, px.
pub const PHONE_MAX_WIDTH: f32 = 600.0;
/// A window shorter than this is a phone's too (a phone on its side), px.
pub const PHONE_MAX_HEIGHT: f32 = 500.0;

/// The four insets of the window, in logical px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SafeInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SafeInsets {
    /// What the system reports for the window now (zero on a desktop).
    #[must_use]
    pub fn of(info: &LayoutCallbackInfo) -> Self {
        let safe = info.get_safe_area_insets();
        let px = |v: OptionPixelValue| v.into_option().map_or(0.0, |p| p.to_pixels(0.0, 16.0, 16.0));
        Self::new(px(safe.top), px(safe.right), px(safe.bottom), px(safe.left))
    }

    /// Insets of the given sides; a side that is not a finite number of pixels, or is negative,
    /// is no inset.
    #[must_use]
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        Self {
            top: clean(top),
            right: clean(right),
            bottom: clean(bottom),
            left: clean(left),
        }
    }

    /// Whether the window has no inset at all.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }

    /// The padding that keeps content clear of the insets (`padding: top right bottom left;`).
    #[must_use]
    pub fn padding(&self) -> String {
        format!(
            "padding: {}px {}px {}px {}px;",
            self.top, self.right, self.bottom, self.left
        )
    }
}

/// Whether a window of `width` x `height` px is a phone's: a touch platform (`touch`) and a
/// window narrower than [`PHONE_MAX_WIDTH`] or shorter than [`PHONE_MAX_HEIGHT`] (a phone on its
/// side is short, a tablet is neither).
#[must_use]
pub fn is_phone_window(touch: bool, width: f32, height: f32) -> bool {
    touch && (width < PHONE_MAX_WIDTH || height < PHONE_MAX_HEIGHT)
}

/// [`is_phone_window`] for this build's platform.
#[must_use]
pub fn phone_window(width: f32, height: f32) -> bool {
    is_phone_window(cfg!(any(target_os = "android", target_os = "ios")), width, height)
}

#[cfg(test)]
#[path = "insets_test.rs"]
mod insets_test;
