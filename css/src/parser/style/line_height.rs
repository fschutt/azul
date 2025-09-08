use crate::{css_properties::*, parser::*};

/// Represents a `line-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLineHeight {
    pub inner: PercentageValue,
}

impl_percentage_value!(StyleLineHeight);

impl Default for StyleLineHeight {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(100),
        }
    }
}

pub fn parse_style_line_height(input: &str) -> Result<StyleLineHeight, PercentageParseError> {
    parse_percentage_value(input).and_then(|e| Ok(StyleLineHeight { inner: e }))
}
