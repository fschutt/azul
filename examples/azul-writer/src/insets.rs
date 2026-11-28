//! The room the system keeps for itself, on a phone whose window draws under the bars.
//!
//! AzWriter's window asks to draw under the system bars and into the cutout area (`ribbon::for_the_phone`): the strip of
//! tabs takes the place of the clock and the battery in the status bar's band, so the top of the window is the
//! strip's. Everything else keeps clear of the insets itself: this is that padding. On a desktop there are no insets
//! and nothing is wrapped.

use azul::{callbacks::LayoutCallbackInfo, dom::Dom, option::OptionPixelValue};

/// The four insets of the window, in logical px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    /// What the system reports for the window now (zero on a desktop).
    #[must_use]
    pub fn of(info: &LayoutCallbackInfo) -> Self {
        let safe = info.get_safe_area_insets();
        let px = |v: OptionPixelValue| v.into_option().map_or(0.0, |p| p.to_pixels(0.0, 16.0, 16.0));
        Self::new(px(safe.top), px(safe.right), px(safe.bottom), px(safe.left))
    }

    /// Insets of the given sides; a side that is not a finite number of pixels, or is negative, is no inset.
    #[must_use]
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        Self { top: clean(top), right: clean(right), bottom: clean(bottom), left: clean(left) }
    }

    /// Whether the window has no inset at all.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }

    /// The padding that keeps content clear of the insets: all four sides, or all but the top (`with_top` false:
    /// what stands under the strip, which already stands in the status bar's band).
    #[must_use]
    pub fn padding(&self, with_top: bool) -> String {
        let top = if with_top { self.top } else { 0.0 };
        format!("padding: {top}px {}px {}px {}px;", self.right, self.bottom, self.left)
    }

    /// `content` kept clear of the insets: wrapped in a box that fills what its parent leaves and is padded, or
    /// returned as it is when the window has no inset (a desktop).
    #[must_use]
    pub fn around(&self, content: Dom, with_top: bool) -> Dom {
        let needed = !self.is_none() && (with_top || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0);
        if !needed {
            return content;
        }
        Dom::create_div()
            .with_css(
                format!(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px; {}",
                    self.padding(with_top)
                )
                .as_str(),
            )
            .with_child(content)
    }
}

#[cfg(test)]
#[path = "insets_test.rs"]
mod insets_test;
