//! `border-top-left-radius` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopLeftRadius {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleBorderTopLeftRadius);

impl StyleBorderTopLeftRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl CssPropertyValue<StyleBorderTopLeftRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.scale_for_dpi(scale_factor);
        }
    }
}

pub type StyleBorderTopLeftRadiusValue = CssPropertyValue<StyleBorderTopLeftRadius>;

impl_option!(
    StyleBorderTopLeftRadius,
    OptionStyleBorderTopLeftRadius,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
