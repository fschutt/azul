//! `border-bottom-color` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{ColorU, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomColor {
    pub inner: ColorU,
}

impl StyleBorderBottomColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

derive_debug_zero!(StyleBorderBottomColor);
derive_display_zero!(StyleBorderBottomColor);

pub type StyleBorderBottomColorValue = CssPropertyValue<StyleBorderBottomColor>;

impl_option!(
    StyleBorderBottomColor,
    OptionStyleBorderBottomColor,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
