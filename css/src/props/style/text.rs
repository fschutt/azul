//! CSS properties for styling text.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{
    format_rust_code::FormatAsRustCode,
    props::{
        basic::{
            error::{InvalidValueErr, InvalidValueErrOwned},
            length::{PercentageParseError, PercentageParseErrorOwned, PercentageValue},
            pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
            ColorU, CssDuration,
        },
        formatter::PrintAsCssValue,
        macros::PixelValueTaker,
    },
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
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextAlign {
    Left,
    Center,
    Right,
    Justify,
    #[default]
    Start,
    End,
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

// -- StyleTabSize --

/// Represents a `tab-size` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTabSize {
    pub inner: PixelValue, // Can be a number (space characters, em-based) or a length
}

impl fmt::Debug for StyleTabSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
impl Default for StyleTabSize {
    fn default() -> Self {
        Self {
            inner: PixelValue::em(8.0),
        }
    }
}
impl_pixel_value!(StyleTabSize);
impl PixelValueTaker for StyleTabSize {
    fn from_pixel_value(inner: PixelValue) -> Self {
        Self { inner }
    }
}
impl PrintAsCssValue for StyleTabSize {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// -- StyleWhiteSpace --

/// How to handle white space inside an element.
/// 
/// CSS Text Level 3: https://www.w3.org/TR/css-text-3/#white-space-property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleWhiteSpace {
    /// Collapse whitespace, wrap lines
    Normal,
    /// Preserve whitespace, no wrap (except for explicit breaks)
    Pre,
    /// Collapse whitespace, no wrap
    Nowrap,
    /// Preserve whitespace, wrap lines
    PreWrap,
    /// Collapse whitespace (except newlines), wrap lines
    PreLine,
    /// Preserve whitespace, allow breaking at spaces
    BreakSpaces,
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
            StyleWhiteSpace::PreWrap => "pre-wrap",
            StyleWhiteSpace::PreLine => "pre-line",
            StyleWhiteSpace::BreakSpaces => "break-spaces",
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

// -- StyleUserSelect --

/// Controls whether the user can select text.
/// Used to prevent accidental text selection on UI controls like buttons.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleUserSelect {
    /// Browser determines selectability (default)
    Auto,
    /// Text is selectable
    Text,
    /// Text is not selectable
    None,
    /// User can select all text with a single action
    All,
}
impl Default for StyleUserSelect {
    fn default() -> Self {
        StyleUserSelect::Auto
    }
}
impl_option!(
    StyleUserSelect,
    OptionStyleUserSelect,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleUserSelect {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleUserSelect::Auto => "auto",
            StyleUserSelect::Text => "text",
            StyleUserSelect::None => "none",
            StyleUserSelect::All => "all",
        })
    }
}

// -- StyleTextDecoration --

/// Text decoration (underline, overline, line-through).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextDecoration {
    /// No decoration
    None,
    /// Underline
    Underline,
    /// Line above text
    Overline,
    /// Strike-through line
    LineThrough,
}
impl Default for StyleTextDecoration {
    fn default() -> Self {
        StyleTextDecoration::None
    }
}
impl_option!(
    StyleTextDecoration,
    OptionStyleTextDecoration,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextDecoration {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleTextDecoration::None => "none",
            StyleTextDecoration::Underline => "underline",
            StyleTextDecoration::Overline => "overline",
            StyleTextDecoration::LineThrough => "line-through",
        })
    }
}

// -- StyleVerticalAlign --

/// Vertical text alignment enum (top, center, bottom) - default: `Top`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVerticalAlign {
    /// CSS default - align baselines
    Baseline,
    /// Align top of element with top of line box
    Top,
    /// Align middle of element with baseline + half x-height
    Middle,
    /// Align bottom of element with bottom of line box
    Bottom,
    /// Align baseline with parent's subscript baseline
    Sub,
    /// Align baseline with parent's superscript baseline
    Superscript,
    /// Align top with top of parent's font
    TextTop,
    /// Align bottom with bottom of parent's font
    TextBottom,
}

impl_option!(
    StyleVerticalAlign,
    OptionStyleVerticalAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl Default for StyleVerticalAlign {
    fn default() -> Self {
        StyleVerticalAlign::Baseline
    }
}
impl PrintAsCssValue for StyleVerticalAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleVerticalAlign::Baseline => "baseline",
            StyleVerticalAlign::Top => "top",
            StyleVerticalAlign::Middle => "middle",
            StyleVerticalAlign::Bottom => "bottom",
            StyleVerticalAlign::Sub => "sub",
            StyleVerticalAlign::Superscript => "super",
            StyleVerticalAlign::TextTop => "text-top",
            StyleVerticalAlign::TextBottom => "text-bottom",
        })
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleVerticalAlign {
    fn format_as_rust_code(&self, _: usize) -> String {
        match self {
            StyleVerticalAlign::Baseline => "StyleVerticalAlign::Baseline",
            StyleVerticalAlign::Top => "StyleVerticalAlign::Top",
            StyleVerticalAlign::Middle => "StyleVerticalAlign::Middle",
            StyleVerticalAlign::Bottom => "StyleVerticalAlign::Bottom",
            StyleVerticalAlign::Sub => "StyleVerticalAlign::Sub",
            StyleVerticalAlign::Superscript => "StyleVerticalAlign::Superscript",
            StyleVerticalAlign::TextTop => "StyleVerticalAlign::TextTop",
            StyleVerticalAlign::TextBottom => "StyleVerticalAlign::TextBottom",
        }
        .to_string()
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
#[repr(C, u8)]
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
#[repr(C, u8)]
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
#[repr(C, u8)]
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

// -- StyleTextIndent (text-indent property) --

/// Represents a `text-indent` attribute (indentation of first line in a block).
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextIndent {
    pub inner: PixelValue,
}

impl fmt::Debug for StyleTextIndent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl_pixel_value!(StyleTextIndent);

impl PrintAsCssValue for StyleTextIndent {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleTextIndent {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleTextIndent {{ inner: PixelValue::const_px(0) /* {} */ }}",
            self.inner
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextIndentParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextIndentParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextIndentParseError<'a>, {
    PixelValue(e) => format!("Invalid text-indent value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    StyleTextIndentParseError::PixelValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleTextIndentParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextIndentParseError<'a> {
    pub fn to_contained(&self) -> StyleTextIndentParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleTextIndentParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextIndentParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextIndentParseError<'a> {
        match self {
            Self::PixelValue(e) => StyleTextIndentParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_indent(input: &str) -> Result<StyleTextIndent, StyleTextIndentParseError> {
    crate::props::basic::pixel::parse_pixel_value(input)
        .map(|inner| StyleTextIndent { inner })
        .map_err(|e| StyleTextIndentParseError::PixelValue(e))
}

/// initial-letter property for drop caps
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleInitialLetter {
    pub size: u32,
    pub sink: crate::corety::OptionU32,
}

impl FormatAsRustCode for StyleInitialLetter {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl PrintAsCssValue for StyleInitialLetter {
    fn print_as_css_value(&self) -> String {
        if let crate::corety::OptionU32::Some(sink) = self.sink {
            format!("{} {}", self.size, sink)
        } else {
            format!("{}", self.size)
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleInitialLetterParseError<'a> {
    InvalidFormat(&'a str),
    InvalidSize(&'a str),
    InvalidSink(&'a str),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleInitialLetterParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleInitialLetterParseError<'a>, {
    InvalidFormat(e) => format!("Invalid initial-letter format: {}", e),
    InvalidSize(e) => format!("Invalid initial-letter size: {}", e),
    InvalidSink(e) => format!("Invalid initial-letter sink: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleInitialLetterParseErrorOwned {
    InvalidFormat(AzString),
    InvalidSize(AzString),
    InvalidSink(AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleInitialLetterParseError<'a> {
    pub fn to_contained(&self) -> StyleInitialLetterParseErrorOwned {
        match self {
            Self::InvalidFormat(s) => {
                StyleInitialLetterParseErrorOwned::InvalidFormat(s.to_string().into())
            }
            Self::InvalidSize(s) => StyleInitialLetterParseErrorOwned::InvalidSize(s.to_string().into()),
            Self::InvalidSink(s) => StyleInitialLetterParseErrorOwned::InvalidSink(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleInitialLetterParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleInitialLetterParseError<'a> {
        match self {
            Self::InvalidFormat(s) => StyleInitialLetterParseError::InvalidFormat(s.as_str()),
            Self::InvalidSize(s) => StyleInitialLetterParseError::InvalidSize(s.as_str()),
            Self::InvalidSink(s) => StyleInitialLetterParseError::InvalidSink(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleInitialLetterParseError<'_>> for StyleInitialLetterParseErrorOwned {
    fn from(e: StyleInitialLetterParseError) -> Self {
        match e {
            StyleInitialLetterParseError::InvalidFormat(s) => {
                StyleInitialLetterParseErrorOwned::InvalidFormat(s.to_string().into())
            }
            StyleInitialLetterParseError::InvalidSize(s) => {
                StyleInitialLetterParseErrorOwned::InvalidSize(s.to_string().into())
            }
            StyleInitialLetterParseError::InvalidSink(s) => {
                StyleInitialLetterParseErrorOwned::InvalidSink(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleInitialLetterParseErrorOwned, {
    InvalidFormat(e) => format!("Invalid initial-letter format: {}", e),
    InvalidSize(e) => format!("Invalid initial-letter size: {}", e),
    InvalidSink(e) => format!("Invalid initial-letter sink: {}", e),
}}

#[cfg(feature = "parser")]
pub fn parse_style_initial_letter<'a>(
    input: &'a str,
) -> Result<StyleInitialLetter, StyleInitialLetterParseError<'a>> {
    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.is_empty() {
        return Err(StyleInitialLetterParseError::InvalidFormat(input));
    }

    // Parse size (required)
    let size = parts[0]
        .parse::<u32>()
        .map_err(|_| StyleInitialLetterParseError::InvalidSize(parts[0]))?;

    if size == 0 {
        return Err(StyleInitialLetterParseError::InvalidSize(parts[0]));
    }

    // Parse sink (optional)
    let sink = if parts.len() > 1 {
        crate::corety::OptionU32::Some(
            parts[1]
                .parse::<u32>()
                .map_err(|_| StyleInitialLetterParseError::InvalidSink(parts[1]))?,
        )
    } else {
        crate::corety::OptionU32::None
    };

    Ok(StyleInitialLetter { size, sink })
}

/// line-clamp property for limiting visible lines
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLineClamp {
    pub max_lines: usize,
}

impl FormatAsRustCode for StyleLineClamp {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl PrintAsCssValue for StyleLineClamp {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.max_lines)
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleLineClampParseError<'a> {
    InvalidValue(&'a str),
    ZeroValue,
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleLineClampParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleLineClampParseError<'a>, {
    InvalidValue(e) => format!("Invalid line-clamp value: {}", e),
    ZeroValue => format!("line-clamp cannot be zero"),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleLineClampParseErrorOwned {
    InvalidValue(AzString),
    ZeroValue,
}

#[cfg(feature = "parser")]
impl<'a> StyleLineClampParseError<'a> {
    pub fn to_contained(&self) -> StyleLineClampParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleLineClampParseErrorOwned::InvalidValue(s.to_string().into()),
            Self::ZeroValue => StyleLineClampParseErrorOwned::ZeroValue,
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLineClampParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleLineClampParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleLineClampParseError::InvalidValue(s.as_str()),
            Self::ZeroValue => StyleLineClampParseError::ZeroValue,
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleLineClampParseError<'_>> for StyleLineClampParseErrorOwned {
    fn from(e: StyleLineClampParseError) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleLineClampParseErrorOwned, {
    InvalidValue(e) => format!("Invalid line-clamp value: {}", e),
    ZeroValue => format!("line-clamp cannot be zero"),
}}

#[cfg(feature = "parser")]
pub fn parse_style_line_clamp<'a>(
    input: &'a str,
) -> Result<StyleLineClamp, StyleLineClampParseError<'a>> {
    let input = input.trim();

    let max_lines = input
        .parse::<usize>()
        .map_err(|_| StyleLineClampParseError::InvalidValue(input))?;

    if max_lines == 0 {
        return Err(StyleLineClampParseError::ZeroValue);
    }

    Ok(StyleLineClamp { max_lines })
}

/// hanging-punctuation property for hanging punctuation marks
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleHangingPunctuation {
    pub enabled: bool,
}

impl Default for StyleHangingPunctuation {
    fn default() -> Self {
        Self { enabled: false }
    }
}

impl FormatAsRustCode for StyleHangingPunctuation {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl PrintAsCssValue for StyleHangingPunctuation {
    fn print_as_css_value(&self) -> String {
        if self.enabled {
            "first allow-end last force-end".to_string()
        } else {
            "none".to_string()
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleHangingPunctuationParseError<'a> {
    InvalidValue(&'a str),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleHangingPunctuationParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleHangingPunctuationParseError<'a>, {
    InvalidValue(e) => format!("Invalid hanging-punctuation value: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleHangingPunctuationParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleHangingPunctuationParseError<'a> {
    pub fn to_contained(&self) -> StyleHangingPunctuationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleHangingPunctuationParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHangingPunctuationParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleHangingPunctuationParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleHangingPunctuationParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleHangingPunctuationParseError<'_>> for StyleHangingPunctuationParseErrorOwned {
    fn from(e: StyleHangingPunctuationParseError) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleHangingPunctuationParseErrorOwned, {
    InvalidValue(e) => format!("Invalid hanging-punctuation value: {}", e),
}}

#[cfg(feature = "parser")]
pub fn parse_style_hanging_punctuation<'a>(
    input: &'a str,
) -> Result<StyleHangingPunctuation, StyleHangingPunctuationParseError<'a>> {
    let input = input.trim().to_lowercase();

    // For simplicity: "none" = disabled, anything else = enabled
    // Full spec supports: first, last, force-end, allow-end
    let enabled = input != "none";

    Ok(StyleHangingPunctuation { enabled })
}

/// text-combine-upright property for combining horizontal text in vertical layout
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleTextCombineUpright {
    None,
    All,
    Digits(u8),
}

impl Default for StyleTextCombineUpright {
    fn default() -> Self {
        Self::None
    }
}

impl FormatAsRustCode for StyleTextCombineUpright {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl PrintAsCssValue for StyleTextCombineUpright {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::All => "all".to_string(),
            Self::Digits(n) => format!("digits {}", n),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextCombineUprightParseError<'a> {
    InvalidValue(&'a str),
    InvalidDigits(&'a str),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextCombineUprightParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextCombineUprightParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-combine-upright value: {}", e),
    InvalidDigits(e) => format!("Invalid text-combine-upright digits: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleTextCombineUprightParseErrorOwned {
    InvalidValue(AzString),
    InvalidDigits(AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextCombineUprightParseError<'a> {
    pub fn to_contained(&self) -> StyleTextCombineUprightParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleTextCombineUprightParseErrorOwned::InvalidValue(s.to_string().into())
            }
            Self::InvalidDigits(s) => {
                StyleTextCombineUprightParseErrorOwned::InvalidDigits(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextCombineUprightParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextCombineUprightParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleTextCombineUprightParseError::InvalidValue(s.as_str()),
            Self::InvalidDigits(s) => StyleTextCombineUprightParseError::InvalidDigits(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleTextCombineUprightParseError<'_>> for StyleTextCombineUprightParseErrorOwned {
    fn from(e: StyleTextCombineUprightParseError) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleTextCombineUprightParseErrorOwned, {
    InvalidValue(e) => format!("Invalid text-combine-upright value: {}", e),
    InvalidDigits(e) => format!("Invalid text-combine-upright digits: {}", e),
}}

#[cfg(feature = "parser")]
pub fn parse_style_text_combine_upright<'a>(
    input: &'a str,
) -> Result<StyleTextCombineUpright, StyleTextCombineUprightParseError<'a>> {
    let trimmed = input.trim();

    if trimmed.eq_ignore_ascii_case("none") {
        Ok(StyleTextCombineUpright::None)
    } else if trimmed.eq_ignore_ascii_case("all") {
        Ok(StyleTextCombineUpright::All)
    } else if trimmed.starts_with("digits") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 2 {
            let n = parts[1]
                .parse::<u8>()
                .map_err(|_| StyleTextCombineUprightParseError::InvalidDigits(input))?;
            if n >= 2 && n <= 4 {
                Ok(StyleTextCombineUpright::Digits(n))
            } else {
                Err(StyleTextCombineUprightParseError::InvalidDigits(input))
            }
        } else {
            // Default to "digits 2"
            Ok(StyleTextCombineUpright::Digits(2))
        }
    } else {
        Err(StyleTextCombineUprightParseError::InvalidValue(input))
    }
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
#[repr(C, u8)]
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
#[repr(C, u8)]
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
pub enum StyleTabSizeParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTabSizeParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTabSizeParseError<'a>, {
    PixelValue(e) => format!("Invalid tab-size value: {}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    StyleTabSizeParseError::PixelValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleTabSizeParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTabSizeParseError<'a> {
    pub fn to_contained(&self) -> StyleTabSizeParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleTabSizeParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTabSizeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTabSizeParseError<'a> {
        match self {
            Self::PixelValue(e) => StyleTabSizeParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_tab_size(input: &str) -> Result<StyleTabSize, StyleTabSizeParseError> {
    if let Ok(number) = input.trim().parse::<f32>() {
        Ok(StyleTabSize {
            inner: PixelValue::em(number),
        })
    } else {
        crate::props::basic::pixel::parse_pixel_value(input)
            .map(|v| StyleTabSize { inner: v })
            .map_err(|e| StyleTabSizeParseError::PixelValue(e))
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
#[repr(C, u8)]
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
        "nowrap" | "no-wrap" => Ok(StyleWhiteSpace::Nowrap),
        "pre-wrap" => Ok(StyleWhiteSpace::PreWrap),
        "pre-line" => Ok(StyleWhiteSpace::PreLine),
        "break-spaces" => Ok(StyleWhiteSpace::BreakSpaces),
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
#[repr(C, u8)]
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
#[repr(C, u8)]
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
pub enum StyleUserSelectParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleUserSelectParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleUserSelectParseError<'a>, {
    InvalidValue(e) => format!("Invalid user-select value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleUserSelectParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleUserSelectParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleUserSelectParseError<'a> {
    pub fn to_contained(&self) -> StyleUserSelectParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleUserSelectParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleUserSelectParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleUserSelectParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleUserSelectParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_user_select(input: &str) -> Result<StyleUserSelect, StyleUserSelectParseError> {
    match input.trim() {
        "auto" => Ok(StyleUserSelect::Auto),
        "text" => Ok(StyleUserSelect::Text),
        "none" => Ok(StyleUserSelect::None),
        "all" => Ok(StyleUserSelect::All),
        other => Err(StyleUserSelectParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextDecorationParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextDecorationParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextDecorationParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-decoration value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(
    InvalidValueErr<'a>,
    StyleTextDecorationParseError::InvalidValue
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleTextDecorationParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextDecorationParseError<'a> {
    pub fn to_contained(&self) -> StyleTextDecorationParseErrorOwned {
        match self {
            Self::InvalidValue(e) => {
                StyleTextDecorationParseErrorOwned::InvalidValue(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextDecorationParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextDecorationParseError<'a> {
        match self {
            Self::InvalidValue(e) => StyleTextDecorationParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_decoration(
    input: &str,
) -> Result<StyleTextDecoration, StyleTextDecorationParseError> {
    match input.trim() {
        "none" => Ok(StyleTextDecoration::None),
        "underline" => Ok(StyleTextDecoration::Underline),
        "overline" => Ok(StyleTextDecoration::Overline),
        "line-through" => Ok(StyleTextDecoration::LineThrough),
        other => Err(StyleTextDecorationParseError::InvalidValue(
            InvalidValueErr(other),
        )),
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
#[repr(C, u8)]
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
        "baseline" => Ok(StyleVerticalAlign::Baseline),
        "top" => Ok(StyleVerticalAlign::Top),
        "middle" => Ok(StyleVerticalAlign::Middle),
        "bottom" => Ok(StyleVerticalAlign::Bottom),
        "sub" => Ok(StyleVerticalAlign::Sub),
        "super" => Ok(StyleVerticalAlign::Superscript),
        "text-top" => Ok(StyleVerticalAlign::TextTop),
        "text-bottom" => Ok(StyleVerticalAlign::TextBottom),
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
    pub inner: CssDuration,
}

impl Default for CaretAnimationDuration {
    fn default() -> Self {
        Self {
            inner: CssDuration { inner: 500 },
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

// --- CaretWidth ---

/// Width of the text cursor (caret) in pixels.
/// CSS doesn't have a standard property for this, so we use `-azul-caret-width`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CaretWidth {
    pub inner: PixelValue,
}

impl Default for CaretWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::px(2.0), // Default 2px caret width
        }
    }
}

impl PrintAsCssValue for CaretWidth {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

impl crate::format_rust_code::FormatAsRustCode for CaretWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretWidth {{ inner: {} }}",
            self.inner.format_as_rust_code(0)
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_caret_width(input: &str) -> Result<CaretWidth, CssPixelValueParseError> {
    use crate::props::basic::pixel::parse_pixel_value;

    parse_pixel_value(input).map(|inner| CaretWidth { inner })
}

// --- From implementations for CssProperty ---

impl From<StyleUserSelect> for crate::props::property::CssProperty {
    fn from(value: StyleUserSelect) -> Self {
        use crate::props::property::CssProperty;
        CssProperty::user_select(value)
    }
}

impl From<StyleTextDecoration> for crate::props::property::CssProperty {
    fn from(value: StyleTextDecoration) -> Self {
        use crate::props::property::CssProperty;
        CssProperty::text_decoration(value)
    }
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
    fn test_parse_tab_size() {
        // Unitless number is treated as `em`
        assert_eq!(
            parse_style_tab_size("4").unwrap().inner,
            PixelValue::em(4.0)
        );
        assert_eq!(
            parse_style_tab_size("20px").unwrap().inner,
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
        assert_eq!(
            parse_style_white_space("pre-wrap").unwrap(),
            StyleWhiteSpace::PreWrap
        );
    }
}
