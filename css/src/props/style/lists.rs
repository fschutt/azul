//! CSS list styling properties (`list-style-type` and `list-style-position`)

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{codegen::format::FormatAsRustCode, props::formatter::PrintAsCssValue};

// --- list-style-type ---

/// CSS `list-style-type` property — controls the marker style for list items.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleListStyleType {
    None,
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerGreek,
    UpperGreek,
    LowerAlpha,
    UpperAlpha,
}


impl PrintAsCssValue for StyleListStyleType {
    fn print_as_css_value(&self) -> String {
        use StyleListStyleType::{None, Disc, Circle, Square, Decimal, DecimalLeadingZero, LowerRoman, UpperRoman, LowerGreek, UpperGreek, LowerAlpha, UpperAlpha};
        String::from(match self {
            None => "none",
            Disc => "disc",
            Circle => "circle",
            Square => "square",
            Decimal => "decimal",
            DecimalLeadingZero => "decimal-leading-zero",
            LowerRoman => "lower-roman",
            UpperRoman => "upper-roman",
            LowerGreek => "lower-greek",
            UpperGreek => "upper-greek",
            LowerAlpha => "lower-alpha",
            UpperAlpha => "upper-alpha",
        })
    }
}

impl FormatAsRustCode for StyleListStyleType {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleListStyleType::{None, Disc, Circle, Square, Decimal, DecimalLeadingZero, LowerRoman, UpperRoman, LowerGreek, UpperGreek, LowerAlpha, UpperAlpha};
        format!(
            "StyleListStyleType::{}",
            match self {
                None => "None",
                Disc => "Disc",
                Circle => "Circle",
                Square => "Square",
                Decimal => "Decimal",
                DecimalLeadingZero => "DecimalLeadingZero",
                LowerRoman => "LowerRoman",
                UpperRoman => "UpperRoman",
                LowerGreek => "LowerGreek",
                UpperGreek => "UpperGreek",
                LowerAlpha => "LowerAlpha",
                UpperAlpha => "UpperAlpha",
            }
        )
    }
}

impl fmt::Display for StyleListStyleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

// --- list-style-position ---

/// CSS `list-style-position` property — controls whether the marker is inside or outside the list item box.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleListStylePosition {
    Inside,
    #[default]
    Outside,
}


impl PrintAsCssValue for StyleListStylePosition {
    fn print_as_css_value(&self) -> String {
        use StyleListStylePosition::{Inside, Outside};
        String::from(match self {
            Inside => "inside",
            Outside => "outside",
        })
    }
}

impl FormatAsRustCode for StyleListStylePosition {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleListStylePosition::{Inside, Outside};
        format!(
            "StyleListStylePosition::{}",
            match self {
                Inside => "Inside",
                Outside => "Outside",
            }
        )
    }
}

impl fmt::Display for StyleListStylePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

// --- Parsing Logic ---

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleListStyleTypeParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleListStyleTypeParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { StyleListStyleTypeParseError<'a>, {
    InvalidValue(val) => format!("Invalid list-style-type value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleListStyleTypeParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl StyleListStyleTypeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleListStyleTypeParseErrorOwned {
        match self {
            Self::InvalidValue(s) => StyleListStyleTypeParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl StyleListStyleTypeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleListStyleTypeParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleListStyleTypeParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parses a CSS `list-style-type` value from a string.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `list-style-type` value.
pub fn parse_style_list_style_type(
    input: &str,
) -> Result<StyleListStyleType, StyleListStyleTypeParseError<'_>> {
    let input = input.trim();
    match input {
        "none" => Ok(StyleListStyleType::None),
        "disc" => Ok(StyleListStyleType::Disc),
        "circle" => Ok(StyleListStyleType::Circle),
        "square" => Ok(StyleListStyleType::Square),
        "decimal" => Ok(StyleListStyleType::Decimal),
        "decimal-leading-zero" => Ok(StyleListStyleType::DecimalLeadingZero),
        "lower-roman" => Ok(StyleListStyleType::LowerRoman),
        "upper-roman" => Ok(StyleListStyleType::UpperRoman),
        "lower-greek" => Ok(StyleListStyleType::LowerGreek),
        "upper-greek" => Ok(StyleListStyleType::UpperGreek),
        "lower-alpha" | "lower-latin" => Ok(StyleListStyleType::LowerAlpha),
        "upper-alpha" | "upper-latin" => Ok(StyleListStyleType::UpperAlpha),
        _ => Err(StyleListStyleTypeParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleListStylePositionParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleListStylePositionParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { StyleListStylePositionParseError<'a>, {
    InvalidValue(val) => format!("Invalid list-style-position value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleListStylePositionParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl StyleListStylePositionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleListStylePositionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleListStylePositionParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleListStylePositionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleListStylePositionParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleListStylePositionParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parses a CSS `list-style-position` value from a string.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `list-style-position` value.
pub fn parse_style_list_style_position(
    input: &str,
) -> Result<StyleListStylePosition, StyleListStylePositionParseError<'_>> {
    let input = input.trim();
    match input {
        "inside" => Ok(StyleListStylePosition::Inside),
        "outside" => Ok(StyleListStylePosition::Outside),
        _ => Err(StyleListStylePositionParseError::InvalidValue(input)),
    }
}

#[cfg(test)]
mod autotest_generated {
    //! Adversarial tests for the two list-style keyword enums: serializer
    //! well-formedness, `print_as_css_value` -> `parse_*` round-trips over every
    //! variant, parser abuse (empty / whitespace / garbage / megabyte-sized /
    //! unicode / deeply nested input) and the `to_contained` / `to_shared` error
    //! conversion invariants.
    //!
    //! Both parsers are pure keyword matchers over a trimmed `&str`, so the
    //! interesting failure modes are (a) panicking or hanging on hostile input,
    //! (b) losing or corrupting the borrowed error payload, and (c) a variant
    //! whose serialized form does not parse back to itself.

    use std::collections::BTreeSet;

    use super::*;

    const ALL_TYPES: [StyleListStyleType; 12] = [
        StyleListStyleType::None,
        StyleListStyleType::Disc,
        StyleListStyleType::Circle,
        StyleListStyleType::Square,
        StyleListStyleType::Decimal,
        StyleListStyleType::DecimalLeadingZero,
        StyleListStyleType::LowerRoman,
        StyleListStyleType::UpperRoman,
        StyleListStyleType::LowerGreek,
        StyleListStyleType::UpperGreek,
        StyleListStyleType::LowerAlpha,
        StyleListStyleType::UpperAlpha,
    ];

    const ALL_POSITIONS: [StyleListStylePosition; 2] =
        [StyleListStylePosition::Inside, StyleListStylePosition::Outside];

    /// Hostile inputs that must never be accepted, never panic and never hang.
    #[cfg(feature = "parser")]
    fn hostile_inputs() -> Vec<String> {
        let mut v = vec![
            String::new(),
            " ".to_string(),
            "   \t\n\r".to_string(),
            "\u{0c}\u{0b}".to_string(),
            "\0".to_string(),
            "disc\0".to_string(),
            "\0disc".to_string(),
            "-".to_string(),
            "--".to_string(),
            ";".to_string(),
            "{}".to_string(),
            "/* disc */".to_string(),
            "disc;garbage".to_string(),
            "disc disc".to_string(),
            "disc,circle".to_string(),
            "disc!important".to_string(),
            "inside;".to_string(),
            "list-style-type: disc".to_string(),
            "lower_roman".to_string(),
            "lower - roman".to_string(),
            "lowerroman".to_string(),
            // boundary numerics
            "0".to_string(),
            "-0".to_string(),
            "NaN".to_string(),
            "nan".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "infinity".to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            u64::MAX.to_string(),
            f64::MAX.to_string(),
            f64::MIN_POSITIVE.to_string(),
            "1e308".to_string(),
            "-1e-308".to_string(),
            // unicode
            "\u{1F600}".to_string(),
            "disc\u{301}".to_string(),
            "\u{301}".to_string(),
            "\u{202E}disc".to_string(),
            "di\u{200B}sc".to_string(),
            "DISС".to_string(), // trailing char is Cyrillic U+0421, not ASCII C
            "diｓc".to_string(), // fullwidth s
            "круг".to_string(),
            "\u{FFFD}".to_string(),
        ];
        // Deeply nested / recursive-looking input must not blow the stack: these
        // parsers are non-recursive, so this is a regression guard.
        v.push("(".repeat(10_000));
        v.push("[".repeat(10_000));
        v.push("disc(".repeat(10_000));
        v.push(format!("{}disc{}", "(".repeat(10_000), ")".repeat(10_000)));
        v
    }

    // --- serializers -------------------------------------------------------

    #[test]
    fn css_values_are_well_formed_for_every_type() {
        for v in ALL_TYPES {
            let s = v.print_as_css_value();
            assert!(!s.is_empty(), "{v:?} serialized to an empty CSS value");
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{v:?} serialized to {s:?}, which is not a bare lowercase CSS keyword"
            );
            assert!(!s.starts_with('-') && !s.ends_with('-'), "{v:?} -> {s:?}");
        }
    }

    #[test]
    fn css_values_are_well_formed_for_every_position() {
        for v in ALL_POSITIONS {
            let s = v.print_as_css_value();
            assert!(!s.is_empty(), "{v:?} serialized to an empty CSS value");
            assert!(s.chars().all(|c| c.is_ascii_lowercase()), "{v:?} -> {s:?}");
        }
    }

    #[test]
    fn css_values_are_unique() {
        let types: BTreeSet<String> = ALL_TYPES.iter().map(PrintAsCssValue::print_as_css_value).collect();
        assert_eq!(types.len(), ALL_TYPES.len(), "two list-style-type variants share a CSS keyword");

        let positions: BTreeSet<String> =
            ALL_POSITIONS.iter().map(PrintAsCssValue::print_as_css_value).collect();
        assert_eq!(positions.len(), ALL_POSITIONS.len());
    }

    #[test]
    fn display_agrees_with_print_as_css_value() {
        for v in ALL_TYPES {
            assert_eq!(v.to_string(), v.print_as_css_value(), "Display diverged for {v:?}");
        }
        for v in ALL_POSITIONS {
            assert_eq!(v.to_string(), v.print_as_css_value(), "Display diverged for {v:?}");
        }
    }

    #[test]
    fn display_of_default_is_the_css_initial_value() {
        // CSS initial values: list-style-type: disc, list-style-position: outside.
        assert_eq!(StyleListStyleType::default(), StyleListStyleType::Disc);
        assert_eq!(StyleListStyleType::default().to_string(), "disc");
        assert_eq!(StyleListStylePosition::default(), StyleListStylePosition::Outside);
        assert_eq!(StyleListStylePosition::default().to_string(), "outside");
    }

    #[test]
    fn rust_code_names_the_variant_and_ignores_the_tab_argument() {
        for v in ALL_TYPES {
            // Debug is derived, so it yields the bare variant name.
            let expected = format!("StyleListStyleType::{v:?}");
            assert_eq!(v.format_as_rust_code(0), expected);
            // `_tabs` is unused: no indentation must leak in, at any depth.
            assert_eq!(v.format_as_rust_code(usize::MAX), expected);
        }
        for v in ALL_POSITIONS {
            let expected = format!("StyleListStylePosition::{v:?}");
            assert_eq!(v.format_as_rust_code(0), expected);
            assert_eq!(v.format_as_rust_code(usize::MAX), expected);
        }
    }

    // --- round-trips -------------------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn every_type_round_trips_through_its_css_value() {
        for v in ALL_TYPES {
            let printed = v.print_as_css_value();
            assert_eq!(
                parse_style_list_style_type(&printed),
                Ok(v),
                "{v:?} serialized to {printed:?}, which does not parse back"
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn every_position_round_trips_through_its_css_value() {
        for v in ALL_POSITIONS {
            let printed = v.print_as_css_value();
            assert_eq!(parse_style_list_style_position(&printed), Ok(v), "{v:?} -> {printed:?}");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parsing_is_idempotent_through_reserialization() {
        // parse -> print -> parse must reach a fixed point, including for the
        // alias spellings that normalize onto a different keyword.
        for input in [
            "disc", "none", "circle", "square", "decimal", "decimal-leading-zero", "lower-roman",
            "upper-roman", "lower-greek", "upper-greek", "lower-alpha", "upper-alpha",
            "lower-latin", "upper-latin",
        ] {
            let first = parse_style_list_style_type(input).expect("known-good keyword");
            let printed = first.print_as_css_value();
            let second = parse_style_list_style_type(&printed).expect("reserialized value must reparse");
            assert_eq!(first, second, "{input:?} was not idempotent (printed {printed:?})");
        }
    }

    // --- parser: positive controls ----------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn valid_keywords_map_to_the_expected_variants() {
        let table = [
            ("none", StyleListStyleType::None),
            ("disc", StyleListStyleType::Disc),
            ("circle", StyleListStyleType::Circle),
            ("square", StyleListStyleType::Square),
            ("decimal", StyleListStyleType::Decimal),
            ("decimal-leading-zero", StyleListStyleType::DecimalLeadingZero),
            ("lower-roman", StyleListStyleType::LowerRoman),
            ("upper-roman", StyleListStyleType::UpperRoman),
            ("lower-greek", StyleListStyleType::LowerGreek),
            ("upper-greek", StyleListStyleType::UpperGreek),
            ("lower-alpha", StyleListStyleType::LowerAlpha),
            ("upper-alpha", StyleListStyleType::UpperAlpha),
            // documented aliases
            ("lower-latin", StyleListStyleType::LowerAlpha),
            ("upper-latin", StyleListStyleType::UpperAlpha),
        ];
        for (input, expected) in table {
            assert_eq!(parse_style_list_style_type(input), Ok(expected), "input {input:?}");
        }

        assert_eq!(parse_style_list_style_position("inside"), Ok(StyleListStylePosition::Inside));
        assert_eq!(parse_style_list_style_position("outside"), Ok(StyleListStylePosition::Outside));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn ascii_whitespace_padding_is_trimmed() {
        for padded in ["  disc", "disc\n", "\t disc \r\n", "\u{0c}disc\u{0c}"] {
            assert_eq!(
                parse_style_list_style_type(padded),
                Ok(StyleListStyleType::Disc),
                "padding was not trimmed from {padded:?}"
            );
        }
        assert_eq!(
            parse_style_list_style_position("\n\t outside \t\n"),
            Ok(StyleListStylePosition::Outside)
        );
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parsers_agree_with_str_trim_on_unicode_whitespace() {
        // `str::trim` strips *Unicode* whitespace (U+00A0 NBSP, U+2003 EM SPACE,
        // U+3000 IDEOGRAPHIC SPACE), which CSS does not consider whitespace. The
        // parsers trim first, so whatever `trim` decides, they must stay
        // consistent with it rather than accepting a keyword `trim` left dirty.
        for pad in ["\u{a0}", "\u{2003}", "\u{3000}"] {
            let padded = format!("{pad}disc{pad}");
            assert_eq!(
                parse_style_list_style_type(&padded).is_ok(),
                padded.trim() == "disc",
                "parser and str::trim disagree on {padded:?}"
            );
        }
    }

    // --- parser: hostile input --------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn hostile_input_is_rejected_without_panicking() {
        for input in hostile_inputs() {
            let ty = parse_style_list_style_type(&input);
            assert!(ty.is_err(), "list-style-type accepted hostile input {input:?} as {ty:?}");

            let pos = parse_style_list_style_position(&input);
            assert!(pos.is_err(), "list-style-position accepted hostile input {input:?} as {pos:?}");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn keyword_matching_is_case_sensitive() {
        // NOTE: CSS keyword values are ASCII case-insensitive, so `DISC` /
        // `Inside` *should* parse per spec. These parsers are exact-match only;
        // the assertions below pin the current (stricter than spec) behavior.
        for input in ["Disc", "DISC", "dIsC", "LOWER-ROMAN", "Decimal-Leading-Zero", "NONE"] {
            assert!(
                parse_style_list_style_type(input).is_err(),
                "unexpectedly case-insensitive for {input:?}"
            );
        }
        for input in ["Inside", "OUTSIDE", "OuTsIdE"] {
            assert!(
                parse_style_list_style_position(input).is_err(),
                "unexpectedly case-insensitive for {input:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn megabyte_sized_input_terminates_and_is_rejected() {
        let long = "a".repeat(1_000_000);
        assert!(parse_style_list_style_type(&long).is_err());
        assert!(parse_style_list_style_position(&long).is_err());

        // A valid keyword repeated a quarter-million times is still not a keyword.
        let repeated = "disc".repeat(250_000);
        assert!(parse_style_list_style_type(&repeated).is_err());

        // Megabytes of padding around a valid keyword must still trim down to it.
        let padded = format!("{}disc{}", " ".repeat(1_000_000), "\n".repeat(1_000_000));
        assert_eq!(parse_style_list_style_type(&padded), Ok(StyleListStyleType::Disc));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn multibyte_input_is_rejected_without_slicing_panics() {
        // Trimming a string whose bytes straddle char boundaries must not panic
        // and must not truncate the error payload mid-codepoint.
        for input in ["🙂", " 🙂 ", "日本語", "e\u{301}", "\u{1F600}\u{1F600}\u{1F600}"] {
            match parse_style_list_style_type(input) {
                Ok(v) => panic!("multibyte input {input:?} parsed as {v:?}"),
                Err(StyleListStyleTypeParseError::InvalidValue(s)) => {
                    assert_eq!(s, input.trim(), "error payload was mangled for {input:?}");
                }
            }
        }
    }

    // --- parser: error payloads -------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn error_payload_is_the_trimmed_input() {
        match parse_style_list_style_type("  bogus-keyword  ") {
            Err(StyleListStyleTypeParseError::InvalidValue(s)) => assert_eq!(s, "bogus-keyword"),
            other => panic!("expected InvalidValue, got {other:?}"),
        }
        match parse_style_list_style_position("\t bogus \n") {
            Err(StyleListStylePositionParseError::InvalidValue(s)) => assert_eq!(s, "bogus"),
            other => panic!("expected InvalidValue, got {other:?}"),
        }
        // Whitespace-only input trims to the empty string, not to a `None`-ish value.
        match parse_style_list_style_type("   \t\n") {
            Err(StyleListStyleTypeParseError::InvalidValue(s)) => assert!(s.is_empty()),
            other => panic!("expected InvalidValue(\"\"), got {other:?}"),
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn error_display_quotes_the_offending_value() {
        let err = parse_style_list_style_type("🙂").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("list-style-type"), "{msg:?}");
        assert!(msg.contains('🙂'), "error message dropped the offending value: {msg:?}");

        let err = parse_style_list_style_position("").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("list-style-position"), "{msg:?}");
        assert!(!msg.is_empty());
    }

    // --- error conversions: to_contained / to_shared ------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn type_error_survives_a_contained_shared_round_trip() {
        let payloads = [
            "",
            " ",
            "bogus",
            "🙂 combining\u{301}",
            "\0embedded nul\0",
            "line\nbreak\r\n",
            "-",
        ];
        for payload in payloads {
            let shared = StyleListStyleTypeParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            match &owned {
                StyleListStyleTypeParseErrorOwned::InvalidValue(s) => {
                    assert_eq!(s.as_str(), payload, "to_contained mangled {payload:?}");
                }
            }
            assert_eq!(owned.to_shared(), shared, "round-trip lost data for {payload:?}");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn position_error_survives_a_contained_shared_round_trip() {
        for payload in ["", "bogus", "🙂", "\0", "  interior  spaces  "] {
            let shared = StyleListStylePositionParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            match &owned {
                StyleListStylePositionParseErrorOwned::InvalidValue(s) => {
                    assert_eq!(s.as_str(), payload);
                }
            }
            assert_eq!(owned.to_shared(), shared, "round-trip lost data for {payload:?}");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn owned_errors_constructed_directly_convert_back_to_shared() {
        let owned = StyleListStyleTypeParseErrorOwned::InvalidValue(String::new().into());
        match owned.to_shared() {
            StyleListStyleTypeParseError::InvalidValue(s) => assert_eq!(s, ""),
        }

        let owned = StyleListStylePositionParseErrorOwned::InvalidValue(String::from("x").into());
        match owned.to_shared() {
            StyleListStylePositionParseError::InvalidValue(s) => assert_eq!(s, "x"),
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn huge_error_payload_is_carried_without_truncation() {
        let huge = "z".repeat(100_000);
        let err = parse_style_list_style_type(&huge).unwrap_err();
        let owned = err.to_contained();
        match &owned {
            StyleListStyleTypeParseErrorOwned::InvalidValue(s) => {
                assert_eq!(s.as_str().len(), 100_000, "payload was truncated");
            }
        }
        assert_eq!(owned.to_shared(), StyleListStyleTypeParseError::InvalidValue(&huge));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn to_contained_is_repeatable_and_does_not_consume_the_error() {
        let err = parse_style_list_style_position("nope").unwrap_err();
        let a = err.to_contained();
        let b = err.to_contained();
        assert_eq!(a, b);
        assert_eq!(err, StyleListStylePositionParseError::InvalidValue("nope"));
    }
}
