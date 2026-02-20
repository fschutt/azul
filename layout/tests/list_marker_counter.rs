/// Tests for CSS counter formatting (CSS Lists Module Level 3)
///
/// Tests cover:
/// - `format_counter()`: all list-style-type values
///   - none, disc, circle, square (symbolic markers)
///   - decimal, decimal-leading-zero (numeric)
///   - lower-alpha, upper-alpha (alphabetic)
///   - lower-roman, upper-roman (additive Roman numerals)
///   - lower-greek, upper-greek (Greek letters)
///
/// CSS Spec references:
/// - CSS Lists Module Level 3 § 7 Counter Styles
/// - CSS Lists Module Level 3 § 3 Markers
/// - https://www.w3.org/TR/css-counter-styles-3/

use azul_css::props::style::lists::StyleListStyleType;
use azul_layout::solver3::counters::format_counter;

// ============================================================================
// list-style-type: none
// ============================================================================

#[test]
fn test_format_counter_none() {
    // CSS Lists L3: none → generates no marker string
    assert_eq!(format_counter(1, StyleListStyleType::None), "");
    assert_eq!(format_counter(0, StyleListStyleType::None), "");
    assert_eq!(format_counter(99, StyleListStyleType::None), "");
}

// ============================================================================
// Symbolic markers: disc, circle, square
// ============================================================================

#[test]
fn test_format_counter_disc() {
    // CSS Lists L3: disc → U+2022 BULLET (•)
    // The marker is always the same regardless of counter value
    assert_eq!(format_counter(1, StyleListStyleType::Disc), "•");
    assert_eq!(format_counter(2, StyleListStyleType::Disc), "•");
    assert_eq!(format_counter(99, StyleListStyleType::Disc), "•");
}

#[test]
fn test_format_counter_circle() {
    // CSS Lists L3: circle → U+25E6 WHITE BULLET (◦)
    assert_eq!(format_counter(1, StyleListStyleType::Circle), "◦");
    assert_eq!(format_counter(5, StyleListStyleType::Circle), "◦");
}

#[test]
fn test_format_counter_square() {
    // CSS Lists L3: square → U+25AA BLACK SMALL SQUARE (▪)
    assert_eq!(format_counter(1, StyleListStyleType::Square), "▪");
    assert_eq!(format_counter(10, StyleListStyleType::Square), "▪");
}

// ============================================================================
// list-style-type: decimal
// ============================================================================

#[test]
fn test_format_counter_decimal_basic() {
    // CSS Counter Styles L3 § 6.1: decimal → Western Arabic numerals
    assert_eq!(format_counter(1, StyleListStyleType::Decimal), "1");
    assert_eq!(format_counter(2, StyleListStyleType::Decimal), "2");
    assert_eq!(format_counter(10, StyleListStyleType::Decimal), "10");
    assert_eq!(format_counter(100, StyleListStyleType::Decimal), "100");
}

#[test]
fn test_format_counter_decimal_zero() {
    assert_eq!(format_counter(0, StyleListStyleType::Decimal), "0");
}

#[test]
fn test_format_counter_decimal_negative() {
    // Negative values should produce negative decimal output
    assert_eq!(format_counter(-1, StyleListStyleType::Decimal), "-1");
    assert_eq!(format_counter(-99, StyleListStyleType::Decimal), "-99");
}

// ============================================================================
// list-style-type: decimal-leading-zero
// ============================================================================

#[test]
fn test_format_counter_decimal_leading_zero() {
    // CSS Counter Styles L3: decimal-leading-zero → zero-padded to at least 2 digits
    assert_eq!(
        format_counter(1, StyleListStyleType::DecimalLeadingZero),
        "01"
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
        format_counter(99, StyleListStyleType::DecimalLeadingZero),
        "99"
    );
    assert_eq!(
        format_counter(100, StyleListStyleType::DecimalLeadingZero),
        "100"
    );
}

// ============================================================================
// list-style-type: lower-alpha / upper-alpha
// ============================================================================

#[test]
fn test_format_counter_lower_alpha_basic() {
    // CSS Counter Styles L3 § 6.1.4: lower-alpha → a, b, c, ..., z
    assert_eq!(format_counter(1, StyleListStyleType::LowerAlpha), "a");
    assert_eq!(format_counter(2, StyleListStyleType::LowerAlpha), "b");
    assert_eq!(format_counter(3, StyleListStyleType::LowerAlpha), "c");
    assert_eq!(format_counter(26, StyleListStyleType::LowerAlpha), "z");
}

#[test]
fn test_format_counter_lower_alpha_beyond_26() {
    // CSS Counter Styles L3: after z, wraps to aa, ab, ..., az, ba, ...
    assert_eq!(format_counter(27, StyleListStyleType::LowerAlpha), "aa");
    assert_eq!(format_counter(28, StyleListStyleType::LowerAlpha), "ab");
    assert_eq!(format_counter(52, StyleListStyleType::LowerAlpha), "az");
    assert_eq!(format_counter(53, StyleListStyleType::LowerAlpha), "ba");
}

#[test]
fn test_format_counter_lower_alpha_zero() {
    // Zero should produce empty string (no alphabetic representation)
    assert_eq!(format_counter(0, StyleListStyleType::LowerAlpha), "");
}

#[test]
fn test_format_counter_upper_alpha_basic() {
    // CSS Counter Styles L3 § 6.1.4: upper-alpha → A, B, C, ..., Z
    assert_eq!(format_counter(1, StyleListStyleType::UpperAlpha), "A");
    assert_eq!(format_counter(2, StyleListStyleType::UpperAlpha), "B");
    assert_eq!(format_counter(26, StyleListStyleType::UpperAlpha), "Z");
    assert_eq!(format_counter(27, StyleListStyleType::UpperAlpha), "AA");
}

// ============================================================================
// list-style-type: lower-roman / upper-roman
// ============================================================================

#[test]
fn test_format_counter_lower_roman_basic() {
    // CSS Counter Styles L3 § 6.1: lower-roman → i, ii, iii, iv, v, ...
    assert_eq!(format_counter(1, StyleListStyleType::LowerRoman), "i");
    assert_eq!(format_counter(2, StyleListStyleType::LowerRoman), "ii");
    assert_eq!(format_counter(3, StyleListStyleType::LowerRoman), "iii");
    assert_eq!(format_counter(4, StyleListStyleType::LowerRoman), "iv");
    assert_eq!(format_counter(5, StyleListStyleType::LowerRoman), "v");
    assert_eq!(format_counter(6, StyleListStyleType::LowerRoman), "vi");
    assert_eq!(format_counter(9, StyleListStyleType::LowerRoman), "ix");
    assert_eq!(format_counter(10, StyleListStyleType::LowerRoman), "x");
}

#[test]
fn test_format_counter_lower_roman_extended() {
    // Larger Roman numerals
    assert_eq!(format_counter(14, StyleListStyleType::LowerRoman), "xiv");
    assert_eq!(format_counter(40, StyleListStyleType::LowerRoman), "xl");
    assert_eq!(format_counter(50, StyleListStyleType::LowerRoman), "l");
    assert_eq!(format_counter(90, StyleListStyleType::LowerRoman), "xc");
    assert_eq!(format_counter(100, StyleListStyleType::LowerRoman), "c");
    assert_eq!(format_counter(400, StyleListStyleType::LowerRoman), "cd");
    assert_eq!(format_counter(500, StyleListStyleType::LowerRoman), "d");
    assert_eq!(format_counter(900, StyleListStyleType::LowerRoman), "cm");
    assert_eq!(
        format_counter(1000, StyleListStyleType::LowerRoman),
        "m"
    );
}

#[test]
fn test_format_counter_lower_roman_complex() {
    // Common complex values
    assert_eq!(
        format_counter(1999, StyleListStyleType::LowerRoman),
        "mcmxcix"
    );
    assert_eq!(
        format_counter(2024, StyleListStyleType::LowerRoman),
        "mmxxiv"
    );
    assert_eq!(
        format_counter(3999, StyleListStyleType::LowerRoman),
        "mmmcmxcix"
    );
}

#[test]
fn test_format_counter_lower_roman_zero() {
    // Zero has no Roman numeral representation → fallback to "0"
    assert_eq!(format_counter(0, StyleListStyleType::LowerRoman), "0");
}

#[test]
fn test_format_counter_lower_roman_beyond_3999() {
    // Roman numerals don't go beyond 3999 → fallback to decimal
    assert_eq!(
        format_counter(4000, StyleListStyleType::LowerRoman),
        "4000"
    );
    assert_eq!(
        format_counter(10000, StyleListStyleType::LowerRoman),
        "10000"
    );
}

#[test]
fn test_format_counter_upper_roman_basic() {
    // CSS Counter Styles L3 § 6.1: upper-roman → I, II, III, IV, V, ...
    assert_eq!(format_counter(1, StyleListStyleType::UpperRoman), "I");
    assert_eq!(format_counter(4, StyleListStyleType::UpperRoman), "IV");
    assert_eq!(format_counter(9, StyleListStyleType::UpperRoman), "IX");
    assert_eq!(format_counter(14, StyleListStyleType::UpperRoman), "XIV");
    assert_eq!(
        format_counter(2024, StyleListStyleType::UpperRoman),
        "MMXXIV"
    );
}

// ============================================================================
// list-style-type: lower-greek / upper-greek
// ============================================================================

#[test]
fn test_format_counter_lower_greek_basic() {
    // CSS Counter Styles L3: lower-greek → α, β, γ, δ, ...
    assert_eq!(format_counter(1, StyleListStyleType::LowerGreek), "α");
    assert_eq!(format_counter(2, StyleListStyleType::LowerGreek), "β");
    assert_eq!(format_counter(3, StyleListStyleType::LowerGreek), "γ");
    assert_eq!(format_counter(4, StyleListStyleType::LowerGreek), "δ");
    assert_eq!(format_counter(24, StyleListStyleType::LowerGreek), "ω");
}

#[test]
fn test_format_counter_lower_greek_zero() {
    assert_eq!(format_counter(0, StyleListStyleType::LowerGreek), "");
}

#[test]
fn test_format_counter_upper_greek_basic() {
    // CSS Counter Styles L3: upper-greek → Α, Β, Γ, Δ, ...
    assert_eq!(format_counter(1, StyleListStyleType::UpperGreek), "Α");
    assert_eq!(format_counter(2, StyleListStyleType::UpperGreek), "Β");
    assert_eq!(format_counter(24, StyleListStyleType::UpperGreek), "Ω");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_format_counter_large_decimal() {
    // Large counter values should still work
    assert_eq!(
        format_counter(999999, StyleListStyleType::Decimal),
        "999999"
    );
}

#[test]
fn test_format_counter_large_alpha() {
    // Very large alphabetic values produce multi-letter strings
    // 702 = 26*27 → "zz" (26 + 26*26 = 702)
    // Actually: 26 = z, 27 = aa, 702 = zz
    let result = format_counter(702, StyleListStyleType::LowerAlpha);
    assert_eq!(result, "zz");
}
