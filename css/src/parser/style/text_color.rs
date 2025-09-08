use crate::{css_properties::*, parser::*};

/// Represents a `color` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextColor {
    pub inner: ColorU,
}

derive_debug_zero!(StyleTextColor);
derive_display_zero!(StyleTextColor);

impl StyleTextColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

pub fn parse_style_text_color<'a>(
    input: &'a str,
) -> Result<StyleTextColor, CssColorParseError<'a>> {
    parse_css_color(input).and_then(|ok| Ok(StyleTextColor { inner: ok }))
}
