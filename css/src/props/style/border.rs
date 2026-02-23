//! CSS properties for border style, width, and color.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

#[cfg(feature = "parser")]
use crate::props::basic::{color::parse_css_color, pixel::parse_pixel_value};
use crate::{
    css::PrintAsCssValue,
    props::{
        basic::{
            color::{ColorU, CssColorParseError, CssColorParseErrorOwned},
            pixel::{
                CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
                MEDIUM_BORDER_THICKNESS, THICK_BORDER_THICKNESS, THIN_BORDER_THICKNESS,
            },
        },
        macros::PixelValueTaker,
    },
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
        impl $struct_name {
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self {
                    inner: self.inner.interpolate(&other.inner, t),
                }
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

// Formatting implementations for border side style values
impl crate::format_rust_code::FormatAsRustCode for StyleBorderTopStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderTopStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBorderRightStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderRightStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBorderLeftStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderLeftStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBorderBottomStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderBottomStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

// Border Color (border-*-color)
define_border_side_property!(StyleBorderTopColor, ColorU);
define_border_side_property!(StyleBorderRightColor, ColorU);
define_border_side_property!(StyleBorderBottomColor, ColorU);
define_border_side_property!(StyleBorderLeftColor, ColorU);

// Border Width (border-*-width)
// The default width is 'medium', which corresponds to 3px.
// Import from pixel.rs for consistency.
define_border_side_property!(LayoutBorderTopWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderRightWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderBottomWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
define_border_side_property!(LayoutBorderLeftWidth, PixelValue, MEDIUM_BORDER_THICKNESS);

// Interpolate implementations for border width types
impl LayoutBorderTopWidth {
    pub fn px(value: f32) -> Self {
        Self {
            inner: PixelValue::px(value),
        }
    }

    pub const fn const_px(value: isize) -> Self {
        Self {
            inner: PixelValue::const_px(value),
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl LayoutBorderRightWidth {
    pub fn px(value: f32) -> Self {
        Self {
            inner: PixelValue::px(value),
        }
    }

    pub const fn const_px(value: isize) -> Self {
        Self {
            inner: PixelValue::const_px(value),
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl LayoutBorderLeftWidth {
    pub fn px(value: f32) -> Self {
        Self {
            inner: PixelValue::px(value),
        }
    }

    pub const fn const_px(value: isize) -> Self {
        Self {
            inner: PixelValue::const_px(value),
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl LayoutBorderBottomWidth {
    pub fn px(value: f32) -> Self {
        Self {
            inner: PixelValue::px(value),
        }
    }

    pub const fn const_px(value: isize) -> Self {
        Self {
            inner: PixelValue::const_px(value),
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

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
#[repr(C, u8)]
pub enum CssBorderStyleParseErrorOwned {
    InvalidStyle(AzString),
}

#[cfg(feature = "parser")]
impl<'a> CssBorderStyleParseError<'a> {
    pub fn to_contained(&self) -> CssBorderStyleParseErrorOwned {
        match self {
            CssBorderStyleParseError::InvalidStyle(s) => {
                CssBorderStyleParseErrorOwned::InvalidStyle(s.to_string().into())
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
#[repr(C, u8)]
pub enum CssBorderSideParseErrorOwned {
    InvalidDeclaration(AzString),
    Width(CssPixelValueParseErrorOwned),
    Style(CssBorderStyleParseErrorOwned),
    Color(CssColorParseErrorOwned),
}

#[cfg(feature = "parser")]
impl<'a> CssBorderSideParseError<'a> {
    pub fn to_contained(&self) -> CssBorderSideParseErrorOwned {
        match self {
            CssBorderSideParseError::InvalidDeclaration(s) => {
                CssBorderSideParseErrorOwned::InvalidDeclaration(s.to_string().into())
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

// Type alias for compatibility with old code
#[cfg(feature = "parser")]
pub type CssBorderParseError<'a> = CssBorderSideParseError<'a>;

/// Newtype wrapper around `CssBorderSideParseErrorOwned` for the `border` shorthand.
#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct CssBorderParseErrorOwned {
    pub inner: CssBorderSideParseErrorOwned,
}

#[cfg(feature = "parser")]
impl From<CssBorderSideParseErrorOwned> for CssBorderParseErrorOwned {
    fn from(v: CssBorderSideParseErrorOwned) -> Self {
        Self { inner: v }
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
                "thin" => Ok(THIN_BORDER_THICKNESS),
                "medium" => Ok(MEDIUM_BORDER_THICKNESS),
                "thick" => Ok(THICK_BORDER_THICKNESS),
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
    let inner = match input.trim() {
        "thin" => THIN_BORDER_THICKNESS,
        "medium" => MEDIUM_BORDER_THICKNESS,
        "thick" => THICK_BORDER_THICKNESS,
        _ => parse_pixel_value(input)?,
    };
    Ok(LayoutBorderTopWidth { inner })
}

#[cfg(feature = "parser")]
pub fn parse_border_right_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderRightWidth, CssPixelValueParseError<'a>> {
    let inner = match input.trim() {
        "thin" => THIN_BORDER_THICKNESS,
        "medium" => MEDIUM_BORDER_THICKNESS,
        "thick" => THICK_BORDER_THICKNESS,
        _ => parse_pixel_value(input)?,
    };
    Ok(LayoutBorderRightWidth { inner })
}

#[cfg(feature = "parser")]
pub fn parse_border_bottom_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderBottomWidth, CssPixelValueParseError<'a>> {
    let inner = match input.trim() {
        "thin" => THIN_BORDER_THICKNESS,
        "medium" => MEDIUM_BORDER_THICKNESS,
        "thick" => THICK_BORDER_THICKNESS,
        _ => parse_pixel_value(input)?,
    };
    Ok(LayoutBorderBottomWidth { inner })
}

#[cfg(feature = "parser")]
pub fn parse_border_left_width<'a>(
    input: &'a str,
) -> Result<LayoutBorderLeftWidth, CssPixelValueParseError<'a>> {
    let inner = match input.trim() {
        "thin" => THIN_BORDER_THICKNESS,
        "medium" => MEDIUM_BORDER_THICKNESS,
        "thick" => THICK_BORDER_THICKNESS,
        _ => parse_pixel_value(input)?,
    };
    Ok(LayoutBorderLeftWidth { inner })
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

// --- Border Color Shorthand ---

/// Parsed result of `border-color` shorthand (1-4 color values)
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderColors {
    pub top: ColorU,
    pub right: ColorU,
    pub bottom: ColorU,
    pub left: ColorU,
}

/// Parses `border-color` shorthand: 1-4 color values
/// - 1 value: all sides
/// - 2 values: top/bottom, left/right
/// - 3 values: top, left/right, bottom
/// - 4 values: top, right, bottom, left
#[cfg(feature = "parser")]
pub fn parse_style_border_color<'a>(
    input: &'a str,
) -> Result<StyleBorderColors, CssColorParseError<'a>> {
    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.len() {
        1 => {
            let color = parse_css_color(parts[0])?;
            Ok(StyleBorderColors {
                top: color,
                right: color,
                bottom: color,
                left: color,
            })
        }
        2 => {
            let top_bottom = parse_css_color(parts[0])?;
            let left_right = parse_css_color(parts[1])?;
            Ok(StyleBorderColors {
                top: top_bottom,
                right: left_right,
                bottom: top_bottom,
                left: left_right,
            })
        }
        3 => {
            let top = parse_css_color(parts[0])?;
            let left_right = parse_css_color(parts[1])?;
            let bottom = parse_css_color(parts[2])?;
            Ok(StyleBorderColors {
                top,
                right: left_right,
                bottom,
                left: left_right,
            })
        }
        4 => {
            let top = parse_css_color(parts[0])?;
            let right = parse_css_color(parts[1])?;
            let bottom = parse_css_color(parts[2])?;
            let left = parse_css_color(parts[3])?;
            Ok(StyleBorderColors {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(CssColorParseError::InvalidColor(input)),
    }
}

// --- Border Style Shorthand ---

/// Parsed result of `border-style` shorthand (1-4 style values)
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderStyles {
    pub top: BorderStyle,
    pub right: BorderStyle,
    pub bottom: BorderStyle,
    pub left: BorderStyle,
}

/// Parses `border-style` shorthand: 1-4 style values
#[cfg(feature = "parser")]
pub fn parse_style_border_style<'a>(
    input: &'a str,
) -> Result<StyleBorderStyles, CssBorderStyleParseError<'a>> {
    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.len() {
        1 => {
            let style = parse_border_style(parts[0])?;
            Ok(StyleBorderStyles {
                top: style,
                right: style,
                bottom: style,
                left: style,
            })
        }
        2 => {
            let top_bottom = parse_border_style(parts[0])?;
            let left_right = parse_border_style(parts[1])?;
            Ok(StyleBorderStyles {
                top: top_bottom,
                right: left_right,
                bottom: top_bottom,
                left: left_right,
            })
        }
        3 => {
            let top = parse_border_style(parts[0])?;
            let left_right = parse_border_style(parts[1])?;
            let bottom = parse_border_style(parts[2])?;
            Ok(StyleBorderStyles {
                top,
                right: left_right,
                bottom,
                left: left_right,
            })
        }
        4 => {
            let top = parse_border_style(parts[0])?;
            let right = parse_border_style(parts[1])?;
            let bottom = parse_border_style(parts[2])?;
            let left = parse_border_style(parts[3])?;
            Ok(StyleBorderStyles {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(CssBorderStyleParseError::InvalidStyle(input)),
    }
}

// --- Border Width Shorthand ---

/// Parsed result of `border-width` shorthand (1-4 width values)
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderWidths {
    pub top: PixelValue,
    pub right: PixelValue,
    pub bottom: PixelValue,
    pub left: PixelValue,
}

/// Parses `border-width` shorthand: 1-4 width values
#[cfg(feature = "parser")]
pub fn parse_style_border_width<'a>(
    input: &'a str,
) -> Result<StyleBorderWidths, CssPixelValueParseError<'a>> {
    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.len() {
        1 => {
            let width = parse_pixel_value(parts[0])?;
            Ok(StyleBorderWidths {
                top: width,
                right: width,
                bottom: width,
                left: width,
            })
        }
        2 => {
            let top_bottom = parse_pixel_value(parts[0])?;
            let left_right = parse_pixel_value(parts[1])?;
            Ok(StyleBorderWidths {
                top: top_bottom,
                right: left_right,
                bottom: top_bottom,
                left: left_right,
            })
        }
        3 => {
            let top = parse_pixel_value(parts[0])?;
            let left_right = parse_pixel_value(parts[1])?;
            let bottom = parse_pixel_value(parts[2])?;
            Ok(StyleBorderWidths {
                top,
                right: left_right,
                bottom,
                left: left_right,
            })
        }
        4 => {
            let top = parse_pixel_value(parts[0])?;
            let right = parse_pixel_value(parts[1])?;
            let bottom = parse_pixel_value(parts[2])?;
            let left = parse_pixel_value(parts[3])?;
            Ok(StyleBorderWidths {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(CssPixelValueParseError::InvalidPixelValue(input)),
    }
}

// Compatibility alias
#[cfg(feature = "parser")]
pub fn parse_style_border<'a>(input: &'a str) -> Result<StyleBorderSide, CssBorderParseError<'a>> {
    parse_border_side(input)
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_border_style() {
        assert_eq!(parse_border_style("solid").unwrap(), BorderStyle::Solid);
        assert_eq!(parse_border_style("dotted").unwrap(), BorderStyle::Dotted);
        assert_eq!(parse_border_style("none").unwrap(), BorderStyle::None);
        assert_eq!(
            parse_border_style("  dashed  ").unwrap(),
            BorderStyle::Dashed
        );
        assert!(parse_border_style("solidd").is_err());
    }

    #[test]
    fn test_parse_border_side_shorthand() {
        // Full
        let result = parse_border_side("2px dotted #ff0000").unwrap();
        assert_eq!(result.border_width, PixelValue::px(2.0));
        assert_eq!(result.border_style, BorderStyle::Dotted);
        assert_eq!(result.border_color, ColorU::new_rgb(255, 0, 0));

        // Different order
        let result = parse_border_side("solid green 1em").unwrap();
        assert_eq!(result.border_width, PixelValue::em(1.0));
        assert_eq!(result.border_style, BorderStyle::Solid);
        assert_eq!(result.border_color, ColorU::new_rgb(0, 128, 0));

        // Missing width
        let result = parse_border_side("ridge #f0f").unwrap();
        assert_eq!(result.border_width, MEDIUM_BORDER_THICKNESS); // default
        assert_eq!(result.border_style, BorderStyle::Ridge);
        assert_eq!(result.border_color, ColorU::new_rgb(255, 0, 255));

        // Missing style
        let result = parse_border_side("5pt blue").unwrap();
        assert_eq!(result.border_width, PixelValue::pt(5.0));
        assert_eq!(result.border_style, BorderStyle::None); // default
        assert_eq!(result.border_color, ColorU::BLUE);

        // Missing color
        let result = parse_border_side("thick double").unwrap();
        assert_eq!(result.border_width, PixelValue::px(5.0));
        assert_eq!(result.border_style, BorderStyle::Double);
        assert_eq!(result.border_color, ColorU::BLACK); // default

        // Only one value
        let result = parse_border_side("inset").unwrap();
        assert_eq!(result.border_width, MEDIUM_BORDER_THICKNESS);
        assert_eq!(result.border_style, BorderStyle::Inset);
        assert_eq!(result.border_color, ColorU::BLACK);
    }

    #[test]
    fn test_parse_border_side_invalid() {
        // Two widths
        assert!(parse_border_side("1px 2px solid red").is_err());
        // Two styles
        assert!(parse_border_side("solid dashed red").is_err());
        // Two colors
        assert!(parse_border_side("red blue solid").is_err());
        // Empty
        assert!(parse_border_side("").is_err());
        // Unknown keyword
        assert!(parse_border_side("1px unknown red").is_err());
    }

    #[test]
    fn test_parse_longhand_border() {
        assert_eq!(
            parse_border_top_width("1.5em").unwrap().inner,
            PixelValue::em(1.5)
        );
        assert_eq!(
            parse_border_left_style("groove").unwrap().inner,
            BorderStyle::Groove
        );
        assert_eq!(
            parse_border_right_color("rgba(10, 20, 30, 0.5)")
                .unwrap()
                .inner,
            ColorU::new(10, 20, 30, 128)
        );
    }
}
