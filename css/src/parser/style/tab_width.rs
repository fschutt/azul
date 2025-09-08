use crate::{css_properties::*, parser::*};

/// Represents a `tab-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTabWidth {
    pub inner: PercentageValue,
}

impl_percentage_value!(StyleTabWidth);

impl Default for StyleTabWidth {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(100),
        }
    }
}

pub fn parse_style_tab_width(input: &str) -> Result<StyleTabWidth, PercentageParseError> {
    parse_percentage_value(input).and_then(|e| Ok(StyleTabWidth { inner: e }))
}
