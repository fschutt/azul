//! CSS properties for positioning elements: `position`, `top`, `right`,
//! `bottom`, `left`, and `z-index`. Types defined here are consumed by the
//! layout solver to resolve positioned elements.

use alloc::string::{String, ToString};
use crate::corety::AzString;

#[cfg(feature = "parser")]
use crate::props::basic::pixel::parse_pixel_value;
use crate::props::{
    basic::pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    formatter::PrintAsCssValue,
    macros::PixelValueTaker,
};

// --- LayoutPosition ---

/// Represents a `position` attribute - default: `Static`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutPosition {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl LayoutPosition {
    #[must_use] pub fn is_positioned(&self) -> bool {
        *self != Self::Static
    }
}


impl PrintAsCssValue for LayoutPosition {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Fixed => "fixed",
            Self::Sticky => "sticky",
        })
    }
}

impl_enum_fmt!(LayoutPosition, Static, Fixed, Absolute, Relative, Sticky);

// -- Parser for LayoutPosition

#[derive(Clone, PartialEq, Eq)]
pub enum LayoutPositionParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(LayoutPositionParseError<'a>);
impl_display! { LayoutPositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid position value: \"{}\"", val),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutPositionParseErrorOwned {
    InvalidValue(AzString),
}

impl LayoutPositionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutPositionParseErrorOwned {
        match self {
            LayoutPositionParseError::InvalidValue(s) => {
                LayoutPositionParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl LayoutPositionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutPositionParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutPositionParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `position` value.
pub fn parse_layout_position(
    input: &str,
) -> Result<LayoutPosition, LayoutPositionParseError<'_>> {
    let input = input.trim();
    match input {
        "static" => Ok(LayoutPosition::Static),
        "relative" => Ok(LayoutPosition::Relative),
        "absolute" => Ok(LayoutPosition::Absolute),
        "fixed" => Ok(LayoutPosition::Fixed),
        "sticky" => Ok(LayoutPosition::Sticky),
        _ => Err(LayoutPositionParseError::InvalidValue(input)),
    }
}

// --- Offset Properties (top, right, bottom, left) ---

macro_rules! define_position_property {
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

        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                format!("{}", self.inner)
            }
        }
    };
}

/// Represents the CSS `top` offset property for positioned elements.
define_position_property!(LayoutTop);
/// Represents the CSS `right` offset property for positioned elements.
define_position_property!(LayoutRight);
/// Represents the CSS `bottom` offset property for positioned elements.
define_position_property!(LayoutInsetBottom);
/// Represents the CSS `left` offset property for positioned elements.
define_position_property!(LayoutLeft);

// -- Parse error types and parsers for offset properties (top, right, bottom, left)

macro_rules! define_offset_parse_error {
    ($struct_name:ident, $error_name:ident, $error_owned_name:ident, $parse_fn:ident) => {
        #[derive(Clone, PartialEq, Eq)]
        pub enum $error_name<'a> {
            PixelValue(CssPixelValueParseError<'a>),
        }
        impl_debug_as_display!($error_name<'a>);
        impl_display! { $error_name<'a>, { PixelValue(e) => format!("{}", e), }}
        impl_from!(CssPixelValueParseError<'a>, $error_name::PixelValue);

        #[derive(Debug, Clone, PartialEq, Eq)]
        #[repr(C, u8)]
        pub enum $error_owned_name {
            PixelValue(CssPixelValueParseErrorOwned),
        }
        impl $error_name<'_> {
            #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                match self {
                    $error_name::PixelValue(e) => {
                        $error_owned_name::PixelValue(e.to_contained())
                    }
                }
            }
        }
        impl $error_owned_name {
            #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                match self {
                    $error_owned_name::PixelValue(e) => {
                        $error_name::PixelValue(e.to_shared())
                    }
                }
            }
        }

        #[cfg(feature = "parser")]
        /// # Errors
        ///
        /// Returns an error if `input` is not a valid CSS value for this property.
        pub fn $parse_fn(input: &str) -> Result<$struct_name, $error_name<'_>> {
            parse_pixel_value(input)
                .map(|v| $struct_name { inner: v })
                .map_err(Into::into)
        }
    };
}

define_offset_parse_error!(LayoutTop, LayoutTopParseError, LayoutTopParseErrorOwned, parse_layout_top);
define_offset_parse_error!(LayoutRight, LayoutRightParseError, LayoutRightParseErrorOwned, parse_layout_right);
define_offset_parse_error!(LayoutInsetBottom, LayoutInsetBottomParseError, LayoutInsetBottomParseErrorOwned, parse_layout_bottom);
define_offset_parse_error!(LayoutLeft, LayoutLeftParseError, LayoutLeftParseErrorOwned, parse_layout_left);

// --- LayoutZIndex ---

/// Represents a `z-index` attribute - controls stacking order of positioned elements
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum LayoutZIndex {
    #[default]
    Auto,
    Integer(i32),
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for LayoutZIndex {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("LayoutZIndex::Auto"),
            Self::Integer(val) => {
                format!("LayoutZIndex::Integer({val})")
            }
        }
    }
}


impl PrintAsCssValue for LayoutZIndex {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => String::from("auto"),
            Self::Integer(val) => val.to_string(),
        }
    }
}

// -- Parser for LayoutZIndex

#[derive(Clone, PartialEq, Eq)]
pub enum LayoutZIndexParseError<'a> {
    InvalidValue(&'a str),
    ParseInt(::core::num::ParseIntError, &'a str),
}
impl_debug_as_display!(LayoutZIndexParseError<'a>);
impl_display! { LayoutZIndexParseError<'a>, {
    InvalidValue(val) => format!("Invalid z-index value: \"{}\"", val),
    ParseInt(e, s) => format!("Invalid z-index integer \"{}\": {}", s, e),
}}

/// Wrapper for `ParseIntError` that stores the error message and original
/// input as owned strings for FFI compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ParseIntErrorWithInput {
    /// The stringified parse error (e.g. "invalid digit found in string").
    pub error: AzString,
    /// The original input string that failed to parse.
    pub input: AzString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutZIndexParseErrorOwned {
    InvalidValue(AzString),
    ParseInt(ParseIntErrorWithInput),
}

impl LayoutZIndexParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutZIndexParseErrorOwned {
        match self {
            LayoutZIndexParseError::InvalidValue(s) => {
                LayoutZIndexParseErrorOwned::InvalidValue((*s).to_string().into())
            }
            LayoutZIndexParseError::ParseInt(e, s) => {
                LayoutZIndexParseErrorOwned::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: (*s).to_string().into() })
            }
        }
    }
}

impl LayoutZIndexParseErrorOwned {
    /// Converts back to the borrowed error type.
    ///
    /// **Note:** This conversion is lossy for `ParseInt` — the original
    /// `core::num::ParseIntError` cannot be reconstructed from its string
    /// representation, so `ParseInt` is mapped to `InvalidValue` instead.
    #[must_use] pub fn to_shared(&self) -> LayoutZIndexParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutZIndexParseError::InvalidValue(s.as_str())
            }
            Self::ParseInt(e) => {
                // We can't reconstruct ParseIntError, so use InvalidValue
                LayoutZIndexParseError::InvalidValue(e.input.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `z-index` value.
pub fn parse_layout_z_index(
    input: &str,
) -> Result<LayoutZIndex, LayoutZIndexParseError<'_>> {
    let input = input.trim();
    if input == "auto" {
        return Ok(LayoutZIndex::Auto);
    }

    match input.parse::<i32>() {
        Ok(val) => Ok(LayoutZIndex::Integer(val)),
        Err(e) => Err(LayoutZIndexParseError::ParseInt(e, input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_position() {
        assert_eq!(
            parse_layout_position("static").unwrap(),
            LayoutPosition::Static
        );
        assert_eq!(
            parse_layout_position("relative").unwrap(),
            LayoutPosition::Relative
        );
        assert_eq!(
            parse_layout_position("absolute").unwrap(),
            LayoutPosition::Absolute
        );
        assert_eq!(
            parse_layout_position("fixed").unwrap(),
            LayoutPosition::Fixed
        );
        assert_eq!(
            parse_layout_position("sticky").unwrap(),
            LayoutPosition::Sticky
        );
    }

    #[test]
    fn test_parse_layout_position_whitespace() {
        assert_eq!(
            parse_layout_position("  absolute  ").unwrap(),
            LayoutPosition::Absolute
        );
    }

    #[test]
    fn test_parse_layout_position_invalid() {
        assert!(parse_layout_position("").is_err());
        assert!(parse_layout_position("absolutely").is_err());
    }

    #[test]
    fn test_parse_layout_z_index() {
        assert_eq!(parse_layout_z_index("auto").unwrap(), LayoutZIndex::Auto);
        assert_eq!(
            parse_layout_z_index("10").unwrap(),
            LayoutZIndex::Integer(10)
        );
        assert_eq!(parse_layout_z_index("0").unwrap(), LayoutZIndex::Integer(0));
        assert_eq!(
            parse_layout_z_index("-5").unwrap(),
            LayoutZIndex::Integer(-5)
        );
        assert_eq!(
            parse_layout_z_index("  999  ").unwrap(),
            LayoutZIndex::Integer(999)
        );
    }

    #[test]
    fn test_parse_layout_z_index_invalid() {
        assert!(parse_layout_z_index("10px").is_err());
        assert!(parse_layout_z_index("1.5").is_err());
        assert!(parse_layout_z_index("none").is_err());
        assert!(parse_layout_z_index("").is_err());
    }

    #[test]
    fn test_parse_offsets() {
        assert_eq!(
            parse_layout_top("10px").unwrap(),
            LayoutTop {
                inner: PixelValue::px(10.0)
            }
        );
        assert_eq!(
            parse_layout_right("5%").unwrap(),
            LayoutRight {
                inner: PixelValue::percent(5.0)
            }
        );
        assert_eq!(
            parse_layout_bottom("2.5em").unwrap(),
            LayoutInsetBottom {
                inner: PixelValue::em(2.5)
            }
        );
        assert_eq!(
            parse_layout_left("0").unwrap(),
            LayoutLeft {
                inner: PixelValue::px(0.0)
            }
        );
    }

    #[test]
    fn test_parse_offsets_invalid() {
        // The simple `parse_pixel_value` does not handle `auto`.
        assert!(parse_layout_top("auto").is_err());
        assert!(parse_layout_right("").is_err());
        // Liberal parsing accepts whitespace between number and unit
        assert!(parse_layout_bottom("10 px").is_ok());
        assert!(parse_layout_left("ten pixels").is_err());
    }
}

#[cfg(test)]
mod autotest_generated {
    use alloc::{string::String, vec::Vec};

    use super::*;
    use crate::{codegen::format::FormatAsRustCode, props::basic::length::SizeMetric};

    /// Every `LayoutPosition` variant, so tests stay exhaustive if one is added.
    const ALL_POSITIONS: [LayoutPosition; 5] = [
        LayoutPosition::Static,
        LayoutPosition::Relative,
        LayoutPosition::Absolute,
        LayoutPosition::Fixed,
        LayoutPosition::Sticky,
    ];

    /// Every `SizeMetric` variant *except* `Vmin`, which cannot survive a
    /// `parse_pixel_value` round-trip — see `vmin_suffix_is_shadowed_by_in_bug`.
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

    // ---------------------------------------------------------------------
    // LayoutPosition::is_positioned (predicate)
    // ---------------------------------------------------------------------

    #[test]
    fn is_positioned_basic_true_false() {
        assert!(!LayoutPosition::Static.is_positioned());
        assert!(LayoutPosition::Absolute.is_positioned());
    }

    #[test]
    fn is_positioned_holds_for_every_variant() {
        // The invariant the layout solver relies on: positioned <=> not static.
        for p in ALL_POSITIONS {
            assert_eq!(p.is_positioned(), p != LayoutPosition::Static, "{p:?}");
        }
    }

    #[test]
    fn is_positioned_default_is_static_and_unpositioned() {
        assert_eq!(LayoutPosition::default(), LayoutPosition::Static);
        assert!(!LayoutPosition::default().is_positioned());
    }

    #[test]
    fn is_positioned_is_pure() {
        // Takes &self; repeated calls must not mutate or drift.
        let p = LayoutPosition::Sticky;
        assert_eq!(p.is_positioned(), p.is_positioned());
        assert!(p.is_positioned());
    }

    // ---------------------------------------------------------------------
    // parse_layout_position (parser)
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_valid_minimal() {
        assert_eq!(parse_layout_position("static"), Ok(LayoutPosition::Static));
        assert_eq!(
            parse_layout_position("relative"),
            Ok(LayoutPosition::Relative)
        );
        assert_eq!(
            parse_layout_position("absolute"),
            Ok(LayoutPosition::Absolute)
        );
        assert_eq!(parse_layout_position("fixed"), Ok(LayoutPosition::Fixed));
        assert_eq!(parse_layout_position("sticky"), Ok(LayoutPosition::Sticky));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_empty_and_whitespace_only() {
        // Empty and whitespace-only both trim down to "" and must report "".
        for input in ["", "   ", "\t\n", "\r\n\t  \x0c", " \u{0b} "] {
            let err = parse_layout_position(input)
                .expect_err("whitespace-only input must not parse");
            assert_eq!(err, LayoutPositionParseError::InvalidValue(""));
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_garbage_never_panics() {
        for input in [
            "absolutely",
            "STATIC",
            "position: absolute",
            "!@#$%^&*()",
            "\0\0\0",
            "-1",
            "0",
            "null",
            "static;",
            "static static",
            "\u{7f}\u{1}",
        ] {
            assert!(
                parse_layout_position(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_is_ascii_case_sensitive() {
        // NOTE: pins *current* behaviour. Per CSS Syntax, keywords are ASCII
        // case-insensitive, so these arguably ought to be Ok(..) — see report.
        for input in ["Static", "STATIC", "sTaTiC", "Absolute", "FIXED"] {
            assert!(
                parse_layout_position(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_trims_but_rejects_inner_junk() {
        // Surrounding whitespace is trimmed...
        assert_eq!(
            parse_layout_position("  \t absolute \n "),
            Ok(LayoutPosition::Absolute)
        );
        // ...but anything else attached to the keyword is rejected.
        for input in ["absolute;", "absolute garbage", ";absolute", "absolute,"] {
            assert!(
                parse_layout_position(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_error_borrows_the_trimmed_input() {
        let err = parse_layout_position("   bogus   ").unwrap_err();
        assert_eq!(err, LayoutPositionParseError::InvalidValue("bogus"));
        // Display must show the trimmed slice, not the padded original.
        assert_eq!(err.to_string(), "Invalid position value: \"bogus\"");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_unicode_never_panics() {
        for input in [
            "\u{1F600}",
            "static\u{0301}",   // combining acute accent
            "\u{202E}static",   // RTL override
            "абсолютный",
            "\u{FEFF}static",   // BOM is not ASCII whitespace -> must not parse
            "𝔰𝔱𝔞𝔱𝔦𝔠",
        ] {
            assert!(
                parse_layout_position(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_extremely_long_input() {
        let huge = "static".repeat(200_000); // ~1.2M bytes
        assert!(parse_layout_position(&huge).is_err());

        let huge_ws = String::from(" ").repeat(1_000_000);
        assert_eq!(
            parse_layout_position(&huge_ws),
            Err(LayoutPositionParseError::InvalidValue(""))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_deeply_nested_does_not_stack_overflow() {
        let nested = "[".repeat(10_000);
        assert!(parse_layout_position(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_round_trip_encode_decode() {
        for p in ALL_POSITIONS {
            let css = p.print_as_css_value();
            assert_eq!(parse_layout_position(&css), Ok(p), "round-trip of {p:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_position_round_trip_decode_encode() {
        for css in ["static", "relative", "absolute", "fixed", "sticky"] {
            let parsed = parse_layout_position(css).unwrap();
            assert_eq!(parsed.print_as_css_value(), css);
        }
    }

    #[test]
    fn position_format_as_rust_code_names_the_variant() {
        assert_eq!(
            LayoutPosition::Static.format_as_rust_code(0),
            "LayoutPosition::Static"
        );
        assert_eq!(
            LayoutPosition::Sticky.format_as_rust_code(usize::MAX),
            "LayoutPosition::Sticky"
        );
    }

    // ---------------------------------------------------------------------
    // LayoutPositionParseError <-> Owned (getters)
    // ---------------------------------------------------------------------

    #[test]
    fn position_error_to_contained_basic_access() {
        let shared = LayoutPositionParseError::InvalidValue("bogus");
        assert_eq!(
            shared.to_contained(),
            LayoutPositionParseErrorOwned::InvalidValue("bogus".to_string().into())
        );
    }

    #[test]
    fn position_error_owned_to_shared_basic_access() {
        let owned = LayoutPositionParseErrorOwned::InvalidValue("bogus".to_string().into());
        assert_eq!(
            owned.to_shared(),
            LayoutPositionParseError::InvalidValue("bogus")
        );
    }

    #[test]
    fn position_error_round_trip_is_lossless_on_edge_payloads() {
        // Empty, unicode, NUL and a 100k-byte payload must all survive intact.
        let long = "x".repeat(100_000);
        for payload in [
            "",
            " ",
            "\0",
            "\u{1F600}\u{0301}",
            "quote\"and\\backslash",
            long.as_str(),
        ] {
            let shared = LayoutPositionParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "payload {payload:?} was mangled");
            // ...and the owned form is a fixed point of the two conversions.
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    // ---------------------------------------------------------------------
    // parse_layout_z_index (parser)
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_valid_minimal() {
        assert_eq!(parse_layout_z_index("auto"), Ok(LayoutZIndex::Auto));
        assert_eq!(parse_layout_z_index("1"), Ok(LayoutZIndex::Integer(1)));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_i32_boundaries_saturate_into_err_not_wraparound() {
        // Exactly at the i32 limits: must parse.
        assert_eq!(
            parse_layout_z_index("2147483647"),
            Ok(LayoutZIndex::Integer(i32::MAX))
        );
        assert_eq!(
            parse_layout_z_index("-2147483648"),
            Ok(LayoutZIndex::Integer(i32::MIN))
        );

        // One past the limits: must be a clean Err, never a wrapped value.
        for overflowing in [
            "2147483648",
            "-2147483649",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "340282366920938463463374607431768211456",
        ] {
            let err = parse_layout_z_index(overflowing)
                .expect_err("out-of-range integer must not parse");
            assert!(
                matches!(err, LayoutZIndexParseError::ParseInt(_, s) if s == overflowing),
                "expected ParseInt({overflowing:?}), got {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_zero_sign_and_padding_forms() {
        assert_eq!(parse_layout_z_index("0"), Ok(LayoutZIndex::Integer(0)));
        assert_eq!(parse_layout_z_index("-0"), Ok(LayoutZIndex::Integer(0)));
        assert_eq!(parse_layout_z_index("+0"), Ok(LayoutZIndex::Integer(0)));
        assert_eq!(parse_layout_z_index("+7"), Ok(LayoutZIndex::Integer(7)));
        assert_eq!(parse_layout_z_index("007"), Ok(LayoutZIndex::Integer(7)));
        assert_eq!(
            parse_layout_z_index("  \t -42 \n "),
            Ok(LayoutZIndex::Integer(-42))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_rejects_floats_and_float_literals() {
        // z-index is an <integer>; nothing float-shaped may sneak through.
        for input in [
            "1.5", "1.0", "0.0", "-0.0", "1e3", "1E3", "1e-3", "NaN", "nan", "inf", "-inf",
            "infinity",
        ] {
            assert!(
                parse_layout_z_index(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_garbage_never_panics() {
        for input in [
            "", "   ", "\t\n", "auto auto", "AUTO", "Auto", "none", "10px", "1_000", "1,000",
            "- 5", "5-", "--5", "++5", "0x1F", "0b1", "١٢٣", // Arabic-Indic digits
            "１２３",                                          // fullwidth digits
            "\u{1F600}", "\0", "5\0",
        ] {
            assert!(
                parse_layout_z_index(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_extremely_long_input_terminates() {
        let huge = "9".repeat(1_000_000);
        assert!(parse_layout_z_index(&huge).is_err());

        let nested = "[".repeat(10_000);
        assert!(parse_layout_z_index(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_error_borrows_the_trimmed_input() {
        let err = parse_layout_z_index("  abc  ").unwrap_err();
        assert!(
            matches!(&err, LayoutZIndexParseError::ParseInt(_, s) if *s == "abc"),
            "got {err:?}"
        );
        assert!(err.to_string().starts_with("Invalid z-index integer \"abc\""));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_z_index_round_trip_encode_decode() {
        let values = [
            LayoutZIndex::Auto,
            LayoutZIndex::Integer(0),
            LayoutZIndex::Integer(1),
            LayoutZIndex::Integer(-1),
            LayoutZIndex::Integer(12_345),
            LayoutZIndex::Integer(-99_999),
            LayoutZIndex::Integer(i32::MAX),
            LayoutZIndex::Integer(i32::MIN),
        ];
        for z in values {
            let css = z.print_as_css_value();
            assert_eq!(parse_layout_z_index(&css), Ok(z), "round-trip of {z:?}");
        }
    }

    #[test]
    fn z_index_default_is_auto() {
        assert_eq!(LayoutZIndex::default(), LayoutZIndex::Auto);
        assert_eq!(LayoutZIndex::default().print_as_css_value(), "auto");
    }

    #[test]
    fn z_index_format_as_rust_code_survives_i32_min() {
        assert_eq!(LayoutZIndex::Auto.format_as_rust_code(0), "LayoutZIndex::Auto");
        assert_eq!(
            LayoutZIndex::Integer(i32::MIN).format_as_rust_code(0),
            "LayoutZIndex::Integer(-2147483648)"
        );
    }

    // ---------------------------------------------------------------------
    // LayoutZIndexParseError <-> Owned (getters)
    // ---------------------------------------------------------------------

    #[test]
    fn z_index_error_invalid_value_round_trips_losslessly() {
        for payload in ["", "auto ", "\u{1F600}", "\0"] {
            let shared = LayoutZIndexParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                LayoutZIndexParseErrorOwned::InvalidValue(payload.to_string().into())
            );
            assert_eq!(owned.to_shared(), shared);
        }
    }

    #[test]
    fn z_index_error_to_contained_captures_parse_int_message_and_input() {
        let int_err = "abc".parse::<i32>().unwrap_err();
        let shared = LayoutZIndexParseError::ParseInt(int_err.clone(), "abc");

        match shared.to_contained() {
            LayoutZIndexParseErrorOwned::ParseInt(e) => {
                assert_eq!(e.input.as_str(), "abc");
                assert!(!e.error.as_str().is_empty());
                assert_eq!(e.error.as_str(), int_err.to_string());
            }
            other => panic!("expected ParseInt, got {other:?}"),
        }
    }

    #[test]
    fn z_index_error_overflow_and_invalid_digit_are_distinct_before_to_shared() {
        let invalid_digit = "abc".parse::<i32>().unwrap_err();
        let overflow = "2147483648".parse::<i32>().unwrap_err();

        let a = LayoutZIndexParseError::ParseInt(invalid_digit, "abc").to_contained();
        let b = LayoutZIndexParseError::ParseInt(overflow, "2147483648").to_contained();
        assert_ne!(a, b, "the two ParseIntError kinds must not collapse");
    }

    #[test]
    fn z_index_error_to_shared_is_lossy_for_parse_int_as_documented() {
        let int_err = "abc".parse::<i32>().unwrap_err();
        let owned = LayoutZIndexParseError::ParseInt(int_err, "abc").to_contained();

        // Documented: ParseIntError cannot be rebuilt, so ParseInt -> InvalidValue.
        let shared = owned.to_shared();
        assert_eq!(shared, LayoutZIndexParseError::InvalidValue("abc"));

        // ...which means the owned form is NOT a fixed point of the two
        // conversions. This asserts the lossiness is exactly as documented,
        // and will fail loudly if someone silently changes the mapping.
        assert_ne!(shared.to_contained(), owned);
        assert_eq!(
            shared.to_contained(),
            LayoutZIndexParseErrorOwned::InvalidValue("abc".to_string().into())
        );
    }

    #[test]
    fn z_index_error_to_shared_on_empty_parse_int_payload_does_not_panic() {
        let owned = LayoutZIndexParseErrorOwned::ParseInt(ParseIntErrorWithInput {
            error: String::new().into(),
            input: String::new().into(),
        });
        assert_eq!(owned.to_shared(), LayoutZIndexParseError::InvalidValue(""));
    }

    // ---------------------------------------------------------------------
    // Offset properties: top / right / bottom / left
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_nan_input_saturates_to_zero_instead_of_propagating_nan() {
        // f32::from_str accepts "NaN"; PixelValue stores fixed-point isize, and
        // `NaN as isize` == 0. Assert the NaN is absorbed, not carried into layout.
        for input in ["NaN", "nan", "-NaN", "NaNpx", "nan%"] {
            let v = parse_layout_top(input)
                .unwrap_or_else(|e| panic!("{input:?} should parse, got {e:?}"));
            let n = v.inner.number.get();
            assert!(!n.is_nan(), "{input:?} leaked a NaN into PixelValue");
            assert_eq!(n, 0.0, "{input:?} should saturate to 0");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_infinite_input_saturates_to_finite_bounds() {
        // f32::from_str maps overflow to +/-inf; the fixed-point cast then
        // saturates at isize::MIN/MAX. Nothing infinite may reach the solver.
        for (input, positive) in [
            ("inf", true),
            ("infinity", true),
            ("-inf", false),
            ("1e40px", true),
            ("-1e40px", false),
            ("99999999999999999999999999999999999999999999px", true),
        ] {
            let v = parse_layout_right(input)
                .unwrap_or_else(|e| panic!("{input:?} should parse, got {e:?}"));
            let n = v.inner.number.get();
            assert!(n.is_finite(), "{input:?} leaked a non-finite PixelValue");
            assert_eq!(n > 0.0, positive, "{input:?} lost its sign");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_subnormal_input_flushes_to_zero() {
        // Below the 1/1000 fixed-point resolution everything truncates to 0.
        for input in ["1e-40px", "-1e-40px", "0.0001px", "0.0009px"] {
            let v = parse_layout_bottom(input)
                .unwrap_or_else(|e| panic!("{input:?} should parse, got {e:?}"));
            assert_eq!(v.inner.number.get(), 0.0, "{input:?} should truncate to 0");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_reject_empty_garbage_and_unicode() {
        for input in [
            "",
            "   ",
            "auto",
            "px",
            "%",
            "10 20px",
            "ten pixels",
            "10pxx",
            "10 px extra",
            "\u{1F600}",
            "١٠px", // Arabic-Indic digits
            "10\u{00B5}m",
        ] {
            assert!(
                parse_layout_left(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_extremely_long_input_terminates() {
        let huge = "9".repeat(1_000_000);
        // Parses to a saturated-but-finite value; must not hang or panic.
        let v = parse_layout_top(&huge).expect("a million 9s is a valid f32 literal");
        assert!(v.inner.number.get().is_finite());

        let junk = "z".repeat(1_000_000);
        assert!(parse_layout_top(&junk).is_err());

        let nested = "[".repeat(10_000);
        assert!(parse_layout_top(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_round_trip_encode_decode_across_metrics() {
        // Values chosen to be exact at the 1/1000 fixed-point resolution.
        for metric in ROUNDTRIPPABLE_METRICS {
            for raw in [0.0_f32, 1.0, -1.0, 10.5, -7.25, 1024.0] {
                let expected = LayoutTop {
                    inner: PixelValue::from_metric(metric, raw),
                };
                let css = expected.print_as_css_value();
                let parsed = parse_layout_top(&css)
                    .unwrap_or_else(|e| panic!("{css:?} failed to re-parse: {e:?}"));
                assert_eq!(parsed, expected, "round-trip of {css:?}");
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_all_four_sides_agree_on_the_same_input() {
        // The four parsers are macro-generated; they must not diverge.
        let t = parse_layout_top("12.5%").unwrap();
        let r = parse_layout_right("12.5%").unwrap();
        let b = parse_layout_bottom("12.5%").unwrap();
        let l = parse_layout_left("12.5%").unwrap();
        assert_eq!(t.inner, r.inner);
        assert_eq!(r.inner, b.inner);
        assert_eq!(b.inner, l.inner);
        assert_eq!(t.inner, PixelValue::percent(12.5));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offsets_zero_is_bare_number_defaulting_to_px() {
        assert_eq!(parse_layout_left("0").unwrap(), LayoutLeft::zero());
        assert_eq!(LayoutTop::zero().inner, PixelValue::px(0.0));
        assert_eq!(LayoutTop::default().inner, PixelValue::zero());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn offset_error_round_trip_is_lossless_for_every_variant() {
        // Drive each CssPixelValueParseError variant through the real parser,
        // then assert to_contained/to_shared preserves it.
        let inputs = ["", "px", "abcpx", "not-a-length"];
        let mut seen: Vec<String> = Vec::new();
        for input in inputs {
            let err = parse_layout_top(input).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "error for {input:?} was mangled");
            assert_eq!(owned.to_shared().to_contained(), owned);
            seen.push(err.to_string());
        }
        // The four inputs must exercise four *distinct* error messages,
        // otherwise this test is silently only covering one variant.
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 4, "expected 4 distinct pixel-value errors");
    }

    /// KNOWN BUG (pinned, not endorsed): `vmin` lengths are unparseable.
    ///
    /// `parse_pixel_value`'s suffix table tries `("in", In)` *before*
    /// `("vmin", Vmin)`, so `"10vmin"` hits `strip_suffix("in")` first, leaving
    /// `"10vm"` to be parsed as an f32 — which fails. Every `vmin` length in a
    /// stylesheet is therefore silently dropped, on `top`/`right`/`bottom`/
    /// `left` and on every other property that goes through `parse_pixel_value`.
    ///
    /// This test asserts the *current* (broken) behaviour so the suite stays
    /// honest; flip it to `is_ok()` / an equality assert once the suffix table
    /// in `css/src/props/basic/pixel.rs` is reordered.
    #[cfg(feature = "parser")]
    #[test]
    fn vmin_suffix_is_shadowed_by_in_bug() {
        // FIXED (was a characterization of the bug): "vmin" used to be shadowed by an
        // earlier "in" suffix and rejected. It now parses on every longhand.
        assert!(parse_layout_top("10vmin").is_ok());
        assert!(parse_layout_right("0vmin").is_ok());
        assert!(parse_layout_bottom("2.5vmin").is_ok());
        assert!(parse_layout_left("100vmin").is_ok());

        // The neighbouring viewport units keep working too.
        assert!(parse_layout_top("10vmax").is_ok());
        assert!(parse_layout_top("10vw").is_ok());
        assert!(parse_layout_top("10vh").is_ok());
    }
}
