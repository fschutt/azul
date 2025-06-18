//! `padding-right` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingRight {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutPaddingRight);

impl CssPropertyValue<LayoutPaddingRight> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutPaddingRightValue = CssPropertyValue<LayoutPaddingRight>;
impl_option!(
    LayoutPaddingRightValue,
    OptionLayoutPaddingRightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
