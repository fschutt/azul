//! `padding-top` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue}; // Assuming PixelValue is still in css_properties or accessible
use crate::{impl_option}; // Assuming impl_option is a macro accessible from crate root or css_properties

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingTop {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutPaddingTop);

impl CssPropertyValue<LayoutPaddingTop> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutPaddingTopValue = CssPropertyValue<LayoutPaddingTop>;
impl_option!(
    LayoutPaddingTopValue,
    OptionLayoutPaddingTopValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
