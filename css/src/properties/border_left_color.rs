//! `border-left-color` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{ColorU, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftColor {
    pub inner: ColorU,
}

impl StyleBorderLeftColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

derive_debug_zero!(StyleBorderLeftColor);
derive_display_zero!(StyleBorderLeftColor);

pub type StyleBorderLeftColorValue = CssPropertyValue<StyleBorderLeftColor>;

impl_option!(
    StyleBorderLeftColor,
    OptionStyleBorderLeftColor,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
