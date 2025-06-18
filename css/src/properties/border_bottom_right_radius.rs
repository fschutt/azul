//! `border-bottom-right-radius` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomRightRadius {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleBorderBottomRightRadius);

impl StyleBorderBottomRightRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl CssPropertyValue<StyleBorderBottomRightRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.scale_for_dpi(scale_factor);
        }
    }
}

pub type StyleBorderBottomRightRadiusValue = CssPropertyValue<StyleBorderBottomRightRadius>;

impl_option!(
    StyleBorderBottomRightRadius,
    OptionStyleBorderBottomRightRadius,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
