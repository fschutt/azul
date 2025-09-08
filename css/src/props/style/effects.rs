//! Visual effects CSS properties (opacity, mix-blend-mode, backface-visibility, scrollbar)

use crate::props::basic::{
    color::ColorU,
    value::{PercentageValue, PixelValue},
};
use crate::props::formatter::FormatAsCssValue;
use crate::props::style::background::StyleBackgroundContent;
use alloc::string::String;
use core::fmt;

/// CSS opacity property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleOpacity {
    pub inner: PercentageValue,
}

impl Default for StyleOpacity {
    fn default() -> Self {
        StyleOpacity {
            inner: PercentageValue::new(100.0), // 100% = fully opaque
        }
    }
}

impl StyleOpacity {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl fmt::Display for StyleOpacity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl FormatAsCssValue for StyleOpacity {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

/// CSS mix-blend-mode property
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
        use self::StyleMixBlendMode::*;
        write!(
            f,
            "{}",
            match self {
                Normal => "normal",
                Multiply => "multiply",
                Screen => "screen",
                Overlay => "overlay",
                Darken => "darken",
                Lighten => "lighten",
                ColorDodge => "color-dodge",
                ColorBurn => "color-burn",
                HardLight => "hard-light",
                SoftLight => "soft-light",
                Difference => "difference",
                Exclusion => "exclusion",
                Hue => "hue",
                Saturation => "saturation",
                Color => "color",
                Luminosity => "luminosity",
            }
        )
    }
}

impl FormatAsCssValue for StyleMixBlendMode {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// CSS backface-visibility property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackfaceVisibility {
    Hidden,
    Visible,
}

impl Default for StyleBackfaceVisibility {
    fn default() -> Self {
        StyleBackfaceVisibility::Visible
    }
}

impl fmt::Display for StyleBackfaceVisibility {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleBackfaceVisibility::Hidden => write!(f, "hidden"),
            StyleBackfaceVisibility::Visible => write!(f, "visible"),
        }
    }
}

impl FormatAsCssValue for StyleBackfaceVisibility {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// Azul-specific scrollbar styling
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScrollbarStyle {
    /// Vertical scrollbar style, if any
    pub horizontal: ScrollbarInfo,
    /// Horizontal scrollbar style, if any
    pub vertical: ScrollbarInfo,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            horizontal: ScrollbarInfo::default(),
            vertical: ScrollbarInfo::default(),
        }
    }
}

/// Individual scrollbar component styling
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScrollbarInfo {
    pub width: PixelValue,
    pub padding_left: PixelValue,
    pub padding_right: PixelValue,
    pub track: StyleBackgroundContent,
    pub thumb: StyleBackgroundContent,
    pub button: StyleBackgroundContent,
    pub corner: StyleBackgroundContent,
    pub resizer: StyleBackgroundContent,
}

impl Default for ScrollbarInfo {
    fn default() -> Self {
        Self {
            width: PixelValue::px(17.0),
            padding_left: PixelValue::zero(),
            padding_right: PixelValue::zero(),
            track: StyleBackgroundContent::Color(ColorU {
                r: 241,
                g: 241,
                b: 241,
                a: 255,
            }),
            thumb: StyleBackgroundContent::Color(ColorU {
                r: 193,
                g: 193,
                b: 193,
                a: 255,
            }),
            button: StyleBackgroundContent::Color(ColorU {
                r: 163,
                g: 163,
                b: 163,
                a: 255,
            }),
            corner: StyleBackgroundContent::default(),
            resizer: StyleBackgroundContent::default(),
        }
    }
}

// TODO: Add parsing functions
// fn parse_style_opacity<'a>(input: &'a str) -> Result<StyleOpacity, OpacityParseError<'a>>
// fn parse_style_mix_blend_mode<'a>(input: &'a str) -> Result<StyleMixBlendMode, CssParsingError<'a>>
// fn parse_style_backface_visibility<'a>(input: &'a str) -> Result<StyleBackfaceVisibility, CssParsingError<'a>>
// fn parse_scrollbar_style<'a>(input: &'a str) -> Result<ScrollbarStyle, CssScrollbarStyleParseError<'a>>
