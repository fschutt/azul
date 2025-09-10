//! Border-related CSS properties

use alloc::string::String;
use core::fmt;

use crate::props::{
    basic::{color::ColorU, value::PixelValue},
    formatter::FormatAsCssValue,
};

/// CSS border-style values
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

impl fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BorderStyle::*;
        match self {
            None => write!(f, "none"),
            Solid => write!(f, "solid"),
            Double => write!(f, "double"),
            Dotted => write!(f, "dotted"),
            Dashed => write!(f, "dashed"),
            Hidden => write!(f, "hidden"),
            Groove => write!(f, "groove"),
            Ridge => write!(f, "ridge"),
            Inset => write!(f, "inset"),
            Outset => write!(f, "outset"),
        }
    }
}

impl BorderStyle {
    pub fn normalize_border(self) -> Option<BorderStyleNoNone> {
        match self {
            BorderStyle::None => None,
            BorderStyle::Solid => Some(BorderStyleNoNone::Solid),
            BorderStyle::Double => Some(BorderStyleNoNone::Double),
            BorderStyle::Dotted => Some(BorderStyleNoNone::Dotted),
            BorderStyle::Dashed => Some(BorderStyleNoNone::Dashed),
            BorderStyle::Hidden => Some(BorderStyleNoNone::Hidden),
            BorderStyle::Groove => Some(BorderStyleNoNone::Groove),
            BorderStyle::Ridge => Some(BorderStyleNoNone::Ridge),
            BorderStyle::Inset => Some(BorderStyleNoNone::Inset),
            BorderStyle::Outset => Some(BorderStyleNoNone::Outset),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum BorderStyleNoNone {
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
        BorderStyle::Solid
    }
}

/// Border details - normal or nine-patch
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum BorderDetails {
    Normal(NormalBorder),
    NinePatch(NinePatchBorder),
}

/// Normal border (no image/nine-patch)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct NormalBorder {
    pub left: BorderSide,
    pub right: BorderSide,
    pub top: BorderSide,
    pub bottom: BorderSide,
    pub radius: Option<(
        StyleBorderTopLeftRadius,
        StyleBorderTopRightRadius,
        StyleBorderBottomLeftRadius,
        StyleBorderBottomRightRadius,
    )>,
}

/// Border side definition
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct BorderSide {
    pub color: ColorU,
    pub style: BorderStyle,
}

/// Nine-patch border (not yet implemented)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct NinePatchBorder {
    // not implemented or parse-able yet, so no fields!
}

/// Complete border side definition
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

// Macro for creating debug/display implementations for wrapper types
macro_rules! derive_debug_zero {
    ($struct:ident) => {
        impl fmt::Debug for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:?}", self.inner)
            }
        }
    };
}

macro_rules! derive_display_zero {
    ($struct:ident) => {
        impl fmt::Display for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}", self.inner)
            }
        }
    };
}

/// CSS border-top-style property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopStyle {
    pub inner: BorderStyle,
}

/// CSS border-left-style property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftStyle {
    pub inner: BorderStyle,
}

/// CSS border-right-style property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightStyle {
    pub inner: BorderStyle,
}

/// CSS border-bottom-style property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderTopStyle);
derive_debug_zero!(StyleBorderLeftStyle);
derive_debug_zero!(StyleBorderBottomStyle);
derive_debug_zero!(StyleBorderRightStyle);
derive_display_zero!(StyleBorderTopStyle);
derive_display_zero!(StyleBorderLeftStyle);
derive_display_zero!(StyleBorderBottomStyle);
derive_display_zero!(StyleBorderRightStyle);

/// CSS border-top-color property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopColor {
    pub inner: ColorU,
}

/// CSS border-left-color property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftColor {
    pub inner: ColorU,
}

/// CSS border-right-color property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightColor {
    pub inner: ColorU,
}

/// CSS border-bottom-color property
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomColor {
    pub inner: ColorU,
}

impl StyleBorderTopColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl StyleBorderLeftColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl StyleBorderRightColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl StyleBorderBottomColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

derive_debug_zero!(StyleBorderTopColor);
derive_debug_zero!(StyleBorderLeftColor);
derive_debug_zero!(StyleBorderRightColor);
derive_debug_zero!(StyleBorderBottomColor);
derive_display_zero!(StyleBorderTopColor);
derive_display_zero!(StyleBorderLeftColor);
derive_display_zero!(StyleBorderRightColor);
derive_display_zero!(StyleBorderBottomColor);

// Forward declarations for border radius types (defined in border_radius.rs)
pub use crate::props::style::border_radius::{
    StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleBorderTopLeftRadius,
    StyleBorderTopRightRadius,
};

#[cfg(feature = "parser")]
use crate::parser_ext::{parse_style_border, parse_style_border_style};
