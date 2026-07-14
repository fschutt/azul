//! CSS properties for controlling fragmentation (page/column breaks).
//!
//! Defines [`PageBreak`], [`BreakInside`], [`Widows`], [`Orphans`], and
//! [`BoxDecorationBreak`]. The `parser` sub-module (behind the `parser`
//! feature) provides CSS-value parsing for each type.

use alloc::string::{String, ToString};

use crate::props::formatter::PrintAsCssValue;

// --- break-before / break-after ---

/// Represents a `break-before` or `break-after` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum PageBreak {
    #[default]
    Auto,
    Avoid,
    Always,
    All,
    Page,
    AvoidPage,
    Left,
    Right,
    Recto,
    Verso,
    Column,
    AvoidColumn,
}


impl PrintAsCssValue for PageBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Avoid => "avoid",
            Self::Always => "always",
            Self::All => "all",
            Self::Page => "page",
            Self::AvoidPage => "avoid-page",
            Self::Left => "left",
            Self::Right => "right",
            Self::Recto => "recto",
            Self::Verso => "verso",
            Self::Column => "column",
            Self::AvoidColumn => "avoid-column",
        })
    }
}

// --- break-inside ---

/// Represents a `break-inside` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum BreakInside {
    #[default]
    Auto,
    Avoid,
    AvoidPage,
    AvoidColumn,
}


impl PrintAsCssValue for BreakInside {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Avoid => "avoid",
            Self::AvoidPage => "avoid-page",
            Self::AvoidColumn => "avoid-column",
        })
    }
}

// --- widows / orphans ---

/// CSS `widows` property - minimum number of lines in a block container
/// that must be shown at the top of a page, region, or column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Widows {
    pub inner: u32,
}

impl Default for Widows {
    fn default() -> Self {
        Self { inner: 2 }
    }
}

impl PrintAsCssValue for Widows {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

/// CSS `orphans` property - minimum number of lines in a block container
/// that must be shown at the bottom of a page, region, or column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Orphans {
    pub inner: u32,
}

impl Default for Orphans {
    fn default() -> Self {
        Self { inner: 2 }
    }
}

impl PrintAsCssValue for Orphans {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

// --- box-decoration-break ---

/// Represents a `box-decoration-break` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum BoxDecorationBreak {
    #[default]
    Slice,
    Clone,
}


impl PrintAsCssValue for BoxDecorationBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Slice => "slice",
            Self::Clone => "clone",
        })
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for PageBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("PageBreak::Auto"),
            Self::Avoid => String::from("PageBreak::Avoid"),
            Self::Always => String::from("PageBreak::Always"),
            Self::All => String::from("PageBreak::All"),
            Self::Page => String::from("PageBreak::Page"),
            Self::AvoidPage => String::from("PageBreak::AvoidPage"),
            Self::Left => String::from("PageBreak::Left"),
            Self::Right => String::from("PageBreak::Right"),
            Self::Recto => String::from("PageBreak::Recto"),
            Self::Verso => String::from("PageBreak::Verso"),
            Self::Column => String::from("PageBreak::Column"),
            Self::AvoidColumn => String::from("PageBreak::AvoidColumn"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for BreakInside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("BreakInside::Auto"),
            Self::Avoid => String::from("BreakInside::Avoid"),
            Self::AvoidPage => String::from("BreakInside::AvoidPage"),
            Self::AvoidColumn => String::from("BreakInside::AvoidColumn"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for Widows {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Widows {{ inner: {} }}", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for Orphans {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Orphans {{ inner: {} }}", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for BoxDecorationBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Slice => String::from("BoxDecorationBreak::Slice"),
            Self::Clone => String::from("BoxDecorationBreak::Clone"),
        }
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use core::num::ParseIntError;
    use crate::corety::AzString;
    use crate::props::layout::position::ParseIntErrorWithInput;

    // -- PageBreak parser (`break-before`, `break-after`)

    /// Error returned when parsing a `break-before` or `break-after` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum PageBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(PageBreakParseError<'a>);
    impl_display! { PageBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid break value: \"{}\"", v),
    }}

    /// Owned version of [`PageBreakParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum PageBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl PageBreakParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> PageBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => PageBreakParseErrorOwned::InvalidValue((*s).to_string().into()),
            }
        }
    }

    impl PageBreakParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> PageBreakParseError<'_> {
            match self {
                Self::InvalidValue(s) => PageBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `page-break` value.
    pub fn parse_page_break(input: &str) -> Result<PageBreak, PageBreakParseError<'_>> {
        match input.trim() {
            "auto" => Ok(PageBreak::Auto),
            "avoid" => Ok(PageBreak::Avoid),
            "always" => Ok(PageBreak::Always),
            "all" => Ok(PageBreak::All),
            "page" => Ok(PageBreak::Page),
            "avoid-page" => Ok(PageBreak::AvoidPage),
            "left" => Ok(PageBreak::Left),
            "right" => Ok(PageBreak::Right),
            "recto" => Ok(PageBreak::Recto),
            "verso" => Ok(PageBreak::Verso),
            "column" => Ok(PageBreak::Column),
            "avoid-column" => Ok(PageBreak::AvoidColumn),
            _ => Err(PageBreakParseError::InvalidValue(input)),
        }
    }

    // -- BreakInside parser

    /// Error returned when parsing a `break-inside` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum BreakInsideParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BreakInsideParseError<'a>);
    impl_display! { BreakInsideParseError<'a>, {
        InvalidValue(v) => format!("Invalid break-inside value: \"{}\"", v),
    }}

    /// Owned version of [`BreakInsideParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum BreakInsideParseErrorOwned {
        InvalidValue(AzString),
    }

    impl BreakInsideParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> BreakInsideParseErrorOwned {
            match self {
                Self::InvalidValue(s) => BreakInsideParseErrorOwned::InvalidValue((*s).to_string().into()),
            }
        }
    }

    impl BreakInsideParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> BreakInsideParseError<'_> {
            match self {
                Self::InvalidValue(s) => BreakInsideParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `break-inside` value.
    pub fn parse_break_inside(
        input: &str,
    ) -> Result<BreakInside, BreakInsideParseError<'_>> {
        match input.trim() {
            "auto" => Ok(BreakInside::Auto),
            "avoid" => Ok(BreakInside::Avoid),
            "avoid-page" => Ok(BreakInside::AvoidPage),
            "avoid-column" => Ok(BreakInside::AvoidColumn),
            _ => Err(BreakInsideParseError::InvalidValue(input)),
        }
    }

    // -- Widows / Orphans parsers

    macro_rules! define_widow_orphan_parser {
        ($fn_name:ident, $struct_name:ident, $error_name:ident, $error_owned_name:ident, $prop_name:expr) => {
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                ParseInt(ParseIntError, &'a str),
                ParseIntOwned(&'a str, &'a str),
                NegativeValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                ParseInt(e, s) => format!("Invalid integer for {}: \"{}\". Reason: {}", $prop_name, s, e),
                ParseIntOwned(e, s) => format!("Invalid integer for {}: \"{}\". Reason: {}", $prop_name, s, e),
                NegativeValue(s) => format!("Invalid value for {}: \"{}\". Value cannot be negative.", $prop_name, s),
            }}

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                ParseInt(ParseIntErrorWithInput),
                NegativeValue(AzString),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::ParseInt(e, s) => $error_owned_name::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: s.to_string().into() }),
                        Self::ParseIntOwned(e, s) => $error_owned_name::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: s.to_string().into() }),
                        Self::NegativeValue(s) => $error_owned_name::NegativeValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                     match self {
                        Self::ParseInt(e) => $error_name::ParseIntOwned(e.error.as_str(), e.input.as_str()),
                        Self::NegativeValue(s) => $error_name::NegativeValue(s),
                    }
                }
            }

            /// # Errors
            ///
            /// Returns an error if `input` is not a valid CSS value for this property.
            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                let trimmed = input.trim();
                let val: i32 = trimmed.parse().map_err(|e| $error_name::ParseInt(e, trimmed))?;
                if val < 0 {
                    return Err($error_name::NegativeValue(trimmed));
                }
                Ok($struct_name { inner: u32::try_from(val).unwrap_or(0) })
            }
        };
    }

    define_widow_orphan_parser!(
        parse_widows,
        Widows,
        WidowsParseError,
        WidowsParseErrorOwned,
        "widows"
    );
    define_widow_orphan_parser!(
        parse_orphans,
        Orphans,
        OrphansParseError,
        OrphansParseErrorOwned,
        "orphans"
    );

    // -- BoxDecorationBreak parser

    /// Error returned when parsing a `box-decoration-break` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum BoxDecorationBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BoxDecorationBreakParseError<'a>);
    impl_display! { BoxDecorationBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-decoration-break value: \"{}\"", v),
    }}

    /// Owned version of [`BoxDecorationBreakParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum BoxDecorationBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl BoxDecorationBreakParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> BoxDecorationBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => {
                    BoxDecorationBreakParseErrorOwned::InvalidValue((*s).to_string().into())
                }
            }
        }
    }

    impl BoxDecorationBreakParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> BoxDecorationBreakParseError<'_> {
            match self {
                Self::InvalidValue(s) => BoxDecorationBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `box-decoration-break` value.
    pub fn parse_box_decoration_break(
        input: &str,
    ) -> Result<BoxDecorationBreak, BoxDecorationBreakParseError<'_>> {
        match input.trim() {
            "slice" => Ok(BoxDecorationBreak::Slice),
            "clone" => Ok(BoxDecorationBreak::Clone),
            _ => Err(BoxDecorationBreakParseError::InvalidValue(input)),
        }
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_break() {
        assert_eq!(parse_page_break("auto").unwrap(), PageBreak::Auto);
        assert_eq!(parse_page_break("page").unwrap(), PageBreak::Page);
        assert_eq!(
            parse_page_break("avoid-column").unwrap(),
            PageBreak::AvoidColumn
        );
        assert!(parse_page_break("invalid").is_err());
    }

    #[test]
    fn test_parse_break_inside() {
        assert_eq!(parse_break_inside("auto").unwrap(), BreakInside::Auto);
        assert_eq!(parse_break_inside("avoid").unwrap(), BreakInside::Avoid);
        assert!(parse_break_inside("always").is_err());
    }

    #[test]
    fn test_parse_widows_orphans() {
        assert_eq!(parse_widows("3").unwrap().inner, 3);
        assert_eq!(parse_orphans("  1  ").unwrap().inner, 1);
        assert!(parse_widows("-2").is_err());
        assert!(parse_orphans("auto").is_err());
    }

    #[test]
    fn test_parse_box_decoration_break() {
        assert_eq!(
            parse_box_decoration_break("slice").unwrap(),
            BoxDecorationBreak::Slice
        );
        assert_eq!(
            parse_box_decoration_break("clone").unwrap(),
            BoxDecorationBreak::Clone
        );
        assert!(parse_box_decoration_break("copy").is_err());
    }
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use alloc::{format, string::String, vec::Vec};

    use super::*;
    use crate::props::formatter::PrintAsCssValue;

    /// Every `PageBreak` variant, so the round-trip tests stay exhaustive if a
    /// variant is added.
    const ALL_PAGE_BREAKS: [PageBreak; 12] = [
        PageBreak::Auto,
        PageBreak::Avoid,
        PageBreak::Always,
        PageBreak::All,
        PageBreak::Page,
        PageBreak::AvoidPage,
        PageBreak::Left,
        PageBreak::Right,
        PageBreak::Recto,
        PageBreak::Verso,
        PageBreak::Column,
        PageBreak::AvoidColumn,
    ];

    const ALL_BREAK_INSIDES: [BreakInside; 4] = [
        BreakInside::Auto,
        BreakInside::Avoid,
        BreakInside::AvoidPage,
        BreakInside::AvoidColumn,
    ];

    const ALL_BOX_DECORATION_BREAKS: [BoxDecorationBreak; 2] =
        [BoxDecorationBreak::Slice, BoxDecorationBreak::Clone];

    /// Inputs that must never parse and must never panic, for any of the
    /// keyword parsers.
    fn hostile_inputs() -> Vec<String> {
        let mut v = Vec::new();
        for s in [
            "",
            " ",
            "   ",
            "\t\n\r\x0c",
            "\0",
            "auto\0",
            "\0auto",
            ";",
            "auto;",
            "auto;garbage",
            "auto garbage",
            "auto auto",
            "auto/**/",
            "/* auto */",
            "\"auto\"",
            "'auto'",
            "-",
            "--",
            "0",
            "-0",
            "+0",
            "1",
            "-1",
            "0.0",
            "1e309",
            "-1e309",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551615", // u64::MAX
            "4294967295",           // u32::MAX
            "AUTO",
            "Auto",
            "aUtO",
            "AVOID-PAGE",
            "\u{1F600}",
            "auto\u{1F600}",
            "\u{0301}",       // lone combining acute accent
            "auto\u{0301}",   // "auto" + combining mark
            "\u{200B}auto",   // zero-width space (NOT Unicode whitespace)
            "auto\u{200B}",
            "\u{FEFF}auto",   // BOM (NOT Unicode whitespace)
            "аuto",           // Cyrillic 'а' (U+0430) homoglyph
            "auto\u{0000}\u{FFFD}",
            "initial",
            "inherit",
            "unset",
            "revert",
            "none",
        ] {
            v.push(String::from(s));
        }
        v
    }

    // ---------------------------------------------------------------
    // parse_page_break
    // ---------------------------------------------------------------

    #[test]
    fn page_break_valid_minimal() {
        assert_eq!(parse_page_break("auto"), Ok(PageBreak::Auto));
    }

    #[test]
    fn page_break_all_keywords_parse() {
        for expected in ALL_PAGE_BREAKS {
            let printed = expected.print_as_css_value();
            assert_eq!(
                parse_page_break(&printed),
                Ok(expected),
                "keyword {printed:?} must parse back"
            );
        }
    }

    /// Round-trip: `print_as_css_value` -> `parse_page_break` is the identity on
    /// every variant, and the printed form is stable across a second pass.
    #[test]
    fn page_break_round_trip_is_identity() {
        for expected in ALL_PAGE_BREAKS {
            let once = expected.print_as_css_value();
            let decoded = parse_page_break(&once).expect("printed value must re-parse");
            assert_eq!(decoded, expected);
            assert_eq!(decoded.print_as_css_value(), once, "encoding must be stable");
        }
    }

    /// Printed forms must be distinct, otherwise the round-trip above would be
    /// lossy (two variants collapsing onto one keyword).
    #[test]
    fn page_break_printed_forms_are_distinct() {
        let mut seen: Vec<String> = Vec::new();
        for pb in ALL_PAGE_BREAKS {
            let s = pb.print_as_css_value();
            assert!(!seen.contains(&s), "duplicate printed form: {s:?}");
            seen.push(s);
        }
        assert_eq!(seen.len(), ALL_PAGE_BREAKS.len());
    }

    #[test]
    fn page_break_hostile_inputs_are_rejected_without_panic() {
        for input in hostile_inputs() {
            let result = parse_page_break(&input);
            assert!(
                result.is_err(),
                "expected Err for {input:?}, got {result:?}"
            );
            // The error must be constructible/printable for every input.
            let err = result.unwrap_err();
            let _ = format!("{err}");
            let _ = err.to_contained();
        }
    }

    /// The error borrows the *untrimmed* input, not the trimmed slice.
    #[test]
    fn page_break_error_carries_untrimmed_input() {
        let err = parse_page_break("  bogus  ").unwrap_err();
        let PageBreakParseError::InvalidValue(v) = err;
        assert_eq!(v, "  bogus  ");
    }

    /// Surrounding ASCII whitespace is trimmed; interior junk is not.
    #[test]
    fn page_break_leading_trailing_junk() {
        assert_eq!(parse_page_break("  auto  "), Ok(PageBreak::Auto));
        assert_eq!(parse_page_break("\n\tavoid-page\r\n"), Ok(PageBreak::AvoidPage));
        assert!(parse_page_break("auto;").is_err());
        assert!(parse_page_break("auto garbage").is_err());
        assert!(parse_page_break("(auto)").is_err());
    }

    /// Characterization: `str::trim` follows the Unicode `White_Space` property,
    /// so NBSP (U+00A0) *is* stripped even though CSS tokenization would not
    /// treat it as whitespace. ZWSP/BOM are not whitespace and stay.
    #[test]
    fn page_break_unicode_whitespace_semantics() {
        assert_eq!(
            parse_page_break("\u{00A0}auto\u{00A0}"),
            Ok(PageBreak::Auto),
            "NBSP is Unicode whitespace, so trim() removes it (lenient vs. CSS)"
        );
        assert!(parse_page_break("\u{200B}auto").is_err(), "ZWSP is not whitespace");
        assert!(parse_page_break("\u{FEFF}auto").is_err(), "BOM is not whitespace");
    }

    /// Characterization: keyword matching is byte-exact, so CSS's ASCII
    /// case-insensitivity for keywords is *not* implemented.
    #[test]
    fn page_break_is_case_sensitive() {
        assert!(parse_page_break("AUTO").is_err());
        assert!(parse_page_break("Auto").is_err());
        assert!(parse_page_break("Avoid-Column").is_err());
        assert_eq!(parse_page_break("auto"), Ok(PageBreak::Auto));
    }

    #[test]
    fn page_break_extremely_long_input_does_not_hang() {
        let long = "auto".repeat(250_000); // 1M chars, trims to itself
        assert_eq!(long.len(), 1_000_000);
        assert!(parse_page_break(&long).is_err());

        // 1M chars of pure padding around a valid keyword still trims to "auto".
        let padded = format!("{}auto{}", " ".repeat(500_000), "\t".repeat(500_000));
        assert_eq!(parse_page_break(&padded), Ok(PageBreak::Auto));

        // A 1M-char run of a single byte must not blow up either.
        let blob = "x".repeat(1_000_000);
        assert!(parse_page_break(&blob).is_err());
    }

    #[test]
    fn page_break_deeply_nested_input_does_not_stack_overflow() {
        let nested = format!("{}auto{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_page_break(&nested).is_err());
    }

    // ---------------------------------------------------------------
    // parse_break_inside
    // ---------------------------------------------------------------

    #[test]
    fn break_inside_valid_minimal() {
        assert_eq!(parse_break_inside("auto"), Ok(BreakInside::Auto));
    }

    #[test]
    fn break_inside_round_trip_is_identity() {
        for expected in ALL_BREAK_INSIDES {
            let printed = expected.print_as_css_value();
            let decoded = parse_break_inside(&printed).expect("printed value must re-parse");
            assert_eq!(decoded, expected);
            assert_eq!(decoded.print_as_css_value(), printed);
        }
    }

    /// `break-inside` accepts a strict subset of the `page-break` keywords;
    /// the ones it does not accept must be rejected rather than silently
    /// falling through to `Auto`.
    #[test]
    fn break_inside_rejects_page_break_only_keywords() {
        for pb in ALL_PAGE_BREAKS {
            let kw = pb.print_as_css_value();
            let accepted = ALL_BREAK_INSIDES
                .iter()
                .any(|bi| bi.print_as_css_value() == kw);
            assert_eq!(
                parse_break_inside(&kw).is_ok(),
                accepted,
                "break-inside acceptance of {kw:?} must match its keyword set"
            );
        }
        assert!(parse_break_inside("always").is_err());
        assert!(parse_break_inside("left").is_err());
        assert!(parse_break_inside("recto").is_err());
    }

    #[test]
    fn break_inside_hostile_inputs_are_rejected_without_panic() {
        for input in hostile_inputs() {
            let result = parse_break_inside(&input);
            assert!(
                result.is_err(),
                "expected Err for {input:?}, got {result:?}"
            );
            let err = result.unwrap_err();
            let _ = format!("{err}");
            let _ = err.to_contained();
        }
    }

    #[test]
    fn break_inside_error_carries_untrimmed_input() {
        let err = parse_break_inside(" \u{1F600} ").unwrap_err();
        let BreakInsideParseError::InvalidValue(v) = err;
        assert_eq!(v, " \u{1F600} ");
    }

    #[test]
    fn break_inside_extremely_long_input_does_not_hang() {
        let long = "avoid-column".repeat(100_000);
        assert!(parse_break_inside(&long).is_err());

        let nested = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(parse_break_inside(&nested).is_err());
    }

    // ---------------------------------------------------------------
    // parse_box_decoration_break
    // ---------------------------------------------------------------

    #[test]
    fn box_decoration_break_valid_minimal() {
        assert_eq!(
            parse_box_decoration_break("slice"),
            Ok(BoxDecorationBreak::Slice)
        );
    }

    #[test]
    fn box_decoration_break_round_trip_is_identity() {
        for expected in ALL_BOX_DECORATION_BREAKS {
            let printed = expected.print_as_css_value();
            let decoded =
                parse_box_decoration_break(&printed).expect("printed value must re-parse");
            assert_eq!(decoded, expected);
            assert_eq!(decoded.print_as_css_value(), printed);
        }
    }

    #[test]
    fn box_decoration_break_hostile_inputs_are_rejected_without_panic() {
        for input in hostile_inputs() {
            let result = parse_box_decoration_break(&input);
            assert!(
                result.is_err(),
                "expected Err for {input:?}, got {result:?}"
            );
            let err = result.unwrap_err();
            let _ = format!("{err}");
            let _ = err.to_contained();
        }
        // "clone" is a keyword here but nowhere else; "copy"/"slice-clone" are not.
        assert!(parse_box_decoration_break("copy").is_err());
        assert!(parse_box_decoration_break("slice-clone").is_err());
        assert!(parse_box_decoration_break("Clone").is_err());
    }

    #[test]
    fn box_decoration_break_whitespace_and_long_input() {
        assert_eq!(
            parse_box_decoration_break("\t\n clone \r\n"),
            Ok(BoxDecorationBreak::Clone)
        );
        let long = "slice".repeat(200_000);
        assert!(parse_box_decoration_break(&long).is_err());
    }

    // ---------------------------------------------------------------
    // Error <-> Owned conversions (to_contained / to_shared)
    // ---------------------------------------------------------------

    #[test]
    fn page_break_error_to_contained_basic_and_round_trip() {
        let shared = PageBreakParseError::InvalidValue("bogus");
        let owned = shared.to_contained();
        assert_eq!(
            owned,
            PageBreakParseErrorOwned::InvalidValue(String::from("bogus").into())
        );
        // shared -> owned -> shared is lossless
        assert_eq!(owned.to_shared(), shared);
        // and owned -> shared -> owned is idempotent
        assert_eq!(owned.to_shared().to_contained(), owned);
    }

    /// Empty / whitespace / huge / non-ASCII payloads must survive the FFI
    /// round-trip byte-for-byte and must not panic.
    #[test]
    fn page_break_error_to_contained_edge_payloads() {
        let huge = "\u{1F600}".repeat(100_000);
        for payload in [
            String::new(),
            String::from(" "),
            String::from("\0"),
            String::from("\u{1F600}\u{0301}"),
            String::from("\u{FFFD}"),
            huge,
        ] {
            let shared = PageBreakParseError::InvalidValue(payload.as_str());
            let owned = shared.to_contained();
            let back = owned.to_shared();
            let PageBreakParseError::InvalidValue(v) = back;
            assert_eq!(v, payload.as_str());
            assert_eq!(owned.to_shared().to_contained(), owned);
            let _ = format!("{shared}");
        }
    }

    #[test]
    fn break_inside_error_to_contained_round_trip() {
        for payload in ["", " ", "always", "\u{1F600}", "\0\0\0"] {
            let shared = BreakInsideParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                BreakInsideParseErrorOwned::InvalidValue(String::from(payload).into())
            );
            assert_eq!(owned.to_shared(), shared);
            assert_eq!(owned.to_shared().to_contained(), owned);
            let _ = format!("{shared}");
        }
    }

    #[test]
    fn box_decoration_break_error_to_contained_round_trip() {
        for payload in ["", "copy", "  ", "\u{1F600}"] {
            let shared = BoxDecorationBreakParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                BoxDecorationBreakParseErrorOwned::InvalidValue(String::from(payload).into())
            );
            assert_eq!(owned.to_shared(), shared);
            assert_eq!(owned.to_shared().to_contained(), owned);
            let _ = format!("{shared}");
        }
    }

    /// The `Display` impl embeds the raw input; a 1M-char input must format
    /// without panicking (and must actually contain the payload).
    #[test]
    fn error_display_handles_huge_payload() {
        let payload = "x".repeat(1_000_000);
        let err = parse_page_break(&payload).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(&payload));
        assert!(msg.starts_with("Invalid break value: \""));
    }

    // ---------------------------------------------------------------
    // Widows / Orphans: numeric limits, overflow, saturation
    // ---------------------------------------------------------------

    #[test]
    fn widows_orphans_defaults_are_two() {
        assert_eq!(Widows::default().inner, 2);
        assert_eq!(Orphans::default().inner, 2);
        assert_eq!(Widows::default().print_as_css_value(), "2");
        assert_eq!(Orphans::default().print_as_css_value(), "2");
    }

    /// Round-trip through the printed form for boundary values that are
    /// representable (i.e. `<= i32::MAX`, since the parser goes through `i32`).
    #[test]
    fn widows_round_trip_representable_values() {
        for inner in [0_u32, 1, 2, 3, 100, 65_535, 2_147_483_647] {
            let printed = Widows { inner }.print_as_css_value();
            assert_eq!(parse_widows(&printed).unwrap().inner, inner);
            let printed = Orphans { inner }.print_as_css_value();
            assert_eq!(parse_orphans(&printed).unwrap().inner, inner);
        }
    }

    /// Characterization: parsing goes through `i32`, so values above
    /// `i32::MAX` are *rejected*, not saturated — even though the field is
    /// `u32` and can hold them. `Widows { inner: u32::MAX }` therefore does not
    /// survive a print/parse round-trip.
    #[test]
    fn widows_above_i32_max_is_rejected_not_saturated() {
        assert!(parse_widows("2147483648").is_err()); // i32::MAX + 1
        assert!(parse_widows("4294967295").is_err()); // u32::MAX
        assert!(parse_orphans("4294967295").is_err());

        let printed = Widows { inner: u32::MAX }.print_as_css_value();
        assert_eq!(printed, "4294967295");
        assert!(
            parse_widows(&printed).is_err(),
            "u32::MAX widows cannot round-trip through the i32-based parser"
        );
    }

    #[test]
    fn widows_negative_and_zero_boundaries() {
        // "-0" parses as 0 and is *not* treated as negative.
        assert_eq!(parse_widows("-0").unwrap().inner, 0);
        assert_eq!(parse_orphans("-0").unwrap().inner, 0);
        assert_eq!(parse_widows("0").unwrap().inner, 0);

        assert!(matches!(
            parse_widows("-1"),
            Err(WidowsParseError::NegativeValue("-1"))
        ));
        assert!(matches!(
            parse_widows("-2147483648"), // i32::MIN, parses then fails the sign check
            Err(WidowsParseError::NegativeValue("-2147483648"))
        ));
        assert!(matches!(
            parse_orphans("-1"),
            Err(OrphansParseError::NegativeValue("-1"))
        ));
    }

    /// The `NegativeValue` / `ParseInt` errors carry the *trimmed* input (unlike
    /// the keyword parsers, which carry the raw input).
    #[test]
    fn widows_error_carries_trimmed_input() {
        assert!(matches!(
            parse_widows("  -5  "),
            Err(WidowsParseError::NegativeValue("-5"))
        ));
        match parse_widows("  abc  ") {
            Err(WidowsParseError::ParseInt(_, s)) => assert_eq!(s, "abc"),
            other => panic!("expected ParseInt error, got {other:?}"),
        }
    }

    #[test]
    fn widows_orphans_reject_non_integers_without_panic() {
        for input in [
            "",
            "   ",
            "\t\n",
            "auto",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
            "1e309",
            "0.0",
            "2.5",
            "1_000",
            "0x10",
            "1 2",
            "2;",
            "١٢", // Arabic-Indic digits — not accepted by i32::from_str
            "\u{1F600}",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551615", // u64::MAX
            "+",
            "-",
        ] {
            let w = parse_widows(input);
            assert!(w.is_err(), "expected Err for widows {input:?}, got {w:?}");
            let _ = format!("{}", w.unwrap_err());

            let o = parse_orphans(input);
            assert!(o.is_err(), "expected Err for orphans {input:?}, got {o:?}");
            let _ = format!("{}", o.unwrap_err());
        }
    }

    /// A leading `+` is accepted by `i32::from_str`, so it is accepted here too.
    #[test]
    fn widows_accepts_explicit_plus_sign() {
        assert_eq!(parse_widows("+3").unwrap().inner, 3);
        assert_eq!(parse_orphans("  +0  ").unwrap().inner, 0);
    }

    #[test]
    fn widows_extremely_long_digit_run_does_not_hang() {
        let digits = "9".repeat(1_000_000);
        let err = parse_widows(&digits).unwrap_err();
        let _ = format!("{err}"); // Display embeds the 1M-char input
        assert!(parse_orphans(&digits).is_err());
    }

    /// `to_contained` folds both `ParseInt` and `ParseIntOwned` onto the single
    /// owned `ParseInt` variant, so `owned -> shared -> owned` must be a
    /// fixed point (it is not the identity on the *shared* side).
    #[test]
    fn widows_error_owned_round_trip_is_a_fixed_point() {
        let parse_err = "abc".parse::<i32>().unwrap_err();
        let shared = WidowsParseError::ParseInt(parse_err, "abc");
        let owned = shared.to_contained();

        let back = owned.to_shared();
        // shared -> owned -> shared is *lossy*: ParseInt becomes ParseIntOwned.
        match &back {
            WidowsParseError::ParseIntOwned(_, input) => assert_eq!(*input, "abc"),
            other => panic!("expected ParseIntOwned, got {other:?}"),
        }
        // ...but the owned representation is stable under a further round-trip.
        assert_eq!(back.to_contained(), owned);

        // Both spellings render the same message.
        assert_eq!(format!("{shared}"), format!("{back}"));

        let neg = WidowsParseError::NegativeValue("-7");
        let neg_owned = neg.to_contained();
        assert_eq!(neg_owned.to_shared(), neg);
        assert_eq!(neg_owned.to_shared().to_contained(), neg_owned);
    }

    #[test]
    fn orphans_error_owned_round_trip_is_a_fixed_point() {
        let err = parse_orphans("nope").unwrap_err();
        let owned = err.to_contained();
        assert_eq!(owned.to_shared().to_contained(), owned);
        let _ = format!("{}", owned.to_shared());

        let neg = parse_orphans("-3").unwrap_err().to_contained();
        assert_eq!(neg.to_shared().to_contained(), neg);
    }

    // ---------------------------------------------------------------
    // Value-type invariants
    // ---------------------------------------------------------------

    #[test]
    fn enum_defaults_match_the_auto_slice_keywords() {
        assert_eq!(PageBreak::default(), PageBreak::Auto);
        assert_eq!(BreakInside::default(), BreakInside::Auto);
        assert_eq!(BoxDecorationBreak::default(), BoxDecorationBreak::Slice);

        assert_eq!(
            parse_page_break(&PageBreak::default().print_as_css_value()),
            Ok(PageBreak::default())
        );
        assert_eq!(
            parse_break_inside(&BreakInside::default().print_as_css_value()),
            Ok(BreakInside::default())
        );
        assert_eq!(
            parse_box_decoration_break(&BoxDecorationBreak::default().print_as_css_value()),
            Ok(BoxDecorationBreak::default())
        );
    }

    /// `Ord` is derived, so it follows declaration order. Anything relying on
    /// `PageBreak::Auto` sorting first (e.g. a `BTreeMap` keyed by these) would
    /// break if the variants were reordered.
    #[test]
    fn derived_ord_follows_declaration_order() {
        let mut sorted = ALL_PAGE_BREAKS;
        sorted.sort_unstable();
        assert_eq!(sorted, ALL_PAGE_BREAKS);
        assert!(PageBreak::Auto < PageBreak::AvoidColumn);

        let mut bi = ALL_BREAK_INSIDES;
        bi.sort_unstable();
        assert_eq!(bi, ALL_BREAK_INSIDES);

        assert!(BoxDecorationBreak::Slice < BoxDecorationBreak::Clone);
    }

    #[test]
    fn widows_orphans_print_saturating_extremes() {
        assert_eq!(Widows { inner: 0 }.print_as_css_value(), "0");
        assert_eq!(
            Widows { inner: u32::MAX }.print_as_css_value(),
            "4294967295"
        );
        assert_eq!(
            Orphans { inner: u32::MAX }.print_as_css_value(),
            "4294967295"
        );
        // Ord on the newtypes follows the inner u32.
        assert!(Widows { inner: 0 } < Widows { inner: u32::MAX });
        assert!(Orphans { inner: 1 } < Orphans::default());
    }
}
