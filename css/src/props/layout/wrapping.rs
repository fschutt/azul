//! CSS properties for writing modes and clearing.
//!
//! Key types:
//! - [`LayoutWritingMode`] — `writing-mode` (`horizontal-tb`, `vertical-rl`, `vertical-lr`)
//! - [`LayoutClear`] — `clear` (`none`, `left`, `right`, `both`)
//!
//! Parse functions are gated behind the `parser` feature and are consumed
//! by the CSS property system in `property.rs`.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

// --- writing-mode (LayoutWritingMode) ---

// +spec:writing-modes:ec496c - writing-mode property: horizontal-tb, vertical-rl, vertical-lr block flow directions
// +spec:writing-modes:fdc4cc - writing-mode property: horizontal-tb | vertical-rl | vertical-lr
// +spec:writing-modes:aeb9bb - writing-mode property determines block flow direction
/// Represents a `writing-mode` attribute
// +spec:writing-modes:a7f174 - line orientation: in vertical-lr the line-over (ascender) side is block-end, not block-start
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
// +spec:block-formatting-context:387117 - writing-mode specifies horizontal/vertical line layout and block progression direction
// +spec:block-formatting-context:3815e7 - vertical-rl writing mode supported via VerticalRl variant
// +spec:block-formatting-context:9d7cd4 - vertical writing mode support (VerticalRl, VerticalLr)
#[derive(Default)]
pub enum LayoutWritingMode {
    /// Top-to-bottom block flow, left-to-right inline direction (Latin, etc.).
    #[default]
    HorizontalTb,
    /// Right-to-left block flow, top-to-bottom inline direction (CJK vertical).
    VerticalRl,
    // +spec:writing-modes:f35728 - vertical-lr writing mode for left-to-right block flow (Manchu, Mongolian)
    /// Left-to-right block flow, top-to-bottom inline direction (Mongolian).
    VerticalLr,
}


impl LayoutWritingMode {
    /// Returns true if the writing mode is vertical (`VerticalRl` or `VerticalLr`)
    #[must_use] pub const fn is_vertical(self) -> bool {
        matches!(self, Self::VerticalRl | Self::VerticalLr)
    }
}

impl core::fmt::Debug for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutWritingMode {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::HorizontalTb => "horizontal-tb".to_string(),
            Self::VerticalRl => "vertical-rl".to_string(),
            Self::VerticalLr => "vertical-lr".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutWritingModeParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutWritingModeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutWritingModeParseErrorOwned {
        match self {
            LayoutWritingModeParseError::InvalidValue(s) => {
                LayoutWritingModeParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutWritingModeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutWritingModeParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutWritingModeParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `writing-mode` value.
pub fn parse_layout_writing_mode(
    input: &str,
) -> Result<LayoutWritingMode, LayoutWritingModeParseError<'_>> {
    let input = input.trim();
    match input {
        "horizontal-tb" => Ok(LayoutWritingMode::HorizontalTb),
        "vertical-rl" => Ok(LayoutWritingMode::VerticalRl),
        // +spec:writing-modes:23147f - SVG1.1 tb-lr maps to vertical-lr
        "vertical-lr" | "tb-lr" => Ok(LayoutWritingMode::VerticalLr),
        _ => Err(LayoutWritingModeParseError::InvalidValue(input)),
    }
}

// --- clear (LayoutClear) ---

/// Represents a `clear` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutClear {
    /// No clearing; element is not moved below preceding floats.
    #[default]
    None,
    /// Element is moved below preceding left floats.
    Left,
    /// Element is moved below preceding right floats.
    Right,
    /// Element is moved below all preceding floats.
    Both,
}


impl core::fmt::Debug for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutClear {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
            Self::Both => "both".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutClearParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutClearParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutClearParseErrorOwned {
        match self {
            LayoutClearParseError::InvalidValue(s) => {
                LayoutClearParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutClearParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutClearParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutClearParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `clear` value.
pub fn parse_layout_clear(input: &str) -> Result<LayoutClear, LayoutClearParseError<'_>> {
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
