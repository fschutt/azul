//! CSS properties for visual effects (opacity, blending, cursor), box sizing
//! (object-fit, object-position, aspect-ratio), and text orientation.

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
        Self {
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

// -- Visibility --

/// Represents a `visibility` attribute, controlling element visibility.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleVisibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
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
#[derive(Default)]
pub enum StyleMixBlendMode {
    #[default]
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


impl fmt::Display for StyleMixBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
#[derive(Default)]
pub enum StyleCursor {
    Alias,
    AllScroll,
    Cell,
    ColResize,
    ContextMenu,
    Copy,
    Crosshair,
    #[default]
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
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use crate::corety::AzString;
    use crate::props::basic::error::{InvalidValueErr, InvalidValueErrOwned};

    // -- Opacity Parser --

    #[derive(Clone, PartialEq, Eq)]
    pub enum OpacityParseError<'a> {
        ParsePercentage(PercentageParseError, &'a str),
        OutOfRange(&'a str),
    }
    impl_debug_as_display!(OpacityParseError<'a>);
    impl_display! { OpacityParseError<'a>, {
        ParsePercentage(e, s) => format!("Invalid opacity value \"{}\": {}", s, e),
        OutOfRange(s) => format!("Invalid opacity value \"{}\": must be between 0 and 1", s),
    }}

    /// Wrapper for `PercentageParseError` with input string.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C)]
    pub struct PercentageParseErrorWithInput {
        pub error: PercentageParseError,
        pub input: AzString,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum OpacityParseErrorOwned {
        ParsePercentage(PercentageParseErrorWithInput),
        OutOfRange(AzString),
    }

    impl OpacityParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> OpacityParseErrorOwned {
            match self {
                Self::ParsePercentage(err, s) => {
                    OpacityParseErrorOwned::ParsePercentage(PercentageParseErrorWithInput { error: err.clone(), input: (*s).to_string().into() })
                }
                Self::OutOfRange(s) => OpacityParseErrorOwned::OutOfRange((*s).to_string().into()),
            }
        }
    }

    impl OpacityParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> OpacityParseError<'_> {
            match self {
                Self::ParsePercentage(e) => {
                    OpacityParseError::ParsePercentage(e.error.clone(), e.input.as_str())
                }
                Self::OutOfRange(s) => OpacityParseError::OutOfRange(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `opacity` value.
    pub fn parse_style_opacity(input: &str) -> Result<StyleOpacity, OpacityParseError<'_>> {
        let val = parse_percentage_value(input)
            .map_err(|e| OpacityParseError::ParsePercentage(e, input))?;

        let normalized = val.normalized();
        if !(0.0..=1.0).contains(&normalized) {
            return Err(OpacityParseError::OutOfRange(input));
        }

        Ok(StyleOpacity { inner: val })
    }

    // -- Visibility Parser --

    #[derive(Clone, PartialEq, Eq)]
    pub enum StyleVisibilityParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(StyleVisibilityParseError<'a>);
    impl_display! { StyleVisibilityParseError<'a>, {
        InvalidValue(e) => format!("Invalid visibility value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, StyleVisibilityParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum StyleVisibilityParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl StyleVisibilityParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> StyleVisibilityParseErrorOwned {
            match self {
                Self::InvalidValue(e) => {
                    StyleVisibilityParseErrorOwned::InvalidValue(e.to_contained())
                }
            }
        }
    }

    impl StyleVisibilityParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> StyleVisibilityParseError<'_> {
            match self {
                Self::InvalidValue(e) => StyleVisibilityParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `visibility` value.
    pub fn parse_style_visibility(
        input: &str,
    ) -> Result<StyleVisibility, StyleVisibilityParseError<'_>> {
        let input = input.trim();
        match input {
            "visible" => Ok(StyleVisibility::Visible),
            "hidden" => Ok(StyleVisibility::Hidden),
            "collapse" => Ok(StyleVisibility::Collapse),
            _ => Err(InvalidValueErr(input).into()),
        }
    }

    // -- Mix Blend Mode Parser --

    #[derive(Clone, PartialEq, Eq)]
    pub enum MixBlendModeParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(MixBlendModeParseError<'a>);
    impl_display! { MixBlendModeParseError<'a>, {
        InvalidValue(e) => format!("Invalid mix-blend-mode value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, MixBlendModeParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum MixBlendModeParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl MixBlendModeParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> MixBlendModeParseErrorOwned {
            match self {
                Self::InvalidValue(e) => {
                    MixBlendModeParseErrorOwned::InvalidValue(e.to_contained())
                }
            }
        }
    }

    impl MixBlendModeParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> MixBlendModeParseError<'_> {
            match self {
                Self::InvalidValue(e) => MixBlendModeParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `mix-blend-mode` value.
    pub fn parse_style_mix_blend_mode(
        input: &str,
    ) -> Result<StyleMixBlendMode, MixBlendModeParseError<'_>> {
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

    #[derive(Clone, PartialEq, Eq)]
    pub enum CursorParseError<'a> {
        InvalidValue(InvalidValueErr<'a>),
    }
    impl_debug_as_display!(CursorParseError<'a>);
    impl_display! { CursorParseError<'a>, {
        InvalidValue(e) => format!("Invalid cursor value: \"{}\"", e.0),
    }}
    impl_from!(InvalidValueErr<'a>, CursorParseError::InvalidValue);

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum CursorParseErrorOwned {
        InvalidValue(InvalidValueErrOwned),
    }

    impl CursorParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CursorParseErrorOwned {
            match self {
                Self::InvalidValue(e) => CursorParseErrorOwned::InvalidValue(e.to_contained()),
            }
        }
    }

    impl CursorParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> CursorParseError<'_> {
            match self {
                Self::InvalidValue(e) => CursorParseError::InvalidValue(e.to_shared()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `cursor` value.
    pub fn parse_style_cursor(input: &str) -> Result<StyleCursor, CursorParseError<'_>> {
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
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
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

    #[test]
    fn test_parse_object_fit() {
        assert_eq!(parse_style_object_fit("fill").unwrap(), StyleObjectFit::Fill);
        assert_eq!(parse_style_object_fit("contain").unwrap(), StyleObjectFit::Contain);
        assert_eq!(parse_style_object_fit("cover").unwrap(), StyleObjectFit::Cover);
        assert_eq!(parse_style_object_fit("none").unwrap(), StyleObjectFit::None);
        assert_eq!(parse_style_object_fit("scale-down").unwrap(), StyleObjectFit::ScaleDown);
        assert_eq!(parse_style_object_fit("  cover  ").unwrap(), StyleObjectFit::Cover);
        assert!(parse_style_object_fit("stretch").is_err());
        assert!(parse_style_object_fit("").is_err());
    }

    #[test]
    fn test_parse_text_orientation() {
        assert_eq!(parse_style_text_orientation("mixed").unwrap(), StyleTextOrientation::Mixed);
        assert_eq!(parse_style_text_orientation("upright").unwrap(), StyleTextOrientation::Upright);
        assert_eq!(parse_style_text_orientation("sideways").unwrap(), StyleTextOrientation::Sideways);
        assert_eq!(parse_style_text_orientation("  mixed  ").unwrap(), StyleTextOrientation::Mixed);
        assert!(parse_style_text_orientation("vertical").is_err());
    }

    #[test]
    fn test_parse_object_position() {
        use crate::props::style::background::{BackgroundPositionHorizontal, BackgroundPositionVertical};
        let centered = parse_style_object_position("center").unwrap();
        assert_eq!(centered, parse_style_object_position("center center").unwrap());

        let lt = parse_style_object_position("left top").unwrap();
        assert_eq!(lt.horizontal, BackgroundPositionHorizontal::Left);
        assert_eq!(lt.vertical, BackgroundPositionVertical::Top);

        let rb = parse_style_object_position("right bottom").unwrap();
        assert_eq!(rb.horizontal, BackgroundPositionHorizontal::Right);
        assert_eq!(rb.vertical, BackgroundPositionVertical::Bottom);

        assert!(parse_style_object_position("left top center").is_err());
        assert!(parse_style_object_position("invalid").is_err());
    }

    #[test]
    fn test_parse_aspect_ratio() {
        assert_eq!(parse_style_aspect_ratio("auto").unwrap(), StyleAspectRatio::Auto);
        assert_eq!(
            parse_style_aspect_ratio("16 / 9").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 16000, height: 9000 })
        );
        assert_eq!(
            parse_style_aspect_ratio("16/9").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 16000, height: 9000 })
        );
        assert_eq!(
            parse_style_aspect_ratio("1.5").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 1500, height: 1000 })
        );
        assert_eq!(
            parse_style_aspect_ratio("  4 / 3  ").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 4000, height: 3000 })
        );
        assert!(parse_style_aspect_ratio("0 / 1").is_err());
        assert!(parse_style_aspect_ratio("1 / 0").is_err());
        assert!(parse_style_aspect_ratio("-1 / 1").is_err());
        assert!(parse_style_aspect_ratio("abc").is_err());
    }
}

// -- StyleObjectFit --

/// CSS object-fit property: how replaced element content is fitted to its box.
/// CSS Images Level 3 §5.5
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}


crate::impl_option!(StyleObjectFit, OptionStyleObjectFit, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleObjectFit {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Fill => "fill",
            Self::Contain => "contain",
            Self::Cover => "cover",
            Self::None => "none",
            Self::ScaleDown => "scale-down",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleObjectFitParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl StyleObjectFitParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleObjectFitParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleObjectFitParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleObjectFitParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleObjectFitParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleObjectFitParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `object-fit` value.
pub fn parse_style_object_fit(
    input: &str,
) -> Result<StyleObjectFit, StyleObjectFitParseError<'_>> {
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
#[derive(Default)]
pub enum StyleTextOrientation {
    #[default]
    Mixed,
    Upright,
    Sideways,
}


crate::impl_option!(StyleTextOrientation, OptionStyleTextOrientation, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleTextOrientation {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Mixed => "mixed",
            Self::Upright => "upright",
            Self::Sideways => "sideways",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleTextOrientationParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl StyleTextOrientationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleTextOrientationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleTextOrientationParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleTextOrientationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleTextOrientationParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleTextOrientationParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-orientation` value.
pub fn parse_style_text_orientation(
    input: &str,
) -> Result<StyleTextOrientation, StyleTextOrientationParseError<'_>> {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleObjectPositionParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl StyleObjectPositionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleObjectPositionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleObjectPositionParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleObjectPositionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleObjectPositionParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleObjectPositionParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parse object-position: accepts keyword pairs or percentage/length values.
/// Examples: "center", "left top", "50% 50%", "10px 20px"
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `object-position` value.
pub fn parse_style_object_position(
    input: &str,
) -> Result<StyleObjectPosition, StyleObjectPositionParseError<'_>> {
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
            // <position>: [left|center|right|<len>] || [top|center|bottom|<len>].
            // The `||` combinator lets two *keywords* appear in either order, so
            // canonicalize to (horizontal, vertical) first. A length in either
            // slot forces positional order (first = horizontal, second = vertical).
            let (a, b) = (parts[0], parts[1]);
            let both_keywords = matches!(a, "left" | "center" | "right" | "top" | "bottom")
                && matches!(b, "left" | "center" | "right" | "top" | "bottom");
            let reversed = both_keywords
                && (matches!(a, "top" | "bottom") || matches!(b, "left" | "right"));
            let (h_str, v_str) = if reversed { (b, a) } else { (a, b) };

            let h = match h_str {
                "left" => BackgroundPositionHorizontal::Left,
                "center" => BackgroundPositionHorizontal::Center,
                "right" => BackgroundPositionHorizontal::Right,
                other => {
                    let px = parse_pixel_value(other)
                        .map_err(|_| StyleObjectPositionParseError::InvalidValue(input))?;
                    BackgroundPositionHorizontal::Exact(px)
                }
            };
            let v = match v_str {
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

impl AspectRatioValue {
    /// Format one fixed-point component (`value * 1000`) back to its CSS number,
    /// dropping the scale and any trailing fractional zeros: 16000 -> "16",
    /// 1500 -> "1.5". Used by `PrintAsCssValue` so a printed ratio re-parses to
    /// the same value (integer math, no lossy f32 cast).
    fn fmt_component(v: u32) -> String {
        let int = v / 1000;
        let frac = v % 1000;
        if frac == 0 {
            int.to_string()
        } else {
            let frac_str = format!("{frac:03}");
            format!("{int}.{}", frac_str.trim_end_matches('0'))
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// CSS aspect-ratio property: preferred aspect ratio for the box.
/// CSS Box Sizing Level 4 §6 — values: `auto | <ratio>` (initial: `auto`)
///
/// Stored as width/height ratio. Auto means no preferred ratio.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum StyleAspectRatio {
    /// No preferred aspect ratio
    #[default]
    Auto,
    /// Fixed ratio (width / height), stored as fixed-point (value * 1000)
    Ratio(AspectRatioValue),
}


crate::impl_option!(StyleAspectRatio, OptionStyleAspectRatio, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl PrintAsCssValue for StyleAspectRatio {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => String::from("auto"),
            Self::Ratio(r) => format!(
                "{} / {}",
                AspectRatioValue::fmt_component(r.width),
                AspectRatioValue::fmt_component(r.height)
            ),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleAspectRatioParseErrorOwned {
    InvalidValue(crate::AzString),
}

#[cfg(feature = "parser")]
impl StyleAspectRatioParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleAspectRatioParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleAspectRatioParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleAspectRatioParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleAspectRatioParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleAspectRatioParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Truncating `f32` → `u32` for aspect-ratio values (callers validate the input
/// is positive and bounded, so the value always fits). Rust's `as u32` saturates
/// out-of-range floats; this isolates the one unavoidable float→int cast.
#[cfg(feature = "parser")]
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn aspect_f32_to_u32(v: f32) -> u32 {
    v as u32
}

/// Validate two ratio components and encode them into the fixed-point
/// [`AspectRatioValue`]. The positive-range checks are written as
/// `!(x > 0.0 && x <= MAX)` so NaN — which is false for every ordered
/// comparison — is rejected instead of sailing through the guards. A component
/// whose fixed-point encoding rounds to 0 (magnitude below ~0.0005) is a
/// degenerate divide-by-zero ratio and is rejected as well.
#[cfg(feature = "parser")]
fn ratio_from_components(
    w: f32,
    h: f32,
    input: &str,
) -> Result<StyleAspectRatio, StyleAspectRatioParseError<'_>> {
    if !(w > 0.0 && w <= 100_000.0 && h > 0.0 && h <= 100_000.0) {
        return Err(StyleAspectRatioParseError::InvalidValue(input));
    }
    let width = aspect_f32_to_u32((w * 1000.0).round());
    let height = aspect_f32_to_u32((h * 1000.0).round());
    if width == 0 || height == 0 {
        return Err(StyleAspectRatioParseError::InvalidValue(input));
    }
    Ok(StyleAspectRatio::Ratio(AspectRatioValue { width, height }))
}

/// Parse aspect-ratio: "auto", "16 / 9", "1.5", "4/3"
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `aspect-ratio` value.
pub fn parse_style_aspect_ratio(
    input: &str,
) -> Result<StyleAspectRatio, StyleAspectRatioParseError<'_>> {
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
        return ratio_from_components(w, h, input);
    }
    // A single number is the "<w> / 1" ratio.
    let w: f32 = input.parse().map_err(|_| StyleAspectRatioParseError::InvalidValue(input))?;
    ratio_from_components(w, 1.0, input)
}

#[cfg(all(test, feature = "parser"))]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]
mod autotest_generated {
    use super::*;
    use crate::{
        props::{
            basic::{
                error::ParseFloatError as CssParseFloatError, pixel::PixelValue,
            },
            formatter::PrintAsCssValue,
            style::background::{BackgroundPositionHorizontal, BackgroundPositionVertical},
        },
    };

    const ALL_VISIBILITY: [StyleVisibility; 3] = [
        StyleVisibility::Visible,
        StyleVisibility::Hidden,
        StyleVisibility::Collapse,
    ];

    const ALL_BLEND_MODES: [StyleMixBlendMode; 16] = [
        StyleMixBlendMode::Normal,
        StyleMixBlendMode::Multiply,
        StyleMixBlendMode::Screen,
        StyleMixBlendMode::Overlay,
        StyleMixBlendMode::Darken,
        StyleMixBlendMode::Lighten,
        StyleMixBlendMode::ColorDodge,
        StyleMixBlendMode::ColorBurn,
        StyleMixBlendMode::HardLight,
        StyleMixBlendMode::SoftLight,
        StyleMixBlendMode::Difference,
        StyleMixBlendMode::Exclusion,
        StyleMixBlendMode::Hue,
        StyleMixBlendMode::Saturation,
        StyleMixBlendMode::Color,
        StyleMixBlendMode::Luminosity,
    ];

    const ALL_CURSORS: [StyleCursor; 30] = [
        StyleCursor::Alias,
        StyleCursor::AllScroll,
        StyleCursor::Cell,
        StyleCursor::ColResize,
        StyleCursor::ContextMenu,
        StyleCursor::Copy,
        StyleCursor::Crosshair,
        StyleCursor::Default,
        StyleCursor::EResize,
        StyleCursor::EwResize,
        StyleCursor::Grab,
        StyleCursor::Grabbing,
        StyleCursor::Help,
        StyleCursor::Move,
        StyleCursor::NResize,
        StyleCursor::NsResize,
        StyleCursor::NeswResize,
        StyleCursor::NwseResize,
        StyleCursor::Pointer,
        StyleCursor::Progress,
        StyleCursor::RowResize,
        StyleCursor::SResize,
        StyleCursor::SeResize,
        StyleCursor::Text,
        StyleCursor::Unset,
        StyleCursor::VerticalText,
        StyleCursor::WResize,
        StyleCursor::Wait,
        StyleCursor::ZoomIn,
        StyleCursor::ZoomOut,
    ];

    const ALL_OBJECT_FIT: [StyleObjectFit; 5] = [
        StyleObjectFit::Fill,
        StyleObjectFit::Contain,
        StyleObjectFit::Cover,
        StyleObjectFit::None,
        StyleObjectFit::ScaleDown,
    ];

    const ALL_TEXT_ORIENTATION: [StyleTextOrientation; 3] = [
        StyleTextOrientation::Mixed,
        StyleTextOrientation::Upright,
        StyleTextOrientation::Sideways,
    ];

    /// Inputs no keyword parser may ever accept, and none may panic on.
    /// Deliberately mixes empty / whitespace / punctuation / multibyte input.
    const HOSTILE_KEYWORDS: [&str; 14] = [
        "",
        " ",
        "\t\n\r",
        "\u{a0}",           // NBSP — `str::trim` treats it as whitespace
        ";",
        "{}",
        "/*",
        "0",
        "-1",
        "NaN",
        "inf",
        "\u{1F600}",        // emoji
        "e\u{0301}",        // combining acute accent
        "\u{0665}",         // ARABIC-INDIC DIGIT FIVE (multibyte, `is_numeric`)
    ];

    // ------------------------------------------------ StyleMixBlendMode::fmt ---

    #[test]
    fn blend_mode_display_is_well_formed_for_every_variant() {
        for mode in ALL_BLEND_MODES {
            let shown = mode.to_string();
            assert!(!shown.is_empty(), "{mode:?} renders as an empty string");
            assert!(
                shown
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{mode:?} renders as {shown:?}, which is not a CSS ident"
            );
            // `PrintAsCssValue` delegates to `Display`; pin them together so a
            // future divergence has to be deliberate.
            assert_eq!(shown, mode.print_as_css_value());
        }
    }

    #[test]
    fn blend_mode_display_of_default_is_normal() {
        assert_eq!(StyleMixBlendMode::default().to_string(), "normal");
        assert_eq!(StyleMixBlendMode::default(), StyleMixBlendMode::Normal);
    }

    #[test]
    fn blend_mode_display_survives_width_and_precision_flags() {
        // The impl forwards through `write!(f, "{}", ..)` instead of `f.pad(..)`,
        // so the caller's width/precision/fill flags are dropped rather than
        // applied. Not a panic, but pin it: `{:>10}` does NOT pad.
        assert_eq!(format!("{:>10}", StyleMixBlendMode::Normal), "normal");
        assert_eq!(format!("{:.2}", StyleMixBlendMode::Multiply), "multiply");
        assert_eq!(format!("{:*^30}", StyleMixBlendMode::ColorDodge), "color-dodge");
    }

    // ----------------------------------------------------- parse_style_opacity ---

    #[test]
    fn opacity_rejects_empty_and_whitespace_only_input() {
        for input in ["", " ", "   ", "\t\n", "\r\n\t ", "\u{a0}"] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} must not parse as an opacity"
            );
        }
    }

    #[test]
    fn opacity_rejects_garbage() {
        for input in [
            "auto", "abc", "%", ";;;", "50%%", "#0.5", "0.5;garbage", "1 2", "rgb(0,0,0)",
            "0,5", "--", "..", "-", ".",
        ] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} must not parse as an opacity"
            );
        }
    }

    #[test]
    fn opacity_boundary_numbers() {
        // In range.
        assert_eq!(parse_style_opacity("0").unwrap().inner.normalized(), 0.0);
        assert_eq!(parse_style_opacity("1").unwrap().inner.normalized(), 1.0);
        assert_eq!(parse_style_opacity("0%").unwrap().inner.normalized(), 0.0);
        assert_eq!(parse_style_opacity("100%").unwrap().inner.normalized(), 1.0);
        // `-0.0 == 0.0` under IEEE-754, so the `0.0..=1.0` guard accepts it.
        assert_eq!(parse_style_opacity("-0").unwrap().inner.normalized(), 0.0);
        assert_eq!(parse_style_opacity("-0%").unwrap().inner.normalized(), 0.0);
        // Below the fixed-point resolution: quantized to 0, still in range.
        assert!(parse_style_opacity("0.0000001").is_ok());

        // Out of range.
        for input in ["1.001", "1.1", "2", "101%", "-0.001", "-1", "-100%"] {
            assert!(
                matches!(
                    parse_style_opacity(input),
                    Err(OpacityParseError::OutOfRange(_))
                ),
                "{input:?} should be rejected as out-of-range"
            );
        }

        // Float extremes: `str::parse::<f32>` maps 1e39 to +inf, which must not
        // panic through the fixed-point cast and must land out of range.
        for input in ["1e39", "3.5e38", "9223372036854775807", "1e30"] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} should be rejected as out-of-range"
            );
        }

        // `NaN` / `inf` contain no numeric char, so the scanner bails out first.
        for input in ["NaN", "nan", "inf", "infinity", "-inf", "-NaN"] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} should be rejected"
            );
        }
    }

    #[test]
    fn opacity_trims_but_rejects_trailing_junk() {
        assert_eq!(
            parse_style_opacity("  0.5  ").unwrap().inner.normalized(),
            0.5
        );
        assert_eq!(
            parse_style_opacity("\t50%\n").unwrap().inner.normalized(),
            0.5
        );
        for input in ["0.5;", "0.5 !important", "0.5px", "0.5 0.5"] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} must not parse as an opacity"
            );
        }

        // Lax, pinned: the unit is trimmed *after* being split off the number, so
        // an internal space between value and unit is accepted even though CSS
        // forbids it.
        assert_eq!(parse_style_opacity("50 %").unwrap().inner.normalized(), 0.5);
    }

    #[test]
    fn opacity_non_numeric_unicode_does_not_panic() {
        // Multibyte input whose *last* numeric char is ASCII (or which has no
        // numeric char at all) must be rejected without slicing mid-codepoint.
        // See `known_bug_opacity_multibyte_numeric_char_panics` for the case
        // that does not hold.
        for input in [
            "\u{1F600}",         // emoji only
            "\u{1F600}0.5",      // emoji then ASCII digits
            "0.5\u{0301}",       // digits then a combining acute accent
            "\u{2603}%",         // snowman + percent sign
            "\u{4F60}\u{597D}",  // CJK
            "\u{202E}0.5",       // RTL override
        ] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} must not parse as an opacity"
            );
        }
    }

    #[test]
    fn opacity_extremely_long_input_terminates() {
        // 100k digits overflow f32 to +inf => out of range, but must not hang.
        let huge = "1".repeat(100_000);
        assert!(parse_style_opacity(&huge).is_err());

        // 100k *leading* fraction zeros exercise the slow float path and stay
        // in range.
        let tiny = format!("0.{}5", "0".repeat(100_000));
        assert_eq!(parse_style_opacity(&tiny).unwrap().inner.normalized(), 0.0);

        // A long trailing unit is rejected, not truncated.
        let long_unit = format!("0.5{}", "z".repeat(100_000));
        assert!(parse_style_opacity(&long_unit).is_err());
    }

    #[test]
    fn opacity_deeply_nested_brackets_do_not_stack_overflow() {
        let nested = "(".repeat(10_000);
        assert!(parse_style_opacity(&nested).is_err());

        let wrapped = format!("{}0.5{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_opacity(&wrapped).is_err());
    }

    #[test]
    fn opacity_valid_minimal_positive_control() {
        assert!(parse_style_opacity("1").unwrap() == StyleOpacity::default());
        assert!(parse_style_opacity("50%").unwrap() == StyleOpacity::new(50.0));
        assert!(parse_style_opacity("0.5").unwrap() == StyleOpacity::new(50.0));
        // `0.5` (fraction) and `50%` are the same value.
        assert!(parse_style_opacity("0.5").unwrap() == parse_style_opacity("50%").unwrap());
    }

    #[test]
    fn opacity_round_trips_through_print_as_css_value_and_display() {
        for pct in [0.0f32, 12.5, 25.0, 50.0, 75.0, 99.9, 100.0] {
            let opacity = StyleOpacity::new(pct);

            // `PrintAsCssValue` emits the normalized 0..=1 fraction.
            let printed = opacity.print_as_css_value();
            let reparsed = parse_style_opacity(&printed)
                .unwrap_or_else(|e| panic!("{printed:?} (from {pct}%) failed to re-parse: {e}"));
            assert_eq!(
                reparsed.inner.normalized(),
                opacity.inner.normalized(),
                "{pct}% printed as {printed:?} but re-parsed differently"
            );

            // `Display` emits the percentage form; that must re-parse too.
            let displayed = opacity.to_string();
            let reparsed = parse_style_opacity(&displayed)
                .unwrap_or_else(|e| panic!("{displayed:?} (from {pct}%) failed to re-parse: {e}"));
            assert_eq!(reparsed.inner.normalized(), opacity.inner.normalized());
        }
    }

    // ------------------------------------------------- parse_style_visibility ---

    #[test]
    fn visibility_parses_every_keyword_and_round_trips() {
        assert_eq!(parse_style_visibility("visible").unwrap(), StyleVisibility::Visible);
        assert_eq!(parse_style_visibility("hidden").unwrap(), StyleVisibility::Hidden);
        assert_eq!(parse_style_visibility("collapse").unwrap(), StyleVisibility::Collapse);
        assert_eq!(StyleVisibility::default(), StyleVisibility::Visible);

        for v in ALL_VISIBILITY {
            let printed = v.print_as_css_value();
            assert!(!printed.is_empty());
            assert_eq!(parse_style_visibility(&printed).unwrap(), v);
            // Surrounding whitespace is trimmed, not rejected.
            assert_eq!(parse_style_visibility(&format!("  {printed}\t")).unwrap(), v);
        }
    }

    #[test]
    fn visibility_rejects_hostile_input() {
        for input in HOSTILE_KEYWORDS {
            assert!(
                parse_style_visibility(input).is_err(),
                "{input:?} must not parse as a visibility"
            );
        }
        for input in ["none", "show", "visible hidden", "visible;", "vis", "visibleX"] {
            assert!(
                parse_style_visibility(input).is_err(),
                "{input:?} must not parse as a visibility"
            );
        }
    }

    // -------------------------------------------- parse_style_mix_blend_mode ---

    #[test]
    fn blend_mode_parses_every_keyword_and_round_trips() {
        for mode in ALL_BLEND_MODES {
            let printed = mode.print_as_css_value();
            assert_eq!(
                parse_style_mix_blend_mode(&printed).unwrap(),
                mode,
                "{printed:?} did not round-trip"
            );
            assert_eq!(parse_style_mix_blend_mode(&format!(" {printed} ")).unwrap(), mode);
        }
        assert_eq!(StyleMixBlendMode::default(), StyleMixBlendMode::Normal);
    }

    #[test]
    fn blend_mode_rejects_hostile_input() {
        for input in HOSTILE_KEYWORDS {
            assert!(
                parse_style_mix_blend_mode(input).is_err(),
                "{input:?} must not parse as a mix-blend-mode"
            );
        }
        // Near-misses: separator swaps, plain-CSS-adjacent words, partial idents.
        for input in [
            "mix", "color dodge", "color_dodge", "colordodge", "normal normal", "plus-lighter",
            "multiply;", "screen!",
        ] {
            assert!(
                parse_style_mix_blend_mode(input).is_err(),
                "{input:?} must not parse as a mix-blend-mode"
            );
        }
    }

    // ------------------------------------------------------ parse_style_cursor ---

    #[test]
    fn cursor_parses_every_keyword_and_round_trips() {
        for cursor in ALL_CURSORS {
            let printed = cursor.print_as_css_value();
            assert_eq!(
                parse_style_cursor(&printed).unwrap(),
                cursor,
                "{printed:?} did not round-trip"
            );
            assert_eq!(parse_style_cursor(&format!("\n{printed}  ")).unwrap(), cursor);
        }
        assert_eq!(StyleCursor::default(), StyleCursor::Default);
    }

    #[test]
    fn cursor_keyword_printing_is_injective() {
        // Two variants mapping to the same CSS ident would silently collapse on
        // re-parse; the round-trip test above cannot catch that on its own.
        let mut printed: Vec<String> = ALL_CURSORS.iter().map(PrintAsCssValue::print_as_css_value).collect();
        printed.sort();
        let count = printed.len();
        printed.dedup();
        assert_eq!(printed.len(), count, "two StyleCursor variants print the same ident");
    }

    #[test]
    fn cursor_rejects_hostile_input() {
        for input in HOSTILE_KEYWORDS {
            assert!(
                parse_style_cursor(input).is_err(),
                "{input:?} must not parse as a cursor"
            );
        }
        for input in [
            "hand",           // legacy IE alias, deliberately unsupported
            "col resize",     // space instead of hyphen
            "e_resize",
            "pointer pointer",
            "url(cursor.png)",
            "auto",           // valid CSS, but not in the enum
        ] {
            assert!(
                parse_style_cursor(input).is_err(),
                "{input:?} must not parse as a cursor"
            );
        }
    }

    // -------------------------------------------------- parse_style_object_fit ---

    #[test]
    fn object_fit_parses_every_keyword_and_round_trips() {
        for fit in ALL_OBJECT_FIT {
            let printed = fit.print_as_css_value();
            assert_eq!(parse_style_object_fit(&printed).unwrap(), fit);
            assert_eq!(parse_style_object_fit(&format!("  {printed} ")).unwrap(), fit);
        }
        assert_eq!(StyleObjectFit::default(), StyleObjectFit::Fill);
    }

    #[test]
    fn object_fit_rejects_hostile_input() {
        for input in HOSTILE_KEYWORDS {
            assert!(
                parse_style_object_fit(input).is_err(),
                "{input:?} must not parse as an object-fit"
            );
        }
        for input in ["stretch", "scale_down", "scale down", "cover cover", "fill;"] {
            assert!(
                parse_style_object_fit(input).is_err(),
                "{input:?} must not parse as an object-fit"
            );
        }
    }

    // -------------------------------------------- parse_style_text_orientation ---

    #[test]
    fn text_orientation_parses_every_keyword_and_round_trips() {
        for orientation in ALL_TEXT_ORIENTATION {
            let printed = orientation.print_as_css_value();
            assert_eq!(parse_style_text_orientation(&printed).unwrap(), orientation);
            assert_eq!(
                parse_style_text_orientation(&format!("\t{printed}\n")).unwrap(),
                orientation
            );
        }
        assert_eq!(StyleTextOrientation::default(), StyleTextOrientation::Mixed);
    }

    #[test]
    fn text_orientation_rejects_hostile_input() {
        for input in HOSTILE_KEYWORDS {
            assert!(
                parse_style_text_orientation(input).is_err(),
                "{input:?} must not parse as a text-orientation"
            );
        }
        for input in ["vertical", "sideways-right", "upright mixed", "mixed;"] {
            assert!(
                parse_style_text_orientation(input).is_err(),
                "{input:?} must not parse as a text-orientation"
            );
        }
    }

    // ----------------------------------------- keyword parsers, shared invariant ---

    #[test]
    fn keyword_parsers_are_case_sensitive() {
        // CSS idents are ASCII case-insensitive per spec, but every keyword
        // parser in this crate matches the lowercase form only. Pinned so that
        // adding case-folding is a deliberate, crate-wide change rather than an
        // accident in one parser.
        assert!(parse_style_visibility("VISIBLE").is_err());
        assert!(parse_style_mix_blend_mode("Multiply").is_err());
        assert!(parse_style_cursor("Pointer").is_err());
        assert!(parse_style_object_fit("COVER").is_err());
        assert!(parse_style_text_orientation("Upright").is_err());
        assert!(parse_style_aspect_ratio("AUTO").is_err());
    }

    #[test]
    fn keyword_parsers_do_not_hang_on_extremely_long_input() {
        let long = "a".repeat(500_000);
        assert!(parse_style_visibility(&long).is_err());
        assert!(parse_style_mix_blend_mode(&long).is_err());
        assert!(parse_style_cursor(&long).is_err());
        assert!(parse_style_object_fit(&long).is_err());
        assert!(parse_style_text_orientation(&long).is_err());

        // A valid keyword buried in 500k of padding is still just whitespace-
        // trimmed, so it parses; the padding must not be quadratic.
        let padded = format!("{}visible{}", " ".repeat(250_000), " ".repeat(250_000));
        assert_eq!(parse_style_visibility(&padded).unwrap(), StyleVisibility::Visible);
    }

    #[test]
    fn keyword_parsers_do_not_stack_overflow_on_nested_input() {
        let nested = format!("{}center{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_visibility(&nested).is_err());
        assert!(parse_style_cursor(&nested).is_err());
        assert!(parse_style_object_fit(&nested).is_err());
        assert!(parse_style_object_position(&nested).is_err());
        assert!(parse_style_aspect_ratio(&nested).is_err());
    }

    // --------------------------------------------- parse_style_object_position ---

    #[test]
    fn object_position_parses_single_keywords() {
        use BackgroundPositionHorizontal as H;
        use BackgroundPositionVertical as V;

        for (input, h, v) in [
            ("center", H::Center, V::Center),
            ("left", H::Left, V::Center),
            ("right", H::Right, V::Center),
            ("top", H::Center, V::Top),
            ("bottom", H::Center, V::Bottom),
        ] {
            let parsed = parse_style_object_position(input).unwrap();
            assert_eq!(parsed.horizontal, h, "{input:?} horizontal");
            assert_eq!(parsed.vertical, v, "{input:?} vertical");
        }
    }

    #[test]
    fn object_position_parses_lengths_and_percentages() {
        let px = parse_style_object_position("10px 20px").unwrap();
        assert_eq!(px.horizontal, BackgroundPositionHorizontal::Exact(PixelValue::px(10.0)));
        assert_eq!(px.vertical, BackgroundPositionVertical::Exact(PixelValue::px(20.0)));

        let pct = parse_style_object_position("50% 50%").unwrap();
        assert_eq!(
            pct.horizontal,
            BackgroundPositionHorizontal::Exact(PixelValue::percent(50.0))
        );
        assert_eq!(
            pct.vertical,
            BackgroundPositionVertical::Exact(PixelValue::percent(50.0))
        );

        // A single length applies to *both* axes.
        let single = parse_style_object_position("25%").unwrap();
        assert_eq!(
            single.horizontal,
            BackgroundPositionHorizontal::Exact(PixelValue::percent(25.0))
        );
        assert_eq!(
            single.vertical,
            BackgroundPositionVertical::Exact(PixelValue::percent(25.0))
        );

        // Mixed keyword + length, both orders.
        assert_eq!(
            parse_style_object_position("left 25%").unwrap(),
            StyleObjectPosition {
                horizontal: BackgroundPositionHorizontal::Left,
                vertical: BackgroundPositionVertical::Exact(PixelValue::percent(25.0)),
            }
        );
        assert_eq!(
            parse_style_object_position("25% top").unwrap(),
            StyleObjectPosition {
                horizontal: BackgroundPositionHorizontal::Exact(PixelValue::percent(25.0)),
                vertical: BackgroundPositionVertical::Top,
            }
        );
    }

    #[test]
    fn object_position_collapses_internal_whitespace() {
        // `split_whitespace` means any run of blanks separates the components.
        let expected = parse_style_object_position("left top").unwrap();
        for input in ["left  top", "left\ttop", "  left \n top  ", "left\r\ntop"] {
            assert_eq!(
                parse_style_object_position(input).unwrap(),
                expected,
                "{input:?} should be equivalent to \"left top\""
            );
        }
    }

    #[test]
    fn object_position_rejects_wrong_component_counts_and_garbage() {
        for input in [
            "",
            "   ",
            "\t\n",
            "left top center",
            "10px 20px 30px",
            "center center center center",
            "invalid",
            "left left",     // second component must be a vertical keyword or a length
            "top top",       // first component must be a horizontal keyword or a length
            "left,top",      // comma is not a component separator
            ";",
            "\u{1F600}",
            "\u{1F600} \u{1F600}",
        ] {
            assert!(
                parse_style_object_position(input).is_err(),
                "{input:?} must not parse as an object-position"
            );
        }
    }

    #[test]
    fn object_position_extreme_lengths_do_not_panic() {
        // `parse_pixel_value` accepts bare floats (incl. NaN/inf) and saturates
        // them in the fixed-point cast — characterized in pixel.rs. All that is
        // asserted here is that object-position does not panic on them.
        for input in [
            "NaN NaN", "inf inf", "-inf", "1e39px", "-1e39px", "340282350000000000000000000000000000000px",
        ] {
            let _ = parse_style_object_position(input);
        }
        let long = format!("{}px", "9".repeat(100_000));
        let _ = parse_style_object_position(&long);
    }

    #[test]
    fn object_position_round_trips_through_print_as_css_value() {
        use BackgroundPositionHorizontal as H;
        use BackgroundPositionVertical as V;

        let horizontals = [H::Left, H::Center, H::Right, H::Exact(PixelValue::percent(25.0))];
        let verticals = [V::Top, V::Center, V::Bottom, V::Exact(PixelValue::px(30.0))];

        for horizontal in horizontals {
            for vertical in verticals {
                let position = StyleObjectPosition { horizontal, vertical };
                let printed = position.print_as_css_value();
                let reparsed = parse_style_object_position(&printed)
                    .unwrap_or_else(|e| panic!("{position:?} printed as {printed:?}, which failed to re-parse: {e}"));
                assert_eq!(reparsed, position, "{printed:?} did not round-trip");
            }
        }

        // The documented initial value is `50% 50%`.
        let default = StyleObjectPosition::default();
        assert_eq!(default.print_as_css_value(), "50% 50%");
        assert_eq!(parse_style_object_position("50% 50%").unwrap(), default);
        assert_eq!(parse_style_object_position("center").unwrap().print_as_css_value(), "center center");
    }

    // ------------------------------------------------------ aspect_f32_to_u32 ---

    #[test]
    fn aspect_f32_to_u32_saturates_instead_of_panicking() {
        // Zero / truncation.
        assert_eq!(aspect_f32_to_u32(0.0), 0);
        assert_eq!(aspect_f32_to_u32(-0.0), 0);
        assert_eq!(aspect_f32_to_u32(0.9), 0);
        assert_eq!(aspect_f32_to_u32(1.0), 1);
        assert_eq!(aspect_f32_to_u32(1.9), 1);
        assert_eq!(aspect_f32_to_u32(f32::MIN_POSITIVE), 0);

        // Negatives saturate to 0 (`as` is a saturating cast since Rust 1.45).
        assert_eq!(aspect_f32_to_u32(-1.0), 0);
        assert_eq!(aspect_f32_to_u32(-0.5), 0);
        assert_eq!(aspect_f32_to_u32(-1e30), 0);
        assert_eq!(aspect_f32_to_u32(f32::MIN), 0);
        assert_eq!(aspect_f32_to_u32(f32::NEG_INFINITY), 0);

        // Above u32::MAX saturates to u32::MAX.
        assert_eq!(aspect_f32_to_u32(f32::MAX), u32::MAX);
        assert_eq!(aspect_f32_to_u32(f32::INFINITY), u32::MAX);
        assert_eq!(aspect_f32_to_u32(1e30), u32::MAX);
        // `u32::MAX as f32` rounds *up* to 2^32, so it saturates back down.
        assert_eq!(aspect_f32_to_u32(u32::MAX as f32), u32::MAX);

        // NaN is defined to be 0, not UB and not a panic.
        assert_eq!(aspect_f32_to_u32(f32::NAN), 0);
        assert_eq!(aspect_f32_to_u32(-f32::NAN), 0);

        // The largest value the parser can hand it (100_000 * 1000) fits exactly.
        assert_eq!(aspect_f32_to_u32(100_000.0 * 1000.0), 100_000_000);
    }

    #[test]
    fn aspect_f32_to_u32_is_usable_in_const_context() {
        const TRUNCATED: u32 = aspect_f32_to_u32(1.999);
        const SATURATED: u32 = aspect_f32_to_u32(f32::INFINITY);
        const NEGATIVE: u32 = aspect_f32_to_u32(-5.0);
        const NOT_A_NUMBER: u32 = aspect_f32_to_u32(f32::NAN);
        assert_eq!((TRUNCATED, SATURATED, NEGATIVE, NOT_A_NUMBER), (1, u32::MAX, 0, 0));
    }

    // ------------------------------------------------ parse_style_aspect_ratio ---

    #[test]
    fn aspect_ratio_parses_valid_forms() {
        assert_eq!(parse_style_aspect_ratio("auto").unwrap(), StyleAspectRatio::Auto);
        assert_eq!(StyleAspectRatio::default(), StyleAspectRatio::Auto);

        for input in ["16 / 9", "16/9", "16 /9", "16/ 9", "  16  /  9  "] {
            assert_eq!(
                parse_style_aspect_ratio(input).unwrap(),
                StyleAspectRatio::Ratio(AspectRatioValue { width: 16000, height: 9000 }),
                "{input:?} should parse as 16/9"
            );
        }

        // A bare number is `<number> / 1`, stored as fixed-point * 1000.
        assert_eq!(
            parse_style_aspect_ratio("1").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 1000, height: 1000 })
        );
        assert_eq!(
            parse_style_aspect_ratio("1.5").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 1500, height: 1000 })
        );

        // Boundary of the documented range: 100_000 is accepted, just above is not.
        assert_eq!(
            parse_style_aspect_ratio("100000").unwrap(),
            StyleAspectRatio::Ratio(AspectRatioValue { width: 100_000_000, height: 1000 })
        );
        assert!(parse_style_aspect_ratio("100001").is_err());
        assert!(parse_style_aspect_ratio("100000.1 / 1").is_err());
        assert!(parse_style_aspect_ratio("1 / 100001").is_err());
    }

    #[test]
    fn aspect_ratio_rejects_non_positive_and_malformed_input() {
        for input in [
            "", "   ", "\t\n", "abc", "auto / auto", "16 / 9 / 4", "1/2/3", "/", "//", "/9",
            "16/", "16 9", ";", "16,9", "\u{1F600}", "\u{1F600}/\u{1F600}",
        ] {
            assert!(
                parse_style_aspect_ratio(input).is_err(),
                "{input:?} must not parse as an aspect-ratio"
            );
        }

        // Zero and negative components are explicitly rejected.
        for input in ["0", "0 / 1", "1 / 0", "0/0", "-0", "-0 / 1", "1 / -0", "-1 / 1", "-1", "-1.5"] {
            assert!(
                parse_style_aspect_ratio(input).is_err(),
                "{input:?} must not parse as an aspect-ratio"
            );
        }

        // Infinities exceed the 100_000 bound (or are non-positive).
        for input in ["inf", "inf / 1", "1 / inf", "-inf", "-inf / 1", "1e39", "1e39 / 1"] {
            assert!(
                parse_style_aspect_ratio(input).is_err(),
                "{input:?} should be rejected: out of the [0, 100_000] range"
            );
        }
    }

    #[test]
    fn aspect_ratio_extremely_long_input_terminates() {
        let long = "9".repeat(100_000);
        assert!(parse_style_aspect_ratio(&long).is_err());
        assert!(parse_style_aspect_ratio(&format!("{long}/{long}")).is_err());

        // 100k slashes: `find('/')` hits the first one, both sides fail to parse.
        let slashes = "/".repeat(100_000);
        assert!(parse_style_aspect_ratio(&slashes).is_err());
    }

    #[test]
    fn aspect_ratio_auto_round_trips() {
        let printed = StyleAspectRatio::Auto.print_as_css_value();
        assert_eq!(printed, "auto");
        assert_eq!(parse_style_aspect_ratio(&printed).unwrap(), StyleAspectRatio::Auto);
    }

    // ------------------------------------------- error types: to_contained/to_shared ---

    #[test]
    fn opacity_parse_error_round_trips_through_the_owned_form() {
        let errors = [
            OpacityParseError::ParsePercentage(
                PercentageParseError::ValueParseErr(CssParseFloatError::Empty),
                "",
            ),
            OpacityParseError::ParsePercentage(
                PercentageParseError::ValueParseErr(CssParseFloatError::Invalid),
                "abc",
            ),
            OpacityParseError::ParsePercentage(PercentageParseError::NoPercentSign, "0.5"),
            OpacityParseError::ParsePercentage(
                PercentageParseError::InvalidUnit(String::from("px").into()),
                "5px",
            ),
            OpacityParseError::OutOfRange("1.5"),
            OpacityParseError::OutOfRange(""),
            OpacityParseError::OutOfRange("\u{1F600}"),
        ];

        for error in errors {
            let owned = error.to_contained();
            assert_eq!(owned.to_shared(), error, "{error:?} did not round-trip");
            assert_eq!(owned.to_shared().to_contained(), owned);

            let shown = error.to_string();
            assert!(!shown.is_empty(), "{error:?} renders as an empty message");
            // `impl_debug_as_display` forwards Debug to Display.
            assert_eq!(format!("{error:?}"), shown);
        }
    }

    #[test]
    fn opacity_parse_error_to_contained_copies_the_borrowed_input() {
        // The owned form must not alias the (possibly temporary) input slice.
        let owned = {
            let input = String::from("1.5");
            parse_style_opacity(&input).unwrap_err().to_contained()
        };
        assert_eq!(owned, OpacityParseErrorOwned::OutOfRange(String::from("1.5").into()));
        assert!(owned.to_shared().to_string().contains("1.5"));
    }

    #[test]
    fn keyword_parse_errors_round_trip_through_the_owned_form() {
        // All four `InvalidValueErr`-backed error types, over hostile payloads.
        for payload in ["", "junk", "  ", "\u{1F600}", "a\0b", "\u{0665}"] {
            let visibility = StyleVisibilityParseError::InvalidValue(InvalidValueErr(payload));
            assert_eq!(visibility.to_contained().to_shared(), visibility);
            assert!(!visibility.to_string().is_empty());
            assert_eq!(format!("{visibility:?}"), visibility.to_string());

            let blend = MixBlendModeParseError::InvalidValue(InvalidValueErr(payload));
            assert_eq!(blend.to_contained().to_shared(), blend);
            assert!(!blend.to_string().is_empty());

            let cursor = CursorParseError::InvalidValue(InvalidValueErr(payload));
            assert_eq!(cursor.to_contained().to_shared(), cursor);
            assert!(!cursor.to_string().is_empty());

            // The `&str`-backed error types.
            let object_fit = StyleObjectFitParseError::InvalidValue(payload);
            assert_eq!(object_fit.to_contained().to_shared(), object_fit);
            assert!(!object_fit.to_string().is_empty());

            let orientation = StyleTextOrientationParseError::InvalidValue(payload);
            assert_eq!(orientation.to_contained().to_shared(), orientation);
            assert!(!orientation.to_string().is_empty());

            let position = StyleObjectPositionParseError::InvalidValue(payload);
            assert_eq!(position.to_contained().to_shared(), position);
            assert!(!position.to_string().is_empty());

            let ratio = StyleAspectRatioParseError::InvalidValue(payload);
            assert_eq!(ratio.to_contained().to_shared(), ratio);
            assert!(!ratio.to_string().is_empty());
        }
    }

    #[test]
    fn parse_errors_quote_the_offending_input() {
        // The rejected value has to survive into the message, or authors cannot
        // find the bad declaration.
        assert!(parse_style_visibility("show").unwrap_err().to_string().contains("show"));
        assert!(parse_style_mix_blend_mode("mix").unwrap_err().to_string().contains("mix"));
        assert!(parse_style_cursor("hand").unwrap_err().to_string().contains("hand"));
        assert!(parse_style_object_fit("stretch").unwrap_err().to_string().contains("stretch"));
        assert!(parse_style_text_orientation("vertical").unwrap_err().to_string().contains("vertical"));
        assert!(parse_style_object_position("nope").unwrap_err().to_string().contains("nope"));
        assert!(parse_style_aspect_ratio("nope").unwrap_err().to_string().contains("nope"));
        assert!(parse_style_opacity("1.5").unwrap_err().to_string().contains("1.5"));
    }

    #[test]
    fn parse_errors_report_the_trimmed_input_not_the_raw_slice() {
        // Every keyword parser trims *before* constructing the error, so the
        // message never contains the caller's padding.
        let shown = parse_style_cursor("  hand  ").unwrap_err().to_string();
        assert!(shown.contains("\"hand\""), "expected the trimmed value, got {shown:?}");

        // ...except `parse_style_opacity`, which passes the *untrimmed* input to
        // the error. Pinned so the inconsistency is visible.
        let shown = parse_style_opacity("  1.5  ").unwrap_err().to_string();
        assert!(shown.contains("\"  1.5  \""), "expected the raw value, got {shown:?}");
    }

    #[test]
    fn owned_error_forms_are_independent_of_the_source_buffer() {
        // `to_contained` must deep-copy: the owned error has to outlive the
        // String it was parsed from.
        let owned = {
            let input = String::from("stretch");
            parse_style_object_fit(&input).unwrap_err().to_contained()
        };
        assert_eq!(
            owned,
            StyleObjectFitParseErrorOwned::InvalidValue(String::from("stretch").into())
        );
        assert!(owned.to_shared().to_string().contains("stretch"));
    }

    // ------------------------------------------------------------ known bugs ---
    //
    // The tests below assert the behaviour these functions must have; they are
    // regression guards for bugs that have since been fixed.

    #[test]
    fn known_bug_opacity_multibyte_numeric_char_panics() {
        // `char::is_numeric()` is true for Nd/Nl/No, including multi-byte chars
        // like '½' (U+00BD) and '٥' (U+0665). `parse_percentage_value` records
        // the *start* byte index of the last such char and then slices at
        // `split_pos + 1`, which lands inside the codepoint => the slice panics.
        //
        // `opacity: ½` in any author stylesheet therefore panics the CSS parser.
        // See `known_bug_percentage_multibyte_numeric_char_panics` in length.rs.
        for input in ["\u{00BD}", "\u{00BD}%", "0.5\u{0665}", "\u{FF15}%"] {
            assert!(
                parse_style_opacity(input).is_err(),
                "{input:?} should be rejected, not panic"
            );
        }
    }

    #[test]
    fn known_bug_aspect_ratio_nan_bypasses_the_range_guards() {
        // Every guard in `parse_style_aspect_ratio` is a float comparison
        // (`h <= 0.0 || w <= 0.0 || w > 100_000.0 || h > 100_000.0`), and every
        // comparison against NaN is false — so a NaN component sails through and
        // `aspect_f32_to_u32(NaN)` turns it into 0. The parser explicitly rejects
        // "0 / 1", but happily returns `Ratio { width: 0, height: 1000 }` for
        // "NaN", which is a division by zero waiting to happen in layout.
        for input in ["NaN", "nan", "NaN / 1", "1 / NaN", "nan/nan", "-NaN"] {
            assert!(
                parse_style_aspect_ratio(input).is_err(),
                "{input:?} should be rejected, but parsed as {:?}",
                parse_style_aspect_ratio(input)
            );
        }
    }

    #[test]
    fn known_bug_aspect_ratio_tiny_positive_values_round_down_to_zero() {
        // `w > 0.0` passes, but `(w * 1000.0).round()` is 0 for anything below
        // 0.0005 — so a positive ratio silently becomes the degenerate 0 that the
        // guard exists to prevent.
        for input in ["0.0001", "0.0004 / 1", "1 / 0.0001", "1e-10"] {
            let Ok(StyleAspectRatio::Ratio(ratio)) = parse_style_aspect_ratio(input) else {
                continue; // rejected outright — that is the fix
            };
            assert!(
                ratio.width > 0 && ratio.height > 0,
                "{input:?} produced the degenerate ratio {ratio:?}"
            );
        }
    }

    #[test]
    fn known_bug_aspect_ratio_does_not_survive_a_print_reparse_cycle() {
        // `Ratio { width: 16000, height: 9000 }` (i.e. 16/9) prints as
        // "16000 / 9000", so every print/parse cycle multiplies both components
        // by 1000. One cycle changes the stored value; two cycles exceed the
        // 100_000 bound and fail to parse at all.
        let ratio = parse_style_aspect_ratio("16 / 9").unwrap();
        let printed = ratio.print_as_css_value();
        assert_eq!(printed, "16 / 9", "printed the fixed-point form: {printed:?}");
        assert_eq!(parse_style_aspect_ratio(&printed).unwrap(), ratio);
    }

    #[test]
    fn known_bug_object_position_rejects_reversed_keyword_pairs() {
        // `<position>` is `[left|center|right] || [top|center|bottom]` — the `||`
        // means either order is valid, so `object-position: top left` is legal
        // CSS. The parser only ever reads parts[0] as the horizontal component,
        // so it hands "top" to `parse_pixel_value` and fails.
        assert_eq!(
            parse_style_object_position("top left").unwrap(),
            parse_style_object_position("left top").unwrap()
        );
        assert_eq!(
            parse_style_object_position("bottom right").unwrap(),
            parse_style_object_position("right bottom").unwrap()
        );
    }
}
