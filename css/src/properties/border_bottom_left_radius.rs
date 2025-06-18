//! `border-bottom-left-radius` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomLeftRadius {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleBorderBottomLeftRadius);

impl StyleBorderBottomLeftRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl CssPropertyValue<StyleBorderBottomLeftRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.scale_for_dpi(scale_factor);
        }
    }
}

pub type StyleBorderBottomLeftRadiusValue = CssPropertyValue<StyleBorderBottomLeftRadius>;

impl_option!(
    StyleBorderBottomLeftRadius,
    OptionStyleBorderBottomLeftRadius,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
