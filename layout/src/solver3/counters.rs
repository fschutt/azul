//! CSS Counter Support
//!
//! Implements CSS counters for ordered lists and generated content as per CSS spec.
//! Counters are cached per-node in the LayoutCache and computed during layout traversal.

use alloc::string::String;

use azul_css::props::style::lists::StyleListStyleType;

/// Formats a counter value into a string based on the list style type.
///
/// Implements CSS counter styles for various numbering systems.
pub fn format_counter(value: i32, style: StyleListStyleType) -> String {
    match style {
        StyleListStyleType::None => String::new(),
        StyleListStyleType::Disc => "•".to_string(),
        StyleListStyleType::Circle => "◦".to_string(),
        StyleListStyleType::Square => "▪".to_string(),
        StyleListStyleType::Decimal => value.to_string(),
        StyleListStyleType::DecimalLeadingZero => format!("{:02}", value),
        StyleListStyleType::LowerAlpha => to_alphabetic(value as u32, false),
        StyleListStyleType::UpperAlpha => to_alphabetic(value as u32, true),
        StyleListStyleType::LowerRoman => to_roman(value as u32, false),
        StyleListStyleType::UpperRoman => to_roman(value as u32, true),
        StyleListStyleType::LowerGreek => to_greek(value as u32, false),
        StyleListStyleType::UpperGreek => to_greek(value as u32, true),
    }
}

// --- Formatting Helpers ---

/// Converts a number to alphabetic representation (a, b, c, ..., z, aa, ab, ...).
///
/// This implements the CSS `lower-alpha` and `upper-alpha` counter styles.
fn to_alphabetic(mut num: u32, uppercase: bool) -> String {
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
fn to_roman(mut num: u32, uppercase: bool) -> String {
    if num == 0 {
        return "0".to_string();
    }
    if num > 3999 {
        // Roman numerals traditionally don't go beyond 3999
        return num.to_string();
    }

    let values = [
        (1000, "M", "m"),
        (900, "CM", "cm"),
        (500, "D", "d"),
        (400, "CD", "cd"),
        (100, "C", "c"),
        (90, "XC", "xc"),
        (50, "L", "l"),
        (40, "XL", "xl"),
        (10, "X", "x"),
        (9, "IX", "ix"),
        (5, "V", "v"),
        (4, "IV", "iv"),
        (1, "I", "i"),
    ];

    let mut result = String::new();
    for (value, upper, lower) in &values {
        while num >= *value {
            result.push_str(if uppercase { upper } else { lower });
            num -= *value;
        }
    }

    result
}

/// Converts a number to Greek letter representation.
///
/// This implements the CSS `lower-greek` and `upper-greek` counter styles.
/// Supports α, β, γ, ... (24 letters of Greek alphabet).
fn to_greek(num: u32, uppercase: bool) -> String {
    if num == 0 {
        return String::new();
    }

    // Greek lowercase letters α-ω (24 letters, omitting archaic letters)
    let greek_lower = [
        'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ',
        'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
    ];

    let greek_upper = [
        'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ', 'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'Σ',
        'Τ', 'Υ', 'Φ', 'Χ', 'Ψ', 'Ω',
    ];

    let letters = if uppercase {
        &greek_upper
    } else {
        &greek_lower
    };

    if num <= 24 {
        letters[(num - 1) as usize].to_string()
    } else {
        // For numbers > 24, fall back to decimal
        num.to_string()
    }
}
