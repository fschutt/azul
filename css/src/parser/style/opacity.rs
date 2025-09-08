use crate::{css_properties::*, parser::*};

/// Represents an `opacity` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleOpacity {
    pub inner: PercentageValue,
}

impl Default for StyleOpacity {
    fn default() -> Self {
        StyleOpacity {
            inner: PercentageValue::const_new(0),
        }
    }
}

impl_percentage_value!(StyleOpacity);

#[derive(Debug, Clone, PartialEq)]
pub enum OpacityParseError<'a> {
    ParsePercentage(PercentageParseError, &'a str),
}

impl_display! {OpacityParseError<'a>, {
    ParsePercentage(e, orig_str) => format!("opacity: Could not parse percentage value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

/// Owned version of OpacityParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum OpacityParseErrorOwned {
    ParsePercentage(PercentageParseError, String),
}

impl<'a> OpacityParseError<'a> {
    pub fn to_contained(&self) -> OpacityParseErrorOwned {
        match self {
            OpacityParseError::ParsePercentage(err, s) => {
                OpacityParseErrorOwned::ParsePercentage(err.clone(), s.to_string())
            }
        }
    }
}

impl OpacityParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> OpacityParseError<'a> {
        match self {
            OpacityParseErrorOwned::ParsePercentage(err, s) => {
                OpacityParseError::ParsePercentage(err.clone(), s.as_str())
            }
        }
    }
}

pub fn parse_style_opacity<'a>(input: &'a str) -> Result<StyleOpacity, OpacityParseError<'a>> {
    parse_percentage_value(input)
        .map_err(|e| OpacityParseError::ParsePercentage(e, input))
        .and_then(|e| Ok(StyleOpacity { inner: e }))
}
