//! `border-top-color` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{ColorU, derive_debug_zero, derive_display_zero};
use crate::{impl_option}; // Assuming impl_option is a macro accessible from crate root

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopColor {
    pub inner: ColorU,
}

impl StyleBorderTopColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

derive_debug_zero!(StyleBorderTopColor);
derive_display_zero!(StyleBorderTopColor);

pub type StyleBorderTopColorValue = CssPropertyValue<StyleBorderTopColor>;

// Add impl_option for the base struct if it can be optional directly
impl_option!(
    StyleBorderTopColor,
    OptionStyleBorderTopColor,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
