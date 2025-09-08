use crate::{css_properties::*, parser::*};

/// Represents a `word-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleWordSpacing {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleWordSpacing);

impl Default for StyleWordSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

typed_pixel_value_parser!(parse_style_word_spacing, StyleWordSpacing);
