//! CSS `text-justify` property.
//!
//! Defines [`LayoutTextJustify`] and its parser [`parse_layout_text_justify`],
//! used by the CSS property parsing pipeline.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{codegen::format::FormatAsRustCode, props::formatter::PrintAsCssValue};

/// CSS `text-justify` property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutTextJustify {
    #[default]
    Auto,
    None,
    InterWord,
    InterCharacter,
    /// Legacy value; the parser maps `"distribute"` to `InterCharacter` per spec.
    /// Retained for `#[repr(C)]` FFI backward compatibility.
    Distribute,
}


impl PrintAsCssValue for LayoutTextJustify {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::InterWord => "inter-word",
            Self::InterCharacter => "inter-character",
            Self::Distribute => "distribute",
        }
        .to_string()
    }
}

impl FormatAsRustCode for LayoutTextJustify {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("LayoutTextJustify::{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextJustifyParseError<'a> {
    InvalidValue(&'a str),
}

impl fmt::Display for TextJustifyParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                write!(f, "Invalid text-justify value: '{s}'.")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum TextJustifyParseErrorOwned {
    InvalidValue(AzString),
}

impl TextJustifyParseError<'_> {
    #[must_use] pub fn to_owned(&self) -> TextJustifyParseErrorOwned {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                TextJustifyParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl TextJustifyParseErrorOwned {
    #[must_use] pub fn to_borrowed(&self) -> TextJustifyParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                TextJustifyParseError::InvalidValue(s.as_str())
            }
        }
    }
}

/// Parses a `text-justify` CSS value string into a [`LayoutTextJustify`].
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-justify` value.
pub fn parse_layout_text_justify(
    input: &str,
) -> Result<LayoutTextJustify, TextJustifyParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutTextJustify::Auto),
        "none" => Ok(LayoutTextJustify::None),
        "inter-word" => Ok(LayoutTextJustify::InterWord),
        // "distribute" is a legacy alias that computes to inter-character:
        // +spec:text-alignment-spacing:4a88c2  +spec:text-alignment-spacing:58c33f
        "inter-character" | "distribute" => Ok(LayoutTextJustify::InterCharacter),
        other => Err(TextJustifyParseError::InvalidValue(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_layout_text_justify() {
        assert_eq!(
            parse_layout_text_justify("auto"),
            Ok(LayoutTextJustify::Auto)
        );
        assert_eq!(
            parse_layout_text_justify("none"),
            Ok(LayoutTextJustify::None)
        );
        assert_eq!(
            parse_layout_text_justify("inter-word"),
            Ok(LayoutTextJustify::InterWord)
        );
        assert_eq!(
            parse_layout_text_justify("inter-character"),
            Ok(LayoutTextJustify::InterCharacter)
        );
        assert_eq!(
            parse_layout_text_justify("distribute"),
            Ok(LayoutTextJustify::InterCharacter)
        );
        assert!(parse_layout_text_justify("invalid").is_err());
    }
}
