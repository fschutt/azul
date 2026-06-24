//! CSS properties for positioning elements: `position`, `top`, `right`,
//! `bottom`, `left`, and `z-index`. Types defined here are consumed by the
//! layout solver to resolve positioned elements.

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
#[derive(Default)]
pub enum LayoutPosition {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl LayoutPosition {
    #[must_use] pub fn is_positioned(&self) -> bool {
        *self != Self::Static
    }
}


impl PrintAsCssValue for LayoutPosition {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Fixed => "fixed",
            Self::Sticky => "sticky",
        })
    }
}

impl_enum_fmt!(LayoutPosition, Static, Fixed, Absolute, Relative, Sticky);

// -- Parser for LayoutPosition

#[derive(Clone, PartialEq, Eq)]
pub enum LayoutPositionParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(LayoutPositionParseError<'a>);
impl_display! { LayoutPositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid position value: \"{}\"", val),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutPositionParseErrorOwned {
    InvalidValue(AzString),
}

impl LayoutPositionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutPositionParseErrorOwned {
        match self {
            LayoutPositionParseError::InvalidValue(s) => {
                LayoutPositionParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl LayoutPositionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutPositionParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutPositionParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `position` value.
pub fn parse_layout_position(
    input: &str,
) -> Result<LayoutPosition, LayoutPositionParseError<'_>> {
    let input = input.trim();
    match input {
        "static" => Ok(LayoutPosition::Static),
        "relative" => Ok(LayoutPosition::Relative),
        "absolute" => Ok(LayoutPosition::Absolute),
        "fixed" => Ok(LayoutPosition::Fixed),
        "sticky" => Ok(LayoutPosition::Sticky),
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
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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

/// Represents the CSS `top` offset property for positioned elements.
define_position_property!(LayoutTop);
/// Represents the CSS `right` offset property for positioned elements.
define_position_property!(LayoutRight);
/// Represents the CSS `bottom` offset property for positioned elements.
define_position_property!(LayoutInsetBottom);
/// Represents the CSS `left` offset property for positioned elements.
define_position_property!(LayoutLeft);

// -- Parse error types and parsers for offset properties (top, right, bottom, left)

macro_rules! define_offset_parse_error {
    ($struct_name:ident, $error_name:ident, $error_owned_name:ident, $parse_fn:ident) => {
        #[derive(Clone, PartialEq, Eq)]
        pub enum $error_name<'a> {
            PixelValue(CssPixelValueParseError<'a>),
        }
        impl_debug_as_display!($error_name<'a>);
        impl_display! { $error_name<'a>, { PixelValue(e) => format!("{}", e), }}
        impl_from!(CssPixelValueParseError<'a>, $error_name::PixelValue);

        #[derive(Debug, Clone, PartialEq, Eq)]
        #[repr(C, u8)]
        pub enum $error_owned_name {
            PixelValue(CssPixelValueParseErrorOwned),
        }
        impl $error_name<'_> {
            #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                match self {
                    $error_name::PixelValue(e) => {
                        $error_owned_name::PixelValue(e.to_contained())
                    }
                }
            }
        }
        impl $error_owned_name {
            #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                match self {
                    $error_owned_name::PixelValue(e) => {
                        $error_name::PixelValue(e.to_shared())
                    }
                }
            }
        }

        #[cfg(feature = "parser")]
        pub fn $parse_fn(input: &str) -> Result<$struct_name, $error_name<'_>> {
            parse_pixel_value(input)
                .map(|v| $struct_name { inner: v })
                .map_err(Into::into)
        }
    };
}

define_offset_parse_error!(LayoutTop, LayoutTopParseError, LayoutTopParseErrorOwned, parse_layout_top);
define_offset_parse_error!(LayoutRight, LayoutRightParseError, LayoutRightParseErrorOwned, parse_layout_right);
define_offset_parse_error!(LayoutInsetBottom, LayoutInsetBottomParseError, LayoutInsetBottomParseErrorOwned, parse_layout_bottom);
define_offset_parse_error!(LayoutLeft, LayoutLeftParseError, LayoutLeftParseErrorOwned, parse_layout_left);

// --- LayoutZIndex ---

/// Represents a `z-index` attribute - controls stacking order of positioned elements
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum LayoutZIndex {
    #[default]
    Auto,
    Integer(i32),
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for LayoutZIndex {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("LayoutZIndex::Auto"),
            Self::Integer(val) => {
                format!("LayoutZIndex::Integer({val})")
            }
        }
    }
}


impl PrintAsCssValue for LayoutZIndex {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => String::from("auto"),
            Self::Integer(val) => val.to_string(),
        }
    }
}

// -- Parser for LayoutZIndex

#[derive(Clone, PartialEq, Eq)]
pub enum LayoutZIndexParseError<'a> {
    InvalidValue(&'a str),
    ParseInt(::core::num::ParseIntError, &'a str),
}
impl_debug_as_display!(LayoutZIndexParseError<'a>);
impl_display! { LayoutZIndexParseError<'a>, {
    InvalidValue(val) => format!("Invalid z-index value: \"{}\"", val),
    ParseInt(e, s) => format!("Invalid z-index integer \"{}\": {}", s, e),
}}

/// Wrapper for `ParseIntError` that stores the error message and original
/// input as owned strings for FFI compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ParseIntErrorWithInput {
    /// The stringified parse error (e.g. "invalid digit found in string").
    pub error: AzString,
    /// The original input string that failed to parse.
    pub input: AzString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutZIndexParseErrorOwned {
    InvalidValue(AzString),
    ParseInt(ParseIntErrorWithInput),
}

impl LayoutZIndexParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutZIndexParseErrorOwned {
        match self {
            LayoutZIndexParseError::InvalidValue(s) => {
                LayoutZIndexParseErrorOwned::InvalidValue((*s).to_string().into())
            }
            LayoutZIndexParseError::ParseInt(e, s) => {
                LayoutZIndexParseErrorOwned::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: (*s).to_string().into() })
            }
        }
    }
}

impl LayoutZIndexParseErrorOwned {
    /// Converts back to the borrowed error type.
    ///
    /// **Note:** This conversion is lossy for `ParseInt` — the original
    /// `core::num::ParseIntError` cannot be reconstructed from its string
    /// representation, so `ParseInt` is mapped to `InvalidValue` instead.
    #[must_use] pub fn to_shared(&self) -> LayoutZIndexParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutZIndexParseError::InvalidValue(s.as_str())
            }
            Self::ParseInt(e) => {
                // We can't reconstruct ParseIntError, so use InvalidValue
                LayoutZIndexParseError::InvalidValue(e.input.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `z-index` value.
pub fn parse_layout_z_index(
    input: &str,
) -> Result<LayoutZIndex, LayoutZIndexParseError<'_>> {
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
        assert_eq!(
            parse_layout_position("sticky").unwrap(),
            LayoutPosition::Sticky
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
