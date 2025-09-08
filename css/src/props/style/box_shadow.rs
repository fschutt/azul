//! Box shadow CSS properties

use alloc::string::String;
use core::fmt;

use crate::props::{
    basic::{color::ColorU, value::PixelValueNoPercent},
    formatter::FormatAsCssValue,
};

/// CSS box-shadow property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBoxShadow {
    pub offset: [PixelValueNoPercent; 2],
    pub color: ColorU,
    pub blur_radius: PixelValueNoPercent,
    pub spread_radius: PixelValueNoPercent,
    pub clip_mode: BoxShadowClipMode,
}

impl StyleBoxShadow {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        for s in self.offset.iter_mut() {
            s.inner.scale_for_dpi(scale_factor);
        }
        self.blur_radius.inner.scale_for_dpi(scale_factor);
        self.spread_radius.inner.scale_for_dpi(scale_factor);
    }
}

impl Default for StyleBoxShadow {
    fn default() -> Self {
        Self {
            offset: [PixelValueNoPercent::px(0.0), PixelValueNoPercent::px(0.0)],
            color: ColorU::BLACK,
            blur_radius: PixelValueNoPercent::px(0.0),
            spread_radius: PixelValueNoPercent::px(0.0),
            clip_mode: BoxShadowClipMode::Outset,
        }
    }
}

impl FormatAsCssValue for StyleBoxShadow {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.offset[0].format_as_css_value(),
            self.offset[1].format_as_css_value(),
            self.blur_radius.format_as_css_value(),
            self.spread_radius.format_as_css_value(),
            self.color.format_as_css_value(),
            self.clip_mode
        )
    }
}

/// Box shadow clip mode (inset or outset)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub enum BoxShadowClipMode {
    Outset,
    Inset,
}

impl fmt::Display for BoxShadowClipMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BoxShadowClipMode::*;
        match self {
            Outset => write!(f, "outset"),
            Inset => write!(f, "inset"),
        }
    }
}

impl Default for BoxShadowClipMode {
    fn default() -> Self {
        BoxShadowClipMode::Outset
    }
}

impl FormatAsCssValue for BoxShadowClipMode {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

// TODO: Add parsing functions
// fn parse_style_box_shadow<'a>(input: &'a str) -> Result<StyleBoxShadow, CssShadowParseError<'a>>
