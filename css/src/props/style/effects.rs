//! CSS properties for visual effects like opacity, blending, and cursor style.

use alloc::string::{String, ToString};
use core::fmt;

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::{InvalidValueErr, InvalidValueErrOwned},
    length::parse_percentage_value,
};
use crate::props::{
    basic::length::{PercentageParseError, PercentageValue},
    formatter::PrintAsCssValue,
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
        StyleOpacity {
            inner: PercentageValue::const_new(100),
        }
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

/// Represents a `visibility` attribute, controlling element visibility.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVisibility {
    Visible,
    Hidden,
    Collapse,
}

impl Default for StyleVisibility {
    fn default() -> StyleVisibility {
        StyleVisibility::Visible
    }
}

impl PrintAsCssValue for StyleVisibility {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Collapse => "collapse",
        })
    }
}

// -- Mix Blend Mode --

/// Represents a `mix-blend-mode` attribute, which determines how an element's
/// content should blend with the content of the element's parent.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleMixBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for StyleMixBlendMode {
    fn default() -> StyleMixBlendMode {
        StyleMixBlendMode::Normal
    }
}

impl fmt::Display for StyleMixBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Normal => "normal",
                Self::Multiply => "multiply",
                Self::Screen => "screen",
                Self::Overlay => "overlay",
                Self::Darken => "darken",
                Self::Lighten => "lighten",
                Self::ColorDodge => "color-dodge",
                Self::ColorBurn => "color-burn",
                Self::HardLight => "hard-light",
                Self::SoftLight => "soft-light",
                Self::Difference => "difference",
                Self::Exclusion => "exclusion",
                Self::Hue => "hue",
                Self::Saturation => "saturation",
                Self::Color => "color",
                Self::Luminosity => "luminosity",
            }
        )
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
    Alias,
    AllScroll,
    Cell,
    ColResize,
    ContextMenu,
    Copy,
    Crosshair,
    Default,
    EResize,
    EwResize,
    Grab,
    Grabbing,
    Help,
    Move,
    NResize,
    NsResize,
    NeswResize,
    NwseResize,
    Pointer,
    Progress,
    RowResize,
    SResize,
    SeResize,
    Text,
    Unset,
    VerticalText,
    WResize,
    Wait,
    ZoomIn,
    ZoomOut,
}

impl Default for StyleCursor {
    fn default() -> StyleCursor {
        StyleCursor::Default
    }
}

impl PrintAsCssValue for StyleCursor {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Alias => "alias",
            Self::AllScroll => "all-scroll",
            Self::Cell => "cell",
            Self::ColResize => "col-resize",
            Self::ContextMenu => "context-menu",
            Self::Copy => "copy",
            Self::Crosshair => "crosshair",
            Self::Default => "default",
            Self::EResize => "e-resize",
            Self::EwResize => "ew-resize",
            Self::Grab => "grab",
            Self::Grabbing => "grabbing",
            Self::Help => "help",
            Self::Move => "move",
            Self::NResize => "n-resize",
            Self::NsResize => "ns-resize",
            Self::NeswResize => "nesw-resize",
            Self::NwseResize => "nwse-resize",
            Self::Pointer => "pointer",
            Self::Progress => "progress",
            Self::RowResize => "row-resize",
            Self::SResize => "s-resize",
            Self::SeResize => "se-resize",
            Self::Text => "text",
            Self::Unset => "unset",
            Self::VerticalText => "vertical-text",
            Self::WResize => "w-resize",
            Self::Wait => "wait",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parsers {
    use super::*;
    use crate::corety::AzString;
    use crate::props::basic::error::{InvalidValueErr, InvalidValueErrOwned};

    // -- Opacity Parser --

    #[derive(Clone, PartialEq)]
    pub enum OpacityParseError<'a> {
        ParsePercentage(PercentageParseError, &'a str),
        OutOfRange(&'a str),
    }
    impl_debug_as_display!(OpacityParseError<'a>);
    impl_display! { OpacityParseError<'a>, {
        ParsePercentage(e, s) => format!("Invalid opacity value \"{}\": {}", s, e),
        OutOfRange(s) => format!("Invalid opacity value \"{}\": must be between 0 and 1", s),
    }}

    /// Wrapper for PercentageParseError with input string.
    #[derive(Debug, Clone, PartialEq)]
    #[repr(C)]
    pub struct PercentageParseErrorWithInput {
        pub error: PercentageParseError,
        pub input: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum OpacityParseErrorOwned {
        ParsePercentage(PercentageParseErrorWithInput),
        OutOfRange(AzString),
    }

    impl<'a> OpacityParseError<'a> {
        pub fn to_contained(&self) -> OpacityParseErrorOwned {
            match self {
                Self::ParsePercentage(err, s) => {
                    OpacityParseErrorOwned::ParsePercentage(PercentageParseErrorWithInput { error: err.clone(), input: s.to_string() })
                }
                Self::OutOfRange(s) => OpacityParseErrorOwned::OutOfRange(s.to_string().into()),
            }
        }
    }

    impl OpacityParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> OpacityParseError<'a> {
            match self {
                Self::ParsePercentage(e) => {
                    OpacityParseError::ParsePercentage(e.error.clone(), e.input.as_str())
                }
                Self::OutOfRange(s) => OpacityParseError::OutOfRange(s.as_str()),
            }
        }
    }

    pub fn parse_style_opacity<'a>(input: &'a str) -> Result<StyleOpacity, OpacityParseError<'a>> {
        let val = parse_percentage_value(input)
            .map_err(|e| OpacityParseError::ParsePercentage(e, input))?;

        let normalized = val.normalized();
        if normalized < 0.0 || normalized > 1.0 {
            return Err(OpacityParseError::OutOfRange(input));
        }

        Ok(StyleOpacity { inner: val })
    }

    // -- Visibility Parser --

    #[derive(Clone, PartialEq)]
    pub enum StyleVisibilityParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(StyleVisibilityParseError<'a>);
    impl_display! { StyleVisibilityParseError<'a>, {
        InvalidValue(e) => format!("Invalid visibility value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, StyleVisibilityParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum StyleVisibilityParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl<'a> StyleVisibilityParseError<'a> {
        pub fn to_contained(&self) -> StyleVisibilityParseErrorOwned {
            match self {
                Self::InvalidValue(e) => {
                    StyleVisibilityParseErrorOwned::InvalidValue(e.to_contained())
                }
            }
        }
    }

    impl StyleVisibilityParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> StyleVisibilityParseError<'a> {
            match self {
                Self::InvalidValue(e) => StyleVisibilityParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    pub fn parse_style_visibility<'a>(
        input: &'a str,
    ) -> Result<StyleVisibility, StyleVisibilityParseError<'a>> {
        let input = input.trim();
        match input {
            "visible" => Ok(StyleVisibility::Visible),
            "hidden" => Ok(StyleVisibility::Hidden),
            "collapse" => Ok(StyleVisibility::Collapse),
            _ => Err(InvalidValueErr(input).into()),
        }
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
    #[repr(C, u8)]
    pub enum MixBlendModeParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl<'a> MixBlendModeParseError<'a> {
        pub fn to_contained(&self) -> MixBlendModeParseErrorOwned {
            match self {
                Self::InvalidValue(e) => {
                    MixBlendModeParseErrorOwned::InvalidValue(e.to_contained())
                }
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

    pub fn parse_style_mix_blend_mode<'a>(
        input: &'a str,
    ) -> Result<StyleMixBlendMode, MixBlendModeParseError<'a>> {
        let input = input.trim();
        match input {
            "normal" => Ok(StyleMixBlendMode::Normal),
            "multiply" => Ok(StyleMixBlendMode::Multiply),
            "screen" => Ok(StyleMixBlendMode::Screen),
            "overlay" => Ok(StyleMixBlendMode::Overlay),
            "darken" => Ok(StyleMixBlendMode::Darken),
            "lighten" => Ok(StyleMixBlendMode::Lighten),
            "color-dodge" => Ok(StyleMixBlendMode::ColorDodge),
            "color-burn" => Ok(StyleMixBlendMode::ColorBurn),
            "hard-light" => Ok(StyleMixBlendMode::HardLight),
            "soft-light" => Ok(StyleMixBlendMode::SoftLight),
            "difference" => Ok(StyleMixBlendMode::Difference),
            "exclusion" => Ok(StyleMixBlendMode::Exclusion),
            "hue" => Ok(StyleMixBlendMode::Hue),
            "saturation" => Ok(StyleMixBlendMode::Saturation),
            "color" => Ok(StyleMixBlendMode::Color),
            "luminosity" => Ok(StyleMixBlendMode::Luminosity),
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
    #[repr(C, u8)]
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
            "alias" => Ok(StyleCursor::Alias),
            "all-scroll" => Ok(StyleCursor::AllScroll),
            "cell" => Ok(StyleCursor::Cell),
            "col-resize" => Ok(StyleCursor::ColResize),
            "context-menu" => Ok(StyleCursor::ContextMenu),
            "copy" => Ok(StyleCursor::Copy),
            "crosshair" => Ok(StyleCursor::Crosshair),
            "default" => Ok(StyleCursor::Default),
            "e-resize" => Ok(StyleCursor::EResize),
            "ew-resize" => Ok(StyleCursor::EwResize),
            "grab" => Ok(StyleCursor::Grab),
            "grabbing" => Ok(StyleCursor::Grabbing),
            "help" => Ok(StyleCursor::Help),
            "move" => Ok(StyleCursor::Move),
            "n-resize" => Ok(StyleCursor::NResize),
            "ns-resize" => Ok(StyleCursor::NsResize),
            "nesw-resize" => Ok(StyleCursor::NeswResize),
            "nwse-resize" => Ok(StyleCursor::NwseResize),
            "pointer" => Ok(StyleCursor::Pointer),
            "progress" => Ok(StyleCursor::Progress),
            "row-resize" => Ok(StyleCursor::RowResize),
            "s-resize" => Ok(StyleCursor::SResize),
            "se-resize" => Ok(StyleCursor::SeResize),
            "text" => Ok(StyleCursor::Text),
            "unset" => Ok(StyleCursor::Unset),
            "vertical-text" => Ok(StyleCursor::VerticalText),
            "w-resize" => Ok(StyleCursor::WResize),
            "wait" => Ok(StyleCursor::Wait),
            "zoom-in" => Ok(StyleCursor::ZoomIn),
            "zoom-out" => Ok(StyleCursor::ZoomOut),
            _ => Err(InvalidValueErr(input).into()),
        }
    }
}

#[cfg(feature = "parser")]
pub use self::parsers::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opacity() {
        assert_eq!(parse_style_opacity("0.5").unwrap().inner.normalized(), 0.5);
        assert_eq!(parse_style_opacity("1").unwrap().inner.normalized(), 1.0);
        assert_eq!(parse_style_opacity("50%").unwrap().inner.normalized(), 0.5);
        assert_eq!(parse_style_opacity("0").unwrap().inner.normalized(), 0.0);
        assert_eq!(
            parse_style_opacity("  75%  ").unwrap().inner.normalized(),
            0.75
        );
        assert!(parse_style_opacity("1.1").is_err());
        assert!(parse_style_opacity("-0.1").is_err());
        assert!(parse_style_opacity("auto").is_err());
    }

    #[test]
    fn test_parse_mix_blend_mode() {
        assert_eq!(
            parse_style_mix_blend_mode("multiply").unwrap(),
            StyleMixBlendMode::Multiply
        );
        assert_eq!(
            parse_style_mix_blend_mode("screen").unwrap(),
            StyleMixBlendMode::Screen
        );
        assert_eq!(
            parse_style_mix_blend_mode("color-dodge").unwrap(),
            StyleMixBlendMode::ColorDodge
        );
        assert!(parse_style_mix_blend_mode("mix").is_err());
    }

    #[test]
    fn test_parse_visibility() {
        assert_eq!(
            parse_style_visibility("visible").unwrap(),
            StyleVisibility::Visible
        );
        assert_eq!(
            parse_style_visibility("hidden").unwrap(),
            StyleVisibility::Hidden
        );
        assert_eq!(
            parse_style_visibility("collapse").unwrap(),
            StyleVisibility::Collapse
        );
        assert_eq!(
            parse_style_visibility("  visible  ").unwrap(),
            StyleVisibility::Visible
        );
        assert!(parse_style_visibility("none").is_err());
        assert!(parse_style_visibility("show").is_err());
    }

    #[test]
    fn test_parse_cursor() {
        assert_eq!(parse_style_cursor("pointer").unwrap(), StyleCursor::Pointer);
        assert_eq!(parse_style_cursor("wait").unwrap(), StyleCursor::Wait);
        assert_eq!(
            parse_style_cursor("col-resize").unwrap(),
            StyleCursor::ColResize
        );
        assert_eq!(parse_style_cursor("  text  ").unwrap(), StyleCursor::Text);
        assert!(parse_style_cursor("hand").is_err()); // "hand" is a legacy IE value
    }
}
