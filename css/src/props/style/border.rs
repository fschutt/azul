//! CSS properties for border style, width, and color.

use alloc::string::{String, ToString};
use core::fmt;

#[cfg(feature = "parser")]
use crate::props::basic::{color::parse_css_color, pixel::parse_pixel_value};
use crate::props::{
    basic::{
        color::{ColorU, CssColorParseError, CssColorParseErrorOwned},
        pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    },
    formatter::PrintAsCssValue,
    macros::PixelValueTaker,
};

/// Style of a `border`: solid, double, dash, ridge, etc.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub enum BorderStyle {
    None,
    Solid,
    Double,
    Dotted,
    Dashed,
    Hidden,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle::None
    }
}

impl fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::None => "none",
                Self::Solid => "solid",
                Self::Double => "double",
                Self::Dotted => "dotted",
                Self::Dashed => "dashed",
                Self::Hidden => "hidden",
                Self::Groove => "groove",
                Self::Ridge => "ridge",
                Self::Inset => "inset",
                Self::Outset => "outset",
            }
        )
    }
}

impl PrintAsCssValue for BorderStyle {
    fn print_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// Internal macro to reduce boilerplate for defining border-top, -right, -bottom, -left properties.
macro_rules! define_border_side_property {
    // For types that have a simple inner value and can be formatted with Display
    ($struct_name:ident, $inner_type:ty, $default:expr) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: $inner_type,
        }
        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }
        impl Default for $struct_name {
            fn default() -> Self {
                Self { inner: $default }
            }
        }
    };
    // Specialization for ColorU
    ($struct_name:ident,ColorU) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: ColorU,
        }
        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner.to_hash())
            }
        }
        // The default border color is 'currentcolor', but for simplicity we default to BLACK.
        // The style property resolver should handle the 'currentcolor' logic.
        impl Default for $struct_name {
            fn default() -> Self {
                Self {
                    inner: ColorU::BLACK,
                }
            }
        }
        impl $struct_name {
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
    // Specialization for PixelValue (border-width)
    ($struct_name:ident,PixelValue, $default:expr) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }
        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }
        impl Default for $struct_name {
            fn default() -> Self {
                Self { inner: $default }
            }
        }
        impl_pixel_value!($struct_name);
        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }
        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                format!("{}", self.inner)
            }
        }
    };
}

// --- Individual Property Structs ---

// Border Style (border-*-style)
define_border_side_property!(StyleBorderTopStyle, BorderStyle, BorderStyle::None);
define_border_side_property!(StyleBorderRightStyle, BorderStyle, BorderStyle::None);
define_border_side_property!(StyleBorderBottomStyle, BorderStyle, BorderStyle::None);
define_border_side_property!(StyleBorderLeftStyle, BorderStyle, BorderStyle::None);

// Border Color (border-*-color)
define_border_side_property!(StyleBorderTopColor, ColorU);
define_border_side_property!(StyleBorderRightColor, ColorU);
define_border_side_property!(StyleBorderBottomColor, ColorU);
define_border_side_property!(StyleBorderLeftColor, ColorU);

// Border Width (border-*-width)
// The default width is 'medium', which corresponds to 3px.
const MEDIUM_BORDER_THICKNESS: PixelValue = PixelValue::const_px(3);
define_border_side_property!(LayoutBorderTopWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderRightWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderBottomWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderLeftWidth, PixelValue, MEDIUM_BORDER_THICKNESS);

/// Represents the three components of a border shorthand property, used as an intermediate
/// representation during parsing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

// --- PARSERS ---

// -- BorderStyle Parser --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum CssBorderStyleParseError<'a> {
    InvalidStyle(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(CssBorderStyleParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { CssBorderStyleParseError<'a>, {
    InvalidStyle(val) => format!("Invalid border style: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderStyleParseErrorOwned {
    InvalidStyle(String),
}

#[cfg(feature = "parser")]
impl<'a> CssBorderStyleParseError<'a> {
    pub fn to_contained(&self) -> CssBorderStyleParseErrorOwned {
        match self {
            CssBorderStyleParseError::InvalidStyle(s) => {
                CssBorderStyleParseErrorOwned::InvalidStyle(s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl CssBorderStyleParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBorderStyleParseError<'a> {
        match self {
            CssBorderStyleParseErrorOwned::InvalidStyle(s) => {
                CssBorderStyleParseError::InvalidStyle(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_border_style<'a>(input: &'a str) -> Result<BorderStyle, CssBorderStyleParseError<'a>> {
    match input.trim() {
        "none" => Ok(BorderStyle::None),
        "solid" => Ok(BorderStyle::Solid),
        "double" => Ok(BorderStyle::Double),
        "dotted" => Ok(BorderStyle::Dotted),
        "dashed" => Ok(BorderStyle::Dashed),
        "hidden" => Ok(BorderStyle::Hidden),
        "groove" => Ok(BorderStyle::Groove),
        "ridge" => Ok(BorderStyle::Ridge),
        "inset" => Ok(BorderStyle::Inset),
        "outset" => Ok(BorderStyle::Outset),
        _ => Err(CssBorderStyleParseError::InvalidStyle(input)),
    }
}

// -- Shorthand Parser (for `border`, `border-top`, etc.) --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum CssBorderSideParseError<'a> {
    InvalidDeclaration(&'a str),
    Width(CssPixelValueParseError<'a>),
    Style(CssBorderStyleParseError<'a>),
    Color(CssColorParseError<'a>),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(CssBorderSideParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { CssBorderSideParseError<'a>, {
    InvalidDeclaration(e) => format!("Invalid border declaration: \"{}\"", e),
    Width(e) => format!("Invalid border-width component: {}", e),
    Style(e) => format!("Invalid border-style component: {}", e),
    Color(e) => format!("Invalid border-color component: {}", e),
}}

#[cfg(feature = "parser")]
impl_from!(CssPixelValueParseError<'a>, CssBorderSideParseError::Width);
#[cfg(feature = "parser")]
impl_from!(CssBorderStyleParseError<'a>, CssBorderSideParseError::Style);
#[cfg(feature = "parser")]
impl_from!(CssColorParseError<'a>, CssBorderSideParseError::Color);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderSideParseErrorOwned {
    InvalidDeclaration(String),
    Width(CssPixelValueParseErrorOwned),
    Style(CssBorderStyleParseErrorOwned),
    Color(CssColorParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> CssBorderSideParseError<'a> {
    pub fn to_contained(&self) -> CssBorderSideParseErrorOwned {
        match self {
            CssBorderSideParseError::InvalidDeclaration(s) => {
                CssBorderSideParseErrorOwned::InvalidDeclaration(s.to_string())
            }
            CssBorderSideParseError::Width(e) => {
                CssBorderSideParseErrorOwned::Width(e.to_contained())
            }
            CssBorderSideParseError::Style(e) => {
                CssBorderSideParseErrorOwned::Style(e.to_contained())
            }
            CssBorderSideParseError::Color(e) => {
                CssBorderSideParseErrorOwned::Color(e.to_contained())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl CssBorderSideParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBorderSideParseError<'a> {
        match self {
            CssBorderSideParseErrorOwned::InvalidDeclaration(s) => {
                CssBorderSideParseError::InvalidDeclaration(s.as_str())
            }
            CssBorderSideParseErrorOwned::Width(e) => CssBorderSideParseError::Width(e.to_shared()),
            CssBorderSideParseErrorOwned::Style(e) => CssBorderSideParseError::Style(e.to_shared()),
            CssBorderSideParseErrorOwned::Color(e) => CssBorderSideParseError::Color(e.to_shared()),
        }
    }
}

/// Parses a border shorthand property such as "1px solid red".
/// Handles any order of components and applies defaults for missing values.
#[cfg(feature = "parser")]
pub fn parse_border_side<'a>(
    input: &'a str,
) -> Result<StyleBorderSide, CssBorderSideParseError<'a>> {
    let mut width = None;
    let mut style = None;
    let mut color = None;

    if input.trim().is_empty() {
        return Err(CssBorderSideParseError::InvalidDeclaration(input));
    }

    for part in input.split_whitespace() {
        // Try to parse as a width.
        if width.is_none() {
            if let Ok(w) = match part {
                "thin" => Ok(PixelValue::px(1.0)),
                "medium" => Ok(PixelValue::px(3.0)),
                "thick" => Ok(PixelValue::px(5.0)),
                _ => parse_pixel_value(part),
            } {
                width = Some(w);
                continue;
            }
        }

        // Try to parse as a style.
        if style.is_none() {
            if let Ok(s) = parse_border_style(part) {
                style = Some(s);
                continue;
            }
        }

        // Try to parse as a color.
        if color.is_none() {
            if let Ok(c) = parse_css_color(part) {
                color = Some(c);
                continue;
            }
        }

        // If we get here, the part didn't match anything, or a value was specified twice.
        return Err(CssBorderSideParseError::InvalidDeclaration(input));
    }

    Ok(StyleBorderSide {
        border_width: width.unwrap_or(MEDIUM_BORDER_THICKNESS),
        border_style: style.unwrap_or(BorderStyle::None),
        border_color: color.unwrap_or(ColorU::BLACK),
    })
}

// --- Individual Property Parsers ---

#[cfg(feature = "parser")]
pub fn parse_border_top_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderTopWidth, CssPixelValueParseError<'a>> {
    parse_pixel_value(input).map(|inner| LayoutBorderTopWidth { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_right_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderRightWidth, CssPixelValueParseError<'a>> {
    parse_pixel_value(input).map(|inner| LayoutBorderRightWidth { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_bottom_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderBottomWidth, CssPixelValueParseError<'a>> {
    parse_pixel_value(input).map(|inner| LayoutBorderBottomWidth { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_left_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderLeftWidth, CssPixelValueParseError<'a>> {
    parse_pixel_value(input).map(|inner| LayoutBorderLeftWidth { inner })
}

#[cfg(feature = "parser")]
pub fn parse_border_top_style<'a>(
    input: &'a str,
) -> Result<StyleBorderTopStyle, CssBorderStyleParseError<'a>> {
    parse_border_style(input).map(|inner| StyleBorderTopStyle { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_right_style<'a>(
    input: &'a str,
) -> Result<StyleBorderRightStyle, CssBorderStyleParseError<'a>> {
    parse_border_style(input).map(|inner| StyleBorderRightStyle { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_bottom_style<'a>(
    input: &'a str,
) -> Result<StyleBorderBottomStyle, CssBorderStyleParseError<'a>> {
    parse_border_style(input).map(|inner| StyleBorderBottomStyle { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_left_style<'a>(
    input: &'a str,
) -> Result<StyleBorderLeftStyle, CssBorderStyleParseError<'a>> {
    parse_border_style(input).map(|inner| StyleBorderLeftStyle { inner })
}

#[cfg(feature = "parser")]
pub fn parse_border_top_color<'a>(
    input: &'a str,
) -> Result<StyleBorderTopColor, CssColorParseError<'a>> {
    parse_css_color(input).map(|inner| StyleBorderTopColor { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_right_color<'a>(
    input: &'a str,
) -> Result<StyleBorderRightColor, CssColorParseError<'a>> {
    parse_css_color(input).map(|inner| StyleBorderRightColor { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_bottom_color<'a>(
    input: &'a str,
) -> Result<StyleBorderBottomColor, CssColorParseError<'a>> {
    parse_css_color(input).map(|inner| StyleBorderBottomColor { inner })
}
#[cfg(feature = "parser")]
pub fn parse_border_left_color<'a>(
    input: &'a str,
) -> Result<StyleBorderLeftColor, CssColorParseError<'a>> {
    parse_css_color(input).map(|inner| StyleBorderLeftColor { inner })
}
