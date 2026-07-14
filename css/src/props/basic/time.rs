//! CSS property types for time durations (s, ms).

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

/// A CSS time duration, stored internally in milliseconds.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub struct CssDuration {
    /// Duration in milliseconds.
    pub inner: u32,
}


impl PrintAsCssValue for CssDuration {
    fn print_as_css_value(&self) -> String {
        format!("{}ms", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for CssDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("CssDuration {{ inner: {} }}", self.inner)
    }
}

/// Error returned when parsing a CSS duration string fails.
#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum DurationParseError<'a> {
    InvalidValue(&'a str),
    ParseFloat(core::num::ParseFloatError),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(DurationParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { DurationParseError<'a>, {
    InvalidValue(v) => format!("Invalid time value: \"{}\"", v),
    ParseFloat(e) => format!("Invalid number for time value: {}", e),
}}

/// Owned version of [`DurationParseError`] for FFI and storage.
#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum DurationParseErrorOwned {
    InvalidValue(AzString),
    ParseFloat(AzString),
}

#[cfg(feature = "parser")]
impl DurationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> DurationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => DurationParseErrorOwned::InvalidValue((*s).to_string().into()),
            Self::ParseFloat(e) => DurationParseErrorOwned::ParseFloat(e.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl DurationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> DurationParseError<'_> {
        match self {
            Self::InvalidValue(s) => DurationParseError::InvalidValue(s),
            Self::ParseFloat(s) => DurationParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parses a CSS duration string (e.g. `"200ms"`, `"1.5s"`) into a [`CssDuration`].
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `duration` value.
pub fn parse_duration(input: &str) -> Result<CssDuration, DurationParseError<'_>> {
    let trimmed = input.trim().to_lowercase();
    if trimmed == "0" {
        return Ok(CssDuration { inner: 0 });
    }
    if let Some(num_str) = trimmed.strip_suffix("ms") {
        let ms = num_str
            .parse::<f32>()
            .map_err(DurationParseError::ParseFloat)?;
        if ms < 0.0 {
            return Err(DurationParseError::InvalidValue(input));
        }
        Ok(CssDuration { inner: crate::cast::f32_to_u32(ms) })
    } else if let Some(num_str) = trimmed.strip_suffix('s') {
        let s = num_str
            .parse::<f32>()
            .map_err(DurationParseError::ParseFloat)?;
        if s < 0.0 {
            return Err(DurationParseError::InvalidValue(input));
        }
        Ok(CssDuration {
            inner: crate::cast::f32_to_u32(s * 1000.0),
        })
    } else {
        Err(DurationParseError::InvalidValue(input))
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;
    use crate::codegen::format::FormatAsRustCode;
    use crate::props::formatter::PrintAsCssValue;

    /// Largest integer an `f32` represents exactly (`2^24`). Above this, the
    /// spacing between neighbouring `f32`s exceeds 1ms, so `parse_duration`
    /// (which round-trips through `f32`) can no longer be lossless.
    #[cfg(feature = "parser")]
    const TWO_POW_24: u32 = 16_777_216;

    /// Convenience: parse and unwrap to the raw millisecond count.
    #[cfg(feature = "parser")]
    fn ms(input: &str) -> u32 {
        parse_duration(input)
            .unwrap_or_else(|e| panic!("expected {input:?} to parse, got {e}"))
            .inner
    }

    // ------------------------------------------------------ positive control ---

    #[cfg(feature = "parser")]
    #[test]
    fn valid_minimal_inputs_parse_to_expected_values() {
        assert_eq!(ms("0"), 0);
        assert_eq!(ms("0ms"), 0);
        assert_eq!(ms("0s"), 0);
        assert_eq!(ms("200ms"), 200);
        assert_eq!(ms("1s"), 1000);
        assert_eq!(ms("1.5s"), 1500);
        assert_eq!(ms("0.5s"), 500);
        assert_eq!(ms(".25s"), 250);
        assert_eq!(ms("5e2ms"), 500);
        assert_eq!(ms("+5ms"), 5);
    }

    /// The `ms` suffix must be stripped before the bare `s` suffix, otherwise
    /// `"5ms"` would be read as 5 *seconds* (a 1000x error).
    #[cfg(feature = "parser")]
    #[test]
    fn ms_suffix_wins_over_s_suffix() {
        assert_eq!(ms("5ms"), 5);
        assert_ne!(ms("5ms"), ms("5s"));
        assert_eq!(ms("5s"), 5000);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn units_are_case_insensitive() {
        assert_eq!(ms("200MS"), 200);
        assert_eq!(ms("200Ms"), 200);
        assert_eq!(ms("1S"), 1000);
        assert_eq!(ms("1.5E1S"), 15000);
    }

    // ----------------------------------------------------------- truncation ---

    /// Fractional milliseconds are truncated toward zero, never rounded.
    #[cfg(feature = "parser")]
    #[test]
    fn sub_millisecond_values_truncate_toward_zero() {
        assert_eq!(ms("5.9ms"), 5);
        assert_eq!(ms("0.9ms"), 0);
        assert_eq!(ms("0.0009s"), 0); // 0.9ms
        assert_eq!(ms("0.0015s"), 1); // 1.5ms
    }

    // ------------------------------------------------------- empty / blank ---

    #[cfg(feature = "parser")]
    #[test]
    fn empty_input_is_rejected_without_panicking() {
        assert_eq!(parse_duration(""), Err(DurationParseError::InvalidValue("")));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn whitespace_only_input_is_rejected_and_error_keeps_the_raw_input() {
        // The input is trimmed for parsing but the *error* carries the original
        // (untrimmed) slice, so callers can point at the offending source text.
        assert_eq!(
            parse_duration("   "),
            Err(DurationParseError::InvalidValue("   "))
        );
        assert_eq!(
            parse_duration("\t\n"),
            Err(DurationParseError::InvalidValue("\t\n"))
        );
    }

    // ---------------------------------------------------------- malformed ---

    #[cfg(feature = "parser")]
    #[test]
    fn a_bare_unit_with_no_number_is_a_parse_float_error_not_a_panic() {
        assert!(matches!(
            parse_duration("ms"),
            Err(DurationParseError::ParseFloat(_))
        ));
        assert!(matches!(
            parse_duration("s"),
            Err(DurationParseError::ParseFloat(_))
        ));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn unitless_numbers_other_than_literal_zero_are_rejected() {
        // Only the exact string "0" is accepted without a unit.
        assert_eq!(ms("0"), 0);
        assert_eq!(
            parse_duration("200"),
            Err(DurationParseError::InvalidValue("200"))
        );
        assert_eq!(
            parse_duration("1.5"),
            Err(DurationParseError::InvalidValue("1.5"))
        );
        assert_eq!(
            parse_duration("0.0"),
            Err(DurationParseError::InvalidValue("0.0"))
        );
        assert_eq!(
            parse_duration("00"),
            Err(DurationParseError::InvalidValue("00"))
        );
        assert_eq!(
            parse_duration("-0"),
            Err(DurationParseError::InvalidValue("-0"))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn garbage_and_junk_never_panic() {
        for garbage in [
            "abc", "!!!", "\0\0\0", "ms ms", "1,5s", "1 ms", "--5ms", "5mss", "5sms", "0x10ms",
            "1e", "1e+", ".s", "-.ms", "s1", "ms200", "200ms;garbage", "200ms !important",
        ] {
            // The only contract is: never panic, and never silently succeed with
            // a value we did not ask for. Every one of these is an error.
            assert!(
                parse_duration(garbage).is_err(),
                "expected {garbage:?} to be rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn leading_and_trailing_whitespace_is_trimmed_but_interior_space_is_not() {
        assert_eq!(ms("   200ms   "), 200);
        assert_eq!(ms("\t\n1.5s\r\n"), 1500);
        // Interior whitespace stays inside the number and kills the float parse.
        assert!(matches!(
            parse_duration("200 ms"),
            Err(DurationParseError::ParseFloat(_))
        ));
        assert!(matches!(
            parse_duration("2 0 0ms"),
            Err(DurationParseError::ParseFloat(_))
        ));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn trailing_junk_after_a_valid_value_is_rejected_not_silently_accepted() {
        assert!(parse_duration("200ms;").is_err());
        assert!(parse_duration("200msx").is_err());
        // ...but note "200msms" strips one "ms" and then fails the float parse.
        assert!(matches!(
            parse_duration("200msms"),
            Err(DurationParseError::ParseFloat(_))
        ));
    }

    // ------------------------------------------------------------ negative ---

    #[cfg(feature = "parser")]
    #[test]
    fn negative_durations_are_rejected_in_both_units() {
        assert_eq!(
            parse_duration("-1ms"),
            Err(DurationParseError::InvalidValue("-1ms"))
        );
        assert_eq!(
            parse_duration("-0.5s"),
            Err(DurationParseError::InvalidValue("-0.5s"))
        );
        assert_eq!(
            parse_duration("-1e-30s"),
            Err(DurationParseError::InvalidValue("-1e-30s"))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn the_invalid_value_error_reports_the_original_untrimmed_uncased_input() {
        // Not the lowercased/trimmed copy used internally.
        assert_eq!(
            parse_duration("  -1MS  "),
            Err(DurationParseError::InvalidValue("  -1MS  "))
        );
    }

    /// `-0.0 < 0.0` is false, so signed zero slips past the negativity check —
    /// but the cast lands on `0`, so the result is still sane.
    #[cfg(feature = "parser")]
    #[test]
    fn negative_zero_is_accepted_and_clamps_to_zero() {
        assert_eq!(ms("-0ms"), 0);
        assert_eq!(ms("-0.0s"), 0);
        assert_eq!(ms("-0e10ms"), 0);
    }

    // ---------------------------------------------- overflow / saturation ---

    #[cfg(feature = "parser")]
    #[test]
    fn values_beyond_u32_max_saturate_instead_of_wrapping_or_panicking() {
        assert_eq!(ms("4294967296ms"), u32::MAX); // 2^32 exactly
        assert_eq!(ms("99999999999ms"), u32::MAX);
        assert_eq!(ms("1e30s"), u32::MAX);
        assert_eq!(ms("5000000s"), u32::MAX); // 5e6 * 1000 = 5e9 > u32::MAX
    }

    /// A float literal too large for `f32` parses to `+inf` (not an error), and
    /// `inf as u32` saturates. Assert the whole chain lands on `u32::MAX`.
    #[cfg(feature = "parser")]
    #[test]
    fn float_overflow_to_infinity_saturates_to_u32_max() {
        assert_eq!(ms("1e39ms"), u32::MAX); // > f32::MAX
        assert_eq!(ms("1e999999ms"), u32::MAX);
        assert_eq!(ms("infms"), u32::MAX);
        assert_eq!(ms("infinityms"), u32::MAX);
        assert_eq!(ms("INFms"), u32::MAX);
        assert_eq!(ms("infs"), u32::MAX);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn negative_infinity_is_rejected_as_a_negative_duration() {
        assert_eq!(
            parse_duration("-infms"),
            Err(DurationParseError::InvalidValue("-infms"))
        );
        assert_eq!(
            parse_duration("-infinitys"),
            Err(DurationParseError::InvalidValue("-infinitys"))
        );
    }

    /// `NaN < 0.0` is false, so `"nan"` is *accepted* rather than rejected; the
    /// saturating cast then turns it into `0ms`. Documented here so that any
    /// future change to reject NaN outright is a visible, intentional change.
    #[cfg(feature = "parser")]
    #[test]
    fn nan_is_accepted_and_degrades_to_zero_rather_than_panicking() {
        assert_eq!(ms("nanms"), 0);
        assert_eq!(ms("NaNms"), 0);
        assert_eq!(ms("-nanms"), 0);
        assert_eq!(ms("nans"), 0); // NaN * 1000.0 is still NaN
    }

    #[cfg(feature = "parser")]
    #[test]
    fn underflow_to_zero_is_not_an_error() {
        assert_eq!(ms("1e-30ms"), 0);
        assert_eq!(ms("1e-999999s"), 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn u32_max_and_f32_max_boundary_strings_are_handled() {
        assert_eq!(ms("4294967295ms"), u32::MAX); // u32::MAX, rounds up in f32 then saturates back
        assert_eq!(ms("4294967040ms"), 4294967040); // 2^32 - 256: exactly representable in f32

        let f32_max = format!("{}ms", f32::MAX);
        assert_eq!(ms(&f32_max), u32::MAX);

        let i64_max = format!("{}ms", i64::MAX);
        assert_eq!(ms(&i64_max), u32::MAX);
    }

    // ------------------------------------------------------------ huge input ---

    #[cfg(feature = "parser")]
    #[test]
    fn extremely_long_digit_string_saturates_without_hanging() {
        let mut input = "9".repeat(100_000);
        input.push_str("ms");
        assert_eq!(ms(&input), u32::MAX);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn extremely_long_run_of_leading_zeros_still_parses_exactly() {
        let mut input = "0".repeat(100_000);
        input.push_str("1ms");
        assert_eq!(ms(&input), 1);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn extremely_long_garbage_is_rejected_without_hanging() {
        let input = "x".repeat(100_000);
        assert!(parse_duration(&input).is_err());

        // Long, *trimmable* padding around a valid value.
        let padded = format!("{}200ms{}", " ".repeat(50_000), " ".repeat(50_000));
        assert_eq!(ms(&padded), 200);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn deeply_nested_brackets_do_not_stack_overflow() {
        // The parser is not recursive; prove it by feeding it 10k nesting levels.
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_duration(&nested).is_err());

        let nested_with_unit = format!("{nested}s");
        assert!(parse_duration(&nested_with_unit).is_err());
    }

    // -------------------------------------------------------------- unicode ---

    #[cfg(feature = "parser")]
    #[test]
    fn non_ascii_input_is_rejected_without_panicking() {
        for input in [
            "\u{1F600}",       // emoji
            "\u{1F600}ms",     // emoji + valid unit
            "1\u{FF53}",       // FULLWIDTH LATIN SMALL LETTER S is not "s"
            "1s\u{0301}",      // combining acute after the unit
            "２００ms",        // fullwidth digits
            "\u{202E}200ms",   // RTL override prefix
            "1\u{00A0}s",      // NBSP *inside* the value
        ] {
            assert!(
                parse_duration(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    /// `str::trim` strips Unicode whitespace, not just ASCII.
    #[cfg(feature = "parser")]
    #[test]
    fn unicode_whitespace_around_a_valid_value_is_trimmed() {
        assert_eq!(ms("\u{00A0}200ms\u{00A0}"), 200); // NBSP
        assert_eq!(ms("\u{3000}1.5s\u{3000}"), 1500); // ideographic space
    }

    /// `to_lowercase` can *grow* the string (`İ` -> `i` + combining dot), which
    /// would corrupt any byte-index-based suffix logic. Suffix stripping here is
    /// char-safe, so this must merely fail to parse.
    #[cfg(feature = "parser")]
    #[test]
    fn lowercasing_that_changes_the_byte_length_does_not_panic() {
        assert!(parse_duration("\u{0130}ms").is_err()); // LATIN CAPITAL I WITH DOT ABOVE
        assert!(parse_duration("1\u{0130}s").is_err());
    }

    // ----------------------------------------------------------- round-trip ---

    #[cfg(feature = "parser")]
    #[test]
    fn print_as_css_value_round_trips_through_parse_duration() {
        for inner in [
            0,
            1,
            2,
            17,
            999,
            1000,
            65_535,
            1_000_000,
            TWO_POW_24,      // last exactly-representable integer in f32
            4_294_967_040,   // 2^32 - 256: still exact (a multiple of the f32 ulp there)
            u32::MAX,        // rounds up to 2^32 in f32, then the cast saturates back down
        ] {
            let duration = CssDuration { inner };
            let printed = duration.print_as_css_value();
            assert_eq!(
                parse_duration(&printed),
                Ok(duration),
                "round-trip failed for {inner}ms (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn print_as_css_value_always_emits_the_ms_unit() {
        for inner in [0, 1, u32::MAX] {
            let printed = CssDuration { inner }.print_as_css_value();
            assert!(printed.ends_with("ms"), "{printed:?} lacks a unit");
            assert_eq!(printed, format!("{inner}ms"));
        }
    }

    /// Above `2^24` the millisecond count no longer survives an `f32`, so the
    /// round-trip is lossy. This is a real precision limit of the parser, pinned
    /// here so it cannot regress further (the error must stay within one ulp).
    #[cfg(feature = "parser")]
    #[test]
    fn round_trip_above_two_pow_24_is_lossy_but_bounded() {
        let duration = CssDuration {
            inner: TWO_POW_24 + 1,
        };
        let reparsed = parse_duration(&duration.print_as_css_value()).unwrap();
        assert_ne!(reparsed.inner, duration.inner);
        assert_eq!(reparsed.inner, TWO_POW_24);
        assert!(reparsed.inner.abs_diff(duration.inner) <= 1);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn seconds_and_milliseconds_agree_for_the_same_duration() {
        assert_eq!(ms("2s"), ms("2000ms"));
        assert_eq!(ms("0.001s"), ms("1ms"));
        assert_eq!(ms("0s"), ms("0ms"));
    }

    // ------------------------------------------------------- CssDuration ---

    #[test]
    fn default_duration_is_zero() {
        assert_eq!(CssDuration::default(), CssDuration { inner: 0 });
        assert_eq!(CssDuration::default().inner, 0);
    }

    #[test]
    fn ordering_and_equality_follow_the_inner_millisecond_count() {
        let a = CssDuration { inner: 1 };
        let b = CssDuration { inner: 2 };
        let max = CssDuration { inner: u32::MAX };
        assert!(a < b);
        assert!(b < max);
        assert_eq!(a, CssDuration { inner: 1 });
        assert_eq!(a.max(b), b);
        assert_eq!(CssDuration::default(), CssDuration { inner: 0 });
    }

    #[test]
    fn format_as_rust_code_emits_a_constructor_and_ignores_indentation() {
        let d = CssDuration { inner: 42 };
        assert_eq!(d.format_as_rust_code(0), "CssDuration { inner: 42 }");
        assert_eq!(d.format_as_rust_code(7), d.format_as_rust_code(0));
        assert_eq!(
            CssDuration { inner: u32::MAX }.format_as_rust_code(0),
            "CssDuration { inner: 4294967295 }"
        );
    }

    // --------------------------------------------------- error conversions ---

    #[cfg(feature = "parser")]
    fn parse_float_error() -> core::num::ParseFloatError {
        "not-a-float".parse::<f32>().unwrap_err()
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_contained_preserves_an_invalid_value_payload() {
        let owned = DurationParseError::InvalidValue("10px").to_contained();
        match owned {
            DurationParseErrorOwned::InvalidValue(s) => assert_eq!(s.as_str(), "10px"),
            DurationParseErrorOwned::ParseFloat(_) => panic!("variant changed"),
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_contained_stringifies_the_float_error() {
        let owned = DurationParseError::ParseFloat(parse_float_error()).to_contained();
        match owned {
            DurationParseErrorOwned::ParseFloat(s) => {
                assert!(!s.as_str().is_empty(), "float error message was empty");
                assert_eq!(s.as_str(), parse_float_error().to_string());
            }
            DurationParseErrorOwned::InvalidValue(_) => panic!("variant changed"),
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_contained_handles_empty_and_extreme_payloads() {
        assert_eq!(
            DurationParseError::InvalidValue("").to_contained(),
            DurationParseErrorOwned::InvalidValue(String::new().into())
        );

        let huge = "x".repeat(100_000);
        let owned = DurationParseError::InvalidValue(&huge).to_contained();
        match owned {
            DurationParseErrorOwned::InvalidValue(s) => assert_eq!(s.as_str().len(), 100_000),
            DurationParseErrorOwned::ParseFloat(_) => panic!("variant changed"),
        }

        // Non-UTF8-boundary-unsafe payloads must survive the copy intact.
        let unicode = "\u{1F600}\u{0301}";
        assert_eq!(
            DurationParseError::InvalidValue(unicode).to_contained(),
            DurationParseErrorOwned::InvalidValue(unicode.to_string().into())
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_shared_preserves_an_invalid_value_payload() {
        let owned = DurationParseErrorOwned::InvalidValue("garbage".to_string().into());
        assert_eq!(owned.to_shared(), DurationParseError::InvalidValue("garbage"));
    }

    /// `DurationParseErrorOwned::to_shared` maps `ParseFloat(msg)` onto
    /// `DurationParseError::InvalidValue(msg)` — the variant is *not* preserved,
    /// so the error message ("invalid float literal") ends up in the slot that
    /// normally holds the offending source text. Pinned as the current behaviour;
    /// see the report accompanying this test module.
    #[cfg(feature = "parser")]
    #[test]
    fn to_shared_downgrades_parse_float_to_invalid_value() {
        let msg = parse_float_error().to_string();
        let owned = DurationParseErrorOwned::ParseFloat(msg.clone().into());
        let shared = owned.to_shared();

        assert!(!matches!(shared, DurationParseError::ParseFloat(_)));
        assert_eq!(shared, DurationParseError::InvalidValue(msg.as_str()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_shared_does_not_panic_on_empty_or_extreme_payloads() {
        assert_eq!(
            DurationParseErrorOwned::InvalidValue(String::new().into()).to_shared(),
            DurationParseError::InvalidValue("")
        );

        let huge = "y".repeat(100_000);
        let owned = DurationParseErrorOwned::InvalidValue(huge.clone().into());
        assert_eq!(owned.to_shared(), DurationParseError::InvalidValue(&huge));

        let empty_float = DurationParseErrorOwned::ParseFloat(String::new().into());
        assert_eq!(empty_float.to_shared(), DurationParseError::InvalidValue(""));
    }

    /// A real error straight out of the parser must survive the owned round-trip
    /// (this is the FFI path: borrow -> own -> borrow).
    #[cfg(feature = "parser")]
    #[test]
    fn invalid_value_survives_a_full_shared_owned_shared_round_trip() {
        let input = "10px";
        let err = parse_duration(input).unwrap_err();
        assert_eq!(err, DurationParseError::InvalidValue(input));

        let owned = err.to_contained();
        assert_eq!(owned.to_shared(), DurationParseError::InvalidValue(input));
    }

    /// `"200 nanoseconds"` ends in `s`, so it goes down the *seconds* branch and
    /// fails in the float parse — not the "unknown unit" branch. Pinning this
    /// keeps the two error variants from being swapped by accident.
    #[cfg(feature = "parser")]
    #[test]
    fn a_word_ending_in_s_is_treated_as_a_seconds_value() {
        assert!(matches!(
            parse_duration("200 nanoseconds"),
            Err(DurationParseError::ParseFloat(_))
        ));
        assert!(matches!(
            parse_duration("always"),
            Err(DurationParseError::ParseFloat(_))
        ));
        // ...whereas a word *not* ending in s/ms is an unknown-unit error.
        assert_eq!(
            parse_duration("200 nanosecond"),
            Err(DurationParseError::InvalidValue("200 nanosecond"))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn error_display_never_panics_and_mentions_the_offender() {
        let invalid = DurationParseError::InvalidValue("\u{1F600}");
        let printed = format!("{invalid}");
        assert!(printed.contains('\u{1F600}'), "{printed:?}");

        let float = DurationParseError::ParseFloat(parse_float_error());
        assert!(!format!("{float}").is_empty());

        // Debug is wired to Display; both must work on both variants.
        assert!(!format!("{invalid:?}").is_empty());
        assert!(!format!("{float:?}").is_empty());
    }
}
