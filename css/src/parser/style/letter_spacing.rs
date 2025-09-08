use crate::{css_properties::*, parser::*};

/// Represents a `letter-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLetterSpacing {
    pub inner: PixelValue,
}

impl Default for StyleLetterSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

impl_pixel_value!(StyleLetterSpacing);

typed_pixel_value_parser!(parse_style_letter_spacing, StyleLetterSpacing);
