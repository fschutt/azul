//! `padding-left` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingLeft {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutPaddingLeft);

impl CssPropertyValue<LayoutPaddingLeft> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

pub type LayoutPaddingLeftValue = CssPropertyValue<LayoutPaddingLeft>;
impl_option!(
    LayoutPaddingLeftValue,
    OptionLayoutPaddingLeftValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
