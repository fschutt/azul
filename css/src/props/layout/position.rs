//! Layout positioning properties

use alloc::string::String;
use core::fmt;

use crate::{
    error::{CssParsingError, CssPixelValueParseError},
    props::{basic::value::PixelValue, formatter::FormatAsCssValue},
};

/// CSS position property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// Layout positioning offset properties
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutTop {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutRight {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutBottom {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutLeft {
    pub inner: PixelValue,
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

impl Default for LayoutTop {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutRight {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutBottom {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutLeft {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl fmt::Display for LayoutPosition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            LayoutPosition::Static => "static",
            LayoutPosition::Relative => "relative",
            LayoutPosition::Absolute => "absolute",
            LayoutPosition::Fixed => "fixed",
            LayoutPosition::Sticky => "sticky",
        };
        write!(f, "{}", s)
    }
}

impl FormatAsCssValue for LayoutPosition {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutTop {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutRight {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutBottom {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutLeft {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_position<'a>(input: &'a str) -> Result<LayoutPosition, CssParsingError<'a>> {
    match input.trim() {
        "static" => Ok(LayoutPosition::Static),
        "relative" => Ok(LayoutPosition::Relative),
        "absolute" => Ok(LayoutPosition::Absolute),
        "fixed" => Ok(LayoutPosition::Fixed),
        "sticky" => Ok(LayoutPosition::Sticky),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_top<'a>(input: &'a str) -> Result<LayoutTop, CssPixelValueParseError<'a>> {
    Ok(LayoutTop {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_layout_right<'a>(input: &'a str) -> Result<LayoutRight, CssPixelValueParseError<'a>> {
    Ok(LayoutRight {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_layout_bottom<'a>(
    input: &'a str,
) -> Result<LayoutBottom, CssPixelValueParseError<'a>> {
    Ok(LayoutBottom {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_layout_left<'a>(input: &'a str) -> Result<LayoutLeft, CssPixelValueParseError<'a>> {
    Ok(LayoutLeft {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}
