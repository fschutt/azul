//! Amounts in integer minor units (cents): money is never a float here (the
//! ERP README's "money is f64" is one of its bugs; MoneyInput's rule).
//!
//! - [`format_amount`]: `159664` -> `"1,596.64"` (what the tables show:
//!   azul's `MoneyInput::format_amount` in [`currency`], no symbol).
//! - [`file_amount`] / [`parse_file_amount`]: `"1596.64"` (what the record
//!   files and the CSV export hold: no grouping, a point, two decimals).
//! - [`parse_amount`]: what a user types or a CSV file holds - `1,596.64`,
//!   `1.596,64`, `1596,64`, `EUR 1 596.64`, `-12`, `(12.50)` - into cents.
//!
//! The form's amount fields are azul's `MoneyInput` in [`currency`] (the
//! widget parses what is typed); the CSV import keeps [`parse_amount`].

use azul::widgets::{MoneyCurrency, MoneyInput, MoneyLocale};

/// Minor units per major unit (two decimals: EUR, USD, GBP, CHF, ...).
pub const MINOR_PER_MAJOR: i64 = 100;

/// The register's money for azul's `MoneyInput`: two decimals
/// ([`MINOR_PER_MAJOR`]), no code and no symbol (the register keeps no
/// currency; the tables show bare amounts).
#[must_use]
pub fn currency() -> MoneyCurrency {
    MoneyCurrency::create("", "", 2)
}

/// How amounts are written: `1,596.64` (a point, a comma between thousands).
#[must_use]
pub fn locale() -> MoneyLocale {
    MoneyLocale::en_us()
}

/// `cents` with thousands separators and two decimals: `"1,596.64"`,
/// `"-0.50"`, `"0.00"` (azul's `MoneyInput::format_amount`).
#[must_use]
pub fn format_amount(cents: i64) -> String {
    MoneyInput::format_amount(cents, currency(), locale())
        .as_str()
        .to_string()
}

/// `cents` as the record files and the CSV export write it: `"1596.64"`,
/// `"-0.50"` (no grouping, a point, two decimals).
#[must_use]
pub fn file_amount(cents: i64) -> String {
    let (major, minor) = split(cents);
    let sign = if cents < 0 { "-" } else { "" };
    format!("{sign}{major}.{minor:02}")
}

/// The major and minor units of `cents`, without the sign.
fn split(cents: i64) -> (u64, u64) {
    let per = MINOR_PER_MAJOR.unsigned_abs();
    let abs = cents.unsigned_abs();
    (abs / per, abs % per)
}

/// Reads [`file_amount`]'s form back (also `"12"` and `"12.5"`).
pub fn parse_file_amount(text: &str) -> Result<i64, String> {
    let t = text.trim();
    let bad = || format!("\"{t}\" is not an amount (like 1596.64)");
    let (negative, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !digits(int) || !digits(frac) || frac.len() > 2 {
        return Err(bad());
    }
    compose(int, frac, negative).ok_or_else(bad)
}

/// `int` major units and `frac` (0 to 2 digits) minor units, negated when
/// asked; `None` when it does not fit.
fn compose(int: &str, frac: &str, negative: bool) -> Option<i64> {
    let major: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let minor: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        2 => frac.parse().ok()?,
        _ => return None,
    };
    let cents = major.checked_mul(MINOR_PER_MAJOR)?.checked_add(minor)?;
    Some(if negative { -cents } else { cents })
}

/// The symbols a currency is written with before or after an amount.
fn is_currency_symbol(c: char) -> bool {
    matches!(
        c,
        '\u{20ac}' | '$' | '\u{a3}' | '\u{a5}' | '\u{20b9}' | '\u{20a3}' | '\u{20bd}' | '\u{20a9}'
    )
}

/// Whether `c` is a currency's letter or symbol (stripped at either end).
fn is_currency(c: char) -> bool {
    c.is_alphabetic() || is_currency_symbol(c)
}

/// The digits of an integer part grouped by `.` or `,` (`1.234.567`): the
/// first group 1 to 3 digits, every other exactly 3. `None` otherwise.
fn ungroup(int: &str) -> Option<String> {
    let groups: Vec<&str> = int.split(['.', ',']).collect();
    if groups.len() == 1 {
        return Some(int.to_string());
    }
    let first_ok = (1..=3).contains(&groups[0].len());
    let rest_ok = groups[1..].iter().all(|g| g.len() == 3);
    (first_ok && rest_ok).then(|| groups.concat())
}

/// An amount as people write it, in cents. The decimal separator is the
/// last `.` or `,` when both appear; a single separator followed by exactly
/// three digits groups thousands (`1,596` = 1596.00) unless the part before
/// it is 0; spaces, apostrophes and a currency (`EUR`, `€`, `$`, ...) are
/// ignored; `-12`, `12-` and `(12)` are negative. More than two decimals is
/// refused (no silent rounding of money).
pub fn parse_amount(text: &str) -> Result<i64, String> {
    let shown = text.trim();
    let bad = |why: &str| format!("\"{shown}\" is not an amount: {why}");
    // Spaces and apostrophes group thousands (`1 596`, `1'596`): dropped.
    let mut body: String = shown
        .chars()
        .filter(|c| !(c.is_whitespace() || matches!(c, '\'' | '\u{2019}')))
        .collect();
    let mut negative = false;
    if body.len() >= 2 && body.starts_with('(') && body.ends_with(')') {
        negative = true;
        body = body[1..body.len() - 1].to_string();
    }
    let trimmed = body.trim_matches(is_currency);
    let (minus, rest) = if let Some(r) = trimmed.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = trimmed.strip_suffix('-') {
        (true, r)
    } else {
        (false, trimmed)
    };
    if minus && negative {
        return Err(bad("two minus signs"));
    }
    negative |= minus;
    let rest = rest.trim_matches(is_currency);
    if rest.is_empty() {
        return Err(bad("no digits"));
    }
    if !rest
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return Err(bad(
            "only digits, one decimal separator and thousands separators",
        ));
    }
    let dots = rest.matches('.').count();
    let commas = rest.matches(',').count();
    let decimal: Option<char> = if dots > 0 && commas > 0 {
        let last = rest.rfind(['.', ',']).unwrap_or(0);
        let sep = if rest[last..].starts_with('.') {
            '.'
        } else {
            ','
        };
        if rest.matches(sep).count() != 1 {
            return Err(bad("two decimal separators"));
        }
        Some(sep)
    } else if dots + commas == 1 {
        let sep = if dots == 1 { '.' } else { ',' };
        let (int, frac) = rest.split_once(sep).unwrap_or((rest, ""));
        let int_is_zero = int.chars().all(|c| c == '0');
        if frac.len() == 3 && !int_is_zero {
            None // `1,596`: a thousands separator
        } else {
            Some(sep)
        }
    } else {
        None
    };
    let (int, frac) = match decimal {
        Some(sep) => {
            let at = rest.rfind(sep).unwrap_or(rest.len());
            (&rest[..at], &rest[at + 1..])
        }
        None => (rest, ""),
    };
    if frac.len() > 2 {
        return Err(bad("more than two decimals"));
    }
    if int.is_empty() && frac.is_empty() {
        return Err(bad("no digits"));
    }
    let digits = ungroup(int).ok_or_else(|| bad("the thousands are not grouped by three"))?;
    compose(&digits, frac, negative).ok_or_else(|| bad("too large"))
}

/// `numerator / denominator` rounded half away from zero (the rounding of
/// every money division here).
#[must_use]
pub fn div_round(numerator: i128, denominator: i128) -> i64 {
    if denominator == 0 {
        return 0;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let away = (numerator < 0) != (denominator < 0);
    let step = if 2 * remainder.abs() >= denominator.abs() {
        if away {
            -1
        } else {
            1
        }
    } else {
        0
    };
    i64::try_from(quotient + step).unwrap_or(if away { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_show_with_thousands_separators_and_two_decimals() {
        assert_eq!(format_amount(159_664), "1,596.64");
        assert_eq!(format_amount(0), "0.00");
        assert_eq!(format_amount(5), "0.05");
        assert_eq!(format_amount(-50), "-0.50");
        assert_eq!(format_amount(123_456_789_01), "123,456,789.01");
        assert_eq!(format_amount(-100_000_00), "-100,000.00");
    }

    #[test]
    fn the_files_hold_plain_decimal_amounts_that_read_back_exactly() {
        for cents in [0, 1, 99, 100, 159_664, -50, -123_456, i64::from(i32::MAX)] {
            let text = file_amount(cents);
            assert!(!text.contains(','), "{text}");
            assert_eq!(parse_file_amount(&text), Ok(cents), "{text}");
        }
        assert_eq!(file_amount(159_664), "1596.64");
        assert_eq!(file_amount(-5), "-0.05");
        assert_eq!(parse_file_amount("12"), Ok(1200));
        assert_eq!(parse_file_amount("12.5"), Ok(1250));
        assert!(
            parse_file_amount("1,596.64").is_err(),
            "the files never group"
        );
        assert!(
            parse_file_amount("1.234").is_err(),
            "three decimals are not money"
        );
    }

    #[test]
    fn typed_amounts_read_in_english_and_german_notation() {
        assert_eq!(parse_amount("1,596.64"), Ok(159_664));
        assert_eq!(parse_amount("1.596,64"), Ok(159_664));
        assert_eq!(parse_amount("1596,64"), Ok(159_664));
        assert_eq!(parse_amount("1596.64"), Ok(159_664));
        assert_eq!(
            parse_amount("1,596"),
            Ok(159_600),
            "three digits after one separator group thousands"
        );
        assert_eq!(parse_amount("1.596"), Ok(159_600));
        assert_eq!(parse_amount("1.234.567,8"), Ok(123_456_780));
        assert_eq!(parse_amount("0,5"), Ok(50));
        assert_eq!(parse_amount("  12 "), Ok(1200));
        assert_eq!(parse_amount("EUR 1 596.64"), Ok(159_664));
        assert_eq!(parse_amount("1'596.64 €"), Ok(159_664));
        assert_eq!(parse_amount("$12"), Ok(1200));
        assert_eq!(parse_amount("-12.50"), Ok(-1250));
        assert_eq!(parse_amount("12.50-"), Ok(-1250));
        assert_eq!(parse_amount("(12.50)"), Ok(-1250));
    }

    #[test]
    fn typed_amounts_that_are_not_money_are_refused_with_a_reason() {
        for bad in ["", "abc", "1.2.3,4,5", "0.125", "12.345,678", "--1", "1-2"] {
            let r = parse_amount(bad);
            assert!(r.is_err(), "{bad:?} gave {r:?}");
            assert!(!r.unwrap_err().is_empty());
        }
    }

    #[test]
    fn money_divisions_round_half_away_from_zero() {
        assert_eq!(div_round(159_664, 3), 53_221);
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(0, 7), 0);
        assert_eq!(div_round(7, 0), 0, "nothing to divide by: nothing");
    }
}
