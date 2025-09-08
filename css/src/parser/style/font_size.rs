use crate::{css_properties::*, parser::*};

/// Represents a `font-size` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFontSize {
    pub inner: PixelValue,
}

impl Default for StyleFontSize {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_em(1),
        }
    }
}

impl_pixel_value!(StyleFontSize);

typed_pixel_value_parser!(parse_style_font_size, StyleFontSize);
