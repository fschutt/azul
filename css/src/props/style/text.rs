//! Text-related CSS properties

use crate::error::{CssColorParseError, CssParsingError, CssPixelValueParseError};
use crate::props::basic::{color::ColorU, value::PixelValue};
use crate::props::formatter::FormatAsCssValue;
use alloc::string::String;
use core::fmt;

/// CSS color property (text color)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleTextColor {
    pub inner: ColorU,
}

/// CSS font-size property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleFontSize {
    pub inner: PixelValue,
}

/// CSS text-align property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextAlign {
    Left,
    Right,
    Center,
    Justify,
}

/// CSS line-height property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleLineHeight {
    pub inner: PixelValue,
}

/// CSS letter-spacing property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleLetterSpacing {
    pub inner: PixelValue,
}

impl Default for StyleTextColor {
    fn default() -> Self {
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl Default for StyleFontSize {
    fn default() -> Self {
        Self {
            inner: PixelValue::px(16.0),
        }
    }
}

impl Default for StyleTextAlign {
    fn default() -> Self {
        StyleTextAlign::Left
    }
}

impl Default for StyleLineHeight {
    fn default() -> Self {
        Self {
            inner: PixelValue::px(1.2 * 16.0),
        }
    }
}

impl Default for StyleLetterSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl fmt::Display for StyleTextAlign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            StyleTextAlign::Left => "left",
            StyleTextAlign::Right => "right",
            StyleTextAlign::Center => "center",
            StyleTextAlign::Justify => "justify",
        };
        write!(f, "{}", s)
    }
}

impl FormatAsCssValue for StyleTextColor {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for StyleFontSize {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for StyleTextAlign {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for StyleLineHeight {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for StyleLetterSpacing {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

pub fn parse_style_text_color<'a>(
    input: &'a str,
) -> Result<StyleTextColor, CssColorParseError<'a>> {
    Ok(StyleTextColor {
        inner: crate::props::basic::color::parse_css_color(input)?,
    })
}

pub fn parse_style_font_size<'a>(
    input: &'a str,
) -> Result<StyleFontSize, CssPixelValueParseError<'a>> {
    Ok(StyleFontSize {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

pub fn parse_style_text_align<'a>(input: &'a str) -> Result<StyleTextAlign, CssParsingError<'a>> {
    match input.trim() {
        "left" => Ok(StyleTextAlign::Left),
        "right" => Ok(StyleTextAlign::Right),
        "center" => Ok(StyleTextAlign::Center),
        "justify" => Ok(StyleTextAlign::Justify),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

pub fn parse_style_line_height<'a>(
    input: &'a str,
) -> Result<StyleLineHeight, CssPixelValueParseError<'a>> {
    Ok(StyleLineHeight {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

pub fn parse_style_letter_spacing<'a>(
    input: &'a str,
) -> Result<StyleLetterSpacing, CssPixelValueParseError<'a>> {
    Ok(StyleLetterSpacing {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}
