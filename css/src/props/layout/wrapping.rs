//! CSS properties for text wrapping and writing modes.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

// --- flex-wrap (LayoutWrap) ---

/// Represents a `flex-wrap` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        LayoutWrap::NoWrap
    }
}

impl core::fmt::Debug for LayoutWrap {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutWrap {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutWrap {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutWrap::NoWrap => "nowrap".to_string(),
            LayoutWrap::Wrap => "wrap".to_string(),
            LayoutWrap::WrapReverse => "wrap-reverse".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum LayoutWrapParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutWrapParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { LayoutWrapParseError<'a>, {
    InvalidValue(e) => format!("Invalid flex-wrap value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutWrapParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> LayoutWrapParseError<'a> {
    pub fn to_contained(&self) -> LayoutWrapParseErrorOwned {
        match self {
            LayoutWrapParseError::InvalidValue(s) => {
                LayoutWrapParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutWrapParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutWrapParseError<'a> {
        match self {
            LayoutWrapParseErrorOwned::InvalidValue(s) => {
                LayoutWrapParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_wrap<'a>(input: &'a str) -> Result<LayoutWrap, LayoutWrapParseError<'a>> {
    let input = input.trim();
    match input {
        "nowrap" => Ok(LayoutWrap::NoWrap),
        "wrap" => Ok(LayoutWrap::Wrap),
        "wrap-reverse" => Ok(LayoutWrap::WrapReverse),
        _ => Err(LayoutWrapParseError::InvalidValue(input)),
    }
}

// --- writing-mode (LayoutWritingMode) ---

/// Represents a `writing-mode` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutWritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

impl Default for LayoutWritingMode {
    fn default() -> Self {
        LayoutWritingMode::HorizontalTb
    }
}

impl LayoutWritingMode {
    /// Returns true if the writing mode is vertical (VerticalRl or VerticalLr)
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::VerticalRl | Self::VerticalLr)
    }
}

impl core::fmt::Debug for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutWritingMode {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutWritingMode::HorizontalTb => "horizontal-tb".to_string(),
            LayoutWritingMode::VerticalRl => "vertical-rl".to_string(),
            LayoutWritingMode::VerticalLr => "vertical-lr".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum LayoutWritingModeParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutWritingModeParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { LayoutWritingModeParseError<'a>, {
    InvalidValue(e) => format!("Invalid writing-mode value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutWritingModeParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> LayoutWritingModeParseError<'a> {
    pub fn to_contained(&self) -> LayoutWritingModeParseErrorOwned {
        match self {
            LayoutWritingModeParseError::InvalidValue(s) => {
                LayoutWritingModeParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutWritingModeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutWritingModeParseError<'a> {
        match self {
            LayoutWritingModeParseErrorOwned::InvalidValue(s) => {
                LayoutWritingModeParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_writing_mode<'a>(
    input: &'a str,
) -> Result<LayoutWritingMode, LayoutWritingModeParseError<'a>> {
    let input = input.trim();
    match input {
        "horizontal-tb" => Ok(LayoutWritingMode::HorizontalTb),
        "vertical-rl" => Ok(LayoutWritingMode::VerticalRl),
        "vertical-lr" => Ok(LayoutWritingMode::VerticalLr),
        _ => Err(LayoutWritingModeParseError::InvalidValue(input)),
    }
}

// --- clear (LayoutClear) ---

/// Represents a `clear` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutClear {
    None,
    Left,
    Right,
    Both,
}

impl Default for LayoutClear {
    fn default() -> Self {
        LayoutClear::None
    }
}

impl core::fmt::Debug for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutClear {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutClear::None => "none".to_string(),
            LayoutClear::Left => "left".to_string(),
            LayoutClear::Right => "right".to_string(),
            LayoutClear::Both => "both".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum LayoutClearParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutClearParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { LayoutClearParseError<'a>, {
    InvalidValue(e) => format!("Invalid clear value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutClearParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> LayoutClearParseError<'a> {
    pub fn to_contained(&self) -> LayoutClearParseErrorOwned {
        match self {
            LayoutClearParseError::InvalidValue(s) => {
                LayoutClearParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutClearParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutClearParseError<'a> {
        match self {
            LayoutClearParseErrorOwned::InvalidValue(s) => {
                LayoutClearParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_clear<'a>(input: &'a str) -> Result<LayoutClear, LayoutClearParseError<'a>> {
    let input = input.trim();
    match input {
        "none" => Ok(LayoutClear::None),
        "left" => Ok(LayoutClear::Left),
        "right" => Ok(LayoutClear::Right),
        "both" => Ok(LayoutClear::Both),
        _ => Err(LayoutClearParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    // LayoutWrap tests
    #[test]
    fn test_parse_layout_wrap_nowrap() {
        assert_eq!(parse_layout_wrap("nowrap").unwrap(), LayoutWrap::NoWrap);
    }

    #[test]
    fn test_parse_layout_wrap_wrap() {
        assert_eq!(parse_layout_wrap("wrap").unwrap(), LayoutWrap::Wrap);
    }

    #[test]
    fn test_parse_layout_wrap_wrap_reverse() {
        assert_eq!(
            parse_layout_wrap("wrap-reverse").unwrap(),
            LayoutWrap::WrapReverse
        );
    }

    #[test]
    fn test_parse_layout_wrap_invalid() {
        assert!(parse_layout_wrap("invalid").is_err());
    }

    #[test]
    fn test_parse_layout_wrap_whitespace() {
        assert_eq!(parse_layout_wrap("  wrap  ").unwrap(), LayoutWrap::Wrap);
    }

    #[test]
    fn test_parse_layout_wrap_case_sensitive() {
        assert!(parse_layout_wrap("Wrap").is_err());
        assert!(parse_layout_wrap("WRAP").is_err());
    }

    // LayoutWritingMode tests
    #[test]
    fn test_parse_writing_mode_horizontal_tb() {
        assert_eq!(
            parse_layout_writing_mode("horizontal-tb").unwrap(),
            LayoutWritingMode::HorizontalTb
        );
    }

    #[test]
    fn test_parse_writing_mode_vertical_rl() {
        assert_eq!(
            parse_layout_writing_mode("vertical-rl").unwrap(),
            LayoutWritingMode::VerticalRl
        );
    }

    #[test]
    fn test_parse_writing_mode_vertical_lr() {
        assert_eq!(
            parse_layout_writing_mode("vertical-lr").unwrap(),
            LayoutWritingMode::VerticalLr
        );
    }

    #[test]
    fn test_parse_writing_mode_invalid() {
        assert!(parse_layout_writing_mode("invalid").is_err());
        assert!(parse_layout_writing_mode("horizontal").is_err());
    }

    #[test]
    fn test_parse_writing_mode_whitespace() {
        assert_eq!(
            parse_layout_writing_mode("  vertical-rl  ").unwrap(),
            LayoutWritingMode::VerticalRl
        );
    }

    // LayoutClear tests
    #[test]
    fn test_parse_layout_clear_none() {
        assert_eq!(parse_layout_clear("none").unwrap(), LayoutClear::None);
    }

    #[test]
    fn test_parse_layout_clear_left() {
        assert_eq!(parse_layout_clear("left").unwrap(), LayoutClear::Left);
    }

    #[test]
    fn test_parse_layout_clear_right() {
        assert_eq!(parse_layout_clear("right").unwrap(), LayoutClear::Right);
    }

    #[test]
    fn test_parse_layout_clear_both() {
        assert_eq!(parse_layout_clear("both").unwrap(), LayoutClear::Both);
    }

    #[test]
    fn test_parse_layout_clear_invalid() {
        assert!(parse_layout_clear("invalid").is_err());
        assert!(parse_layout_clear("all").is_err());
    }

    #[test]
    fn test_parse_layout_clear_whitespace() {
        assert_eq!(parse_layout_clear("  both  ").unwrap(), LayoutClear::Both);
    }

    // Print tests
    #[test]
    fn test_print_layout_wrap() {
        assert_eq!(LayoutWrap::NoWrap.print_as_css_value(), "nowrap");
        assert_eq!(LayoutWrap::Wrap.print_as_css_value(), "wrap");
        assert_eq!(LayoutWrap::WrapReverse.print_as_css_value(), "wrap-reverse");
    }

    #[test]
    fn test_print_writing_mode() {
        assert_eq!(
            LayoutWritingMode::HorizontalTb.print_as_css_value(),
            "horizontal-tb"
        );
        assert_eq!(
            LayoutWritingMode::VerticalRl.print_as_css_value(),
            "vertical-rl"
        );
        assert_eq!(
            LayoutWritingMode::VerticalLr.print_as_css_value(),
            "vertical-lr"
        );
    }

    #[test]
    fn test_print_layout_clear() {
        assert_eq!(LayoutClear::None.print_as_css_value(), "none");
        assert_eq!(LayoutClear::Left.print_as_css_value(), "left");
        assert_eq!(LayoutClear::Right.print_as_css_value(), "right");
        assert_eq!(LayoutClear::Both.print_as_css_value(), "both");
    }
}
