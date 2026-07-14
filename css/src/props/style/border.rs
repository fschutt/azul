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
// +spec:box-model:28fad6 - Border style variants including groove/ridge/inset/outset for separated/collapsing border models
#[derive(Default)]
pub enum BorderStyle {
    #[default]
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


impl fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
            #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
    // NOTE: no separate `PixelValue` specialization arm — the generic
    // `($struct_name, $inner_type:ty, $default)` arm above already matches
    // `define_border_side_property!(.., PixelValue, ..)` (PixelValue is a `:ty`),
    // so a 3-arg PixelValue arm here would be unreachable (unused_macro_rules).
}

// --- Individual Property Structs ---

// +spec:box-model:8c49fe - Border style properties (none, solid, double, dashed, etc.) and border color defaulting to element's color
// Border Style (border-*-style)
/// CSS `border-top-style` property (e.g. `solid`, `dashed`, `none`).
define_border_side_property!(StyleBorderTopStyle, BorderStyle, BorderStyle::None);
/// CSS `border-right-style` property (e.g. `solid`, `dashed`, `none`).
define_border_side_property!(StyleBorderRightStyle, BorderStyle, BorderStyle::None);
/// CSS `border-bottom-style` property (e.g. `solid`, `dashed`, `none`).
define_border_side_property!(StyleBorderBottomStyle, BorderStyle, BorderStyle::None);
/// CSS `border-left-style` property (e.g. `solid`, `dashed`, `none`).
define_border_side_property!(StyleBorderLeftStyle, BorderStyle, BorderStyle::None);

// Formatting implementations for border side style values
impl crate::codegen::format::FormatAsRustCode for StyleBorderTopStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderTopStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBorderRightStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderRightStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBorderLeftStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderLeftStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBorderBottomStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleBorderBottomStyle {{ inner: {} }}",
            &self.inner.format_as_rust_code(tabs)
        )
    }
}

// Border Color (border-*-color)
/// CSS `border-top-color` property. Defaults to `ColorU::BLACK`.
define_border_side_property!(StyleBorderTopColor, ColorU);
/// CSS `border-right-color` property. Defaults to `ColorU::BLACK`.
define_border_side_property!(StyleBorderRightColor, ColorU);
/// CSS `border-bottom-color` property. Defaults to `ColorU::BLACK`.
define_border_side_property!(StyleBorderBottomColor, ColorU);
/// CSS `border-left-color` property. Defaults to `ColorU::BLACK`.
define_border_side_property!(StyleBorderLeftColor, ColorU);

// Border Width (border-*-width)
// The default width is 'medium', which corresponds to 3px.
// Import from pixel.rs for consistency.
/// CSS `border-top-width` property. Defaults to `MEDIUM_BORDER_THICKNESS` (3px).
define_border_side_property!(LayoutBorderTopWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
/// CSS `border-right-width` property. Defaults to `MEDIUM_BORDER_THICKNESS` (3px).
define_border_side_property!(LayoutBorderRightWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
/// CSS `border-bottom-width` property. Defaults to `MEDIUM_BORDER_THICKNESS` (3px).
define_border_side_property!(LayoutBorderBottomWidth, PixelValue, MEDIUM_BORDER_THICKNESS);
/// CSS `border-left-width` property. Defaults to `MEDIUM_BORDER_THICKNESS` (3px).
define_border_side_property!(LayoutBorderLeftWidth, PixelValue, MEDIUM_BORDER_THICKNESS);

macro_rules! impl_border_width_helpers {
    ($($t:ty),+) => { $(
        impl $t {
            #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self { inner: self.inner.interpolate(&other.inner, t) }
            }
            #[must_use] pub const fn const_px(value: isize) -> Self {
                Self { inner: PixelValue::const_px(value) }
            }
        }
    )+ };
}

impl_border_width_helpers!(
    LayoutBorderTopWidth,
    LayoutBorderRightWidth,
    LayoutBorderBottomWidth,
    LayoutBorderLeftWidth
);

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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssBorderStyleParseErrorOwned {
    InvalidStyle(AzString),
}

#[cfg(feature = "parser")]
impl CssBorderStyleParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBorderStyleParseErrorOwned {
        match self {
            CssBorderStyleParseError::InvalidStyle(s) => {
                CssBorderStyleParseErrorOwned::InvalidStyle((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl CssBorderStyleParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssBorderStyleParseError<'_> {
        match self {
            Self::InvalidStyle(s) => {
                CssBorderStyleParseError::InvalidStyle(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-style` value.
pub fn parse_border_style(input: &str) -> Result<BorderStyle, CssBorderStyleParseError<'_>> {
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
impl CssBorderSideParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBorderSideParseErrorOwned {
        match self {
            CssBorderSideParseError::InvalidDeclaration(s) => {
                CssBorderSideParseErrorOwned::InvalidDeclaration((*s).to_string().into())
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
    #[must_use] pub fn to_shared(&self) -> CssBorderSideParseError<'_> {
        match self {
            Self::InvalidDeclaration(s) => {
                CssBorderSideParseError::InvalidDeclaration(s.as_str())
            }
            Self::Width(e) => CssBorderSideParseError::Width(e.to_shared()),
            Self::Style(e) => CssBorderSideParseError::Style(e.to_shared()),
            Self::Color(e) => CssBorderSideParseError::Color(e.to_shared()),
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
fn parse_border_side(
    input: &str,
) -> Result<StyleBorderSide, CssBorderSideParseError<'_>> {
    let mut width = None;
    let mut style = None;
    let mut color = None;

    if input.trim().is_empty() {
        return Err(CssBorderSideParseError::InvalidDeclaration(input));
    }

    for part in input.split_whitespace() {
        // Try to parse as a width.
        if width.is_none() {
            if let Ok(w) = parse_border_width_value(part) {
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
fn parse_border_width_value(
    input: &str,
) -> Result<PixelValue, CssPixelValueParseError<'_>> {
    match input.trim() {
        "thin" => Ok(THIN_BORDER_THICKNESS),
        "medium" => Ok(MEDIUM_BORDER_THICKNESS),
        "thick" => Ok(THICK_BORDER_THICKNESS),
        _ => parse_pixel_value(input),
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-top-width` value.
pub fn parse_border_top_width(
    input: &str,
) -> Result<LayoutBorderTopWidth, CssPixelValueParseError<'_>> {
    parse_border_width_value(input).map(|inner| LayoutBorderTopWidth { inner })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-right-width` value.
pub fn parse_border_right_width(
    input: &str,
) -> Result<LayoutBorderRightWidth, CssPixelValueParseError<'_>> {
    parse_border_width_value(input).map(|inner| LayoutBorderRightWidth { inner })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-bottom-width` value.
pub fn parse_border_bottom_width(
    input: &str,
) -> Result<LayoutBorderBottomWidth, CssPixelValueParseError<'_>> {
    parse_border_width_value(input).map(|inner| LayoutBorderBottomWidth { inner })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-left-width` value.
pub fn parse_border_left_width(
    input: &str,
) -> Result<LayoutBorderLeftWidth, CssPixelValueParseError<'_>> {
    parse_border_width_value(input).map(|inner| LayoutBorderLeftWidth { inner })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-top-style` value.
pub fn parse_border_top_style(
    input: &str,
) -> Result<StyleBorderTopStyle, CssBorderStyleParseError<'_>> {
    parse_border_style(input).map(|inner| StyleBorderTopStyle { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-right-style` value.
pub fn parse_border_right_style(
    input: &str,
) -> Result<StyleBorderRightStyle, CssBorderStyleParseError<'_>> {
    parse_border_style(input).map(|inner| StyleBorderRightStyle { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-bottom-style` value.
pub fn parse_border_bottom_style(
    input: &str,
) -> Result<StyleBorderBottomStyle, CssBorderStyleParseError<'_>> {
    parse_border_style(input).map(|inner| StyleBorderBottomStyle { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-left-style` value.
pub fn parse_border_left_style(
    input: &str,
) -> Result<StyleBorderLeftStyle, CssBorderStyleParseError<'_>> {
    parse_border_style(input).map(|inner| StyleBorderLeftStyle { inner })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-top-color` value.
pub fn parse_border_top_color(
    input: &str,
) -> Result<StyleBorderTopColor, CssColorParseError<'_>> {
    parse_css_color(input).map(|inner| StyleBorderTopColor { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-right-color` value.
pub fn parse_border_right_color(
    input: &str,
) -> Result<StyleBorderRightColor, CssColorParseError<'_>> {
    parse_css_color(input).map(|inner| StyleBorderRightColor { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-bottom-color` value.
pub fn parse_border_bottom_color(
    input: &str,
) -> Result<StyleBorderBottomColor, CssColorParseError<'_>> {
    parse_css_color(input).map(|inner| StyleBorderBottomColor { inner })
}
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-left-color` value.
pub fn parse_border_left_color(
    input: &str,
) -> Result<StyleBorderLeftColor, CssColorParseError<'_>> {
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-color` value.
pub fn parse_style_border_color(
    input: &str,
) -> Result<StyleBorderColors, CssColorParseError<'_>> {
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-style` value.
pub fn parse_style_border_style(
    input: &str,
) -> Result<StyleBorderStyles, CssBorderStyleParseError<'_>> {
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-width` value.
pub fn parse_style_border_width(
    input: &str,
) -> Result<StyleBorderWidths, CssPixelValueParseError<'_>> {
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border` value.
pub fn parse_style_border(input: &str) -> Result<StyleBorderSide, CssBorderParseError<'_>> {
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

#[cfg(test)]
mod autotest_generated {
    use super::*;

    const ALL_STYLES: [BorderStyle; 10] = [
        BorderStyle::None,
        BorderStyle::Solid,
        BorderStyle::Double,
        BorderStyle::Dotted,
        BorderStyle::Dashed,
        BorderStyle::Hidden,
        BorderStyle::Groove,
        BorderStyle::Ridge,
        BorderStyle::Inset,
        BorderStyle::Outset,
    ];

    // =====================================================================
    // BorderStyle: Display / PrintAsCssValue / Default
    // =====================================================================

    #[test]
    fn border_style_display_is_a_unique_lowercase_keyword_for_every_variant() {
        let mut seen: Vec<String> = Vec::new();
        for style in ALL_STYLES {
            let s = style.to_string();
            assert!(!s.is_empty(), "{style:?} renders as the empty string");
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase()),
                "{style:?} renders as {s:?}, which is not a lowercase ASCII keyword"
            );
            assert!(
                !seen.contains(&s),
                "two BorderStyle variants both render as {s:?} (copy-paste in Display)"
            );
            seen.push(s);
        }
        assert_eq!(seen.len(), ALL_STYLES.len());
    }

    #[test]
    fn border_style_print_as_css_value_matches_display() {
        for style in ALL_STYLES {
            assert_eq!(style.print_as_css_value(), style.to_string());
        }
    }

    #[test]
    fn border_style_default_is_none_and_formats_as_none() {
        assert_eq!(BorderStyle::default(), BorderStyle::None);
        assert_eq!(BorderStyle::default().to_string(), "none");
    }

    // =====================================================================
    // Side-property structs: Default / Debug / const_px / interpolate
    // =====================================================================

    #[test]
    fn border_side_property_defaults_match_the_css_initial_values() {
        // border-*-style initial value is `none`
        assert_eq!(StyleBorderTopStyle::default().inner, BorderStyle::None);
        assert_eq!(StyleBorderRightStyle::default().inner, BorderStyle::None);
        assert_eq!(StyleBorderBottomStyle::default().inner, BorderStyle::None);
        assert_eq!(StyleBorderLeftStyle::default().inner, BorderStyle::None);

        // border-*-color has no `currentcolor` here; the documented stand-in is BLACK
        assert_eq!(StyleBorderTopColor::default().inner, ColorU::BLACK);
        assert_eq!(StyleBorderRightColor::default().inner, ColorU::BLACK);
        assert_eq!(StyleBorderBottomColor::default().inner, ColorU::BLACK);
        assert_eq!(StyleBorderLeftColor::default().inner, ColorU::BLACK);

        // border-*-width initial value is `medium` (3px)
        assert_eq!(
            LayoutBorderTopWidth::default().inner,
            MEDIUM_BORDER_THICKNESS
        );
        assert_eq!(
            LayoutBorderRightWidth::default().inner,
            MEDIUM_BORDER_THICKNESS
        );
        assert_eq!(
            LayoutBorderBottomWidth::default().inner,
            MEDIUM_BORDER_THICKNESS
        );
        assert_eq!(
            LayoutBorderLeftWidth::default().inner,
            MEDIUM_BORDER_THICKNESS
        );
        assert_eq!(MEDIUM_BORDER_THICKNESS, PixelValue::px(3.0));
    }

    #[test]
    fn border_side_property_debug_impls_are_the_documented_shapes() {
        // The macro deliberately overrides Debug: styles print the keyword,
        // colors print the 8-digit hash, widths print the pixel value.
        assert_eq!(format!("{:?}", StyleBorderTopStyle::default()), "none");
        assert_eq!(
            format!(
                "{:?}",
                StyleBorderLeftStyle {
                    inner: BorderStyle::Groove
                }
            ),
            "groove"
        );
        assert_eq!(
            format!("{:?}", StyleBorderTopColor::default()),
            "#000000ff"
        );
        assert_eq!(format!("{:?}", LayoutBorderTopWidth::default()), "3px");
    }

    #[test]
    fn layout_border_width_const_px_matches_the_runtime_constructor() {
        assert_eq!(
            LayoutBorderTopWidth::const_px(5).inner,
            PixelValue::px(5.0)
        );
        assert_eq!(LayoutBorderRightWidth::const_px(0).inner, PixelValue::zero());
        assert_eq!(
            LayoutBorderBottomWidth::const_px(-2).inner,
            PixelValue::px(-2.0)
        );
        // The largest magnitude `const_px` can scale by FP_PRECISION_MULTIPLIER
        // (1000) without overflowing the isize multiply. Anything beyond this
        // overflows — see the FloatValue::const_new tests in length.rs.
        let max_safe = isize::MAX / 1000;
        assert!(LayoutBorderLeftWidth::const_px(max_safe)
            .inner
            .number
            .get()
            .is_finite());
    }

    #[test]
    fn layout_border_width_interpolate_endpoints_are_exact() {
        let a = LayoutBorderTopWidth::const_px(0);
        let b = LayoutBorderTopWidth::const_px(10);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5).inner, PixelValue::px(5.0));
    }

    #[test]
    fn layout_border_width_interpolate_stays_finite_for_hostile_t() {
        let a = LayoutBorderTopWidth::const_px(0);
        let b = LayoutBorderTopWidth::const_px(10);
        for t in [
            0.0,
            1.0,
            -1.0,
            2.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
        ] {
            // A NaN/inf must never leak out of the animation path: FloatValue
            // stores an isize, so the cast saturates instead of propagating.
            assert!(
                a.interpolate(&b, t).inner.number.get().is_finite(),
                "interpolate(t = {t}) produced a non-finite width"
            );
            assert!(b.interpolate(&a, t).inner.number.get().is_finite());
        }
    }

    #[test]
    fn layout_border_width_interpolate_across_metrics_stays_finite() {
        let px = LayoutBorderRightWidth {
            inner: PixelValue::px(4.0),
        };
        let em = LayoutBorderRightWidth {
            inner: PixelValue::em(2.0),
        };
        let percent = LayoutBorderRightWidth {
            inner: PixelValue::percent(100.0),
        };
        for t in [0.0, 0.5, 1.0, -3.0, f32::NAN, f32::INFINITY] {
            assert!(px.interpolate(&em, t).inner.number.get().is_finite());
            assert!(em.interpolate(&percent, t).inner.number.get().is_finite());
            assert!(percent.interpolate(&px, t).inner.number.get().is_finite());
        }
    }

    #[test]
    fn style_border_color_interpolate_endpoints_are_exact() {
        let black = StyleBorderTopColor {
            inner: ColorU::BLACK,
        };
        let white = StyleBorderTopColor {
            inner: ColorU::WHITE,
        };
        assert_eq!(black.interpolate(&white, 0.0).inner, ColorU::BLACK);
        assert_eq!(black.interpolate(&white, 1.0).inner, ColorU::WHITE);
        let mid = black.interpolate(&white, 0.5).inner;
        assert_eq!((mid.r, mid.g, mid.b), (128, 128, 128));
    }

    #[test]
    fn style_border_color_interpolate_hostile_t_does_not_panic() {
        let a = StyleBorderLeftColor {
            inner: ColorU::new(10, 20, 30, 40),
        };
        let b = StyleBorderLeftColor {
            inner: ColorU::new(200, 210, 220, 230),
        };
        for t in [
            -1000.0,
            1000.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
        ] {
            // u8 channels saturate; the only requirement is that this returns.
            let _ = a.interpolate(&b, t);
            let _ = b.interpolate(&a, t);
        }
    }

    // =====================================================================
    // parse_border_style
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_accepts_every_keyword() {
        assert_eq!(parse_border_style("none").unwrap(), BorderStyle::None);
        assert_eq!(parse_border_style("solid").unwrap(), BorderStyle::Solid);
        assert_eq!(parse_border_style("double").unwrap(), BorderStyle::Double);
        assert_eq!(parse_border_style("dotted").unwrap(), BorderStyle::Dotted);
        assert_eq!(parse_border_style("dashed").unwrap(), BorderStyle::Dashed);
        assert_eq!(parse_border_style("hidden").unwrap(), BorderStyle::Hidden);
        assert_eq!(parse_border_style("groove").unwrap(), BorderStyle::Groove);
        assert_eq!(parse_border_style("ridge").unwrap(), BorderStyle::Ridge);
        assert_eq!(parse_border_style("inset").unwrap(), BorderStyle::Inset);
        assert_eq!(parse_border_style("outset").unwrap(), BorderStyle::Outset);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_round_trips_through_display() {
        for style in ALL_STYLES {
            let encoded = style.to_string();
            assert_eq!(
                parse_border_style(&encoded).unwrap(),
                style,
                "{encoded} did not round-trip"
            );
            // and through the PrintAsCssValue path, which must agree
            assert_eq!(
                parse_border_style(&style.print_as_css_value()).unwrap(),
                style
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_trims_surrounding_whitespace() {
        for input in [" solid", "solid ", "\t\nsolid\r\n ", "   solid   "] {
            assert_eq!(
                parse_border_style(input).unwrap(),
                BorderStyle::Solid,
                "{input:?} should trim to `solid`"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_empty_and_whitespace_only_are_errors() {
        for input in ["", " ", "   ", "\t", "\n", "\r\n\t "] {
            assert!(
                parse_border_style(input).is_err(),
                "{input:?} must not parse as a border style"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_error_carries_the_untrimmed_input() {
        // The Ok path trims, but the Err path hands back the *raw* input.
        let input = "  bogus  ";
        let err = parse_border_style(input).unwrap_err();
        assert!(
            matches!(err, CssBorderStyleParseError::InvalidStyle(s) if s == input),
            "unexpected error payload: {err:?}"
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_rejects_uppercase_keywords() {
        // NOTE: CSS keywords are ASCII case-insensitive, so a spec-conformant
        // parser would accept these. This parser does not — asserted here so
        // the divergence is visible rather than silent.
        for input in ["SOLID", "Solid", "sOlId", "NONE", "Dashed"] {
            assert!(
                parse_border_style(input).is_err(),
                "{input:?} unexpectedly parsed (case-insensitivity was added?)"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_rejects_garbage_unicode_and_numbers() {
        for input in [
            "solidd",
            "soli",
            "solid solid",
            "solid;garbage",
            "solid!important",
            "0",
            "-0",
            "1px",
            "9223372036854775807",
            "NaN",
            "inf",
            "-inf",
            "\u{1F600}",
            "s\u{0301}olid",
            "sölid",
            "\u{0}",
            "\u{202e}solid",
            "sol\tid",
            "()",
            "solid()",
        ] {
            assert!(
                parse_border_style(input).is_err(),
                "{input:?} unexpectedly parsed as a border style"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_style_handles_huge_and_nested_input_without_panicking() {
        let huge = "a".repeat(100_000);
        assert!(parse_border_style(&huge).is_err());

        let repeated = "solid ".repeat(50_000);
        assert!(parse_border_style(&repeated).is_err());

        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_border_style(&nested).is_err());

        let padded = format!("{}solid{}", " ".repeat(100_000), " ".repeat(100_000));
        assert_eq!(parse_border_style(&padded).unwrap(), BorderStyle::Solid);
    }

    // =====================================================================
    // border-*-style longhands
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn border_style_longhands_agree_with_parse_border_style() {
        for style in ALL_STYLES {
            let input = style.to_string();
            assert_eq!(parse_border_top_style(&input).unwrap().inner, style);
            assert_eq!(parse_border_right_style(&input).unwrap().inner, style);
            assert_eq!(parse_border_bottom_style(&input).unwrap().inner, style);
            assert_eq!(parse_border_left_style(&input).unwrap().inner, style);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_style_longhands_reject_everything_parse_border_style_rejects() {
        for input in ["", "   ", "SOLID", "solidd", "\u{1F600}", "1px", "solid red"] {
            assert!(parse_border_top_style(input).is_err(), "top: {input:?}");
            assert!(parse_border_right_style(input).is_err(), "right: {input:?}");
            assert!(
                parse_border_bottom_style(input).is_err(),
                "bottom: {input:?}"
            );
            assert!(parse_border_left_style(input).is_err(), "left: {input:?}");
        }
    }

    // =====================================================================
    // parse_border_width_value (private)
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_accepts_the_three_keywords() {
        assert_eq!(
            parse_border_width_value("thin").unwrap(),
            THIN_BORDER_THICKNESS
        );
        assert_eq!(
            parse_border_width_value("medium").unwrap(),
            MEDIUM_BORDER_THICKNESS
        );
        assert_eq!(
            parse_border_width_value("thick").unwrap(),
            THICK_BORDER_THICKNESS
        );
        // keywords are trimmed like everything else
        assert_eq!(
            parse_border_width_value("  \tthick\n ").unwrap(),
            THICK_BORDER_THICKNESS
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_rejects_uppercase_keywords() {
        // Same case-sensitivity divergence as parse_border_style.
        for input in ["THIN", "Medium", "THICK"] {
            assert!(
                parse_border_width_value(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_empty_and_whitespace_only_are_errors() {
        for input in ["", " ", "\t\n", "    "] {
            let err = parse_border_width_value(input).unwrap_err();
            assert!(
                matches!(err, CssPixelValueParseError::EmptyString),
                "{input:?} -> {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_bare_number_is_interpreted_as_px() {
        assert_eq!(parse_border_width_value("0").unwrap(), PixelValue::px(0.0));
        assert_eq!(parse_border_width_value("42").unwrap(), PixelValue::px(42.0));
        assert_eq!(
            parse_border_width_value("1.5").unwrap(),
            PixelValue::px(1.5)
        );
        // -0 collapses to +0 once quantized into the isize-backed FloatValue
        assert_eq!(parse_border_width_value("-0").unwrap(), PixelValue::px(0.0));
        // negative widths are *accepted* (CSS would reject them) — pinned so a
        // future validity check is a deliberate change, not an accident.
        assert_eq!(
            parse_border_width_value("-5px").unwrap(),
            PixelValue::px(-5.0)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_nan_saturates_to_zero() {
        // "NaN" is a valid f32 literal for Rust's FromStr, so this reaches
        // FloatValue::new(NaN) — which saturates to 0 rather than storing NaN.
        let parsed = parse_border_width_value("NaN").unwrap();
        assert!(parsed.number.get().is_finite());
        assert_eq!(parsed, PixelValue::px(0.0));

        let parsed = parse_border_width_value("NaNpx").unwrap();
        assert_eq!(parsed, PixelValue::px(0.0));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_infinities_and_overflow_saturate_finite() {
        for input in [
            "inf",
            "-inf",
            "infpx",
            "1e999",
            "-1e999",
            "1e40px",
            "340282350000000000000000000000000000000px", // ~f32::MAX
            "9223372036854775807",                       // i64::MAX
            "-9223372036854775808",                      // i64::MIN
        ] {
            let parsed = parse_border_width_value(input)
                .unwrap_or_else(|e| panic!("{input:?} failed to parse: {e:?}"));
            assert!(
                parsed.number.get().is_finite(),
                "{input:?} produced a non-finite width: {parsed:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_rejects_garbage_and_unicode() {
        for input in [
            "px",
            "em",
            "abc",
            "1px 2px",
            "1px;",
            "--1px",
            "1PX",
            "\u{1F600}",
            "1\u{1F600}px",
            "١px", // arabic-indic digit one
            "()",
            "calc(1px + 2px)",
        ] {
            assert!(
                parse_border_width_value(input).is_err(),
                "{input:?} unexpectedly parsed as a border width"
            );
        }

        // ...but note the suffix strip trims what's left of the number, so a
        // space between value and unit is silently accepted:
        assert_eq!(parse_border_width_value("1 px").unwrap(), PixelValue::px(1.0));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_bare_unit_reports_no_value_given() {
        let err = parse_border_width_value("px").unwrap_err();
        assert!(
            matches!(err, CssPixelValueParseError::NoValueGiven(..)),
            "expected NoValueGiven, got {err:?}"
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_width_value_huge_input_does_not_hang() {
        let huge_digits = format!("{}px", "9".repeat(1_000));
        let parsed = parse_border_width_value(&huge_digits).unwrap();
        assert!(parsed.number.get().is_finite());

        let huge_garbage = "z".repeat(100_000);
        assert!(parse_border_width_value(&huge_garbage).is_err());

        let padded = format!("{}1px{}", " ".repeat(50_000), " ".repeat(50_000));
        assert_eq!(
            parse_border_width_value(&padded).unwrap(),
            PixelValue::px(1.0)
        );
    }

    // =====================================================================
    // border-*-width longhands
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn border_width_longhands_agree_with_each_other() {
        for input in ["thin", "medium", "thick", "0", "1.5em", "3px", "50%", "-2pt"] {
            let expected = parse_border_width_value(input).unwrap();
            assert_eq!(parse_border_top_width(input).unwrap().inner, expected);
            assert_eq!(parse_border_right_width(input).unwrap().inner, expected);
            assert_eq!(parse_border_bottom_width(input).unwrap().inner, expected);
            assert_eq!(parse_border_left_width(input).unwrap().inner, expected);
        }
        for input in ["", "   ", "px", "abc", "\u{1F600}", "1px 2px"] {
            assert!(parse_border_top_width(input).is_err(), "top: {input:?}");
            assert!(parse_border_right_width(input).is_err(), "right: {input:?}");
            assert!(
                parse_border_bottom_width(input).is_err(),
                "bottom: {input:?}"
            );
            assert!(parse_border_left_width(input).is_err(), "left: {input:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_width_longhands_round_trip_through_display() {
        for value in [
            PixelValue::px(0.0),
            PixelValue::px(1.0),
            PixelValue::px(1.5),
            PixelValue::px(-3.25),
            PixelValue::em(2.0),
            PixelValue::rem(0.5),
            PixelValue::pt(12.0),
            PixelValue::inch(1.0),
            PixelValue::cm(2.5),
            PixelValue::mm(10.0),
            PixelValue::percent(50.0),
            THIN_BORDER_THICKNESS,
            MEDIUM_BORDER_THICKNESS,
            THICK_BORDER_THICKNESS,
        ] {
            let encoded = value.to_string();
            let decoded = parse_border_top_width(&encoded)
                .unwrap_or_else(|e| panic!("{encoded} failed to re-parse: {e:?}"))
                .inner;
            assert_eq!(decoded, value, "{encoded} did not round-trip");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_width_longhands_inherit_the_vmin_suffix_shadowing_bug() {
        // FIXED (this pin flipped, as intended): the suffix table used to test "in"
        // before "vmin", so "5vmin" stripped "in" and failed to parse "5vm" — every
        // `border-width: 5vmin` was rejected. The table now orders "vmin" ahead of "in".
        assert_eq!(
            parse_border_top_width("5vmin").unwrap().inner,
            PixelValue::from_metric(crate::props::basic::SizeMetric::Vmin, 5.0)
        );
        assert_eq!(
            parse_border_left_width("5vmin").unwrap().inner,
            PixelValue::from_metric(crate::props::basic::SizeMetric::Vmin, 5.0)
        );
        assert_eq!(
            parse_border_top_width("5vmax").unwrap().inner,
            PixelValue::from_metric(crate::props::basic::SizeMetric::Vmax, 5.0)
        );
        assert_eq!(
            parse_border_top_width("5vw").unwrap().inner,
            PixelValue::from_metric(crate::props::basic::SizeMetric::Vw, 5.0)
        );
    }

    // =====================================================================
    // border-*-color longhands
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn border_color_longhands_agree_with_each_other() {
        for input in [
            "#ff0000",
            "#f0f",
            "#11223344",
            "red",
            "transparent",
            "rgb(1, 2, 3)",
            "rgba(10, 20, 30, 0.5)",
            "hsl(0, 100%, 50%)",
        ] {
            let expected = parse_css_color(input)
                .unwrap_or_else(|e| panic!("{input:?} failed to parse: {e:?}"));
            assert_eq!(parse_border_top_color(input).unwrap().inner, expected);
            assert_eq!(parse_border_right_color(input).unwrap().inner, expected);
            assert_eq!(parse_border_bottom_color(input).unwrap().inner, expected);
            assert_eq!(parse_border_left_color(input).unwrap().inner, expected);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_color_longhands_round_trip_through_to_hash() {
        for color in [
            ColorU::BLACK,
            ColorU::WHITE,
            ColorU::RED,
            ColorU::BLUE,
            ColorU::TRANSPARENT,
            ColorU::new(1, 2, 3, 4),
            ColorU::new(255, 254, 253, 252),
            ColorU::new(0, 128, 0, 255),
        ] {
            let encoded = color.to_hash();
            assert_eq!(
                parse_border_left_color(&encoded)
                    .unwrap_or_else(|e| panic!("{encoded} failed to re-parse: {e:?}"))
                    .inner,
                color,
                "{encoded} did not round-trip"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_color_longhands_empty_input_is_an_error() {
        for input in ["", " ", "\t\n  "] {
            let err = parse_border_top_color(input).unwrap_err();
            assert!(
                matches!(err, CssColorParseError::EmptyInput),
                "{input:?} -> {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_color_longhands_reject_garbage_and_unicode() {
        for input in [
            "#",
            "#z",
            "#ff",
            "#fffff",
            "#\u{1F600}",
            "notacolor",
            "rgb(1, 2)",
            "rgb(1, 2, 3, 4, 5)",
            "\u{1F600}",
            "red;",
            "red blue",
            "0",
        ] {
            assert!(
                parse_border_top_color(input).is_err(),
                "{input:?} unexpectedly parsed as a color"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn border_color_longhands_never_panic_on_hostile_input() {
        let long = "f".repeat(100_000);
        let nested = format!("rgb{}1,2,3{}", "(".repeat(10_000), ")".repeat(10_000));
        let deep_parens = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        let hostile: [&str; 18] = [
            "#",
            "##ff0000",
            "rgb(",
            "rgb()",
            "rgba(0,0,0,NaN)",
            "rgba(0,0,0,inf)",
            "rgb(-1,-2,-3)",
            "rgb(999,999,999)",
            "rgb(NaN, NaN, NaN)",
            "hsl(inf, 0%, 0%)",
            "hsla(NaN, NaN%, NaN%, NaN)",
            "\u{0}",
            "\u{202e}",
            "s\u{0301}",
            ")))",
            &long,
            &nested,
            &deep_parens,
        ];
        for input in hostile {
            // The contract is only "returns, never panics / never overflows the stack".
            let _ = parse_border_top_color(input);
            let _ = parse_border_right_color(input);
            let _ = parse_border_bottom_color(input);
            let _ = parse_border_left_color(input);
        }
    }

    // =====================================================================
    // parse_border_side / parse_style_border
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_positive_control() {
        let side = parse_border_side("1px solid red").unwrap();
        assert_eq!(side.border_width, PixelValue::px(1.0));
        assert_eq!(side.border_style, BorderStyle::Solid);
        assert_eq!(side.border_color, ColorU::RED);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_is_component_order_independent() {
        let expected = StyleBorderSide {
            border_width: PixelValue::px(2.0),
            border_style: BorderStyle::Dashed,
            border_color: ColorU::new_rgb(0, 255, 0),
        };
        for input in [
            "2px dashed #00ff00",
            "2px #00ff00 dashed",
            "dashed 2px #00ff00",
            "dashed #00ff00 2px",
            "#00ff00 2px dashed",
            "#00ff00 dashed 2px",
            "  2px   dashed   #00ff00  ",
            "\t2px\ndashed\r#00ff00\t",
        ] {
            assert_eq!(
                parse_border_side(input).unwrap(),
                expected,
                "{input:?} parsed differently"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_applies_defaults_for_missing_components() {
        // Missing components fall back to medium / none / black.
        let only_style = parse_border_side("inset").unwrap();
        assert_eq!(only_style.border_width, MEDIUM_BORDER_THICKNESS);
        assert_eq!(only_style.border_style, BorderStyle::Inset);
        assert_eq!(only_style.border_color, ColorU::BLACK);

        let only_width = parse_border_side("7px").unwrap();
        assert_eq!(only_width.border_width, PixelValue::px(7.0));
        assert_eq!(only_width.border_style, BorderStyle::None);
        assert_eq!(only_width.border_color, ColorU::BLACK);

        let only_color = parse_border_side("blue").unwrap();
        assert_eq!(only_color.border_width, MEDIUM_BORDER_THICKNESS);
        assert_eq!(only_color.border_style, BorderStyle::None);
        assert_eq!(only_color.border_color, ColorU::BLUE);

        // keyword widths work in the shorthand too
        let keyword = parse_border_side("thin solid").unwrap();
        assert_eq!(keyword.border_width, THIN_BORDER_THICKNESS);
        assert_eq!(keyword.border_style, BorderStyle::Solid);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_rejects_duplicate_components() {
        for input in [
            "1px 2px solid red",
            "solid dashed red",
            "red blue solid",
            "1px solid red 2px",
            "1px solid red solid",
            "1px solid red red",
        ] {
            let err = parse_border_side(input).unwrap_err();
            assert!(
                matches!(err, CssBorderSideParseError::InvalidDeclaration(s) if s == input),
                "{input:?} -> {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_empty_and_whitespace_only_are_errors() {
        for input in ["", " ", "\t\n", "      "] {
            let err = parse_border_side(input).unwrap_err();
            // The raw (untrimmed) input is echoed back in the error.
            assert!(
                matches!(err, CssBorderSideParseError::InvalidDeclaration(s) if s == input),
                "{input:?} -> {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_rejects_unknown_tokens() {
        for input in [
            "1px unknown red",
            "1px solid red !important",
            "1px solid red;",
            "\u{1F600}",
            "1px solid \u{1F600}",
            "solid \u{0}",
            "1px, solid, red",
        ] {
            assert!(
                parse_border_side(input).is_err(),
                "{input:?} unexpectedly parsed as a border shorthand"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_hostile_numbers_stay_finite() {
        // "NaN" / "inf" are valid f32 literals, so they *do* parse as widths —
        // but the isize-backed FloatValue saturates them to a finite value.
        for input in ["NaN solid red", "inf solid red", "1e999 solid red"] {
            let side = parse_border_side(input)
                .unwrap_or_else(|e| panic!("{input:?} failed to parse: {e:?}"));
            assert!(
                side.border_width.number.get().is_finite(),
                "{input:?} produced a non-finite width: {:?}",
                side.border_width
            );
            assert_eq!(side.border_style, BorderStyle::Solid);
            assert_eq!(side.border_color, ColorU::RED);
        }
        assert_eq!(
            parse_border_side("NaN solid red").unwrap().border_width,
            PixelValue::px(0.0)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_border_side_long_and_nested_input_does_not_hang() {
        // Repeated tokens: the 2nd `solid` cannot be re-assigned, so this must
        // bail out immediately rather than scanning all 50k tokens.
        let repeated = "solid ".repeat(50_000);
        assert!(parse_border_side(&repeated).is_err());

        let repeated_px = "1px ".repeat(50_000);
        assert!(parse_border_side(&repeated_px).is_err());

        let huge_token = "z".repeat(100_000);
        assert!(parse_border_side(&huge_token).is_err());

        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_border_side(&nested).is_err());

        let padded = format!("{}1px solid red{}", " ".repeat(50_000), " ".repeat(50_000));
        assert_eq!(
            parse_border_side(&padded).unwrap().border_style,
            BorderStyle::Solid
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_is_an_alias_of_parse_border_side() {
        for input in [
            "1px solid red",
            "thick double",
            "inset",
            "",
            "   ",
            "1px 2px solid red",
            "\u{1F600}",
            "solid green 1em",
        ] {
            match (parse_style_border(input), parse_border_side(input)) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "{input:?}"),
                (Err(a), Err(b)) => assert_eq!(a, b, "{input:?}"),
                (a, b) => panic!("{input:?}: alias disagrees: {a:?} vs {b:?}"),
            }
        }
    }

    // =====================================================================
    // border-color shorthand
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_color_expands_one_to_four_values() {
        let red = ColorU::RED;
        let blue = ColorU::BLUE;
        let green = ColorU::new_rgb(0, 128, 0);
        let white = ColorU::WHITE;

        let one = parse_style_border_color("red").unwrap();
        assert_eq!(
            one,
            StyleBorderColors {
                top: red,
                right: red,
                bottom: red,
                left: red
            }
        );

        // 2 values: top/bottom, left/right
        let two = parse_style_border_color("red blue").unwrap();
        assert_eq!(
            two,
            StyleBorderColors {
                top: red,
                right: blue,
                bottom: red,
                left: blue
            }
        );

        // 3 values: top, left/right, bottom
        let three = parse_style_border_color("red blue green").unwrap();
        assert_eq!(
            three,
            StyleBorderColors {
                top: red,
                right: blue,
                bottom: green,
                left: blue
            }
        );

        // 4 values: top, right, bottom, left
        let four = parse_style_border_color("red blue green white").unwrap();
        assert_eq!(
            four,
            StyleBorderColors {
                top: red,
                right: blue,
                bottom: green,
                left: white
            }
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_color_normalizes_whitespace() {
        let expected = parse_style_border_color("red blue green").unwrap();
        for input in [
            "  red blue green  ",
            "red\tblue\ngreen",
            "red   blue \r\n green",
        ] {
            assert_eq!(parse_style_border_color(input).unwrap(), expected, "{input:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_color_rejects_zero_and_more_than_four_values() {
        for input in ["", "   ", "\t\n"] {
            let err = parse_style_border_color(input).unwrap_err();
            assert!(
                matches!(err, CssColorParseError::InvalidColor(_)),
                "{input:?} -> {err:?}"
            );
        }
        let too_many = "red ".repeat(1_000);
        let inputs: [&str; 3] = [
            "red red red red red",
            "red blue green white black yellow",
            &too_many,
        ];
        for input in inputs {
            assert!(
                parse_style_border_color(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_color_propagates_component_errors() {
        for input in [
            "notacolor",
            "red notacolor",
            "red blue notacolor",
            "red blue green notacolor",
            "red \u{1F600}",
            "#zzz",
        ] {
            assert!(
                parse_style_border_color(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_color_round_trips_through_to_hash() {
        let colors = StyleBorderColors {
            top: ColorU::new(1, 2, 3, 4),
            right: ColorU::new(255, 0, 0, 255),
            bottom: ColorU::TRANSPARENT,
            left: ColorU::new(9, 8, 7, 6),
        };
        let encoded = format!(
            "{} {} {} {}",
            colors.top.to_hash(),
            colors.right.to_hash(),
            colors.bottom.to_hash(),
            colors.left.to_hash()
        );
        assert_eq!(parse_style_border_color(&encoded).unwrap(), colors);
    }

    // =====================================================================
    // border-style shorthand
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_style_expands_one_to_four_values() {
        assert_eq!(
            parse_style_border_style("solid").unwrap(),
            StyleBorderStyles {
                top: BorderStyle::Solid,
                right: BorderStyle::Solid,
                bottom: BorderStyle::Solid,
                left: BorderStyle::Solid,
            }
        );
        assert_eq!(
            parse_style_border_style("solid dashed").unwrap(),
            StyleBorderStyles {
                top: BorderStyle::Solid,
                right: BorderStyle::Dashed,
                bottom: BorderStyle::Solid,
                left: BorderStyle::Dashed,
            }
        );
        assert_eq!(
            parse_style_border_style("solid dashed dotted").unwrap(),
            StyleBorderStyles {
                top: BorderStyle::Solid,
                right: BorderStyle::Dashed,
                bottom: BorderStyle::Dotted,
                left: BorderStyle::Dashed,
            }
        );
        assert_eq!(
            parse_style_border_style("solid dashed dotted double").unwrap(),
            StyleBorderStyles {
                top: BorderStyle::Solid,
                right: BorderStyle::Dashed,
                bottom: BorderStyle::Dotted,
                left: BorderStyle::Double,
            }
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_style_rejects_zero_and_more_than_four_values() {
        for input in ["", "   ", "\t\n"] {
            let err = parse_style_border_style(input).unwrap_err();
            // NOTE: the error payload here is the *trimmed* input, unlike
            // parse_border_style, which echoes the raw input back.
            assert!(
                matches!(err, CssBorderStyleParseError::InvalidStyle(s) if s == input.trim()),
                "{input:?} -> {err:?}"
            );
        }
        let too_many = "dotted ".repeat(1_000);
        let inputs: [&str; 2] = ["solid solid solid solid solid", &too_many];
        for input in inputs {
            assert!(
                parse_style_border_style(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_style_propagates_component_errors() {
        for input in [
            "bogus",
            "solid bogus",
            "solid solid bogus",
            "solid solid solid bogus",
            "solid \u{1F600}",
            "SOLID",
        ] {
            assert!(
                parse_style_border_style(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_style_round_trips_through_display() {
        for (t, r, b, l) in [
            (
                BorderStyle::Solid,
                BorderStyle::Dashed,
                BorderStyle::Dotted,
                BorderStyle::Double,
            ),
            (
                BorderStyle::None,
                BorderStyle::Hidden,
                BorderStyle::Groove,
                BorderStyle::Ridge,
            ),
            (
                BorderStyle::Inset,
                BorderStyle::Outset,
                BorderStyle::None,
                BorderStyle::Solid,
            ),
        ] {
            let expected = StyleBorderStyles {
                top: t,
                right: r,
                bottom: b,
                left: l,
            };
            let encoded = format!("{t} {r} {b} {l}");
            assert_eq!(
                parse_style_border_style(&encoded).unwrap(),
                expected,
                "{encoded} did not round-trip"
            );
        }
    }

    // =====================================================================
    // border-width shorthand
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_expands_one_to_four_values() {
        assert_eq!(
            parse_style_border_width("1px").unwrap(),
            StyleBorderWidths {
                top: PixelValue::px(1.0),
                right: PixelValue::px(1.0),
                bottom: PixelValue::px(1.0),
                left: PixelValue::px(1.0),
            }
        );
        assert_eq!(
            parse_style_border_width("1px 2px").unwrap(),
            StyleBorderWidths {
                top: PixelValue::px(1.0),
                right: PixelValue::px(2.0),
                bottom: PixelValue::px(1.0),
                left: PixelValue::px(2.0),
            }
        );
        assert_eq!(
            parse_style_border_width("1px 2px 3px").unwrap(),
            StyleBorderWidths {
                top: PixelValue::px(1.0),
                right: PixelValue::px(2.0),
                bottom: PixelValue::px(3.0),
                left: PixelValue::px(2.0),
            }
        );
        assert_eq!(
            parse_style_border_width("1px 2em 3pt 4%").unwrap(),
            StyleBorderWidths {
                top: PixelValue::px(1.0),
                right: PixelValue::em(2.0),
                bottom: PixelValue::pt(3.0),
                left: PixelValue::percent(4.0),
            }
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_rejects_zero_and_more_than_four_values() {
        for input in ["", "   ", "\t\n"] {
            let err = parse_style_border_width(input).unwrap_err();
            assert!(
                matches!(err, CssPixelValueParseError::InvalidPixelValue(s) if s == input.trim()),
                "{input:?} -> {err:?}"
            );
        }
        let too_many = "1px ".repeat(1_000);
        let inputs: [&str; 2] = ["1px 1px 1px 1px 1px", &too_many];
        for input in inputs {
            assert!(
                parse_style_border_width(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_rejects_the_thin_medium_thick_keywords() {
        // KNOWN DIVERGENCE: the longhands (parse_border_top_width) go through
        // parse_border_width_value and DO accept thin/medium/thick, but this
        // shorthand calls parse_pixel_value directly, so `border-width: thin`
        // is rejected. Worse, "thin" ends with the "in" (inch) suffix, so it is
        // reported as a broken *inches* value rather than an unknown keyword.
        let err = parse_style_border_width("thin").unwrap_err();
        assert!(
            matches!(err, CssPixelValueParseError::ValueParseErr(_, s) if s == "th"),
            "expected `thin` to be misread as inches, got {err:?}"
        );
        assert!(parse_style_border_width("medium").is_err());
        assert!(parse_style_border_width("thick").is_err());
        assert!(parse_style_border_width("thin thick").is_err());

        // ...while the longhand happily accepts all three:
        assert_eq!(
            parse_border_top_width("thin").unwrap().inner,
            THIN_BORDER_THICKNESS
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_propagates_component_errors() {
        for input in [
            "abc",
            "1px abc",
            "1px 2px abc",
            "1px 2px 3px abc",
            "1px \u{1F600}",
            "1PX",
        ] {
            assert!(
                parse_style_border_width(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_hostile_numbers_stay_finite() {
        let widths = parse_style_border_width("NaN inf -inf 1e999").unwrap();
        for w in [widths.top, widths.right, widths.bottom, widths.left] {
            assert!(
                w.number.get().is_finite(),
                "hostile width did not saturate: {w:?}"
            );
        }
        assert_eq!(widths.top, PixelValue::px(0.0)); // NaN -> 0
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_border_width_round_trips_through_display() {
        let widths = StyleBorderWidths {
            top: PixelValue::px(1.5),
            right: PixelValue::em(2.0),
            bottom: PixelValue::pt(3.25),
            left: PixelValue::percent(50.0),
        };
        let encoded = format!(
            "{} {} {} {}",
            widths.top, widths.right, widths.bottom, widths.left
        );
        assert_eq!(parse_style_border_width(&encoded).unwrap(), widths);
    }

    // =====================================================================
    // Error types: to_contained / to_shared / From / Display
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn css_border_style_parse_error_round_trips_owned_and_shared() {
        let huge = "x".repeat(10_000);
        let payloads: [&str; 7] = ["", " ", "bogus", "\u{1F600}", "s\u{0301}", "\u{0}", &huge];
        for payload in payloads {
            let shared = CssBorderStyleParseError::InvalidStyle(payload);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "{payload:?} did not round-trip");
            assert!(
                matches!(&owned, CssBorderStyleParseErrorOwned::InvalidStyle(s) if s.as_str() == payload)
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_border_style_parse_error_from_a_real_parse_failure_round_trips() {
        let err = parse_border_style("\u{1F600}bogus").unwrap_err();
        let owned = err.to_contained();
        assert_eq!(owned.to_shared(), err);
        // Debug is routed through Display, so both must mention the input.
        assert!(format!("{err}").contains("bogus"));
        assert!(format!("{err:?}").contains("bogus"));
        assert!(!format!("{owned:?}").is_empty());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_border_side_parse_error_round_trips_for_every_variant() {
        let variants = [
            CssBorderSideParseError::InvalidDeclaration(""),
            CssBorderSideParseError::InvalidDeclaration("1px 2px solid"),
            CssBorderSideParseError::InvalidDeclaration("\u{1F600}"),
            CssBorderSideParseError::Width(CssPixelValueParseError::EmptyString),
            CssBorderSideParseError::Width(CssPixelValueParseError::InvalidPixelValue("zz")),
            CssBorderSideParseError::Style(CssBorderStyleParseError::InvalidStyle("zz")),
            CssBorderSideParseError::Style(CssBorderStyleParseError::InvalidStyle("")),
            CssBorderSideParseError::Color(CssColorParseError::InvalidColor("zz")),
            CssBorderSideParseError::Color(CssColorParseError::EmptyInput),
            CssBorderSideParseError::Color(CssColorParseError::InvalidColorComponent(u8::MAX)),
        ];
        for shared in variants {
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "{shared:?} did not round-trip");
            assert!(!format!("{shared}").is_empty());
            assert!(!format!("{owned:?}").is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_border_side_parse_error_from_conversions_pick_the_right_variant() {
        let width: CssBorderSideParseError<'_> =
            CssPixelValueParseError::InvalidPixelValue("zz").into();
        assert!(matches!(width, CssBorderSideParseError::Width(_)));

        let style: CssBorderSideParseError<'_> =
            CssBorderStyleParseError::InvalidStyle("zz").into();
        assert!(matches!(style, CssBorderSideParseError::Style(_)));

        let color: CssBorderSideParseError<'_> = CssColorParseError::InvalidColor("zz").into();
        assert!(matches!(color, CssBorderSideParseError::Color(_)));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_border_parse_error_owned_newtype_wraps_the_side_error() {
        let owned = CssBorderSideParseError::InvalidDeclaration("bogus").to_contained();
        let wrapped = CssBorderParseErrorOwned::from(owned.clone());
        assert_eq!(wrapped.inner, owned);
        assert_eq!(wrapped.inner.to_shared().to_contained(), owned);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_display_mentions_the_offending_input() {
        let err = parse_border_side("1px bogus red").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("1px bogus red"), "unhelpful message: {msg}");

        let err = parse_border_style("bogus").unwrap_err();
        assert!(format!("{err}").contains("bogus"));

        let err = parse_border_top_width("bogus").unwrap_err();
        assert!(format!("{err}").contains("bogus"));
    }
}
