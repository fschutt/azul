//! CSS properties for styling text selections (`-azul-selection-*`).
//!
//! Defines the following properties (analogous to the `::selection` pseudo-element):
//!
//! - `-azul-selection-background-color` ([`SelectionBackgroundColor`])
//! - `-azul-selection-color` ([`SelectionColor`])
//! - `-azul-selection-radius` ([`SelectionRadius`])

use alloc::string::String;

use crate::props::{
    basic::color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
    formatter::PrintAsCssValue,
};

/// Default selection highlight background — light blue, similar to macOS.
const DEFAULT_SELECTION_BG: ColorU = ColorU::new(173, 214, 255, 255);

// --- -azul-selection-background-color ---

/// Parsed value for the `-azul-selection-background-color` CSS property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionBackgroundColor {
    pub inner: ColorU,
}

impl Default for SelectionBackgroundColor {
    fn default() -> Self {
        Self {
            inner: DEFAULT_SELECTION_BG,
        }
    }
}

impl PrintAsCssValue for SelectionBackgroundColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl crate::codegen::format::FormatAsRustCode for SelectionBackgroundColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "SelectionBackgroundColor {{ inner: {} }}",
            crate::codegen::format::format_color_value(&self.inner)
        )
    }
}

/// Parses a `-azul-selection-background-color` CSS value.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `selection-background-color` value.
pub fn parse_selection_background_color(
    input: &str,
) -> Result<SelectionBackgroundColor, CssColorParseError<'_>> {
    parse_css_color(input).map(|inner| SelectionBackgroundColor { inner })
}

// --- -azul-selection-color ---

/// Parsed value for the `-azul-selection-color` CSS property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionColor {
    pub inner: ColorU,
}

impl Default for SelectionColor {
    fn default() -> Self {
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl PrintAsCssValue for SelectionColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl crate::codegen::format::FormatAsRustCode for SelectionColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "SelectionColor {{ inner: {} }}",
            crate::codegen::format::format_color_value(&self.inner)
        )
    }
}

/// Parses a `-azul-selection-color` CSS value.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `selection-color` value.
pub fn parse_selection_color(input: &str) -> Result<SelectionColor, CssColorParseError<'_>> {
    parse_css_color(input).map(|inner| SelectionColor { inner })
}

// --- -azul-selection-radius ---

use crate::props::basic::{
    pixel::{parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    SizeMetric,
};

/// Parsed value for the `-azul-selection-radius` CSS property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionRadius {
    pub inner: PixelValue,
}

impl Default for SelectionRadius {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl PrintAsCssValue for SelectionRadius {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

impl crate::codegen::format::FormatAsRustCode for SelectionRadius {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // Use the Display implementation of PixelValue to get a string like "5px" or "1em"
        format!(
            "SelectionRadius {{ inner: PixelValue::from_metric(SizeMetric::{:?}, {}) }}",
            self.inner.metric,
            self.inner.number.get()
        )
    }
}

/// Parses a `-azul-selection-radius` CSS value.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `selection-radius` value.
pub fn parse_selection_radius(input: &str) -> Result<SelectionRadius, CssPixelValueParseError<'_>> {
    parse_pixel_value(input).map(|inner| SelectionRadius { inner })
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;
    use crate::codegen::format::FormatAsRustCode;

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    /// Every metric whose `Display` string round-trips through `parse_pixel_value`.
    ///
    /// `Vmin` is deliberately absent — see
    /// [`known_bug_vmin_radius_is_rejected_by_metric_table_order`].
    const ROUND_TRIPPABLE_METRICS: [SizeMetric; 11] = [
        SizeMetric::Px,
        SizeMetric::Pt,
        SizeMetric::Em,
        SizeMetric::Rem,
        SizeMetric::In,
        SizeMetric::Cm,
        SizeMetric::Mm,
        SizeMetric::Percent,
        SizeMetric::Vw,
        SizeMetric::Vh,
        SizeMetric::Vmax,
    ];

    /// Inputs that must never panic in *any* of the three parsers in this module,
    /// regardless of which one they were aimed at.
    fn hostile_corpus() -> Vec<String> {
        vec![
            String::new(),
            " ".to_string(),
            "\t\n\r\u{b}\u{c}".to_string(),
            "\u{a0}".to_string(),   // NBSP: is White_Space, so trim() eats it
            "\u{200b}".to_string(), // ZWSP: NOT White_Space, survives the trim
            "\0".to_string(),
            "#\0\0\0".to_string(),
            ";".to_string(),
            "}{".to_string(),
            ")rgb(".to_string(),
            "#".to_string(),
            "##".to_string(),
            "#zzz".to_string(),
            "#-fff000f".to_string(), // from_str_radix must not accept a sign here
            "rgb(".to_string(),
            "rgb()".to_string(),
            "rgb(1,2)".to_string(),
            "rgb(-1,-1,-1)".to_string(),
            "rgb(999,999,999)".to_string(),
            "rgba(0,0,0,NaN)".to_string(),
            "rgba(255,255,255,inf)".to_string(),
            "hsl(400,-10%,999%)".to_string(),
            "-".to_string(),
            "--".to_string(),
            "+".to_string(),
            "e".to_string(),
            "E9".to_string(),
            "NaN".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "infinity".to_string(),
            "0x10px".to_string(),
            "px".to_string(),
            "vmin".to_string(),
            "5 5px".to_string(),
            "5px;".to_string(),
            "%".to_string(),
            "-0".to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            format!("{}px", i64::MAX),
            "1e400px".to_string(),
            "\u{1F600}".to_string(),
            "#\u{1F600}".to_string(),           // 4 *bytes* -> hits the #rgba branch
            "#\u{e9}1".to_string(),             // 3 *bytes* -> hits the #rgb branch
            "a\u{0301}\u{0301}\u{0301}".to_string(), // stacked combining marks
            "\u{202e}der".to_string(),          // RTL override
            "\u{130}".to_string(),              // dotted capital I: to_lowercase() expands
            "RED".to_string(),
            "red red".to_string(),
        ]
    }

    #[test]
    fn hostile_inputs_never_panic_in_any_selection_parser() {
        for input in hostile_corpus() {
            let _ = parse_selection_background_color(&input);
            let _ = parse_selection_color(&input);

            // Anything the radius parser *accepts* must be a finite fixed-point
            // value: `FloatValue` stores `value * 1000` in an isize, and the `as`
            // cast saturates (NaN -> 0, +-inf -> isize::MAX/MIN). No input may
            // smuggle a NaN/infinite length into the layout engine.
            if let Ok(r) = parse_selection_radius(&input) {
                assert!(
                    r.inner.number.get().is_finite(),
                    "{input:?} produced a non-finite radius"
                );
            }
        }
    }

    // --- empty / whitespace ---

    #[test]
    fn empty_and_whitespace_only_input_is_rejected() {
        // All of these trim to "" (U+00A0 and U+2003 have White_Space=yes).
        for blank in ["", " ", "   ", "\t\n", "\r\n\t ", "\u{a0}", "\u{2003}"] {
            assert!(
                matches!(
                    parse_selection_background_color(blank),
                    Err(CssColorParseError::EmptyInput)
                ),
                "{blank:?} should be EmptyInput"
            );
            assert!(
                matches!(
                    parse_selection_color(blank),
                    Err(CssColorParseError::EmptyInput)
                ),
                "{blank:?} should be EmptyInput"
            );
            assert!(
                matches!(
                    parse_selection_radius(blank),
                    Err(CssPixelValueParseError::EmptyString)
                ),
                "{blank:?} should be EmptyString"
            );
        }

        // ZWSP is *not* White_Space, so it is not trimmed away and must be
        // rejected as a value rather than reported as "empty".
        assert!(parse_selection_color("\u{200b}").is_err());
        assert!(parse_selection_radius("\u{200b}").is_err());
    }

    // --- garbage / malformed ---

    #[test]
    fn malformed_color_input_is_rejected_without_panicking() {
        for bad in [
            "#", "##", "#f", "#ff", "#fffff", "#zzz", "#gggggg", "rgb(", "rgb()", "rgb(1,2)",
            "rgb(1,2,3", "rgba(1,2,3)", "hsl(1,2)", "not-a-color", "}{", ";", ")rgb(",
            "rgb (0,0,0)", // CSS forbids a space before the paren
        ] {
            assert!(
                parse_selection_background_color(bad).is_err(),
                "{bad:?} was accepted as a background color"
            );
            assert!(
                parse_selection_color(bad).is_err(),
                "{bad:?} was accepted as a color"
            );
        }
    }

    #[test]
    fn malformed_radius_input_is_rejected_without_panicking() {
        for bad in [
            "px", "em", "%", "5 5px", "5px;", "px5", "5pxpx", "--5px", "5,px", "#5px", "5 px x",
            "0x10px", "e", "+",
        ] {
            assert!(
                parse_selection_radius(bad).is_err(),
                "{bad:?} was accepted as a radius"
            );
        }

        // A bare unit reports "the number is missing", not a generic parse error.
        assert!(matches!(
            parse_selection_radius("px"),
            Err(CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px))
        ));
    }

    // --- leading / trailing junk ---

    #[test]
    fn surrounding_whitespace_is_trimmed_but_trailing_junk_is_rejected() {
        // Trimmed, deterministically.
        assert_eq!(
            parse_selection_background_color("  #ff0000ff  ").unwrap().inner,
            ColorU::new(255, 0, 0, 255)
        );
        assert_eq!(
            parse_selection_color("\t red \n").unwrap().inner,
            ColorU::new(255, 0, 0, 255)
        );
        assert_eq!(
            parse_selection_radius("   5px \n").unwrap().inner,
            PixelValue::px(5.0)
        );
        // Internal whitespace between number and unit is trimmed too.
        assert_eq!(
            parse_selection_radius("5   px").unwrap().inner,
            PixelValue::px(5.0)
        );

        // Rejected: a valid value followed by junk is never silently truncated.
        for junk in ["#ff0000;", "red;", "red garbage", "#ff0000ff extra"] {
            assert!(
                parse_selection_color(junk).is_err(),
                "{junk:?} was silently truncated to a valid color"
            );
        }
        for junk in ["5px;", "5px garbage", "5px 6px"] {
            assert!(
                parse_selection_radius(junk).is_err(),
                "{junk:?} was silently truncated to a valid radius"
            );
        }
    }

    // --- extremely long / deeply nested ---

    #[test]
    fn extremely_long_input_neither_panics_nor_hangs() {
        let long_hex = format!("#{}", "f".repeat(1_000_000));
        assert!(parse_selection_background_color(&long_hex).is_err());
        assert!(parse_selection_color(&long_hex).is_err());

        let long_word = "z".repeat(1_000_000);
        assert!(parse_selection_color(&long_word).is_err());
        assert!(parse_selection_radius(&long_word).is_err());

        // 1M digits overflow f32 to +inf, which the fixed-point cast then
        // saturates to isize::MAX -> a huge but *finite* length.
        let long_number = format!("{}px", "9".repeat(1_000_000));
        let r = parse_selection_radius(&long_number).unwrap();
        assert!(r.inner.number.get().is_finite());
        assert_eq!(r.inner, PixelValue::px(f32::INFINITY));
    }

    #[test]
    fn deeply_nested_input_does_not_stack_overflow() {
        let open = "(".repeat(10_000);
        assert!(parse_selection_color(&open).is_err());
        assert!(parse_selection_radius(&open).is_err());

        let unclosed = "rgb(".repeat(10_000);
        assert!(parse_selection_background_color(&unclosed).is_err());

        let balanced = format!("{}{}", "rgb(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_selection_background_color(&balanced).is_err());
    }

    // --- unicode ---

    #[test]
    fn unicode_input_is_rejected_without_slicing_through_a_char() {
        // `parse_color_no_hash` dispatches on the *byte* length, so multi-byte
        // input can reach the 3-/4-byte hex branches. It must error there rather
        // than index into the middle of a char.
        for u in [
            "#\u{e9}1",       // 2-byte é + '1' == 3 bytes
            "#\u{1F600}",     // 4-byte emoji == the #rgba branch
            "#\u{e9}\u{e9}\u{e9}", // 6 bytes == the from_str_radix branch
            "\u{1F600}",
            "a\u{0301}\u{0301}",
            "\u{202e}red",
            "\u{130}", // to_lowercase() expands this to two chars
            "r\u{e9}d",
        ] {
            assert!(
                parse_selection_background_color(u).is_err(),
                "{u:?} was accepted as a color"
            );
            assert!(parse_selection_color(u).is_err(), "{u:?} was accepted");
            assert!(parse_selection_radius(u).is_err(), "{u:?} was accepted");
        }
    }

    // --- numeric boundaries / saturation ---

    #[test]
    fn radius_nan_and_infinity_saturate_instead_of_leaking_into_layout() {
        // LENIENCY (worth knowing): `parse_pixel_value` falls back to
        // `str::parse::<f32>()`, which accepts "NaN"/"inf"/"infinity". CSS does
        // not. The values are at least clamped by the fixed-point cast, so no
        // NaN/inf ever reaches layout — that saturation is what is pinned here.
        let nan = parse_selection_radius("NaN").unwrap();
        assert_eq!(nan.inner.number.get(), 0.0, "NaN must collapse to 0");
        assert!(nan.inner.number.get().is_finite());

        let pos_inf = parse_selection_radius("inf").unwrap();
        assert!(pos_inf.inner.number.get().is_finite());
        assert!(pos_inf.inner.number.get() > 0.0);

        let neg_inf = parse_selection_radius("-inf").unwrap();
        assert!(neg_inf.inner.number.get().is_finite());
        assert!(neg_inf.inner.number.get() < 0.0);

        // Every route to +inf saturates to the *same* clamped value.
        assert_eq!(pos_inf.inner, PixelValue::px(f32::INFINITY));
        assert_eq!(
            parse_selection_radius("1e40px").unwrap().inner,
            PixelValue::px(f32::INFINITY)
        );
        assert_eq!(
            parse_selection_radius(&format!("{}px", i64::MAX))
                .unwrap()
                .inner,
            PixelValue::px(f32::INFINITY)
        );
        assert_eq!(
            parse_selection_radius(&format!("{}px", i64::MIN))
                .unwrap()
                .inner,
            PixelValue::px(f32::NEG_INFINITY)
        );
    }

    #[test]
    fn radius_zero_boundaries_are_normalized() {
        assert_eq!(parse_selection_radius("0px").unwrap().inner, PixelValue::px(0.0));
        // -0.0 * 1000 -> -0.0 -> `as isize` -> 0: no distinct negative zero survives.
        assert_eq!(
            parse_selection_radius("-0px").unwrap().inner,
            parse_selection_radius("0px").unwrap().inner
        );
        assert_eq!(
            parse_selection_radius("-0.0px").unwrap().inner,
            PixelValue::zero()
        );

        // Sub-milli values truncate to zero (the fixed-point grid is 1/1000).
        assert_eq!(
            parse_selection_radius("1e-45px").unwrap().inner.number.get(),
            0.0
        );
        assert_eq!(
            parse_selection_radius("0.0001px").unwrap().inner.number.get(),
            0.0
        );
        // ...but the smallest representable step survives intact.
        assert_eq!(
            parse_selection_radius("0.001px").unwrap().inner.number.get(),
            0.001
        );
    }

    #[test]
    fn radius_accepts_unitless_numbers_which_css_would_reject() {
        // LENIENCY: CSS only allows a unitless length for `0`. This parser treats
        // *any* bare number as px. Pinned so a future tightening is a visible,
        // deliberate change rather than a silent one.
        assert_eq!(parse_selection_radius("12").unwrap().inner, PixelValue::px(12.0));
        assert_eq!(parse_selection_radius("-7.5").unwrap().inner, PixelValue::px(-7.5));
        assert_eq!(parse_selection_radius("1e3").unwrap().inner, PixelValue::px(1000.0));
    }

    #[test]
    fn color_component_boundaries_are_enforced() {
        assert_eq!(
            parse_selection_color("rgb(0,0,0)").unwrap().inner,
            ColorU::new(0, 0, 0, 255)
        );
        assert_eq!(
            parse_selection_color("rgb(255,255,255)").unwrap().inner,
            ColorU::new(255, 255, 255, 255)
        );
        // Out-of-range / negative / non-numeric components must not wrap around.
        for bad in [
            "rgb(256,0,0)",
            "rgb(-1,0,0)",
            "rgb(999999999999999999999,0,0)",
            "rgba(0,0,0,2.5)",
            "rgba(0,0,0,-1)",
        ] {
            assert!(
                parse_selection_background_color(bad).is_err(),
                "{bad:?} was accepted — a component wrapped instead of erroring"
            );
        }
    }

    // --- round-trip: encode == decode ---

    #[test]
    fn background_color_round_trips_through_print_as_css_value() {
        for c in [
            ColorU::new(0, 0, 0, 0),
            ColorU::new(255, 255, 255, 255),
            ColorU::new(0, 0, 0, 255),
            ColorU::new(173, 214, 255, 255),
            ColorU::new(1, 2, 3, 4),
            ColorU::new(254, 253, 252, 251),
            ColorU::new(255, 0, 128, 1),
            SelectionBackgroundColor::default().inner,
        ] {
            let v = SelectionBackgroundColor { inner: c };
            let printed = v.print_as_css_value();
            let reparsed = parse_selection_background_color(&printed)
                .unwrap_or_else(|e| panic!("{printed:?} did not re-parse: {e:?}"));
            assert_eq!(reparsed, v, "round-trip changed the value via {printed:?}");
            // ...and printing is idempotent (a fixpoint, not a drift).
            assert_eq!(reparsed.print_as_css_value(), printed);
        }
    }

    #[test]
    fn selection_color_round_trips_through_print_as_css_value() {
        for c in [
            ColorU::new(0, 0, 0, 0),
            ColorU::new(255, 255, 255, 255),
            ColorU::new(18, 52, 86, 120),
            ColorU::new(255, 0, 0, 255),
            SelectionColor::default().inner,
        ] {
            let v = SelectionColor { inner: c };
            let printed = v.print_as_css_value();
            let reparsed = parse_selection_color(&printed)
                .unwrap_or_else(|e| panic!("{printed:?} did not re-parse: {e:?}"));
            assert_eq!(reparsed, v);
            assert_eq!(reparsed.print_as_css_value(), printed);
        }
    }

    #[test]
    fn every_byte_of_the_color_channels_survives_the_round_trip() {
        // The 8-digit hex writer and the from_str_radix reader must agree on
        // channel order for *every* channel value, not just the pretty ones.
        for b in 0u8..=255 {
            let c = ColorU::new(b, 255 - b, b.wrapping_mul(3), 255 - b / 2);
            let v = SelectionColor { inner: c };
            assert_eq!(
                parse_selection_color(&v.print_as_css_value()).unwrap(),
                v,
                "channel round-trip failed for {c:?}"
            );
        }
    }

    #[test]
    fn radius_round_trips_for_every_metric_except_vmin() {
        for metric in ROUND_TRIPPABLE_METRICS {
            for value in [0.0_f32, 1.0, 1.5, 12.0, -3.25, 0.001, -0.5, 999.999] {
                let v = SelectionRadius {
                    inner: PixelValue::from_metric(metric, value),
                };
                let printed = v.print_as_css_value();
                let reparsed = parse_selection_radius(&printed)
                    .unwrap_or_else(|e| panic!("{printed:?} did not re-parse: {e:?}"));
                assert_eq!(
                    reparsed, v,
                    "round-trip changed {value} {metric:?} via {printed:?}"
                );
                assert_eq!(reparsed.inner.metric, metric);
                assert_eq!(reparsed.print_as_css_value(), printed);
            }
        }
    }

    /// KNOWN BUG (root cause characterized in `props::basic::pixel`): the metric
    /// table in `parse_pixel_value` tests the `"in"` suffix *before* `"vmin"`, so
    /// `5vmin` strips to `5vm`, which is not an f32. Every `vmin` selection radius
    /// is therefore rejected outright, even though it is valid CSS.
    ///
    /// WHEN pixel.rs IS FIXED (longest-suffix-first, or move vmax/vmin ahead of
    /// "in"), this test fails — replace it with the positive assertion:
    ///     assert_eq!(parse_selection_radius("5vmin").unwrap().inner.metric, SizeMetric::Vmin);
    /// and add `SizeMetric::Vmin` to `ROUND_TRIPPABLE_METRICS`.
    #[test]
    fn known_bug_vmin_radius_is_rejected_by_metric_table_order() {
        // FIXED (as this pin's own message instructed): the metric-order bug is fixed,
        // so "5vmin" now parses to SizeMetric::Vmin.
        assert_eq!(
            parse_selection_radius("5vmin").unwrap().inner.metric,
            SizeMetric::Vmin
        );

        // And it now round-trips instead of being print-only:
        let v = SelectionRadius {
            inner: PixelValue::from_metric(SizeMetric::Vmin, 5.0),
        };
        assert_eq!(v.print_as_css_value(), "5vmin");
        assert_eq!(
            parse_selection_radius(&v.print_as_css_value()).unwrap().inner.metric,
            SizeMetric::Vmin
        );

        // The sibling viewport units are fine — only the unit that *ends in* an
        // earlier metric is shadowed, which is what makes this easy to miss.
        assert_eq!(
            parse_selection_radius("5vmax").unwrap().inner.metric,
            SizeMetric::Vmax
        );
        assert_eq!(
            parse_selection_radius("5vw").unwrap().inner.metric,
            SizeMetric::Vw
        );
        assert_eq!(
            parse_selection_radius("5vh").unwrap().inner.metric,
            SizeMetric::Vh
        );
        assert_eq!(
            parse_selection_radius("5in").unwrap().inner.metric,
            SizeMetric::In
        );
    }

    /// KNOWN BUG: the 6-/8-digit hex branches parse with `u32::from_str_radix`,
    /// which accepts a leading `+`. So `#+fff000f` is accepted as a color even
    /// though `+` is not a hex digit and this is not valid CSS. The 3-/4-digit
    /// branches use a per-byte hex decoder and correctly reject it.
    ///
    /// WHEN color.rs IS FIXED (reject any non-hex-digit byte before the radix
    /// conversion), this test fails — flip the two `unwrap()`s to `is_err()`.
    #[test]
    fn hex_color_rejects_a_leading_plus_sign() {
        // FIXED (as this pin's own message instructed): the 6-/8-digit branches now
        // reject any non-hex-digit byte before the radix conversion, so a leading '+'
        // (which u32::from_str_radix used to swallow) is an error like every other
        // non-hex character.
        assert!(parse_selection_color("#+fff000f").is_err());
        assert!(parse_selection_background_color("#+fff00").is_err());

        // A leading '-' was already rejected (unsigned from_str_radix refuses it), and
        // the short branches reject '+' too via the per-byte hex decoder.
        assert!(parse_selection_color("#-fff000f").is_err());
        assert!(parse_selection_color("#+ff").is_err());
        assert!(parse_selection_color("#+fff").is_err());
    }

    // --- defaults / getters / invariants ---

    #[test]
    fn defaults_are_the_documented_values_and_re_parse_to_themselves() {
        assert_eq!(
            SelectionBackgroundColor::default().inner,
            ColorU::new(173, 214, 255, 255)
        );
        assert_eq!(SelectionBackgroundColor::default().inner, DEFAULT_SELECTION_BG);
        assert_eq!(SelectionColor::default().inner, ColorU::BLACK);
        assert_eq!(SelectionRadius::default().inner, PixelValue::zero());

        assert_eq!(
            SelectionBackgroundColor::default().print_as_css_value(),
            "#add6ffff"
        );
        assert_eq!(SelectionColor::default().print_as_css_value(), "#000000ff");
        assert_eq!(SelectionRadius::default().print_as_css_value(), "0px");

        assert_eq!(
            parse_selection_background_color("#add6ffff").unwrap(),
            SelectionBackgroundColor::default()
        );
        assert_eq!(
            parse_selection_color("#000000ff").unwrap(),
            SelectionColor::default()
        );
        assert_eq!(
            parse_selection_radius("0px").unwrap(),
            SelectionRadius::default()
        );
    }

    #[test]
    fn equal_values_hash_and_compare_equal() {
        let a = SelectionColor { inner: ColorU::new(1, 2, 3, 4) };
        let b = SelectionColor { inner: ColorU::new(1, 2, 3, 4) };
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);

        let black = SelectionColor { inner: ColorU::new(0, 0, 0, 255) };
        let white = SelectionColor { inner: ColorU::new(255, 255, 255, 255) };
        assert!(black < white);
        assert!(white > black);

        // Parsing the same input twice is deterministic (no interior state).
        assert_eq!(
            parse_selection_radius("1.5em").unwrap(),
            parse_selection_radius("1.5em").unwrap()
        );
        assert_eq!(
            hash_of(&parse_selection_radius("1.5em").unwrap()),
            hash_of(&parse_selection_radius("1.5em").unwrap())
        );
    }

    #[test]
    fn format_as_rust_code_emits_a_constructor_for_each_type() {
        let radius = SelectionRadius {
            inner: PixelValue::px(5.0),
        };
        assert_eq!(
            radius.format_as_rust_code(0),
            "SelectionRadius { inner: PixelValue::from_metric(SizeMetric::Px, 5) }"
        );
        assert_eq!(
            SelectionRadius {
                inner: PixelValue::from_metric(SizeMetric::Percent, 50.0),
            }
            .format_as_rust_code(0),
            "SelectionRadius { inner: PixelValue::from_metric(SizeMetric::Percent, 50) }"
        );
        assert_eq!(
            SelectionRadius::default().format_as_rust_code(0),
            "SelectionRadius { inner: PixelValue::from_metric(SizeMetric::Px, 0) }"
        );

        // The colour formatter's exact spelling belongs to codegen::format; only
        // the wrapper shape is this module's contract.
        let bg = SelectionBackgroundColor::default().format_as_rust_code(0);
        assert!(bg.starts_with("SelectionBackgroundColor { inner: "), "{bg}");
        assert!(bg.ends_with(" }"), "{bg}");
        let fg = SelectionColor::default().format_as_rust_code(0);
        assert!(fg.starts_with("SelectionColor { inner: "), "{fg}");
        assert!(fg.ends_with(" }"), "{fg}");

        // Indentation is documented as ignored — every depth prints the same.
        assert_eq!(radius.format_as_rust_code(0), radius.format_as_rust_code(9));
    }

    #[test]
    fn parse_errors_survive_the_owned_round_trip() {
        // The borrowed errors are re-hydrated from their owned form (used to send
        // errors across the FFI boundary); the message must not change.
        let color_err = parse_selection_color("not-a-color").unwrap_err();
        let owned: CssColorParseErrorOwned = color_err.to_contained();
        assert_eq!(format!("{:?}", owned.to_shared()), format!("{color_err:?}"));

        let px_err = parse_selection_radius("5 5px").unwrap_err();
        let owned_px: CssPixelValueParseErrorOwned = px_err.to_contained();
        assert_eq!(format!("{:?}", owned_px.to_shared()), format!("{px_err:?}"));

        // An error message must never be empty — it is surfaced to stylesheet authors.
        assert!(!format!("{color_err:?}").is_empty());
        assert!(!format!("{px_err:?}").is_empty());
    }

    #[test]
    fn the_two_color_properties_agree_on_every_input() {
        // Both delegate to `parse_css_color`; the only difference is the wrapper.
        // A divergence would mean one of them grew its own (wrong) grammar.
        for input in hostile_corpus() {
            let bg = parse_selection_background_color(&input);
            let fg = parse_selection_color(&input);
            match (bg, fg) {
                (Ok(b), Ok(f)) => assert_eq!(b.inner, f.inner, "diverged on {input:?}"),
                (Err(_), Err(_)) => {}
                (b, f) => panic!("{input:?} parsed inconsistently: {b:?} vs {f:?}"),
            }
        }
    }
}
