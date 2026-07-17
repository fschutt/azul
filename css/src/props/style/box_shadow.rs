//! Shared types for CSS shadow properties (used by both `box-shadow` and `text-shadow`).

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outset => Ok(()), // Outset is the default, not written
            Self::Inset => write!(f, "inset"),
        }
    }
}

/// Represents a single CSS shadow value, shared by both `box-shadow` and `text-shadow`.
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
impl crate::codegen::format::FormatAsRustCode for StyleBoxShadow {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        format!(
            "StyleBoxShadow {{\r\n{}    offset_x: {},\r\n{}    offset_y: {},\r\n{}    color: \
             {},\r\n{}    blur_radius: {},\r\n{}    spread_radius: {},\r\n{}    clip_mode: \
             BoxShadowClipMode::{:?},\r\n{}}}",
            t,
            crate::codegen::format::format_pixel_value_no_percent(&self.offset_x),
            t,
            crate::codegen::format::format_pixel_value_no_percent(&self.offset_y),
            t,
            crate::codegen::format::format_color_value(&self.color),
            t,
            crate::codegen::format::format_pixel_value_no_percent(&self.blur_radius),
            t,
            crate::codegen::format::format_pixel_value_no_percent(&self.spread_radius),
            t,
            self.clip_mode,
            t
        )
    }
}

// --- PARSER ---

/// Error returned when parsing a CSS shadow value fails.
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

impl CssShadowParseError<'_> {
    /// Converts the borrowed error into an owned version for storage.
    #[must_use] pub fn to_contained(&self) -> CssShadowParseErrorOwned {
        match self {
            CssShadowParseError::TooManyOrTooFewComponents(s) => {
                CssShadowParseErrorOwned::TooManyOrTooFewComponents((*s).to_string().into())
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
    /// Converts the owned error back into a borrowed version.
    #[must_use] pub fn to_shared(&self) -> CssShadowParseError<'_> {
        match self {
            Self::TooManyOrTooFewComponents(s) => {
                CssShadowParseError::TooManyOrTooFewComponents(s.as_str())
            }
            Self::ValueParseErr(e) => {
                CssShadowParseError::ValueParseErr(e.to_shared())
            }
            Self::ColorParseError(e) => {
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `box-shadow` value.
pub fn parse_style_box_shadow(
    input: &str,
) -> Result<StyleBoxShadow, CssShadowParseError<'_>> {
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
    if let Some((pos, color)) = parts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, p)| parse_css_color(p).ok().map(|c| (i, c)))
    {
        shadow.color = color;
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
        // Two colors: rposition picks "blue" as the color, leaving "red" which
        // fails to parse as a pixel value.
        assert!(parse_style_box_shadow("10px 5px red blue").is_err());
        assert!(parse_style_box_shadow("10% 5px").is_err()); // No percent allowed
    }
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use super::*;
    use crate::{
        codegen::format::FormatAsRustCode,
        props::basic::{
            pixel::{CssPixelValueParseError, PixelValue},
            SizeMetric,
        },
    };

    fn px(val: f32) -> PixelValueNoPercent {
        PixelValueNoPercent {
            inner: PixelValue::px(val),
        }
    }

    const fn shadow(
        offset_x: PixelValueNoPercent,
        offset_y: PixelValueNoPercent,
        blur_radius: PixelValueNoPercent,
        spread_radius: PixelValueNoPercent,
        clip_mode: BoxShadowClipMode,
        color: ColorU,
    ) -> StyleBoxShadow {
        StyleBoxShadow {
            offset_x,
            offset_y,
            blur_radius,
            spread_radius,
            clip_mode,
            color,
        }
    }

    /// Every component of a shadow, as raw f32s.
    fn numbers(s: &StyleBoxShadow) -> [f32; 4] {
        [
            s.offset_x.inner.number.get(),
            s.offset_y.inner.number.get(),
            s.blur_radius.inner.number.get(),
            s.spread_radius.inner.number.get(),
        ]
    }

    /// A representative corpus of shadows that are exactly representable in the
    /// fixed-point `FloatValue` encoding (<= 3 decimal places).
    fn round_trip_corpus() -> Vec<StyleBoxShadow> {
        vec![
            StyleBoxShadow::default(),
            shadow(
                px(1.0),
                px(2.0),
                px(0.0),
                px(0.0),
                BoxShadowClipMode::Outset,
                ColorU::BLACK,
            ),
            // blur only
            shadow(
                px(1.0),
                px(2.0),
                px(3.0),
                px(0.0),
                BoxShadowClipMode::Outset,
                ColorU::BLACK,
            ),
            // spread only: forces a "0px" blur to be printed as a placeholder
            shadow(
                px(1.0),
                px(2.0),
                px(0.0),
                px(4.0),
                BoxShadowClipMode::Outset,
                ColorU::BLACK,
            ),
            // negative offsets + inset + named color
            shadow(
                px(-5.5),
                px(-7.25),
                px(2.5),
                px(1.125),
                BoxShadowClipMode::Inset,
                ColorU::RED,
            ),
            // non-px metric
            shadow(
                PixelValueNoPercent {
                    inner: PixelValue::em(1.5),
                },
                PixelValueNoPercent {
                    inner: PixelValue::pt(2.0),
                },
                px(0.0),
                px(0.0),
                BoxShadowClipMode::Inset,
                ColorU::BLACK,
            ),
            // fully transparent black -- differs from ColorU::BLACK only in alpha
            shadow(
                px(0.0),
                px(0.0),
                px(0.0),
                px(0.0),
                BoxShadowClipMode::Outset,
                ColorU::new(0, 0, 0, 0),
            ),
            // every channel distinct, incl. a non-opaque alpha
            shadow(
                px(0.0),
                px(0.0),
                px(9.0),
                px(0.0),
                BoxShadowClipMode::Inset,
                ColorU::new(1, 2, 3, 4),
            ),
        ]
    }

    // ---------------------------------------------------------------
    // serializer: BoxShadowClipMode::fmt
    // ---------------------------------------------------------------

    #[test]
    fn clip_mode_display_outset_is_empty_inset_is_keyword() {
        // Outset is the CSS default and is deliberately NOT written out.
        assert_eq!(BoxShadowClipMode::Outset.to_string(), "");
        assert_eq!(BoxShadowClipMode::Inset.to_string(), "inset");
    }

    #[test]
    fn clip_mode_display_default_does_not_panic() {
        let default: BoxShadowClipMode = Default::default();
        assert_eq!(default, BoxShadowClipMode::Outset);
        assert_eq!(default.to_string(), "");
        // Debug (derived) must stay non-empty even though Display is empty.
        assert_eq!(format!("{:?}", BoxShadowClipMode::Outset), "Outset");
        assert_eq!(format!("{:?}", BoxShadowClipMode::Inset), "Inset");
    }

    #[test]
    fn clip_mode_display_with_format_flags_does_not_panic() {
        // The impl writes straight to the formatter, so width/fill/precision are
        // ignored rather than applied -- assert only that nothing panics and the
        // keyword survives.
        assert!(format!("{:>16}", BoxShadowClipMode::Inset).contains("inset"));
        assert!(format!("{:*<16}", BoxShadowClipMode::Inset).contains("inset"));
        assert!(format!("{:.2}", BoxShadowClipMode::Inset).contains("inset"));
        // Outset writes nothing at all, whatever the flags.
        assert_eq!(format!("{:>16}", BoxShadowClipMode::Outset), "");
    }

    #[test]
    fn clip_mode_display_is_repeatable_and_ordered() {
        // Same value must serialize identically every time.
        for _ in 0..4 {
            assert_eq!(BoxShadowClipMode::Inset.to_string(), "inset");
        }
        // Derived Ord: the default variant sorts first.
        assert!(BoxShadowClipMode::Outset < BoxShadowClipMode::Inset);
    }

    // ---------------------------------------------------------------
    // numeric: StyleBoxShadow::scale_for_dpi
    // ---------------------------------------------------------------

    fn scaled(mut s: StyleBoxShadow, factor: f32) -> StyleBoxShadow {
        s.scale_for_dpi(factor);
        s
    }

    fn all_ones() -> StyleBoxShadow {
        shadow(
            px(1.0),
            px(2.0),
            px(4.0),
            px(8.0),
            BoxShadowClipMode::Inset,
            ColorU::RED,
        )
    }

    #[test]
    fn scale_for_dpi_zero_zeroes_every_length() {
        let s = scaled(all_ones(), 0.0);
        assert_eq!(numbers(&s), [0.0, 0.0, 0.0, 0.0]);
        // -0.0 must not leak a negative zero into the fixed-point encoding.
        let neg = scaled(all_ones(), -0.0);
        assert_eq!(numbers(&neg), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(neg.offset_x.inner.number.number(), 0);
    }

    #[test]
    fn scale_for_dpi_identity_and_double() {
        let s = scaled(all_ones(), 1.0);
        assert_eq!(numbers(&s), [1.0, 2.0, 4.0, 8.0]);
        let d = scaled(all_ones(), 2.0);
        assert_eq!(numbers(&d), [2.0, 4.0, 8.0, 16.0]);
        let h = scaled(all_ones(), 0.5);
        assert_eq!(numbers(&h), [0.5, 1.0, 2.0, 4.0]);
    }

    #[test]
    fn scale_for_dpi_negative_flips_sign_deterministically() {
        let s = scaled(all_ones(), -1.5);
        assert_eq!(numbers(&s), [-1.5, -3.0, -6.0, -12.0]);
    }

    #[test]
    fn scale_for_dpi_nan_yields_zero_never_nan() {
        // `f32 as isize` saturates and maps NaN -> 0, so a NaN scale factor
        // collapses the shadow to zero instead of poisoning it with NaN.
        let s = scaled(all_ones(), f32::NAN);
        for n in numbers(&s) {
            assert!(!n.is_nan(), "NaN leaked into the fixed-point encoding");
            assert_eq!(n, 0.0);
        }
    }

    #[test]
    fn scale_for_dpi_infinity_saturates_to_finite() {
        let pos = scaled(all_ones(), f32::INFINITY);
        for n in numbers(&pos) {
            assert!(n.is_finite(), "+inf scale produced a non-finite value");
            assert!(n > 0.0);
        }

        let neg = scaled(all_ones(), f32::NEG_INFINITY);
        for n in numbers(&neg) {
            assert!(n.is_finite(), "-inf scale produced a non-finite value");
            assert!(n < 0.0);
        }
    }

    #[test]
    fn scale_for_dpi_float_extremes_do_not_panic() {
        // MAX: 1.0 * f32::MAX * 1000.0 overflows f32 -> inf -> saturates on cast.
        for n in numbers(&scaled(all_ones(), f32::MAX)) {
            assert!(n.is_finite());
            assert!(n > 0.0);
        }
        for n in numbers(&scaled(all_ones(), f32::MIN)) {
            assert!(n.is_finite());
            assert!(n < 0.0);
        }
        // Subnormal / tiny factors underflow to exactly zero (3-decimal precision).
        assert_eq!(numbers(&scaled(all_ones(), f32::MIN_POSITIVE)), [0.0; 4]);
        assert_eq!(numbers(&scaled(all_ones(), f32::EPSILON)), [0.0; 4]);
    }

    #[test]
    fn scale_for_dpi_saturation_is_stable_under_repetition() {
        // Scaling an already-saturated shadow again must stay finite (no panic,
        // no NaN, no wraparound to the opposite sign).
        let mut s = all_ones();
        for _ in 0..8 {
            s.scale_for_dpi(1e30);
            for n in numbers(&s) {
                assert!(n.is_finite());
                assert!(n > 0.0, "saturating scale wrapped to a negative value");
            }
        }
    }

    #[test]
    fn scale_for_dpi_quantizes_below_the_fixed_point_precision() {
        // FloatValue keeps 3 decimals; anything smaller truncates to zero.
        let s = scaled(all_ones(), 0.0001);
        assert_eq!(numbers(&s), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn scale_for_dpi_on_default_is_a_noop() {
        for factor in [0.0, 1.0, 2.0, -3.0, f32::NAN, f32::INFINITY, f32::MAX] {
            let s = scaled(StyleBoxShadow::default(), factor);
            assert_eq!(
                numbers(&s),
                [0.0, 0.0, 0.0, 0.0],
                "default shadow changed under scale {factor}"
            );
        }
    }

    #[test]
    fn scale_for_dpi_preserves_metric_color_and_clip_mode() {
        let mut s = shadow(
            PixelValueNoPercent {
                inner: PixelValue::em(2.0),
            },
            PixelValueNoPercent {
                inner: PixelValue::pt(3.0),
            },
            PixelValueNoPercent {
                inner: PixelValue::rem(4.0),
            },
            px(5.0),
            BoxShadowClipMode::Inset,
            ColorU::new(1, 2, 3, 4),
        );
        s.scale_for_dpi(2.0);

        // Only the *numbers* are scaled -- units are never converted.
        assert_eq!(s.offset_x.inner.metric, SizeMetric::Em);
        assert_eq!(s.offset_y.inner.metric, SizeMetric::Pt);
        assert_eq!(s.blur_radius.inner.metric, SizeMetric::Rem);
        assert_eq!(s.spread_radius.inner.metric, SizeMetric::Px);
        assert_eq!(numbers(&s), [4.0, 6.0, 8.0, 10.0]);

        // ... and the non-numeric fields are untouched.
        assert_eq!(s.clip_mode, BoxShadowClipMode::Inset);
        assert_eq!(s.color, ColorU::new(1, 2, 3, 4));
    }

    // ---------------------------------------------------------------
    // getters: CssShadowParseError::to_contained / ..Owned::to_shared
    // ---------------------------------------------------------------

    /// One error of each `CssShadowParseError` variant.
    fn error_corpus() -> Vec<CssShadowParseError<'static>> {
        vec![
            CssShadowParseError::TooManyOrTooFewComponents("1px"),
            CssShadowParseError::TooManyOrTooFewComponents(""),
            CssShadowParseError::TooManyOrTooFewComponents("\u{1F600}\u{0301} \u{202E}"),
            CssShadowParseError::ValueParseErr(CssPixelValueParseError::EmptyString),
            CssShadowParseError::ValueParseErr(CssPixelValueParseError::InvalidPixelValue("abc")),
            CssShadowParseError::ValueParseErr(CssPixelValueParseError::NoValueGiven(
                "px",
                SizeMetric::Px,
            )),
            CssShadowParseError::ValueParseErr(CssPixelValueParseError::ValueParseErr(
                "x".parse::<f32>().unwrap_err(),
                "x",
            )),
            CssShadowParseError::ValueParseErr(CssPixelValueParseError::ValueParseErr(
                "".parse::<f32>().unwrap_err(),
                "",
            )),
            CssShadowParseError::ColorParseError(parse_css_color("notacolor").unwrap_err()),
            CssShadowParseError::ColorParseError(parse_css_color("#gg").unwrap_err()),
            CssShadowParseError::ColorParseError(parse_css_color("rgb(1,2").unwrap_err()),
        ]
    }

    #[test]
    fn shadow_error_to_contained_to_shared_is_lossless() {
        for e in error_corpus() {
            let owned = e.to_contained();
            assert_eq!(owned.to_shared(), e, "borrow -> own -> borrow lost data");
        }
    }

    #[test]
    fn shadow_error_owned_to_shared_to_contained_is_lossless() {
        for e in error_corpus() {
            let owned = e.to_contained();
            assert_eq!(
                owned.to_shared().to_contained(),
                owned,
                "own -> borrow -> own lost data"
            );
        }
    }

    #[test]
    fn shadow_error_to_contained_preserves_the_input_string_verbatim() {
        for input in [
            "",
            "   ",
            "\u{1F600}",
            "e\u{0301}\u{0301}\u{0301}",
            "10px 5px 4px 3px 2px",
        ] {
            let owned = CssShadowParseError::TooManyOrTooFewComponents(input).to_contained();
            match owned {
                CssShadowParseErrorOwned::TooManyOrTooFewComponents(s) => {
                    assert_eq!(s.as_str(), input);
                }
                other => panic!("wrong variant: {other:?}"),
            }
        }
    }

    #[test]
    fn shadow_error_to_contained_survives_a_very_long_input() {
        let long = "x".repeat(200_000);
        let owned = CssShadowParseError::TooManyOrTooFewComponents(&long).to_contained();
        match owned.to_shared() {
            CssShadowParseError::TooManyOrTooFewComponents(s) => {
                assert_eq!(s.len(), 200_000);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn shadow_error_display_and_debug_are_non_empty() {
        for e in error_corpus() {
            assert!(!e.to_string().is_empty(), "empty Display for {e:?}");
            // Debug is implemented as Display for this type.
            assert_eq!(format!("{e:?}"), e.to_string());
            assert!(!format!("{:?}", e.to_contained()).is_empty());
        }
    }

    #[test]
    fn shadow_error_to_contained_round_trips_real_parser_errors() {
        // Errors as they are actually produced by the parser, not hand-built.
        for bad in ["", "1px", "abc 1px", "10% 5px", "1 2 3 4 5"] {
            let e = parse_style_box_shadow(bad).unwrap_err();
            assert_eq!(e.to_contained().to_shared(), e);
        }
    }

    // ---------------------------------------------------------------
    // parser: parse_style_box_shadow -- malformed / boundary / unicode
    // ---------------------------------------------------------------

    #[test]
    fn parse_valid_minimal_positive_control() {
        let s = parse_style_box_shadow("10px 5px").unwrap();
        assert_eq!(
            s,
            shadow(
                px(10.0),
                px(5.0),
                px(0.0),
                px(0.0),
                BoxShadowClipMode::Outset,
                ColorU::BLACK,
            )
        );
    }

    #[test]
    fn parse_empty_and_whitespace_only_input_is_err() {
        for input in ["", " ", "   ", "\t", "\n", "\t\n\r ", "\u{00a0}", "\u{3000}"] {
            let e = parse_style_box_shadow(input).unwrap_err();
            assert_eq!(
                e,
                CssShadowParseError::TooManyOrTooFewComponents(input),
                "unexpected error for {input:?}"
            );
        }
    }

    #[test]
    fn parse_garbage_is_err_and_never_panics() {
        for input in [
            "!!!",
            "@#$%^&*",
            "; drop table",
            "{}{}{}",
            "\\\\\\",
            "\0\0",
            "1px",                  // too few
            "1px 2px 3px 4px 5px",  // too many
            "inset",                // keyword only
            "red",                  // color only
            "inset red",            // keyword + color, no lengths
            "inset red 1px",        // only one length left
            "1px 2px red blue",     // two colors: "red" is left over as a length
            "inset inset 1px 2px",  // second "inset" is not removed
            "10% 5px",              // percent is rejected
            "1px 10%",
            "1px 2px 3%",
            "px px",
            "in in",
            "1px 2px 3px 4px 5px red inset",
        ] {
            assert!(
                parse_style_box_shadow(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn parse_leading_trailing_junk_is_trimmed_or_rejected_deterministically() {
        // Surrounding whitespace is absorbed by split_whitespace.
        let padded = parse_style_box_shadow("   10px    5px   ").unwrap();
        assert_eq!(padded, parse_style_box_shadow("10px 5px").unwrap());

        // Trailing punctuation is NOT stripped -- it makes the length unparseable.
        for input in ["10px 5px;", "10px, 5px", "10px 5px !important", "10px 5px}"] {
            let e = parse_style_box_shadow(input).unwrap_err();
            assert!(
                matches!(e, CssShadowParseError::ValueParseErr(_))
                    || matches!(e, CssShadowParseError::TooManyOrTooFewComponents(_)),
                "unexpected error kind for {input:?}: {e:?}"
            );
        }
    }

    #[test]
    fn parse_component_count_boundaries() {
        assert!(parse_style_box_shadow("1px").is_err()); // 1 -> too few
        assert!(parse_style_box_shadow("1px 2px").is_ok()); // 2 -> ok
        assert!(parse_style_box_shadow("1px 2px 3px").is_ok()); // 3 -> ok
        assert!(parse_style_box_shadow("1px 2px 3px 4px").is_ok()); // 4 -> ok
        assert!(parse_style_box_shadow("1px 2px 3px 4px 5px").is_err()); // 5 -> too many

        // The `inset` keyword and the color do not count toward the 2..=4 budget.
        assert!(parse_style_box_shadow("inset red 1px 2px 3px 4px").is_ok());
        // ... but five lengths are still too many, even with them present.
        assert!(parse_style_box_shadow("inset red 1px 2px 3px 4px 5px").is_err());
    }

    #[test]
    fn parse_is_insensitive_to_keyword_and_color_position() {
        let expected = shadow(
            px(1.0),
            px(2.0),
            px(0.0),
            px(0.0),
            BoxShadowClipMode::Inset,
            ColorU::RED,
        );
        for input in [
            "inset red 1px 2px",
            "red inset 1px 2px",
            "1px 2px red inset",
            "1px 2px inset red",
            "red 1px 2px inset",
            "1px red 2px inset",
            "inset 1px red 2px",
        ] {
            assert_eq!(
                parse_style_box_shadow(input).unwrap(),
                expected,
                "position of inset/color changed the result for {input:?}"
            );
        }
    }

    #[test]
    fn parse_accepts_every_color_syntax() {
        for (input, expected) in [
            ("1px 2px #888", ColorU::new_rgb(0x88, 0x88, 0x88)),
            ("1px 2px #ff0000", ColorU::RED),
            ("1px 2px #ff0000ff", ColorU::RED),
            ("1px 2px #f00f", ColorU::RED),
            ("1px 2px rgb(255,0,0)", ColorU::RED),
            ("1px 2px rgba(255,0,0,1.0)", ColorU::RED),
            ("1px 2px red", ColorU::RED),
            ("1px 2px RED", ColorU::RED),
            ("1px 2px transparent", ColorU::new(0, 0, 0, 0)),
        ] {
            let s = parse_style_box_shadow(input).unwrap();
            assert_eq!(s.color, expected, "wrong color for {input:?}");
            assert_eq!(s.offset_x, px(1.0));
            assert_eq!(s.offset_y, px(2.0));
        }
    }

    #[test]
    fn parse_bare_zero_is_a_pixel_value_not_a_color() {
        let s = parse_style_box_shadow("0 0").unwrap();
        assert_eq!(s.offset_x, px(0.0));
        assert_eq!(s.offset_y, px(0.0));
        assert_eq!(s.offset_x.inner.metric, SizeMetric::Px);
        assert_eq!(s.color, ColorU::BLACK, "a bare 0 was eaten by the color parser");
    }

    #[test]
    fn parse_boundary_numbers_are_finite_and_defined() {
        // Signed zeroes collapse to a single +0 in the fixed-point encoding.
        let zeroes = parse_style_box_shadow("-0 -0").unwrap();
        assert_eq!(numbers(&zeroes)[0], 0.0);
        assert_eq!(numbers(&zeroes)[1], 0.0);
        assert!(!numbers(&zeroes)[0].is_sign_negative());

        // Huge finite values saturate on the f32 -> isize cast rather than wrap.
        for input in [
            "1e30px 1e30px",
            "9223372036854775807px 9223372036854775807px", // i64::MAX
            "340282350000000000000000000000000000000px 1px", // ~f32::MAX
        ] {
            let s = parse_style_box_shadow(input).unwrap();
            let n = numbers(&s)[0];
            assert!(n.is_finite(), "{input:?} produced a non-finite offset");
            assert!(n > 0.0, "{input:?} saturated to the wrong sign");
        }
        let neg = parse_style_box_shadow("-1e30px 1px").unwrap();
        assert!(numbers(&neg)[0].is_finite() && numbers(&neg)[0] < 0.0);

        // Tiny values underflow to exactly zero (3 decimals of precision).
        let tiny = parse_style_box_shadow("1e-30px 0.00001px").unwrap();
        assert_eq!(numbers(&tiny)[0], 0.0);
        assert_eq!(numbers(&tiny)[1], 0.0);
    }

    #[test]
    fn parse_nan_and_infinity_tokens_are_defined_not_poisonous() {
        // f32::from_str accepts "NaN"/"inf", so these reach the fixed-point cast.
        // NaN as isize == 0, so the value collapses to zero rather than staying NaN.
        let nan = parse_style_box_shadow("NaN NaN").unwrap();
        for n in numbers(&nan) {
            assert!(!n.is_nan(), "NaN survived into a parsed shadow");
            assert_eq!(n, 0.0);
        }
        let nan_px = parse_style_box_shadow("NaNpx 1px").unwrap();
        assert_eq!(numbers(&nan_px)[0], 0.0);

        // inf saturates to isize::MAX / 1000 -- finite, signed correctly.
        let inf = parse_style_box_shadow("inf 1px").unwrap();
        assert!(numbers(&inf)[0].is_finite() && numbers(&inf)[0] > 0.0);
        let neg_inf = parse_style_box_shadow("-inf 1px").unwrap();
        assert!(numbers(&neg_inf)[0].is_finite() && numbers(&neg_inf)[0] < 0.0);
        let infinity = parse_style_box_shadow("1px infinity").unwrap();
        assert!(numbers(&infinity)[1].is_finite());

        // A bare "in" is the inch metric with no value -> Err, not a panic.
        assert!(parse_style_box_shadow("in 1px").is_err());
    }

    #[test]
    fn parse_never_yields_a_percent_metric() {
        for input in [
            "1px 2px",
            "0 0",
            "1em 2rem 3pt 4in",
            "5vw 5vh 5vmin 5vmax",
            "1cm 2mm",
            "inset 1px 2px red",
        ] {
            let s = parse_style_box_shadow(input).unwrap();
            for m in [
                s.offset_x.inner.metric,
                s.offset_y.inner.metric,
                s.blur_radius.inner.metric,
                s.spread_radius.inner.metric,
            ] {
                assert_ne!(m, SizeMetric::Percent, "percent metric leaked from {input:?}");
            }
        }
    }

    #[test]
    fn parse_unicode_input_does_not_panic() {
        // parse_color_no_hash indexes by BYTE length, so multi-byte tokens behind
        // a '#' must error instead of slicing through a char boundary.
        for input in [
            "\u{1F600}",
            "\u{1F600} \u{1F600}",
            "1px 2px \u{1F600}",
            "1px 2px #\u{1F600}",  // 4-byte char -> hits the len==4 hex branch
            "1px 2px #\u{e9}1",    // 3 bytes -> hits the len==3 hex branch
            "1px 2px #\u{e9}\u{e9}\u{e9}\u{e9}", // 8 bytes -> hits the len==8 branch
            "e\u{0301}\u{0301} 1px",
            "\u{202E}1px 2px",
            "\u{0661}px \u{0662}px", // arabic-indic digits
            "１px ２px",             // fullwidth digits
            "1px 2px \u{fffd}",
        ] {
            let _ = parse_style_box_shadow(input); // must not panic
        }

        // Unicode whitespace still splits tokens, so this is a valid shadow.
        let nbsp = parse_style_box_shadow("10px\u{00a0}5px").unwrap();
        assert_eq!(nbsp, parse_style_box_shadow("10px 5px").unwrap());
    }

    #[test]
    fn parse_extremely_long_input_terminates() {
        // 50k length tokens: rejected on the component count, no hang.
        let many = "1px ".repeat(50_000);
        assert!(matches!(
            parse_style_box_shadow(&many),
            Err(CssShadowParseError::TooManyOrTooFewComponents(_))
        ));

        // A single 1M-char garbage token.
        let long_garbage = "a".repeat(1_000_000);
        assert!(parse_style_box_shadow(&long_garbage).is_err());

        // A single token of 50k digits: f32 parses it as inf, which then saturates.
        let huge = format!("{}px 1px", "9".repeat(50_000));
        let s = parse_style_box_shadow(&huge).unwrap();
        assert!(numbers(&s)[0].is_finite());
        assert!(numbers(&s)[0] > 0.0);

        // A very long *valid* input padded with whitespace.
        let padded = format!("{}1px 2px{}", " ".repeat(100_000), " ".repeat(100_000));
        assert_eq!(
            parse_style_box_shadow(&padded).unwrap(),
            parse_style_box_shadow("1px 2px").unwrap()
        );
    }

    #[test]
    fn parse_deeply_nested_input_does_not_stack_overflow() {
        let open = "(".repeat(10_000);
        let nested = format!("{}{}", open, ")".repeat(10_000));
        assert!(parse_style_box_shadow(&nested).is_err());
        assert!(parse_style_box_shadow(&open).is_err());
        assert!(parse_style_box_shadow(&format!("1px 2px rgb{nested}")).is_err());
        assert!(parse_style_box_shadow(&format!("1px 2px {}", "[".repeat(10_000))).is_err());
    }

    // ---------------------------------------------------------------
    // round-trip: print_as_css_value <-> parse_style_box_shadow
    // ---------------------------------------------------------------

    #[test]
    fn print_then_parse_is_the_identity() {
        for original in round_trip_corpus() {
            let printed = original.print_as_css_value();
            let reparsed = parse_style_box_shadow(&printed)
                .unwrap_or_else(|e| panic!("printed {printed:?} does not re-parse: {e:?}"));
            assert_eq!(reparsed, original, "round-trip changed {printed:?}");
        }
    }

    #[test]
    fn parse_then_print_then_parse_is_idempotent() {
        for input in [
            "10px 5px",
            "5px 10px 20px",
            "2px 2px 2px 1px rgba(0,0,0,0.2)",
            "inset 0 0 10px #000",
            "5px 1em red inset",
            "1px 2px transparent",
            "-3px -4px 0 2px #12345678",
        ] {
            let first = parse_style_box_shadow(input).unwrap();
            let printed = first.print_as_css_value();
            let second = parse_style_box_shadow(&printed).unwrap();
            assert_eq!(first, second, "{input:?} -> {printed:?} was not idempotent");
            // Printing is stable: same value, same string.
            assert_eq!(printed, second.print_as_css_value());
        }
    }

    #[test]
    fn print_omits_defaults_for_brevity() {
        let default = StyleBoxShadow::default();
        assert_eq!(default.print_as_css_value(), "0px 0px");

        // A black shadow never writes a color...
        let black = shadow(
            px(1.0),
            px(2.0),
            px(3.0),
            px(0.0),
            BoxShadowClipMode::Outset,
            ColorU::BLACK,
        );
        assert_eq!(black.print_as_css_value(), "1px 2px 3px");
        assert!(!black.print_as_css_value().contains('#'));

        // ...and an outset shadow never writes the `inset` keyword.
        assert!(!black.print_as_css_value().contains("inset"));
    }

    #[test]
    fn print_writes_inset_first_and_color_last() {
        let s = shadow(
            px(1.0),
            px(2.0),
            px(0.0),
            px(0.0),
            BoxShadowClipMode::Inset,
            ColorU::RED,
        );
        assert_eq!(s.print_as_css_value(), "inset 1px 2px #ff0000ff");
    }

    #[test]
    fn print_emits_a_placeholder_blur_when_only_the_spread_is_set() {
        // A spread cannot be positional without a blur before it, so a zero blur
        // must still be written -- otherwise the spread would re-parse as a blur.
        let s = shadow(
            px(1.0),
            px(2.0),
            px(0.0),
            px(4.0),
            BoxShadowClipMode::Outset,
            ColorU::BLACK,
        );
        assert_eq!(s.print_as_css_value(), "1px 2px 0px 4px");
        let reparsed = parse_style_box_shadow(&s.print_as_css_value()).unwrap();
        assert_eq!(reparsed.blur_radius, px(0.0));
        assert_eq!(reparsed.spread_radius, px(4.0));
    }

    #[test]
    fn print_of_extreme_values_does_not_panic() {
        // Saturated / NaN-scaled shadows must still serialize to *something*.
        for factor in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, 1e30] {
            let s = scaled(all_ones(), factor);
            let printed = s.print_as_css_value();
            assert!(!printed.is_empty());
            assert!(!printed.contains("NaN"), "NaN reached the CSS output: {printed}");
            assert!(!printed.contains("inf"), "inf reached the CSS output: {printed}");
        }
    }

    #[test]
    fn format_as_rust_code_is_well_formed_for_extremes() {
        for s in round_trip_corpus() {
            let code = s.format_as_rust_code(0);
            assert!(code.starts_with("StyleBoxShadow {"));
            assert!(code.contains("offset_x:"));
            assert!(code.contains("offset_y:"));
            assert!(code.contains("blur_radius:"));
            assert!(code.contains("spread_radius:"));
            assert!(code.contains("clip_mode: BoxShadowClipMode::"));
            assert!(code.contains("color:"));
        }
        // Indentation is applied, and extreme values do not panic the formatter.
        let inset = scaled(all_ones(), f32::INFINITY);
        let code = inset.format_as_rust_code(3);
        assert!(code.contains("clip_mode: BoxShadowClipMode::Inset"));
        assert!(code.contains("            offset_x:"));
    }

    // ---------------------------------------------------------------
    // predicates / invariants on StyleBoxShadow itself
    // ---------------------------------------------------------------

    #[test]
    fn default_shadow_is_an_opaque_black_outset_at_the_origin() {
        let d = StyleBoxShadow::default();
        assert_eq!(d.clip_mode, BoxShadowClipMode::Outset);
        assert_eq!(d.color, ColorU::BLACK);
        assert_eq!(numbers(&d), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(d.offset_x.inner.metric, SizeMetric::Px);
        // The parser's baseline must agree with Default.
        assert_eq!(parse_style_box_shadow("0 0").unwrap(), d);
    }

    #[test]
    fn equality_is_component_wise() {
        let base = all_ones();
        assert_eq!(base, base);
        assert_ne!(base, StyleBoxShadow::default());

        let mut clip = base;
        clip.clip_mode = BoxShadowClipMode::Outset;
        assert_ne!(base, clip, "clip_mode is ignored by PartialEq");

        let mut color = base;
        color.color = ColorU::BLACK;
        assert_ne!(base, color, "color is ignored by PartialEq");

        let mut spread = base;
        spread.spread_radius = px(9.0);
        assert_ne!(base, spread, "spread_radius is ignored by PartialEq");
    }
}
