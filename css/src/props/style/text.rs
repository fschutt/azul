//! CSS properties for styling text.

use alloc::string::{String, ToString};
use core::fmt;

use crate::props::{
    basic::{
        error::{InvalidValueErr, InvalidValueErrOwned},
        length::{PercentageParseError, PercentageParseErrorOwned, PercentageValue},
        pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
        ColorU, Duration,
    },
    formatter::PrintAsCssValue,
    macros::PixelValueTaker,
};

// -- StyleTextColor (color property) --
// NOTE: `color` is a text property, but the `ColorU` type itself is in `basic/color.rs`.
// This is a newtype wrapper for type safety.

/// Represents a `color` attribute.
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextColor {
    pub inner: crate::props::basic::color::ColorU,
}

impl fmt::Debug for StyleTextColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl StyleTextColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl PrintAsCssValue for StyleTextColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

// -- StyleTextAlign --

/// Horizontal text alignment enum (left, center, right) - default: `Left`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextAlign {
    Left,
    Center,
    Right,
    Justify,
    Start,
    End,
}

impl Default for StyleTextAlign {
    fn default() -> Self {
        StyleTextAlign::Left
    }
}

impl_option!(
    StyleTextAlign,
    OptionStyleTextAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl PrintAsCssValue for StyleTextAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleTextAlign::Left => "left",
            StyleTextAlign::Center => "center",
            StyleTextAlign::Right => "right",
            StyleTextAlign::Justify => "justify",
            StyleTextAlign::Start => "start",
            StyleTextAlign::End => "end",
        })
    }
}

// -- StyleLetterSpacing --

/// Represents a `letter-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLetterSpacing {
    pub inner: PixelValue,
}

impl fmt::Debug for StyleLetterSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
impl Default for StyleLetterSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}
impl_pixel_value!(StyleLetterSpacing);
impl PixelValueTaker for StyleLetterSpacing {
    fn from_pixel_value(inner: PixelValue) -> Self {
        Self { inner }
    }
}
impl PrintAsCssValue for StyleLetterSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// -- StyleWordSpacing --

/// Represents a `word-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleWordSpacing {
    pub inner: PixelValue,
}

impl fmt::Debug for StyleWordSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
impl Default for StyleWordSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}
impl_pixel_value!(StyleWordSpacing);
impl PixelValueTaker for StyleWordSpacing {
    fn from_pixel_value(inner: PixelValue) -> Self {
        Self { inner }
    }
}
impl PrintAsCssValue for StyleWordSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// -- StyleLineHeight --

/// Represents a `line-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLineHeight {
    pub inner: PercentageValue,
}
impl Default for StyleLineHeight {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(120),
        }
    }
}
impl_percentage_value!(StyleLineHeight);
impl PrintAsCssValue for StyleLineHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// -- StyleTabWidth --

/// Represents a `tab-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTabWidth {
    pub inner: PixelValue, // Can be a number (space characters, em-based) or a length
}

impl fmt::Debug for StyleTabWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
impl Default for StyleTabWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::em(8.0),
        }
    }
}
impl_pixel_value!(StyleTabWidth);
impl PixelValueTaker for StyleTabWidth {
    fn from_pixel_value(inner: PixelValue) -> Self {
        Self { inner }
    }
}
impl PrintAsCssValue for StyleTabWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// -- StyleWhiteSpace --

/// How to handle white space inside an element.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleWhiteSpace {
    Normal,
    Pre,
    Nowrap,
}
impl Default for StyleWhiteSpace {
    fn default() -> Self {
        StyleWhiteSpace::Normal
    }
}
impl_option!(
    StyleWhiteSpace,
    OptionStyleWhiteSpace,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleWhiteSpace {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleWhiteSpace::Normal => "normal",
            StyleWhiteSpace::Pre => "pre",
            StyleWhiteSpace::Nowrap => "nowrap",
        })
    }
}

// -- StyleHyphens --

/// Hyphenation rules.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleHyphens {
    Auto,
    None,
}
impl Default for StyleHyphens {
    fn default() -> Self {
        StyleHyphens::None
    }
}
impl_option!(
    StyleHyphens,
    OptionStyleHyphens,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleHyphens {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleHyphens::Auto => "auto",
            StyleHyphens::None => "none",
        })
    }
}

// -- StyleDirection --

/// Text direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleDirection {
    Ltr,
    Rtl,
}
impl Default for StyleDirection {
    fn default() -> Self {
        StyleDirection::Ltr
    }
}
impl_option!(
    StyleDirection,
    OptionStyleDirection,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleDirection {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleDirection::Ltr => "ltr",
            StyleDirection::Rtl => "rtl",
        })
    }
}

// -- StyleVerticalAlign --

/// Vertical text alignment enum (top, center, bottom) - default: `Top`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVerticalAlign {
    Top,
    Center,
    Bottom,
}
impl Default for StyleVerticalAlign {
    fn default() -> Self {
        StyleVerticalAlign::Top
    }
}
impl PrintAsCssValue for StyleVerticalAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleVerticalAlign::Top => "top",
            StyleVerticalAlign::Center => "center",
            StyleVerticalAlign::Bottom => "bottom",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
use crate::props::basic::{
    color::{parse_css_color, CssColorParseError, CssColorParseErrorOwned},
    DurationParseError,
};

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextColorParseError<'a> {
    ColorParseError(CssColorParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextColorParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextColorParseError<'a>, {
    ColorParseError(e) => format!("Invalid color: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssColorParseError<'a>,
    StyleTextColorParseError::ColorParseError
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleTextColorParseErrorOwned {
    ColorParseError(CssColorParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextColorParseError<'a> {
    pub fn to_contained(&self) -> StyleTextColorParseErrorOwned {
        match self {
            Self::ColorParseError(e) => {
                StyleTextColorParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextColorParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextColorParseError<'a> {
        match self {
            Self::ColorParseError(e) => StyleTextColorParseError::ColorParseError(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_color(input: &str) -> Result<StyleTextColor, StyleTextColorParseError> {
    parse_css_color(input)
        .map(|inner| StyleTextColor { inner })
        .map_err(|e| StyleTextColorParseError::ColorParseError(e))
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextAlignParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextAlignParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextAlignParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-align value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleTextAlignParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleTextAlignParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextAlignParseError<'a> {
    pub fn to_contained(&self) -> StyleTextAlignParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextAlignParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextAlignParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextAlignParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleTextAlignParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_align(input: &str) -> Result<StyleTextAlign, StyleTextAlignParseError> {
    match input.trim() {
        "left" => Ok(StyleTextAlign::Left),
        "center" => Ok(StyleTextAlign::Center),
        "right" => Ok(StyleTextAlign::Right),
        "justify" => Ok(StyleTextAlign::Justify),
        "start" => Ok(StyleTextAlign::Start),
        "end" => Ok(StyleTextAlign::End),
        other => Err(StyleTextAlignParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleLetterSpacingParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleLetterSpacingParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleLetterSpacingParseError<'a>, {
    PixelValue(e) => format!("Invalid letter-spacing value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    StyleLetterSpacingParseError::PixelValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleLetterSpacingParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleLetterSpacingParseError<'a> {
    pub fn to_contained(&self) -> StyleLetterSpacingParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleLetterSpacingParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLetterSpacingParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleLetterSpacingParseError<'a> {
        match self {
            Self::PixelValue(e) => StyleLetterSpacingParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_letter_spacing(
    input: &str,
) -> Result<StyleLetterSpacing, StyleLetterSpacingParseError> {
    crate::props::basic::pixel::parse_pixel_value(input)
        .map(|inner| StyleLetterSpacing { inner })
        .map_err(|e| StyleLetterSpacingParseError::PixelValue(e))
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleWordSpacingParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleWordSpacingParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleWordSpacingParseError<'a>, {
    PixelValue(e) => format!("Invalid word-spacing value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    StyleWordSpacingParseError::PixelValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleWordSpacingParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleWordSpacingParseError<'a> {
    pub fn to_contained(&self) -> StyleWordSpacingParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleWordSpacingParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleWordSpacingParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleWordSpacingParseError<'a> {
        match self {
            Self::PixelValue(e) => StyleWordSpacingParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_word_spacing(
    input: &str,
) -> Result<StyleWordSpacing, StyleWordSpacingParseError> {
    crate::props::basic::pixel::parse_pixel_value(input)
        .map(|inner| StyleWordSpacing { inner })
        .map_err(|e| StyleWordSpacingParseError::PixelValue(e))
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleLineHeightParseError {
    Percentage(PercentageParseError),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleLineHeightParseError);
#[cfg(feature = "parser")]
impl_display! { StyleLineHeightParseError, {
    Percentage(e) => format!("Invalid line-height value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(PercentageParseError, StyleLineHeightParseError::Percentage);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleLineHeightParseErrorOwned {
    Percentage(PercentageParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleLineHeightParseError {
    pub fn to_contained(&self) -> StyleLineHeightParseErrorOwned {
        match self {
            Self::Percentage(e) => StyleLineHeightParseErrorOwned::Percentage(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLineHeightParseErrorOwned {
    pub fn to_shared(&self) -> StyleLineHeightParseError {
        match self {
            Self::Percentage(e) => StyleLineHeightParseError::Percentage(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_line_height(input: &str) -> Result<StyleLineHeight, StyleLineHeightParseError> {
    crate::props::basic::length::parse_percentage_value(input)
        .map(|inner| StyleLineHeight { inner })
        .map_err(|e| StyleLineHeightParseError::Percentage(e))
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTabWidthParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTabWidthParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTabWidthParseError<'a>, {
    PixelValue(e) => format!("Invalid tab-width value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    StyleTabWidthParseError::PixelValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleTabWidthParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTabWidthParseError<'a> {
    pub fn to_contained(&self) -> StyleTabWidthParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleTabWidthParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTabWidthParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTabWidthParseError<'a> {
        match self {
            Self::PixelValue(e) => StyleTabWidthParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_tab_width(input: &str) -> Result<StyleTabWidth, StyleTabWidthParseError> {
    if let Ok(number) = input.trim().parse::<f32>() {
        Ok(StyleTabWidth {
            inner: PixelValue::em(number),
        })
    } else {
        crate::props::basic::pixel::parse_pixel_value(input)
            .map(|v| StyleTabWidth { inner: v })
            .map_err(|e| StyleTabWidthParseError::PixelValue(e))
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleWhiteSpaceParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleWhiteSpaceParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleWhiteSpaceParseError<'a>, {
    InvalidValue(e) => format!("Invalid white-space value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleWhiteSpaceParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleWhiteSpaceParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleWhiteSpaceParseError<'a> {
    pub fn to_contained(&self) -> StyleWhiteSpaceParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleWhiteSpaceParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleWhiteSpaceParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleWhiteSpaceParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleWhiteSpaceParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_white_space(input: &str) -> Result<StyleWhiteSpace, StyleWhiteSpaceParseError> {
    match input.trim() {
        "normal" => Ok(StyleWhiteSpace::Normal),
        "pre" => Ok(StyleWhiteSpace::Pre),
        "nowrap" => Ok(StyleWhiteSpace::Nowrap),
        other => Err(StyleWhiteSpaceParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleHyphensParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleHyphensParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleHyphensParseError<'a>, {
    InvalidValue(e) => format!("Invalid hyphens value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleHyphensParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleHyphensParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleHyphensParseError<'a> {
    pub fn to_contained(&self) -> StyleHyphensParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleHyphensParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHyphensParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleHyphensParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleHyphensParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_hyphens(input: &str) -> Result<StyleHyphens, StyleHyphensParseError> {
    match input.trim() {
        "auto" => Ok(StyleHyphens::Auto),
        "none" => Ok(StyleHyphens::None),
        other => Err(StyleHyphensParseError::InvalidValue(InvalidValueErr(other))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleDirectionParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleDirectionParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleDirectionParseError<'a>, {
    InvalidValue(e) => format!("Invalid direction value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleDirectionParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleDirectionParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleDirectionParseError<'a> {
    pub fn to_contained(&self) -> StyleDirectionParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleDirectionParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleDirectionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleDirectionParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleDirectionParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_direction(input: &str) -> Result<StyleDirection, StyleDirectionParseError> {
    match input.trim() {
        "ltr" => Ok(StyleDirection::Ltr),
        "rtl" => Ok(StyleDirection::Rtl),
        other => Err(StyleDirectionParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleVerticalAlignParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleVerticalAlignParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleVerticalAlignParseError<'a>, {
    InvalidValue(e) => format!("Invalid vertical-align value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(
    InvalidValueErr<'a>,
    StyleVerticalAlignParseError::InvalidValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleVerticalAlignParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleVerticalAlignParseError<'a> {
    pub fn to_contained(&self) -> StyleVerticalAlignParseErrorOwned {
        match self {
            Self::InvalidValue(e) => {
                StyleVerticalAlignParseErrorOwned::InvalidValue(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleVerticalAlignParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleVerticalAlignParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleVerticalAlignParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_vertical_align(
    input: &str,
) -> Result<StyleVerticalAlign, StyleVerticalAlignParseError> {
    match input.trim() {
        "top" => Ok(StyleVerticalAlign::Top),
        "center" => Ok(StyleVerticalAlign::Center),
        "bottom" => Ok(StyleVerticalAlign::Bottom),
        other => Err(StyleVerticalAlignParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

// --- CaretColor ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CaretColor {
    pub inner: ColorU,
}

impl Default for CaretColor {
    fn default() -> Self {
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl PrintAsCssValue for CaretColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl crate::format_rust_code::FormatAsRustCode for CaretColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretColor {{ inner: {} }}",
            crate::format_rust_code::format_color_value(&self.inner)
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_caret_color(input: &str) -> Result<CaretColor, CssColorParseError> {
    parse_css_color(input).map(|inner| CaretColor { inner })
}

// --- CaretAnimationDuration ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CaretAnimationDuration {
    pub inner: Duration,
}

impl Default for CaretAnimationDuration {
    fn default() -> Self {
        Self {
            inner: Duration { inner: 500 },
        } // Default 500ms blink time
    }
}

impl PrintAsCssValue for CaretAnimationDuration {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

impl crate::format_rust_code::FormatAsRustCode for CaretAnimationDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretAnimationDuration {{ inner: {} }}",
            self.inner.format_as_rust_code(0)
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_caret_animation_duration(
    input: &str,
) -> Result<CaretAnimationDuration, DurationParseError> {
    use crate::props::basic::parse_duration;

    parse_duration(input).map(|inner| CaretAnimationDuration { inner })
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::{color::ColorU, length::PercentageValue, pixel::PixelValue};

    #[test]
    fn test_parse_style_text_color() {
        assert_eq!(
            parse_style_text_color("red").unwrap().inner,
            ColorU::new_rgb(255, 0, 0)
        );
        assert_eq!(
            parse_style_text_color("#aabbcc").unwrap().inner,
            ColorU::new_rgb(170, 187, 204)
        );
        assert!(parse_style_text_color("not-a-color").is_err());
    }

    #[test]
    fn test_parse_style_text_align() {
        assert_eq!(
            parse_style_text_align("left").unwrap(),
            StyleTextAlign::Left
        );
        assert_eq!(
            parse_style_text_align("center").unwrap(),
            StyleTextAlign::Center
        );
        assert_eq!(
            parse_style_text_align("right").unwrap(),
            StyleTextAlign::Right
        );
        assert_eq!(
            parse_style_text_align("justify").unwrap(),
            StyleTextAlign::Justify
        );
        assert_eq!(
            parse_style_text_align("start").unwrap(),
            StyleTextAlign::Start
        );
        assert_eq!(parse_style_text_align("end").unwrap(), StyleTextAlign::End);
        assert!(parse_style_text_align("middle").is_err());
    }

    #[test]
    fn test_parse_spacing() {
        assert_eq!(
            parse_style_letter_spacing("2px").unwrap().inner,
            PixelValue::px(2.0)
        );
        assert_eq!(
            parse_style_letter_spacing("-0.1em").unwrap().inner,
            PixelValue::em(-0.1)
        );
        assert_eq!(
            parse_style_word_spacing("0.5em").unwrap().inner,
            PixelValue::em(0.5)
        );
    }

    #[test]
    fn test_parse_line_height() {
        assert_eq!(
            parse_style_line_height("1.5").unwrap().inner,
            PercentageValue::new(150.0)
        );
        assert_eq!(
            parse_style_line_height("120%").unwrap().inner,
            PercentageValue::new(120.0)
        );
        assert!(parse_style_line_height("20px").is_err()); // lengths not supported by this parser
    }

    #[test]
    fn test_parse_tab_width() {
        // Unitless number is treated as `em`
        assert_eq!(
            parse_style_tab_width("4").unwrap().inner,
            PixelValue::em(4.0)
        );
        assert_eq!(
            parse_style_tab_width("20px").unwrap().inner,
            PixelValue::px(20.0)
        );
    }

    #[test]
    fn test_parse_white_space() {
        assert_eq!(
            parse_style_white_space("normal").unwrap(),
            StyleWhiteSpace::Normal
        );
        assert_eq!(
            parse_style_white_space("pre").unwrap(),
            StyleWhiteSpace::Pre
        );
        assert_eq!(
            parse_style_white_space("nowrap").unwrap(),
            StyleWhiteSpace::Nowrap
        );
        assert!(parse_style_white_space("pre-wrap").is_err());
    }
}
