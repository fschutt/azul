//! Azul-specific CSS properties for advanced layout features
//!
//! Defines `StyleExclusionMargin` (spacing between text and shape exclusions)
//! and `StyleHyphenationLanguage` (BCP 47 language code for automatic hyphenation).

use std::num::ParseFloatError;

#[cfg(feature = "parser")]
use crate::macros::*;
use crate::{
    corety::AzString,
    codegen::format::FormatAsRustCode,
    props::{
        basic::{length::parse_float_value, FloatValue},
        formatter::{FormatAsCssValue, PrintAsCssValue},
    },
};

/// `-azul-exclusion-margin` property: defines margin around shape exclusions
///
/// This property controls the spacing between text and shapes that text flows around.
/// It's similar to `shape-margin` but specifically for exclusions (text wrapping).
///
/// # Example
/// ```css
/// .element {
///     -azul-exclusion-margin: 10.5;
/// }
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleExclusionMargin {
    pub inner: FloatValue,
}

impl Default for StyleExclusionMargin {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(0),
        }
    }
}

impl StyleExclusionMargin {
    #[must_use] pub const fn is_initial(&self) -> bool {
        self.inner.number == 0
    }
}

impl PrintAsCssValue for StyleExclusionMargin {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner.get())
    }
}

impl FormatAsCssValue for StyleExclusionMargin {
    fn format_as_css_value(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.get())
    }
}

impl FormatAsRustCode for StyleExclusionMargin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleExclusionMargin {{ inner: FloatValue::const_new({}) }}",
            self.inner.get()
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleExclusionMarginParseError {
    FloatValue(ParseFloatError),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleExclusionMarginParseError);

#[cfg(feature = "parser")]
impl_display! { StyleExclusionMarginParseError, {
    FloatValue(e) => format!("Invalid -azul-exclusion-margin value: {}", e),
}}

#[cfg(feature = "parser")]
impl_from!(ParseFloatError, StyleExclusionMarginParseError::FloatValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleExclusionMarginParseErrorOwned {
    FloatValue(AzString),
}

#[cfg(feature = "parser")]
impl StyleExclusionMarginParseError {
    #[must_use] pub fn to_contained(&self) -> StyleExclusionMarginParseErrorOwned {
        match self {
            Self::FloatValue(e) => {
                StyleExclusionMarginParseErrorOwned::FloatValue(format!("{e}").into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleExclusionMarginParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleExclusionMarginParseError {
        match self {
            Self::FloatValue(_) => {
                // ParseFloatError can't be reconstructed from its display string,
                // so we create one by parsing a known-invalid string
                StyleExclusionMarginParseError::FloatValue("".parse::<f32>().unwrap_err())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `exclusion-margin` value.
pub fn parse_style_exclusion_margin(
    input: &str,
) -> Result<StyleExclusionMargin, StyleExclusionMarginParseError> {
    parse_float_value(input)
        .map(|inner| StyleExclusionMargin { inner })
        .map_err(StyleExclusionMarginParseError::FloatValue)
}

/// `-azul-hyphenation-language` property: specifies language for hyphenation
///
/// This property defines the language code (BCP 47 format) used for automatic
/// hyphenation. Examples: "en-US", "de-DE", "fr-FR"
///
/// # Example
/// ```css
/// .element {
///     -azul-hyphenation-language: "en-US";
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleHyphenationLanguage {
    pub inner: AzString,
}

impl Default for StyleHyphenationLanguage {
    fn default() -> Self {
        Self {
            inner: AzString::from_const_str("en-US"),
        }
    }
}

impl StyleHyphenationLanguage {
    #[must_use] pub fn is_initial(&self) -> bool {
        self.inner.as_str() == "en-US"
    }
}

impl PrintAsCssValue for StyleHyphenationLanguage {
    fn print_as_css_value(&self) -> String {
        format!("\"{}\"", self.inner.as_str())
    }
}

impl FormatAsCssValue for StyleHyphenationLanguage {
    fn format_as_css_value(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.inner.as_str())
    }
}

impl FormatAsRustCode for StyleHyphenationLanguage {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleHyphenationLanguage {{ inner: AzString::from_const_str(\"{}\") }}",
            self.inner.as_str()
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum StyleHyphenationLanguageParseError {
    InvalidString(String),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleHyphenationLanguageParseError);

#[cfg(feature = "parser")]
impl_display! { StyleHyphenationLanguageParseError, {
    InvalidString(e) => format!("Invalid -azul-hyphenation-language value: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleHyphenationLanguageParseErrorOwned {
    InvalidString(AzString),
}

#[cfg(feature = "parser")]
impl StyleHyphenationLanguageParseError {
    #[must_use] pub fn to_contained(&self) -> StyleHyphenationLanguageParseErrorOwned {
        match self {
            Self::InvalidString(e) => {
                StyleHyphenationLanguageParseErrorOwned::InvalidString(e.clone().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHyphenationLanguageParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleHyphenationLanguageParseError {
        match self {
            Self::InvalidString(e) => StyleHyphenationLanguageParseError::InvalidString(e.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `hyphenation-language` value.
pub fn parse_style_hyphenation_language(
    input: &str,
) -> Result<StyleHyphenationLanguage, StyleHyphenationLanguageParseError> {
    // Remove surrounding quotes if present. Require len >= 2 so a lone quote
    // (where starts_with and ends_with match the *same* char) is not stripped to
    // `&s[1..0]`, which would panic instead of failing validation below.
    let trimmed = input.trim();
    let unquoted = if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    // Basic BCP 47 validation: non-empty, ASCII alphanumeric + hyphens, no leading/trailing hyphens
    if unquoted.is_empty()
        || !unquoted.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || unquoted.starts_with('-')
        || unquoted.ends_with('-')
    {
        return Err(StyleHyphenationLanguageParseError::InvalidString(
            unquoted.to_string(),
        ));
    }

    Ok(StyleHyphenationLanguage {
        inner: AzString::from_string(unquoted.to_string()),
    })
}

#[cfg(test)]
mod tests {
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn test_parse_exclusion_margin() {
        let margin = parse_style_exclusion_margin("10.5").unwrap();
        assert_eq!(margin.inner.get(), 10.5);

        let margin = parse_style_exclusion_margin("0").unwrap();
        assert_eq!(margin.inner.get(), 0.0);
    }

    #[test]
    fn test_parse_hyphenation_language() {
        let lang = parse_style_hyphenation_language("\"en-US\"").unwrap();
        assert_eq!(lang.inner.as_str(), "en-US");

        let lang = parse_style_hyphenation_language("'de-DE'").unwrap();
        assert_eq!(lang.inner.as_str(), "de-DE");

        let lang = parse_style_hyphenation_language("fr-FR").unwrap();
        assert_eq!(lang.inner.as_str(), "fr-FR");

        let lang = parse_style_hyphenation_language("zh").unwrap();
        assert_eq!(lang.inner.as_str(), "zh");

        let lang = parse_style_hyphenation_language("sr-Latn-RS").unwrap();
        assert_eq!(lang.inner.as_str(), "sr-Latn-RS");

        // Double hyphen is permitted by the current ASCII/format rules.
        let lang = parse_style_hyphenation_language("en--US").unwrap();
        assert_eq!(lang.inner.as_str(), "en--US");
    }

    #[test]
    fn test_parse_hyphenation_language_invalid() {
        assert!(matches!(
            parse_style_hyphenation_language(""),
            Err(StyleHyphenationLanguageParseError::InvalidString(_))
        ));
        assert!(matches!(
            parse_style_hyphenation_language("-en"),
            Err(StyleHyphenationLanguageParseError::InvalidString(_))
        ));
        assert!(matches!(
            parse_style_hyphenation_language("en-"),
            Err(StyleHyphenationLanguageParseError::InvalidString(_))
        ));
        assert!(matches!(
            parse_style_hyphenation_language("en_US"),
            Err(StyleHyphenationLanguageParseError::InvalidString(_))
        ));
        assert!(matches!(
            parse_style_hyphenation_language("日本語"),
            Err(StyleHyphenationLanguageParseError::InvalidString(_))
        ));
    }

    #[test]
    fn test_exclusion_margin_default() {
        let margin = StyleExclusionMargin::default();
        assert_eq!(margin.inner.get(), 0.0);
        assert!(margin.is_initial());
    }

    #[test]
    fn test_hyphenation_language_default() {
        let lang = StyleHyphenationLanguage::default();
        assert_eq!(lang.inner.as_str(), "en-US");
    }
}

#[cfg(test)]
mod autotest_generated {
    //! Adversarial tests: malformed / huge / unicode parser input, numeric
    //! saturation (`FloatValue` encodes `f32 * 1000.0` into an `isize`, so every
    //! non-finite input must land on a *finite* encoded value), encode/decode
    //! round-trips and predicate invariants.
    #![allow(clippy::float_cmp)]

    use super::*;

    /// `FormatAsCssValue` needs a real `Formatter`; this adapter supplies one.
    struct AsCss<'a, T: FormatAsCssValue>(&'a T);

    impl<T: FormatAsCssValue> std::fmt::Display for AsCss<'_, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.format_as_css_value(f)
        }
    }

    // ---------------------------------------------------------------------
    // StyleExclusionMargin::is_initial (predicate)
    // ---------------------------------------------------------------------

    #[test]
    fn exclusion_margin_is_initial_true_and_false() {
        assert!(StyleExclusionMargin::default().is_initial());
        assert!(StyleExclusionMargin {
            inner: FloatValue::const_new(0)
        }
        .is_initial());
        assert!(!StyleExclusionMargin {
            inner: FloatValue::const_new(1)
        }
        .is_initial());
        assert!(!StyleExclusionMargin {
            inner: FloatValue::const_new(-1)
        }
        .is_initial());
    }

    #[test]
    fn exclusion_margin_is_initial_on_boundary_encodings() {
        // Negative zero and sub-precision magnitudes encode to 0 => "initial".
        for v in [0.0_f32, -0.0, 0.0004, -0.0004, f32::MIN_POSITIVE, 1e-30] {
            let m = StyleExclusionMargin {
                inner: FloatValue::new(v),
            };
            assert!(m.is_initial(), "{v} should encode to the initial value");
            assert_eq!(m.inner.get(), 0.0);
        }

        // NaN saturates to 0 in the f32 -> isize cast, so it is *also* "initial".
        let nan = StyleExclusionMargin {
            inner: FloatValue::new(f32::NAN),
        };
        assert!(nan.is_initial());
        assert!(!nan.inner.get().is_nan());

        // Saturating extremes are deterministic and decidedly not initial.
        for v in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            let m = StyleExclusionMargin {
                inner: FloatValue::new(v),
            };
            assert!(!m.is_initial(), "{v} must not be reported as initial");
            assert!(m.inner.get().is_finite());
        }
    }

    // ---------------------------------------------------------------------
    // StyleHyphenationLanguage::is_initial (predicate)
    // ---------------------------------------------------------------------

    #[test]
    fn hyphenation_is_initial_true_and_false() {
        assert!(StyleHyphenationLanguage::default().is_initial());
        assert!(StyleHyphenationLanguage {
            inner: AzString::from_const_str("en-US"),
        }
        .is_initial());

        // The comparison is exact and case-sensitive.
        for not_initial in ["", " ", "en-us", "EN-US", "en-US ", "en", "de-DE", "en\u{0}US"] {
            assert!(
                !StyleHyphenationLanguage {
                    inner: AzString::from_string(not_initial.to_string()),
                }
                .is_initial(),
                "{not_initial:?} must not be reported as initial"
            );
        }
    }

    #[test]
    fn hyphenation_is_initial_on_extreme_strings_does_not_panic() {
        for s in [
            "\u{1F600}".to_string(),
            "e\u{0301}n-US".to_string(), // combining acute on the 'e'
            "en-US\u{200B}".to_string(), // zero-width space
            "a".repeat(1_000_000),
        ] {
            let lang = StyleHyphenationLanguage {
                inner: AzString::from_string(s.clone()),
            };
            assert!(!lang.is_initial(), "{s:?} must not be reported as initial");
        }
    }

    // ---------------------------------------------------------------------
    // Formatting / round-trip of the value types
    // ---------------------------------------------------------------------

    #[test]
    fn exclusion_margin_print_and_format_agree() {
        for v in [0.0_f32, 10.5, -3.25, 123.456, f32::INFINITY, f32::NAN] {
            let m = StyleExclusionMargin {
                inner: FloatValue::new(v),
            };
            let printed = m.print_as_css_value();
            assert_eq!(printed, AsCss(&m).to_string());
            // Whatever went in, what comes out is always a finite number.
            assert!(!printed.contains("NaN") && !printed.contains("inf"));
            assert!(m.format_as_rust_code(0).contains(&printed));
        }
    }

    #[test]
    fn hyphenation_print_and_format_agree() {
        for s in ["en-US", "", "a", "\u{1F600}", "quote\"inside"] {
            let lang = StyleHyphenationLanguage {
                inner: AzString::from_string(s.to_string()),
            };
            let printed = lang.print_as_css_value();
            assert_eq!(printed, AsCss(&lang).to_string());
            assert_eq!(printed, format!("\"{s}\""));
            assert!(lang.format_as_rust_code(0).contains(s));
        }
    }

    #[test]
    fn exclusion_margin_ord_and_hash_are_consistent_with_value() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let hash = |m: &StyleExclusionMargin| {
            let mut h = DefaultHasher::new();
            m.hash(&mut h);
            h.finish()
        };

        let a = StyleExclusionMargin {
            inner: FloatValue::new(1.5),
        };
        let b = StyleExclusionMargin {
            inner: FloatValue::new(1.5),
        };
        let c = StyleExclusionMargin {
            inner: FloatValue::new(2.5),
        };

        assert_eq!(a, b);
        assert_eq!(hash(&a), hash(&b));
        assert!(a < c);
        assert!(a.inner.get() < c.inner.get());

        // 1.5 and 1.5004 collide: CSS keeps ~3 decimals of precision.
        let d = StyleExclusionMargin {
            inner: FloatValue::new(1.5004),
        };
        assert_eq!(a, d);
        assert_eq!(hash(&a), hash(&d));
    }

    // ---------------------------------------------------------------------
    // parse_style_exclusion_margin (parser)
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_valid_minimal() {
        assert_eq!(
            parse_style_exclusion_margin("0").unwrap(),
            StyleExclusionMargin::default()
        );
        assert_eq!(parse_style_exclusion_margin("1").unwrap().inner.get(), 1.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_empty_and_whitespace_only() {
        for input in ["", " ", "   ", "\t\n", "\r\n\t ", "\u{00A0}"] {
            assert!(
                parse_style_exclusion_margin(input).is_err(),
                "{input:?} must not parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_garbage_is_rejected() {
        for input in [
            "abc", "px", "10px", "1,5", "1_000", "0x10", "--5", "5-", "+-1", ".", "-", "+", "e5",
            "1e", "1.2.3", "null", "None", "{}", "()", "/*10*/", "10 20", "\0", "\u{7}\u{1b}[0m",
        ] {
            assert!(
                parse_style_exclusion_margin(input).is_err(),
                "{input:?} must be rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_leading_trailing_junk() {
        // Surrounding ASCII whitespace is trimmed...
        assert_eq!(
            parse_style_exclusion_margin("  10.5  ").unwrap().inner.get(),
            10.5
        );
        assert_eq!(
            parse_style_exclusion_margin("\n\t-3.25\t\n")
                .unwrap()
                .inner
                .get(),
            -3.25
        );
        // ...but any trailing non-numeric junk is fatal, never silently dropped.
        for input in ["10.5px", "10.5;garbage", "10.5 !important", "10.5;", "10.5%"] {
            assert!(
                parse_style_exclusion_margin(input).is_err(),
                "{input:?} must be rejected, not truncated to a number"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_boundary_numbers() {
        // Signed zeroes all collapse onto the initial value.
        for input in ["0", "-0", "+0", "0.0", "-0.0", "0e0"] {
            let m = parse_style_exclusion_margin(input).unwrap();
            assert_eq!(m.inner.number, 0, "{input:?} should encode to zero");
            assert!(m.is_initial());
        }

        // Sub-precision magnitudes truncate to zero rather than rounding away.
        for input in ["0.0004", "-0.0004", "1e-30", "-1e-30"] {
            let m = parse_style_exclusion_margin(input).unwrap();
            assert_eq!(m.inner.number, 0, "{input:?} should truncate to zero");
        }

        // i64::MAX / f32::MAX overflow the isize encoding and must saturate,
        // not wrap or panic.
        for input in [
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            f32::MAX.to_string(),
            format!("{}", f32::MIN),
            "1e38".to_string(),
            "-1e38".to_string(),
        ] {
            let m = parse_style_exclusion_margin(&input).unwrap();
            assert!(
                m.inner.get().is_finite(),
                "{input:?} must decode to a finite value, got {}",
                m.inner.get()
            );
        }

        assert_eq!(
            parse_style_exclusion_margin(&f32::MAX.to_string())
                .unwrap()
                .inner
                .number,
            isize::MAX
        );
        assert_eq!(
            parse_style_exclusion_margin(&format!("{}", f32::MIN))
                .unwrap()
                .inner
                .number,
            isize::MIN
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_nan_and_infinity_never_escape() {
        // Rust's f32 parser accepts these; the isize encoding must sanitize them
        // so that no NaN/inf ever reaches layout.
        let nan = parse_style_exclusion_margin("NaN").unwrap();
        assert_eq!(nan.inner.number, 0);
        assert!(!nan.inner.get().is_nan());
        assert!(nan.is_initial());
        assert_eq!(parse_style_exclusion_margin("nan").unwrap().inner.number, 0);
        assert_eq!(parse_style_exclusion_margin("-NaN").unwrap().inner.number, 0);

        for input in ["inf", "infinity", "+inf", "INF", "Infinity", "1e400"] {
            let m = parse_style_exclusion_margin(input).unwrap();
            assert_eq!(m.inner.number, isize::MAX, "{input:?} must saturate");
            assert!(m.inner.get().is_finite());
        }
        for input in ["-inf", "-infinity", "-INF", "-1e400"] {
            let m = parse_style_exclusion_margin(input).unwrap();
            assert_eq!(m.inner.number, isize::MIN, "{input:?} must saturate");
            assert!(m.inner.get().is_finite());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_unicode_input() {
        // Non-ASCII digits/letters are not numbers.
        for input in [
            "\u{1F600}",         // emoji
            "\u{FF15}",          // fullwidth digit five
            "1\u{0301}",         // combining acute after a digit
            "\u{202E}10.5",      // right-to-left override
            "\u{FEFF}10.5",      // BOM
            "١٢٣",               // arabic-indic digits
            "10.5\u{1F4A9}",     // trailing emoji
        ] {
            assert!(
                parse_style_exclusion_margin(input).is_err(),
                "{input:?} must be rejected"
            );
        }

        // Unicode whitespace is stripped by str::trim; whatever the outcome, the
        // parser must never invent a wrong number.
        let r = parse_style_exclusion_margin("\u{00A0}10.5\u{2003}");
        assert!(r.is_err() || r.unwrap().inner.get() == 10.5);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_extremely_long_input() {
        // 1M digits overflow f32 -> inf -> saturated isize. Must not hang or panic.
        let huge = "9".repeat(1_000_000);
        let m = parse_style_exclusion_margin(&huge).unwrap();
        assert_eq!(m.inner.number, isize::MAX);
        assert!(m.inner.get().is_finite());

        // 1M fractional digits are a legal (if silly) float.
        let long_fraction = format!("1.{}", "0".repeat(1_000_000));
        assert_eq!(
            parse_style_exclusion_margin(&long_fraction).unwrap().inner.get(),
            1.0
        );

        // 1M garbage bytes still just return Err.
        assert!(parse_style_exclusion_margin(&"z".repeat(1_000_000)).is_err());
        assert!(parse_style_exclusion_margin(&" ".repeat(1_000_000)).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_deeply_nested_input_does_not_stack_overflow() {
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_exclusion_margin(&nested).is_err());

        let nested_number = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_exclusion_margin(&nested_number).is_err());

        assert!(parse_style_exclusion_margin(&"-".repeat(10_000)).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_exclusion_margin_round_trips_through_css_and_rust_code() {
        for input in ["0", "1", "10.5", "-3.25", "0.001", "123.456", "-0.5", "1000"] {
            let parsed = parse_style_exclusion_margin(input).unwrap();

            // encode == decode: printing and re-parsing is a fixed point.
            let printed = parsed.print_as_css_value();
            let reparsed = parse_style_exclusion_margin(&printed).unwrap();
            assert_eq!(
                parsed, reparsed,
                "{input:?} printed as {printed:?} did not round-trip"
            );
            assert_eq!(printed, reparsed.print_as_css_value());

            // The generated Rust code embeds the same decoded value.
            assert!(parsed.format_as_rust_code(0).contains(&printed));
        }
    }

    // ---------------------------------------------------------------------
    // StyleExclusionMarginParseError::to_contained / ...Owned::to_shared
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn exclusion_margin_error_to_contained_carries_the_message() {
        let err = parse_style_exclusion_margin("garbage").unwrap_err();
        let StyleExclusionMarginParseErrorOwned::FloatValue(msg) = err.to_contained();
        assert!(!msg.as_str().is_empty());
        // impl_debug_as_display: Debug and Display must agree, and the Display
        // form must name the property.
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("-azul-exclusion-margin"));
        assert!(format!("{err}").contains(msg.as_str()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn exclusion_margin_error_to_shared_is_lossy_but_total() {
        // to_shared() cannot rebuild a ParseFloatError from its message, so it
        // always yields the empty-string error. Pin that: it is the one shape a
        // caller can rely on, and it must never panic - not even for a message
        // that no ParseFloatError would ever produce.
        let empty_err_msg: AzString = format!("{}", "".parse::<f32>().unwrap_err()).into();

        for msg in [
            String::new(),
            "invalid float literal".to_string(),
            "\u{1F600}".to_string(),
            "x".repeat(1_000_000),
        ] {
            let owned = StyleExclusionMarginParseErrorOwned::FloatValue(msg.clone().into());
            let shared = owned.to_shared();
            assert_eq!(
                shared.to_contained(),
                StyleExclusionMarginParseErrorOwned::FloatValue(empty_err_msg.clone()),
                "to_shared() should normalise {msg:?} onto the empty-string error"
            );
        }

        // Consequently the Owned -> shared -> Owned round-trip is *not* the
        // identity for a non-empty-string error; only the empty-string one is a
        // fixed point.
        let empty = parse_style_exclusion_margin("").unwrap_err();
        assert_eq!(empty.to_contained().to_shared(), empty);
        assert_eq!(empty.to_contained().to_shared().to_contained(), empty.to_contained());
    }

    // ---------------------------------------------------------------------
    // parse_style_hyphenation_language (parser)
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_valid_minimal() {
        let lang = parse_style_hyphenation_language("en-US").unwrap();
        assert_eq!(lang.inner.as_str(), "en-US");
        assert!(lang.is_initial());
        assert_eq!(parse_style_hyphenation_language("a").unwrap().inner.as_str(), "a");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_empty_and_whitespace_only() {
        for input in ["", " ", "   ", "\t\n", "\r\n\t ", "\"\"", "''", "\" \""] {
            assert!(
                parse_style_hyphenation_language(input).is_err(),
                "{input:?} must not parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_garbage_is_rejected() {
        for input in [
            "en_US",     // underscore is not BCP 47
            "-en",       // leading hyphen
            "en-",       // trailing hyphen
            "en US",     // interior space
            "\" en-US \"", // interior space after unquoting
            "en;US",
            "en/US",
            "en.US",
            "en*",
            "<script>",
            "en\0US",    // interior NUL
            "en\nUS",
            "'en-US\"",  // mismatched quotes: quotes survive into the tag
            "\"en-US'",
            "\"en-US",   // unterminated
            "en-US\"",
        ] {
            assert!(
                matches!(
                    parse_style_hyphenation_language(input),
                    Err(StyleHyphenationLanguageParseError::InvalidString(_))
                ),
                "{input:?} must be rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_leading_trailing_junk() {
        // Surrounding whitespace is trimmed, inside and outside of quotes.
        assert_eq!(
            parse_style_hyphenation_language("  en-US  ").unwrap().inner.as_str(),
            "en-US"
        );
        assert_eq!(
            parse_style_hyphenation_language("\t\"de-DE\"\n").unwrap().inner.as_str(),
            "de-DE"
        );
        // Trailing junk is fatal, never silently dropped.
        for input in ["en-US;", "en-US !important", "\"en-US\";", "en-US /*c*/"] {
            assert!(
                parse_style_hyphenation_language(input).is_err(),
                "{input:?} must be rejected, not truncated"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_unicode_input() {
        for input in [
            "日本語",
            "\u{1F600}",
            "\"\u{1F600}\"",
            "e\u{0301}n-US",   // combining acute
            "en-US\u{200B}",   // zero-width space
            "\u{FEFF}en-US",   // BOM
            "ｅｎ-ＵＳ",        // fullwidth latin
            "ру-RU",
        ] {
            assert!(
                matches!(
                    parse_style_hyphenation_language(input),
                    Err(StyleHyphenationLanguageParseError::InvalidString(_))
                ),
                "{input:?} must be rejected without panicking"
            );
        }

        // A multibyte char right inside the quotes must not split a char boundary.
        let r = parse_style_hyphenation_language("\"日本語\"");
        assert!(r.is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_accepts_any_ascii_alphanumeric_tag() {
        // The validation is "ASCII alphanumeric + interior hyphens", so purely
        // numeric and nonsense-but-ASCII tags are accepted today. Characterises
        // the current (lax) behaviour so a future tightening is a visible change.
        for input in ["0", "123", "NaN", "inf", "zzzzzz", "sr-Latn-RS", "x-private"] {
            let lang = parse_style_hyphenation_language(input).unwrap();
            assert_eq!(lang.inner.as_str(), input);
        }
        assert_eq!(
            parse_style_hyphenation_language(&i64::MAX.to_string())
                .unwrap()
                .inner
                .as_str(),
            i64::MAX.to_string()
        );
        // ...but a negative number starts with a hyphen and is therefore rejected.
        assert!(parse_style_hyphenation_language(&i64::MIN.to_string()).is_err());
        assert!(parse_style_hyphenation_language("-0").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_extremely_long_input() {
        let huge = "a".repeat(1_000_000);
        assert_eq!(
            parse_style_hyphenation_language(&huge).unwrap().inner.as_str(),
            huge
        );

        let huge_quoted = format!("\"{huge}\"");
        assert_eq!(
            parse_style_hyphenation_language(&huge_quoted)
                .unwrap()
                .inner
                .as_str(),
            huge
        );

        // A 1M-char run of hyphens is rejected (leading hyphen), not hung on.
        assert!(parse_style_hyphenation_language(&"-".repeat(1_000_000)).is_err());
        // 1M non-ASCII bytes: rejected, and the error carries the whole input.
        assert!(parse_style_hyphenation_language(&"é".repeat(1_000_000)).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_deeply_nested_input_does_not_stack_overflow() {
        let nested = format!("{}en-US{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_hyphenation_language(&nested).is_err());

        let nested_quotes = format!("{}en-US{}", "\"".repeat(10_000), "\"".repeat(10_000));
        assert!(parse_style_hyphenation_language(&nested_quotes).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_round_trips_through_css_and_rust_code() {
        for input in ["en-US", "de-DE", "zh", "sr-Latn-RS", "en--US", "x-private", "0"] {
            let parsed = parse_style_hyphenation_language(input).unwrap();

            // print_as_css_value() re-quotes; re-parsing must strip the quotes
            // back to exactly the same tag (encode == decode).
            let printed = parsed.print_as_css_value();
            assert_eq!(printed, format!("\"{input}\""));
            let reparsed = parse_style_hyphenation_language(&printed).unwrap();
            assert_eq!(parsed, reparsed, "{input:?} did not round-trip");
            assert_eq!(printed, reparsed.print_as_css_value());

            // Single quotes are an equally valid encoding of the same value.
            assert_eq!(
                parse_style_hyphenation_language(&format!("'{input}'")).unwrap(),
                parsed
            );

            assert!(parsed.format_as_rust_code(0).contains(input));
        }
    }

    // ---------------------------------------------------------------------
    // StyleHyphenationLanguageParseError::to_contained / ...Owned::to_shared
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn hyphenation_error_to_contained_carries_the_offending_string() {
        let err = parse_style_hyphenation_language("en_US").unwrap_err();
        let StyleHyphenationLanguageParseErrorOwned::InvalidString(msg) = err.to_contained();
        assert_eq!(msg.as_str(), "en_US");
        assert_eq!(format!("{err:?}"), format!("{err}"));
        assert!(format!("{err}").contains("-azul-hyphenation-language"));
        assert!(format!("{err}").contains("en_US"));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn hyphenation_error_round_trips_losslessly() {
        for msg in [
            String::new(),
            "en_US".to_string(),
            "\u{1F600}".to_string(),
            "\0".to_string(),
            "x".repeat(1_000_000),
        ] {
            let shared = StyleHyphenationLanguageParseError::InvalidString(msg.clone());
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "{msg:?} lost data on round-trip");
            assert_eq!(owned.to_shared().to_contained(), owned);

            let owned_direct =
                StyleHyphenationLanguageParseErrorOwned::InvalidString(msg.clone().into());
            assert_eq!(owned_direct, owned);
            let StyleHyphenationLanguageParseErrorOwned::InvalidString(inner) = owned_direct;
            assert_eq!(inner.as_str(), msg);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn hyphenation_error_for_empty_input_reports_the_unquoted_string() {
        // The error carries the *unquoted* text, not the raw input.
        let err = parse_style_hyphenation_language("\"\"").unwrap_err();
        assert_eq!(
            err.to_contained(),
            StyleHyphenationLanguageParseErrorOwned::InvalidString(AzString::from_const_str(""))
        );
        assert!(err.to_contained().to_shared() == err);
    }

    // ---------------------------------------------------------------------
    // Regression: a lone quote must fail validation, not panic.
    // ---------------------------------------------------------------------

    /// A lone quote character must return `Err`, not panic.
    ///
    /// `parse_style_hyphenation_language("\"")` used to see a string that both
    /// starts and ends with `"`, slice `&trimmed[1..trimmed.len() - 1]` ==
    /// `&s[1..0]`, and panic with "slice index starts at 1 but ends at 0". Any
    /// CSS input of `-azul-hyphenation-language: ";` reached it. The `len() >= 2`
    /// guard now keeps a single-quote input intact so it fails BCP-47
    /// validation cleanly.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_hyphenation_language_lone_quote_must_not_panic() {
        for input in ["\"", "'", " \" ", "\t'\n"] {
            assert!(
                parse_style_hyphenation_language(input).is_err(),
                "{input:?} must return Err, not panic"
            );
        }
    }
}
