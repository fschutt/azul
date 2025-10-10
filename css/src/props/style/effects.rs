//! CSS properties for visual effects like opacity, blending, and cursor style.

use alloc::string::{String, ToString};
use core::fmt;
use crate::{
    parser::{impl_debug_as_display, impl_display, impl_from},
    props::{
        formatter::PrintAsCssValue,
        basic::value::{PercentageValue, PercentageParseError},
    },
};

#[cfg(feature = "parser")]
use crate::{
    parser::{
        InvalidValueErr, InvalidValueErrOwned,
        parse_percentage_value,
    },
    props::macros::impl_percentage_value
};

// -- Opacity --

/// Represents an `opacity` attribute, a value from 0.0 to 1.0.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleOpacity {
    pub inner: PercentageValue,
}

impl Default for StyleOpacity {
    fn default() -> Self {
        StyleOpacity { inner: PercentageValue::const_new(100) }
    }
}

impl PrintAsCssValue for StyleOpacity {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner.normalized())
    }
}

#[cfg(feature = "parser")]
impl_percentage_value!(StyleOpacity);

// -- Mix Blend Mode --

/// Represents a `mix-blend-mode` attribute, which determines how an element's
/// content should blend with the content of the element's parent.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleMixBlendMode {
    Normal, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge,
    ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue,
    Saturation, Color, Luminosity,
}

impl Default for StyleMixBlendMode {
    fn default() -> StyleMixBlendMode {
        StyleMixBlendMode::Normal
    }
}

impl fmt::Display for StyleMixBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Normal => "normal", Self::Multiply => "multiply", Self::Screen => "screen",
            Self::Overlay => "overlay", Self::Darken => "darken", Self::Lighten => "lighten",
            Self::ColorDodge => "color-dodge", Self::ColorBurn => "color-burn",
            Self::HardLight => "hard-light", Self::SoftLight => "soft-light",
            Self::Difference => "difference", Self::Exclusion => "exclusion",
            Self::Hue => "hue", Self::Saturation => "saturation",
            Self::Color => "color", Self::Luminosity => "luminosity",
        })
    }
}

impl PrintAsCssValue for StyleMixBlendMode {
    fn print_as_css_value(&self) -> String {
        self.to_string()
    }
}

// -- Cursor --

/// Represents a `cursor` attribute, defining the mouse cursor to be displayed
/// when pointing over an element.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleCursor {
    Alias, AllScroll, Cell, ColResize, ContextMenu, Copy, Crosshair, Default,
    EResize, EwResize, Grab, Grabbing, Help, Move, NResize, NsResize, NeswResize,
    NwseResize, Pointer, Progress, RowResize, SResize, SeResize, Text, Unset,
    VerticalText, WResize, Wait, ZoomIn, ZoomOut,
}

impl Default for StyleCursor {
    fn default() -> StyleCursor {
        StyleCursor::Default
    }
}

impl PrintAsCssValue for StyleCursor {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Alias => "alias", Self::AllScroll => "all-scroll", Self::Cell => "cell",
            Self::ColResize => "col-resize", Self::ContextMenu => "context-menu",
            Self::Copy => "copy", Self::Crosshair => "crosshair", Self::Default => "default",
            Self::EResize => "e-resize", Self::EwResize => "ew-resize", Self::Grab => "grab",
            Self::Grabbing => "grabbing", Self::Help => "help", Self::Move => "move",
            Self::NResize => "n-resize", Self::NsResize => "ns-resize",
            Self::NeswResize => "nesw-resize", Self::NwseResize => "nwse-resize",
            Self::Pointer => "pointer", Self::Progress => "progress",
            Self::RowResize => "row-resize", Self::SResize => "s-resize",
            Self::SeResize => "se-resize", Self::Text => "text", Self::Unset => "unset",
            Self::VerticalText => "vertical-text", Self::WResize => "w-resize",
            Self::Wait => "wait", Self::ZoomIn => "zoom-in", Self::ZoomOut => "zoom-out",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parsers {
    use super::*;
    use crate::parser::{InvalidValueErr, InvalidValueErrOwned};

    // -- Opacity Parser --

    #[derive(Clone, PartialEq)]
    pub enum OpacityParseError<'a> {
        ParsePercentage(PercentageParseError, &'a str),
    }
    impl_debug_as_display!(OpacityParseError<'a>);
    impl_display! { OpacityParseError<'a>, {
        ParsePercentage(e, s) => format!("Invalid opacity value \"{}\": {}", s, e),
    }}

    #[derive(Debug, Clone, PartialEq)]
    pub enum OpacityParseErrorOwned {
        ParsePercentage(PercentageParseError, String),
    }

    impl<'a> OpacityParseError<'a> {
        pub fn to_contained(&self) -> OpacityParseErrorOwned {
            match self {
                Self::ParsePercentage(err, s) => OpacityParseErrorOwned::ParsePercentage(err.clone(), s.to_string()),
            }
        }
    }

    impl OpacityParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> OpacityParseError<'a> {
            match self {
                Self::ParsePercentage(err, s) => OpacityParseError::ParsePercentage(err.clone(), s.as_str()),
            }
        }
    }

    pub fn parse_style_opacity<'a>(input: &'a str) -> Result<StyleOpacity, OpacityParseError<'a>> {
        parse_percentage_value(input)
            .map(|val| StyleOpacity { inner: val })
            .map_err(|e| OpacityParseError::ParsePercentage(e, input))
    }

    // -- Mix Blend Mode Parser --

    #[derive(Clone, PartialEq)]
    pub enum MixBlendModeParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(MixBlendModeParseError<'a>);
    impl_display! { MixBlendModeParseError<'a>, {
        InvalidValue(e) => format!("Invalid mix-blend-mode value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, MixBlendModeParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq)]
    pub enum MixBlendModeParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl<'a> MixBlendModeParseError<'a> {
        pub fn to_contained(&self) -> MixBlendModeParseErrorOwned {
            match self {
                Self::InvalidValue(e) => MixBlendModeParseErrorOwned::InvalidValue(e.to_contained()),
            }
        }
    }

    impl MixBlendModeParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> MixBlendModeParseError<'a> {
            match self {
                Self::InvalidValue(e) => MixBlendModeParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    pub fn parse_style_mix_blend_mode<'a>(input: &'a str) -> Result<StyleMixBlendMode, MixBlendModeParseError<'a>> {
        let input = input.trim();
        match input {
            "normal" => Ok(StyleMixBlendMode::Normal), "multiply" => Ok(StyleMixBlendMode::Multiply),
            "screen" => Ok(StyleMixBlendMode::Screen), "overlay" => Ok(StyleMixBlendMode::Overlay),
            "darken" => Ok(StyleMixBlendMode::Darken), "lighten" => Ok(StyleMixBlendMode::Lighten),
            "color-dodge" => Ok(StyleMixBlendMode::ColorDodge), "color-burn" => Ok(StyleMixBlendMode::ColorBurn),
            "hard-light" => Ok(StyleMixBlendMode::HardLight), "soft-light" => Ok(StyleMixBlendMode::SoftLight),
            "difference" => Ok(StyleMixBlendMode::Difference), "exclusion" => Ok(StyleMixBlendMode::Exclusion),
            "hue" => Ok(StyleMixBlendMode::Hue), "saturation" => Ok(StyleMixBlendMode::Saturation),
            "color" => Ok(StyleMixBlendMode::Color), "luminosity" => Ok(StyleMixBlendMode::Luminosity),
            _ => Err(InvalidValueErr(input).into()),
        }
    }

    // -- Cursor Parser --

    #[derive(Clone, PartialEq)]
    pub enum CursorParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(CursorParseError<'a>);
    impl_display! { CursorParseError<'a>, {
        InvalidValue(e) => format!("Invalid cursor value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, CursorParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq)]
    pub enum CursorParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl<'a> CursorParseError<'a> {
        pub fn to_contained(&self) -> CursorParseErrorOwned {
            match self {
                Self::InvalidValue(e) => CursorParseErrorOwned::InvalidValue(e.to_contained()),
            }
        }
    }

    impl CursorParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CursorParseError<'a> {
            match self {
                Self::InvalidValue(e) => CursorParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    pub fn parse_style_cursor<'a>(input: &'a str) -> Result<StyleCursor, CursorParseError<'a>> {
        let input = input.trim();
        match input {
            "alias" => Ok(StyleCursor::Alias), "all-scroll" => Ok(StyleCursor::AllScroll),
            "cell" => Ok(StyleCursor::Cell), "col-resize" => Ok(StyleCursor::ColResize),
            "context-menu" => Ok(StyleCursor::ContextMenu), "copy" => Ok(StyleCursor::Copy),
            "crosshair" => Ok(StyleCursor::Crosshair), "default" => Ok(StyleCursor::Default),
            "e-resize" => Ok(StyleCursor::EResize), "ew-resize" => Ok(StyleCursor::EwResize),
            "grab" => Ok(StyleCursor::Grab), "grabbing" => Ok(StyleCursor::Grabbing),
            "help" => Ok(StyleCursor::Help), "move" => Ok(StyleCursor::Move),
            "n-resize" => Ok(StyleCursor::NResize), "ns-resize" => Ok(StyleCursor::NsResize),
            "nesw-resize" => Ok(StyleCursor::NeswResize), "nwse-resize" => Ok(StyleCursor::NwseResize),
            "pointer" => Ok(StyleCursor::Pointer), "progress" => Ok(StyleCursor::Progress),
            "row-resize" => Ok(StyleCursor::RowResize), "s-resize" => Ok(StyleCursor::SResize),
            "se-resize" => Ok(StyleCursor::SeResize), "text" => Ok(StyleCursor::Text),
            "unset" => Ok(StyleCursor::Unset), "vertical-text" => Ok(StyleCursor::VerticalText),
            "w-resize" => Ok(StyleCursor::WResize), "wait" => Ok(StyleCursor::Wait),
            "zoom-in" => Ok(StyleCursor::ZoomIn), "zoom-out" => Ok(StyleCursor::ZoomOut),
            _ => Err(InvalidValueErr(input).into()),
        }
    }
}
