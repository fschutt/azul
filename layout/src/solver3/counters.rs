//! CSS Counter Support
//!
//! Implements CSS counters for ordered lists and generated content as per CSS spec.
//! Counters are cached per-node in the `LayoutCache` and computed during layout traversal.
//!
//! This module is the single canonical home for numbering-system formatting
//! (decimal, roman, alphabetic, greek). The low-level converters
//! [`to_roman`], [`to_alphabetic`], and [`to_greek`] back both [`format_counter`]
//! (list markers) and `super::pagination::CounterFormat` (paged-media page counters),
//! so the same number renders identically in both contexts.

use alloc::string::String;

use azul_css::props::style::lists::StyleListStyleType;

/// Formats a counter value into a string based on the list style type.
///
/// Implements CSS counter styles for various numbering systems.
#[must_use]
pub fn format_counter(value: i32, style: StyleListStyleType) -> String {
    match style {
        StyleListStyleType::None => String::new(),
        StyleListStyleType::Disc => "•".to_string(),
        StyleListStyleType::Circle => "◦".to_string(),
        StyleListStyleType::Square => "▪".to_string(),
        StyleListStyleType::Decimal => value.to_string(),
        StyleListStyleType::DecimalLeadingZero => format!("{value:02}"),
        StyleListStyleType::LowerAlpha => decimal_fallback(value, with_sign(value, |n| to_alphabetic(n, false))),
        StyleListStyleType::UpperAlpha => decimal_fallback(value, with_sign(value, |n| to_alphabetic(n, true))),
        StyleListStyleType::LowerRoman => with_sign(value, |n| to_roman(n, false)),
        StyleListStyleType::UpperRoman => with_sign(value, |n| to_roman(n, true)),
        StyleListStyleType::LowerGreek => decimal_fallback(value, with_sign(value, |n| to_greek(n, false))),
        StyleListStyleType::UpperGreek => decimal_fallback(value, with_sign(value, |n| to_greek(n, true))),
    }
}

/// CSS fallback: when an alphabetic/greek counter style cannot represent a value
/// (e.g. `value == 0`, where `to_alphabetic`/`to_greek` yield an empty string), the
/// spec falls back to `decimal` so the marker is never blank.
fn decimal_fallback(value: i32, formatted: String) -> String {
    if formatted.is_empty() {
        value.to_string()
    } else {
        formatted
    }
}

// --- Formatting Helpers ---

/// Formats the magnitude of `value`, prefixing `-` for negatives.
///
/// Avoids the lossy `value as u32` cast: a negative counter such as `-3` in
/// `lower-roman` formats as `-iii` instead of wrapping to a huge unsigned value.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/counter/fixed-point cast
fn with_sign<F: Fn(usize) -> String>(value: i32, format: F) -> String {
    if value < 0 {
        let magnitude = i64::from(value).unsigned_abs() as usize;
        format!("-{}", format(magnitude))
    } else {
        format(value as usize)
    }
}

/// Converts a number to alphabetic representation (a, b, c, ..., z, aa, ab, ...).
///
/// This implements the CSS `lower-alpha` and `upper-alpha` counter styles.
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
pub(crate) fn to_alphabetic(mut num: usize, uppercase: bool) -> String {
    if num == 0 {
        return String::new();
    }

    let mut result = String::new();
    let base = if uppercase { b'A' } else { b'a' };

    while num > 0 {
        let remainder = ((num - 1) % 26) as u8;
        result.insert(0, (base + remainder) as char);
        num = (num - 1) / 26;
    }

    result
}

/// Converts a number to Roman numeral representation.
///
/// This implements the CSS `lower-roman` and `upper-roman` counter styles.
pub(crate) fn to_roman(mut num: usize, uppercase: bool) -> String {
    const MAX_ROMAN: usize = 3999;
    if num == 0 {
        return "0".to_string();
    }
    if num > MAX_ROMAN {
        // Roman numerals traditionally don't go beyond 3999
        return num.to_string();
    }

    let numerals = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];

    let mut result = String::new();
    for (value, numeral) in &numerals {
        while num >= *value {
            result.push_str(numeral);
            num -= value;
        }
    }

    if uppercase {
        result.to_uppercase()
    } else {
        result
    }
}

/// Converts a number to Greek letter representation.
///
/// This implements the CSS `lower-greek` and `upper-greek` counter styles.
/// Supports α, β, γ, ... (24 letters). For numbers > 24, wraps as αα, αβ, etc.
pub(crate) fn to_greek(num: usize, uppercase: bool) -> String {
    const GREEK_LOWER: &[char] = &[
        'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ',
        'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
    ];
    const GREEK_UPPER: &[char] = &[
        'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ', 'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'Σ',
        'Τ', 'Υ', 'Φ', 'Χ', 'Ψ', 'Ω',
    ];

    if num == 0 {
        return String::new();
    }

    let letters = if uppercase { GREEK_UPPER } else { GREEK_LOWER };

    if num <= letters.len() {
        return letters[num - 1].to_string();
    }

    let mut result = String::new();
    let mut remaining = num;
    while remaining > 0 {
        remaining -= 1;
        result.insert(0, letters[remaining % letters.len()]);
        remaining /= letters.len();
    }
    result
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // Fixtures / helpers
    // ------------------------------------------------------------------

    /// Every variant of the enum under test — used for "no panic on any style"
    /// sweeps.
    const ALL_STYLES: [StyleListStyleType; 12] = [
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

    /// Adversarial `i32` inputs: the saturation points, the sign boundary, and
    /// the roman-numeral cliff at 3999/4000.
    const EDGE_VALUES: [i32; 12] = [
        i32::MIN,
        i32::MIN + 1,
        -4000,
        -3999,
        -27,
        -1,
        0,
        1,
        26,
        3999,
        4000,
        i32::MAX,
    ];

    const GREEK_LOWER_LETTERS: [char; 24] = [
        'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ',
        'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
    ];
    const GREEK_UPPER_LETTERS: [char; 24] = [
        'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ', 'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'Σ',
        'Τ', 'Υ', 'Φ', 'Χ', 'Ψ', 'Ω',
    ];

    /// Independent decoder for the bijective base-26 alphabetic system.
    /// Returns `None` if `s` contains a character outside the expected case.
    fn decode_alphabetic(s: &str, uppercase: bool) -> Option<u128> {
        if s.is_empty() {
            return None;
        }
        let base = if uppercase { b'A' } else { b'a' };
        let mut acc: u128 = 0;
        for b in s.bytes() {
            if b < base || b >= base + 26 {
                return None;
            }
            acc = acc * 26 + u128::from(b - base + 1);
        }
        Some(acc)
    }

    /// Independent decoder for the bijective base-24 greek system.
    fn decode_greek(s: &str, uppercase: bool) -> Option<u128> {
        let letters = if uppercase {
            &GREEK_UPPER_LETTERS
        } else {
            &GREEK_LOWER_LETTERS
        };
        if s.is_empty() {
            return None;
        }
        let mut acc: u128 = 0;
        for c in s.chars() {
            let idx = letters.iter().position(|l| *l == c)?;
            acc = acc * 24 + (idx as u128 + 1);
        }
        Some(acc)
    }

    /// Independent subtractive-notation roman decoder.
    fn decode_roman(s: &str) -> Option<u32> {
        fn digit(c: char) -> Option<i64> {
            match c {
                'i' => Some(1),
                'v' => Some(5),
                'x' => Some(10),
                'l' => Some(50),
                'c' => Some(100),
                'd' => Some(500),
                'm' => Some(1000),
                _ => None,
            }
        }
        if s.is_empty() {
            return None;
        }
        let digits: Option<Vec<i64>> = s.chars().map(digit).collect();
        let digits = digits?;
        // Accumulate signed: subtractive pairs go negative before the following
        // larger numeral is added, so an unsigned accumulator would underflow.
        let mut total: i64 = 0;
        for (i, d) in digits.iter().enumerate() {
            // A smaller numeral placed before a larger one is subtractive.
            if digits[i + 1..].iter().any(|next| next > d) {
                total -= *d;
            } else {
                total += *d;
            }
        }
        u32::try_from(total).ok()
    }

    // ------------------------------------------------------------------
    // to_alphabetic — numeric: zero / min_max / overflow / round-trip
    // ------------------------------------------------------------------

    #[test]
    fn to_alphabetic_zero_is_empty_not_a_panic() {
        // 0 is not representable in a bijective base — the function signals this
        // with an empty string (the caller is expected to `decimal_fallback`).
        assert_eq!(to_alphabetic(0, false), "");
        assert_eq!(to_alphabetic(0, true), "");
    }

    #[test]
    fn to_alphabetic_known_values() {
        assert_eq!(to_alphabetic(1, false), "a");
        assert_eq!(to_alphabetic(26, false), "z");
        // The carry boundary: 27 must roll over to two letters, not wrap to "a".
        assert_eq!(to_alphabetic(27, false), "aa");
        assert_eq!(to_alphabetic(28, false), "ab");
        assert_eq!(to_alphabetic(52, false), "az");
        assert_eq!(to_alphabetic(53, false), "ba");
        assert_eq!(to_alphabetic(702, false), "zz");
        assert_eq!(to_alphabetic(703, false), "aaa");
    }

    #[test]
    fn to_alphabetic_uppercase_only_shifts_case() {
        for n in 1..=1000usize {
            let lower = to_alphabetic(n, false);
            let upper = to_alphabetic(n, true);
            assert_eq!(upper, lower.to_uppercase(), "case mismatch at {n}");
            assert!(
                upper.bytes().all(|b| b.is_ascii_uppercase()),
                "non-uppercase byte at {n}: {upper}"
            );
            assert!(
                lower.bytes().all(|b| b.is_ascii_lowercase()),
                "non-lowercase byte at {n}: {lower}"
            );
        }
    }

    #[test]
    fn to_alphabetic_round_trips_through_an_independent_decoder() {
        for n in 1..=5000u128 {
            for uppercase in [false, true] {
                let encoded = to_alphabetic(n as usize, uppercase);
                assert_eq!(
                    decode_alphabetic(&encoded, uppercase),
                    Some(n),
                    "round-trip failed for {n} (uppercase={uppercase}) -> {encoded}"
                );
            }
        }
    }

    /// Asserts that no two inputs in `markers` produced the same string.
    fn assert_all_distinct(markers: &[String], what: &str) {
        let mut sorted: Vec<&String> = markers.iter().collect();
        sorted.sort();
        for pair in sorted.windows(2) {
            assert_ne!(pair[0], pair[1], "duplicate {what} marker: {}", pair[0]);
        }
    }

    #[test]
    fn to_alphabetic_is_injective() {
        // Two different counters must never render the same marker.
        let markers: Vec<String> = (1..=2000usize).map(|n| to_alphabetic(n, false)).collect();
        assert_all_distinct(&markers, "alphabetic");
    }

    #[test]
    fn to_alphabetic_usize_max_terminates_and_stays_ascii() {
        // `num = (num - 1) / 26` strictly decreases, so this must terminate;
        // the `base + remainder` byte add must not overflow past 'z'/'Z'.
        let lower = to_alphabetic(usize::MAX, false);
        let upper = to_alphabetic(usize::MAX, true);
        assert!(!lower.is_empty());
        assert!(lower.bytes().all(|b: u8| b.is_ascii_lowercase()));
        assert!(upper.bytes().all(|b: u8| b.is_ascii_uppercase()));
        // The extreme still encodes *exactly* — no digit dropped, no wrap.
        assert_eq!(decode_alphabetic(&lower, false), Some(usize::MAX as u128));
        assert_eq!(decode_alphabetic(&upper, true), Some(usize::MAX as u128));
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn to_alphabetic_magnitude_of_i32_min_does_not_panic() {
        // The magnitude that `with_sign` hands over for i32::MIN.
        let magnitude = 2_147_483_648usize;
        let s = to_alphabetic(magnitude, false);
        assert!(!s.is_empty());
        assert_eq!(decode_alphabetic(&s, false), Some(magnitude as u128));
    }

    // ------------------------------------------------------------------
    // to_roman — numeric: zero / limits / overflow / round-trip
    // ------------------------------------------------------------------

    #[test]
    fn to_roman_zero_falls_back_to_decimal_zero() {
        // Roman has no zero; the impl emits "0" rather than an empty marker.
        assert_eq!(to_roman(0, false), "0");
        assert_eq!(to_roman(0, true), "0");
    }

    #[test]
    fn to_roman_known_values() {
        assert_eq!(to_roman(1, false), "i");
        assert_eq!(to_roman(4, false), "iv");
        assert_eq!(to_roman(9, false), "ix");
        assert_eq!(to_roman(14, false), "xiv");
        assert_eq!(to_roman(40, false), "xl");
        assert_eq!(to_roman(90, false), "xc");
        assert_eq!(to_roman(400, false), "cd");
        assert_eq!(to_roman(900, false), "cm");
        assert_eq!(to_roman(1990, false), "mcmxc");
        assert_eq!(to_roman(2024, false), "mmxxiv");
        assert_eq!(to_roman(3999, false), "mmmcmxcix");
        assert_eq!(to_roman(2024, true), "MMXXIV");
        assert_eq!(to_roman(3999, true), "MMMCMXCIX");
    }

    #[test]
    fn to_roman_at_and_past_the_3999_cliff() {
        // 3999 is the last representable numeral...
        assert_eq!(to_roman(3999, false), "mmmcmxcix");
        // ...and 4000 must degrade to decimal instead of emitting "mmmm" or
        // looping forever.
        assert_eq!(to_roman(4000, false), "4000");
        assert_eq!(to_roman(4000, true), "4000");
        assert_eq!(to_roman(4001, false), "4001");
    }

    #[test]
    fn to_roman_usize_max_degrades_to_decimal() {
        assert_eq!(to_roman(usize::MAX, false), usize::MAX.to_string());
        assert_eq!(to_roman(usize::MAX, true), usize::MAX.to_string());
        // The i32::MIN magnitude handed over by `with_sign`.
        assert_eq!(to_roman(2_147_483_648, false), "2147483648");
    }

    #[test]
    fn to_roman_round_trips_over_the_whole_representable_range() {
        for n in 1..=3999u32 {
            let lower = to_roman(n as usize, false);
            let upper = to_roman(n as usize, true);
            assert_eq!(
                decode_roman(&lower),
                Some(n),
                "round-trip failed for {n} -> {lower}"
            );
            assert_eq!(upper, lower.to_uppercase(), "case mismatch at {n}");
            // No numeral may repeat more than 3 times in a row (mmm is the max).
            assert!(
                !lower.contains("iiii")
                    && !lower.contains("xxxx")
                    && !lower.contains("cccc")
                    && !lower.contains("mmmm"),
                "malformed numeral at {n}: {lower}"
            );
            assert!(lower.bytes().all(|b| b"ivxlcdm".contains(&b)));
        }
    }

    // ------------------------------------------------------------------
    // to_greek — numeric + unicode: zero / wrap / round-trip
    // ------------------------------------------------------------------

    #[test]
    fn to_greek_zero_is_empty_not_a_panic() {
        assert_eq!(to_greek(0, false), "");
        assert_eq!(to_greek(0, true), "");
    }

    #[test]
    fn to_greek_known_values_and_wrap_boundary() {
        assert_eq!(to_greek(1, false), "α");
        assert_eq!(to_greek(2, false), "β");
        assert_eq!(to_greek(24, false), "ω");
        // 24 letters, then the documented wrap to two letters.
        assert_eq!(to_greek(25, false), "αα");
        assert_eq!(to_greek(26, false), "αβ");
        assert_eq!(to_greek(48, false), "αω");
        assert_eq!(to_greek(49, false), "βα");
        assert_eq!(to_greek(1, true), "Α");
        assert_eq!(to_greek(24, true), "Ω");
        assert_eq!(to_greek(25, true), "ΑΑ");
    }

    #[test]
    fn to_greek_emits_multibyte_chars_without_slicing_bugs() {
        // Each greek letter is 2 bytes in UTF-8: byte len must be 2x char count,
        // and the string must survive a full char walk (i.e. `insert(0, ..)` never
        // split a code point).
        let s = to_greek(25, false);
        assert_eq!(s.chars().count(), 2);
        assert_eq!(s.len(), 4);
        assert!(s.chars().all(|c| GREEK_LOWER_LETTERS.contains(&c)));
        assert!(s.is_char_boundary(0) && s.is_char_boundary(2) && s.is_char_boundary(4));
    }

    #[test]
    fn to_greek_round_trips_through_an_independent_decoder() {
        for n in 1..=5000u128 {
            for uppercase in [false, true] {
                let encoded = to_greek(n as usize, uppercase);
                assert_eq!(
                    decode_greek(&encoded, uppercase),
                    Some(n),
                    "round-trip failed for {n} (uppercase={uppercase}) -> {encoded}"
                );
            }
        }
    }

    #[test]
    fn to_greek_is_injective() {
        let markers: Vec<String> = (1..=2000usize).map(|n| to_greek(n, true)).collect();
        assert_all_distinct(&markers, "greek");
    }

    #[test]
    fn to_greek_usize_max_terminates_and_stays_in_the_alphabet() {
        // `remaining = (remaining - 1) / 24` strictly decreases -> must terminate.
        let lower = to_greek(usize::MAX, false);
        let upper = to_greek(usize::MAX, true);
        assert!(!lower.is_empty());
        assert!(lower.chars().all(|c| GREEK_LOWER_LETTERS.contains(&c)));
        assert!(upper.chars().all(|c| GREEK_UPPER_LETTERS.contains(&c)));
        // The extreme still encodes *exactly* — no letter dropped, no wrap.
        assert_eq!(decode_greek(&lower, false), Some(usize::MAX as u128));
        assert_eq!(decode_greek(&upper, true), Some(usize::MAX as u128));
        assert_eq!(lower.chars().count(), upper.chars().count());
    }

    // ------------------------------------------------------------------
    // with_sign — numeric: sign handling, no lossy unsigned wrap
    // ------------------------------------------------------------------

    #[test]
    fn with_sign_passes_the_magnitude_not_a_wrapped_cast() {
        // The whole point of `with_sign`: -3 must hand `3` to the formatter,
        // NOT `(-3 as u32) == 4294967293`.
        assert_eq!(with_sign(-3, |n| n.to_string()), "-3");
        assert_eq!(with_sign(-1, |n| n.to_string()), "-1");
        assert_eq!(with_sign(0, |n| n.to_string()), "0");
        assert_eq!(with_sign(1, |n| n.to_string()), "1");
        assert_eq!(with_sign(i32::MAX, |n| n.to_string()), "2147483647");
    }

    #[test]
    fn with_sign_handles_i32_min_without_overflow() {
        // `-i32::MIN` overflows i32 — the impl must widen to i64 first.
        assert_eq!(with_sign(i32::MIN, |n| n.to_string()), "-2147483648");
        assert_eq!(with_sign(i32::MIN + 1, |n| n.to_string()), "-2147483647");
    }

    #[test]
    fn with_sign_zero_is_unsigned() {
        // 0 is not negative, so no "-0" marker may be produced.
        let s = with_sign(0, |n| to_alphabetic(n, false));
        assert!(!s.starts_with('-'), "produced a signed zero: {s}");
        assert_eq!(s, "");
    }

    #[test]
    fn with_sign_prefixes_exactly_one_minus() {
        for v in [-1, -26, -3999, -4000, i32::MIN] {
            let s = with_sign(v, |n| to_roman(n, false));
            assert!(s.starts_with('-'), "missing sign for {v}: {s}");
            assert_eq!(s.matches('-').count(), 1, "double sign for {v}: {s}");
            assert!(s.len() > 1, "sign with no magnitude for {v}");
        }
    }

    #[test]
    fn with_sign_is_transparent_to_the_formatter_output() {
        // A formatter returning an empty string yields just the sign — the sign is
        // never swallowed (`decimal_fallback` is what rescues the empty case).
        assert_eq!(with_sign(-5, |_| String::new()), "-");
        assert_eq!(with_sign(5, |_| String::new()), "");
        // Unicode from the formatter passes through byte-for-byte.
        assert_eq!(with_sign(-5, |_| "αβγ".to_string()), "-αβγ");
    }

    #[test]
    fn with_sign_negation_is_symmetric_across_the_range() {
        for v in [1i32, 2, 26, 27, 3999, 4000, i32::MAX] {
            let pos = with_sign(v, |n| to_alphabetic(n, false));
            let neg = with_sign(-v, |n| to_alphabetic(n, false));
            assert_eq!(neg, format!("-{pos}"), "asymmetric at {v}");
        }
    }

    // ------------------------------------------------------------------
    // decimal_fallback — numeric: the "never blank" invariant
    // ------------------------------------------------------------------

    #[test]
    fn decimal_fallback_replaces_empty_with_decimal() {
        assert_eq!(decimal_fallback(0, String::new()), "0");
        assert_eq!(decimal_fallback(-1, String::new()), "-1");
        assert_eq!(decimal_fallback(i32::MAX, String::new()), "2147483647");
        assert_eq!(decimal_fallback(i32::MIN, String::new()), "-2147483648");
    }

    #[test]
    fn decimal_fallback_passes_non_empty_through_untouched() {
        // Even a "wrong looking" formatted value is preserved: the fallback keys
        // off emptiness only, never off the numeric value.
        assert_eq!(decimal_fallback(5, "a".to_string()), "a");
        assert_eq!(decimal_fallback(0, "z".to_string()), "z");
        assert_eq!(decimal_fallback(0, "α".to_string()), "α");
        // Whitespace is NOT empty -> not replaced.
        assert_eq!(decimal_fallback(7, " ".to_string()), " ");
        // A lone NUL byte counts as non-empty too.
        assert_eq!(decimal_fallback(7, "\0".to_string()), "\0");
    }

    #[test]
    fn decimal_fallback_output_is_never_blank_for_any_i32() {
        for v in EDGE_VALUES {
            assert!(
                !decimal_fallback(v, String::new()).is_empty(),
                "blank marker for {v}"
            );
        }
    }

    // ------------------------------------------------------------------
    // format_counter — serializer: no panic, well-formed, spec-shaped
    // ------------------------------------------------------------------

    #[test]
    fn format_counter_no_panic_on_edge_values_for_every_style() {
        for style in ALL_STYLES {
            for v in EDGE_VALUES {
                let s = format_counter(v, style);
                if style == StyleListStyleType::None {
                    assert!(s.is_empty(), "`none` must render nothing, got {s:?}");
                } else {
                    // The core CSS invariant: a marker is never blank.
                    assert!(!s.is_empty(), "blank marker for {v} in {style:?}");
                    // ...and never blank-looking either.
                    assert!(
                        !s.chars().all(char::is_whitespace),
                        "whitespace-only marker for {v} in {style:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn format_counter_is_deterministic() {
        for style in ALL_STYLES {
            for v in EDGE_VALUES {
                assert_eq!(format_counter(v, style), format_counter(v, style));
            }
        }
    }

    #[test]
    fn format_counter_default_style_is_disc_and_ignores_the_value() {
        assert_eq!(format_counter(0, StyleListStyleType::default()), "•");
        for v in EDGE_VALUES {
            assert_eq!(format_counter(v, StyleListStyleType::default()), "•");
        }
    }

    #[test]
    fn format_counter_bullet_styles_ignore_the_value() {
        for (style, expected) in [
            (StyleListStyleType::Disc, "•"),
            (StyleListStyleType::Circle, "◦"),
            (StyleListStyleType::Square, "▪"),
        ] {
            for v in EDGE_VALUES {
                let s = format_counter(v, style);
                assert_eq!(s, expected, "bullet changed with value {v}");
                assert_eq!(s.chars().count(), 1);
            }
        }
    }

    #[test]
    fn format_counter_decimal_matches_i32_display() {
        for v in EDGE_VALUES {
            assert_eq!(format_counter(v, StyleListStyleType::Decimal), v.to_string());
        }
    }

    #[test]
    fn format_counter_decimal_leading_zero_pads_single_digits() {
        assert_eq!(
            format_counter(0, StyleListStyleType::DecimalLeadingZero),
            "00"
        );
        assert_eq!(
            format_counter(5, StyleListStyleType::DecimalLeadingZero),
            "05"
        );
        assert_eq!(
            format_counter(9, StyleListStyleType::DecimalLeadingZero),
            "09"
        );
        assert_eq!(
            format_counter(10, StyleListStyleType::DecimalLeadingZero),
            "10"
        );
        assert_eq!(
            format_counter(100, StyleListStyleType::DecimalLeadingZero),
            "100"
        );
        assert_eq!(
            format_counter(i32::MAX, StyleListStyleType::DecimalLeadingZero),
            "2147483647"
        );
    }

    #[test]
    fn format_counter_decimal_leading_zero_negative_current_behavior() {
        // NOTE (reported, not weakened): `format!("{value:02}")` pads the *total*
        // width including the sign, so -5 renders as "-5". CSS
        // `decimal-leading-zero` pads the digits only, i.e. "-05". This test pins
        // the current behavior so the deviation is visible if/when it is fixed.
        assert_eq!(
            format_counter(-5, StyleListStyleType::DecimalLeadingZero),
            "-5"
        );
        assert_eq!(
            format_counter(i32::MIN, StyleListStyleType::DecimalLeadingZero),
            "-2147483648"
        );
    }

    #[test]
    fn format_counter_alpha_known_values_and_zero_fallback() {
        assert_eq!(format_counter(1, StyleListStyleType::LowerAlpha), "a");
        assert_eq!(format_counter(26, StyleListStyleType::LowerAlpha), "z");
        assert_eq!(format_counter(27, StyleListStyleType::LowerAlpha), "aa");
        assert_eq!(format_counter(1, StyleListStyleType::UpperAlpha), "A");
        assert_eq!(format_counter(27, StyleListStyleType::UpperAlpha), "AA");
        // 0 has no alphabetic representation -> decimal fallback, not a blank.
        assert_eq!(format_counter(0, StyleListStyleType::LowerAlpha), "0");
        assert_eq!(format_counter(0, StyleListStyleType::UpperAlpha), "0");
        // Negatives keep the sign instead of wrapping to a giant unsigned value.
        assert_eq!(format_counter(-1, StyleListStyleType::LowerAlpha), "-a");
        assert_eq!(format_counter(-3, StyleListStyleType::UpperAlpha), "-C");
    }

    #[test]
    fn format_counter_roman_known_values_and_limits() {
        assert_eq!(format_counter(1, StyleListStyleType::LowerRoman), "i");
        assert_eq!(format_counter(4, StyleListStyleType::LowerRoman), "iv");
        assert_eq!(format_counter(2024, StyleListStyleType::UpperRoman), "MMXXIV");
        assert_eq!(format_counter(3999, StyleListStyleType::LowerRoman), "mmmcmxcix");
        // Past the roman range -> decimal, never an unbounded "mmmm..." string.
        assert_eq!(format_counter(4000, StyleListStyleType::LowerRoman), "4000");
        assert_eq!(format_counter(4000, StyleListStyleType::UpperRoman), "4000");
        // No roman zero.
        assert_eq!(format_counter(0, StyleListStyleType::LowerRoman), "0");
        assert_eq!(format_counter(0, StyleListStyleType::UpperRoman), "0");
        assert_eq!(format_counter(-3, StyleListStyleType::LowerRoman), "-iii");
        assert_eq!(format_counter(-14, StyleListStyleType::UpperRoman), "-XIV");
    }

    #[test]
    fn format_counter_roman_at_i32_min_matches_signed_decimal() {
        // The magnitude of i32::MIN is far past MAX_ROMAN, so the marker degrades
        // to decimal — and must equal the plain decimal rendering, i.e. the
        // `unsigned_abs` widening must not have wrapped.
        assert_eq!(
            format_counter(i32::MIN, StyleListStyleType::LowerRoman),
            "-2147483648"
        );
        assert_eq!(
            format_counter(i32::MIN, StyleListStyleType::LowerRoman),
            format_counter(i32::MIN, StyleListStyleType::Decimal)
        );
    }

    #[test]
    fn format_counter_greek_known_values_and_zero_fallback() {
        assert_eq!(format_counter(1, StyleListStyleType::LowerGreek), "α");
        assert_eq!(format_counter(24, StyleListStyleType::LowerGreek), "ω");
        assert_eq!(format_counter(25, StyleListStyleType::LowerGreek), "αα");
        assert_eq!(format_counter(1, StyleListStyleType::UpperGreek), "Α");
        assert_eq!(format_counter(24, StyleListStyleType::UpperGreek), "Ω");
        // 0 -> decimal fallback, not a blank marker.
        assert_eq!(format_counter(0, StyleListStyleType::LowerGreek), "0");
        assert_eq!(format_counter(0, StyleListStyleType::UpperGreek), "0");
        assert_eq!(format_counter(-2, StyleListStyleType::LowerGreek), "-β");
        assert_eq!(format_counter(-1, StyleListStyleType::UpperGreek), "-Α");
    }

    #[test]
    fn format_counter_negative_is_positive_with_a_minus_for_letter_styles() {
        // Guards the regression the `with_sign` doc calls out: a lossy `as u32`
        // cast would make -3 render as a huge unsigned counter, not "-iii"/"-c".
        for style in [
            StyleListStyleType::LowerAlpha,
            StyleListStyleType::UpperAlpha,
            StyleListStyleType::LowerRoman,
            StyleListStyleType::UpperRoman,
            StyleListStyleType::LowerGreek,
            StyleListStyleType::UpperGreek,
        ] {
            for v in [1i32, 2, 3, 24, 25, 26, 27, 3999] {
                let pos = format_counter(v, style);
                let neg = format_counter(-v, style);
                assert_eq!(neg, format!("-{pos}"), "asymmetric at {v} in {style:?}");
                assert!(!pos.starts_with('-'), "positive gained a sign at {v}");
            }
        }
    }

    #[test]
    fn format_counter_letter_styles_never_leak_huge_unsigned_markers() {
        // A wrapped cast would produce a marker for -1 as long as the one for
        // 4294967295. Bound the length instead of trusting the exact string.
        for style in [
            StyleListStyleType::LowerAlpha,
            StyleListStyleType::UpperGreek,
        ] {
            let s = format_counter(-1, style);
            assert_eq!(s.chars().count(), 2, "suspiciously long marker: {s}");
        }
    }

    #[test]
    fn format_counter_marker_length_stays_bounded_at_i32_extremes() {
        // No style may blow up into a megabyte-long marker at the i32 extremes.
        for style in ALL_STYLES {
            for v in [i32::MIN, i32::MAX] {
                let s = format_counter(v, style);
                assert!(
                    s.chars().count() <= 32,
                    "marker for {v} in {style:?} is {} chars: {s}",
                    s.chars().count()
                );
            }
        }
    }
}
