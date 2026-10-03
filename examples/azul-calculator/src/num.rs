//! The decimal numbers of Standard, Scientific and Convert.
//!
//! [`Num`] wraps `bigdecimal::BigDecimal` (already in the tree through
//! turso_core), so `0.1 + 0.2` is `0.3` and `1,280 x 0.19` is `243.2`
//! exactly, as on a paper receipt. Every operation keeps [`WORKING_DIGITS`]
//! significant digits and checks the magnitude (a result beyond
//! 10^[`MAX_EXPONENT`] is an overflow, as on Windows' calculator, rather
//! than a number with a million digits that eats the memory). The
//! transcendental functions go through `f64` and come back rounded to
//! [`F64_DIGITS`] significant digits, which hides the binary noise:
//! `sin(30deg)` is `0.5`, `log(1000)` is `3`.
//!
//! Formatting is done here too: thousands grouping, a significant-digit
//! budget, and scientific notation for numbers too large or too small to
//! show plainly (or always, with F-E).

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use bigdecimal::{BigDecimal, ToPrimitive, Zero};

/// Significant digits every intermediate result keeps.
pub const WORKING_DIGITS: u64 = 50;
/// Significant digits a result shows (Windows' calculator shows 32).
pub const DISPLAY_DIGITS: u64 = 32;
/// Significant digits a result computed through `f64` is trusted with.
pub const F64_DIGITS: u64 = 15;
/// The largest decimal exponent a result may have.
pub const MAX_EXPONENT: i64 = 9999;
/// The largest n whose n! stays under 10^[`MAX_EXPONENT`].
pub const MAX_FACTORIAL: i64 = 3248;

/// Why a calculation has no result. The text is what the display shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcError {
    /// `x / 0`, `x mod 0`, `1/0`.
    DivideByZero,
    /// Outside a function's domain: `sqrt(-1)`, `ln(0)`, `asin(2)`, `(-8)^0.5`.
    InvalidInput,
    /// Beyond 10^9999 (or not finite).
    Overflow,
    /// Not an expression: `2 +`, `)`, `sin`.
    Syntax(String),
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::DivideByZero => write!(f, "Cannot divide by zero"),
            CalcError::InvalidInput => write!(f, "Invalid input"),
            CalcError::Overflow => write!(f, "Overflow"),
            CalcError::Syntax(what) => write!(f, "{what}"),
        }
    }
}

/// A decimal number.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Num(BigDecimal);

/// How a number is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// Significant digits at most.
    pub digits: u64,
    /// `1,234,567.89` instead of `1234567.89`.
    pub grouping: bool,
    /// Always scientific notation (`1.2345e+3`), the F-E key.
    pub scientific: bool,
}

impl Default for Format {
    fn default() -> Self {
        Format {
            digits: DISPLAY_DIGITS,
            grouping: true,
            scientific: false,
        }
    }
}

impl Num {
    /// Zero.
    #[must_use]
    pub fn zero() -> Num {
        Num(BigDecimal::from(0i64))
    }

    /// An integer.
    #[must_use]
    pub fn from_i64(v: i64) -> Num {
        Num(BigDecimal::from(v))
    }

    /// A number as typed or pasted: `1280`, `-0.19`, `.5`, `5.`, `1.5E3`,
    /// `1.5e-3`. Grouping commas and blanks are ignored, `−` (U+2212) is a minus.
    pub fn parse(text: &str) -> Result<Num, CalcError> {
        let mut t: String = text
            .chars()
            .filter(|c| !matches!(c, ',' | ' ' | '_' | '\u{a0}' | '\u{202f}'))
            .map(|c| if c == '\u{2212}' { '-' } else { c })
            .collect();
        if t.is_empty() || t == "-" {
            return Err(CalcError::Syntax(format!("{text:?} is not a number")));
        }
        // `5.` and `5.E3` are 5; `.5` and `-.5` get their leading zero.
        if let Some(pos) = t.find(['e', 'E']) {
            if t[..pos].ends_with('.') {
                t.remove(pos - 1);
            }
        } else if t.ends_with('.') {
            t.pop();
        }
        if t.starts_with('.') {
            t.insert(0, '0');
        } else if t.starts_with("-.") {
            t.insert(1, '0');
        }
        let valid = t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
        if !valid {
            return Err(CalcError::Syntax(format!("{text:?} is not a number")));
        }
        let n = BigDecimal::from_str(&t)
            .map_err(|_| CalcError::Syntax(format!("{text:?} is not a number")))?;
        Num(n).checked()
    }

    /// A number from an `f64` result, trusted with [`F64_DIGITS`] digits.
    pub fn from_f64(v: f64) -> Result<Num, CalcError> {
        if !v.is_finite() {
            return Err(CalcError::Overflow);
        }
        if v == 0.0 {
            return Ok(Num::zero());
        }
        let n = BigDecimal::from_str(&format!("{v:e}")).map_err(|_| CalcError::Overflow)?;
        Ok(Num(n).round_sig(F64_DIGITS).normalized())
    }

    /// The nearest `f64` (for the functions computed through it).
    pub fn to_f64(&self) -> Result<f64, CalcError> {
        match self.0.to_f64() {
            Some(v) if v.is_finite() => Ok(v),
            _ => Err(CalcError::Overflow),
        }
    }

    /// The value as an `i64` if it is an integer that fits.
    #[must_use]
    pub fn to_i64_exact(&self) -> Option<i64> {
        if self.0.is_integer() {
            self.0.to_i64()
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.0 < BigDecimal::from(0i64)
    }

    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.0.is_integer()
    }

    /// The decimal exponent of the leading digit: 0 for 1..9, 2 for 100..999,
    /// -1 for 0.1..0.9; `None` for zero.
    #[must_use]
    pub fn exponent(&self) -> Option<i64> {
        if self.is_zero() {
            return None;
        }
        let (int_val, scale) = self.0.as_bigint_and_exponent();
        let digits = int_val.to_string().trim_start_matches('-').len() as i64;
        Some(digits - 1 - scale)
    }

    /// At most `digits` significant digits (rounded half up); fewer stay as they are.
    #[must_use]
    pub fn round_sig(&self, digits: u64) -> Num {
        if self.0.digits() > digits {
            Num(self.0.with_prec(digits))
        } else {
            self.clone()
        }
    }

    /// Without trailing zeros.
    #[must_use]
    pub fn normalized(&self) -> Num {
        Num(self.0.normalized())
    }

    /// Rounded to the working precision, an overflow refused, an underflow zero.
    pub fn checked(self) -> Result<Num, CalcError> {
        let n = self.round_sig(WORKING_DIGITS);
        match n.exponent() {
            Some(e) if e > MAX_EXPONENT => Err(CalcError::Overflow),
            Some(e) if e < -MAX_EXPONENT => Ok(Num::zero()),
            _ => Ok(n),
        }
    }

    pub fn add(&self, other: &Num) -> Result<Num, CalcError> {
        Num(self.0.clone() + other.0.clone()).checked()
    }

    pub fn sub(&self, other: &Num) -> Result<Num, CalcError> {
        Num(self.0.clone() - other.0.clone()).checked()
    }

    pub fn mul(&self, other: &Num) -> Result<Num, CalcError> {
        if let (Some(a), Some(b)) = (self.exponent(), other.exponent()) {
            if a + b > MAX_EXPONENT + 1 {
                return Err(CalcError::Overflow);
            }
        }
        Num(self.0.clone() * other.0.clone()).checked()
    }

    pub fn div(&self, other: &Num) -> Result<Num, CalcError> {
        if other.is_zero() {
            return Err(CalcError::DivideByZero);
        }
        if let (Some(a), Some(b)) = (self.exponent(), other.exponent()) {
            if a - b > MAX_EXPONENT + 1 {
                return Err(CalcError::Overflow);
            }
        }
        Num(self.0.clone() / other.0.clone()).checked()
    }

    /// The remainder with the sign of the dividend (`-7 mod 3 = -1`), as
    /// Windows' and most calculators' `mod`.
    pub fn rem(&self, other: &Num) -> Result<Num, CalcError> {
        if other.is_zero() {
            return Err(CalcError::DivideByZero);
        }
        Num(self.0.clone() % other.0.clone()).checked()
    }

    #[must_use]
    pub fn neg(&self) -> Num {
        Num(-self.0.clone())
    }

    #[must_use]
    pub fn abs(&self) -> Num {
        Num(self.0.abs())
    }

    /// `x / 100`.
    pub fn percent(&self) -> Result<Num, CalcError> {
        self.div(&Num::from_i64(100))
    }

    pub fn sqrt(&self) -> Result<Num, CalcError> {
        if self.is_negative() {
            return Err(CalcError::InvalidInput);
        }
        match self.0.sqrt() {
            Some(r) => Num(r).checked(),
            None => Err(CalcError::InvalidInput),
        }
    }

    pub fn cbrt(&self) -> Result<Num, CalcError> {
        let root = Num(self.abs().0.cbrt()).checked()?;
        Ok(if self.is_negative() { root.neg() } else { root })
    }

    /// `self ^ exponent`: exact for an integer exponent, through `f64` otherwise.
    pub fn pow(&self, exponent: &Num) -> Result<Num, CalcError> {
        if let Some(n) = exponent.to_i64_exact() {
            if self.is_zero() {
                return match n.cmp(&0) {
                    Ordering::Less => Err(CalcError::DivideByZero),
                    Ordering::Equal => Ok(Num::from_i64(1)),
                    Ordering::Greater => Ok(Num::zero()),
                };
            }
            // bigdecimal's powi keeps 100 significant digits while it squares, so
            // even 2^1000000000 costs a few dozen multiplications; the exponent
            // check of `checked` turns the result into an overflow or zero.
            return Num(self.0.powi(n)).checked();
        }
        let base = self.to_f64()?;
        let exp = exponent.to_f64()?;
        if base < 0.0 {
            // A negative base with an odd-denominator exponent: the real root.
            let recip = 1.0 / exp;
            if (recip.round() - recip).abs() < 1e-9 && (recip.round() as i64) % 2 != 0 {
                return Num::from_f64(-(-base).powf(exp));
            }
            return Err(CalcError::InvalidInput);
        }
        Num::from_f64(base.powf(exp))
    }

    /// n! for an integer 0 <= n <= [`MAX_FACTORIAL`], exactly.
    pub fn factorial(&self) -> Result<Num, CalcError> {
        let n = match self.to_i64_exact() {
            Some(n) if n >= 0 => n,
            _ => return Err(CalcError::InvalidInput),
        };
        if n > MAX_FACTORIAL {
            return Err(CalcError::Overflow);
        }
        let mut acc = BigDecimal::from(1i64);
        for i in 2..=n {
            acc = acc * BigDecimal::from(i);
        }
        Num(acc).checked()
    }

    /// A function computed through `f64`, `None` from `f` = outside its domain.
    pub fn via_f64(&self, f: impl Fn(f64) -> Option<f64>) -> Result<Num, CalcError> {
        match f(self.to_f64()?) {
            Some(v) if v.is_finite() => Num::from_f64(v),
            Some(_) => Err(CalcError::Overflow),
            None => Err(CalcError::InvalidInput),
        }
    }

    /// 10^n for an integer n, exactly.
    pub fn pow10(n: i64) -> Result<Num, CalcError> {
        if n.abs() > MAX_EXPONENT {
            return if n > 0 { Err(CalcError::Overflow) } else { Ok(Num::zero()) };
        }
        Ok(Num(BigDecimal::new(1.into(), -n)))
    }

    /// The plain text of the value (no grouping, scientific only when it is
    /// huge or tiny), with up to `digits` significant digits: what a result
    /// becomes when it is typed on with.
    #[must_use]
    pub fn to_literal(&self) -> String {
        self.format(&Format {
            digits: WORKING_DIGITS,
            grouping: false,
            scientific: false,
        })
    }

    /// The value as shown.
    #[must_use]
    pub fn format(&self, f: &Format) -> String {
        let n = self.round_sig(f.digits.max(1)).normalized();
        let (int_val, scale) = n.0.as_bigint_and_exponent();
        let text = int_val.to_string();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.to_string()),
            None => (false, text),
        };
        if digits == "0" {
            return "0".to_string();
        }
        let exponent = digits.len() as i64 - 1 - scale;
        let sign = if negative { "-" } else { "" };
        let plain_limit = f.digits.min(DISPLAY_DIGITS) as i64;
        if f.scientific || exponent >= plain_limit || exponent < -9 {
            let (lead, rest) = digits.split_at(1);
            let mantissa = if rest.is_empty() {
                lead.to_string()
            } else {
                format!("{lead}.{rest}")
            };
            let esign = if exponent < 0 { '-' } else { '+' };
            return format!("{sign}{mantissa}e{esign}{}", exponent.abs());
        }
        let (int_part, frac_part) = if scale <= 0 {
            (format!("{digits}{}", "0".repeat((-scale) as usize)), String::new())
        } else if (digits.len() as i64) > scale {
            let (a, b) = digits.split_at(digits.len() - scale as usize);
            (a.to_string(), b.to_string())
        } else {
            (
                "0".to_string(),
                format!("{}{digits}", "0".repeat((scale as usize) - digits.len())),
            )
        };
        let int_part = if f.grouping {
            group_thousands(&int_part)
        } else {
            int_part
        };
        if frac_part.is_empty() {
            format!("{sign}{int_part}")
        } else {
            format!("{sign}{int_part}.{frac_part}")
        }
    }
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format(&Format::default()))
    }
}

/// `1234567` -> `1,234,567` (an integer part of ASCII digits).
#[must_use]
pub fn group_thousands(int_part: &str) -> String {
    let len = int_part.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A number as it is being typed, grouped but otherwise verbatim: `1280.50`
/// shows `1,280.50` (the trailing zero stays), `0.` shows `0.`, `-12` `-12`,
/// `1.5E-3` `1.5e-3`.
#[must_use]
pub fn format_typed(text: &str, grouping: bool) -> String {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", text),
    };
    let (mantissa, exponent) = match rest.find(['e', 'E']) {
        Some(p) => (&rest[..p], Some(&rest[p + 1..])),
        None => (rest, None),
    };
    let (int_part, frac) = match mantissa.find('.') {
        Some(p) => (&mantissa[..p], Some(&mantissa[p + 1..])),
        None => (mantissa, None),
    };
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    let mut out = String::from(sign);
    out.push_str(&if grouping {
        group_thousands(int_part)
    } else {
        int_part.to_string()
    });
    if let Some(frac) = frac {
        out.push('.');
        out.push_str(frac);
    }
    if let Some(e) = exponent {
        out.push('e');
        if !e.starts_with(['-', '+']) {
            out.push('+');
        }
        out.push_str(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Num {
        Num::parse(s).unwrap()
    }

    fn show(x: &Num) -> String {
        x.format(&Format::default())
    }

    #[test]
    fn decimal_arithmetic_has_no_binary_artefacts() {
        assert_eq!(show(&n("0.1").add(&n("0.2")).unwrap()), "0.3");
        assert_eq!(show(&n("1280").mul(&n("0.19")).unwrap()), "243.2");
        assert_eq!(show(&n("243.2").add(&n("18.5")).unwrap()), "261.7");
        assert_eq!(show(&n("1").sub(&n("0.9")).unwrap()), "0.1");
        assert_eq!(show(&n("3").mul(&n("1.1")).unwrap()), "3.3");
    }

    #[test]
    fn division_keeps_thirty_two_digits_and_one_third_times_three_is_one() {
        let third = n("1").div(&n("3")).unwrap();
        assert_eq!(show(&third), "0.33333333333333333333333333333333");
        assert_eq!(show(&third.mul(&n("3")).unwrap()), "1");
        assert_eq!(show(&n("2").div(&n("3")).unwrap()), "0.66666666666666666666666666666667");
    }

    #[test]
    fn dividing_by_zero_is_an_error_not_a_panic() {
        assert_eq!(n("5").div(&Num::zero()), Err(CalcError::DivideByZero));
        assert_eq!(n("5").rem(&Num::zero()), Err(CalcError::DivideByZero));
        assert_eq!(CalcError::DivideByZero.to_string(), "Cannot divide by zero");
    }

    #[test]
    fn the_remainder_has_the_sign_of_the_dividend() {
        assert_eq!(show(&n("7").rem(&n("3")).unwrap()), "1");
        assert_eq!(show(&n("-7").rem(&n("3")).unwrap()), "-1");
        assert_eq!(show(&n("7.5").rem(&n("2")).unwrap()), "1.5");
    }

    #[test]
    fn typed_and_pasted_numbers_parse() {
        assert_eq!(n("1,280"), n("1280"));
        assert_eq!(n(".5"), n("0.5"));
        assert_eq!(n("5."), n("5"));
        assert_eq!(n("-.5"), n("-0.5"));
        assert_eq!(show(&n("1.5E3")), "1,500");
        assert_eq!(show(&n("1.5e-3")), "0.0015");
        assert_eq!(n("\u{2212}2"), n("-2"));
        assert!(Num::parse("").is_err());
        assert!(Num::parse("12a").is_err());
        assert!(Num::parse("-").is_err());
    }

    #[test]
    fn results_are_grouped_and_trimmed() {
        assert_eq!(show(&n("1234567.890")), "1,234,567.89");
        assert_eq!(show(&n("-1234")), "-1,234");
        assert_eq!(show(&n("100")), "100");
        assert_eq!(show(&n("1000")), "1,000");
        assert_eq!(show(&n("0.000123")), "0.000123");
        let plain = Format {
            grouping: false,
            ..Format::default()
        };
        assert_eq!(n("1234567.5").format(&plain), "1234567.5");
    }

    #[test]
    fn huge_and_tiny_results_switch_to_scientific_notation() {
        assert_eq!(show(&n("1e40")), "1e+40");
        assert_eq!(show(&n("-1.5e40")), "-1.5e+40");
        assert_eq!(show(&n("1.25e-12")), "1.25e-12");
        let fe = Format {
            scientific: true,
            ..Format::default()
        };
        assert_eq!(n("1234.5").format(&fe), "1.2345e+3");
        assert_eq!(n("0.5").format(&fe), "5e-1");
        assert_eq!(Num::zero().format(&fe), "0");
    }

    #[test]
    fn a_result_beyond_ten_to_the_9999_is_an_overflow() {
        let big = n("1e9999");
        assert_eq!(big.mul(&n("10")), Err(CalcError::Overflow));
        assert_eq!(n("10").pow(&n("10000")), Err(CalcError::Overflow));
        assert!(n("10").pow(&n("9999")).is_ok());
        assert_eq!(n("2").pow(&n("1000000000")), Err(CalcError::Overflow));
        assert_eq!(show(&n("2").pow(&n("-1000000000")).unwrap()), "0", "underflow is zero");
        assert_eq!(n("1e-9999").div(&n("1e9999")).unwrap(), Num::zero());
    }

    #[test]
    fn integer_powers_are_exact_and_fractional_ones_go_through_f64() {
        assert_eq!(show(&n("2").pow(&n("10")).unwrap()), "1,024");
        assert_eq!(show(&n("1.1").pow(&n("2")).unwrap()), "1.21");
        assert_eq!(show(&n("2").pow(&n("-2")).unwrap()), "0.25");
        assert_eq!(show(&n("9").pow(&n("0.5")).unwrap()), "3");
        assert_eq!(show(&n("-8").pow(&n("0.3333333333333333")).unwrap()), "-2");
        assert_eq!(n("-8").pow(&n("0.5")), Err(CalcError::InvalidInput));
        assert_eq!(Num::zero().pow(&n("-1")), Err(CalcError::DivideByZero));
        assert_eq!(show(&Num::zero().pow(&Num::zero()).unwrap()), "1");
    }

    #[test]
    fn roots_and_factorials() {
        assert_eq!(show(&n("2").sqrt().unwrap()), "1.4142135623730950488016887242097");
        assert_eq!(show(&n("144").sqrt().unwrap()), "12");
        assert_eq!(n("-1").sqrt(), Err(CalcError::InvalidInput));
        assert_eq!(show(&n("27").cbrt().unwrap()), "3");
        assert_eq!(show(&n("-27").cbrt().unwrap()), "-3");
        assert_eq!(show(&n("5").factorial().unwrap()), "120");
        assert_eq!(show(&Num::zero().factorial().unwrap()), "1");
        assert_eq!(n("2.5").factorial(), Err(CalcError::InvalidInput));
        assert_eq!(n("-1").factorial(), Err(CalcError::InvalidInput));
        assert_eq!(n("3249").factorial(), Err(CalcError::Overflow));
        assert!(n("3248").factorial().is_ok());
    }

    #[test]
    fn f64_results_are_rounded_to_fifteen_digits() {
        assert_eq!(show(&Num::from_f64(0.1 + 0.2).unwrap()), "0.3");
        assert_eq!(show(&Num::from_f64((30f64).to_radians().sin()).unwrap()), "0.5");
        assert_eq!(show(&Num::from_f64(1000f64.log10()).unwrap()), "3");
        assert_eq!(Num::from_f64(f64::INFINITY), Err(CalcError::Overflow));
        assert_eq!(
            n("-1").via_f64(|x| (x > 0.0).then(|| x.ln())),
            Err(CalcError::InvalidInput)
        );
    }

    #[test]
    fn a_number_being_typed_keeps_its_trailing_zeros_and_point() {
        assert_eq!(format_typed("1280.50", true), "1,280.50");
        assert_eq!(format_typed("0.", true), "0.");
        assert_eq!(format_typed("-1234", true), "-1,234");
        assert_eq!(format_typed("1234", false), "1234");
        assert_eq!(format_typed("1.5E3", true), "1.5e+3");
        assert_eq!(format_typed("1.5E-", true), "1.5e-");
        assert_eq!(format_typed(".5", true), "0.5");
    }

    #[test]
    fn a_result_typed_on_with_is_its_plain_text() {
        assert_eq!(n("1234.5").to_literal(), "1234.5");
        assert_eq!(n("1e40").to_literal(), "1e+40");
        assert_eq!(Num::parse(&n("1e40").to_literal()).unwrap(), n("1e40"));
        assert_eq!(n("-0.25").to_literal(), "-0.25");
    }

    #[test]
    fn the_exponent_is_the_position_of_the_leading_digit() {
        assert_eq!(n("5").exponent(), Some(0));
        assert_eq!(n("123.4").exponent(), Some(2));
        assert_eq!(n("0.05").exponent(), Some(-2));
        assert_eq!(Num::zero().exponent(), None);
        assert_eq!(show(&Num::pow10(3).unwrap()), "1,000");
        assert_eq!(show(&Num::pow10(-2).unwrap()), "0.01");
    }
}
