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
        pub input: AzString,
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
                    OpacityParseErrorOwned::ParsePercentage(PercentageParseErrorWithInput { error: err.clone(), input: s.to_string().into() })
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

// -- StyleObjectFit --

/// CSS object-fit property: how replaced element content is fitted to its box.
/// CSS Images Level 3 §5.5
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl Default for StyleObjectFit {
    fn default() -> Self {
        StyleObjectFit::Fill
    }
}

crate::impl_option!(StyleObjectFit, OptionStyleObjectFit, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleObjectFit {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleObjectFit::Fill => "fill",
            StyleObjectFit::Contain => "contain",
            StyleObjectFit::Cover => "cover",
            StyleObjectFit::None => "none",
            StyleObjectFit::ScaleDown => "scale-down",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleObjectFitParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
crate::impl_debug_as_display!(StyleObjectFitParseError<'a>);

#[cfg(feature = "parser")]
crate::impl_display! { StyleObjectFitParseError<'a>, {
    InvalidValue(val) => format!("Invalid object-fit value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleObjectFitParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleObjectFitParseError<'a> {
    pub fn to_contained(&self) -> StyleObjectFitParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleObjectFitParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleObjectFitParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleObjectFitParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleObjectFitParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_object_fit<'a>(
    input: &'a str,
) -> Result<StyleObjectFit, StyleObjectFitParseError<'a>> {
    let input = input.trim();
    match input {
        "fill" => Ok(StyleObjectFit::Fill),
        "contain" => Ok(StyleObjectFit::Contain),
        "cover" => Ok(StyleObjectFit::Cover),
        "none" => Ok(StyleObjectFit::None),
        "scale-down" => Ok(StyleObjectFit::ScaleDown),
        _ => Err(StyleObjectFitParseError::InvalidValue(input)),
    }
}

// -- StyleTextOrientation --

/// CSS text-orientation property for vertical writing modes.
/// CSS Writing Modes Level 4 §5.1
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextOrientation {
    Mixed,
    Upright,
    Sideways,
}

impl Default for StyleTextOrientation {
    fn default() -> Self {
        StyleTextOrientation::Mixed
    }
}

crate::impl_option!(StyleTextOrientation, OptionStyleTextOrientation, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleTextOrientation {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleTextOrientation::Mixed => "mixed",
            StyleTextOrientation::Upright => "upright",
            StyleTextOrientation::Sideways => "sideways",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleTextOrientationParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
crate::impl_debug_as_display!(StyleTextOrientationParseError<'a>);

#[cfg(feature = "parser")]
crate::impl_display! { StyleTextOrientationParseError<'a>, {
    InvalidValue(val) => format!("Invalid text-orientation value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleTextOrientationParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleTextOrientationParseError<'a> {
    pub fn to_contained(&self) -> StyleTextOrientationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleTextOrientationParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextOrientationParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleTextOrientationParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleTextOrientationParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_orientation<'a>(
    input: &'a str,
) -> Result<StyleTextOrientation, StyleTextOrientationParseError<'a>> {
    let input = input.trim();
    match input {
        "mixed" => Ok(StyleTextOrientation::Mixed),
        "upright" => Ok(StyleTextOrientation::Upright),
        "sideways" => Ok(StyleTextOrientation::Sideways),
        _ => Err(StyleTextOrientationParseError::InvalidValue(input)),
    }
}

// -- StyleObjectPosition --

/// CSS object-position property: position of replaced element content within its box.
/// CSS Images Level 3 §5.6 — default: `50% 50%` (centered)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleObjectPosition {
    pub horizontal: crate::props::style::background::BackgroundPositionHorizontal,
    pub vertical: crate::props::style::background::BackgroundPositionVertical,
}

impl Default for StyleObjectPosition {
    fn default() -> Self {
        use crate::props::basic::pixel::PixelValue;
        Self {
            horizontal: crate::props::style::background::BackgroundPositionHorizontal::Exact(
                PixelValue::percent(50.0),
            ),
            vertical: crate::props::style::background::BackgroundPositionVertical::Exact(
                PixelValue::percent(50.0),
            ),
        }
    }
}

crate::impl_option!(StyleObjectPosition, OptionStyleObjectPosition, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleObjectPosition {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {}",
            self.horizontal.print_as_css_value(),
            self.vertical.print_as_css_value()
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleObjectPositionParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
crate::impl_debug_as_display!(StyleObjectPositionParseError<'a>);

#[cfg(feature = "parser")]
crate::impl_display! { StyleObjectPositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid object-position value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleObjectPositionParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleObjectPositionParseError<'a> {
    pub fn to_contained(&self) -> StyleObjectPositionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleObjectPositionParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleObjectPositionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleObjectPositionParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleObjectPositionParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parse object-position: accepts keyword pairs or percentage/length values.
/// Examples: "center", "left top", "50% 50%", "10px 20px"
#[cfg(feature = "parser")]
pub fn parse_style_object_position<'a>(
    input: &'a str,
) -> Result<StyleObjectPosition, StyleObjectPositionParseError<'a>> {
    use crate::props::style::background::{
        BackgroundPositionHorizontal, BackgroundPositionVertical,
    };
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    let (h, v) = match parts.len() {
        1 => {
            let val = parts[0];
            match val {
                "center" => (BackgroundPositionHorizontal::Center, BackgroundPositionVertical::Center),
                "left" => (BackgroundPositionHorizontal::Left, BackgroundPositionVertical::Center),
                "right" => (BackgroundPositionHorizontal::Right, BackgroundPositionVertical::Center),
                "top" => (BackgroundPositionHorizontal::Center, BackgroundPositionVertical::Top),
                "bottom" => (BackgroundPositionHorizontal::Center, BackgroundPositionVertical::Bottom),
                _ => {
                    let px = parse_pixel_value(val)
                        .map_err(|_| StyleObjectPositionParseError::InvalidValue(input))?;
                    (BackgroundPositionHorizontal::Exact(px), BackgroundPositionVertical::Exact(px))
                }
            }
        }
        2 => {
            let h = match parts[0] {
                "left" => BackgroundPositionHorizontal::Left,
                "center" => BackgroundPositionHorizontal::Center,
                "right" => BackgroundPositionHorizontal::Right,
                other => {
                    let px = parse_pixel_value(other)
                        .map_err(|_| StyleObjectPositionParseError::InvalidValue(input))?;
                    BackgroundPositionHorizontal::Exact(px)
                }
            };
            let v = match parts[1] {
                "top" => BackgroundPositionVertical::Top,
                "center" => BackgroundPositionVertical::Center,
                "bottom" => BackgroundPositionVertical::Bottom,
                other => {
                    let px = parse_pixel_value(other)
                        .map_err(|_| StyleObjectPositionParseError::InvalidValue(input))?;
                    BackgroundPositionVertical::Exact(px)
                }
            };
            (h, v)
        }
        _ => return Err(StyleObjectPositionParseError::InvalidValue(input)),
    };

    Ok(StyleObjectPosition { horizontal: h, vertical: v })
}

// -- StyleAspectRatio --

/// Width/height ratio stored as fixed-point (value * 1000).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AspectRatioValue {
    pub width: u32,
    pub height: u32,
}

/// CSS aspect-ratio property: preferred aspect ratio for the box.
/// CSS Box Sizing Level 4 §6 — values: `auto | <ratio>` (initial: `auto`)
///
/// Stored as width/height ratio. Auto means no preferred ratio.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleAspectRatio {
    /// No preferred aspect ratio
    Auto,
    /// Fixed ratio (width / height), stored as fixed-point (value * 1000)
    Ratio(AspectRatioValue),
}

impl Default for StyleAspectRatio {
    fn default() -> Self {
        StyleAspectRatio::Auto
    }
}

crate::impl_option!(StyleAspectRatio, OptionStyleAspectRatio, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleAspectRatio {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleAspectRatio::Auto => String::from("auto"),
            StyleAspectRatio::Ratio(r) => format!("{} / {}", r.width, r.height),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleAspectRatioParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
crate::impl_debug_as_display!(StyleAspectRatioParseError<'a>);

#[cfg(feature = "parser")]
crate::impl_display! { StyleAspectRatioParseError<'a>, {
    InvalidValue(val) => format!("Invalid aspect-ratio value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleAspectRatioParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl<'a> StyleAspectRatioParseError<'a> {
    pub fn to_contained(&self) -> StyleAspectRatioParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleAspectRatioParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleAspectRatioParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleAspectRatioParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleAspectRatioParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parse aspect-ratio: "auto", "16 / 9", "1.5", "4/3"
#[cfg(feature = "parser")]
pub fn parse_style_aspect_ratio<'a>(
    input: &'a str,
) -> Result<StyleAspectRatio, StyleAspectRatioParseError<'a>> {
    let input = input.trim();
    if input == "auto" {
        return Ok(StyleAspectRatio::Auto);
    }
    // Try "w / h" or "w/h" format
    if let Some(slash_pos) = input.find('/') {
        let w_str = input[..slash_pos].trim();
        let h_str = input[slash_pos + 1..].trim();
        let w: f32 = w_str.parse().map_err(|_| StyleAspectRatioParseError::InvalidValue(input))?;
        let h: f32 = h_str.parse().map_err(|_| StyleAspectRatioParseError::InvalidValue(input))?;
        if h <= 0.0 || w <= 0.0 {
            return Err(StyleAspectRatioParseError::InvalidValue(input));
        }
        return Ok(StyleAspectRatio::Ratio(AspectRatioValue {
            width: (w * 1000.0).round() as u32,
            height: (h * 1000.0).round() as u32,
        }));
    }
    // Try single number (width/1)
    let w: f32 = input.parse().map_err(|_| StyleAspectRatioParseError::InvalidValue(input))?;
    if w <= 0.0 {
        return Err(StyleAspectRatioParseError::InvalidValue(input));
    }
    Ok(StyleAspectRatio::Ratio(AspectRatioValue {
        width: (w * 1000.0).round() as u32,
        height: 1000,
    }))
}
