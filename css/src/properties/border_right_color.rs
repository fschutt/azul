//! `border-right-color` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{ColorU, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightColor {
    pub inner: ColorU,
}

impl StyleBorderRightColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

derive_debug_zero!(StyleBorderRightColor);
derive_display_zero!(StyleBorderRightColor);

pub type StyleBorderRightColorValue = CssPropertyValue<StyleBorderRightColor>;

impl_option!(
    StyleBorderRightColor,
    OptionStyleBorderRightColor,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
