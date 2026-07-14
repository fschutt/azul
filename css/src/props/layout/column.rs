//! CSS properties for multi-column layout.
//!
//! Covers `column-count`, `column-width`, `column-span`, `column-fill`,
//! `column-rule-width`, `column-rule-style`, and `column-rule-color`.
//! Types are consumed via the `CssProperty` enum in the CSS property system.

use alloc::string::{String, ToString};
use core::num::ParseIntError;

use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
    },
    formatter::PrintAsCssValue,
    style::border::{
        parse_border_style, BorderStyle, CssBorderStyleParseError, CssBorderStyleParseErrorOwned,
    },
};

// --- column-count ---

/// CSS `column-count` property: specifies the number of columns in a multi-column layout.
///
/// Values: `auto` or a positive integer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ColumnCount {
    #[default]
    Auto,
    Integer(u32),
}


impl PrintAsCssValue for ColumnCount {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Integer(i) => i.to_string(),
        }
    }
}

// --- column-width ---
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// CSS `column-width` property: specifies the optimal width of columns.
///
/// Values: `auto` or a length value (e.g. `200px`, `15em`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ColumnWidth {
    #[default]
    Auto,
    Length(PixelValue),
}


impl PrintAsCssValue for ColumnWidth {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Length(px) => px.print_as_css_value(),
        }
    }
}

// --- column-span ---

/// CSS `column-span` property: whether an element spans across all columns.
///
/// Values: `none` (default) or `all`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ColumnSpan {
    #[default]
    None,
    All,
}


impl PrintAsCssValue for ColumnSpan {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::All => "all",
        })
    }
}

// --- column-fill ---

/// CSS `column-fill` property: how content is distributed across columns.
///
/// Values: `balance` (default) or `auto`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ColumnFill {
    Auto,
    #[default]
    Balance,
}


impl PrintAsCssValue for ColumnFill {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Balance => "balance",
        })
    }
}

// --- column-rule ---

/// CSS `column-rule-width` property: the width of the rule between columns.
///
/// Defaults to `medium` (3px).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleWidth {
    pub inner: PixelValue,
}

impl Default for ColumnRuleWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(3),
        }
    }
}

impl PrintAsCssValue for ColumnRuleWidth {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// CSS `column-rule-style` property: the style of the rule between columns.
///
/// Uses `BorderStyle` values (e.g. `none`, `solid`, `dotted`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleStyle {
    pub inner: BorderStyle,
}

impl Default for ColumnRuleStyle {
    fn default() -> Self {
        Self {
            inner: BorderStyle::None,
        }
    }
}

impl PrintAsCssValue for ColumnRuleStyle {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// CSS `column-rule-color` property: the color of the rule between columns.
///
/// Per the CSS spec this should default to `currentcolor`, but currently
/// defaults to black as `currentcolor` requires a resolved-value pass at
/// layout time.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleColor {
    pub inner: ColorU,
}

impl Default for ColumnRuleColor {
    fn default() -> Self {
        // NOTE: should be `currentcolor` per CSS spec, see doc comment on type
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl PrintAsCssValue for ColumnRuleColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for ColumnCount {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnCount::Auto"),
            Self::Integer(i) => format!("ColumnCount::Integer({i})"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnWidth::Auto"),
            Self::Length(px) => format!(
                "ColumnWidth::Length({})",
                crate::codegen::format::format_pixel_value(px)
            ),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnSpan {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("ColumnSpan::None"),
            Self::All => String::from("ColumnSpan::All"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnFill {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnFill::Auto"),
            Self::Balance => String::from("ColumnFill::Balance"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleWidth {{ inner: {} }}",
            crate::codegen::format::format_pixel_value(&self.inner)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "ColumnRuleStyle {{ inner: {} }}",
            self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleColor {{ inner: {} }}",
            crate::codegen::format::format_color_value(&self.inner)
        )
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use crate::corety::AzString;

    // -- ColumnCount parser

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnCountParseError<'a> {
        InvalidValue(&'a str),
        ParseInt(ParseIntError),
    }

    impl_debug_as_display!(ColumnCountParseError<'a>);
    impl_display! { ColumnCountParseError<'a>, {
        InvalidValue(v) => format!("Invalid column-count value: \"{}\"", v),
        ParseInt(e) => format!("Invalid integer for column-count: {}", e),
    }}

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnCountParseErrorOwned {
        InvalidValue(AzString),
        ParseInt(AzString),
    }

    impl ColumnCountParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnCountParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnCountParseErrorOwned::InvalidValue((*s).to_string().into()),
                Self::ParseInt(e) => ColumnCountParseErrorOwned::ParseInt(e.to_string().into()),
            }
        }
    }

    impl ColumnCountParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnCountParseError<'_> {
            match self {
                Self::InvalidValue(s) => ColumnCountParseError::InvalidValue(s),
                // ParseIntError cannot be reconstructed from its Display string,
                // so we fall back to a generic message. The original error text
                // is preserved in the owned `AzString` but not round-trippable.
                Self::ParseInt(_) => ColumnCountParseError::InvalidValue("invalid integer"),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-count` value.
    pub fn parse_column_count(
        input: &str,
    ) -> Result<ColumnCount, ColumnCountParseError<'_>> {
        let trimmed = input.trim();
        if trimmed == "auto" {
            return Ok(ColumnCount::Auto);
        }
        let val: u32 = trimmed
            .parse()
            .map_err(ColumnCountParseError::ParseInt)?;
        Ok(ColumnCount::Integer(val))
    }

    // -- ColumnWidth parser

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnWidthParseError<'a> {
        InvalidValue(&'a str),
        PixelValue(CssPixelValueParseError<'a>),
    }

    impl_debug_as_display!(ColumnWidthParseError<'a>);
    impl_display! { ColumnWidthParseError<'a>, {
        InvalidValue(v) => format!("Invalid column-width value: \"{}\"", v),
        PixelValue(e) => format!("{}", e),
    }}
    impl_from! { CssPixelValueParseError<'a>, ColumnWidthParseError::PixelValue }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnWidthParseErrorOwned {
        InvalidValue(AzString),
        PixelValue(CssPixelValueParseErrorOwned),
    }

    impl ColumnWidthParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnWidthParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseErrorOwned::InvalidValue((*s).to_string().into()),
                Self::PixelValue(e) => ColumnWidthParseErrorOwned::PixelValue(e.to_contained()),
            }
        }
    }

    impl ColumnWidthParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnWidthParseError<'_> {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseError::InvalidValue(s),
                Self::PixelValue(e) => ColumnWidthParseError::PixelValue(e.to_shared()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-width` value.
    pub fn parse_column_width(
        input: &str,
    ) -> Result<ColumnWidth, ColumnWidthParseError<'_>> {
        let trimmed = input.trim();
        if trimmed == "auto" {
            return Ok(ColumnWidth::Auto);
        }
        Ok(ColumnWidth::Length(parse_pixel_value(trimmed)?))
    }

    // -- Other column parsers...
    macro_rules! define_simple_column_parser {
        (
            $fn_name:ident,
            $struct_name:ident,
            $error_name:ident,
            $error_owned_name:ident,
            $prop_name:expr,
            $($val:expr => $variant:path),+
        ) => {
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                InvalidValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                InvalidValue(v) => format!("Invalid {} value: \"{}\"", $prop_name, v),
            }}

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                InvalidValue(AzString),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::InvalidValue(s) => $error_owned_name::InvalidValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                    match self {
                        Self::InvalidValue(s) => $error_name::InvalidValue(s.as_str()),
                    }
                }
            }

            /// # Errors
            ///
            /// Returns an error if `input` is not a valid CSS value for this property.
            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                match input.trim() {
                    $( $val => Ok($variant), )+
                    _ => Err($error_name::InvalidValue(input)),
                }
            }
        };
    }

    define_simple_column_parser!(
        parse_column_span,
        ColumnSpan,
        ColumnSpanParseError,
        ColumnSpanParseErrorOwned,
        "column-span",
        "none" => ColumnSpan::None,
        "all" => ColumnSpan::All
    );

    define_simple_column_parser!(
        parse_column_fill,
        ColumnFill,
        ColumnFillParseError,
        ColumnFillParseErrorOwned,
        "column-fill",
        "auto" => ColumnFill::Auto,
        "balance" => ColumnFill::Balance
    );

    // Parsers for column-rule-*

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnRuleWidthParseError<'a> {
        Pixel(CssPixelValueParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleWidthParseError<'a>);
    impl_display! { ColumnRuleWidthParseError<'a>, { Pixel(e) => format!("{}", e) }}
    impl_from! { CssPixelValueParseError<'a>, ColumnRuleWidthParseError::Pixel }
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnRuleWidthParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
    }
    impl ColumnRuleWidthParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleWidthParseErrorOwned {
            match self {
                ColumnRuleWidthParseError::Pixel(e) => {
                    ColumnRuleWidthParseErrorOwned::Pixel(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleWidthParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleWidthParseError<'_> {
            match self {
                Self::Pixel(e) => {
                    ColumnRuleWidthParseError::Pixel(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-width` value.
    pub fn parse_column_rule_width(
        input: &str,
    ) -> Result<ColumnRuleWidth, ColumnRuleWidthParseError<'_>> {
        Ok(ColumnRuleWidth {
            inner: parse_pixel_value(input)?,
        })
    }

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnRuleStyleParseError<'a> {
        Style(CssBorderStyleParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleStyleParseError<'a>);
    impl_display! { ColumnRuleStyleParseError<'a>, { Style(e) => format!("{}", e) }}
    impl_from! { CssBorderStyleParseError<'a>, ColumnRuleStyleParseError::Style }
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnRuleStyleParseErrorOwned {
        Style(CssBorderStyleParseErrorOwned),
    }
    impl ColumnRuleStyleParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleStyleParseErrorOwned {
            match self {
                ColumnRuleStyleParseError::Style(e) => {
                    ColumnRuleStyleParseErrorOwned::Style(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleStyleParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleStyleParseError<'_> {
            match self {
                Self::Style(e) => {
                    ColumnRuleStyleParseError::Style(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-style` value.
    pub fn parse_column_rule_style(
        input: &str,
    ) -> Result<ColumnRuleStyle, ColumnRuleStyleParseError<'_>> {
        Ok(ColumnRuleStyle {
            inner: parse_border_style(input)?,
        })
    }

    #[derive(Clone, PartialEq)]
    pub enum ColumnRuleColorParseError<'a> {
        Color(CssColorParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleColorParseError<'a>);
    impl_display! { ColumnRuleColorParseError<'a>, { Color(e) => format!("{}", e) }}
    impl_from! { CssColorParseError<'a>, ColumnRuleColorParseError::Color }
    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnRuleColorParseErrorOwned {
        Color(CssColorParseErrorOwned),
    }
    impl ColumnRuleColorParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleColorParseErrorOwned {
            match self {
                ColumnRuleColorParseError::Color(e) => {
                    ColumnRuleColorParseErrorOwned::Color(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleColorParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleColorParseError<'_> {
            match self {
                Self::Color(e) => {
                    ColumnRuleColorParseError::Color(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-color` value.
    pub fn parse_column_rule_color(
        input: &str,
    ) -> Result<ColumnRuleColor, ColumnRuleColorParseError<'_>> {
        Ok(ColumnRuleColor {
            inner: parse_css_color(input)?,
        })
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_column_count() {
        assert_eq!(parse_column_count("auto").unwrap(), ColumnCount::Auto);
        assert_eq!(parse_column_count("3").unwrap(), ColumnCount::Integer(3));
        assert!(parse_column_count("none").is_err());
        assert!(parse_column_count("2.5").is_err());
    }

    #[test]
    fn test_parse_column_width() {
        assert_eq!(parse_column_width("auto").unwrap(), ColumnWidth::Auto);
        assert_eq!(
            parse_column_width("200px").unwrap(),
            ColumnWidth::Length(PixelValue::px(200.0))
        );
        assert_eq!(
            parse_column_width("15em").unwrap(),
            ColumnWidth::Length(PixelValue::em(15.0))
        );
        assert!(parse_column_width("50%").is_ok()); // Percentage is valid for column-width
    }

    #[test]
    fn test_parse_column_span() {
        assert_eq!(parse_column_span("none").unwrap(), ColumnSpan::None);
        assert_eq!(parse_column_span("all").unwrap(), ColumnSpan::All);
        assert!(parse_column_span("2").is_err());
    }

    #[test]
    fn test_parse_column_fill() {
        assert_eq!(parse_column_fill("auto").unwrap(), ColumnFill::Auto);
        assert_eq!(parse_column_fill("balance").unwrap(), ColumnFill::Balance);
        assert!(parse_column_fill("none").is_err());
    }

    #[test]
    fn test_parse_column_rule() {
        assert_eq!(
            parse_column_rule_width("5px").unwrap().inner,
            PixelValue::px(5.0)
        );
        assert_eq!(
            parse_column_rule_style("dotted").unwrap().inner,
            BorderStyle::Dotted
        );
        assert_eq!(parse_column_rule_color("blue").unwrap().inner, ColorU::BLUE);
    }
}

#[cfg(all(test, feature = "parser"))]
#[allow(clippy::float_cmp)] // parsed values are compared against the exact source literals
mod autotest_generated {
    use super::*;
    use crate::{codegen::format::FormatAsRustCode, corety::AzString, props::basic::SizeMetric};

    // A long-but-not-pathological input size for the "does not hang" cases.
    const LONG: usize = 1_000_000;

    // -----------------------------------------------------------------
    // parse_column_count
    // -----------------------------------------------------------------

    #[test]
    fn column_count_valid_minimal_and_trimming() {
        assert_eq!(parse_column_count("auto").unwrap(), ColumnCount::Auto);
        assert_eq!(parse_column_count("1").unwrap(), ColumnCount::Integer(1));
        // The parser trims, so surrounding whitespace must not change the value.
        assert_eq!(parse_column_count("  auto\t\n").unwrap(), ColumnCount::Auto);
        assert_eq!(parse_column_count(" \n 12 \t").unwrap(), ColumnCount::Integer(12));
    }

    #[test]
    fn column_count_rejects_empty_and_whitespace_only() {
        assert!(parse_column_count("").is_err());
        assert!(parse_column_count("   ").is_err());
        assert!(parse_column_count("\t\n\r ").is_err());
    }

    #[test]
    fn column_count_rejects_garbage_without_panicking() {
        for bad in [
            "none", "auto auto", "3px", "3;garbage", "3 4", "2.5", "0x10", "1_000", "--3", "+-3",
            "\0", "3\0", "١٢٣", "٣", "３", "NaN", "inf", "-inf", "e5", "1e3",
        ] {
            assert!(
                parse_column_count(bad).is_err(),
                "column-count accepted garbage: {bad:?}"
            );
        }
    }

    #[test]
    fn column_count_case_sensitivity_is_exact() {
        // NOTE: CSS keywords are ASCII-case-insensitive; this parser is not.
        // Documented here as the *current* contract - it must at least not panic.
        assert!(parse_column_count("AUTO").is_err());
        assert!(parse_column_count("Auto").is_err());
    }

    #[test]
    fn column_count_u32_boundaries_saturate_into_err_not_wrap() {
        // Lower bound: 0 is accepted even though CSS requires a positive integer.
        assert_eq!(parse_column_count("0").unwrap(), ColumnCount::Integer(0));
        // `u32::from_str` accepts a leading '+'.
        assert_eq!(parse_column_count("+7").unwrap(), ColumnCount::Integer(7));
        // Exact u32 ceiling parses; one above must be a clean Err, never a wrap to 0.
        assert_eq!(
            parse_column_count("4294967295").unwrap(),
            ColumnCount::Integer(u32::MAX)
        );
        assert!(parse_column_count("4294967296").is_err());
        // i64::MAX / u64::MAX / a 40-digit number all overflow u32 -> Err, no wraparound.
        assert!(parse_column_count("9223372036854775807").is_err());
        assert!(parse_column_count("18446744073709551615").is_err());
        assert!(parse_column_count("9999999999999999999999999999999999999999").is_err());
        // Negatives (including -0) are not representable in u32.
        assert!(parse_column_count("-1").is_err());
        assert!(parse_column_count("-0").is_err());
    }

    #[test]
    fn column_count_extremely_long_input_terminates() {
        let huge = "9".repeat(LONG);
        assert!(parse_column_count(&huge).is_err());
        let huge_keyword = "auto".repeat(LONG / 4);
        assert!(parse_column_count(&huge_keyword).is_err());
        // Whitespace-padded huge input: trimming must not be quadratic or panic.
        let padded = format!("{}{}{}", " ".repeat(10_000), "7", " ".repeat(10_000));
        assert_eq!(parse_column_count(&padded).unwrap(), ColumnCount::Integer(7));
    }

    #[test]
    fn column_count_deeply_nested_input_does_not_stack_overflow() {
        let nested = "(".repeat(10_000);
        assert!(parse_column_count(&nested).is_err());
        let balanced = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_column_count(&balanced).is_err());
    }

    #[test]
    fn column_count_unicode_input_does_not_panic() {
        for bad in [
            "\u{1F600}",
            "auto\u{1F600}",
            "e\u{301}",       // combining acute accent
            "\u{202E}3",      // RTL override
            "\u{FEFF}auto",   // BOM (not whitespace -> not trimmed)
            "au\u{0000}to",
        ] {
            assert!(
                parse_column_count(bad).is_err(),
                "column-count accepted unicode junk: {bad:?}"
            );
        }
    }

    #[test]
    fn column_count_roundtrips_through_print_as_css_value() {
        for value in [
            ColumnCount::Auto,
            ColumnCount::Integer(0),
            ColumnCount::Integer(1),
            ColumnCount::Integer(u32::MAX),
        ] {
            let printed = value.print_as_css_value();
            assert_eq!(
                parse_column_count(&printed).unwrap(),
                value,
                "round-trip failed for {value:?} (printed as {printed:?})"
            );
        }
    }

    // -----------------------------------------------------------------
    // ColumnCountParseError::to_contained / ColumnCountParseErrorOwned::to_shared
    // -----------------------------------------------------------------

    #[test]
    fn column_count_error_invalid_value_roundtrips_losslessly() {
        for s in ["", "x", "  padded  ", "\u{1F600}"] {
            let shared = ColumnCountParseError::InvalidValue(s);
            let owned = shared.to_contained();
            assert_eq!(owned, ColumnCountParseErrorOwned::InvalidValue(AzString::from(s)));
            assert_eq!(owned.to_shared(), shared);
        }
    }

    #[test]
    fn column_count_error_parse_int_roundtrip_is_lossy_but_safe() {
        let err = parse_column_count("abc").unwrap_err();
        assert_eq!(
            err,
            ColumnCountParseError::ParseInt("abc".parse::<u32>().unwrap_err())
        );

        let owned = err.to_contained();
        match &owned {
            ColumnCountParseErrorOwned::ParseInt(msg) => {
                assert!(!msg.as_str().is_empty(), "ParseInt message was dropped");
            }
            other => panic!("expected ParseInt, got {other:?}"),
        }

        // Documented lossy path: a ParseIntError cannot be rebuilt from its Display
        // string, so to_shared() degrades to a generic InvalidValue instead of panicking.
        assert_eq!(
            owned.to_shared(),
            ColumnCountParseError::InvalidValue("invalid integer")
        );
    }

    #[test]
    fn column_count_error_debug_equals_display_and_keeps_input() {
        let err = ColumnCountParseError::InvalidValue("weird\u{1F600}input");
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("weird\u{1F600}input"));

        // Overflow errors must render without panicking on an empty/huge instance.
        let overflow = parse_column_count("4294967296").unwrap_err();
        assert!(!format!("{overflow}").is_empty());
        assert!(!format!("{:?}", overflow.to_contained()).is_empty());
    }

    // -----------------------------------------------------------------
    // parse_column_width
    // -----------------------------------------------------------------

    #[test]
    fn column_width_valid_minimal_and_trimming() {
        assert_eq!(parse_column_width("auto").unwrap(), ColumnWidth::Auto);
        assert_eq!(parse_column_width("  auto  ").unwrap(), ColumnWidth::Auto);
        assert_eq!(
            parse_column_width("\t1px\n").unwrap(),
            ColumnWidth::Length(PixelValue::px(1.0))
        );
    }

    #[test]
    fn column_width_rejects_empty_whitespace_and_garbage() {
        assert!(parse_column_width("").is_err());
        assert!(parse_column_width("   ").is_err());
        assert!(parse_column_width("\t\n").is_err());
        for bad in [
            "px", "em", "%", "ten-px", "200px;garbage", "200 px extra", "#200px", "\u{1F600}",
            "20\u{301}px", "AUTO",
        ] {
            assert!(
                parse_column_width(bad).is_err(),
                "column-width accepted garbage: {bad:?}"
            );
        }
    }

    #[test]
    fn column_width_non_finite_numbers_are_stored_finite() {
        // f32::from_str accepts "NaN"/"inf", so these reach FloatValue::new().
        // The isize-backed FloatValue must clamp them - a NaN/inf leaking into
        // layout would poison every downstream computation.
        let nan = parse_column_width("NaN").unwrap();
        assert_eq!(nan, ColumnWidth::Length(PixelValue::px(0.0)));

        for input in ["inf", "Infinity", "-inf", "-Infinity", "1e40px", "-1e40px"] {
            let parsed = parse_column_width(input).unwrap();
            match parsed {
                ColumnWidth::Length(px) => assert!(
                    px.number.get().is_finite(),
                    "{input:?} produced a non-finite PixelValue"
                ),
                ColumnWidth::Auto => panic!("{input:?} unexpectedly parsed as auto"),
            }
        }
    }

    #[test]
    fn column_width_zero_and_subnormal_boundaries() {
        assert_eq!(
            parse_column_width("0").unwrap(),
            ColumnWidth::Length(PixelValue::px(0.0))
        );
        // -0.0 must normalize to the same stored value as +0.0 (FloatValue is an isize).
        assert_eq!(
            parse_column_width("-0").unwrap(),
            parse_column_width("0").unwrap()
        );
        assert_eq!(
            parse_column_width("-0px").unwrap(),
            ColumnWidth::Length(PixelValue::px(0.0))
        );
        // Subnormal f32: 1e-45 * 1000 truncates to 0 rather than panicking.
        assert_eq!(
            parse_column_width("1e-45px").unwrap(),
            ColumnWidth::Length(PixelValue::px(0.0))
        );
        // Negative lengths are (currently) accepted by the parser; must not wrap sign.
        match parse_column_width("-10px").unwrap() {
            ColumnWidth::Length(px) => assert!(px.number.get() < 0.0),
            ColumnWidth::Auto => panic!("-10px parsed as auto"),
        }
    }

    #[test]
    fn column_width_extremely_long_input_terminates() {
        let huge_digits = format!("{}px", "9".repeat(LONG));
        let parsed = parse_column_width(&huge_digits).unwrap();
        match parsed {
            ColumnWidth::Length(px) => assert!(px.number.get().is_finite()),
            ColumnWidth::Auto => panic!("digit soup parsed as auto"),
        }
        assert!(parse_column_width(&"a".repeat(LONG)).is_err());
        assert!(parse_column_width(&"auto".repeat(LONG / 4)).is_err());
    }

    #[test]
    fn column_width_deeply_nested_input_does_not_stack_overflow() {
        assert!(parse_column_width(&"(".repeat(10_000)).is_err());
        let balanced = format!("calc{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_column_width(&balanced).is_err());
    }

    #[test]
    fn column_width_roundtrips_through_print_as_css_value() {
        // Only exactly-representable numbers: FloatValue truncates to 1/1000ths,
        // so e.g. 2.54cm is *not* expected to survive a print/parse cycle.
        let values = [
            ColumnWidth::Auto,
            ColumnWidth::Length(PixelValue::px(0.0)),
            ColumnWidth::Length(PixelValue::px(200.0)),
            ColumnWidth::Length(PixelValue::px(-12.5)),
            ColumnWidth::Length(PixelValue::em(1.5)),
            ColumnWidth::Length(PixelValue::rem(2.0)),
            ColumnWidth::Length(PixelValue::pt(-20.0)),
            ColumnWidth::Length(PixelValue::percent(50.0)),
            ColumnWidth::Length(PixelValue::inch(1.0)),
            ColumnWidth::Length(PixelValue::cm(3.0)),
            ColumnWidth::Length(PixelValue::mm(10.0)),
            ColumnWidth::Length(PixelValue::from_metric(SizeMetric::Vw, 10.0)),
            ColumnWidth::Length(PixelValue::from_metric(SizeMetric::Vh, 10.0)),
            ColumnWidth::Length(PixelValue::from_metric(SizeMetric::Vmax, 10.0)),
        ];
        for value in values {
            let printed = value.print_as_css_value();
            assert_eq!(
                parse_column_width(&printed).unwrap(),
                value,
                "round-trip failed for {value:?} (printed as {printed:?})"
            );
        }
    }

    // -----------------------------------------------------------------
    // ColumnWidthParseError::to_contained / ColumnWidthParseErrorOwned::to_shared
    // -----------------------------------------------------------------

    #[test]
    fn column_width_error_roundtrips_through_owned() {
        // InvalidValue is only reachable by hand - the parser always delegates to
        // the pixel parser - but the conversion still has to be lossless.
        for s in ["", "bogus", "\u{1F600}"] {
            let shared = ColumnWidthParseError::InvalidValue(s);
            let owned = shared.to_contained();
            assert_eq!(owned, ColumnWidthParseErrorOwned::InvalidValue(AzString::from(s)));
            assert_eq!(owned.to_shared(), shared);
        }

        // The variants the parser actually produces.
        for bad in ["", "ten-px", "px", "%"] {
            let err = parse_column_width(bad).unwrap_err();
            assert!(
                matches!(err, ColumnWidthParseError::PixelValue(_)),
                "{bad:?} produced an unexpected error variant: {err:?}"
            );
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "lossy round-trip for {bad:?}");
            assert_eq!(format!("{err:?}"), format!("{err}"));
        }
    }

    // -----------------------------------------------------------------
    // parse_column_span / parse_column_fill
    // -----------------------------------------------------------------

    #[test]
    fn column_span_and_fill_accept_only_their_keywords() {
        assert_eq!(parse_column_span("none").unwrap(), ColumnSpan::None);
        assert_eq!(parse_column_span(" all \t").unwrap(), ColumnSpan::All);
        assert_eq!(parse_column_fill("auto").unwrap(), ColumnFill::Auto);
        assert_eq!(parse_column_fill("\n balance ").unwrap(), ColumnFill::Balance);

        for bad in ["", "   ", "2", "ALL", "None", "all all", "all;", "\u{1F600}", "nonee", "\0"] {
            assert!(parse_column_span(bad).is_err(), "column-span accepted {bad:?}");
        }
        for bad in ["", "   ", "none", "AUTO", "balanced", "auto balance", "\u{1F600}"] {
            assert!(parse_column_fill(bad).is_err(), "column-fill accepted {bad:?}");
        }
        // The two properties must not accept each other's keywords.
        assert!(parse_column_span("balance").is_err());
        assert!(parse_column_fill("all").is_err());
    }

    #[test]
    fn column_span_and_fill_survive_long_and_nested_input() {
        let huge = "all".repeat(LONG / 3);
        assert!(parse_column_span(&huge).is_err());
        assert!(parse_column_fill(&huge).is_err());
        let nested = "(".repeat(10_000);
        assert!(parse_column_span(&nested).is_err());
        assert!(parse_column_fill(&nested).is_err());
    }

    #[test]
    fn column_span_error_reports_the_untrimmed_input() {
        // The macro-generated parser matches on the trimmed input but reports the
        // *original* one - that asymmetry is load-bearing for error messages.
        let err = parse_column_span("  bogus  ").unwrap_err();
        match err {
            ColumnSpanParseError::InvalidValue(s) => assert_eq!(s, "  bogus  "),
        }
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("column-span"));

        let owned = err.to_contained();
        assert_eq!(owned, ColumnSpanParseErrorOwned::InvalidValue(AzString::from("  bogus  ")));
        assert_eq!(owned.to_shared(), err);

        let fill_err = parse_column_fill("\u{1F600}").unwrap_err();
        let fill_owned = fill_err.to_contained();
        assert_eq!(fill_owned.to_shared(), fill_err);
        assert!(format!("{fill_err}").contains("column-fill"));
    }

    // -----------------------------------------------------------------
    // parse_column_rule_width
    // -----------------------------------------------------------------

    #[test]
    fn column_rule_width_valid_and_invalid() {
        assert_eq!(
            parse_column_rule_width("5px").unwrap().inner,
            PixelValue::px(5.0)
        );
        assert_eq!(
            parse_column_rule_width("  0  ").unwrap().inner,
            PixelValue::px(0.0)
        );
        for bad in ["", "   ", "auto", "solid", "px", "\u{1F600}", "5px;5px", "5 px extra"] {
            assert!(
                parse_column_rule_width(bad).is_err(),
                "column-rule-width accepted {bad:?}"
            );
        }
    }

    #[test]
    fn column_rule_width_extremes_stay_finite() {
        for input in ["NaN", "inf", "-inf", "1e40px", "-1e40px", "1e-45px"] {
            let w = parse_column_rule_width(input).unwrap();
            assert!(
                w.inner.number.get().is_finite(),
                "{input:?} produced a non-finite column-rule-width"
            );
        }
        assert!(parse_column_rule_width(&"9".repeat(LONG)).unwrap().inner.number.get().is_finite());
        assert!(parse_column_rule_width(&"(".repeat(10_000)).is_err());
    }

    #[test]
    fn column_rule_width_default_is_medium_and_roundtrips() {
        let default = ColumnRuleWidth::default();
        assert_eq!(default.inner, PixelValue::const_px(3));
        assert_eq!(default.print_as_css_value(), "3px");
        assert_eq!(parse_column_rule_width("3px").unwrap(), default);

        for value in [
            ColumnRuleWidth::default(),
            ColumnRuleWidth { inner: PixelValue::px(0.0) },
            ColumnRuleWidth { inner: PixelValue::em(2.5) },
            ColumnRuleWidth { inner: PixelValue::percent(-25.0) },
        ] {
            let printed = value.print_as_css_value();
            assert_eq!(parse_column_rule_width(&printed).unwrap(), value);
        }
    }

    #[test]
    fn column_rule_width_error_roundtrips_through_owned() {
        for bad in ["", "auto", "px"] {
            let err = parse_column_rule_width(bad).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "lossy round-trip for {bad:?}");
            assert_eq!(format!("{err:?}"), format!("{err}"));
            assert!(!format!("{err}").is_empty());
        }
    }

    // -----------------------------------------------------------------
    // parse_column_rule_style
    // -----------------------------------------------------------------

    #[test]
    fn column_rule_style_accepts_every_border_style_and_roundtrips() {
        for style in [
            BorderStyle::None,
            BorderStyle::Solid,
            BorderStyle::Double,
            BorderStyle::Dotted,
            BorderStyle::Dashed,
            BorderStyle::Hidden,
            BorderStyle::Groove,
            BorderStyle::Ridge,
            BorderStyle::Inset,
            BorderStyle::Outset,
        ] {
            let value = ColumnRuleStyle { inner: style };
            let printed = value.print_as_css_value();
            assert_eq!(
                parse_column_rule_style(&printed).unwrap(),
                value,
                "round-trip failed for {style:?} (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn column_rule_style_rejects_garbage_without_panicking() {
        for bad in [
            "", "   ", "SOLID", "solidd", "solid solid", "3px", "\u{1F600}", "\0", "soli\u{0301}d",
        ] {
            assert!(
                parse_column_rule_style(bad).is_err(),
                "column-rule-style accepted {bad:?}"
            );
        }
        assert!(parse_column_rule_style(&"solid".repeat(LONG / 5)).is_err());
        assert!(parse_column_rule_style(&"(".repeat(10_000)).is_err());
        // Leading/trailing whitespace *is* trimmed.
        assert_eq!(
            parse_column_rule_style("  dotted \n").unwrap().inner,
            BorderStyle::Dotted
        );
    }

    #[test]
    fn column_rule_style_error_roundtrips_through_owned() {
        let err = parse_column_rule_style("bogus").unwrap_err();
        let owned = err.to_contained();
        assert_eq!(owned.to_shared(), err);
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("bogus"));

        let unicode_err = parse_column_rule_style("\u{1F600}").unwrap_err();
        let unicode_owned = unicode_err.to_contained();
        assert_eq!(unicode_owned.to_shared(), unicode_err);
    }

    // -----------------------------------------------------------------
    // parse_column_rule_color
    // -----------------------------------------------------------------

    #[test]
    fn column_rule_color_accepts_names_hex_and_functions() {
        assert_eq!(parse_column_rule_color("blue").unwrap().inner, ColorU::BLUE);
        // Named colors *are* case-insensitive here (unlike column-span/fill/count).
        assert_eq!(parse_column_rule_color("  BLUE \t").unwrap().inner, ColorU::BLUE);
        assert_eq!(parse_column_rule_color("#000f").unwrap().inner, ColorU::BLACK);
        assert_eq!(
            parse_column_rule_color("#ff000080").unwrap().inner,
            ColorU::rgba(255, 0, 0, 128)
        );
        assert_eq!(parse_column_rule_color("transparent").unwrap().inner.a, 0);
        assert_eq!(
            parse_column_rule_color("rgba(255, 0, 0, 1.0)").unwrap().inner,
            ColorU::RED
        );
    }

    #[test]
    fn column_rule_color_rejects_garbage_without_panicking() {
        for bad in [
            "",
            "   ",
            "#",
            "#f",
            "#ff",
            "#fffff",
            "#zzzzzz",
            "notacolor",
            "rgb(",
            "rgb(1,2",
            "rgb()",
            "rgb(300)",
            "hsl(",
            "\u{1F600}",
            "#\u{1F600}",
            "\u{130}", // dotted capital I: to_lowercase() expands to 2 chars
        ] {
            assert!(
                parse_column_rule_color(bad).is_err(),
                "column-rule-color accepted {bad:?}"
            );
        }
    }

    #[test]
    fn column_rule_color_survives_long_and_nested_input() {
        assert!(parse_column_rule_color(&"a".repeat(100_000)).is_err());
        assert!(parse_column_rule_color(&"#".repeat(100_000)).is_err());
        // Unbalanced and deeply nested parens must not recurse or hang.
        assert!(parse_column_rule_color(&"(".repeat(10_000)).is_err());
        assert!(parse_column_rule_color(&format!("rgb{}", "(".repeat(10_000))).is_err());
        let nested = format!("rgb{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_column_rule_color(&nested).is_err());
    }

    #[test]
    fn column_rule_color_roundtrips_through_to_hash() {
        for color in [
            ColorU::BLACK,
            ColorU::WHITE,
            ColorU::RED,
            ColorU::BLUE,
            ColorU::TRANSPARENT,
            ColorU::rgba(1, 2, 3, 4),
            ColorU::rgba(255, 255, 255, 0),
            ColorU::rgba(0, 0, 0, 255),
        ] {
            let value = ColumnRuleColor { inner: color };
            let printed = value.print_as_css_value(); // "#rrggbbaa"
            assert_eq!(printed.len(), 9, "unexpected hash form: {printed:?}");
            assert_eq!(
                parse_column_rule_color(&printed).unwrap(),
                value,
                "round-trip failed for {color:?} (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn column_rule_color_error_roundtrips_through_owned() {
        for bad in ["", "notacolor", "rgb(", "#zzzzzz"] {
            let err = parse_column_rule_color(bad).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "lossy round-trip for {bad:?}");
            assert_eq!(format!("{err:?}"), format!("{err}"));
            assert!(!format!("{err}").is_empty());
        }
    }

    // -----------------------------------------------------------------
    // Type invariants: defaults, ordering, hashing, codegen
    // -----------------------------------------------------------------

    #[test]
    fn defaults_match_the_documented_css_initial_values() {
        assert_eq!(ColumnCount::default(), ColumnCount::Auto);
        assert_eq!(ColumnWidth::default(), ColumnWidth::Auto);
        assert_eq!(ColumnSpan::default(), ColumnSpan::None);
        assert_eq!(ColumnFill::default(), ColumnFill::Balance);
        assert_eq!(ColumnRuleStyle::default().inner, BorderStyle::None);
        // NOTE: per CSS this should be `currentcolor`; the type doc records the deviation.
        assert_eq!(ColumnRuleColor::default().inner, ColorU::BLACK);

        // Every default must print to something its own parser accepts.
        assert_eq!(
            parse_column_count(&ColumnCount::default().print_as_css_value()).unwrap(),
            ColumnCount::default()
        );
        assert_eq!(
            parse_column_width(&ColumnWidth::default().print_as_css_value()).unwrap(),
            ColumnWidth::default()
        );
        assert_eq!(
            parse_column_span(&ColumnSpan::default().print_as_css_value()).unwrap(),
            ColumnSpan::default()
        );
        assert_eq!(
            parse_column_fill(&ColumnFill::default().print_as_css_value()).unwrap(),
            ColumnFill::default()
        );
        assert_eq!(
            parse_column_rule_width(&ColumnRuleWidth::default().print_as_css_value()).unwrap(),
            ColumnRuleWidth::default()
        );
        assert_eq!(
            parse_column_rule_style(&ColumnRuleStyle::default().print_as_css_value()).unwrap(),
            ColumnRuleStyle::default()
        );
        assert_eq!(
            parse_column_rule_color(&ColumnRuleColor::default().print_as_css_value()).unwrap(),
            ColumnRuleColor::default()
        );
    }

    #[test]
    fn ord_and_hash_agree_with_eq() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash_of<T: Hash>(t: &T) -> u64 {
            let mut h = DefaultHasher::new();
            t.hash(&mut h);
            h.finish()
        }

        // Keyword variants sort before their value-carrying counterparts.
        assert!(ColumnCount::Auto < ColumnCount::Integer(0));
        assert!(ColumnCount::Integer(1) < ColumnCount::Integer(u32::MAX));
        assert!(ColumnWidth::Auto < ColumnWidth::Length(PixelValue::px(0.0)));
        assert!(ColumnSpan::None < ColumnSpan::All);
        assert!(ColumnFill::Auto < ColumnFill::Balance);

        // Eq implies equal hashes (these types are used as prop-cache keys).
        assert_eq!(
            hash_of(&ColumnCount::Integer(7)),
            hash_of(&parse_column_count("7").unwrap())
        );
        assert_eq!(
            hash_of(&ColumnWidth::Length(PixelValue::px(0.0))),
            hash_of(&parse_column_width("-0px").unwrap())
        );
        assert_ne!(hash_of(&ColumnFill::Auto), hash_of(&ColumnFill::Balance));
    }

    #[test]
    fn format_as_rust_code_emits_constructible_snippets() {
        assert_eq!(ColumnCount::Auto.format_as_rust_code(0), "ColumnCount::Auto");
        assert_eq!(
            ColumnCount::Integer(u32::MAX).format_as_rust_code(0),
            "ColumnCount::Integer(4294967295)"
        );
        assert_eq!(ColumnWidth::Auto.format_as_rust_code(0), "ColumnWidth::Auto");
        assert_eq!(ColumnSpan::All.format_as_rust_code(0), "ColumnSpan::All");
        assert_eq!(ColumnSpan::None.format_as_rust_code(0), "ColumnSpan::None");
        assert_eq!(ColumnFill::Auto.format_as_rust_code(0), "ColumnFill::Auto");
        assert_eq!(ColumnFill::Balance.format_as_rust_code(0), "ColumnFill::Balance");

        // Extreme instances must format without panicking.
        let wide = ColumnWidth::Length(PixelValue::px(f32::MAX));
        assert!(wide.format_as_rust_code(0).starts_with("ColumnWidth::Length("));
        let rule = ColumnRuleWidth { inner: PixelValue::px(-0.5) };
        assert!(rule.format_as_rust_code(0).starts_with("ColumnRuleWidth {"));
        let style = ColumnRuleStyle { inner: BorderStyle::Dotted };
        assert!(style.format_as_rust_code(0).contains("Dotted"));
        let color = ColumnRuleColor { inner: ColorU::TRANSPARENT };
        assert!(color.format_as_rust_code(0).starts_with("ColumnRuleColor {"));
    }
}
