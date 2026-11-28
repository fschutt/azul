//! The room the system keeps for itself on a phone: the status bar and the cutout at the top, the
//! home indicator or the navigation bar at the bottom, the rounded corners at the sides, and the
//! on-screen keyboard over the bottom.
//!
//! The engine reports them as one number per side ([`azul::css::SafeAreaInsets`], zero on a
//! desktop, `LayoutCallbackInfo::get_safe_area_insets`); this turns them into px a layout can
//! compute with, cleaned: a side that is not a finite, positive number of px is no inset. The
//! keyboard is separate from the bottom inset on purpose (the home indicator does not move when
//! the keyboard comes): [`Insets::lift`] is how far what sits at the bottom of the window has to
//! rise to stay in sight, [`Insets::bottom_clear`] the room the bottom bar keeps when it does
//! not have to rise.

#[cfg(feature = "look")]
use azul::{callbacks::LayoutCallbackInfo, option::OptionPixelValue};

/// The five insets of the window, in logical px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    /// The status bar and the cutout.
    pub top: f32,
    /// A rounded corner or a cutout at the right (a phone on its side).
    pub right: f32,
    /// The home indicator or the navigation bar.
    pub bottom: f32,
    /// A rounded corner or a cutout at the left.
    pub left: f32,
    /// How much of the window the on-screen keyboard covers from its bottom edge; zero while none is up.
    pub keyboard: f32,
}

impl Insets {
    /// Insets of the given sides; a side that is not a finite number of px, or is negative, is no inset.
    #[must_use]
    pub fn new(top: f32, right: f32, bottom: f32, left: f32, keyboard: f32) -> Self {
        let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        Self {
            top: clean(top),
            right: clean(right),
            bottom: clean(bottom),
            left: clean(left),
            keyboard: clean(keyboard),
        }
    }

    /// What the system reports for the window now (zero on a desktop). Reading the insets makes a
    /// change of them (the keyboard coming up, a rotation) rebuild the window.
    #[cfg(feature = "look")]
    #[must_use]
    pub fn of(info: &LayoutCallbackInfo) -> Self {
        let safe = info.get_safe_area_insets();
        let px = |v: OptionPixelValue| v.into_option().map_or(0.0, |p| p.to_pixels(0.0, 16.0, 16.0));
        Self::new(px(safe.top), px(safe.right), px(safe.bottom), px(safe.left), px(safe.keyboard))
    }

    /// Whether the window has no inset at all.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0 && self.keyboard == 0.0
    }

    /// How far what stands at the bottom of the window rises to stay out from under the keyboard:
    /// the keyboard's height, or nothing while none is up.
    #[must_use]
    pub fn lift(&self) -> f32 {
        self.keyboard
    }

    /// The room the bottom of a bar keeps clear of the home indicator or the navigation bar:
    /// nothing while the keyboard is up (it covers that part of the screen, the bar stands on it).
    #[must_use]
    pub fn bottom_clear(&self) -> f32 {
        if self.keyboard > 0.0 {
            0.0
        } else {
            self.bottom
        }
    }
}

#[cfg(test)]
#[path = "insets_test.rs"]
mod insets_test;
