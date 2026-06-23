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
        StyleListStyleType::LowerAlpha => with_sign(value, |n| to_alphabetic(n, false)),
        StyleListStyleType::UpperAlpha => with_sign(value, |n| to_alphabetic(n, true)),
        StyleListStyleType::LowerRoman => with_sign(value, |n| to_roman(n, false)),
        StyleListStyleType::UpperRoman => with_sign(value, |n| to_roman(n, true)),
        StyleListStyleType::LowerGreek => with_sign(value, |n| to_greek(n, false)),
        StyleListStyleType::UpperGreek => with_sign(value, |n| to_greek(n, true)),
    }
}

// --- Formatting Helpers ---

/// Formats the magnitude of `value`, prefixing `-` for negatives.
///
/// Avoids the lossy `value as u32` cast: a negative counter such as `-3` in
/// `lower-roman` formats as `-iii` instead of wrapping to a huge unsigned value.
fn with_sign<F: Fn(usize) -> String>(value: i32, format: F) -> String {
    if value < 0 {
        let magnitude = (value as i64).unsigned_abs() as usize;
        format!("-{}", format(magnitude))
    } else {
        format(value as usize)
    }
}

/// Converts a number to alphabetic representation (a, b, c, ..., z, aa, ab, ...).
///
/// This implements the CSS `lower-alpha` and `upper-alpha` counter styles.
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
    if num == 0 {
        return "0".to_string();
    }
    const MAX_ROMAN: usize = 3999;
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
