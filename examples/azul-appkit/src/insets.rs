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
//!
//! WHO KEEPS CLEAR OF WHAT. The engine insets the ROOT of a window by the bars and the cutout
//! (the root's viewport is the window less the safe area, unless the app sets
//! `WindowFlags::extend_into_safe_area`, a window that draws under them): an app that does not
//! opt out has nothing of those four sides left to handle - using them again would leave the
//! gap twice - and only the keyboard, which overlays the window and is no part of the root's
//! inset, is left, less the part of it that lies over the bar the root already excludes.
//! [`Insets::inside_root`] is that. An app that DOES draw under the bars reads the raw insets
//! ([`Insets::of`]) and keeps clear itself.

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

    /// What is left for an app whose root the engine has already inset by the bars (every window that does not draw under them):
    /// the four sides are outside its root, so none of them; the keyboard, which overlays the window from its bottom edge and is
    /// no part of the root's inset, less the bottom bar (the navigation bar, the home indicator) the root already excludes - the
    /// root's lower edge is that far above the window's.
    #[must_use]
    pub fn inside_root(&self) -> Self {
        Self {
            keyboard: (self.keyboard - self.bottom).max(0.0),
            ..Self::default()
        }
    }

    /// The size of the ROOT of a window `size` px big that does not draw under the bars: the engine insets the root by the four
    /// sides, so a layout that places things against the window's size (a sheet's height, the keys above it) must use this one, or a
    /// sheet drawn up to its full height reaches over the bar at the top by the status bar's height.
    #[must_use]
    pub fn root_size(&self, size: (f32, f32)) -> (f32, f32) {
        ((size.0 - self.left - self.right).max(0.0), (size.1 - self.top - self.bottom).max(0.0))
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
