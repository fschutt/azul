//! CSS properties for shadows (`box-shadow` and `text-shadow`).

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        pixel::{
            parse_pixel_value_no_percent, CssPixelValueParseError, CssPixelValueParseErrorOwned,
            PixelValueNoPercent,
        },
    },
    formatter::PrintAsCssValue,
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
    pub offset_x: PixelValueNoPercent,
    pub offset_y: PixelValueNoPercent,
    pub blur_radius: PixelValueNoPercent,
    pub spread_radius: PixelValueNoPercent,
    pub clip_mode: BoxShadowClipMode,
    pub color: ColorU,
}

impl Default for StyleBoxShadow {
    fn default() -> Self {
        Self {
            offset_x: PixelValueNoPercent::default(),
            offset_y: PixelValueNoPercent::default(),
            blur_radius: PixelValueNoPercent::default(),
            spread_radius: PixelValueNoPercent::default(),
            clip_mode: BoxShadowClipMode::default(),
            color: ColorU::BLACK,
        }
    }
}

impl StyleBoxShadow {
    /// Scales the pixel values of the shadow for a given DPI factor.
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.offset_x.scale_for_dpi(scale_factor);
        self.offset_y.scale_for_dpi(scale_factor);
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
        components.push(self.offset_x.to_string());
        components.push(self.offset_y.to_string());

        // Only print blur, spread, and color if they are not default, for brevity
        if self.blur_radius.inner.number.get() != 0.0
            || self.spread_radius.inner.number.get() != 0.0
        {
            components.push(self.blur_radius.to_string());
        }
        if self.spread_radius.inner.number.get() != 0.0 {
            components.push(self.spread_radius.to_string());
        }
        if self.color != ColorU::BLACK {
            // Assuming black is the default
            components.push(self.color.to_hash());
        }

        components.join(" ")
    }
}

// Formatting to Rust code for StyleBoxShadow
impl crate::format_rust_code::FormatAsRustCode for StyleBoxShadow {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        format!(
            "StyleBoxShadow {{\r\n{}    offset_x: {},\r\n{}    offset_y: {},\r\n{}    color: \
             {},\r\n{}    blur_radius: {},\r\n{}    spread_radius: {},\r\n{}    clip_mode: \
             BoxShadowClipMode::{:?},\r\n{}}}",
            t,
            crate::format_rust_code::format_pixel_value_no_percent(&self.offset_x),
            t,
            crate::format_rust_code::format_pixel_value_no_percent(&self.offset_y),
            t,
            crate::format_rust_code::format_color_value(&self.color),
            t,
            crate::format_rust_code::format_pixel_value_no_percent(&self.blur_radius),
            t,
            crate::format_rust_code::format_pixel_value_no_percent(&self.spread_radius),
            t,
            self.clip_mode,
            t
        )
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

impl_from!(
    CssPixelValueParseError<'a>,
    CssShadowParseError::ValueParseErr
);
impl_from!(CssColorParseError<'a>, CssShadowParseError::ColorParseError);

/// Owned version of `CssShadowParseError`.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssShadowParseErrorOwned {
    TooManyOrTooFewComponents(AzString),
    ValueParseErr(CssPixelValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssShadowParseError<'a> {
    pub fn to_contained(&self) -> CssShadowParseErrorOwned {
        match self {
            CssShadowParseError::TooManyOrTooFewComponents(s) => {
                CssShadowParseErrorOwned::TooManyOrTooFewComponents(s.to_string().into())
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
            shadow.offset_x = parse_pixel_value_no_percent(parts[0])?;
            shadow.offset_y = parse_pixel_value_no_percent(parts[1])?;
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

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::pixel::PixelValue;

    fn px_no_percent(val: f32) -> PixelValueNoPercent {
        PixelValueNoPercent {
            inner: PixelValue::px(val),
        }
    }

    #[test]
    fn test_parse_box_shadow_simple() {
        let result = parse_style_box_shadow("10px 5px").unwrap();
        assert_eq!(result.offset_x, px_no_percent(10.0));
        assert_eq!(result.offset_y, px_no_percent(5.0));
        assert_eq!(result.blur_radius, px_no_percent(0.0));
        assert_eq!(result.spread_radius, px_no_percent(0.0));
        assert_eq!(result.color, ColorU::BLACK);
        assert_eq!(result.clip_mode, BoxShadowClipMode::Outset);
    }

    #[test]
    fn test_parse_box_shadow_with_color() {
        let result = parse_style_box_shadow("10px 5px #888").unwrap();
        assert_eq!(result.offset_x, px_no_percent(10.0));
        assert_eq!(result.offset_y, px_no_percent(5.0));
        assert_eq!(result.color, ColorU::new_rgb(0x88, 0x88, 0x88));
    }

    #[test]
    fn test_parse_box_shadow_with_blur() {
        let result = parse_style_box_shadow("5px 10px 20px").unwrap();
        assert_eq!(result.offset_x, px_no_percent(5.0));
        assert_eq!(result.offset_y, px_no_percent(10.0));
        assert_eq!(result.blur_radius, px_no_percent(20.0));
    }

    #[test]
    fn test_parse_box_shadow_with_spread() {
        let result = parse_style_box_shadow("2px 2px 2px 1px rgba(0,0,0,0.2)").unwrap();
        assert_eq!(result.offset_x, px_no_percent(2.0));
        assert_eq!(result.offset_y, px_no_percent(2.0));
        assert_eq!(result.blur_radius, px_no_percent(2.0));
        assert_eq!(result.spread_radius, px_no_percent(1.0));
        assert_eq!(result.color, ColorU::new(0, 0, 0, 51));
    }

    #[test]
    fn test_parse_box_shadow_inset() {
        let result = parse_style_box_shadow("inset 0 0 10px #000").unwrap();
        assert_eq!(result.clip_mode, BoxShadowClipMode::Inset);
        assert_eq!(result.offset_x, px_no_percent(0.0));
        assert_eq!(result.offset_y, px_no_percent(0.0));
        assert_eq!(result.blur_radius, px_no_percent(10.0));
        assert_eq!(result.color, ColorU::BLACK);
    }

    #[test]
    fn test_parse_box_shadow_mixed_order() {
        let result = parse_style_box_shadow("5px 1em red inset").unwrap();
        assert_eq!(result.clip_mode, BoxShadowClipMode::Inset);
        assert_eq!(result.offset_x, px_no_percent(5.0));
        assert_eq!(
            result.offset_y,
            PixelValueNoPercent {
                inner: PixelValue::em(1.0)
            }
        );
        assert_eq!(result.color, ColorU::RED);
    }

    #[test]
    fn test_parse_box_shadow_invalid() {
        assert!(parse_style_box_shadow("10px").is_err());
        assert!(parse_style_box_shadow("10px 5px 4px 3px 2px").is_err());
        assert!(parse_style_box_shadow("10px 5px red blue").is_err());
        assert!(parse_style_box_shadow("10% 5px").is_err()); // No percent allowed
    }
}
