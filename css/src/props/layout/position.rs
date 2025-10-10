//! CSS properties for positioning elements.

use alloc::string::{String, ToString};

#[cfg(feature = "parser")]
use crate::props::basic::value::parse_pixel_value;
use crate::{
    parser::{impl_debug_as_display, impl_display, impl_from},
    props::{
        basic::value::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
        formatter::PrintAsCssValue,
        macros::{impl_pixel_value, PixelValueTaker},
    },
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
        })
    }
}

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
pub enum LayoutPositionParseErrorOwned {
    InvalidValue(String),
}

impl<'a> LayoutPositionParseError<'a> {
    pub fn to_contained(&self) -> LayoutPositionParseErrorOwned {
        match self {
            LayoutPositionParseError::InvalidValue(s) => {
                LayoutPositionParseErrorOwned::InvalidValue(s.to_string())
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
define_position_property!(LayoutBottom);
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

// -- Parser for LayoutBottom

#[derive(Clone, PartialEq)]
pub enum LayoutBottomParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(LayoutBottomParseError<'a>);
impl_display! { LayoutBottomParseError<'a>, { PixelValue(e) => format!("{}", e), }}
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutBottomParseError::PixelValue
);

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutBottomParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> LayoutBottomParseError<'a> {
    pub fn to_contained(&self) -> LayoutBottomParseErrorOwned {
        match self {
            LayoutBottomParseError::PixelValue(e) => {
                LayoutBottomParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}
impl LayoutBottomParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutBottomParseError<'a> {
        match self {
            LayoutBottomParseErrorOwned::PixelValue(e) => {
                LayoutBottomParseError::PixelValue(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_bottom<'a>(input: &'a str) -> Result<LayoutBottom, LayoutBottomParseError<'a>> {
    parse_pixel_value(input)
        .map(|v| LayoutBottom { inner: v })
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
