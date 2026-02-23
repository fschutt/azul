//! CSS properties for positioning elements.

use alloc::string::{String, ToString};
use crate::corety::AzString;

#[cfg(feature = "parser")]
use crate::props::basic::pixel::parse_pixel_value;
use crate::props::{
    basic::pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    formatter::PrintAsCssValue,
    macros::PixelValueTaker,
};

// --- LayoutPosition ---

/// Represents a `position` attribute - default: `Static`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl LayoutPosition {
    pub fn is_positioned(&self) -> bool {
        *self != LayoutPosition::Static
    }
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

impl PrintAsCssValue for LayoutPosition {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutPosition::Static => "static",
            LayoutPosition::Relative => "relative",
            LayoutPosition::Absolute => "absolute",
            LayoutPosition::Fixed => "fixed",
            LayoutPosition::Sticky => "sticky",
        })
    }
}

impl_enum_fmt!(LayoutPosition, Static, Fixed, Absolute, Relative, Sticky);

// -- Parser for LayoutPosition

#[derive(Clone, PartialEq)]
pub enum LayoutPositionParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(LayoutPositionParseError<'a>);
impl_display! { LayoutPositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid position value: \"{}\"", val),
}}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutPositionParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> LayoutPositionParseError<'a> {
    pub fn to_contained(&self) -> LayoutPositionParseErrorOwned {
        match self {
            LayoutPositionParseError::InvalidValue(s) => {
                LayoutPositionParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

impl LayoutPositionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutPositionParseError<'a> {
        match self {
            LayoutPositionParseErrorOwned::InvalidValue(s) => {
                LayoutPositionParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_position<'a>(
    input: &'a str,
) -> Result<LayoutPosition, LayoutPositionParseError<'a>> {
    let input = input.trim();
    match input {
        "static" => Ok(LayoutPosition::Static),
        "relative" => Ok(LayoutPosition::Relative),
        "absolute" => Ok(LayoutPosition::Absolute),
        "fixed" => Ok(LayoutPosition::Fixed),
        _ => Err(LayoutPositionParseError::InvalidValue(input)),
    }
}

// --- Offset Properties (top, right, bottom, left) ---

macro_rules! define_position_property {
    ($struct_name:ident) => {
        #[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }

        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);

        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                format!("{}", self.inner)
            }
        }
    };
}

define_position_property!(LayoutTop);
define_position_property!(LayoutRight);
define_position_property!(LayoutInsetBottom);
define_position_property!(LayoutLeft);

// -- Parser for LayoutTop

#[derive(Clone, PartialEq)]
pub enum LayoutTopParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(LayoutTopParseError<'a>);
impl_display! { LayoutTopParseError<'a>, { PixelValue(e) => format!("{}", e), }}
impl_from!(CssPixelValueParseError<'a>, LayoutTopParseError::PixelValue);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutTopParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> LayoutTopParseError<'a> {
    pub fn to_contained(&self) -> LayoutTopParseErrorOwned {
        match self {
            LayoutTopParseError::PixelValue(e) => {
                LayoutTopParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}
impl LayoutTopParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutTopParseError<'a> {
        match self {
            LayoutTopParseErrorOwned::PixelValue(e) => {
                LayoutTopParseError::PixelValue(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_top<'a>(input: &'a str) -> Result<LayoutTop, LayoutTopParseError<'a>> {
    parse_pixel_value(input)
        .map(|v| LayoutTop { inner: v })
        .map_err(Into::into)
}

// -- Parser for LayoutRight

#[derive(Clone, PartialEq)]
pub enum LayoutRightParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(LayoutRightParseError<'a>);
impl_display! { LayoutRightParseError<'a>, { PixelValue(e) => format!("{}", e), }}
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutRightParseError::PixelValue
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutRightParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> LayoutRightParseError<'a> {
    pub fn to_contained(&self) -> LayoutRightParseErrorOwned {
        match self {
            LayoutRightParseError::PixelValue(e) => {
                LayoutRightParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}
impl LayoutRightParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutRightParseError<'a> {
        match self {
            LayoutRightParseErrorOwned::PixelValue(e) => {
                LayoutRightParseError::PixelValue(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_right<'a>(input: &'a str) -> Result<LayoutRight, LayoutRightParseError<'a>> {
    parse_pixel_value(input)
        .map(|v| LayoutRight { inner: v })
        .map_err(Into::into)
}

// -- Parser for LayoutInsetBottom

#[derive(Clone, PartialEq)]
pub enum LayoutInsetBottomParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(LayoutInsetBottomParseError<'a>);
impl_display! { LayoutInsetBottomParseError<'a>, { PixelValue(e) => format!("{}", e), }}
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutInsetBottomParseError::PixelValue
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutInsetBottomParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> LayoutInsetBottomParseError<'a> {
    pub fn to_contained(&self) -> LayoutInsetBottomParseErrorOwned {
        match self {
            LayoutInsetBottomParseError::PixelValue(e) => {
                LayoutInsetBottomParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}
impl LayoutInsetBottomParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutInsetBottomParseError<'a> {
        match self {
            LayoutInsetBottomParseErrorOwned::PixelValue(e) => {
                LayoutInsetBottomParseError::PixelValue(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_bottom<'a>(
    input: &'a str,
) -> Result<LayoutInsetBottom, LayoutInsetBottomParseError<'a>> {
    parse_pixel_value(input)
        .map(|v| LayoutInsetBottom { inner: v })
        .map_err(Into::into)
}

// -- Parser for LayoutLeft

#[derive(Clone, PartialEq)]
pub enum LayoutLeftParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(LayoutLeftParseError<'a>);
impl_display! { LayoutLeftParseError<'a>, { PixelValue(e) => format!("{}", e), }}
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutLeftParseError::PixelValue
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutLeftParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> LayoutLeftParseError<'a> {
    pub fn to_contained(&self) -> LayoutLeftParseErrorOwned {
        match self {
            LayoutLeftParseError::PixelValue(e) => {
                LayoutLeftParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}
impl LayoutLeftParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutLeftParseError<'a> {
        match self {
            LayoutLeftParseErrorOwned::PixelValue(e) => {
                LayoutLeftParseError::PixelValue(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_left<'a>(input: &'a str) -> Result<LayoutLeft, LayoutLeftParseError<'a>> {
    parse_pixel_value(input)
        .map(|v| LayoutLeft { inner: v })
        .map_err(Into::into)
}

// --- LayoutZIndex ---

/// Represents a `z-index` attribute - controls stacking order of positioned elements
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum LayoutZIndex {
    Auto,
    Integer(i32),
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for LayoutZIndex {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            LayoutZIndex::Auto => String::from("LayoutZIndex::Auto"),
            LayoutZIndex::Integer(val) => {
                format!("LayoutZIndex::Integer({})", val)
            }
        }
    }
}

impl Default for LayoutZIndex {
    fn default() -> Self {
        LayoutZIndex::Auto
    }
}

impl PrintAsCssValue for LayoutZIndex {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutZIndex::Auto => String::from("auto"),
            LayoutZIndex::Integer(val) => val.to_string(),
        }
    }
}

// -- Parser for LayoutZIndex

#[derive(Clone, PartialEq)]
pub enum LayoutZIndexParseError<'a> {
    InvalidValue(&'a str),
    ParseInt(::core::num::ParseIntError, &'a str),
}
impl_debug_as_display!(LayoutZIndexParseError<'a>);
impl_display! { LayoutZIndexParseError<'a>, {
    InvalidValue(val) => format!("Invalid z-index value: \"{}\"", val),
    ParseInt(e, s) => format!("Invalid z-index integer \"{}\": {}", s, e),
}}

/// Wrapper for ParseInt error with input string.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ParseIntErrorWithInput {
    pub error: String,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutZIndexParseErrorOwned {
    InvalidValue(AzString),
    ParseInt(ParseIntErrorWithInput),
}

impl<'a> LayoutZIndexParseError<'a> {
    pub fn to_contained(&self) -> LayoutZIndexParseErrorOwned {
        match self {
            LayoutZIndexParseError::InvalidValue(s) => {
                LayoutZIndexParseErrorOwned::InvalidValue(s.to_string().into())
            }
            LayoutZIndexParseError::ParseInt(e, s) => {
                LayoutZIndexParseErrorOwned::ParseInt(ParseIntErrorWithInput { error: e.to_string(), input: s.to_string() })
            }
        }
    }
}

impl LayoutZIndexParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutZIndexParseError<'a> {
        match self {
            LayoutZIndexParseErrorOwned::InvalidValue(s) => {
                LayoutZIndexParseError::InvalidValue(s.as_str())
            }
            LayoutZIndexParseErrorOwned::ParseInt(e) => {
                // We can't reconstruct ParseIntError, so use InvalidValue
                LayoutZIndexParseError::InvalidValue(e.input.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_z_index<'a>(
    input: &'a str,
) -> Result<LayoutZIndex, LayoutZIndexParseError<'a>> {
    let input = input.trim();
    if input == "auto" {
        return Ok(LayoutZIndex::Auto);
    }

    match input.parse::<i32>() {
        Ok(val) => Ok(LayoutZIndex::Integer(val)),
        Err(e) => Err(LayoutZIndexParseError::ParseInt(e, input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_position() {
        assert_eq!(
            parse_layout_position("static").unwrap(),
            LayoutPosition::Static
        );
        assert_eq!(
            parse_layout_position("relative").unwrap(),
            LayoutPosition::Relative
        );
        assert_eq!(
            parse_layout_position("absolute").unwrap(),
            LayoutPosition::Absolute
        );
        assert_eq!(
            parse_layout_position("fixed").unwrap(),
            LayoutPosition::Fixed
        );
    }

    #[test]
    fn test_parse_layout_position_whitespace() {
        assert_eq!(
            parse_layout_position("  absolute  ").unwrap(),
            LayoutPosition::Absolute
        );
    }

    #[test]
    fn test_parse_layout_position_invalid() {
        assert!(parse_layout_position("sticky").is_err());
        assert!(parse_layout_position("").is_err());
        assert!(parse_layout_position("absolutely").is_err());
    }

    #[test]
    fn test_parse_layout_z_index() {
        assert_eq!(parse_layout_z_index("auto").unwrap(), LayoutZIndex::Auto);
        assert_eq!(
            parse_layout_z_index("10").unwrap(),
            LayoutZIndex::Integer(10)
        );
        assert_eq!(parse_layout_z_index("0").unwrap(), LayoutZIndex::Integer(0));
        assert_eq!(
            parse_layout_z_index("-5").unwrap(),
            LayoutZIndex::Integer(-5)
        );
        assert_eq!(
            parse_layout_z_index("  999  ").unwrap(),
            LayoutZIndex::Integer(999)
        );
    }

    #[test]
    fn test_parse_layout_z_index_invalid() {
        assert!(parse_layout_z_index("10px").is_err());
        assert!(parse_layout_z_index("1.5").is_err());
        assert!(parse_layout_z_index("none").is_err());
        assert!(parse_layout_z_index("").is_err());
    }

    #[test]
    fn test_parse_offsets() {
        assert_eq!(
            parse_layout_top("10px").unwrap(),
            LayoutTop {
                inner: PixelValue::px(10.0)
            }
        );
        assert_eq!(
            parse_layout_right("5%").unwrap(),
            LayoutRight {
                inner: PixelValue::percent(5.0)
            }
        );
        assert_eq!(
            parse_layout_bottom("2.5em").unwrap(),
            LayoutInsetBottom {
                inner: PixelValue::em(2.5)
            }
        );
        assert_eq!(
            parse_layout_left("0").unwrap(),
            LayoutLeft {
                inner: PixelValue::px(0.0)
            }
        );
    }

    #[test]
    fn test_parse_offsets_invalid() {
        // The simple `parse_pixel_value` does not handle `auto`.
        assert!(parse_layout_top("auto").is_err());
        assert!(parse_layout_right("").is_err());
        // Liberal parsing accepts whitespace between number and unit
        assert!(parse_layout_bottom("10 px").is_ok());
        assert!(parse_layout_left("ten pixels").is_err());
    }
}
