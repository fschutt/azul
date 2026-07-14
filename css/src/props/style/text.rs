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

// -- StyleTextTransform --

/// Controls capitalization of a text run (applied before shaping).
///
/// CSS Text Level 3 §2.1: <https://www.w3.org/TR/css-text-3/#text-transform-property>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleTextTransform {
    /// No capitalization effect.
    #[default]
    None,
    /// Uppercase the first typographic letter unit of each word.
    Capitalize,
    /// Uppercase every typographic letter unit.
    Uppercase,
    /// Lowercase every typographic letter unit.
    Lowercase,
    /// Map to the full-width form where available.
    FullWidth,
}
impl_option!(
    StyleTextTransform,
    OptionStyleTextTransform,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl PrintAsCssValue for StyleTextTransform {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::Capitalize => "capitalize",
            Self::Uppercase => "uppercase",
            Self::Lowercase => "lowercase",
            Self::FullWidth => "full-width",
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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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

// -- StyleTextTransform parse --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleTextTransformParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
#[cfg(feature = "parser")]
impl_debug_as_display!(StyleTextTransformParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { StyleTextTransformParseError<'a>, {
    InvalidValue(e) => format!("Invalid text-transform value: \"{}\"", e.0),
}}
#[cfg(feature = "parser")]
impl_from!(InvalidValueErr<'a>, StyleTextTransformParseError::InvalidValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextTransformParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}

#[cfg(feature = "parser")]
impl StyleTextTransformParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextTransformParseErrorOwned {
        match self {
            Self::InvalidValue(e) => StyleTextTransformParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextTransformParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextTransformParseError<'_> {
        match self {
            Self::InvalidValue(e) => StyleTextTransformParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-transform` value.
pub fn parse_style_text_transform(input: &str) -> Result<StyleTextTransform, StyleTextTransformParseError<'_>> {
    match input.trim() {
        "none" => Ok(StyleTextTransform::None),
        "capitalize" => Ok(StyleTextTransform::Capitalize),
        "uppercase" => Ok(StyleTextTransform::Uppercase),
        "lowercase" => Ok(StyleTextTransform::Lowercase),
        "full-width" => Ok(StyleTextTransform::FullWidth),
        other => Err(StyleTextTransformParseError::InvalidValue(InvalidValueErr(other))),
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines, clippy::cast_precision_loss)]
mod autotest_generated {
    use super::*;
    use crate::props::basic::length::SizeMetric;

    const OPAQUE_BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };
    const OPAQUE_WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
    const TRANSPARENT_BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };

    /// `FloatValue` encodes an f32 as `isize` scaled by this factor.
    const SCALE: isize = 1000;

    // =====================================================================
    // StyleTextIndent — constructors + numeric edges
    // =====================================================================

    #[test]
    fn text_indent_zero_is_the_neutral_element() {
        let z = StyleTextIndent::zero();
        assert_eq!(z, StyleTextIndent::default());
        assert_eq!(z.inner.metric, SizeMetric::Px);
        assert_eq!(z.inner.number.get(), 0.0);
        assert!(!z.each_line);
        assert!(!z.hanging);
        assert_eq!(z.print_as_css_value(), "0px");
    }

    #[test]
    fn text_indent_const_ctors_pin_metric_and_value() {
        // (constructed value, expected metric) for 0 / positive / negative.
        for v in [0_isize, 1, -1, 42, -42] {
            let cases = [
                (StyleTextIndent::const_px(v), SizeMetric::Px),
                (StyleTextIndent::const_em(v), SizeMetric::Em),
                (StyleTextIndent::const_pt(v), SizeMetric::Pt),
                (StyleTextIndent::const_percent(v), SizeMetric::Percent),
                (StyleTextIndent::const_in(v), SizeMetric::In),
                (StyleTextIndent::const_cm(v), SizeMetric::Cm),
                (StyleTextIndent::const_mm(v), SizeMetric::Mm),
            ];
            for (got, metric) in cases {
                assert_eq!(got.inner.metric, metric, "metric for {v}");
                assert_eq!(got.inner.number.get(), v as f32, "value for {v} {metric:?}");
                // The keyword flags are never set by the numeric constructors.
                assert!(!got.each_line && !got.hanging);
            }
        }
    }

    #[test]
    fn text_indent_const_ctors_hold_the_whole_encodable_isize_range() {
        // The isize encoding is `value * 1000`, so the representable input range is
        // isize::MIN/1000 ..= isize::MAX/1000. Pin the scale, then both ends of it.
        assert_eq!(StyleTextIndent::const_px(1).inner.number.number(), SCALE);

        let max = isize::MAX / SCALE;
        let min = isize::MIN / SCALE;
        assert_eq!(StyleTextIndent::const_px(max).inner.number.number(), max * SCALE);
        assert_eq!(StyleTextIndent::const_px(min).inner.number.number(), min * SCALE);
        assert_eq!(
            StyleTextIndent::const_from_metric(SizeMetric::Em, max).inner.number.number(),
            max * SCALE
        );
        // NOTE: one step past those bounds (e.g. `const_px(isize::MAX)`) overflows the
        // `value * 1000` multiply and panics in debug. See the report — not asserted here
        // because the behaviour differs between debug (panic) and release (wrap).
    }

    #[test]
    fn text_indent_float_ctors_saturate_on_nan_and_infinity() {
        // f32 -> isize is a saturating `as` cast: NaN -> 0, +inf -> MAX, -inf -> MIN.
        assert_eq!(StyleTextIndent::px(f32::NAN).inner.number.get(), 0.0);
        assert_eq!(StyleTextIndent::em(f32::NAN).inner.number.get(), 0.0);

        assert_eq!(StyleTextIndent::px(f32::INFINITY).inner.number.number(), isize::MAX);
        assert_eq!(StyleTextIndent::px(f32::NEG_INFINITY).inner.number.number(), isize::MIN);
        assert_eq!(StyleTextIndent::pt(f32::MAX).inner.number.number(), isize::MAX);
        assert_eq!(StyleTextIndent::pt(-f32::MAX).inner.number.number(), isize::MIN);

        // Sub-precision magnitudes collapse to zero rather than trapping.
        assert_eq!(StyleTextIndent::percent(f32::MIN_POSITIVE).inner.number.number(), 0);
        assert_eq!(StyleTextIndent::px(-0.0).inner.number.number(), 0);

        // Every saturated result is still a finite, readable f32.
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, -f32::MAX] {
            assert!(StyleTextIndent::px(v).inner.number.get().is_finite());
        }
    }

    #[test]
    fn text_indent_from_metric_agrees_with_the_typed_ctors() {
        assert_eq!(StyleTextIndent::from_metric(SizeMetric::Px, 1.5), StyleTextIndent::px(1.5));
        assert_eq!(StyleTextIndent::from_metric(SizeMetric::Em, -2.5), StyleTextIndent::em(-2.5));
        assert_eq!(StyleTextIndent::from_metric(SizeMetric::Pt, 0.0), StyleTextIndent::pt(0.0));
        assert_eq!(
            StyleTextIndent::from_metric(SizeMetric::Percent, 50.0),
            StyleTextIndent::percent(50.0)
        );
        assert_eq!(StyleTextIndent::const_from_metric(SizeMetric::Cm, 3), StyleTextIndent::const_cm(3));
        assert_eq!(StyleTextIndent::const_from_metric(SizeMetric::Mm, -3), StyleTextIndent::const_mm(-3));

        // A metric with no typed ctor still round-trips through from_metric.
        let vw = StyleTextIndent::from_metric(SizeMetric::Vw, 10.0);
        assert_eq!(vw.inner.metric, SizeMetric::Vw);
        assert_eq!(vw.inner.number.get(), 10.0);
    }

    #[test]
    fn text_indent_interpolate_endpoints_and_extrapolation() {
        let a = StyleTextIndent::px(0.0);
        let b = StyleTextIndent::px(100.0);

        assert_eq!(a.interpolate(&b, 0.0).inner.number.get(), 0.0);
        assert_eq!(a.interpolate(&b, 1.0).inner.number.get(), 100.0);
        assert_eq!(a.interpolate(&b, 0.5).inner.number.get(), 50.0);
        // t outside [0,1] extrapolates rather than clamping.
        assert_eq!(a.interpolate(&b, -1.0).inner.number.get(), -100.0);
        assert_eq!(a.interpolate(&b, 2.0).inner.number.get(), 200.0);
        // Interpolating a value with itself is the identity for any finite t.
        assert_eq!(b.interpolate(&b, 0.25), b);
    }

    #[test]
    fn text_indent_interpolate_with_nonfinite_t_is_defined() {
        let a = StyleTextIndent::px(0.0);
        let b = StyleTextIndent::px(100.0);

        // NaN propagates into the f32 -> isize cast, which saturates NaN to 0.
        assert_eq!(a.interpolate(&b, f32::NAN).inner.number.get(), 0.0);
        // +/-inf saturate to the isize bounds instead of panicking.
        assert_eq!(a.interpolate(&b, f32::INFINITY).inner.number.number(), isize::MAX);
        assert_eq!(a.interpolate(&b, f32::NEG_INFINITY).inner.number.number(), isize::MIN);
        // 0 * inf is NaN, so interpolating equal endpoints by inf collapses to zero.
        assert_eq!(b.interpolate(&b, f32::INFINITY).inner.number.get(), 0.0);

        for t in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
            assert!(a.interpolate(&b, t).inner.number.get().is_finite());
        }
    }

    #[test]
    fn text_indent_interpolate_keeps_self_flags_and_normalizes_mixed_metrics() {
        let a = StyleTextIndent { each_line: true, hanging: true, ..StyleTextIndent::px(0.0) };
        let b = StyleTextIndent { each_line: false, hanging: false, ..StyleTextIndent::px(10.0) };

        // Flags are taken from `self`, never blended.
        let mid = a.interpolate(&b, 0.5);
        assert!(mid.each_line && mid.hanging);
        assert!(!b.interpolate(&a, 0.5).each_line);

        // Mismatched metrics are resolved into px.
        let mixed = StyleTextIndent::px(10.0).interpolate(&StyleTextIndent::em(2.0), 0.5);
        assert_eq!(mixed.inner.metric, SizeMetric::Px);
        assert!(mixed.inner.number.get().is_finite());
    }

    // =====================================================================
    // StyleTextColor::interpolate
    // =====================================================================

    #[test]
    fn text_color_interpolate_endpoints_are_exact() {
        let a = StyleTextColor { inner: OPAQUE_BLACK };
        let b = StyleTextColor { inner: OPAQUE_WHITE };

        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        // 0 + 255*0.5 = 127.5, rounded half-away-from-zero.
        assert_eq!(a.interpolate(&b, 0.5).inner, ColorU { r: 128, g: 128, b: 128, a: 255 });
        assert_eq!(a.print_as_css_value(), "#000000ff");
    }

    #[test]
    fn text_color_interpolate_saturates_out_of_range_t() {
        let a = StyleTextColor { inner: OPAQUE_BLACK };
        let b = StyleTextColor { inner: OPAQUE_WHITE };

        // 0 + 255*2 = 510 -> clamped to 255 by the saturating u8 cast (no wrap to 254).
        assert_eq!(a.interpolate(&b, 2.0).inner, OPAQUE_WHITE);
        // 0 + 255*-1 = -255 -> clamped to 0 (no wrap to 1).
        assert_eq!(a.interpolate(&b, -1.0).inner, OPAQUE_BLACK);
    }

    #[test]
    fn text_color_interpolate_with_nonfinite_t_is_defined() {
        let a = StyleTextColor { inner: OPAQUE_BLACK };
        let b = StyleTextColor { inner: OPAQUE_WHITE };

        // NaN saturates to 0 in every channel — including alpha, so the result is
        // transparent black rather than a panic or garbage.
        assert_eq!(a.interpolate(&b, f32::NAN).inner, TRANSPARENT_BLACK);

        // With t = +inf the changing channels saturate to 255, but alpha is *equal* in
        // both endpoints, so it computes 255 + (0 * inf) = NaN and saturates to 0.
        // A fully-opaque pair therefore interpolates to a fully-transparent colour.
        assert_eq!(
            a.interpolate(&b, f32::INFINITY).inner,
            ColorU { r: 255, g: 255, b: 255, a: 0 }
        );
        assert_eq!(
            a.interpolate(&b, f32::NEG_INFINITY).inner,
            ColorU { r: 0, g: 0, b: 0, a: 0 }
        );
    }

    // =====================================================================
    // StyleHangingPunctuation::is_enabled (predicate invariants)
    // =====================================================================

    #[test]
    fn hanging_punctuation_is_enabled_matches_its_flags_exhaustively() {
        assert!(!StyleHangingPunctuation::default().is_enabled());

        for bits in 0_u8..16 {
            let hp = StyleHangingPunctuation {
                first: bits & 1 != 0,
                force_end: bits & 2 != 0,
                allow_end: bits & 4 != 0,
                last: bits & 8 != 0,
            };
            assert_eq!(hp.is_enabled(), bits != 0, "bits={bits}");
            // is_enabled() is exactly the "prints as something other than none" predicate.
            assert_eq!(hp.print_as_css_value() == "none", !hp.is_enabled(), "bits={bits}");
        }
    }

    #[test]
    fn hanging_punctuation_prints_flags_in_spec_order() {
        let all = StyleHangingPunctuation { first: true, force_end: true, allow_end: true, last: true };
        assert_eq!(all.print_as_css_value(), "first force-end allow-end last");
        assert_eq!(
            StyleHangingPunctuation { last: true, ..Default::default() }.print_as_css_value(),
            "last"
        );
    }

    // =====================================================================
    // Parser-gated tests
    // =====================================================================

    #[cfg(feature = "parser")]
    mod parser {
        use super::super::*;
        use crate::props::basic::length::SizeMetric;

        const GARBAGE: &[&str] = &[
            "",
            "   ",
            "\t\n",
            "!!!",
            "\0\0",
            "0",
            "-0",
            "9223372036854775807",
            "1e400",
            "NaN",
            "inf",
            "\u{1F600}",
            "e\u{0301}",
            "\u{202E}left",
            "left;garbage",
            "left garbage",
        ];

        /// Every keyword parser must reject junk, and accept its own printed form.
        macro_rules! assert_keyword_round_trip {
            ($parse:ident, $variants:expr) => {{
                for v in $variants {
                    let printed = v.print_as_css_value();
                    assert_eq!($parse(&printed).as_ref(), Ok(&v), "round-trip of {printed:?}");
                    // Surrounding whitespace is trimmed, not rejected.
                    assert_eq!($parse(&format!("  {printed}  ")).as_ref(), Ok(&v));
                }
                for g in GARBAGE.iter().copied() {
                    assert!($parse(g).is_err(), "{} accepted garbage {g:?}", stringify!($parse));
                }
            }};
        }

        #[test]
        fn keyword_parsers_round_trip_every_variant_and_reject_garbage() {
            type TA = StyleTextAlign;
            assert_keyword_round_trip!(
                parse_style_text_align,
                [TA::Left, TA::Center, TA::Right, TA::Justify, TA::Start, TA::End]
            );
            type WS = StyleWhiteSpace;
            assert_keyword_round_trip!(
                parse_style_white_space,
                [WS::Normal, WS::Pre, WS::Nowrap, WS::PreWrap, WS::PreLine, WS::BreakSpaces]
            );
            type H = StyleHyphens;
            assert_keyword_round_trip!(parse_style_hyphens, [H::None, H::Manual, H::Auto]);
            type LB = StyleLineBreak;
            assert_keyword_round_trip!(
                parse_style_line_break,
                [LB::Auto, LB::Loose, LB::Normal, LB::Strict, LB::Anywhere]
            );
            type WB = StyleWordBreak;
            assert_keyword_round_trip!(
                parse_style_word_break,
                [WB::Normal, WB::BreakAll, WB::KeepAll, WB::BreakWord]
            );
            type OW = StyleOverflowWrap;
            assert_keyword_round_trip!(
                parse_style_overflow_wrap,
                [OW::Normal, OW::Anywhere, OW::BreakWord]
            );
            type TAL = StyleTextAlignLast;
            assert_keyword_round_trip!(
                parse_style_text_align_last,
                [TAL::Auto, TAL::Start, TAL::End, TAL::Left, TAL::Right, TAL::Center, TAL::Justify]
            );
            type TT = StyleTextTransform;
            assert_keyword_round_trip!(
                parse_style_text_transform,
                [TT::None, TT::Capitalize, TT::Uppercase, TT::Lowercase, TT::FullWidth]
            );
            type D = StyleDirection;
            assert_keyword_round_trip!(parse_style_direction, [D::Ltr, D::Rtl]);
            type US = StyleUserSelect;
            assert_keyword_round_trip!(
                parse_style_user_select,
                [US::Auto, US::Text, US::None, US::All]
            );
            type TD = StyleTextDecoration;
            assert_keyword_round_trip!(
                parse_style_text_decoration,
                [TD::None, TD::Underline, TD::Overline, TD::LineThrough]
            );
            type UB = StyleUnicodeBidi;
            assert_keyword_round_trip!(
                parse_style_unicode_bidi,
                [UB::Normal, UB::Embed, UB::Isolate, UB::BidiOverride, UB::IsolateOverride, UB::Plaintext]
            );
            type TBT = StyleTextBoxTrim;
            assert_keyword_round_trip!(
                parse_style_text_box_trim,
                [TBT::None, TBT::TrimStart, TBT::TrimEnd, TBT::TrimBoth]
            );
            type TBE = StyleTextBoxEdge;
            assert_keyword_round_trip!(
                parse_style_text_box_edge,
                [TBE::Auto, TBE::TextEdge, TBE::CapHeight, TBE::ExHeight]
            );
            type DB = StyleDominantBaseline;
            assert_keyword_round_trip!(
                parse_style_dominant_baseline,
                [
                    DB::Auto, DB::TextBottom, DB::Alphabetic, DB::Ideographic, DB::Middle,
                    DB::Central, DB::Mathematical, DB::Hanging, DB::TextTop
                ]
            );
            type AB = StyleAlignmentBaseline;
            assert_keyword_round_trip!(
                parse_style_alignment_baseline,
                [
                    AB::Baseline, AB::TextBottom, AB::Alphabetic, AB::Ideographic, AB::Middle,
                    AB::Central, AB::Mathematical, AB::TextTop
                ]
            );
            type ILA = StyleInitialLetterAlign;
            assert_keyword_round_trip!(
                parse_style_initial_letter_align,
                [ILA::Auto, ILA::Alphabetic, ILA::Hanging, ILA::Ideographic]
            );
            type ILW = StyleInitialLetterWrap;
            assert_keyword_round_trip!(
                parse_style_initial_letter_wrap,
                [ILW::None, ILW::First, ILW::All, ILW::Grid]
            );
        }

        #[test]
        fn keyword_parsers_are_case_sensitive() {
            // BUG: CSS keywords are ASCII case-insensitive (CSS Syntax 3 §3.1), but every
            // `match input.trim()` parser in this file compares exactly, so `text-align: LEFT`
            // is rejected. `hanging-punctuation` / `text-combine-upright` *do* fold case, so
            // the file is internally inconsistent too. Pinned as-is; see the report.
            assert!(parse_style_text_align("LEFT").is_err());
            assert!(parse_style_text_align("Left").is_err());
            assert!(parse_style_white_space("Normal").is_err());
            assert!(parse_style_direction("LTR").is_err());
            // ...whereas these two fold case as the spec requires:
            assert!(parse_style_hanging_punctuation("FIRST").is_ok());
            assert!(parse_style_text_combine_upright("NONE").is_ok());
        }

        #[test]
        fn extremely_long_and_deeply_nested_input_terminates_with_err() {
            let long = "a".repeat(1_000_000);
            assert!(parse_style_text_align(&long).is_err());
            assert!(parse_style_white_space(&long).is_err());
            assert!(parse_style_text_color(&long).is_err());
            assert!(parse_style_letter_spacing(&long).is_err());
            assert!(parse_style_word_spacing(&long).is_err());
            assert!(parse_style_tab_size(&long).is_err());
            assert!(parse_style_line_height(&long).is_err());
            assert!(parse_style_text_indent(&long).is_err());
            assert!(parse_style_hanging_punctuation(&long).is_err());
            assert!(parse_style_initial_letter(&long).is_err());
            assert!(parse_style_line_clamp(&long).is_err());
            assert!(parse_style_vertical_align(&long).is_err());

            // A 1000-digit integer overflows every numeric target -> Err, never a wrap.
            let huge_number = "9".repeat(1000);
            assert!(parse_style_line_clamp(&huge_number).is_err());
            assert!(parse_style_initial_letter(&huge_number).is_err());

            // No parser here recurses, so nesting cannot blow the stack.
            let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_style_text_align(&nested).is_err());
            assert!(parse_style_letter_spacing(&nested).is_err());
            assert!(parse_style_text_indent(&nested).is_err());
            assert!(parse_style_line_height(&nested).is_err());
        }

        // -- pixel-valued properties -------------------------------------------

        #[test]
        fn spacing_and_tab_size_round_trip_through_their_printed_form() {
            for pv in [
                PixelValue::px(2.0),
                PixelValue::px(-3.0),
                PixelValue::em(0.5),
                PixelValue::pt(12.0),
                PixelValue::percent(10.0),
                PixelValue::zero(),
            ] {
                let ls = StyleLetterSpacing { inner: pv };
                assert_eq!(parse_style_letter_spacing(&ls.print_as_css_value()).unwrap(), ls);
                let ws = StyleWordSpacing { inner: pv };
                assert_eq!(parse_style_word_spacing(&ws.print_as_css_value()).unwrap(), ws);
            }
            // tab-size: unitless numbers mean `em`, lengths keep their unit.
            let ts = StyleTabSize::default();
            assert_eq!(ts.inner, PixelValue::em(8.0));
            assert_eq!(parse_style_tab_size(&ts.print_as_css_value()).unwrap(), ts);
            assert_eq!(parse_style_tab_size("4").unwrap().inner, PixelValue::em(4.0));
            assert_eq!(parse_style_tab_size("20px").unwrap().inner, PixelValue::px(20.0));
        }

        #[test]
        fn pixel_parsers_reject_empty_and_unit_only_input() {
            for bad in ["", "   ", "\t\n", "px", "em", "abc", "px px", "10pxx", "\u{1F600}"] {
                assert!(parse_style_letter_spacing(bad).is_err(), "letter-spacing {bad:?}");
                assert!(parse_style_word_spacing(bad).is_err(), "word-spacing {bad:?}");
            }
            // BUG: the shared pixel parser trims *after* stripping the unit suffix, so a space
            // between the number and its unit is accepted even though CSS forbids it.
            assert_eq!(parse_style_letter_spacing("10 px").unwrap().inner, PixelValue::px(10.0));
        }

        #[test]
        fn pixel_parsers_accept_float_keywords_and_saturate_instead_of_panicking() {
            // BUG: `f32::from_str` accepts "NaN"/"inf"/"1e400", so these are *not* rejected
            // as CSS lengths. They cannot panic (the isize cast saturates), but they should
            // be Err. Pinned as-is; see the report.
            assert_eq!(parse_style_letter_spacing("NaN").unwrap().inner.number.number(), 0);
            assert_eq!(
                parse_style_letter_spacing("inf").unwrap().inner.number.number(),
                isize::MAX
            );
            assert_eq!(
                parse_style_letter_spacing("-inf").unwrap().inner.number.number(),
                isize::MIN
            );
            assert_eq!(
                parse_style_word_spacing("1e400px").unwrap().inner.number.number(),
                isize::MAX
            );
            // Same story via the tab-size unitless branch.
            assert_eq!(parse_style_tab_size("NaN").unwrap().inner, PixelValue::em(0.0));
            assert!(parse_style_tab_size("inf").unwrap().inner.number.get().is_finite());

            // Boundary numeric strings that *are* legal still parse to the right value.
            assert_eq!(parse_style_letter_spacing("0").unwrap().inner, PixelValue::px(0.0));
            assert_eq!(parse_style_letter_spacing("-0").unwrap().inner.number.number(), 0);
            assert!(parse_style_letter_spacing("9223372036854775807px").unwrap().inner.number.get().is_finite());
        }

        // -- text-indent --------------------------------------------------------

        #[test]
        fn text_indent_round_trips_length_and_keywords() {
            for pv in [PixelValue::px(10.0), PixelValue::em(-2.0), PixelValue::percent(50.0)] {
                for (each_line, hanging) in [(false, false), (true, false), (false, true), (true, true)] {
                    let ti = StyleTextIndent { inner: pv, each_line, hanging };
                    let printed = ti.print_as_css_value();
                    assert_eq!(parse_style_text_indent(&printed).unwrap(), ti, "{printed:?}");
                }
            }
            assert!(parse_style_text_indent("10px hanging").unwrap().hanging);
            assert!(parse_style_text_indent("each-line 10px").unwrap().each_line);
        }

        #[test]
        fn text_indent_defaults_missing_length_to_zero_and_keeps_the_last_one() {
            // BUG: `text-indent` requires a <length-percentage>; these should all be Err.
            // Instead an absent length silently defaults to 0px, so empty/whitespace/keyword-
            // only input parses Ok. Pinned as-is; see the report.
            assert_eq!(parse_style_text_indent("").unwrap(), StyleTextIndent::zero());
            assert_eq!(parse_style_text_indent("   ").unwrap(), StyleTextIndent::zero());
            assert_eq!(
                parse_style_text_indent("hanging").unwrap(),
                StyleTextIndent { hanging: true, ..StyleTextIndent::zero() }
            );
            // BUG: repeated lengths are not rejected — the last token silently wins.
            assert_eq!(parse_style_text_indent("10px 20px").unwrap().inner, PixelValue::px(20.0));

            // Junk in the length slot is still rejected.
            assert!(parse_style_text_indent("garbage").is_err());
            assert!(parse_style_text_indent("hanging garbage").is_err());
        }

        // -- initial-letter -----------------------------------------------------

        #[test]
        fn initial_letter_parses_size_and_optional_sink() {
            assert_eq!(
                parse_style_initial_letter("3").unwrap(),
                StyleInitialLetter { size: 3, sink: crate::corety::OptionU32::None }
            );
            let with_sink = StyleInitialLetter { size: 3, sink: crate::corety::OptionU32::Some(2) };
            assert_eq!(parse_style_initial_letter("3 2").unwrap(), with_sink);
            // Printed form round-trips.
            assert_eq!(parse_style_initial_letter(&with_sink.print_as_css_value()).unwrap(), with_sink);
        }

        #[test]
        fn initial_letter_rejects_zero_negative_and_overflowing_sizes() {
            assert!(parse_style_initial_letter("0").is_err(), "size 0 must be rejected");
            assert!(parse_style_initial_letter("-1").is_err());
            assert!(parse_style_initial_letter("1.5").is_err());
            assert!(parse_style_initial_letter("4294967296").is_err(), "u32::MAX + 1");
            assert!(parse_style_initial_letter("3 -1").is_err(), "negative sink");
            assert!(parse_style_initial_letter("3 x").is_err());
            assert!(parse_style_initial_letter("").is_err());
            assert!(parse_style_initial_letter("   ").is_err());
            // u32::MAX itself is in range.
            assert_eq!(parse_style_initial_letter("4294967295").unwrap().size, u32::MAX);
            // BUG: a third component should be a parse error, but it is silently dropped.
            assert_eq!(parse_style_initial_letter("3 2 9").unwrap(), StyleInitialLetter {
                size: 3,
                sink: crate::corety::OptionU32::Some(2),
            });
        }

        // -- line-clamp ---------------------------------------------------------

        #[test]
        fn line_clamp_rejects_zero_and_out_of_range_values() {
            assert_eq!(parse_style_line_clamp("3").unwrap(), StyleLineClamp { max_lines: 3 });
            assert_eq!(parse_style_line_clamp("  7  ").unwrap().max_lines, 7);
            assert_eq!(
                parse_style_line_clamp("0").unwrap_err(),
                StyleLineClampParseError::ZeroValue
            );
            assert!(parse_style_line_clamp("-1").is_err());
            assert!(parse_style_line_clamp("1.0").is_err());
            assert!(parse_style_line_clamp("").is_err());
            assert!(parse_style_line_clamp("   ").is_err());
            assert!(parse_style_line_clamp("\u{1F600}").is_err());
            // Saturating/wrapping never happens: an out-of-range integer is an error.
            assert!(parse_style_line_clamp("99999999999999999999999").is_err());
            let max = usize::MAX.to_string();
            assert_eq!(parse_style_line_clamp(&max).unwrap().max_lines, usize::MAX);
            // Printed form round-trips.
            let lc = StyleLineClamp { max_lines: 42 };
            assert_eq!(parse_style_line_clamp(&lc.print_as_css_value()).unwrap(), lc);
        }

        // -- hanging-punctuation ------------------------------------------------

        #[test]
        fn hanging_punctuation_round_trips_and_enforces_mutual_exclusion() {
            for bits in 0_u8..16 {
                let hp = StyleHangingPunctuation {
                    first: bits & 1 != 0,
                    force_end: bits & 2 != 0,
                    allow_end: bits & 4 != 0,
                    last: bits & 8 != 0,
                };
                let printed = hp.print_as_css_value();
                if hp.force_end && hp.allow_end {
                    // `force-end` and `allow-end` are mutually exclusive per CSS Text 3 §8.
                    assert!(parse_style_hanging_punctuation(&printed).is_err(), "{printed:?}");
                } else {
                    assert_eq!(parse_style_hanging_punctuation(&printed).unwrap(), hp, "{printed:?}");
                }
            }
            assert!(parse_style_hanging_punctuation("first bogus").is_err());
            assert!(parse_style_hanging_punctuation("\u{1F600}").is_err());
            assert!(parse_style_hanging_punctuation("none first").is_err());
        }

        #[test]
        fn hanging_punctuation_accepts_empty_input_as_none() {
            // BUG: empty / whitespace-only input has no tokens, so the loop body never runs
            // and the parser returns Ok(none) instead of Err. Pinned as-is; see the report.
            assert_eq!(
                parse_style_hanging_punctuation("").unwrap(),
                StyleHangingPunctuation::default()
            );
            assert_eq!(
                parse_style_hanging_punctuation("   ").unwrap(),
                StyleHangingPunctuation::default()
            );
            // Duplicate keywords are also accepted (idempotent flag set).
            assert!(parse_style_hanging_punctuation("first first").unwrap().first);
        }

        // -- text-combine-upright -----------------------------------------------

        #[test]
        fn text_combine_upright_bounds_the_digits_operand() {
            assert_eq!(parse_style_text_combine_upright("none").unwrap(), StyleTextCombineUpright::None);
            assert_eq!(parse_style_text_combine_upright("all").unwrap(), StyleTextCombineUpright::All);
            for n in 2_u8..=4 {
                let v = StyleTextCombineUpright::Digits(n);
                assert_eq!(parse_style_text_combine_upright(&v.print_as_css_value()).unwrap(), v);
            }
            // Outside the spec'd 2..=4 range -> Err, not a silent clamp or wrap.
            for bad in ["digits 0", "digits 1", "digits 5", "digits 255", "digits 256", "digits -1"] {
                assert!(parse_style_text_combine_upright(bad).is_err(), "{bad:?} accepted");
            }
            assert!(parse_style_text_combine_upright("").is_err());
            assert!(parse_style_text_combine_upright("bogus").is_err());
        }

        #[test]
        fn text_combine_upright_accepts_garbage_after_the_digits_prefix() {
            // BUG: the `digits` branch is chosen by `starts_with("digits")` with no word
            // boundary, and any token count != 2 falls back to `digits 2`. So junk that
            // merely starts with "digits" parses Ok. Pinned as-is; see the report.
            assert_eq!(
                parse_style_text_combine_upright("digits").unwrap(),
                StyleTextCombineUpright::Digits(2)
            );
            assert_eq!(
                parse_style_text_combine_upright("digitsgarbage").unwrap(),
                StyleTextCombineUpright::Digits(2)
            );
            assert_eq!(
                parse_style_text_combine_upright("digits 2 3").unwrap(),
                StyleTextCombineUpright::Digits(2)
            );
        }

        // -- line-height --------------------------------------------------------

        #[test]
        fn line_height_parses_numbers_percentages_and_px() {
            assert_eq!(parse_style_line_height("1.5").unwrap().inner, PercentageValue::new(150.0));
            assert_eq!(parse_style_line_height("120%").unwrap().inner, PercentageValue::new(120.0));
            // px lengths are encoded as a *negative* percentage (documented convention).
            assert_eq!(parse_style_line_height("20px").unwrap().inner, PercentageValue::new(-2000.0));
            assert!(parse_style_line_height("").is_err());
            assert!(parse_style_line_height("   ").is_err());
            assert!(parse_style_line_height("abc").is_err());
            assert!(parse_style_line_height("\u{1F600}").is_err());
            // Printed form round-trips as a value.
            let lh = StyleLineHeight::default();
            assert_eq!(parse_style_line_height(&lh.print_as_css_value()).unwrap(), lh);
        }

        #[test]
        fn line_height_negative_numbers_alias_absolute_px_lengths() {
            // BUG: negative values are the internal marker for "absolute px", but the number
            // branch happily parses a negative <number>, so `line-height: -1` and
            // `line-height: 1px` produce the *same* value and are indistinguishable
            // downstream. A negative line-height is invalid CSS and should be Err.
            assert_eq!(
                parse_style_line_height("-1").unwrap(),
                parse_style_line_height("1px").unwrap()
            );
            assert_eq!(
                parse_style_line_height("-100%").unwrap(),
                parse_style_line_height("1px").unwrap()
            );
        }

        #[test]
        fn line_height_rejects_em_and_other_length_units() {
            // BUG: `line-height: 1.5em` (and rem/pt/...) is valid CSS but only Px survives
            // the length branch, so every other unit is rejected. Pinned as-is.
            assert!(parse_style_line_height("1.5em").is_err());
            assert!(parse_style_line_height("12pt").is_err());
            assert!(parse_style_line_height("2rem").is_err());
        }

        #[test]
        #[ignore = "BUG: panics — parse_percentage_value slices at a non-char-boundary on \
                    non-ASCII numerals (e.g. U+FF15). Un-ignore once length.rs is fixed."]
        fn line_height_rejects_non_ascii_numerals_without_panicking() {
            // `char::is_numeric()` is true for U+FF15 FULLWIDTH DIGIT FIVE, so
            // parse_percentage_value sets split_pos = idx + 1 = 1 and then slices
            // `input[1..]` — a byte index inside a 3-byte char -> panic.
            // Any of these is a CSS-reachable crash:
            assert!(parse_style_line_height("\u{FF15}").is_err()); // fullwidth 5
            assert!(parse_style_line_height("\u{0665}").is_err()); // arabic-indic 5
            assert!(parse_style_line_height("1\u{00B2}").is_err()); // superscript 2
        }

        // -- vertical-align -----------------------------------------------------

        #[test]
        fn vertical_align_round_trips_keywords_percentages_and_lengths() {
            type VA = StyleVerticalAlign;
            for v in [
                VA::Baseline, VA::Top, VA::Middle, VA::Bottom, VA::Sub, VA::Superscript,
                VA::TextTop, VA::TextBottom,
                VA::Percentage(PercentageValue::new(50.0)),
                VA::Percentage(PercentageValue::new(-25.0)),
                VA::Length(PixelValue::px(12.0)),
                VA::Length(PixelValue::em(1.5)),
            ] {
                let printed = v.print_as_css_value();
                assert_eq!(parse_style_vertical_align(&printed).unwrap(), v, "{printed:?}");
            }
            assert!(parse_style_vertical_align("").is_err());
            assert!(parse_style_vertical_align("%").is_err());
            assert!(parse_style_vertical_align("bogus%").is_err());
            assert!(parse_style_vertical_align("\u{1F600}").is_err());
        }

        // -- caret-* helpers ----------------------------------------------------

        #[test]
        fn caret_parsers_reject_garbage_and_accept_minimal_input() {
            assert_eq!(parse_caret_color("red").unwrap().inner, parse_style_text_color("red").unwrap().inner);
            assert!(parse_caret_color("").is_err());
            assert!(parse_caret_color("not-a-color").is_err());
            assert_eq!(parse_caret_width("2px").unwrap().inner, PixelValue::px(2.0));
            assert!(parse_caret_width("").is_err());
            assert!(parse_caret_animation_duration("bogus").is_err());
            assert!(parse_caret_animation_duration("500ms").is_ok());
        }

        // -- error type getters: to_contained / to_shared ------------------------

        /// Owned<->shared conversion must be lossless for every error family here.
        macro_rules! assert_error_round_trip {
            ($parse:ident, $($bad:expr),+ $(,)?) => {{
                $(
                    let e = $parse($bad).expect_err(concat!(stringify!($parse), " accepted ", $bad));
                    assert_eq!(e.to_contained().to_shared(), e, "{:?} via {}", $bad, stringify!($parse));
                )+
            }};
        }

        #[test]
        fn invalid_value_errors_round_trip_through_their_owned_form() {
            assert_error_round_trip!(parse_style_text_align, "", "middle", "\u{1F600}");
            assert_error_round_trip!(parse_style_white_space, "", "wrap");
            assert_error_round_trip!(parse_style_hyphens, "", "always");
            assert_error_round_trip!(parse_style_line_break, "", "tight");
            assert_error_round_trip!(parse_style_word_break, "", "break");
            assert_error_round_trip!(parse_style_overflow_wrap, "", "wrap");
            assert_error_round_trip!(parse_style_text_align_last, "", "middle");
            assert_error_round_trip!(parse_style_text_transform, "", "smallcaps");
            assert_error_round_trip!(parse_style_direction, "", "sideways");
            assert_error_round_trip!(parse_style_user_select, "", "some");
            assert_error_round_trip!(parse_style_text_decoration, "", "blink");
            assert_error_round_trip!(parse_style_vertical_align, "", "bogus");
            assert_error_round_trip!(parse_style_unicode_bidi, "", "override");
            assert_error_round_trip!(parse_style_text_box_trim, "", "trim");
            assert_error_round_trip!(parse_style_text_box_edge, "", "edge");
            assert_error_round_trip!(parse_style_dominant_baseline, "", "bogus");
            assert_error_round_trip!(parse_style_alignment_baseline, "", "bogus");
            assert_error_round_trip!(parse_style_initial_letter_align, "", "bogus");
            assert_error_round_trip!(parse_style_initial_letter_wrap, "", "bogus");
        }

        #[test]
        fn pixel_and_numeric_errors_round_trip_through_their_owned_form() {
            // EmptyString / ValueParseErr / NoValueGiven / InvalidPixelValue variants.
            assert_error_round_trip!(parse_style_letter_spacing, "", "abcpx", "px", "zz");
            assert_error_round_trip!(parse_style_word_spacing, "", "abcem", "em", "zz");
            assert_error_round_trip!(parse_style_text_indent, "abcpx", "zz");
            assert_error_round_trip!(parse_style_tab_size, "", "abcpx", "zz");
            assert_error_round_trip!(parse_style_line_height, "", "abc", "1.5em");
            assert_error_round_trip!(parse_style_initial_letter, "", "x", "0", "3 x");
            assert_error_round_trip!(parse_style_line_clamp, "", "x", "0");
            assert_error_round_trip!(parse_style_hanging_punctuation, "bogus", "force-end allow-end");
            assert_error_round_trip!(parse_style_text_combine_upright, "bogus", "digits 9");
        }

        #[test]
        fn text_color_error_round_trips_and_preserves_its_message() {
            let e = parse_style_text_color("not-a-color").unwrap_err();
            let owned = e.to_contained();
            let round_tripped = owned.to_shared();
            assert_eq!(format!("{e}"), format!("{round_tripped}"));
            assert!(parse_style_text_color("").is_err());
            assert!(parse_style_text_color("#gggggg").is_err());
            assert!(parse_style_text_color("\u{1F600}").is_err());
            // Positive control.
            assert_eq!(parse_style_text_color("#aabbcc").unwrap().inner.to_hash(), "#aabbccff");
        }

        #[test]
        fn error_types_survive_a_default_ish_extreme_instance() {
            // to_contained/to_shared must not panic on empty or huge payloads.
            let long = "z".repeat(100_000);
            let e = parse_style_text_align(&long).unwrap_err();
            assert_eq!(e.to_contained().to_shared(), e);
            let e = parse_style_line_clamp(&long).unwrap_err();
            assert_eq!(e.to_contained().to_shared(), e);
            let e = parse_style_hanging_punctuation(&long).unwrap_err();
            assert_eq!(e.to_contained().to_shared(), e);
            // Empty payload.
            let e = parse_style_letter_spacing("").unwrap_err();
            assert_eq!(e.to_contained().to_shared(), e);
        }

        // -- metric coverage ----------------------------------------------------

        #[test]
        fn letter_spacing_accepts_every_size_metric_it_prints() {
            for (unit, metric) in [
                ("px", SizeMetric::Px),
                ("pt", SizeMetric::Pt),
                ("em", SizeMetric::Em),
                ("rem", SizeMetric::Rem),
                ("in", SizeMetric::In),
                ("cm", SizeMetric::Cm),
                ("mm", SizeMetric::Mm),
                ("%", SizeMetric::Percent),
                ("vw", SizeMetric::Vw),
                ("vh", SizeMetric::Vh),
                ("vmax", SizeMetric::Vmax),
            ] {
                let parsed = parse_style_letter_spacing(&format!("1{unit}")).unwrap();
                assert_eq!(parsed.inner.metric, metric, "unit {unit}");
                assert_eq!(parsed.inner.number.get(), 1.0);
            }
            // BUG: `vmin` is unreachable — the suffix table tries "in" before "vmin", so
            // "1vmin" is stripped to "1vm" and fails to parse. Every property backed by
            // parse_pixel_value (letter-spacing, word-spacing, text-indent, tab-size,
            // vertical-align) therefore rejects a valid CSS unit. See the report.
            assert!(parse_style_letter_spacing("1vmin").is_err());
        }
    }
}
