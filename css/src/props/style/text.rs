//! CSS properties for styling text.
//!
//! Each property type implements `PrintAsCssValue` for CSS serialization and
//! (behind the `parser` feature) has a corresponding `parse_style_*` function
//! with borrowed/owned error type pairs.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{
    codegen::format::FormatAsRustCode,
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
    pub inner: ColorU,
}

impl fmt::Debug for StyleTextColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl StyleTextColor {
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
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
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Justify => "justify",
            Self::Start => "start",
            Self::End => "end",
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
/// CSS Text Level 3: <https://www.w3.org/TR/css-text-3/#white-space-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleWhiteSpace {
    /// Collapse whitespace, wrap lines
    #[default]
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
impl_option!(
    StyleWhiteSpace,
    OptionStyleWhiteSpace,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleWhiteSpace {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Normal => "normal",
            Self::Pre => "pre",
            Self::Nowrap => "nowrap",
            Self::PreWrap => "pre-wrap",
            Self::PreLine => "pre-line",
            Self::BreakSpaces => "break-spaces",
        })
    }
}

// -- StyleHyphens --

/// Hyphenation rules.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleHyphens {
    /// No hyphenation: words are not broken at hyphenation opportunities.
    None,
    /// Manual hyphenation: words are only broken at explicit soft hyphens (U+00AD)
    /// or unconditional hyphens (U+2010).
    #[default]
    Manual,
    /// Automatic hyphenation: words may be broken at automatic hyphenation
    /// opportunities determined by a language-appropriate hyphenation resource,
    /// in addition to explicit opportunities.
    Auto,
}
impl_option!(
    StyleHyphens,
    OptionStyleHyphens,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleHyphens {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::Manual => "manual",
            Self::Auto => "auto",
        })
    }
}

// -- StyleLineBreak --

/// Controls the strictness of line breaking rules.
///
/// CSS Text Level 3: <https://www.w3.org/TR/css-text-3/#line-break-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleLineBreak {
    /// The browser determines the set of line-breaking restrictions to use.
    #[default]
    Auto,
    /// Breaks text using the least restrictive set of line-breaking rules.
    Loose,
    /// Breaks text using the most common set of line-breaking rules.
    Normal,
    /// Breaks text using the most stringent set of line-breaking rules.
    Strict,
    /// There is a soft wrap opportunity around every typographic character unit,
    /// including around any punctuation character or preserved white spaces,
    /// or in the middle of words, disregarding any prohibition against line breaks.
    Anywhere,
}
impl_option!(
    StyleLineBreak,
    OptionStyleLineBreak,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleLineBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Loose => "loose",
            Self::Normal => "normal",
            Self::Strict => "strict",
            Self::Anywhere => "anywhere",
        })
    }
}

// -- StyleWordBreak --

/// Controls line breaking rules within words.
///
/// CSS Text Level 3 §5.2: <https://www.w3.org/TR/css-text-3/#word-break-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleWordBreak {
    /// Use default line break rules.
    #[default]
    Normal,
    /// Allow break opportunities between any two characters (CJK and non-CJK).
    BreakAll,
    /// Forbid break opportunities within CJK character sequences.
    KeepAll,
    // +spec:line-breaking:815882 - deprecated break-word keyword: same as normal + overflow-wrap: anywhere
    /// Deprecated: equivalent to word-break: normal and overflow-wrap: anywhere.
    BreakWord,
}
impl_option!(
    StyleWordBreak,
    OptionStyleWordBreak,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleWordBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Normal => "normal",
            Self::BreakAll => "break-all",
            Self::KeepAll => "keep-all",
            Self::BreakWord => "break-word",
        })
    }
}

// -- StyleOverflowWrap --

/// Controls whether the browser may break at otherwise disallowed points
/// to prevent overflow.
///
/// CSS Text Level 3 §3.3: <https://www.w3.org/TR/css-text-3/#overflow-wrap-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleOverflowWrap {
    /// Lines may only break at allowed break points.
    #[default]
    Normal,
    /// An otherwise unbreakable sequence may be broken at an arbitrary point
    /// if there are no otherwise acceptable break points.
    Anywhere,
    /// Same as `anywhere` but soft wrap opportunities introduced are not
    /// considered when calculating min-content intrinsic sizes.
    BreakWord,
}
impl_option!(
    StyleOverflowWrap,
    OptionStyleOverflowWrap,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleOverflowWrap {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Normal => "normal",
            Self::Anywhere => "anywhere",
            Self::BreakWord => "break-word",
        })
    }
}

// -- StyleTextAlignLast --

/// Controls alignment of the last line of a block or a line right before
/// a forced line break.
///
/// CSS Text Level 3 §7.2: <https://www.w3.org/TR/css-text-3/#text-align-last-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleTextAlignLast {
    /// Alignment of the last line is determined by text-align (or start if justify).
    #[default]
    Auto,
    /// Align to the start edge of the line box.
    Start,
    /// Align to the end edge of the line box.
    End,
    /// Align to the line left.
    Left,
    /// Align to the line right.
    Right,
    /// Center the content.
    Center,
    /// Justify the content.
    Justify,
}
impl_option!(
    StyleTextAlignLast,
    OptionStyleTextAlignLast,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextAlignLast {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
        })
    }
}

// -- StyleDirection --

/// Text direction.
// +spec:writing-modes:46fed3 - direction property provides explicit bidi controls in CSS
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleDirection {
    /// Left-to-right text direction
    #[default]
    Ltr,
    /// Right-to-left text direction
    Rtl,
}
impl_option!(
    StyleDirection,
    OptionStyleDirection,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleDirection {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        })
    }
}

// -- StyleUserSelect --

/// Controls whether the user can select text.
/// Used to prevent accidental text selection on UI controls like buttons.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleUserSelect {
    /// Browser determines selectability (default)
    #[default]
    Auto,
    /// Text is selectable
    Text,
    /// Text is not selectable
    None,
    /// User can select all text with a single action
    All,
}
impl_option!(
    StyleUserSelect,
    OptionStyleUserSelect,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleUserSelect {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Text => "text",
            Self::None => "none",
            Self::All => "all",
        })
    }
}

// -- StyleTextDecoration --

/// Text decoration (underline, overline, line-through).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleTextDecoration {
    /// No decoration
    #[default]
    None,
    /// Underline
    Underline,
    /// Line above text
    Overline,
    /// Strike-through line
    LineThrough,
}
impl_option!(
    StyleTextDecoration,
    OptionStyleTextDecoration,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextDecoration {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::Underline => "underline",
            Self::Overline => "overline",
            Self::LineThrough => "line-through",
        })
    }
}

// -- StyleVerticalAlign --

/// CSS 2.2 §10.8.1 vertical-align property values
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum StyleVerticalAlign {
    /// CSS default - align baselines
    #[default]
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
    /// <percentage> refers to line-height of the element itself
    Percentage(PercentageValue),
    /// <length> offset from baseline
    Length(PixelValue),
}

impl_option!(
    StyleVerticalAlign,
    OptionStyleVerticalAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl PrintAsCssValue for StyleVerticalAlign {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Baseline => String::from("baseline"),
            Self::Top => String::from("top"),
            Self::Middle => String::from("middle"),
            Self::Bottom => String::from("bottom"),
            Self::Sub => String::from("sub"),
            Self::Superscript => String::from("super"),
            Self::TextTop => String::from("text-top"),
            Self::TextBottom => String::from("text-bottom"),
            Self::Percentage(p) => format!("{}%", p.normalized() * 100.0),
            Self::Length(l) => l.print_as_css_value(),
        }
    }
}

impl FormatAsRustCode for StyleVerticalAlign {
    fn format_as_rust_code(&self, indent: usize) -> String {
        match self {
            Self::Baseline => "StyleVerticalAlign::Baseline".to_string(),
            Self::Top => "StyleVerticalAlign::Top".to_string(),
            Self::Middle => "StyleVerticalAlign::Middle".to_string(),
            Self::Bottom => "StyleVerticalAlign::Bottom".to_string(),
            Self::Sub => "StyleVerticalAlign::Sub".to_string(),
            Self::Superscript => "StyleVerticalAlign::Superscript".to_string(),
            Self::TextTop => "StyleVerticalAlign::TextTop".to_string(),
            Self::TextBottom => "StyleVerticalAlign::TextBottom".to_string(),
            Self::Percentage(p) => format!("StyleVerticalAlign::Percentage(PercentageValue::new({}))", p.normalized() * 100.0),
            Self::Length(l) => format!("StyleVerticalAlign::Length({l})"),
        }
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
impl StyleTextColorParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextColorParseErrorOwned {
        match self {
            Self::ColorParseError(e) => {
                StyleTextColorParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextColorParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextColorParseError<'_> {
        match self {
            Self::ColorParseError(e) => StyleTextColorParseError::ColorParseError(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-color` value.
pub fn parse_style_text_color(input: &str) -> Result<StyleTextColor, StyleTextColorParseError<'_>> {
    parse_css_color(input)
        .map(|inner| StyleTextColor { inner })
        .map_err(StyleTextColorParseError::ColorParseError)
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextAlignParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextAlignParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextAlignParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextAlignParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextAlignParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextAlignParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextAlignParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-align` value.
pub fn parse_style_text_align(input: &str) -> Result<StyleTextAlign, StyleTextAlignParseError<'_>> {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleLetterSpacingParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleLetterSpacingParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleLetterSpacingParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleLetterSpacingParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLetterSpacingParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleLetterSpacingParseError<'_> {
        match self {
            Self::PixelValue(e) => StyleLetterSpacingParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `letter-spacing` value.
pub fn parse_style_letter_spacing(
    input: &str,
) -> Result<StyleLetterSpacing, StyleLetterSpacingParseError<'_>> {
    crate::props::basic::pixel::parse_pixel_value(input)
        .map(|inner| StyleLetterSpacing { inner })
        .map_err(StyleLetterSpacingParseError::PixelValue)
}

// -- StyleTextIndent (text-indent property) --

/// Represents a `text-indent` attribute (indentation of first line in a block).
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextIndent {
    pub inner: PixelValue,
    /// `each-line` keyword: indent first line of each block container
    /// AND each line after a forced line break (but not after soft wrap).
    pub each_line: bool,
    /// `hanging` keyword: inverts which lines are affected by the indent.
    pub hanging: bool,
}

impl fmt::Debug for StyleTextIndent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl StyleTextIndent {
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self { inner: PixelValue::zero(), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_px(value: isize) -> Self {
        Self { inner: PixelValue::const_px(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_em(value: isize) -> Self {
        Self { inner: PixelValue::const_em(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_pt(value: isize) -> Self {
        Self { inner: PixelValue::const_pt(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_percent(value: isize) -> Self {
        Self { inner: PixelValue::const_percent(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_in(value: isize) -> Self {
        Self { inner: PixelValue::const_in(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_cm(value: isize) -> Self {
        Self { inner: PixelValue::const_cm(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_mm(value: isize) -> Self {
        Self { inner: PixelValue::const_mm(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub const fn const_from_metric(metric: crate::props::basic::length::SizeMetric, value: isize) -> Self {
        Self { inner: PixelValue::const_from_metric(metric, value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn px(value: f32) -> Self {
        Self { inner: PixelValue::px(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn em(value: f32) -> Self {
        Self { inner: PixelValue::em(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn pt(value: f32) -> Self {
        Self { inner: PixelValue::pt(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn percent(value: f32) -> Self {
        Self { inner: PixelValue::percent(value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn from_metric(metric: crate::props::basic::length::SizeMetric, value: f32) -> Self {
        Self { inner: PixelValue::from_metric(metric, value), each_line: false, hanging: false }
    }
    #[inline]
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self { inner: self.inner.interpolate(&other.inner, t), each_line: self.each_line, hanging: self.hanging }
    }
}

impl PrintAsCssValue for StyleTextIndent {
    fn print_as_css_value(&self) -> String {
        let mut s = self.inner.to_string();
        if self.hanging {
            s.push_str(" hanging");
        }
        if self.each_line {
            s.push_str(" each-line");
        }
        s
    }
}

impl FormatAsRustCode for StyleTextIndent {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleTextIndent {{ inner: {}, each_line: {}, hanging: {} }}",
            self.inner.format_as_rust_code(0), self.each_line, self.hanging
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextIndentParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleTextIndentParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextIndentParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleTextIndentParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextIndentParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextIndentParseError<'_> {
        match self {
            Self::PixelValue(e) => StyleTextIndentParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-indent` value.
pub fn parse_style_text_indent(input: &str) -> Result<StyleTextIndent, StyleTextIndentParseError<'_>> {
    let mut each_line = false;
    let mut hanging = false;
    let mut pixel_part: Option<&str> = None;

    for token in input.split_whitespace() {
        match token {
            "each-line" => each_line = true,
            "hanging" => hanging = true,
            _ => {
                pixel_part = Some(token);
            }
        }
    }

    let pixel_str = pixel_part.unwrap_or("0px");

    crate::props::basic::pixel::parse_pixel_value(pixel_str)
        .map(|inner| StyleTextIndent { inner, each_line, hanging })
        .map_err(StyleTextIndentParseError::PixelValue)
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
        format!("{self:?}")
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleInitialLetterParseErrorOwned {
    InvalidFormat(AzString),
    InvalidSize(AzString),
    InvalidSink(AzString),
}

#[cfg(feature = "parser")]
impl StyleInitialLetterParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleInitialLetterParseErrorOwned {
        match self {
            Self::InvalidFormat(s) => {
                StyleInitialLetterParseErrorOwned::InvalidFormat((*s).to_string().into())
            }
            Self::InvalidSize(s) => StyleInitialLetterParseErrorOwned::InvalidSize((*s).to_string().into()),
            Self::InvalidSink(s) => StyleInitialLetterParseErrorOwned::InvalidSink((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleInitialLetterParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleInitialLetterParseError<'_> {
        match self {
            Self::InvalidFormat(s) => StyleInitialLetterParseError::InvalidFormat(s.as_str()),
            Self::InvalidSize(s) => StyleInitialLetterParseError::InvalidSize(s.as_str()),
            Self::InvalidSink(s) => StyleInitialLetterParseError::InvalidSink(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleInitialLetterParseError<'_>> for StyleInitialLetterParseErrorOwned {
    fn from(e: StyleInitialLetterParseError<'_>) -> Self {
        match e {
            StyleInitialLetterParseError::InvalidFormat(s) => {
                Self::InvalidFormat(s.to_string().into())
            }
            StyleInitialLetterParseError::InvalidSize(s) => {
                Self::InvalidSize(s.to_string().into())
            }
            StyleInitialLetterParseError::InvalidSink(s) => {
                Self::InvalidSink(s.to_string().into())
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `initial-letter` value.
pub fn parse_style_initial_letter(
    input: &str,
) -> Result<StyleInitialLetter, StyleInitialLetterParseError<'_>> {
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
        format!("{self:?}")
    }
}

impl PrintAsCssValue for StyleLineClamp {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.max_lines)
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleLineClampParseErrorOwned {
    InvalidValue(AzString),
    ZeroValue,
}

#[cfg(feature = "parser")]
impl StyleLineClampParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleLineClampParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleLineClampParseErrorOwned::InvalidValue((*s).to_string().into()),
            Self::ZeroValue => StyleLineClampParseErrorOwned::ZeroValue,
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLineClampParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleLineClampParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleLineClampParseError::InvalidValue(s.as_str()),
            Self::ZeroValue => StyleLineClampParseError::ZeroValue,
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleLineClampParseError<'_>> for StyleLineClampParseErrorOwned {
    fn from(e: StyleLineClampParseError<'_>) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleLineClampParseErrorOwned, {
    InvalidValue(e) => format!("Invalid line-clamp value: {}", e),
    ZeroValue => format!("line-clamp cannot be zero"),
}}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `line-clamp` value.
pub fn parse_style_line_clamp(
    input: &str,
) -> Result<StyleLineClamp, StyleLineClampParseError<'_>> {
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
///
/// CSS Text 3 §8: `none | [ first || [ force-end | allow-end ] || last ]`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub struct StyleHangingPunctuation {
    pub first: bool,
    pub force_end: bool,
    pub allow_end: bool,
    pub last: bool,
}

impl StyleHangingPunctuation {
    #[must_use] pub const fn is_enabled(&self) -> bool {
        self.first || self.force_end || self.allow_end || self.last
    }
}

impl FormatAsRustCode for StyleHangingPunctuation {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{self:?}")
    }
}

impl PrintAsCssValue for StyleHangingPunctuation {
    fn print_as_css_value(&self) -> String {
        if !self.is_enabled() {
            return "none".to_string();
        }
        let mut parts = Vec::new();
        if self.first { parts.push("first"); }
        if self.force_end { parts.push("force-end"); }
        if self.allow_end { parts.push("allow-end"); }
        if self.last { parts.push("last"); }
        parts.join(" ")
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleHangingPunctuationParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl StyleHangingPunctuationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleHangingPunctuationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleHangingPunctuationParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHangingPunctuationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleHangingPunctuationParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleHangingPunctuationParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleHangingPunctuationParseError<'_>> for StyleHangingPunctuationParseErrorOwned {
    fn from(e: StyleHangingPunctuationParseError<'_>) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleHangingPunctuationParseErrorOwned, {
    InvalidValue(e) => format!("Invalid hanging-punctuation value: {}", e),
}}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `hanging-punctuation` value.
pub fn parse_style_hanging_punctuation(
    input: &str,
) -> Result<StyleHangingPunctuation, StyleHangingPunctuationParseError<'_>> {
    let input = input.trim();

    if input.eq_ignore_ascii_case("none") {
        return Ok(StyleHangingPunctuation::default());
    }

    let mut first = false;
    let mut force_end = false;
    let mut allow_end = false;
    let mut last = false;

    for token in input.split_whitespace() {
        match token.to_lowercase().as_str() {
            "first" => first = true,
            "force-end" => force_end = true,
            "allow-end" => allow_end = true,
            "last" => last = true,
            _ => return Err(StyleHangingPunctuationParseError::InvalidValue(input)),
        }
    }

    if force_end && allow_end {
        return Err(StyleHangingPunctuationParseError::InvalidValue(input));
    }

    Ok(StyleHangingPunctuation { first, force_end, allow_end, last })
}

/// text-combine-upright property for combining horizontal text in vertical layout
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum StyleTextCombineUpright {
    #[default]
    None,
    All,
    Digits(u8),
}


impl FormatAsRustCode for StyleTextCombineUpright {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{self:?}")
    }
}

impl PrintAsCssValue for StyleTextCombineUpright {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::All => "all".to_string(),
            Self::Digits(n) => format!("digits {n}"),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextCombineUprightParseErrorOwned {
    InvalidValue(AzString),
    InvalidDigits(AzString),
}

#[cfg(feature = "parser")]
impl StyleTextCombineUprightParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextCombineUprightParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleTextCombineUprightParseErrorOwned::InvalidValue((*s).to_string().into())
            }
            Self::InvalidDigits(s) => {
                StyleTextCombineUprightParseErrorOwned::InvalidDigits((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextCombineUprightParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextCombineUprightParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleTextCombineUprightParseError::InvalidValue(s.as_str()),
            Self::InvalidDigits(s) => StyleTextCombineUprightParseError::InvalidDigits(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<StyleTextCombineUprightParseError<'_>> for StyleTextCombineUprightParseErrorOwned {
    fn from(e: StyleTextCombineUprightParseError<'_>) -> Self {
        e.to_contained()
    }
}

#[cfg(feature = "parser")]
impl_display! { StyleTextCombineUprightParseErrorOwned, {
    InvalidValue(e) => format!("Invalid text-combine-upright value: {}", e),
    InvalidDigits(e) => format!("Invalid text-combine-upright digits: {}", e),
}}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-combine-upright` value.
pub fn parse_style_text_combine_upright(
    input: &str,
) -> Result<StyleTextCombineUpright, StyleTextCombineUprightParseError<'_>> {
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
            if (2..=4).contains(&n) {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleWordSpacingParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleWordSpacingParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleWordSpacingParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleWordSpacingParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleWordSpacingParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleWordSpacingParseError<'_> {
        match self {
            Self::PixelValue(e) => StyleWordSpacingParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `word-spacing` value.
pub fn parse_style_word_spacing(
    input: &str,
) -> Result<StyleWordSpacing, StyleWordSpacingParseError<'_>> {
    crate::props::basic::pixel::parse_pixel_value(input)
        .map(|inner| StyleWordSpacing { inner })
        .map_err(StyleWordSpacingParseError::PixelValue)
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleLineHeightParseErrorOwned {
    Percentage(PercentageParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleLineHeightParseError {
    #[must_use] pub fn to_contained(&self) -> StyleLineHeightParseErrorOwned {
        match self {
            Self::Percentage(e) => StyleLineHeightParseErrorOwned::Percentage(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLineHeightParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleLineHeightParseError {
        match self {
            Self::Percentage(e) => StyleLineHeightParseError::Percentage(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `line-height` value.
pub fn parse_style_line_height(input: &str) -> Result<StyleLineHeight, StyleLineHeightParseError> {
    // Try <number> or <percentage> first (multiplier of font-size)
    if let Ok(inner) = crate::props::basic::length::parse_percentage_value(input) {
        return Ok(StyleLineHeight { inner });
    }
    // Try <length> (e.g., "50px") — store as NEGATIVE PercentageValue to signal absolute px.
    // Convention: negative normalized() = absolute pixel value (CSS line-height can't be negative).
    // Resolved at layout time in fc.rs where font_size is known.
    if let Ok(px) = crate::props::basic::pixel::parse_pixel_value(input) {
        if px.metric == crate::props::basic::length::SizeMetric::Px {
            let px_val = px.number.get();
            return Ok(StyleLineHeight {
                inner: PercentageValue::new(-px_val * 100.0),
            });
        }
    }
    Err(StyleLineHeightParseError::Percentage(
        PercentageParseError::InvalidUnit(String::new().into()),
    ))
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTabSizeParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}

#[cfg(feature = "parser")]
impl StyleTabSizeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTabSizeParseErrorOwned {
        match self {
            Self::PixelValue(e) => StyleTabSizeParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTabSizeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTabSizeParseError<'_> {
        match self {
            Self::PixelValue(e) => StyleTabSizeParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `tab-size` value.
pub fn parse_style_tab_size(input: &str) -> Result<StyleTabSize, StyleTabSizeParseError<'_>> {
    input.trim().parse::<f32>().map_or_else(
        |_| {
            crate::props::basic::pixel::parse_pixel_value(input)
                .map(|v| StyleTabSize { inner: v })
                .map_err(StyleTabSizeParseError::PixelValue)
        },
        |number| {
            Ok(StyleTabSize {
                inner: PixelValue::em(number),
            })
        },
    )
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleWhiteSpaceParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleWhiteSpaceParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleWhiteSpaceParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleWhiteSpaceParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleWhiteSpaceParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleWhiteSpaceParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleWhiteSpaceParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `white-space` value.
pub fn parse_style_white_space(input: &str) -> Result<StyleWhiteSpace, StyleWhiteSpaceParseError<'_>> {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleHyphensParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleHyphensParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleHyphensParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleHyphensParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHyphensParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleHyphensParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleHyphensParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `hyphens` value.
pub fn parse_style_hyphens(input: &str) -> Result<StyleHyphens, StyleHyphensParseError<'_>> {
    match input.trim() {
        "none" => Ok(StyleHyphens::None),
        "manual" => Ok(StyleHyphens::Manual),
        "auto" => Ok(StyleHyphens::Auto),
        other => Err(StyleHyphensParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleLineBreak parse --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleLineBreakParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleLineBreakParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleLineBreakParseError<'a>, {
    InvalidValue(e) => format!("Invalid line-break value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleLineBreakParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleLineBreakParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleLineBreakParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleLineBreakParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleLineBreakParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleLineBreakParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleLineBreakParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleLineBreakParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `line-break` value.
pub fn parse_style_line_break(input: &str) -> Result<StyleLineBreak, StyleLineBreakParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleLineBreak::Auto),
        "loose" => Ok(StyleLineBreak::Loose),
        "normal" => Ok(StyleLineBreak::Normal),
        "strict" => Ok(StyleLineBreak::Strict),
        "anywhere" => Ok(StyleLineBreak::Anywhere),
        other => Err(StyleLineBreakParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleWordBreak parse --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleWordBreakParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleWordBreakParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleWordBreakParseError<'a>, {
    InvalidValue(e) => format!("Invalid word-break value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleWordBreakParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleWordBreakParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleWordBreakParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleWordBreakParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleWordBreakParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleWordBreakParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleWordBreakParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleWordBreakParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `word-break` value.
pub fn parse_style_word_break(input: &str) -> Result<StyleWordBreak, StyleWordBreakParseError<'_>> {
    match input.trim() {
        "normal" => Ok(StyleWordBreak::Normal),
        "break-all" => Ok(StyleWordBreak::BreakAll),
        "keep-all" => Ok(StyleWordBreak::KeepAll),
        "break-word" => Ok(StyleWordBreak::BreakWord),
        other => Err(StyleWordBreakParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleOverflowWrap parse --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleOverflowWrapParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleOverflowWrapParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleOverflowWrapParseError<'a>, {
    InvalidValue(e) => format!("Invalid overflow-wrap value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleOverflowWrapParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleOverflowWrapParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleOverflowWrapParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleOverflowWrapParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleOverflowWrapParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleOverflowWrapParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleOverflowWrapParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleOverflowWrapParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `overflow-wrap` value.
pub fn parse_style_overflow_wrap(input: &str) -> Result<StyleOverflowWrap, StyleOverflowWrapParseError<'_>> {
    match input.trim() {
        "normal" => Ok(StyleOverflowWrap::Normal),
        "anywhere" => Ok(StyleOverflowWrap::Anywhere),
        "break-word" => Ok(StyleOverflowWrap::BreakWord),
        other => Err(StyleOverflowWrapParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleTextAlignLast parse --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleTextAlignLastParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextAlignLastParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextAlignLastParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-align-last value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleTextAlignLastParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextAlignLastParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextAlignLastParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextAlignLastParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextAlignLastParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextAlignLastParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextAlignLastParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextAlignLastParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-align-last` value.
pub fn parse_style_text_align_last(input: &str) -> Result<StyleTextAlignLast, StyleTextAlignLastParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleTextAlignLast::Auto),
        "start" => Ok(StyleTextAlignLast::Start),
        "end" => Ok(StyleTextAlignLast::End),
        "left" => Ok(StyleTextAlignLast::Left),
        "right" => Ok(StyleTextAlignLast::Right),
        "center" => Ok(StyleTextAlignLast::Center),
        "justify" => Ok(StyleTextAlignLast::Justify),
        other => Err(StyleTextAlignLastParseError::InvalidValue(InvalidValueErr(other))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleDirectionParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleDirectionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleDirectionParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleDirectionParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleDirectionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleDirectionParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleDirectionParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `direction` value.
pub fn parse_style_direction(input: &str) -> Result<StyleDirection, StyleDirectionParseError<'_>> {
    match input.trim() {
        "ltr" => Ok(StyleDirection::Ltr),
        "rtl" => Ok(StyleDirection::Rtl),
        other => Err(StyleDirectionParseError::InvalidValue(InvalidValueErr(
            other,
        ))),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleUserSelectParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleUserSelectParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleUserSelectParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleUserSelectParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleUserSelectParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleUserSelectParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleUserSelectParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `user-select` value.
pub fn parse_style_user_select(input: &str) -> Result<StyleUserSelect, StyleUserSelectParseError<'_>> {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextDecorationParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextDecorationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextDecorationParseErrorOwned {
        match self {
            Self::InvalidValue(e) => {
                StyleTextDecorationParseErrorOwned::InvalidValue(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextDecorationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextDecorationParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextDecorationParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-decoration` value.
pub fn parse_style_text_decoration(
    input: &str,
) -> Result<StyleTextDecoration, StyleTextDecorationParseError<'_>> {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleVerticalAlignParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleVerticalAlignParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleVerticalAlignParseErrorOwned {
        match self {
            Self::InvalidValue(e) => {
                StyleVerticalAlignParseErrorOwned::InvalidValue(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleVerticalAlignParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleVerticalAlignParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleVerticalAlignParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `vertical-align` value.
pub fn parse_style_vertical_align(
    input: &str,
) -> Result<StyleVerticalAlign, StyleVerticalAlignParseError<'_>> {
    match input.trim() {
        "baseline" => Ok(StyleVerticalAlign::Baseline),
        "top" => Ok(StyleVerticalAlign::Top),
        "middle" => Ok(StyleVerticalAlign::Middle),
        "bottom" => Ok(StyleVerticalAlign::Bottom),
        "sub" => Ok(StyleVerticalAlign::Sub),
        "super" => Ok(StyleVerticalAlign::Superscript),
        "text-top" => Ok(StyleVerticalAlign::TextTop),
        "text-bottom" => Ok(StyleVerticalAlign::TextBottom),
        other if other.ends_with('%') => {
            let num_str = other.trim_end_matches('%').trim();
            num_str.parse::<f32>().map_or_else(
                |_| Err(StyleVerticalAlignParseError::InvalidValue(InvalidValueErr(other))),
                |val| Ok(StyleVerticalAlign::Percentage(PercentageValue::new(val))),
            )
        }
        other => crate::props::basic::pixel::parse_pixel_value(other).map_or_else(
            |_| Err(StyleVerticalAlignParseError::InvalidValue(InvalidValueErr(other))),
            |pv| Ok(StyleVerticalAlign::Length(pv)),
        ),
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

impl FormatAsRustCode for CaretColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretColor {{ inner: {} }}",
            crate::codegen::format::format_color_value(&self.inner)
        )
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `caret-color` value.
pub fn parse_caret_color(input: &str) -> Result<CaretColor, CssColorParseError<'_>> {
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

impl FormatAsRustCode for CaretAnimationDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretAnimationDuration {{ inner: {} }}",
            self.inner.format_as_rust_code(0)
        )
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `caret-animation-duration` value.
pub fn parse_caret_animation_duration(
    input: &str,
) -> Result<CaretAnimationDuration, DurationParseError<'_>> {
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

impl FormatAsRustCode for CaretWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CaretWidth {{ inner: {} }}",
            self.inner.format_as_rust_code(0)
        )
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `caret-width` value.
pub fn parse_caret_width(input: &str) -> Result<CaretWidth, CssPixelValueParseError<'_>> {
    use crate::props::basic::pixel::parse_pixel_value;

    parse_pixel_value(input).map(|inner| CaretWidth { inner })
}

// --- From implementations for CssProperty ---

impl From<StyleUserSelect> for crate::props::property::CssProperty {
    fn from(value: StyleUserSelect) -> Self {
        use crate::props::property::CssProperty;
        Self::user_select(value)
    }
}

impl From<StyleTextDecoration> for crate::props::property::CssProperty {
    fn from(value: StyleTextDecoration) -> Self {
        use crate::props::property::CssProperty;
        Self::text_decoration(value)
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
        // px values stored as negative PercentageValue (convention: negative = absolute px)
        assert_eq!(
            parse_style_line_height("20px").unwrap().inner,
            PercentageValue::new(-20.0 * 100.0)
        );
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

// -- StyleUnicodeBidi --

/// Represents the `unicode-bidi` CSS property.
///
/// Controls how bidirectional text is handled within an element.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleUnicodeBidi {
    /// No additional level of embedding
    #[default]
    Normal,
    /// Open an additional level of embedding
    Embed,
    /// Isolate the element from surrounding bidirectional text
    Isolate,
    /// Override the bidirectional algorithm for inline content
    BidiOverride,
    /// Combine isolation and override
    IsolateOverride,
    /// Determine paragraph direction from content without bidi algorithm
    Plaintext,
}
impl_option!(
    StyleUnicodeBidi,
    OptionStyleUnicodeBidi,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleUnicodeBidi {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Normal => "normal",
            Self::Embed => "embed",
            Self::Isolate => "isolate",
            Self::BidiOverride => "bidi-override",
            Self::IsolateOverride => "isolate-override",
            Self::Plaintext => "plaintext",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleUnicodeBidiParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleUnicodeBidiParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleUnicodeBidiParseError<'a>, {
    InvalidValue(e) => format!("Invalid unicode-bidi value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleUnicodeBidiParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleUnicodeBidiParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleUnicodeBidiParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleUnicodeBidiParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleUnicodeBidiParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleUnicodeBidiParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleUnicodeBidiParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleUnicodeBidiParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `unicode-bidi` value.
pub fn parse_style_unicode_bidi(input: &str) -> Result<StyleUnicodeBidi, StyleUnicodeBidiParseError<'_>> {
    match input.trim() {
        "normal" => Ok(StyleUnicodeBidi::Normal),
        "embed" => Ok(StyleUnicodeBidi::Embed),
        "isolate" => Ok(StyleUnicodeBidi::Isolate),
        "bidi-override" => Ok(StyleUnicodeBidi::BidiOverride),
        "isolate-override" => Ok(StyleUnicodeBidi::IsolateOverride),
        "plaintext" => Ok(StyleUnicodeBidi::Plaintext),
        other => Err(StyleUnicodeBidiParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleTextBoxTrim --

/// Represents the `text-box-trim` CSS property.
///
/// Controls whether the leading is trimmed at the start/end of a block container.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleTextBoxTrim {
    /// No trimming
    #[default]
    None,
    /// Trim leading over the first formatted line
    TrimStart,
    /// Trim leading under the last formatted line
    TrimEnd,
    /// Trim both start and end
    TrimBoth,
}
impl_option!(
    StyleTextBoxTrim,
    OptionStyleTextBoxTrim,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextBoxTrim {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::TrimStart => "trim-start",
            Self::TrimEnd => "trim-end",
            Self::TrimBoth => "trim-both",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleTextBoxTrimParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextBoxTrimParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextBoxTrimParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-box-trim value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleTextBoxTrimParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextBoxTrimParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextBoxTrimParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextBoxTrimParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextBoxTrimParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextBoxTrimParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextBoxTrimParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextBoxTrimParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-box-trim` value.
pub fn parse_style_text_box_trim(input: &str) -> Result<StyleTextBoxTrim, StyleTextBoxTrimParseError<'_>> {
    match input.trim() {
        "none" => Ok(StyleTextBoxTrim::None),
        "trim-start" => Ok(StyleTextBoxTrim::TrimStart),
        "trim-end" => Ok(StyleTextBoxTrim::TrimEnd),
        "trim-both" => Ok(StyleTextBoxTrim::TrimBoth),
        other => Err(StyleTextBoxTrimParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleTextBoxEdge --

/// Represents the `text-box-edge` CSS property.
///
/// Specifies the metrics used for determining the over/under edges of text
/// for the purposes of `text-box-trim`.
// +spec:writing-modes:daad86 - first value = over edge, second = under edge; single value applies to both (else "text" assumed for missing)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleTextBoxEdge {
    // +spec:line-height:cc03df - Auto uses line-fit-edge value, interpreting leading (initial) as text
    /// Use the line-fit-edge value (initial: text)
    #[default]
    Auto,
    /// Use the text-over / text-under baselines
    TextEdge,
    /// Use the cap-height baseline
    CapHeight,
    /// Use the x-height baseline
    ExHeight,
}
impl_option!(
    StyleTextBoxEdge,
    OptionStyleTextBoxEdge,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextBoxEdge {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::TextEdge => "text",
            Self::CapHeight => "cap",
            Self::ExHeight => "ex",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleTextBoxEdgeParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextBoxEdgeParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextBoxEdgeParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-box-edge value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleTextBoxEdgeParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextBoxEdgeParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextBoxEdgeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextBoxEdgeParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextBoxEdgeParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextBoxEdgeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextBoxEdgeParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextBoxEdgeParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-box-edge` value.
pub fn parse_style_text_box_edge(input: &str) -> Result<StyleTextBoxEdge, StyleTextBoxEdgeParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleTextBoxEdge::Auto),
        "text" => Ok(StyleTextBoxEdge::TextEdge),
        "cap" => Ok(StyleTextBoxEdge::CapHeight),
        "ex" => Ok(StyleTextBoxEdge::ExHeight),
        other => Err(StyleTextBoxEdgeParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleDominantBaseline --

/// Represents the `dominant-baseline` CSS property.
///
/// Specifies the dominant baseline used to align inline-level contents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleDominantBaseline {
    /// Use the dominant baseline of the parent
    #[default]
    Auto,
    /// Use the text-under baseline
    TextBottom,
    /// Use the alphabetic baseline
    Alphabetic,
    /// Use the ideographic baseline
    Ideographic,
    /// Use the middle baseline
    Middle,
    /// Use the central baseline
    Central,
    /// Use the mathematical baseline
    Mathematical,
    /// Use the hanging baseline
    Hanging,
    /// Use the text-over baseline
    TextTop,
}
impl_option!(
    StyleDominantBaseline,
    OptionStyleDominantBaseline,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleDominantBaseline {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::TextBottom => "text-bottom",
            Self::Alphabetic => "alphabetic",
            Self::Ideographic => "ideographic",
            Self::Middle => "middle",
            Self::Central => "central",
            Self::Mathematical => "mathematical",
            Self::Hanging => "hanging",
            Self::TextTop => "text-top",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleDominantBaselineParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleDominantBaselineParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleDominantBaselineParseError<'a>, {
    InvalidValue(e) => format!("Invalid dominant-baseline value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleDominantBaselineParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleDominantBaselineParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleDominantBaselineParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleDominantBaselineParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleDominantBaselineParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleDominantBaselineParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleDominantBaselineParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleDominantBaselineParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `dominant-baseline` value.
pub fn parse_style_dominant_baseline(input: &str) -> Result<StyleDominantBaseline, StyleDominantBaselineParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleDominantBaseline::Auto),
        "text-bottom" => Ok(StyleDominantBaseline::TextBottom),
        "alphabetic" => Ok(StyleDominantBaseline::Alphabetic),
        "ideographic" => Ok(StyleDominantBaseline::Ideographic),
        "middle" => Ok(StyleDominantBaseline::Middle),
        "central" => Ok(StyleDominantBaseline::Central),
        "mathematical" => Ok(StyleDominantBaseline::Mathematical),
        "hanging" => Ok(StyleDominantBaseline::Hanging),
        "text-top" => Ok(StyleDominantBaseline::TextTop),
        other => Err(StyleDominantBaselineParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleAlignmentBaseline --

// +spec:display-property:c90924 - alignment-baseline property: values, initial value, and applies-to per CSS Inline 3 §4.2.2
// +spec:font-metrics:fa4489 - alignment-baseline property: specifies box's alignment baseline used before post-alignment shift
// +spec:inline-block:939f05 - alignment-baseline property definition with all spec values (baseline, text-bottom, alphabetic, ideographic, middle, central, mathematical, text-top)
/// Represents the `alignment-baseline` CSS property.
///
/// Specifies which baseline of the element is aligned with the dominant baseline.
// +spec:writing-modes:cc8e70 - alignment-baseline values for inline baseline alignment
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleAlignmentBaseline {
    /// Use the dominant baseline of the parent
    #[default]
    Baseline,
    /// Align to the text-under baseline
    TextBottom,
    /// Align to the alphabetic baseline
    Alphabetic,
    /// Align to the ideographic baseline
    Ideographic,
    /// Align to the middle baseline
    Middle,
    /// Align to the central baseline
    Central,
    /// Align to the mathematical baseline
    Mathematical,
    /// Align to the text-over baseline
    TextTop,
}
impl_option!(
    StyleAlignmentBaseline,
    OptionStyleAlignmentBaseline,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleAlignmentBaseline {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Baseline => "baseline",
            Self::TextBottom => "text-bottom",
            Self::Alphabetic => "alphabetic",
            Self::Ideographic => "ideographic",
            Self::Middle => "middle",
            Self::Central => "central",
            Self::Mathematical => "mathematical",
            Self::TextTop => "text-top",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleAlignmentBaselineParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleAlignmentBaselineParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleAlignmentBaselineParseError<'a>, {
    InvalidValue(e) => format!("Invalid alignment-baseline value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleAlignmentBaselineParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleAlignmentBaselineParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleAlignmentBaselineParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleAlignmentBaselineParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleAlignmentBaselineParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleAlignmentBaselineParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleAlignmentBaselineParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleAlignmentBaselineParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `alignment-baseline` value.
pub fn parse_style_alignment_baseline(input: &str) -> Result<StyleAlignmentBaseline, StyleAlignmentBaselineParseError<'_>> {
    match input.trim() {
        "baseline" => Ok(StyleAlignmentBaseline::Baseline),
        "text-bottom" => Ok(StyleAlignmentBaseline::TextBottom),
        "alphabetic" => Ok(StyleAlignmentBaseline::Alphabetic),
        "ideographic" => Ok(StyleAlignmentBaseline::Ideographic),
        "middle" => Ok(StyleAlignmentBaseline::Middle),
        "central" => Ok(StyleAlignmentBaseline::Central),
        "mathematical" => Ok(StyleAlignmentBaseline::Mathematical),
        "text-top" => Ok(StyleAlignmentBaseline::TextTop),
        other => Err(StyleAlignmentBaselineParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleInitialLetterAlign --

/// Represents the `initial-letter-align` CSS property.
///
/// Specifies the alignment points used to align an initial letter.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleInitialLetterAlign {
    /// Automatically determine alignment based on script
    #[default]
    Auto,
    /// Align to the alphabetic baseline
    Alphabetic,
    /// Align to the hanging baseline
    Hanging,
    /// Align to the ideographic baseline
    Ideographic,
}
impl_option!(
    StyleInitialLetterAlign,
    OptionStyleInitialLetterAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleInitialLetterAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Alphabetic => "alphabetic",
            Self::Hanging => "hanging",
            Self::Ideographic => "ideographic",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleInitialLetterAlignParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleInitialLetterAlignParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleInitialLetterAlignParseError<'a>, {
    InvalidValue(e) => format!("Invalid initial-letter-align value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleInitialLetterAlignParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleInitialLetterAlignParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleInitialLetterAlignParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleInitialLetterAlignParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleInitialLetterAlignParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleInitialLetterAlignParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleInitialLetterAlignParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleInitialLetterAlignParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `initial-letter-align` value.
pub fn parse_style_initial_letter_align(input: &str) -> Result<StyleInitialLetterAlign, StyleInitialLetterAlignParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleInitialLetterAlign::Auto),
        "alphabetic" => Ok(StyleInitialLetterAlign::Alphabetic),
        "hanging" => Ok(StyleInitialLetterAlign::Hanging),
        "ideographic" => Ok(StyleInitialLetterAlign::Ideographic),
        other => Err(StyleInitialLetterAlignParseError::InvalidValue(InvalidValueErr(other))),
    }
}

// -- StyleInitialLetterWrap --

/// Represents the `initial-letter-wrap` CSS property.
///
/// Specifies how text adjacent to an initial letter wraps.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleInitialLetterWrap {
    /// No special wrapping around the initial letter
    #[default]
    None,
    /// Wrap only the first line adjacent to the initial letter
    First,
    /// Wrap all lines adjacent to the initial letter
    All,
    /// Wrap using a grid-based layout
    Grid,
}
impl_option!(
    StyleInitialLetterWrap,
    OptionStyleInitialLetterWrap,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleInitialLetterWrap {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::First => "first",
            Self::All => "all",
            Self::Grid => "grid",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleInitialLetterWrapParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleInitialLetterWrapParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleInitialLetterWrapParseError<'a>, {
    InvalidValue(e) => format!("Invalid initial-letter-wrap value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleInitialLetterWrapParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleInitialLetterWrapParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleInitialLetterWrapParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleInitialLetterWrapParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleInitialLetterWrapParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleInitialLetterWrapParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleInitialLetterWrapParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleInitialLetterWrapParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `initial-letter-wrap` value.
pub fn parse_style_initial_letter_wrap(input: &str) -> Result<StyleInitialLetterWrap, StyleInitialLetterWrapParseError<'_>> {
    match input.trim() {
        "none" => Ok(StyleInitialLetterWrap::None),
        "first" => Ok(StyleInitialLetterWrap::First),
        "all" => Ok(StyleInitialLetterWrap::All),
        "grid" => Ok(StyleInitialLetterWrap::Grid),
        other => Err(StyleInitialLetterWrapParseError::InvalidValue(InvalidValueErr(other))),
    }
}
