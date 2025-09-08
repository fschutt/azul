//! Layout spacing properties: padding, margin, box-sizing

use crate::error::{CssParsingError, CssPixelValueParseError};
use crate::props::basic::value::PixelValue;
use crate::props::formatter::FormatAsCssValue;
use alloc::string::String;
use core::fmt;

/// CSS box-sizing property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutBoxSizing {
    ContentBox,
    BorderBox,
}

/// Padding properties
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPaddingTop {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPaddingRight {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPaddingBottom {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPaddingLeft {
    pub inner: PixelValue,
}

/// Margin properties
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMarginTop {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMarginRight {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMarginBottom {
    pub inner: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMarginLeft {
    pub inner: PixelValue,
}

/// Combined padding for all sides
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPadding {
    pub top: PixelValue,
    pub right: PixelValue,
    pub bottom: PixelValue,
    pub left: PixelValue,
}

/// Combined margin for all sides
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMargin {
    pub top: PixelValue,
    pub right: PixelValue,
    pub bottom: PixelValue,
    pub left: PixelValue,
}

impl Default for LayoutBoxSizing {
    fn default() -> Self {
        LayoutBoxSizing::ContentBox
    }
}

// Default implementations for individual sides
macro_rules! impl_defaults {
    ($($type:ident),*) => {
        $(
            impl Default for $type {
                fn default() -> Self { Self { inner: PixelValue::zero() } }
            }
        )*
    };
}

impl_defaults!(
    LayoutPaddingTop,
    LayoutPaddingRight,
    LayoutPaddingBottom,
    LayoutPaddingLeft,
    LayoutMarginTop,
    LayoutMarginRight,
    LayoutMarginBottom,
    LayoutMarginLeft
);

impl Default for LayoutPadding {
    fn default() -> Self {
        Self {
            top: PixelValue::zero(),
            right: PixelValue::zero(),
            bottom: PixelValue::zero(),
            left: PixelValue::zero(),
        }
    }
}

impl Default for LayoutMargin {
    fn default() -> Self {
        Self {
            top: PixelValue::zero(),
            right: PixelValue::zero(),
            bottom: PixelValue::zero(),
            left: PixelValue::zero(),
        }
    }
}

impl fmt::Display for LayoutBoxSizing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            LayoutBoxSizing::ContentBox => "content-box",
            LayoutBoxSizing::BorderBox => "border-box",
        };
        write!(f, "{}", s)
    }
}

// FormatAsCssValue implementations
impl FormatAsCssValue for LayoutBoxSizing {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

macro_rules! impl_format_css_value {
    ($($type:ident),*) => {
        $(
            impl FormatAsCssValue for $type {
                fn format_as_css_value(&self) -> String {
                    self.inner.format_as_css_value()
                }
            }
        )*
    };
}

impl_format_css_value!(
    LayoutPaddingTop,
    LayoutPaddingRight,
    LayoutPaddingBottom,
    LayoutPaddingLeft,
    LayoutMarginTop,
    LayoutMarginRight,
    LayoutMarginBottom,
    LayoutMarginLeft
);

impl FormatAsCssValue for LayoutPadding {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {} {} {}",
            self.top.format_as_css_value(),
            self.right.format_as_css_value(),
            self.bottom.format_as_css_value(),
            self.left.format_as_css_value()
        )
    }
}

impl FormatAsCssValue for LayoutMargin {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {} {} {}",
            self.top.format_as_css_value(),
            self.right.format_as_css_value(),
            self.bottom.format_as_css_value(),
            self.left.format_as_css_value()
        )
    }
}

// Parsing functions
pub fn parse_layout_box_sizing<'a>(input: &'a str) -> Result<LayoutBoxSizing, CssParsingError<'a>> {
    match input.trim() {
        "content-box" => Ok(LayoutBoxSizing::ContentBox),
        "border-box" => Ok(LayoutBoxSizing::BorderBox),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

macro_rules! impl_parse_functions {
    ($(($func:ident, $type:ident)),*) => {
        $(
            pub fn $func<'a>(input: &'a str) -> Result<$type, CssPixelValueParseError<'a>> {
                Ok($type { inner: crate::props::basic::value::parse_pixel_value(input)? })
            }
        )*
    };
}

impl_parse_functions!(
    (parse_layout_padding_top, LayoutPaddingTop),
    (parse_layout_padding_right, LayoutPaddingRight),
    (parse_layout_padding_bottom, LayoutPaddingBottom),
    (parse_layout_padding_left, LayoutPaddingLeft),
    (parse_layout_margin_top, LayoutMarginTop),
    (parse_layout_margin_right, LayoutMarginRight),
    (parse_layout_margin_bottom, LayoutMarginBottom),
    (parse_layout_margin_left, LayoutMarginLeft)
);

pub fn parse_layout_padding<'a>(
    input: &'a str,
) -> Result<LayoutPadding, CssPixelValueParseError<'a>> {
    let values: Vec<&str> = input.trim().split_whitespace().collect();
    match values.len() {
        1 => {
            let val = crate::props::basic::value::parse_pixel_value(values[0])?;
            Ok(LayoutPadding {
                top: val,
                right: val,
                bottom: val,
                left: val,
            })
        }
        2 => {
            let vertical = crate::props::basic::value::parse_pixel_value(values[0])?;
            let horizontal = crate::props::basic::value::parse_pixel_value(values[1])?;
            Ok(LayoutPadding {
                top: vertical,
                right: horizontal,
                bottom: vertical,
                left: horizontal,
            })
        }
        3 => {
            let top = crate::props::basic::value::parse_pixel_value(values[0])?;
            let horizontal = crate::props::basic::value::parse_pixel_value(values[1])?;
            let bottom = crate::props::basic::value::parse_pixel_value(values[2])?;
            Ok(LayoutPadding {
                top,
                right: horizontal,
                bottom,
                left: horizontal,
            })
        }
        4 => {
            let top = crate::props::basic::value::parse_pixel_value(values[0])?;
            let right = crate::props::basic::value::parse_pixel_value(values[1])?;
            let bottom = crate::props::basic::value::parse_pixel_value(values[2])?;
            let left = crate::props::basic::value::parse_pixel_value(values[3])?;
            Ok(LayoutPadding {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(CssPixelValueParseError::InvalidNumber(input)),
    }
}

pub fn parse_layout_margin<'a>(
    input: &'a str,
) -> Result<LayoutMargin, CssPixelValueParseError<'a>> {
    let values: Vec<&str> = input.trim().split_whitespace().collect();
    match values.len() {
        1 => {
            let val = crate::props::basic::value::parse_pixel_value(values[0])?;
            Ok(LayoutMargin {
                top: val,
                right: val,
                bottom: val,
                left: val,
            })
        }
        2 => {
            let vertical = crate::props::basic::value::parse_pixel_value(values[0])?;
            let horizontal = crate::props::basic::value::parse_pixel_value(values[1])?;
            Ok(LayoutMargin {
                top: vertical,
                right: horizontal,
                bottom: vertical,
                left: horizontal,
            })
        }
        3 => {
            let top = crate::props::basic::value::parse_pixel_value(values[0])?;
            let horizontal = crate::props::basic::value::parse_pixel_value(values[1])?;
            let bottom = crate::props::basic::value::parse_pixel_value(values[2])?;
            Ok(LayoutMargin {
                top,
                right: horizontal,
                bottom,
                left: horizontal,
            })
        }
        4 => {
            let top = crate::props::basic::value::parse_pixel_value(values[0])?;
            let right = crate::props::basic::value::parse_pixel_value(values[1])?;
            let bottom = crate::props::basic::value::parse_pixel_value(values[2])?;
            let left = crate::props::basic::value::parse_pixel_value(values[3])?;
            Ok(LayoutMargin {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(CssPixelValueParseError::InvalidNumber(input)),
    }
}
