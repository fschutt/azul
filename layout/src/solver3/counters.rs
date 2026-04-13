//! CSS Counter Support
//!
//! Implements CSS counters for ordered lists and generated content as per CSS spec.
//! Counters are cached per-node in the LayoutCache and computed during layout traversal.

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
        StyleListStyleType::DecimalLeadingZero => format!("{:02}", value),
        StyleListStyleType::LowerAlpha
        | StyleListStyleType::UpperAlpha
        | StyleListStyleType::LowerRoman
        | StyleListStyleType::UpperRoman
        | StyleListStyleType::LowerGreek
        | StyleListStyleType::UpperGreek => {
            let uppercase = matches!(
                style,
                StyleListStyleType::UpperAlpha
                    | StyleListStyleType::UpperRoman
                    | StyleListStyleType::UpperGreek
            );
            if value < 0 {
                let abs_val = (value as i64).unsigned_abs() as usize;
                let formatted = match style {
                    StyleListStyleType::LowerAlpha | StyleListStyleType::UpperAlpha => {
                        to_alphabetic(abs_val, uppercase)
                    }
                    StyleListStyleType::LowerRoman | StyleListStyleType::UpperRoman => {
                        to_roman(abs_val, uppercase)
                    }
                    _ => to_greek(abs_val, uppercase),
                };
                format!("-{}", formatted)
            } else {
                let n = value as usize;
                match style {
                    StyleListStyleType::LowerAlpha | StyleListStyleType::UpperAlpha => {
                        to_alphabetic(n, uppercase)
                    }
                    StyleListStyleType::LowerRoman | StyleListStyleType::UpperRoman => {
                        to_roman(n, uppercase)
                    }
                    _ => to_greek(n, uppercase),
                }
            }
        }
    }
}

// --- Formatting Helpers ---

/// Converts a number to alphabetic representation (a, b, c, ..., z, aa, ab, ...).
///
/// This implements the CSS `lower-alpha` and `upper-alpha` counter styles.
pub(crate) fn to_alphabetic(mut num: usize, uppercase: bool) -> String {
    if num == 0 {
        return "0".to_string();
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
        return "0".to_string();
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
