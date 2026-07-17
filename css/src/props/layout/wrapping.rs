//! CSS properties for writing modes and clearing.
//!
//! Key types:
//! - [`LayoutWritingMode`] — `writing-mode` (`horizontal-tb`, `vertical-rl`, `vertical-lr`)
//! - [`LayoutClear`] — `clear` (`none`, `left`, `right`, `both`)
//!
//! Parse functions are gated behind the `parser` feature and are consumed
//! by the CSS property system in `property.rs`.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

// --- writing-mode (LayoutWritingMode) ---

// +spec:writing-modes:ec496c - writing-mode property: horizontal-tb, vertical-rl, vertical-lr block flow directions
// +spec:writing-modes:fdc4cc - writing-mode property: horizontal-tb | vertical-rl | vertical-lr
// +spec:writing-modes:aeb9bb - writing-mode property determines block flow direction
/// Represents a `writing-mode` attribute
// +spec:writing-modes:a7f174 - line orientation: in vertical-lr the line-over (ascender) side is block-end, not block-start
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
// +spec:block-formatting-context:387117 - writing-mode specifies horizontal/vertical line layout and block progression direction
// +spec:block-formatting-context:3815e7 - vertical-rl writing mode supported via VerticalRl variant
// +spec:block-formatting-context:9d7cd4 - vertical writing mode support (VerticalRl, VerticalLr)
#[derive(Default)]
pub enum LayoutWritingMode {
    /// Top-to-bottom block flow, left-to-right inline direction (Latin, etc.).
    #[default]
    HorizontalTb,
    /// Right-to-left block flow, top-to-bottom inline direction (CJK vertical).
    VerticalRl,
    // +spec:writing-modes:f35728 - vertical-lr writing mode for left-to-right block flow (Manchu, Mongolian)
    /// Left-to-right block flow, top-to-bottom inline direction (Mongolian).
    VerticalLr,
}


impl LayoutWritingMode {
    /// Returns true if the writing mode is vertical (`VerticalRl` or `VerticalLr`)
    #[must_use] pub const fn is_vertical(self) -> bool {
        matches!(self, Self::VerticalRl | Self::VerticalLr)
    }
}

impl core::fmt::Debug for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutWritingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutWritingMode {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::HorizontalTb => "horizontal-tb".to_string(),
            Self::VerticalRl => "vertical-rl".to_string(),
            Self::VerticalLr => "vertical-lr".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutWritingModeParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutWritingModeParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { LayoutWritingModeParseError<'a>, {
    InvalidValue(e) => format!("Invalid writing-mode value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutWritingModeParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutWritingModeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutWritingModeParseErrorOwned {
        match self {
            LayoutWritingModeParseError::InvalidValue(s) => {
                LayoutWritingModeParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutWritingModeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutWritingModeParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutWritingModeParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `writing-mode` value.
pub fn parse_layout_writing_mode(
    input: &str,
) -> Result<LayoutWritingMode, LayoutWritingModeParseError<'_>> {
    let input = input.trim();
    match input {
        "horizontal-tb" => Ok(LayoutWritingMode::HorizontalTb),
        "vertical-rl" => Ok(LayoutWritingMode::VerticalRl),
        // +spec:writing-modes:23147f - SVG1.1 tb-lr maps to vertical-lr
        "vertical-lr" | "tb-lr" => Ok(LayoutWritingMode::VerticalLr),
        _ => Err(LayoutWritingModeParseError::InvalidValue(input)),
    }
}

// --- clear (LayoutClear) ---

/// Represents a `clear` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutClear {
    /// No clearing; element is not moved below preceding floats.
    #[default]
    None,
    /// Element is moved below preceding left floats.
    Left,
    /// Element is moved below preceding right floats.
    Right,
    /// Element is moved below all preceding floats.
    Both,
}


impl core::fmt::Debug for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl core::fmt::Display for LayoutClear {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl PrintAsCssValue for LayoutClear {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
            Self::Both => "both".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutClearParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutClearParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { LayoutClearParseError<'a>, {
    InvalidValue(e) => format!("Invalid clear value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutClearParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutClearParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutClearParseErrorOwned {
        match self {
            LayoutClearParseError::InvalidValue(s) => {
                LayoutClearParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutClearParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutClearParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutClearParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `clear` value.
pub fn parse_layout_clear(input: &str) -> Result<LayoutClear, LayoutClearParseError<'_>> {
    let input = input.trim();
    match input {
        "none" => Ok(LayoutClear::None),
        "left" => Ok(LayoutClear::Left),
        "right" => Ok(LayoutClear::Right),
        "both" => Ok(LayoutClear::Both),
        _ => Err(LayoutClearParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    // LayoutWritingMode tests
    #[test]
    fn test_parse_writing_mode_horizontal_tb() {
        assert_eq!(
            parse_layout_writing_mode("horizontal-tb").unwrap(),
            LayoutWritingMode::HorizontalTb
        );
    }

    #[test]
    fn test_parse_writing_mode_vertical_rl() {
        assert_eq!(
            parse_layout_writing_mode("vertical-rl").unwrap(),
            LayoutWritingMode::VerticalRl
        );
    }

    #[test]
    fn test_parse_writing_mode_vertical_lr() {
        assert_eq!(
            parse_layout_writing_mode("vertical-lr").unwrap(),
            LayoutWritingMode::VerticalLr
        );
    }

    #[test]
    fn test_parse_writing_mode_invalid() {
        assert!(parse_layout_writing_mode("invalid").is_err());
        assert!(parse_layout_writing_mode("horizontal").is_err());
    }

    #[test]
    fn test_parse_writing_mode_whitespace() {
        assert_eq!(
            parse_layout_writing_mode("  vertical-rl  ").unwrap(),
            LayoutWritingMode::VerticalRl
        );
    }

    // LayoutClear tests
    #[test]
    fn test_parse_layout_clear_none() {
        assert_eq!(parse_layout_clear("none").unwrap(), LayoutClear::None);
    }

    #[test]
    fn test_parse_layout_clear_left() {
        assert_eq!(parse_layout_clear("left").unwrap(), LayoutClear::Left);
    }

    #[test]
    fn test_parse_layout_clear_right() {
        assert_eq!(parse_layout_clear("right").unwrap(), LayoutClear::Right);
    }

    #[test]
    fn test_parse_layout_clear_both() {
        assert_eq!(parse_layout_clear("both").unwrap(), LayoutClear::Both);
    }

    #[test]
    fn test_parse_layout_clear_invalid() {
        assert!(parse_layout_clear("invalid").is_err());
        assert!(parse_layout_clear("all").is_err());
    }

    #[test]
    fn test_parse_layout_clear_whitespace() {
        assert_eq!(parse_layout_clear("  both  ").unwrap(), LayoutClear::Both);
    }

    // Print tests
    #[test]
    fn test_print_writing_mode() {
        assert_eq!(
            LayoutWritingMode::HorizontalTb.print_as_css_value(),
            "horizontal-tb"
        );
        assert_eq!(
            LayoutWritingMode::VerticalRl.print_as_css_value(),
            "vertical-rl"
        );
        assert_eq!(
            LayoutWritingMode::VerticalLr.print_as_css_value(),
            "vertical-lr"
        );
    }

    #[test]
    fn test_print_layout_clear() {
        assert_eq!(LayoutClear::None.print_as_css_value(), "none");
        assert_eq!(LayoutClear::Left.print_as_css_value(), "left");
        assert_eq!(LayoutClear::Right.print_as_css_value(), "right");
        assert_eq!(LayoutClear::Both.print_as_css_value(), "both");
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    const ALL_WRITING_MODES: [LayoutWritingMode; 3] = [
        LayoutWritingMode::HorizontalTb,
        LayoutWritingMode::VerticalRl,
        LayoutWritingMode::VerticalLr,
    ];

    const ALL_CLEARS: [LayoutClear; 4] = [
        LayoutClear::None,
        LayoutClear::Left,
        LayoutClear::Right,
        LayoutClear::Both,
    ];

    // --- LayoutWritingMode::is_vertical (predicate) ---

    #[test]
    fn is_vertical_known_true_and_false() {
        assert!(!LayoutWritingMode::HorizontalTb.is_vertical());
        assert!(LayoutWritingMode::VerticalRl.is_vertical());
        assert!(LayoutWritingMode::VerticalLr.is_vertical());
    }

    #[test]
    fn is_vertical_default_is_horizontal() {
        assert!(!LayoutWritingMode::default().is_vertical());
        assert_eq!(LayoutWritingMode::default(), LayoutWritingMode::HorizontalTb);
    }

    #[test]
    fn is_vertical_is_const_evaluable() {
        const HORIZONTAL: bool = LayoutWritingMode::HorizontalTb.is_vertical();
        const VERTICAL: bool = LayoutWritingMode::VerticalRl.is_vertical();
        const _: () = assert!(!HORIZONTAL && VERTICAL);
    }

    #[test]
    fn is_vertical_agrees_with_css_keyword_prefix() {
        // Invariant: is_vertical() must agree with the serialized keyword, so the
        // predicate can never drift away from the CSS value it claims to describe.
        for mode in ALL_WRITING_MODES {
            let css = mode.print_as_css_value();
            assert_eq!(
                mode.is_vertical(),
                css.starts_with("vertical-"),
                "is_vertical() disagrees with keyword {css}"
            );
        }
    }

    #[test]
    fn is_vertical_is_pure_and_does_not_consume() {
        // `self`-by-value on a Copy enum: repeated calls must be stable.
        let mode = LayoutWritingMode::VerticalLr;
        assert!(mode.is_vertical());
        assert!(mode.is_vertical());
        assert_eq!(mode, LayoutWritingMode::VerticalLr);
    }

    // --- Debug / Display / PrintAsCssValue (serializers) ---

    #[test]
    fn writing_mode_debug_display_and_css_value_all_agree() {
        for mode in ALL_WRITING_MODES {
            let css = mode.print_as_css_value();
            assert!(!css.is_empty());
            assert_eq!(format!("{mode:?}"), css);
            assert_eq!(format!("{mode}"), css);
        }
    }

    #[test]
    fn clear_debug_display_and_css_value_all_agree() {
        for clear in ALL_CLEARS {
            let css = clear.print_as_css_value();
            assert!(!css.is_empty());
            assert_eq!(format!("{clear:?}"), css);
            assert_eq!(format!("{clear}"), css);
        }
    }

    #[test]
    fn serializing_defaults_does_not_panic() {
        assert_eq!(LayoutWritingMode::default().print_as_css_value(), "horizontal-tb");
        assert_eq!(LayoutClear::default().print_as_css_value(), "none");
    }

    #[test]
    fn serialized_keywords_are_distinct() {
        // Two variants sharing a keyword would silently collapse on re-parse.
        let modes: alloc::collections::BTreeSet<String> = ALL_WRITING_MODES
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect();
        assert_eq!(modes.len(), ALL_WRITING_MODES.len());

        let clears: alloc::collections::BTreeSet<String> = ALL_CLEARS
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect();
        assert_eq!(clears.len(), ALL_CLEARS.len());
    }

    #[test]
    fn display_impl_ignores_width_and_precision_flags() {
        // The Display impl forwards through `write!(f, "{}", String)`, which writes
        // straight to the underlying buffer and drops the outer formatter's flags.
        // Padding/truncation therefore does NOT apply — pinning the real behaviour so
        // callers never build a stylesheet assuming `{:>20}` aligns.
        assert_eq!(format!("{:>20}", LayoutClear::Both), "both");
        assert_eq!(format!("{:.2}", LayoutClear::Both), "both");
        assert_eq!(format!("{:>20}", LayoutWritingMode::VerticalRl), "vertical-rl");
    }

    // --- ordering / hashing invariants on the plain enums ---

    #[test]
    fn writing_mode_ord_follows_declaration_order() {
        assert!(LayoutWritingMode::HorizontalTb < LayoutWritingMode::VerticalRl);
        assert!(LayoutWritingMode::VerticalRl < LayoutWritingMode::VerticalLr);
    }

    #[test]
    fn clear_ord_follows_declaration_order() {
        assert!(LayoutClear::None < LayoutClear::Left);
        assert!(LayoutClear::Left < LayoutClear::Right);
        assert!(LayoutClear::Right < LayoutClear::Both);
    }

    #[test]
    fn equal_values_hash_equally() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        fn hash_of<T: Hash>(value: &T) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        // Hash must agree with Eq: independently-constructed equal values hash alike.
        assert_eq!(
            hash_of(&LayoutWritingMode::VerticalRl),
            hash_of(&ALL_WRITING_MODES[1])
        );
        assert_eq!(hash_of(&LayoutClear::Both), hash_of(&ALL_CLEARS[3]));
        assert_ne!(
            hash_of(&LayoutWritingMode::HorizontalTb),
            hash_of(&LayoutWritingMode::VerticalRl)
        );
    }

    // --- parsers ---

    #[cfg(feature = "parser")]
    #[test]
    fn parse_writing_mode_valid_minimal() {
        assert_eq!(
            parse_layout_writing_mode("horizontal-tb"),
            Ok(LayoutWritingMode::HorizontalTb)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_writing_mode_accepts_svg_tb_lr_alias() {
        // SVG 1.1 `tb-lr` is an accepted alias for `vertical-lr`.
        assert_eq!(
            parse_layout_writing_mode("tb-lr"),
            Ok(LayoutWritingMode::VerticalLr)
        );
        // ...but the other SVG 1.1 writing-mode aliases are NOT mapped.
        for unsupported in ["lr", "lr-tb", "rl", "rl-tb", "tb"] {
            assert!(
                parse_layout_writing_mode(unsupported).is_err(),
                "{unsupported} unexpectedly parsed"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_clear_valid_minimal() {
        assert_eq!(parse_layout_clear("none"), Ok(LayoutClear::None));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_empty_input_errors_with_empty_payload() {
        let err = parse_layout_writing_mode("").unwrap_err();
        assert_eq!(err, LayoutWritingModeParseError::InvalidValue(""));
        let err = parse_layout_clear("").unwrap_err();
        assert_eq!(err, LayoutClearParseError::InvalidValue(""));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_whitespace_only_input_errors_after_trimming() {
        for blank in ["   ", "\t\n", "\r\n\r\n", "\u{0c}", " \t \n \r "] {
            assert_eq!(
                parse_layout_writing_mode(blank).unwrap_err(),
                LayoutWritingModeParseError::InvalidValue(""),
                "whitespace {blank:?} should trim to the empty payload"
            );
            assert_eq!(
                parse_layout_clear(blank).unwrap_err(),
                LayoutClearParseError::InvalidValue("")
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_garbage_returns_err_and_echoes_trimmed_input() {
        for garbage in [
            "invalid",
            "!!!",
            "\u{0}",
            "none\u{0}",
            "{}[]();",
            "-",
            "--",
            "vertical-",
            "-rl",
            "vertical rl",
            "vertical - rl",
            "vertical_rl",
            "verticalrl",
        ] {
            let err = parse_layout_writing_mode(garbage).unwrap_err();
            assert_eq!(err, LayoutWritingModeParseError::InvalidValue(garbage.trim()));
            let err = parse_layout_clear(garbage).unwrap_err();
            assert_eq!(err, LayoutClearParseError::InvalidValue(garbage.trim()));
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_rejects_css_wide_keywords() {
        // These are handled (if at all) by the caller in property.rs, not here —
        // the value parser itself must not silently accept them as keywords.
        for wide in ["inherit", "initial", "unset", "revert", "revert-layer"] {
            assert!(parse_layout_writing_mode(wide).is_err(), "{wide} accepted");
            assert!(parse_layout_clear(wide).is_err(), "{wide} accepted");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_is_ascii_case_sensitive() {
        // NOTE: CSS keywords are ASCII case-insensitive per css-values-4, but these
        // parsers match case-sensitively. Locking in the CURRENT behaviour; see the
        // report for the spec deviation.
        for upper in ["HORIZONTAL-TB", "Vertical-Rl", "VERTICAL-LR", "TB-LR"] {
            assert!(
                parse_layout_writing_mode(upper).is_err(),
                "{upper} parsed — case-insensitive matching was added, update this test"
            );
        }
        for upper in ["NONE", "Left", "RIGHT", "Both"] {
            assert!(
                parse_layout_clear(upper).is_err(),
                "{upper} parsed — case-insensitive matching was added, update this test"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_leading_trailing_ascii_whitespace_is_trimmed() {
        assert_eq!(
            parse_layout_writing_mode("  vertical-rl  "),
            Ok(LayoutWritingMode::VerticalRl)
        );
        assert_eq!(
            parse_layout_writing_mode("\t\nvertical-lr\r\n"),
            Ok(LayoutWritingMode::VerticalLr)
        );
        assert_eq!(parse_layout_clear("\t both \n"), Ok(LayoutClear::Both));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_rejects_trailing_junk_after_valid_keyword() {
        for junk in ["both;garbage", "both;", "both both", "both!important", "both,"] {
            assert_eq!(
                parse_layout_clear(junk).unwrap_err(),
                LayoutClearParseError::InvalidValue(junk.trim())
            );
        }
        assert!(parse_layout_writing_mode("vertical-rl;garbage").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_trims_unicode_whitespace_beyond_css_whitespace() {
        // `str::trim` uses the Unicode White_Space property, which is a SUPERSET of
        // CSS whitespace (space, tab, LF, CR, FF). So NBSP-padded keywords parse
        // here even though a spec-conformant CSS tokenizer would reject them.
        // Asserting the CURRENT behaviour; see the report.
        assert_eq!(
            parse_layout_clear("\u{00a0}both\u{00a0}"),
            Ok(LayoutClear::Both)
        );
        assert_eq!(
            parse_layout_writing_mode("\u{2003}vertical-rl\u{2003}"),
            Ok(LayoutWritingMode::VerticalRl)
        );
        // Zero-width space and BOM are NOT White_Space, so they stay in the payload.
        assert_eq!(
            parse_layout_clear("\u{200b}both").unwrap_err(),
            LayoutClearParseError::InvalidValue("\u{200b}both")
        );
        assert_eq!(
            parse_layout_clear("\u{feff}both").unwrap_err(),
            LayoutClearParseError::InvalidValue("\u{feff}both")
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_unicode_input_does_not_panic_on_char_boundaries() {
        for weird in [
            "\u{1F600}",
            "🙂🙃",
            "e\u{0301}",              // combining acute
            "n\u{0303}one",           // combining tilde inside a keyword
            "ｎｏｎｅ",               // fullwidth
            "نص",                     // RTL
            "\u{202E}both",           // RTL override
            "both\u{0301}",
        ] {
            let err = parse_layout_writing_mode(weird).unwrap_err();
            assert_eq!(err, LayoutWritingModeParseError::InvalidValue(weird.trim()));
            let err = parse_layout_clear(weird).unwrap_err();
            assert_eq!(err, LayoutClearParseError::InvalidValue(weird.trim()));
            // The borrowed payload must still be valid UTF-8 we can round-trip.
            assert_eq!(err.to_contained().to_shared(), err);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_boundary_numeric_strings_are_rejected() {
        for numeric in [
            "0",
            "-0",
            "0.0",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "infinity",
            "1e999",
            "-1e-999",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551616", // u64::MAX + 1
            "179769313486231570000000000000000000000000000000000000000000000000000000000000000",
        ] {
            assert!(parse_layout_writing_mode(numeric).is_err(), "{numeric} parsed");
            assert!(parse_layout_clear(numeric).is_err(), "{numeric} parsed");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_extremely_long_input_does_not_panic_or_hang() {
        let long = "a".repeat(1_000_000);
        let err = parse_layout_clear(&long).unwrap_err();
        assert_eq!(err, LayoutClearParseError::InvalidValue(long.as_str()));

        // A megabyte of whitespace must trim down to the empty payload, not hang.
        let blank = " ".repeat(1_000_000);
        assert_eq!(
            parse_layout_writing_mode(&blank).unwrap_err(),
            LayoutWritingModeParseError::InvalidValue("")
        );

        // A valid keyword buried in a megabyte of padding still parses.
        let padded = format!("{blank}vertical-rl{blank}");
        assert_eq!(
            parse_layout_writing_mode(&padded),
            Ok(LayoutWritingMode::VerticalRl)
        );

        // A near-miss keyword repeated: still exactly one Err, no quadratic blowup.
        let repeated = "both ".repeat(200_000);
        assert!(parse_layout_clear(&repeated).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_deeply_nested_input_does_not_stack_overflow() {
        let nested = "(".repeat(10_000) + &")".repeat(10_000);
        assert!(parse_layout_clear(&nested).is_err());
        assert!(parse_layout_writing_mode(&nested).is_err());

        let brackets = "[".repeat(10_000);
        assert_eq!(
            parse_layout_clear(&brackets).unwrap_err(),
            LayoutClearParseError::InvalidValue(brackets.as_str())
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_error_payload_borrows_the_trimmed_slice_not_the_whole_input() {
        let input = "   bogus-value   ";
        let LayoutClearParseError::InvalidValue(payload) = parse_layout_clear(input).unwrap_err();
        // The error echoes the TRIMMED slice, not the raw input.
        assert_eq!(payload, "bogus-value");
        assert_eq!(payload.len(), 11);
        // ...and it is a borrowed subslice of the original buffer, not a copy.
        assert!(input.as_ptr() <= payload.as_ptr());
    }

    // --- round-trip: serialize -> parse -> same value ---

    #[test]
    #[cfg(feature = "parser")]
    fn writing_mode_round_trips_through_css_value() {
        for mode in ALL_WRITING_MODES {
            let css = mode.print_as_css_value();
            assert_eq!(parse_layout_writing_mode(&css), Ok(mode), "round-trip {css}");
            // Display and Debug are the same wire format, so they round-trip too.
            assert_eq!(parse_layout_writing_mode(&format!("{mode}")), Ok(mode));
            assert_eq!(parse_layout_writing_mode(&format!("{mode:?}")), Ok(mode));
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn clear_round_trips_through_css_value() {
        for clear in ALL_CLEARS {
            let css = clear.print_as_css_value();
            assert_eq!(parse_layout_clear(&css), Ok(clear), "round-trip {css}");
            assert_eq!(parse_layout_clear(&format!("{clear}")), Ok(clear));
            assert_eq!(parse_layout_clear(&format!("{clear:?}")), Ok(clear));
        }
    }

    // --- error getters: to_contained / to_shared ---

    #[cfg(feature = "parser")]
    #[test]
    fn writing_mode_error_to_contained_preserves_the_value() {
        let err = parse_layout_writing_mode("bogus").unwrap_err();
        assert_eq!(
            err.to_contained(),
            LayoutWritingModeParseErrorOwned::InvalidValue("bogus".into())
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn clear_error_to_contained_preserves_the_value() {
        let err = parse_layout_clear("bogus").unwrap_err();
        assert_eq!(
            err.to_contained(),
            LayoutClearParseErrorOwned::InvalidValue("bogus".into())
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_to_contained_outlives_the_parsed_input() {
        // The whole point of to_contained(): escape the input's lifetime.
        let owned = {
            let scratch = String::from("temporary-garbage");
            parse_layout_clear(&scratch).unwrap_err().to_contained()
        };
        assert_eq!(
            owned,
            LayoutClearParseErrorOwned::InvalidValue("temporary-garbage".into())
        );
        assert_eq!(
            owned.to_shared(),
            LayoutClearParseError::InvalidValue("temporary-garbage")
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_shared_owned_round_trip_is_lossless() {
        for value in [
            "",
            " ",
            "\u{0}",
            "🙂",
            "e\u{0301}",
            "quote\"inside",
            "back\\slash",
            "new\nline",
        ] {
            let shared = LayoutClearParseError::InvalidValue(value);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "clear round-trip {value:?}");
            // to_contained -> to_shared -> to_contained is idempotent.
            assert_eq!(owned.to_shared().to_contained(), owned);

            let shared = LayoutWritingModeParseError::InvalidValue(value);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "writing-mode round-trip {value:?}");
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_round_trip_survives_a_huge_payload() {
        let huge = "x".repeat(100_000);
        let shared = LayoutClearParseError::InvalidValue(huge.as_str());
        let owned = shared.to_contained();
        let LayoutClearParseError::InvalidValue(back) = owned.to_shared();
        assert_eq!(back.len(), 100_000);
        assert_eq!(back, huge.as_str());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_to_shared_on_default_azstring_does_not_panic() {
        // AzString::default() is the empty &'static str — the degenerate case for
        // the unchecked from_utf8 inside AzString::as_str().
        let owned = LayoutClearParseErrorOwned::InvalidValue(AzString::default());
        assert_eq!(owned.to_shared(), LayoutClearParseError::InvalidValue(""));

        let owned = LayoutWritingModeParseErrorOwned::InvalidValue(AzString::default());
        assert_eq!(
            owned.to_shared(),
            LayoutWritingModeParseError::InvalidValue("")
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_display_and_debug_include_the_offending_value() {
        let err = parse_layout_writing_mode("  bogus  ").unwrap_err();
        let display = format!("{err}");
        assert_eq!(display, "Invalid writing-mode value: \"bogus\"");
        // impl_debug_as_display!: Debug must be identical to Display.
        assert_eq!(format!("{err:?}"), display);

        let err = parse_layout_clear("bogus").unwrap_err();
        let display = format!("{err}");
        assert_eq!(display, "Invalid clear value: \"bogus\"");
        assert_eq!(format!("{err:?}"), display);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_display_does_not_panic_on_empty_or_unicode_payloads() {
        for value in ["", "\u{0}", "🙂", "\u{202E}"] {
            let err = LayoutClearParseError::InvalidValue(value);
            assert!(format!("{err}").contains(value));
            let owned = err.to_contained();
            assert!(format!("{:?}", owned.to_shared()).contains(value));
        }
    }
}
