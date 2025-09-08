//! Text-related CSS properties

use alloc::string::String;
use core::fmt;

use crate::{
    error::{CssColorParseError, CssParsingError, CssPixelValueParseError},
    impl_option,
    props::{
        basic::{
            color::ColorU,
            value::{PercentageValue, PixelValue},
        },
        formatter::FormatAsCssValue,
    },
    AzString,
};

/// CSS color property (text color)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextColor {
    pub inner: ColorU,
}

impl fmt::Debug for StyleTextColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleTextColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleTextColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

/// CSS font-size property (moved to font.rs - this is just for compatibility)
pub use crate::props::style::font::StyleFontSize;

/// CSS text-align property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextAlign {
    Left,
    Center,
    Right,
    Justify,
}

impl Default for StyleTextAlign {
    fn default() -> Self {
        StyleTextAlign::Left
    }
}

impl_option!(
    StyleTextAlign,
    OptionStyleTextAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// CSS line-height property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLineHeight {
    pub inner: PercentageValue,
}

impl Default for StyleLineHeight {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(100),
        }
    }
}

impl fmt::Debug for StyleLineHeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleLineHeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleLineHeight {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

/// CSS letter-spacing property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLetterSpacing {
    pub inner: PixelValue,
}

impl Default for StyleLetterSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

impl fmt::Debug for StyleLetterSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleLetterSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleLetterSpacing {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

/// CSS word-spacing property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleWordSpacing {
    pub inner: PixelValue,
}

impl Default for StyleWordSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

impl fmt::Debug for StyleWordSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleWordSpacing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleWordSpacing {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

/// CSS tab-width property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTabWidth {
    pub inner: PercentageValue,
}

impl Default for StyleTabWidth {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(800), // 8 characters at 100%
        }
    }
}

impl fmt::Debug for StyleTabWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleTabWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleTabWidth {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

/// CSS direction property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleDirection {
    Ltr,
    Rtl,
}

impl Default for StyleDirection {
    fn default() -> Self {
        StyleDirection::Ltr
    }
}

impl_option!(
    StyleDirection,
    OptionStyleDirection,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for StyleDirection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleDirection::Ltr => write!(f, "ltr"),
            StyleDirection::Rtl => write!(f, "rtl"),
        }
    }
}

/// CSS hyphens property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleHyphens {
    Auto,
    None,
}

impl Default for StyleHyphens {
    fn default() -> Self {
        StyleHyphens::Auto
    }
}

impl_option!(
    StyleHyphens,
    OptionStyleHyphens,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for StyleHyphens {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleHyphens::Auto => write!(f, "auto"),
            StyleHyphens::None => write!(f, "none"),
        }
    }
}

/// CSS white-space property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleWhiteSpace {
    Normal,
    Pre,
    Nowrap,
}

impl Default for StyleWhiteSpace {
    fn default() -> Self {
        StyleWhiteSpace::Normal
    }
}

impl_option!(
    StyleWhiteSpace,
    OptionStyleWhiteSpace,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for StyleWhiteSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleWhiteSpace::Normal => write!(f, "normal"),
            StyleWhiteSpace::Pre => write!(f, "pre"),
            StyleWhiteSpace::Nowrap => write!(f, "nowrap"),
        }
    }
}

/// CSS vertical-align property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVerticalAlign {
    Top,
    Center,
    Bottom,
}

impl Default for StyleVerticalAlign {
    fn default() -> Self {
        StyleVerticalAlign::Top
    }
}

impl fmt::Display for StyleVerticalAlign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleVerticalAlign::Top => write!(f, "top"),
            StyleVerticalAlign::Center => write!(f, "center"),
            StyleVerticalAlign::Bottom => write!(f, "bottom"),
        }
    }
}

/// CSS cursor property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleCursor {
    /// `alias`
    Alias,
    /// `all-scroll`
    AllScroll,
    /// `cell`
    Cell,
    /// `col-resize`
    ColResize,
    /// `context-menu`
    ContextMenu,
    /// `copy`
    Copy,
    /// `crosshair`
    Crosshair,
    /// `default` - note: called "arrow" in winit
    Default,
    /// `e-resize`
    EResize,
    /// `ew-resize`
    EwResize,
    /// `grab`
    Grab,
    /// `grabbing`
    Grabbing,
    /// `help`
    Help,
    /// `move`
    Move,
    /// `n-resize`
    NResize,
    /// `ne-resize`
    NeResize,
    /// `nesw-resize`
    NeswResize,
    /// `ns-resize`
    NsResize,
    /// `nw-resize`
    NwResize,
    /// `nwse-resize`
    NwseResize,
    /// `pointer`
    Pointer,
    /// `progress`
    Progress,
    /// `row-resize`
    RowResize,
    /// `s-resize`
    SResize,
    /// `se-resize`
    SeResize,
    /// `sw-resize`
    SwResize,
    /// `text`
    Text,
    /// `unset`
    Unset,
    /// `w-resize`
    WResize,
    /// `wait`
    Wait,
    /// `zoom-in`
    ZoomIn,
    /// `zoom-out`
    ZoomOut,
}

impl Default for StyleCursor {
    fn default() -> Self {
        StyleCursor::Default
    }
}

impl fmt::Display for StyleCursor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use StyleCursor::*;
        let s = match self {
            Alias => "alias",
            AllScroll => "all-scroll",
            Cell => "cell",
            ColResize => "col-resize",
            ContextMenu => "context-menu",
            Copy => "copy",
            Crosshair => "crosshair",
            Default => "default",
            EResize => "e-resize",
            EwResize => "ew-resize",
            Grab => "grab",
            Grabbing => "grabbing",
            Help => "help",
            Move => "move",
            NResize => "n-resize",
            NeResize => "ne-resize",
            NeswResize => "nesw-resize",
            NsResize => "ns-resize",
            NwResize => "nw-resize",
            NwseResize => "nwse-resize",
            Pointer => "pointer",
            Progress => "progress",
            RowResize => "row-resize",
            SResize => "s-resize",
            SeResize => "se-resize",
            SwResize => "sw-resize",
            Text => "text",
            Unset => "unset",
            WResize => "w-resize",
            Wait => "wait",
            ZoomIn => "zoom-in",
            ZoomOut => "zoom-out",
        };
        write!(f, "{}", s)
    }
}

// Parsing functions and trait implementations

impl fmt::Display for StyleTextAlign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            StyleTextAlign::Left => "left",
            StyleTextAlign::Right => "right",
            StyleTextAlign::Center => "center",
            StyleTextAlign::Justify => "justify",
        };
        write!(f, "{}", s)
    }
}

impl FormatAsCssValue for StyleTextColor {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

// StyleFontSize FormatAsCssValue implementation is in font.rs

impl FormatAsCssValue for StyleTextAlign {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for StyleLineHeight {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for StyleLetterSpacing {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_text_color<'a>(
    input: &'a str,
) -> Result<StyleTextColor, CssColorParseError<'a>> {
    Ok(StyleTextColor {
        inner: crate::props::basic::color::parse_css_color(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_style_font_size<'a>(
    input: &'a str,
) -> Result<StyleFontSize, CssPixelValueParseError<'a>> {
    Ok(StyleFontSize {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_style_text_align<'a>(input: &'a str) -> Result<StyleTextAlign, CssParsingError<'a>> {
    match input.trim() {
        "left" => Ok(StyleTextAlign::Left),
        "right" => Ok(StyleTextAlign::Right),
        "center" => Ok(StyleTextAlign::Center),
        "justify" => Ok(StyleTextAlign::Justify),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_line_height<'a>(
    input: &'a str,
) -> Result<StyleLineHeight, CssPixelValueParseError<'a>> {
    Ok(StyleLineHeight {
        inner: crate::props::basic::value::parse_percentage_value(input)?,
    })
}

#[cfg(feature = "parser")]
pub fn parse_style_letter_spacing<'a>(
    input: &'a str,
) -> Result<StyleLetterSpacing, CssPixelValueParseError<'a>> {
    Ok(StyleLetterSpacing {
        inner: crate::props::basic::value::parse_pixel_value(input)?,
    })
}
