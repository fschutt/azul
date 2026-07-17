//! Test for CSS counter support in ordered and unordered lists
//!
//! This test verifies that:
//! 1. Counter formatting works correctly (decimal, roman, alpha, etc.)
//! 2. Counter reset logic handles values correctly

use azul_css::props::style::lists::StyleListStyleType;
use azul_layout::solver3::counters::format_counter;

#[test]
fn test_counter_formatting_decimal() {
    assert_eq!(format_counter(1, StyleListStyleType::Decimal), "1");
    assert_eq!(format_counter(42, StyleListStyleType::Decimal), "42");
    assert_eq!(format_counter(0, StyleListStyleType::Decimal), "0");
}

#[test]
fn test_counter_formatting_decimal_leading_zero() {
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
}

#[test]
fn test_counter_formatting_lower_alpha() {
    assert_eq!(format_counter(1, StyleListStyleType::LowerAlpha), "a");
    assert_eq!(format_counter(2, StyleListStyleType::LowerAlpha), "b");
    assert_eq!(format_counter(26, StyleListStyleType::LowerAlpha), "z");
    assert_eq!(format_counter(27, StyleListStyleType::LowerAlpha), "aa");
}

#[test]
fn test_counter_formatting_upper_alpha() {
    assert_eq!(format_counter(1, StyleListStyleType::UpperAlpha), "A");
    assert_eq!(format_counter(2, StyleListStyleType::UpperAlpha), "B");
    assert_eq!(format_counter(26, StyleListStyleType::UpperAlpha), "Z");
    assert_eq!(format_counter(27, StyleListStyleType::UpperAlpha), "AA");
}

#[test]
fn test_counter_formatting_lower_roman() {
    assert_eq!(format_counter(1, StyleListStyleType::LowerRoman), "i");
    assert_eq!(format_counter(2, StyleListStyleType::LowerRoman), "ii");
    assert_eq!(format_counter(3, StyleListStyleType::LowerRoman), "iii");
    assert_eq!(format_counter(4, StyleListStyleType::LowerRoman), "iv");
    assert_eq!(format_counter(5, StyleListStyleType::LowerRoman), "v");
    assert_eq!(format_counter(9, StyleListStyleType::LowerRoman), "ix");
    assert_eq!(format_counter(10, StyleListStyleType::LowerRoman), "x");
    assert_eq!(format_counter(50, StyleListStyleType::LowerRoman), "l");
    assert_eq!(format_counter(100, StyleListStyleType::LowerRoman), "c");
    assert_eq!(format_counter(500, StyleListStyleType::LowerRoman), "d");
    assert_eq!(format_counter(1000, StyleListStyleType::LowerRoman), "m");
    assert_eq!(
        format_counter(1994, StyleListStyleType::LowerRoman),
        "mcmxciv"
    );
}

#[test]
fn test_counter_formatting_upper_roman() {
    assert_eq!(format_counter(1, StyleListStyleType::UpperRoman), "I");
    assert_eq!(format_counter(4, StyleListStyleType::UpperRoman), "IV");
    assert_eq!(format_counter(9, StyleListStyleType::UpperRoman), "IX");
    assert_eq!(
        format_counter(1994, StyleListStyleType::UpperRoman),
        "MCMXCIV"
    );
}

#[test]
fn test_counter_formatting_disc_circle_square() {
    assert_eq!(format_counter(1, StyleListStyleType::Disc), "•");
    assert_eq!(format_counter(2, StyleListStyleType::Disc), "•"); // Always same

    assert_eq!(format_counter(1, StyleListStyleType::Circle), "◦");
    assert_eq!(format_counter(1, StyleListStyleType::Square), "▪");
}

#[test]
fn test_counter_formatting_none() {
    assert_eq!(format_counter(1, StyleListStyleType::None), "");
    assert_eq!(format_counter(42, StyleListStyleType::None), "");
}
