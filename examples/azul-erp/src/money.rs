//! Amounts in integer minor units (cents): money is never a float here (the
//! ERP README's "money is f64" is one of its bugs; MoneyInput's rule).
//!
//! - [`format_amount`]: `159664` -> `"1,596.64"` (what the tables show).
//! - [`file_amount`] / [`parse_file_amount`]: `"1596.64"` (what the record
//!   files and the CSV export hold: no grouping, a point, two decimals).
//! - [`parse_amount`]: what a user types or a CSV file holds - `1,596.64`,
//!   `1.596,64`, `1596,64`, `EUR 1 596.64`, `-12`, `(12.50)` - into cents.
//!
//! TODO(WIDGETS9B): MoneyInput - the form's amount fields take azul's
//! MoneyInput (locale-aware parsing in the widget) when it lands; the CSV
//! import keeps [`parse_amount`].

/// Minor units per major unit (two decimals: EUR, USD, GBP, CHF, ...).
pub const MINOR_PER_MAJOR: i64 = 100;

/// `cents` with thousands separators and two decimals: `"1,596.64"`,
/// `"-0.50"`, `"0.00"`.
#[must_use]
pub fn format_amount(cents: i64) -> String {
    let _ = cents;
    todo!("GREEN")
}

/// `cents` as the record files and the CSV export write it: `"1596.64"`,
/// `"-0.50"` (no grouping, a point, two decimals).
#[must_use]
pub fn file_amount(cents: i64) -> String {
    let _ = cents;
    todo!("GREEN")
}

/// Reads [`file_amount`]'s form back (also `"12"` and `"12.5"`).
pub fn parse_file_amount(text: &str) -> Result<i64, String> {
    let _ = text;
    todo!("GREEN")
}

/// An amount as people write it, in cents. The decimal separator is the
/// last `.` or `,` when both appear; a single separator followed by exactly
/// three digits groups thousands (`1,596` = 1596.00) unless the part before
/// it is 0; spaces, apostrophes and a currency (`EUR`, `€`, `$`, ...) are
/// ignored; `-12`, `12-` and `(12)` are negative. More than two decimals is
/// refused (no silent rounding of money).
pub fn parse_amount(text: &str) -> Result<i64, String> {
    let _ = text;
    todo!("GREEN")
}

/// `numerator / denominator` rounded half away from zero (the rounding of
/// every money division here).
#[must_use]
pub fn div_round(numerator: i128, denominator: i128) -> i64 {
    let _ = (numerator, denominator);
    todo!("GREEN")
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
        assert!(parse_file_amount("1,596.64").is_err(), "the files never group");
        assert!(parse_file_amount("1.234").is_err(), "three decimals are not money");
    }

    #[test]
    fn typed_amounts_read_in_english_and_german_notation() {
        assert_eq!(parse_amount("1,596.64"), Ok(159_664));
        assert_eq!(parse_amount("1.596,64"), Ok(159_664));
        assert_eq!(parse_amount("1596,64"), Ok(159_664));
        assert_eq!(parse_amount("1596.64"), Ok(159_664));
        assert_eq!(parse_amount("1,596"), Ok(159_600), "three digits after one separator group thousands");
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
