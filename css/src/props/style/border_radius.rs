//! CSS properties for border radius (`border-top-left-radius`,
//! `border-top-right-radius`, `border-bottom-left-radius`,
//! `border-bottom-right-radius`) and the `border-radius` shorthand parser.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::{
    props::{
        basic::pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
        macros::PixelValueTaker,
    },
};

// --- Property Struct Definitions ---

macro_rules! define_border_radius_property {
    ($struct_name:ident) => {
        #[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }

        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);
    };
}

/// CSS `border-top-left-radius` property value.
define_border_radius_property!(StyleBorderTopLeftRadius);
/// CSS `border-top-right-radius` property value.
define_border_radius_property!(StyleBorderTopRightRadius);
/// CSS `border-bottom-left-radius` property value.
define_border_radius_property!(StyleBorderBottomLeftRadius);
/// CSS `border-bottom-right-radius` property value.
define_border_radius_property!(StyleBorderBottomRightRadius);

// --- Parser-only Struct ---

/// A temporary struct used only during the parsing of the `border-radius` shorthand property.
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRadius {
    pub top_left: PixelValue,
    pub top_right: PixelValue,
    pub bottom_left: PixelValue,
    pub bottom_right: PixelValue,
}

// --- Error Types ---

/// Error for the shorthand `border-radius` property.
#[derive(Clone, PartialEq, Eq)]
pub enum CssBorderRadiusParseError<'a> {
    /// Too many values were provided (max is 4).
    TooManyValues(&'a str),
    /// An underlying pixel value could not be parsed.
    PixelValue(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssBorderRadiusParseError<'a>);
impl_display! { CssBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values for border-radius: \"{}\"", val),
    PixelValue(e) => format!("{}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssBorderRadiusParseError::PixelValue
);

/// Owned version of `CssBorderRadiusParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssBorderRadiusParseErrorOwned {
    TooManyValues(AzString),
    PixelValue(CssPixelValueParseErrorOwned),
}

/// Newtype wrapper around `CssBorderRadiusParseErrorOwned` for the `border-radius` shorthand.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CssStyleBorderRadiusParseErrorOwned {
    pub inner: CssBorderRadiusParseErrorOwned,
}

impl From<CssBorderRadiusParseErrorOwned> for CssStyleBorderRadiusParseErrorOwned {
    fn from(v: CssBorderRadiusParseErrorOwned) -> Self {
        Self { inner: v }
    }
}

impl CssBorderRadiusParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBorderRadiusParseErrorOwned {
        match self {
            CssBorderRadiusParseError::TooManyValues(s) => {
                CssBorderRadiusParseErrorOwned::TooManyValues((*s).to_string().into())
            }
            CssBorderRadiusParseError::PixelValue(e) => {
                CssBorderRadiusParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}

impl CssBorderRadiusParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssBorderRadiusParseError<'_> {
        match self {
            Self::TooManyValues(s) => {
                CssBorderRadiusParseError::TooManyValues(s)
            }
            Self::PixelValue(e) => {
                CssBorderRadiusParseError::PixelValue(e.to_shared())
            }
        }
    }
}

/// Macro to generate error types for individual radius properties.
macro_rules! define_border_radius_parse_error {
    ($error_name:ident, $error_name_owned:ident) => {
        #[derive(Clone, PartialEq, Eq)]
        pub enum $error_name<'a> {
            PixelValue(CssPixelValueParseError<'a>),
        }

        impl_debug_as_display!($error_name<'a>);
        impl_display! { $error_name<'a>, {
            PixelValue(e) => format!("{}", e),
        }}

        impl_from!(CssPixelValueParseError<'a>, $error_name::PixelValue);

        #[derive(Debug, Clone, PartialEq, Eq)]
        #[repr(C, u8)]
        pub enum $error_name_owned {
            PixelValue(CssPixelValueParseErrorOwned),
        }

        impl $error_name<'_> {
            #[must_use] pub fn to_contained(&self) -> $error_name_owned {
                match self {
                    $error_name::PixelValue(e) => $error_name_owned::PixelValue(e.to_contained()),
                }
            }
        }

        impl $error_name_owned {
            #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                match self {
                    $error_name_owned::PixelValue(e) => $error_name::PixelValue(e.to_shared()),
                }
            }
        }
    };
}

define_border_radius_parse_error!(
    StyleBorderTopLeftRadiusParseError,
    StyleBorderTopLeftRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderTopRightRadiusParseError,
    StyleBorderTopRightRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderBottomLeftRadiusParseError,
    StyleBorderBottomLeftRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderBottomRightRadiusParseError,
    StyleBorderBottomRightRadiusParseErrorOwned
);

// --- Parsing Functions ---

/// Parse the CSS `border-radius` shorthand into individual corner values.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-radius` value.
pub fn parse_style_border_radius(
    input: &str,
) -> Result<StyleBorderRadius, CssBorderRadiusParseError<'_>> {
    let components: Vec<_> = input.split_whitespace().collect();
    let mut values = Vec::with_capacity(components.len());
    for comp in &components {
        values.push(parse_pixel_value(comp)?);
    }

    match values.len() {
        1 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[0],
            bottom_right: values[0],
            bottom_left: values[0],
        }),
        2 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[0],
            bottom_left: values[1],
        }),
        3 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[2],
            bottom_left: values[1],
        }),
        4 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[2],
            bottom_left: values[3],
        }),
        _ => Err(CssBorderRadiusParseError::TooManyValues(input)),
    }
}

/// Parse the CSS `border-top-left-radius` longhand property.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-top-left-radius` value.
pub fn parse_style_border_top_left_radius(
    input: &str,
) -> Result<StyleBorderTopLeftRadius, StyleBorderTopLeftRadiusParseError<'_>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderTopLeftRadius { inner: pixel_value })
}

/// Parse the CSS `border-top-right-radius` longhand property.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-top-right-radius` value.
pub fn parse_style_border_top_right_radius(
    input: &str,
) -> Result<StyleBorderTopRightRadius, StyleBorderTopRightRadiusParseError<'_>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderTopRightRadius { inner: pixel_value })
}

/// Parse the CSS `border-bottom-left-radius` longhand property.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-bottom-left-radius` value.
pub fn parse_style_border_bottom_left_radius(
    input: &str,
) -> Result<StyleBorderBottomLeftRadius, StyleBorderBottomLeftRadiusParseError<'_>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderBottomLeftRadius { inner: pixel_value })
}

/// Parse the CSS `border-bottom-right-radius` longhand property.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `border-bottom-right-radius` value.
pub fn parse_style_border_bottom_right_radius(
    input: &str,
) -> Result<StyleBorderBottomRightRadius, StyleBorderBottomRightRadiusParseError<'_>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderBottomRightRadius { inner: pixel_value })
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_border_radius_shorthand() {
        // One value
        let result = parse_style_border_radius("10px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(10.0));
        assert_eq!(result.top_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_left, PixelValue::px(10.0));

        // Two values
        let result = parse_style_border_radius("10px 5%").unwrap();
        assert_eq!(result.top_left, PixelValue::px(10.0));
        assert_eq!(result.top_right, PixelValue::percent(5.0));
        assert_eq!(result.bottom_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_left, PixelValue::percent(5.0));

        // Three values
        let result = parse_style_border_radius("2px 4px 8px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(2.0));
        assert_eq!(result.top_right, PixelValue::px(4.0));
        assert_eq!(result.bottom_right, PixelValue::px(8.0));
        assert_eq!(result.bottom_left, PixelValue::px(4.0));

        // Four values
        let result = parse_style_border_radius("1px 0 3px 4px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(1.0));
        assert_eq!(result.top_right, PixelValue::px(0.0));
        assert_eq!(result.bottom_right, PixelValue::px(3.0));
        assert_eq!(result.bottom_left, PixelValue::px(4.0));

        // Weird whitespace
        let result = parse_style_border_radius("  1em   2em  ").unwrap();
        assert_eq!(result.top_left, PixelValue::em(1.0));
        assert_eq!(result.top_right, PixelValue::em(2.0));
    }

    #[test]
    fn test_parse_border_radius_shorthand_errors() {
        assert!(parse_style_border_radius("").is_err());
        assert!(parse_style_border_radius("1px 2px 3px 4px 5px").is_err());
        assert!(parse_style_border_radius("1px bad 3px").is_err());
    }

    #[test]
    fn test_parse_longhand_radius() {
        let result = parse_style_border_top_left_radius("25%").unwrap();
        assert_eq!(result.inner, PixelValue::percent(25.0));
    }
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
    use crate::{css::PrintAsCssValue, props::basic::length::SizeMetric};

    /// Every metric that `parse_pixel_value` has a suffix for. `Vmin` is
    /// deliberately absent — see `vmin_radius_should_parse_but_is_shadowed_by_in`.
    const ROUNDTRIPPABLE_METRICS: [SizeMetric; 11] = [
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

    /// Values that survive the 1/1000 fixed-point quantization of `FloatValue`
    /// exactly, so a failed round-trip means a real parser/printer bug and not
    /// a rounding artifact.
    const EXACT_VALUES: [f32; 6] = [0.0, 1.0, 12.5, -3.25, 0.125, 1000.5];

    /// Inputs that are not valid CSS lengths in any position.
    const GARBAGE: [&str; 14] = [
        "bad",
        "px",
        "%",
        "em",
        "-",
        "+",
        ".",
        "e",
        "1..2px",
        "1,2px",
        "10px;",
        "10px!important",
        "\0",
        "\u{7f}\u{1}",
    ];

    fn all_longhands(input: &str) -> [Result<PixelValue, ()>; 4] {
        [
            parse_style_border_top_left_radius(input)
                .map(|v| v.inner)
                .map_err(|_| ()),
            parse_style_border_top_right_radius(input)
                .map(|v| v.inner)
                .map_err(|_| ()),
            parse_style_border_bottom_left_radius(input)
                .map(|v| v.inner)
                .map_err(|_| ()),
            parse_style_border_bottom_right_radius(input)
                .map(|v| v.inner)
                .map_err(|_| ()),
        ]
    }

    // ================================================= shorthand: malformed ===

    #[test]
    fn shorthand_rejects_empty_and_whitespace_only() {
        // `split_whitespace` yields zero components, which falls into the `_`
        // arm — so a *missing* value is reported as TooManyValues. That variant
        // is a misnomer for this input (see report), but it is still an Err.
        for input in ["", " ", "   ", "\t\n", "\r\n\t ", "\u{a0}"] {
            let err = parse_style_border_radius(input)
                .expect_err("whitespace-only border-radius must not parse");
            assert!(
                matches!(err, CssBorderRadiusParseError::TooManyValues(_)),
                "unexpected error for {input:?}: {err}"
            );
        }
    }

    #[test]
    fn shorthand_rejects_garbage_without_panicking() {
        for input in GARBAGE {
            assert!(
                parse_style_border_radius(input).is_err(),
                "garbage {input:?} was accepted"
            );
        }
    }

    #[test]
    fn shorthand_rejects_more_than_four_values() {
        let input = "1px 2px 3px 4px 5px";
        let err = parse_style_border_radius(input).unwrap_err();
        assert_eq!(err, CssBorderRadiusParseError::TooManyValues(input));
        // The error carries the *whole* input back, not just the extra value.
        assert!(format!("{err}").contains(input));
    }

    #[test]
    fn shorthand_reports_the_bad_component_before_counting_values() {
        // Values are parsed eagerly, so a malformed component wins over the
        // arity check even when the arity is also wrong.
        let err = parse_style_border_radius("1px 2px 3px 4px 5px bad").unwrap_err();
        assert_eq!(
            err,
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::InvalidPixelValue("bad"))
        );
    }

    #[test]
    fn shorthand_rejects_elliptical_slash_syntax() {
        // `border-radius: 10px / 20px` is valid CSS but unsupported here; the
        // important part is that it is *rejected* rather than silently
        // mis-parsed into the wrong corners.
        assert!(parse_style_border_radius("10px / 20px").is_err());
        assert!(parse_style_border_radius("10px/20px").is_err());
        assert!(parse_style_border_radius("1px 2px / 3px 4px").is_err());
    }

    #[test]
    fn shorthand_rejects_split_number_and_unit() {
        // "10 px" is two components: "10" (a bare number => px) and "px"
        // (a unit with no value) — the latter fails.
        let err = parse_style_border_radius("10 px").unwrap_err();
        assert_eq!(
            err,
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::NoValueGiven(
                "px",
                SizeMetric::Px
            ))
        );
    }

    // ================================================== shorthand: numerics ===

    #[test]
    fn shorthand_boundary_numbers_never_produce_nan_or_infinity() {
        // Whatever these do, the stored FloatValue must stay finite: it is an
        // isize under the hood and a NaN/inf leak would poison layout.
        let inputs = [
            "0",
            "-0",
            "+0",
            "0px",
            "-0px",
            "0.0001px",
            "1e-30px",
            "1e30px",
            "-1e30px",
            "3.4028235e38px",
            "-3.4028235e38px",
            "9223372036854775807px",
            "-9223372036854775808px",
            "340282350000000000000000000000000000000%",
        ];
        for input in inputs {
            let Ok(radius) = parse_style_border_radius(input) else {
                continue; // rejection is an equally safe outcome
            };
            for corner in [
                radius.top_left,
                radius.top_right,
                radius.bottom_left,
                radius.bottom_right,
            ] {
                let n = corner.number.get();
                assert!(n.is_finite(), "{input:?} produced non-finite {n}");
            }
        }
    }

    #[test]
    fn shorthand_nan_input_is_flattened_to_zero() {
        // f32::from_str accepts "NaN"; FloatValue::new then casts NaN*1000 to
        // isize, and `as` maps NaN to 0. So `border-radius: NaNpx` is silently
        // accepted as 0px rather than rejected — assert it at least cannot
        // smuggle a NaN into the layout engine.
        for input in ["NaN", "nan", "NaNpx", "-nan%"] {
            let radius = parse_style_border_radius(input)
                .unwrap_or_else(|e| panic!("{input:?} unexpectedly rejected: {e}"));
            let n = radius.top_left.number.get();
            assert!(!n.is_nan(), "{input:?} leaked a NaN");
            assert_eq!(n, 0.0, "{input:?} should quantize to 0");
        }
    }

    #[test]
    fn shorthand_infinite_input_saturates_instead_of_overflowing() {
        let pos = parse_style_border_radius("infpx").unwrap().top_left;
        let neg = parse_style_border_radius("-infpx").unwrap().top_left;
        assert!(pos.number.get().is_finite());
        assert!(neg.number.get().is_finite());
        assert!(pos.number.get() > 0.0);
        assert!(neg.number.get() < 0.0);
        // Saturation, not wraparound: +inf must not come back out negative.
        assert_eq!(pos.number.number(), isize::MAX);
        assert_eq!(neg.number.number(), isize::MIN);
    }

    #[test]
    fn shorthand_sub_quantum_values_truncate_to_zero() {
        // FloatValue keeps 1/1000 of a unit; anything smaller becomes 0.
        // A 0.0001px radius is therefore indistinguishable from no radius.
        let radius = parse_style_border_radius("0.0001px").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(0.0));
        assert_eq!(radius.top_left.number.number(), 0);
    }

    #[test]
    fn shorthand_negative_zero_equals_positive_zero() {
        let neg = parse_style_border_radius("-0px").unwrap();
        let pos = parse_style_border_radius("0px").unwrap();
        assert_eq!(neg, pos);
        assert_eq!(neg.top_left.number.number(), 0);
    }

    // ==================================== shorthand: long / unicode / nested ===

    #[test]
    fn shorthand_extremely_long_input_terminates() {
        // 100k components: must reject on arity, not hang or blow the stack.
        let many = "1px ".repeat(100_000);
        assert!(matches!(
            parse_style_border_radius(&many),
            Err(CssBorderRadiusParseError::TooManyValues(_))
        ));

        // A single 100k-digit number: f32 parsing overflows to inf, which the
        // fixed-point cast saturates.
        let huge = format!("{}px", "1".repeat(100_000));
        let radius = parse_style_border_radius(&huge).unwrap();
        assert!(radius.top_left.number.get().is_finite());

        // 100k chars of pure junk in one token.
        let junk = "z".repeat(100_000);
        assert!(parse_style_border_radius(&junk).is_err());
    }

    #[test]
    fn shorthand_deeply_nested_brackets_do_not_stack_overflow() {
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_border_radius(&nested).is_err());

        let calls = format!("{}1px{}", "calc(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_border_radius(&calls).is_err());
    }

    #[test]
    fn shorthand_unicode_input_is_rejected_without_panicking() {
        // Multibyte input must not be byte-sliced anywhere in the parser.
        for input in [
            "\u{1F600}",
            "10px\u{1F600}",
            "\u{1F600}px",
            "\u{661}\u{660}px", // arabic-indic digits
            "10\u{301}px",      // combining acute
            "10px\u{200b}",     // zero-width space (not a separator)
            "１０px",           // fullwidth digits
            "10\u{202e}px",     // right-to-left override
        ] {
            assert!(
                parse_style_border_radius(input).is_err(),
                "unicode {input:?} was accepted"
            );
        }
    }

    #[test]
    fn shorthand_treats_non_breaking_space_as_a_value_separator() {
        // `str::split_whitespace` uses the Unicode White_Space property, so
        // U+00A0 separates values here even though CSS tokenization does not
        // treat it as whitespace (browsers reject this). Pinning current
        // behaviour — see report.
        let radius = parse_style_border_radius("1px\u{a0}2px").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(1.0));
        assert_eq!(radius.top_right, PixelValue::px(2.0));
    }

    // ==================================== shorthand: corner-expansion invariants

    #[test]
    fn shorthand_one_value_fills_every_corner() {
        let radius = parse_style_border_radius("7px").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(7.0));
        assert_eq!(radius.top_right, PixelValue::px(7.0));
        assert_eq!(radius.bottom_left, PixelValue::px(7.0));
        assert_eq!(radius.bottom_right, PixelValue::px(7.0));
    }

    #[test]
    fn shorthand_two_and_three_values_expand_along_the_diagonals() {
        // CSS Backgrounds 3 §5.1: 2 values => TL/BR = 1st, TR/BL = 2nd.
        let two = parse_style_border_radius("1px 2px").unwrap();
        assert_eq!(two.top_left, two.bottom_right);
        assert_eq!(two.top_right, two.bottom_left);
        assert_eq!(two.top_left, PixelValue::px(1.0));
        assert_eq!(two.top_right, PixelValue::px(2.0));

        // 3 values => TL = 1st, TR/BL = 2nd, BR = 3rd.
        let three = parse_style_border_radius("1px 2px 3px").unwrap();
        assert_eq!(three.top_left, PixelValue::px(1.0));
        assert_eq!(three.top_right, PixelValue::px(2.0));
        assert_eq!(three.bottom_left, PixelValue::px(2.0));
        assert_eq!(three.bottom_right, PixelValue::px(3.0));
    }

    #[test]
    fn shorthand_four_values_map_clockwise_from_top_left() {
        let four = parse_style_border_radius("1px 2px 3px 4px").unwrap();
        assert_eq!(four.top_left, PixelValue::px(1.0));
        assert_eq!(four.top_right, PixelValue::px(2.0));
        assert_eq!(four.bottom_right, PixelValue::px(3.0));
        assert_eq!(four.bottom_left, PixelValue::px(4.0));
        // No two corners aliased: a copy-paste slip in the match arm would
        // duplicate one of these.
        assert_ne!(four.top_left, four.top_right);
        assert_ne!(four.bottom_right, four.bottom_left);
    }

    #[test]
    fn shorthand_preserves_per_corner_units() {
        let radius = parse_style_border_radius("1px 2em 3% 4rem").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(1.0));
        assert_eq!(radius.top_right, PixelValue::em(2.0));
        assert_eq!(radius.bottom_right, PixelValue::percent(3.0));
        assert_eq!(radius.bottom_left, PixelValue::rem(4.0));
    }

    #[test]
    fn shorthand_valid_minimal_input() {
        let radius = parse_style_border_radius("0").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(0.0));
    }

    #[test]
    fn shorthand_tolerates_arbitrary_ascii_whitespace_runs() {
        let radius = parse_style_border_radius("\n\t 1px \t 2px\r\n").unwrap();
        assert_eq!(radius.top_left, PixelValue::px(1.0));
        assert_eq!(radius.top_right, PixelValue::px(2.0));
    }

    // ====================================================== longhand parsers ===

    #[test]
    fn longhands_reject_empty_whitespace_and_garbage() {
        for input in ["", " ", "\t\n"] {
            for result in all_longhands(input) {
                assert_eq!(result, Err(()), "{input:?} was accepted by a longhand");
            }
        }
        for input in GARBAGE {
            for result in all_longhands(input) {
                assert_eq!(result, Err(()), "{input:?} was accepted by a longhand");
            }
        }
    }

    #[test]
    fn longhands_reject_multiple_values() {
        // A longhand takes exactly one length; the shorthand list must not leak
        // through (e.g. by silently using the first value).
        for input in ["1px 2px", "1px 2px 3px 4px", "1px,2px"] {
            for result in all_longhands(input) {
                assert_eq!(result, Err(()), "{input:?} was accepted by a longhand");
            }
        }
    }

    #[test]
    fn all_four_longhands_agree_on_every_input() {
        // The four parsers are macro-free copies of each other; a copy-paste
        // bug (wrong metric, wrong field) would show up as a disagreement.
        let inputs = [
            "0", "10px", "-3.25em", "50%", "1.5rem", "2pt", "1in", "2.54cm", "10mm", "5vw", "5vh",
            "5vmax", "bad", "", "   ", "\u{1F600}", "1e30px", "NaNpx",
        ];
        for input in inputs {
            let [tl, tr, bl, br] = all_longhands(input);
            assert_eq!(tl, tr, "top-left vs top-right disagree on {input:?}");
            assert_eq!(tl, bl, "top-left vs bottom-left disagree on {input:?}");
            assert_eq!(tl, br, "top-left vs bottom-right disagree on {input:?}");
        }
    }

    #[test]
    fn longhands_agree_with_the_shorthand_on_single_values() {
        for input in ["0", "10px", "-3.25em", "50%", "1.5rem", "2pt"] {
            let shorthand = parse_style_border_radius(input).unwrap();
            let [tl, tr, bl, br] = all_longhands(input);
            assert_eq!(tl, Ok(shorthand.top_left), "{input:?}");
            assert_eq!(tr, Ok(shorthand.top_right), "{input:?}");
            assert_eq!(bl, Ok(shorthand.bottom_left), "{input:?}");
            assert_eq!(br, Ok(shorthand.bottom_right), "{input:?}");
        }
    }

    #[test]
    fn longhands_accept_a_gap_between_number_and_unit_but_the_shorthand_does_not() {
        // `parse_pixel_value` trims the value before parsing, so "10 px" is a
        // valid longhand — while the shorthand splits it into two components
        // and fails. Divergent, but deterministic; pinning both sides.
        for result in all_longhands("10 px") {
            assert_eq!(result, Ok(PixelValue::px(10.0)));
        }
        assert!(parse_style_border_radius("10 px").is_err());
    }

    #[test]
    fn longhands_survive_extremely_long_and_nested_input() {
        let long = format!("{}px", "9".repeat(100_000));
        for result in all_longhands(&long) {
            assert!(result.unwrap().number.get().is_finite());
        }
        let nested = format!("{}1px{}", "(".repeat(10_000), ")".repeat(10_000));
        for result in all_longhands(&nested) {
            assert_eq!(result, Err(()));
        }
    }

    #[test]
    fn longhands_boundary_numbers_stay_finite() {
        for input in [
            "0",
            "-0",
            "1e-30px",
            "1e30px",
            "-1e30px",
            "9223372036854775807px",
            "NaNpx",
            "infpx",
            "-infpx",
        ] {
            for result in all_longhands(input) {
                let value = result.unwrap_or_else(|()| panic!("{input:?} rejected"));
                let n = value.number.get();
                assert!(n.is_finite() && !n.is_nan(), "{input:?} produced {n}");
            }
        }
    }

    // =========================================================== round-trips ===

    #[test]
    fn print_as_css_value_round_trips_through_every_longhand_parser() {
        for metric in ROUNDTRIPPABLE_METRICS {
            for value in EXACT_VALUES {
                let pixel = PixelValue::from_metric(metric, value);

                let tl = StyleBorderTopLeftRadius { inner: pixel };
                let reparsed = parse_style_border_top_left_radius(&tl.print_as_css_value())
                    .unwrap_or_else(|e| panic!("{:?} did not re-parse: {e}", tl.print_as_css_value()));
                assert_eq!(reparsed, tl);

                let tr = StyleBorderTopRightRadius { inner: pixel };
                assert_eq!(
                    parse_style_border_top_right_radius(&tr.print_as_css_value()).unwrap(),
                    tr
                );

                let bl = StyleBorderBottomLeftRadius { inner: pixel };
                assert_eq!(
                    parse_style_border_bottom_left_radius(&bl.print_as_css_value()).unwrap(),
                    bl
                );

                let br = StyleBorderBottomRightRadius { inner: pixel };
                assert_eq!(
                    parse_style_border_bottom_right_radius(&br.print_as_css_value()).unwrap(),
                    br
                );
            }
        }
    }

    #[test]
    fn shorthand_round_trips_through_the_printed_corner_values() {
        let original = parse_style_border_radius("1px 2em 3% 4rem").unwrap();
        let printed = format!(
            "{} {} {} {}",
            StyleBorderTopLeftRadius {
                inner: original.top_left
            }
            .print_as_css_value(),
            StyleBorderTopRightRadius {
                inner: original.top_right
            }
            .print_as_css_value(),
            StyleBorderBottomRightRadius {
                inner: original.bottom_right
            }
            .print_as_css_value(),
            StyleBorderBottomLeftRadius {
                inner: original.bottom_left
            }
            .print_as_css_value(),
        );
        assert_eq!(parse_style_border_radius(&printed).unwrap(), original);
    }

    #[test]
    fn debug_output_matches_the_printed_css_value() {
        // The Debug impl is hand-written to delegate to `inner`; if it ever
        // reverts to the derived one, printed CSS and logs would diverge.
        let pixel = PixelValue::em(1.5);
        let tl = StyleBorderTopLeftRadius { inner: pixel };
        assert_eq!(format!("{tl:?}"), "1.5em");
        assert_eq!(format!("{tl:?}"), tl.print_as_css_value());
    }

    #[test]
    fn vmin_radius_parses() {
        // `border-radius: 3vmin` is valid CSS. parse_pixel_value used to check the
        // "in" suffix before "vmin", so "3vmin" stripped to "3vm" and failed to
        // parse as f32. The suffix table now orders "vmin"/"vmax" before "in"
        // (css/src/props/basic/pixel.rs), so this round-trips like every other metric.
        let printed = StyleBorderTopLeftRadius {
            inner: PixelValue::from_metric(SizeMetric::Vmin, 3.0),
        }
        .print_as_css_value();
        assert_eq!(printed, "3vmin");
        assert_eq!(
            parse_style_border_top_left_radius(&printed).unwrap().inner,
            PixelValue::from_metric(SizeMetric::Vmin, 3.0)
        );
    }

    // ================================================ constructors / getters ===

    #[test]
    fn default_and_zero_agree_for_every_corner_type() {
        assert_eq!(
            StyleBorderTopLeftRadius::default(),
            StyleBorderTopLeftRadius::zero()
        );
        assert_eq!(
            StyleBorderTopRightRadius::default(),
            StyleBorderTopRightRadius::zero()
        );
        assert_eq!(
            StyleBorderBottomLeftRadius::default(),
            StyleBorderBottomLeftRadius::zero()
        );
        assert_eq!(
            StyleBorderBottomRightRadius::default(),
            StyleBorderBottomRightRadius::zero()
        );
        assert_eq!(
            StyleBorderTopLeftRadius::default().inner,
            PixelValue::px(0.0)
        );
    }

    #[test]
    fn const_constructors_agree_with_the_float_constructors() {
        assert_eq!(
            StyleBorderTopLeftRadius::const_px(10),
            StyleBorderTopLeftRadius::px(10.0)
        );
        assert_eq!(
            StyleBorderTopLeftRadius::const_em(2),
            StyleBorderTopLeftRadius::em(2.0)
        );
        assert_eq!(
            StyleBorderTopLeftRadius::const_percent(50),
            StyleBorderTopLeftRadius::percent(50.0)
        );
        assert_eq!(
            StyleBorderTopLeftRadius::const_pt(-3),
            StyleBorderTopLeftRadius::pt(-3.0)
        );
        assert_eq!(
            StyleBorderTopLeftRadius::from_pixel_value(PixelValue::px(4.0)),
            StyleBorderTopLeftRadius::px(4.0)
        );
    }

    #[test]
    fn interpolate_returns_the_endpoints_for_matching_metrics() {
        let a = StyleBorderTopLeftRadius::px(0.0);
        let b = StyleBorderTopLeftRadius::px(10.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5), StyleBorderTopLeftRadius::px(5.0));
    }

    #[test]
    fn interpolate_stays_finite_for_hostile_t_values() {
        let a = StyleBorderTopLeftRadius::px(0.0);
        let b = StyleBorderTopLeftRadius::px(10.0);
        for t in [
            -1.0,
            2.0,
            1e30,
            -1e30,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let n = a.interpolate(&b, t).inner.number.get();
            assert!(n.is_finite(), "t={t} produced {n}");
            assert!(!n.is_nan(), "t={t} produced NaN");
        }
        // NaN t collapses to 0 rather than poisoning the value.
        assert_eq!(
            a.interpolate(&b, f32::NAN).inner.number.get(),
            0.0
        );
    }

    #[test]
    fn interpolate_across_metrics_converts_to_px() {
        // Mixed metrics fall back to px using the default 16px font size,
        // so 1em (=16px) -> 10px at t=0.5 is 13px.
        let em = StyleBorderTopLeftRadius::em(1.0);
        let px = StyleBorderTopLeftRadius::px(10.0);
        let mid = em.interpolate(&px, 0.5);
        assert_eq!(mid.inner.metric, SizeMetric::Px);
        assert_eq!(mid.inner.number.get(), 13.0);
        // Note: unlike the same-metric case, t=0 does NOT return `em` itself.
        assert_eq!(em.interpolate(&px, 0.0), StyleBorderTopLeftRadius::px(16.0));
    }

    // ========================================================= error getters ===

    #[test]
    fn shorthand_error_to_contained_round_trips_for_every_variant() {
        let float_err = "x".parse::<f32>().unwrap_err();
        let variants = [
            CssBorderRadiusParseError::TooManyValues("1px 2px 3px 4px 5px"),
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::EmptyString),
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::NoValueGiven(
                "px",
                SizeMetric::Px,
            )),
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::ValueParseErr(
                float_err, "bad",
            )),
            CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::InvalidPixelValue(
                "nope",
            )),
        ];
        for variant in variants {
            let owned = variant.to_contained();
            assert_eq!(owned.to_shared(), variant);
            // Display must survive the round-trip too.
            assert_eq!(format!("{}", owned.to_shared()), format!("{variant}"));
        }
    }

    #[test]
    fn shorthand_error_to_contained_handles_empty_unicode_and_huge_inputs() {
        let huge = "1px ".repeat(50_000);
        for input in ["", " ", "\u{1F600}\u{301}", "\0", huge.as_str()] {
            let err = CssBorderRadiusParseError::TooManyValues(input);
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err);
            match owned {
                CssBorderRadiusParseErrorOwned::TooManyValues(s) => {
                    assert_eq!(s.as_str(), input);
                }
                CssBorderRadiusParseErrorOwned::PixelValue(_) => panic!("wrong variant"),
            }
        }
    }

    #[test]
    fn shorthand_errors_from_the_parser_round_trip() {
        // Same as above, but with errors the parser actually produces rather
        // than hand-built ones.
        for input in ["", "1px 2px 3px 4px 5px", "1px bad 3px", "px", "\u{1F600}"] {
            let err = parse_style_border_radius(input).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err);
        }
    }

    #[test]
    fn error_debug_delegates_to_display() {
        let err = CssBorderRadiusParseError::TooManyValues("a b c d e");
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("a b c d e"));

        let inner = CssBorderRadiusParseError::PixelValue(CssPixelValueParseError::EmptyString);
        assert_eq!(
            format!("{inner}"),
            format!("{}", CssPixelValueParseError::EmptyString)
        );
    }

    #[test]
    fn longhand_error_types_round_trip() {
        // Each of the four longhand error enums has its own generated
        // to_contained/to_shared pair.
        let tl = parse_style_border_top_left_radius("bad").unwrap_err();
        assert_eq!(tl.to_contained().to_shared(), tl);

        let tr = parse_style_border_top_right_radius("").unwrap_err();
        assert_eq!(tr.to_contained().to_shared(), tr);

        let bl = parse_style_border_bottom_left_radius("px").unwrap_err();
        assert_eq!(bl.to_contained().to_shared(), bl);

        let br = parse_style_border_bottom_right_radius("\u{1F600}").unwrap_err();
        assert_eq!(br.to_contained().to_shared(), br);

        // ... and each keeps the underlying pixel error intact.
        assert_eq!(
            tl.to_contained(),
            StyleBorderTopLeftRadiusParseErrorOwned::PixelValue(
                CssPixelValueParseError::InvalidPixelValue("bad").to_contained()
            )
        );
    }

    #[test]
    fn owned_error_newtype_wrapper_preserves_the_inner_error() {
        let owned = CssBorderRadiusParseError::TooManyValues("1 2 3 4 5").to_contained();
        let wrapped = CssStyleBorderRadiusParseErrorOwned::from(owned.clone());
        assert_eq!(wrapped.inner, owned);
        assert_eq!(wrapped.inner.to_shared().to_contained(), owned);
    }
}
