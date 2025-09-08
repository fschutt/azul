use core::num::ParseFloatError;

use crate::{css_properties::*, parser::*};

/// Represents a `flex-grow` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexGrow {
    pub inner: FloatValue,
}

impl Default for LayoutFlexGrow {
    fn default() -> Self {
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        }
    }
}

/// Represents a `flex-shrink` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexShrink {
    pub inner: FloatValue,
}

impl Default for LayoutFlexShrink {
    fn default() -> Self {
        LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        }
    }
}

impl_float_value!(LayoutFlexGrow);
impl_float_value!(LayoutFlexShrink);

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display! {FlexGrowParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-grow: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

impl<'a> FlexGrowParseError<'a> {
    pub fn to_contained(&self) -> FlexGrowParseErrorOwned {
        match self {
            FlexGrowParseError::ParseFloat(err, s) => {
                FlexGrowParseErrorOwned::ParseFloat(err.clone(), s.to_string())
            }
        }
    }
}

impl FlexGrowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexGrowParseError<'a> {
        match self {
            FlexGrowParseErrorOwned::ParseFloat(err, s) => {
                FlexGrowParseError::ParseFloat(err.clone(), s)
            }
        }
    }
}

pub fn parse_layout_flex_grow<'a>(
    input: &'a str,
) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexGrow { inner: o }),
        Err(e) => Err(FlexGrowParseError::ParseFloat(e, input)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display! {FlexShrinkParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-shrink: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

/// Owned version of FlexShrinkParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

impl<'a> FlexShrinkParseError<'a> {
    pub fn to_contained(&self) -> FlexShrinkParseErrorOwned {
        match self {
            FlexShrinkParseError::ParseFloat(err, s) => {
                FlexShrinkParseErrorOwned::ParseFloat(err.clone(), s.to_string())
            }
        }
    }
}

impl FlexShrinkParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexShrinkParseError<'a> {
        match self {
            FlexShrinkParseErrorOwned::ParseFloat(err, s) => {
                FlexShrinkParseError::ParseFloat(err.clone(), s.as_str())
            }
        }
    }
}

pub fn parse_layout_flex_shrink<'a>(
    input: &'a str,
) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexShrink { inner: o }),
        Err(e) => Err(FlexShrinkParseError::ParseFloat(e, input)),
    }
}
