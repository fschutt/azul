//! CSS properties for shadows (`box-shadow` and `text-shadow`).

use alloc::string::{String, ToString};
use core::fmt;

use crate::{
    parser::{impl_debug_as_display, impl_display, impl_from},
    props::{
        basic::{
            color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
            value::{
                parse_pixel_value_no_percent, PixelValueNoPercent, CssPixelValueParseError,
                CssPixelValueParseErrorOwned,
            },
        },
        formatter::PrintAsCssValue,
    },
};

/// What direction should a `box-shadow` be clipped in (inset or outset).
#[derive(Debug, Default, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub enum BoxShadowClipMode {
    #[default]
    Outset,
    Inset,
}

impl fmt::Display for BoxShadowClipMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BoxShadowClipMode::Outset => Ok(()), // Outset is the default, not written
            BoxShadowClipMode::Inset => write!(f, "inset"),
        }
    }
}

/// Represents a `box-shadow` or `text-shadow` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBoxShadow {
    pub offset: [PixelValueNoPercent; 2],
    pub color: ColorU,
    pub blur_radius: PixelValueNoPercent,
    pub spread_radius: PixelValueNoPercent,
    pub clip_mode: BoxShadowClipMode,
}

impl Default for StyleBoxShadow {
    fn default() -> Self {
        Self {
            offset: [PixelValueNoPercent::default(), PixelValueNoPercent::default()],
            color: ColorU::BLACK,
            blur_radius: PixelValueNoPercent::default(),
            spread_radius: PixelValueNoPercent::default(),
            clip_mode: BoxShadowClipMode::default(),
        }
    }
}

impl StyleBoxShadow {
    /// Scales the pixel values of the shadow for a given DPI factor.
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        for s in self.offset.iter_mut() {
            s.scale_for_dpi(scale_factor);
        }
        self.blur_radius.scale_for_dpi(scale_factor);
        self.spread_radius.scale_for_dpi(scale_factor);
    }
}

impl PrintAsCssValue for StyleBoxShadow {
    fn print_as_css_value(&self) -> String {
        let mut components = Vec::new();

        if self.clip_mode == BoxShadowClipMode::Inset {
            components.push("inset".to_string());
        }
        components.push(self.offset[0].to_string());
        components.push(self.offset[1].to_string());

        // Only print blur, spread, and color if they are not default, for brevity
        if self.blur_radius.inner.number.get() != 0.0
            || self.spread_radius.inner.number.get() != 0.0
        {
            components.push(self.blur_radius.to_string());
        }
        if self.spread_radius.inner.number.get() != 0.0 {
            components.push(self.spread_radius.to_string());
        }
        if self.color != ColorU::BLACK { // Assuming black is the default
             components.push(self.color.to_hash());
        }

        components.join(" ")
    }
}

// --- PARSER ---

#[derive(Clone, PartialEq)]
pub enum CssShadowParseError<'a> {
    TooManyOrTooFewComponents(&'a str),
    ValueParseErr(CssPixelValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssShadowParseError<'a>);
impl_display! { CssShadowParseError<'a>, {
    TooManyOrTooFewComponents(e) => format!("Expected 2 to 4 length values for box-shadow, found an invalid number of components in: \"{}\"", e),
    ValueParseErr(e) => format!("Invalid length value in box-shadow: {}", e),
    ColorParseError(e) => format!("Invalid color value in box-shadow: {}", e),
}}

impl_from!(CssPixelValueParseError<'a>, CssShadowParseError::ValueParseErr);
impl_from!(CssColorParseError<'a>, CssShadowParseError::ColorParseError);

/// Owned version of `CssShadowParseError`.
#[derive(Debug, Clone, PartialEq)]
pub enum CssShadowParseErrorOwned {
    TooManyOrTooFewComponents(String),
    ValueParseErr(CssPixelValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssShadowParseError<'a> {
    pub fn to_contained(&self) -> CssShadowParseErrorOwned {
        match self {
            CssShadowParseError::TooManyOrTooFewComponents(s) => {
                CssShadowParseErrorOwned::TooManyOrTooFewComponents(s.to_string())
            }
            CssShadowParseError::ValueParseErr(e) => {
                CssShadowParseErrorOwned::ValueParseErr(e.to_contained())
            }
            CssShadowParseError::ColorParseError(e) => {
                CssShadowParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssShadowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssShadowParseError<'a> {
        match self {
            CssShadowParseErrorOwned::TooManyOrTooFewComponents(s) => {
                CssShadowParseError::TooManyOrTooFewComponents(s.as_str())
            }
            CssShadowParseErrorOwned::ValueParseErr(e) => {
                CssShadowParseError::ValueParseErr(e.to_shared())
            }
            CssShadowParseErrorOwned::ColorParseError(e) => {
                CssShadowParseError::ColorParseError(e.to_shared())
            }
        }
    }
}

/// Parses a CSS box-shadow, such as `"5px 10px #888 inset"`.
///
/// Note: This parser does not handle the `none` keyword, as that is handled by the
/// `CssPropertyValue` enum wrapper. It also does not handle comma-separated lists
/// of multiple shadows; it only parses a single shadow value.
#[cfg(feature = "parser")]
pub fn parse_style_box_shadow<'a>(
    input: &'a str,
) -> Result<StyleBoxShadow, CssShadowParseError<'a>> {
    let mut parts: Vec<&str> = input.split_whitespace().collect();
    let mut shadow = StyleBoxShadow::default();

    // The `inset` keyword can appear anywhere. Find it, set the flag, and remove it.
    if let Some(pos) = parts.iter().position(|&p| p == "inset") {
        shadow.clip_mode = BoxShadowClipMode::Inset;
        parts.remove(pos);
    }

    // The color can also be anywhere. Find it, set the color, and remove it.
    // It's the only part that isn't a length. We iterate from the back because
    // it's slightly more common for the color to be last.
    if let Some(pos) = parts.iter().rposition(|p| parse_css_color(p).is_ok()) {
        shadow.color = parse_css_color(parts[pos])?;
        parts.remove(pos);
    }

    // The remaining parts must be 2, 3, or 4 length values.
    match parts.len() {
        2..=4 => {
            shadow.offset[0] = parse_pixel_value_no_percent(parts[0])?;
            shadow.offset[1] = parse_pixel_value_no_percent(parts[1])?;
            if parts.len() > 2 {
                shadow.blur_radius = parse_pixel_value_no_percent(parts[2])?;
            }
            if parts.len() > 3 {
                shadow.spread_radius = parse_pixel_value_no_percent(parts[3])?;
            }
        }
        _ => return Err(CssShadowParseError::TooManyOrTooFewComponents(input)),
    }

    Ok(shadow)
}

