//! `border-right-width` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderRightWidth {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutBorderRightWidth);

impl CssPropertyValue<LayoutBorderRightWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutBorderRightWidthValue = CssPropertyValue<LayoutBorderRightWidth>;

impl_option!(
    LayoutBorderRightWidth,
    OptionLayoutBorderRightWidth,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
