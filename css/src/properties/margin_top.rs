//! `margin-top` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue}; // Assuming PixelValue is still in css_properties or accessible
use crate::{impl_option}; // Assuming impl_option is a macro accessible from crate root

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginTop {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutMarginTop);

impl CssPropertyValue<LayoutMarginTop> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutMarginTopValue = CssPropertyValue<LayoutMarginTop>;
impl_option!(
    LayoutMarginTopValue,
    OptionLayoutMarginTopValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
