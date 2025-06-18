//! `border-bottom-width` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderBottomWidth {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutBorderBottomWidth);

impl CssPropertyValue<LayoutBorderBottomWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutBorderBottomWidthValue = CssPropertyValue<LayoutBorderBottomWidth>;

impl_option!(
    LayoutBorderBottomWidth,
    OptionLayoutBorderBottomWidth,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
