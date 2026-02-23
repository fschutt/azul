//! CSS property for text-justify

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{format_rust_code::FormatAsRustCode, props::formatter::PrintAsCssValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutTextJustify {
    Auto,
    None,
    InterWord,
    InterCharacter,
    Distribute,
}

impl Default for LayoutTextJustify {
    fn default() -> Self {
        LayoutTextJustify::Auto
    }
}

impl PrintAsCssValue for LayoutTextJustify {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutTextJustify::Auto => "auto",
            LayoutTextJustify::None => "none",
            LayoutTextJustify::InterWord => "inter-word",
            LayoutTextJustify::InterCharacter => "inter-character",
            LayoutTextJustify::Distribute => "distribute",
        }
        .to_string()
    }
}

impl FormatAsRustCode for LayoutTextJustify {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("LayoutTextJustify::{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextJustifyParseError<'a> {
    InvalidValue(&'a str),
}

impl<'a> fmt::Display for TextJustifyParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                write!(f, "Invalid text-justify value: '{}'.", s)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum TextJustifyParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> TextJustifyParseError<'a> {
    pub fn to_owned(&self) -> TextJustifyParseErrorOwned {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                TextJustifyParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl TextJustifyParseErrorOwned {
    pub fn to_borrowed(&self) -> TextJustifyParseError {
        match self {
            TextJustifyParseErrorOwned::InvalidValue(s) => {
                TextJustifyParseError::InvalidValue(s.as_str())
            }
        }
    }
}

pub fn parse_layout_text_justify<'a>(
    input: &'a str,
) -> Result<LayoutTextJustify, TextJustifyParseError<'a>> {
    match input.trim() {
        "auto" => Ok(LayoutTextJustify::Auto),
        "none" => Ok(LayoutTextJustify::None),
        "inter-word" => Ok(LayoutTextJustify::InterWord),
        "inter-character" => Ok(LayoutTextJustify::InterCharacter),
        "distribute" => Ok(LayoutTextJustify::Distribute),
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
            Ok(LayoutTextJustify::Distribute)
        );
        assert!(parse_layout_text_justify("invalid").is_err());
    }
}
