//! C-compatible string formatting via `strfmt`.
//!
//! Provides [`FmtValue`], [`FmtArg`], and [`FmtArgVec`] for passing
//! heterogeneous format arguments across FFI, and [`fmt_string`] as the
//! main entry point. Used by `fluent.rs` and `icu.rs` for localization.

use std::fmt;

use azul_css::{AzString, StringVec, impl_option, impl_option_inner};

/// A format argument value that can hold any primitive type or string.
/// Used in [`FmtArg`] to pass typed values into `strfmt`-based formatting.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum FmtValue {
    Bool(bool),
    Uchar(u8),
    Schar(i8),
    Ushort(u16),
    Sshort(i16),
    Uint(u32),
    Sint(i32),
    Ulong(u64),
    Slong(i64),
    Isize(isize),
    Usize(usize),
    Float(f32),
    Double(f64),
    Str(AzString),
    StrVec(StringVec),
}

impl strfmt::DisplayStr for FmtValue {
    fn display_str(&self, f: &mut strfmt::Formatter<'_, '_>) -> strfmt::Result<()> {
        use strfmt::DisplayStr;
        match self {
            Self::Bool(v) => format!("{v}").display_str(f),
            Self::Uchar(v) => v.display_str(f),
            Self::Schar(v) => v.display_str(f),
            Self::Ushort(v) => v.display_str(f),
            Self::Sshort(v) => v.display_str(f),
            Self::Uint(v) => v.display_str(f),
            Self::Sint(v) => v.display_str(f),
            Self::Ulong(v) => v.display_str(f),
            Self::Slong(v) => v.display_str(f),
            Self::Isize(v) => v.display_str(f),
            Self::Usize(v) => v.display_str(f),
            Self::Float(v) => v.display_str(f),
            Self::Double(v) => v.display_str(f),
            Self::Str(v) => v.as_str().display_str(f),
            Self::StrVec(sv) => {
                "[".display_str(f)?;
                for (i, s) in sv.as_ref().iter().enumerate() {
                    if i != 0 {
                        ", ".display_str(f)?;
                    }
                    s.as_str().display_str(f)?;
                }
                "]".display_str(f)?;
                Ok(())
            }
        }
    }
}

impl fmt::Display for FmtValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(v) => v.fmt(f),
            Self::Uchar(v) => v.fmt(f),
            Self::Schar(v) => v.fmt(f),
            Self::Ushort(v) => v.fmt(f),
            Self::Sshort(v) => v.fmt(f),
            Self::Uint(v) => v.fmt(f),
            Self::Sint(v) => v.fmt(f),
            Self::Ulong(v) => v.fmt(f),
            Self::Slong(v) => v.fmt(f),
            Self::Isize(v) => v.fmt(f),
            Self::Usize(v) => v.fmt(f),
            Self::Float(v) => v.fmt(f),
            Self::Double(v) => v.fmt(f),
            Self::Str(v) => v.as_str().fmt(f),
            Self::StrVec(sv) => {
                use std::fmt::Debug;
                let vec: Vec<&str> = sv.as_ref().iter().map(AzString::as_str).collect();
                vec.fmt(f)
            }
        }
    }
}

/// A key-value pair mapping a format placeholder name to its value.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FmtArg {
    pub key: AzString,
    pub value: FmtValue,
}

azul_css::impl_option!(FmtArg, OptionFmtArg, copy = false, [Debug, Clone, PartialEq, PartialOrd]);
azul_css::impl_vec!(FmtArg, FmtArgVec, FmtArgVecDestructor, FmtArgVecDestructorType, FmtArgVecSlice, OptionFmtArg);
azul_css::impl_vec_clone!(FmtArg, FmtArgVec, FmtArgVecDestructor);
azul_css::impl_vec_debug!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialeq!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialord!(FmtArg, FmtArgVec);

/// Formats `format` by substituting placeholders with values from `args`.
/// Returns the error message as a string on failure (for C FFI ergonomics).
// FFI-exported formatter: owned AzString/FmtArgVec args are the api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use] pub fn fmt_string(format: AzString, args: FmtArgVec) -> String {
    use strfmt::Format;
    let format_map = args
        .iter()
        .map(|a| (a.key.clone().into_library_owned_string(), a.value.clone()))
        .collect();
    match format.as_str().format(&format_map) {
        Ok(o) => o,
        Err(e) => format!("{e}"),
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    fn az(s: &str) -> AzString {
        AzString::from(s)
    }

    fn strvec(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| az(s)).collect())
    }

    /// Formats `format` with the given key/value pairs. Because `fmt_string`
    /// swallows every `strfmt` failure into the returned `String`, the return
    /// value here is EITHER the formatted output OR an error message of the
    /// shape `Invalid(..)` / `KeyError(..)` / `TypeError(..)`.
    fn run(format: &str, args: &[(&str, FmtValue)]) -> String {
        let v: Vec<FmtArg> = args
            .iter()
            .map(|(k, value)| FmtArg {
                key: az(k),
                value: value.clone(),
            })
            .collect();
        fmt_string(az(format), FmtArgVec::from_vec(v))
    }

    /// True if the output is one of the three `strfmt` error renderings rather
    /// than a formatted string.
    fn is_err_str(s: &str) -> bool {
        s.starts_with("Invalid(") || s.starts_with("KeyError(") || s.starts_with("TypeError(")
    }

    /// Every scalar (non-`StrVec`) variant at an interesting boundary.
    fn scalar_variants() -> Vec<FmtValue> {
        vec![
            FmtValue::Bool(true),
            FmtValue::Bool(false),
            FmtValue::Uchar(0),
            FmtValue::Uchar(u8::MAX),
            FmtValue::Schar(i8::MIN),
            FmtValue::Schar(i8::MAX),
            FmtValue::Ushort(0),
            FmtValue::Ushort(u16::MAX),
            FmtValue::Sshort(i16::MIN),
            FmtValue::Sshort(i16::MAX),
            FmtValue::Uint(0),
            FmtValue::Uint(u32::MAX),
            FmtValue::Sint(i32::MIN),
            FmtValue::Sint(i32::MAX),
            FmtValue::Ulong(0),
            FmtValue::Ulong(u64::MAX),
            FmtValue::Slong(i64::MIN),
            FmtValue::Slong(i64::MAX),
            FmtValue::Isize(isize::MIN),
            FmtValue::Isize(isize::MAX),
            FmtValue::Usize(0),
            FmtValue::Usize(usize::MAX),
            FmtValue::Float(0.0),
            FmtValue::Float(f32::MIN),
            FmtValue::Float(f32::MAX),
            FmtValue::Float(f32::NAN),
            FmtValue::Float(f32::INFINITY),
            FmtValue::Float(f32::NEG_INFINITY),
            FmtValue::Float(f32::EPSILON),
            FmtValue::Double(0.0),
            FmtValue::Double(f64::MIN),
            FmtValue::Double(f64::MAX),
            FmtValue::Double(f64::NAN),
            FmtValue::Double(f64::INFINITY),
            FmtValue::Double(f64::NEG_INFINITY),
            FmtValue::Double(f64::MIN_POSITIVE),
            FmtValue::Str(AzString::default()),
            FmtValue::Str(az("plain")),
        ]
    }

    // ------------------------------------------------------------------
    // fmt_string: happy path + substitution invariants
    // ------------------------------------------------------------------

    #[test]
    fn substitutes_a_single_placeholder() {
        assert_eq!(
            run("Hello {name}!", &[("name", FmtValue::Str(az("World")))]),
            "Hello World!"
        );
    }

    #[test]
    fn empty_format_and_empty_args_yield_empty_output() {
        assert_eq!(fmt_string(AzString::default(), FmtArgVec::new()), "");
        assert_eq!(fmt_string(az("plain text"), FmtArgVec::new()), "plain text");
    }

    #[test]
    fn args_without_a_matching_placeholder_are_ignored() {
        assert_eq!(
            run(
                "{a}",
                &[("a", FmtValue::Sint(1)), ("unused", FmtValue::Sint(2))]
            ),
            "1"
        );
    }

    #[test]
    fn the_same_key_can_be_substituted_repeatedly() {
        assert_eq!(run("{k}-{k}-{k}", &[("k", FmtValue::Uint(7))]), "7-7-7");
    }

    #[test]
    fn duplicate_keys_resolve_to_the_last_arg() {
        // `fmt_string` collects into a `HashMap`, so a later duplicate overwrites
        // the earlier one instead of erroring or panicking.
        assert_eq!(
            run("{k}", &[("k", FmtValue::Sint(1)), ("k", FmtValue::Sint(2))]),
            "2"
        );
    }

    #[test]
    fn substituted_values_are_not_re_scanned_for_placeholders() {
        // A value that itself looks like a format string must be emitted
        // literally — no recursive expansion, no second-order injection.
        assert_eq!(
            run(
                "{a}",
                &[
                    ("a", FmtValue::Str(az("{b}"))),
                    ("b", FmtValue::Str(az("PWNED"))),
                ]
            ),
            "{b}"
        );
    }

    // ------------------------------------------------------------------
    // fmt_string: malformed format strings must return an error, never panic
    // ------------------------------------------------------------------

    #[test]
    fn missing_key_returns_a_key_error_string() {
        let out = run("{missing}", &[]);
        assert!(out.contains("Invalid key: missing"), "{out}");
        assert!(is_err_str(&out), "{out}");
    }

    #[test]
    fn unclosed_brace_returns_an_error_string() {
        for bad in ["{", "{name", "abc {name"] {
            let out = run(bad, &[("name", FmtValue::Sint(1))]);
            assert!(out.contains("Expected '}'"), "{bad:?} -> {out}");
        }
    }

    #[test]
    fn lone_closing_brace_returns_an_error_string() {
        for bad in ["}", "a}b", "trailing}"] {
            let out = run(bad, &[]);
            assert!(out.contains("Single '}'"), "{bad:?} -> {out}");
        }
    }

    #[test]
    fn nested_opening_brace_returns_an_error_string() {
        let out = run("{a{b}", &[("a", FmtValue::Sint(1))]);
        assert!(out.contains("extra {"), "{out}");
    }

    #[test]
    fn empty_placeholder_is_rejected_even_when_an_empty_key_exists() {
        // `{}` has no identifier; the empty-string key in `args` is unreachable.
        let out = run("{}", &[("", FmtValue::Sint(1))]);
        assert!(out.contains("must specify identifier"), "{out}");
    }

    #[test]
    fn a_colon_in_a_key_makes_that_key_unaddressable() {
        // Everything after the first `:` is parsed as a format spec, so the key
        // "a:b" can never be looked up — this must be a KeyError, not a panic.
        let out = run("{a:b}", &[("a:b", FmtValue::Sint(1))]);
        assert!(out.contains("Invalid key: a"), "{out}");
    }

    #[test]
    fn escaped_braces_are_emitted_literally() {
        assert_eq!(run("{{literal}}", &[]), "{literal}");
        assert_eq!(run("{{{{", &[]), "{{");
        assert_eq!(
            run("{{{k}}}", &[("k", FmtValue::Str(az("v")))]),
            "{v}",
            "escaped braces around a real placeholder"
        );
    }

    // ------------------------------------------------------------------
    // fmt_string: numeric limits, saturation and non-finite floats
    // ------------------------------------------------------------------

    #[test]
    fn every_scalar_variant_round_trips_through_its_display_impl() {
        // Invariant: for scalars, the strfmt path (`DisplayStr`) and the
        // `fmt::Display` path must agree. This also pins NaN/inf/MIN/MAX.
        for v in scalar_variants() {
            let out = run("{v}", &[("v", v.clone())]);
            assert_eq!(out, v.to_string(), "mismatch for {v:?}");
            assert!(!is_err_str(&out), "unexpected error for {v:?}: {out}");
        }
    }

    #[test]
    fn integer_limits_match_the_native_rendering() {
        assert_eq!(
            run("{v}", &[("v", FmtValue::Slong(i64::MIN))]),
            i64::MIN.to_string()
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::Ulong(u64::MAX))]),
            u64::MAX.to_string()
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::Usize(usize::MAX))]),
            usize::MAX.to_string()
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::Isize(isize::MIN))]),
            isize::MIN.to_string()
        );
        assert_eq!(run("{v}", &[("v", FmtValue::Schar(i8::MIN))]), "-128");
    }

    #[test]
    fn non_finite_floats_render_without_panicking() {
        assert_eq!(run("{v}", &[("v", FmtValue::Float(f32::NAN))]), "NaN");
        assert_eq!(run("{v}", &[("v", FmtValue::Float(f32::INFINITY))]), "inf");
        assert_eq!(
            run("{v}", &[("v", FmtValue::Double(f64::NEG_INFINITY))]),
            "-inf"
        );
        // Precision must not turn NaN/inf into a panic or a bogus number.
        assert_eq!(run("{v:.2}", &[("v", FmtValue::Double(f64::NAN))]), "NaN");
        assert_eq!(
            run("{v:.5}", &[("v", FmtValue::Double(f64::INFINITY))]),
            "inf"
        );
    }

    #[test]
    fn float_extremes_match_the_native_rendering() {
        assert_eq!(
            run("{v}", &[("v", FmtValue::Double(f64::MAX))]),
            f64::MAX.to_string()
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::Double(f64::MIN_POSITIVE))]),
            f64::MIN_POSITIVE.to_string()
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::Float(f32::MIN))]),
            f32::MIN.to_string()
        );
    }

    #[test]
    fn radix_format_codes_match_the_native_rendering() {
        assert_eq!(run("{v:x}", &[("v", FmtValue::Uint(255))]), "ff");
        assert_eq!(run("{v:X}", &[("v", FmtValue::Uint(255))]), "FF");
        assert_eq!(run("{v:#x}", &[("v", FmtValue::Uint(255))]), "0xff");
        assert_eq!(run("{v:b}", &[("v", FmtValue::Uchar(5))]), "101");
        assert_eq!(run("{v:o}", &[("v", FmtValue::Uint(8))]), "10");
        // Two's-complement rendering of the most negative value must not panic.
        assert_eq!(
            run("{v:b}", &[("v", FmtValue::Sint(i32::MIN))]),
            format!("{:b}", i32::MIN)
        );
        assert_eq!(
            run("{v:x}", &[("v", FmtValue::Usize(usize::MAX))]),
            format!("{:x}", usize::MAX)
        );
    }

    #[test]
    fn explicit_sign_is_honoured_for_numbers() {
        assert_eq!(run("{v:+}", &[("v", FmtValue::Sint(5))]), "+5");
        assert_eq!(run("{v:+}", &[("v", FmtValue::Sint(-5))]), "-5");
        assert_eq!(run("{v:+}", &[("v", FmtValue::Sint(0))]), "+0");
    }

    #[test]
    fn exponent_and_precision_codes_match_the_native_rendering() {
        assert_eq!(
            run("{v:e}", &[("v", FmtValue::Double(1234.0))]),
            format!("{:e}", 1234.0_f64)
        );
        assert_eq!(
            run("{v:.3}", &[("v", FmtValue::Double(3.141_59))]),
            format!("{:.3}", 3.141_59_f64)
        );
    }

    // ------------------------------------------------------------------
    // fmt_string: format-spec type errors are reported, not panicked
    // ------------------------------------------------------------------

    #[test]
    fn precision_on_an_integer_is_a_type_error() {
        let out = run("{v:.2}", &[("v", FmtValue::Sint(5))]);
        assert!(out.contains("precision not allowed for integers"), "{out}");
    }

    #[test]
    fn a_string_valued_arg_rejects_numeric_format_codes() {
        for spec in ["{v:x}", "{v:b}", "{v:e}", "{v:#}", "{v:+}"] {
            let out = run(spec, &[("v", FmtValue::Str(az("hi")))]);
            assert!(is_err_str(&out), "{spec} unexpectedly succeeded: {out}");
        }
    }

    #[test]
    fn a_float_valued_arg_rejects_integer_format_codes() {
        let out = run("{v:x}", &[("v", FmtValue::Double(1.0))]);
        assert!(out.contains("Unknown format code"), "{out}");
        let out = run("{v:#}", &[("v", FmtValue::Double(1.0))]);
        assert!(out.contains("Alternate form"), "{out}");
    }

    #[test]
    fn an_unknown_type_specifier_is_reported() {
        let out = run("{v:z}", &[("v", FmtValue::Sint(1))]);
        assert!(out.contains("Invalid type specifier"), "{out}");
    }

    #[test]
    fn unsupported_specs_degrade_to_an_error_string() {
        // These are documented `strfmt` gaps; assert they surface as errors
        // instead of producing wrong output or panicking.
        let zero_pad = run("{v:08}", &[("v", FmtValue::Sint(42))]);
        assert!(is_err_str(&zero_pad), "{zero_pad}");
        let thousands = run("{v:,}", &[("v", FmtValue::Sint(1000))]);
        assert!(thousands.contains("not yet supported"), "{thousands}");
    }

    #[test]
    fn an_out_of_range_width_or_precision_is_rejected_not_overflowed() {
        // 20 nines does not fit in the i64 the spec parser uses.
        let w = run("{v:>99999999999999999999}", &[("v", FmtValue::Sint(1))]);
        assert!(w.contains("overflow error when parsing width"), "{w}");
        let p = run(
            "{v:.99999999999999999999}",
            &[("v", FmtValue::Str(az("abc")))],
        );
        assert!(p.contains("overflow error when parsing precision"), "{p}");
        // A bare `.` with no digits is malformed.
        let d = run("{v:.}", &[("v", FmtValue::Str(az("abc")))]);
        assert!(d.contains("missing precision"), "{d}");
    }

    // ------------------------------------------------------------------
    // fmt_string: unicode
    // ------------------------------------------------------------------

    #[test]
    fn keys_values_and_literals_may_be_non_ascii() {
        assert_eq!(
            run("こんにちは、{名前}！🦀", &[("名前", FmtValue::Str(az("世界")))]),
            "こんにちは、世界！🦀"
        );
    }

    #[test]
    fn width_and_precision_count_chars_not_bytes() {
        // A byte-oriented implementation would slice mid-codepoint and panic.
        assert_eq!(
            run("{v:.2}", &[("v", FmtValue::Str(az("日本語")))]),
            "日本",
            "precision truncates on a char boundary"
        );
        assert_eq!(
            run("{v:>5}", &[("v", FmtValue::Str(az("日本語")))]),
            "  日本語",
            "width pads by chars, not bytes"
        );
    }

    #[test]
    fn a_multibyte_fill_char_pads_correctly() {
        assert_eq!(run("{v:🦀>4}", &[("v", FmtValue::Str(az("ab")))]), "🦀🦀ab");
    }

    #[test]
    fn control_and_nul_characters_pass_through_untouched() {
        let weird = "a\0b\tc\nd\u{7f}e";
        assert_eq!(run(weird, &[]), weird);
    }

    // ------------------------------------------------------------------
    // fmt_string: string alignment / truncation
    // ------------------------------------------------------------------

    #[test]
    fn string_alignment_and_truncation_behave_as_specified() {
        assert_eq!(run("{v:>8}", &[("v", FmtValue::Str(az("ab")))]), "      ab");
        assert_eq!(run("{v:<4}|", &[("v", FmtValue::Str(az("ab")))]), "ab  |");
        assert_eq!(run("{v:^6}", &[("v", FmtValue::Str(az("ab")))]), "  ab  ");
        assert_eq!(run("{v:.3}", &[("v", FmtValue::Str(az("abcdefg")))]), "abc");
        assert_eq!(
            run("{v:.0}", &[("v", FmtValue::Str(az("abc")))]),
            "",
            "zero precision erases the value"
        );
        assert_eq!(
            run("{v:.9}", &[("v", FmtValue::Str(az("abc")))]),
            "abc",
            "precision longer than the value does not overrun"
        );
        assert_eq!(
            run("{v:>2}", &[("v", FmtValue::Str(az("abcdef")))]),
            "abcdef",
            "width smaller than the value never truncates"
        );
    }

    // ------------------------------------------------------------------
    // fmt_string: StrVec
    // ------------------------------------------------------------------

    #[test]
    fn strvec_renders_as_a_bracketed_comma_list() {
        assert_eq!(run("{v}", &[("v", FmtValue::StrVec(strvec(&[])))]), "[]");
        assert_eq!(run("{v}", &[("v", FmtValue::StrVec(strvec(&["a"])))]), "[a]");
        assert_eq!(
            run("{v}", &[("v", FmtValue::StrVec(strvec(&["a", "b", "c"])))]),
            "[a, b, c]"
        );
        assert_eq!(
            run("{v}", &[("v", FmtValue::StrVec(strvec(&["", ""])))]),
            "[, ]",
            "empty members still get a separator"
        );
    }

    #[test]
    fn strvec_applies_the_width_spec_to_every_fragment() {
        // KNOWN QUIRK, pinned deliberately: `DisplayStr for StrVec` reuses the
        // same `Formatter` for "[", each item and "]", so a width spec pads each
        // fragment rather than the list as a whole. 3 fragments * width 10 = 30.
        let out = run("{v:>10}", &[("v", FmtValue::StrVec(strvec(&["a"])))]);
        assert_eq!(out, "         [         a         ]");
        assert_eq!(out.chars().count(), 30);
    }

    // ------------------------------------------------------------------
    // fmt_string: size / stress
    // ------------------------------------------------------------------

    #[test]
    fn many_placeholders_do_not_blow_up() {
        let format: String = "{k}".repeat(500);
        let out = run(&format, &[("k", FmtValue::Uchar(9))]);
        assert_eq!(out, "9".repeat(500));
    }

    #[test]
    fn a_very_long_value_is_substituted_whole() {
        let big = "x".repeat(50_000);
        let out = run("{v}", &[("v", FmtValue::Str(AzString::from(big.clone())))]);
        assert_eq!(out.len(), 50_000);
        assert_eq!(out, big);
    }

    #[test]
    fn a_long_literal_format_string_is_returned_verbatim() {
        let big = "y".repeat(100_000);
        assert_eq!(fmt_string(AzString::from(big.clone()), FmtArgVec::new()), big);
    }

    // ------------------------------------------------------------------
    // FmtValue::fmt (Display) — the serializer under test
    // ------------------------------------------------------------------

    #[test]
    fn display_never_panics_on_any_variant_or_edge_value() {
        for v in scalar_variants() {
            let _ = v.to_string();
        }
        let _ = FmtValue::StrVec(strvec(&[])).to_string();
        let _ = FmtValue::StrVec(strvec(&["a", "", "🦀"])).to_string();
    }

    #[test]
    fn display_renders_each_variant_as_its_inner_value() {
        assert_eq!(FmtValue::Bool(true).to_string(), "true");
        assert_eq!(FmtValue::Bool(false).to_string(), "false");
        assert_eq!(FmtValue::Uchar(u8::MAX).to_string(), "255");
        assert_eq!(FmtValue::Schar(i8::MIN).to_string(), "-128");
        assert_eq!(FmtValue::Sshort(i16::MIN).to_string(), "-32768");
        assert_eq!(FmtValue::Ulong(u64::MAX).to_string(), u64::MAX.to_string());
        assert_eq!(FmtValue::Slong(i64::MIN).to_string(), i64::MIN.to_string());
        assert_eq!(FmtValue::Usize(usize::MAX).to_string(), usize::MAX.to_string());
        assert_eq!(FmtValue::Isize(isize::MIN).to_string(), isize::MIN.to_string());
        assert_eq!(FmtValue::Float(f32::NAN).to_string(), "NaN");
        assert_eq!(FmtValue::Double(f64::INFINITY).to_string(), "inf");
        assert_eq!(FmtValue::Double(f64::NEG_INFINITY).to_string(), "-inf");
    }

    #[test]
    fn display_of_a_string_is_unquoted_but_a_strvec_is_debug_quoted() {
        // The `Str` arm goes through `str::fmt`, the `StrVec` arm through
        // `Vec<&str>::fmt` (Debug) — so quoting differs. Pin both.
        assert_eq!(FmtValue::Str(az("a\"b")).to_string(), "a\"b");
        assert_eq!(FmtValue::Str(AzString::default()).to_string(), "");
        assert_eq!(FmtValue::StrVec(strvec(&[])).to_string(), "[]");
        assert_eq!(
            FmtValue::StrVec(strvec(&["a", "b"])).to_string(),
            "[\"a\", \"b\"]"
        );
    }

    #[test]
    fn display_of_a_strvec_diverges_from_the_strfmt_rendering() {
        // Documented divergence: `Display` is Debug-quoted, `DisplayStr` is not.
        let v = FmtValue::StrVec(strvec(&["a", "b"]));
        assert_ne!(run("{v}", &[("v", v.clone())]), v.to_string());
        assert_eq!(run("{v}", &[("v", v)]), "[a, b]");
    }

    #[test]
    fn display_forwards_width_precision_and_fill_flags() {
        assert_eq!(format!("{:>6}", FmtValue::Uint(42)), "    42");
        assert_eq!(format!("{:<6}|", FmtValue::Uint(42)), "42    |");
        assert_eq!(format!("{:0>6}", FmtValue::Uint(42)), "000042");
        assert_eq!(format!("{:>6}", FmtValue::Str(az("ab"))), "    ab");
        assert_eq!(
            format!("{:.2}", FmtValue::Double(2.0 / 3.0)),
            format!("{:.2}", 2.0_f64 / 3.0)
        );
    }

    #[test]
    fn debug_is_variant_tagged() {
        assert_eq!(format!("{:?}", FmtValue::Uint(1)), "Uint(1)");
        assert_eq!(format!("{:?}", FmtValue::Bool(false)), "Bool(false)");
    }

    // ------------------------------------------------------------------
    // Derived predicates: PartialEq / PartialOrd / Clone
    // ------------------------------------------------------------------

    #[test]
    fn nan_breaks_reflexive_equality_and_has_no_ordering() {
        let nan = FmtValue::Double(f64::NAN);
        assert_ne!(nan, nan.clone(), "NaN must not compare equal to itself");
        assert_eq!(nan.partial_cmp(&nan.clone()), None);
        assert_eq!(
            FmtValue::Float(f32::NAN).partial_cmp(&FmtValue::Float(1.0)),
            None
        );
    }

    #[test]
    fn equality_and_ordering_discriminate_between_variants() {
        assert_eq!(FmtValue::Uint(1), FmtValue::Uint(1));
        assert_ne!(
            FmtValue::Uint(1),
            FmtValue::Sint(1),
            "same numeric value, different variant"
        );
        // Derived PartialOrd compares the discriminant first.
        assert!(FmtValue::Bool(true) < FmtValue::Uchar(0));
        assert!(FmtValue::Uchar(1) > FmtValue::Uchar(0));
        assert!(FmtValue::Double(f64::NEG_INFINITY) < FmtValue::Double(f64::MIN));
    }

    #[test]
    fn cloning_an_arg_vec_preserves_equality_and_survives_a_double_drop() {
        let args = FmtArgVec::from_vec(vec![
            FmtArg {
                key: az("a"),
                value: FmtValue::Str(az("one")),
            },
            FmtArg {
                key: az("b"),
                value: FmtValue::StrVec(strvec(&["x", "y"])),
            },
        ]);
        let copy = args.clone();
        assert_eq!(args, copy);
        assert_eq!(
            fmt_string(az("{a}{b}"), args),
            fmt_string(az("{a}{b}"), copy.clone()),
            "a clone must format identically to its source"
        );
        drop(copy); // both the original and the clone are dropped: no double free
    }
}