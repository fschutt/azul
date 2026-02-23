//! css/src/props/style/lists.rs
//!
//! CSS properties related to list styling.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{format_rust_code::FormatAsRustCode, props::formatter::PrintAsCssValue};

// --- list-style-type ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerGreek,
    UpperGreek,
    LowerAlpha,
    UpperAlpha,
}

impl Default for StyleListStyleType {
    fn default() -> Self {
        Self::Disc // Default for <ul>
    }
}

impl PrintAsCssValue for StyleListStyleType {
    fn print_as_css_value(&self) -> String {
        use StyleListStyleType::*;
        String::from(match self {
            None => "none",
            Disc => "disc",
            Circle => "circle",
            Square => "square",
            Decimal => "decimal",
            DecimalLeadingZero => "decimal-leading-zero",
            LowerRoman => "lower-roman",
            UpperRoman => "upper-roman",
            LowerGreek => "lower-greek",
            UpperGreek => "upper-greek",
            LowerAlpha => "lower-alpha",
            UpperAlpha => "upper-alpha",
        })
    }
}

impl FormatAsRustCode for StyleListStyleType {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleListStyleType::*;
        format!(
            "StyleListStyleType::{}",
            match self {
                None => "None",
                Disc => "Disc",
                Circle => "Circle",
                Square => "Square",
                Decimal => "Decimal",
                DecimalLeadingZero => "DecimalLeadingZero",
                LowerRoman => "LowerRoman",
                UpperRoman => "UpperRoman",
                LowerGreek => "LowerGreek",
                UpperGreek => "UpperGreek",
                LowerAlpha => "LowerAlpha",
                UpperAlpha => "UpperAlpha",
            }
        )
    }
}

impl fmt::Display for StyleListStyleType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

// --- list-style-position ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleListStylePosition {
    Inside,
    Outside,
}

impl Default for StyleListStylePosition {
    fn default() -> Self {
        Self::Outside
    }
}

impl PrintAsCssValue for StyleListStylePosition {
    fn print_as_css_value(&self) -> String {
        use StyleListStylePosition::*;
        String::from(match self {
            Inside => "inside",
            Outside => "outside",
        })
    }
}

impl FormatAsRustCode for StyleListStylePosition {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleListStylePosition::*;
        format!(
            "StyleListStylePosition::{}",
            match self {
                Inside => "Inside",
                Outside => "Outside",
            }
        )
    }
}

impl fmt::Display for StyleListStylePosition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

// --- Parsing Logic ---

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleListStyleTypeParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleListStyleTypeParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { StyleListStyleTypeParseError<'a>, {
    InvalidValue(val) => format!("Invalid list-style-type value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleListStyleTypeParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleListStyleTypeParseError<'a> {
    pub fn to_contained(&self) -> StyleListStyleTypeParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleListStyleTypeParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleListStyleTypeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleListStyleTypeParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleListStyleTypeParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_list_style_type<'a>(
    input: &'a str,
) -> Result<StyleListStyleType, StyleListStyleTypeParseError<'a>> {
    let input = input.trim();
    match input {
        "none" => Ok(StyleListStyleType::None),
        "disc" => Ok(StyleListStyleType::Disc),
        "circle" => Ok(StyleListStyleType::Circle),
        "square" => Ok(StyleListStyleType::Square),
        "decimal" => Ok(StyleListStyleType::Decimal),
        "decimal-leading-zero" => Ok(StyleListStyleType::DecimalLeadingZero),
        "lower-roman" => Ok(StyleListStyleType::LowerRoman),
        "upper-roman" => Ok(StyleListStyleType::UpperRoman),
        "lower-greek" => Ok(StyleListStyleType::LowerGreek),
        "upper-greek" => Ok(StyleListStyleType::UpperGreek),
        "lower-alpha" | "lower-latin" => Ok(StyleListStyleType::LowerAlpha),
        "upper-alpha" | "upper-latin" => Ok(StyleListStyleType::UpperAlpha),
        _ => Err(StyleListStyleTypeParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleListStylePositionParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleListStylePositionParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { StyleListStylePositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid list-style-position value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleListStylePositionParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleListStylePositionParseError<'a> {
    pub fn to_contained(&self) -> StyleListStylePositionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleListStylePositionParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleListStylePositionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleListStylePositionParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleListStylePositionParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_list_style_position<'a>(
    input: &'a str,
) -> Result<StyleListStylePosition, StyleListStylePositionParseError<'a>> {
    let input = input.trim();
    match input {
        "inside" => Ok(StyleListStylePosition::Inside),
        "outside" => Ok(StyleListStylePosition::Outside),
        _ => Err(StyleListStylePositionParseError::InvalidValue(input)),
    }
}
