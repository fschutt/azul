//! Hash-able floating-point wrappers, percentage values, and CSS size
//! metric types used by the CSS property system.

use core::fmt;
use std::num::ParseFloatError;

use crate::corety::AzString;

/// Multiplier for floating point accuracy.
///
/// Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 3 digits, i.e
/// `1.001 == 1.0`
pub const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
const FP_PRECISION_MULTIPLIER_CONST: isize = crate::cast::f32_to_isize(FP_PRECISION_MULTIPLIER);

/// Wrapper around `FloatValue`, represents a percentage instead
/// of just being a regular floating-point value, i.e `5` = `5%`
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PercentageValue {
    number: FloatValue,
}

impl_option!(
    PercentageValue,
    OptionPercentageValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for PercentageValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.normalized() * 100.0)
    }
}

impl PercentageValue {
    /// Same as `PercentageValue::new()`, but only accepts whole numbers.
    /// Uses isize arithmetic to avoid floating-point in const context.
    #[inline]
    #[must_use] pub const fn const_new(value: isize) -> Self {
        Self {
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a `PercentageValue` from a fractional number in const context.
    ///
    /// # Arguments
    /// * `pre_comma` - The integer part (e.g., 100 for 100.5%)
    /// * `post_comma` - The fractional part as digits (e.g., 5 for 0.5%)
    ///
    /// # Examples
    /// ```
    /// // 100% = const_new_fractional(100, 0)
    /// // 50.5% = const_new_fractional(50, 5)
    /// ```
    #[inline]
    #[must_use] pub const fn const_new_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self {
            number: FloatValue::const_new_fractional(pre_comma, post_comma),
        }
    }

    #[inline]
    #[must_use] pub fn new(value: f32) -> Self {
        Self {
            number: value.into(),
        }
    }

    // NOTE: no get() function, to avoid confusion with "150%"

    #[inline]
    #[must_use] pub fn normalized(&self) -> f32 {
        self.number.get() / 100.0
    }

    #[inline]
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            number: self.number.interpolate(&other.number, t),
        }
    }
}

/// Wrapper around an f32 value that is internally casted to an isize,
/// in order to provide hash-ability (to avoid numerical instability).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FloatValue {
    pub(crate) number: isize,
}

impl fmt::Display for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl ::core::fmt::Debug for FloatValue {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "{self}")
    }
}

impl Default for FloatValue {
    fn default() -> Self {
        const DEFAULT_FLV: FloatValue = FloatValue::const_new(0);
        DEFAULT_FLV
    }
}

impl FloatValue {
    /// Same as `FloatValue::new()`, but only accepts whole numbers.
    /// Uses isize arithmetic to avoid floating-point in const context.
    #[inline]
    #[must_use] pub const fn const_new(value: isize) -> Self {
        Self {
            number: value * FP_PRECISION_MULTIPLIER_CONST,
        }
    }

    /// Creates a `FloatValue` from a fractional number in const context.
    ///
    /// This uses integer arithmetic to represent fractional values like 1.5, 0.83, etc.
    /// in const context without relying on f32 operations.
    ///
    /// The function automatically detects the number of decimal places in `post_comma`
    /// and supports up to 3 decimal places. If more digits are provided, only the first
    /// 3 are used (truncation, not rounding).
    ///
    /// # Arguments
    /// * `pre_comma` - The integer part (e.g., 1 for 1.5)
    /// * `post_comma` - The fractional part as digits (e.g., 5 for 0.5, 52 for 0.52, 523 for 0.523)
    ///
    /// # Examples
    /// ```
    /// // 1.5 = const_new_fractional(1, 5)
    /// // 1.52 = const_new_fractional(1, 52)
    /// // 1.523 = const_new_fractional(1, 523)
    /// // 0.83 = const_new_fractional(0, 83)
    /// // 1.17 = const_new_fractional(1, 17)
    /// // 2.123456 -> 2.123 (truncated to 3 decimal places)
    /// ```
    #[inline]
    #[must_use] pub const fn const_new_fractional(pre_comma: isize, post_comma: isize) -> Self {
        // Get absolute value for digit counting
        let abs_post = if post_comma < 0 {
            -post_comma
        } else {
            post_comma
        };

        // Determine the number of digits and extract only the first 3
        // Note: We limit to values that fit in 32-bit isize for WASM compatibility
        let (normalized_post, divisor) = if abs_post < 10 {
            // 1 digit: 5 → 0.5
            (abs_post, 10)
        } else if abs_post < 100 {
            // 2 digits: 83 → 0.83
            (abs_post, 100)
        } else if abs_post < 1000 {
            // 3 digits: 523 → 0.523
            (abs_post, 1000)
        } else {
            // 4+ digits: keep only the first 3 (e.g. 5234 → 523 → 0.523).
            // A fixed division ladder cannot bound the digit count for
            // arbitrarily large `post_comma` (an 11-digit value keeps 4 digits,
            // etc.), letting the "fraction" grow past 1.0 and corrupt the
            // integer part. Reduce until strictly below 1000 so the result is
            // always a proper 3-digit fraction.
            let mut reduced = abs_post;
            while reduced >= 1000 {
                reduced /= 10;
            }
            (reduced, 1000)
        };

        // Calculate fractional part
        let fractional_part = normalized_post * (FP_PRECISION_MULTIPLIER_CONST / divisor);

        // Apply sign: if post_comma is negative, negate the fractional part
        let signed_fractional = if post_comma < 0 {
            -fractional_part
        } else {
            fractional_part
        };

        // For negative pre_comma, the fractional part should also be negative
        // E.g., -1.5 = -1 + (-0.5), not -1 + 0.5
        let final_fractional = if pre_comma < 0 && post_comma >= 0 {
            -signed_fractional
        } else {
            signed_fractional
        };

        Self {
            number: pre_comma * FP_PRECISION_MULTIPLIER_CONST + final_fractional,
        }
    }

    #[inline]
    #[must_use] pub fn new(value: f32) -> Self {
        Self {
            number: crate::cast::f32_to_isize(value * FP_PRECISION_MULTIPLIER),
        }
    }

    #[inline]
    #[must_use] pub fn get(&self) -> f32 {
        crate::cast::isize_to_f32(self.number) / FP_PRECISION_MULTIPLIER
    }

    /// Returns the raw encoded `isize` (the f32 value scaled by
    /// `FP_PRECISION_MULTIPLIER`). Exposed so external callers can
    /// round-trip the value through the compact-cache encoding without
    /// re-multiplying through f32.
    #[inline]
    #[must_use] pub const fn number(&self) -> isize {
        self.number
    }

    #[inline]
    #[allow(clippy::suboptimal_flops)] // explicit FP; mul_add slower without +fma
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        let self_val_f32 = self.get();
        let other_val_f32 = other.get();
        let interpolated = self_val_f32 + ((other_val_f32 - self_val_f32) * t);
        Self::new(interpolated)
    }
}

impl From<f32> for FloatValue {
    #[inline]
    fn from(val: f32) -> Self {
        Self::new(val)
    }
}

/// Enum representing the metric associated with a number (px, pt, em, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum SizeMetric {
    #[default]
    Px,
    Pt,
    Em,
    Rem,
    In,
    Cm,
    Mm,
    Percent,
    /// Viewport width: 1vw = 1% of viewport width
    Vw,
    /// Viewport height: 1vh = 1% of viewport height
    Vh,
    /// Viewport minimum: 1vmin = 1% of smaller viewport dimension
    Vmin,
    /// Viewport maximum: 1vmax = 1% of larger viewport dimension
    Vmax,
}


impl fmt::Display for SizeMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::SizeMetric::{Px, Pt, Em, Rem, In, Cm, Mm, Percent, Vw, Vh, Vmin, Vmax};
        match self {
            Px => write!(f, "px"),
            Pt => write!(f, "pt"),
            Em => write!(f, "em"),
            Rem => write!(f, "rem"),
            In => write!(f, "in"),
            Cm => write!(f, "cm"),
            Mm => write!(f, "mm"),
            Percent => write!(f, "%"),
            Vw => write!(f, "vw"),
            Vh => write!(f, "vh"),
            Vmin => write!(f, "vmin"),
            Vmax => write!(f, "vmax"),
        }
    }
}

/// # Errors
///
/// Returns an error if `input` is not a valid CSS `float-value` value.
pub fn parse_float_value(input: &str) -> Result<FloatValue, ParseFloatError> {
    Ok(FloatValue::new(input.trim().parse::<f32>()?))
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum PercentageParseError {
    ValueParseErr(crate::props::basic::error::ParseFloatError),
    NoPercentSign,
    InvalidUnit(AzString),
}

impl_debug_as_display!(PercentageParseError);

impl From<ParseFloatError> for PercentageParseError {
    fn from(e: ParseFloatError) -> Self {
        Self::ValueParseErr(crate::props::basic::error::ParseFloatError::from(e))
    }
}

impl_display! { PercentageParseError, {
    ValueParseErr(e) => format!("\"{}\"", e),
    NoPercentSign => format!("No percent sign after number"),
    InvalidUnit(u) => format!("Error parsing percentage: invalid unit \"{}\"", u.as_str()),
}}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum PercentageParseErrorOwned {
    ValueParseErr(crate::props::basic::error::ParseFloatError),
    NoPercentSign,
    InvalidUnit(AzString),
}

impl PercentageParseError {
    #[must_use] pub fn to_contained(&self) -> PercentageParseErrorOwned {
        match self {
            Self::ValueParseErr(e) => PercentageParseErrorOwned::ValueParseErr(*e),
            Self::NoPercentSign => PercentageParseErrorOwned::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseErrorOwned::InvalidUnit(u.clone()),
        }
    }
}

impl PercentageParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> PercentageParseError {
        match self {
            Self::ValueParseErr(e) => PercentageParseError::ValueParseErr(*e),
            Self::NoPercentSign => PercentageParseError::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseError::InvalidUnit(u.clone()),
        }
    }
}

/// Parse "1.2" or "120%" (similar to `parse_pixel_value`)
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `percentage-value` value.
pub fn parse_percentage_value(input: &str) -> Result<PercentageValue, PercentageParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(PercentageParseError::ValueParseErr(
            crate::props::basic::error::ParseFloatError::from("empty string".parse::<f32>().unwrap_err()),
        ));
    }

    let mut split_pos = 0;
    let mut found_numeric = false;
    for (idx, ch) in input.char_indices() {
        if ch.is_numeric() || ch == '.' || ch == '-' {
            // Advance past the *whole* char: `is_numeric()` matches multi-byte
            // Unicode digits (½ U+00BD, ٥ U+0665, ５ U+FF15). Using `idx + 1`
            // would land inside the codepoint and panic on the slice below.
            split_pos = idx + ch.len_utf8();
            found_numeric = true;
        }
    }

    if !found_numeric {
        return Err(PercentageParseError::ValueParseErr(
            crate::props::basic::error::ParseFloatError::from("no numeric value".parse::<f32>().unwrap_err()),
        ));
    }

    let unit = input[split_pos..].trim();
    let mut number = input[..split_pos]
        .trim()
        .parse::<f32>()
        .map_err(|e| PercentageParseError::ValueParseErr(crate::props::basic::error::ParseFloatError::from(e)))?;

    match unit {
        "" => {
            number *= 100.0;
        } // 0.5 => 50%
        "%" => {} // 50% => PercentageValue(50.0)
        other => {
            return Err(PercentageParseError::InvalidUnit(other.to_string().into()));
        }
    }

    Ok(PercentageValue::new(number))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn test_parse_float_value() {
        assert_eq!(parse_float_value("10").unwrap().get(), 10.0);
        assert_eq!(parse_float_value("2.5").unwrap().get(), 2.5);
        assert_eq!(parse_float_value("-50.2").unwrap().get(), -50.2);
        assert_eq!(parse_float_value("  0  ").unwrap().get(), 0.0);
        assert!(parse_float_value("10a").is_err());
        assert!(parse_float_value("").is_err());
    }

    #[test]
    fn test_parse_percentage_value() {
        // With percent sign
        assert_eq!(parse_percentage_value("50%").unwrap().normalized(), 0.5);
        assert_eq!(parse_percentage_value("120%").unwrap().normalized(), 1.2);
        assert_eq!(parse_percentage_value("-25%").unwrap().normalized(), -0.25);
        assert_eq!(
            parse_percentage_value("  75.5%  ").unwrap().normalized(),
            0.755
        );

        // As a ratio
        assert!((parse_percentage_value("0.5").unwrap().normalized() - 0.5).abs() < 1e-6);
        assert!((parse_percentage_value("1.2").unwrap().normalized() - 1.2).abs() < 1e-6);
        assert!((parse_percentage_value("1").unwrap().normalized() - 1.0).abs() < 1e-6);

        // Errors
        assert!(matches!(
            parse_percentage_value("50px").err().unwrap(),
            PercentageParseError::InvalidUnit(_)
        ));
        assert!(parse_percentage_value("fifty%").is_err());
        assert!(parse_percentage_value("").is_err());
    }

    #[test]
    fn test_const_new_fractional_single_digit() {
        // Single digit post_comma (1 decimal place)
        let val = FloatValue::const_new_fractional(1, 5);
        assert_eq!(val.get(), 1.5);

        let val = FloatValue::const_new_fractional(0, 5);
        assert_eq!(val.get(), 0.5);

        let val = FloatValue::const_new_fractional(2, 3);
        assert_eq!(val.get(), 2.3);

        let val = FloatValue::const_new_fractional(0, 0);
        assert_eq!(val.get(), 0.0);

        let val = FloatValue::const_new_fractional(10, 9);
        assert_eq!(val.get(), 10.9);
    }

    #[test]
    fn test_const_new_fractional_two_digits() {
        // Two digits post_comma (2 decimal places)
        let val = FloatValue::const_new_fractional(0, 83);
        assert!((val.get() - 0.83).abs() < 0.001);

        let val = FloatValue::const_new_fractional(1, 17);
        assert!((val.get() - 1.17).abs() < 0.001);

        let val = FloatValue::const_new_fractional(1, 52);
        assert!((val.get() - 1.52).abs() < 0.001);

        let val = FloatValue::const_new_fractional(0, 33);
        assert!((val.get() - 0.33).abs() < 0.001);

        let val = FloatValue::const_new_fractional(2, 67);
        assert!((val.get() - 2.67).abs() < 0.001);

        let val = FloatValue::const_new_fractional(0, 10);
        assert!((val.get() - 0.10).abs() < 0.001);

        let val = FloatValue::const_new_fractional(0, 99);
        assert!((val.get() - 0.99).abs() < 0.001);
    }

    #[test]
    fn test_const_new_fractional_three_digits() {
        // Three digits post_comma (3 decimal places)
        let val = FloatValue::const_new_fractional(1, 523);
        assert!((val.get() - 1.523).abs() < 0.001);

        let val = FloatValue::const_new_fractional(0, 123);
        assert!((val.get() - 0.123).abs() < 0.001);

        let val = FloatValue::const_new_fractional(2, 999);
        assert!((val.get() - 2.999).abs() < 0.001);

        let val = FloatValue::const_new_fractional(0, 100);
        assert!((val.get() - 0.100).abs() < 0.001);

        let val = FloatValue::const_new_fractional(5, 1);
        assert!((val.get() - 5.1).abs() < 0.001);
    }

    #[test]
    fn test_const_new_fractional_truncation() {
        // More than 3 digits should be truncated (not rounded)

        // 4 digits: 5234 → 523 → 0.523
        let val = FloatValue::const_new_fractional(0, 5234);
        assert!((val.get() - 0.523).abs() < 0.001);

        // 5 digits: 12345 → 123 → 0.123
        let val = FloatValue::const_new_fractional(1, 12345);
        assert!((val.get() - 1.123).abs() < 0.001);

        // 6 digits: 123456 → 123 → 1.123
        let val = FloatValue::const_new_fractional(1, 123_456);
        assert!((val.get() - 1.123).abs() < 0.001);

        // 7 digits: 9876543 → 987 → 0.987
        let val = FloatValue::const_new_fractional(0, 9_876_543);
        assert!((val.get() - 0.987).abs() < 0.001);

        // 10 digits
        let val = FloatValue::const_new_fractional(2, 1_234_567_890);
        assert!((val.get() - 2.123).abs() < 0.001);
    }

    #[test]
    fn test_const_new_fractional_negative() {
        // Negative pre_comma values
        let val = FloatValue::const_new_fractional(-1, 5);
        assert_eq!(val.get(), -1.5);

        let val = FloatValue::const_new_fractional(0, 83);
        assert!((val.get() - 0.83).abs() < 0.001);

        let val = FloatValue::const_new_fractional(-2, 123);
        assert!((val.get() - -2.123).abs() < 0.001);

        // Negative post_comma (unusual case - treated as negative fractional part)
        let val = FloatValue::const_new_fractional(1, -5);
        assert_eq!(val.get(), 0.5); // 1 + (-0.5) = 0.5

        let val = FloatValue::const_new_fractional(0, -50);
        assert!((val.get() - -0.5).abs() < 0.001); // 0 + (-0.5) = -0.5
    }

    #[test]
    fn test_const_new_fractional_edge_cases() {
        // Zero
        let val = FloatValue::const_new_fractional(0, 0);
        assert_eq!(val.get(), 0.0);

        // Large integer part
        let val = FloatValue::const_new_fractional(100, 5);
        assert_eq!(val.get(), 100.5);

        let val = FloatValue::const_new_fractional(1000, 99);
        assert!((val.get() - 1000.99).abs() < 0.001);

        // Maximum precision (3 digits)
        let val = FloatValue::const_new_fractional(0, 999);
        assert!((val.get() - 0.999).abs() < 0.001);

        // Small fractional values
        let val = FloatValue::const_new_fractional(1, 1);
        assert!((val.get() - 1.1).abs() < 0.001);

        let val = FloatValue::const_new_fractional(1, 10);
        assert!((val.get() - 1.10).abs() < 0.001);
    }

    #[test]
    fn test_const_new_fractional_ua_css_values() {
        // Test actual values used in ua_css.rs

        // H1: 2em
        let val = FloatValue::const_new_fractional(2, 0);
        assert_eq!(val.get(), 2.0);

        // H2: 1.5em
        let val = FloatValue::const_new_fractional(1, 5);
        assert_eq!(val.get(), 1.5);

        // H3: 1.17em
        let val = FloatValue::const_new_fractional(1, 17);
        assert!((val.get() - 1.17).abs() < 0.001);

        // H4: 1em
        let val = FloatValue::const_new_fractional(1, 0);
        assert_eq!(val.get(), 1.0);

        // H5: 0.83em
        let val = FloatValue::const_new_fractional(0, 83);
        assert!((val.get() - 0.83).abs() < 0.001);

        // H6: 0.67em
        let val = FloatValue::const_new_fractional(0, 67);
        assert!((val.get() - 0.67).abs() < 0.001);

        // Margins: 0.67em
        let val = FloatValue::const_new_fractional(0, 67);
        assert!((val.get() - 0.67).abs() < 0.001);

        // Margins: 0.83em
        let val = FloatValue::const_new_fractional(0, 83);
        assert!((val.get() - 0.83).abs() < 0.001);

        // Margins: 1.33em
        let val = FloatValue::const_new_fractional(1, 33);
        assert!((val.get() - 1.33).abs() < 0.001);

        // Margins: 1.67em
        let val = FloatValue::const_new_fractional(1, 67);
        assert!((val.get() - 1.67).abs() < 0.001);

        // Margins: 2.33em
        let val = FloatValue::const_new_fractional(2, 33);
        assert!((val.get() - 2.33).abs() < 0.001);
    }

    #[test]
    fn test_const_new_fractional_consistency() {
        // Verify consistency between const_new_fractional and new()

        let const_val = FloatValue::const_new_fractional(1, 5);
        let runtime_val = FloatValue::new(1.5);
        assert_eq!(const_val.get(), runtime_val.get());

        let const_val = FloatValue::const_new_fractional(0, 83);
        let runtime_val = FloatValue::new(0.83);
        assert!((const_val.get() - runtime_val.get()).abs() < 0.001);

        let const_val = FloatValue::const_new_fractional(1, 523);
        let runtime_val = FloatValue::new(1.523);
        assert!((const_val.get() - runtime_val.get()).abs() < 0.001);

        let const_val = FloatValue::const_new_fractional(2, 99);
        let runtime_val = FloatValue::new(2.99);
        assert!((const_val.get() - runtime_val.get()).abs() < 0.001);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::excessive_precision
)]
mod autotest_generated {
    use std::{
        collections::{hash_map::DefaultHasher, HashSet},
        hash::{Hash, Hasher},
    };

    use super::*;
    use crate::props::basic::error::ParseFloatError as CssParseFloatError;

    /// Largest `isize` that `const_new` can scale by `FP_PRECISION_MULTIPLIER`
    /// without overflowing the multiplication.
    const MAX_SAFE_CONST_NEW: isize = isize::MAX / 1000;
    const MIN_SAFE_CONST_NEW: isize = isize::MIN / 1000;

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    // ------------------------------------------------------- FloatValue::new ---

    #[test]
    fn float_value_new_never_produces_a_non_finite_get() {
        // `get()` decodes an isize, so it must be finite for *every* input,
        // including the ones that overflow the f32 multiply inside `new()`.
        for v in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            0.0,
            -0.0,
            1e30,
            -1e30,
        ] {
            let got = FloatValue::new(v).get();
            assert!(
                got.is_finite(),
                "FloatValue::new({v}).get() leaked a non-finite value: {got}"
            );
        }
    }

    #[test]
    fn float_value_new_saturates_at_the_isize_bounds() {
        // f32 -> isize `as` casts saturate; +inf/-inf and anything that overflows
        // the *1000 multiply must clamp instead of wrapping.
        assert_eq!(FloatValue::new(f32::INFINITY).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::NEG_INFINITY).number(), isize::MIN);
        // f32::MAX * 1000.0 overflows to +inf before the cast.
        assert_eq!(FloatValue::new(f32::MAX).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::MIN).number(), isize::MIN);
    }

    #[test]
    fn float_value_new_collapses_nan_to_zero() {
        // NaN `as isize` is defined to be 0 — assert it, so a future hand-rolled
        // cast that panics or wraps is caught.
        let nan = FloatValue::new(f32::NAN);
        assert_eq!(nan.number(), 0);
        assert_eq!(nan.get(), 0.0);
        // ...and NaN is therefore *equal* to the default value, not unequal-to-itself.
        assert_eq!(nan, FloatValue::default());
        assert_eq!(hash_of(&nan), hash_of(&FloatValue::default()));
    }

    #[test]
    fn float_value_new_does_not_leak_negative_zero() {
        let neg_zero = FloatValue::new(-0.0);
        assert_eq!(neg_zero.number(), 0);
        assert!(
            neg_zero.get().is_sign_positive(),
            "-0.0 round-tripped back out as a negative zero"
        );
        assert_eq!(neg_zero, FloatValue::new(0.0));
    }

    #[test]
    fn float_value_new_underflows_subnormals_to_zero() {
        // Anything below 1/1000 truncates away entirely.
        assert_eq!(FloatValue::new(f32::MIN_POSITIVE).number(), 0);
        assert_eq!(FloatValue::new(1e-30).number(), 0);
        assert_eq!(FloatValue::new(0.0009).number(), 0);
    }

    #[test]
    fn float_value_new_truncates_toward_zero_not_to_nearest() {
        // Encoding is `(v * 1000) as isize`, i.e. truncation — 0.0019 must NOT
        // round up to 0.002, and the negative side must truncate toward zero too.
        assert_eq!(FloatValue::new(0.0019).number(), 1);
        assert_eq!(FloatValue::new(0.0019).get(), 0.001);
        assert_eq!(FloatValue::new(-0.0019).number(), -1);
        assert_eq!(FloatValue::new(-0.0019).get(), -0.001);
    }

    #[test]
    fn float_value_quantizes_below_the_precision_limit() {
        // The type's whole purpose: sub-precision differences collapse, so that
        // Eq/Hash are stable. 4th decimal is dropped, 3rd is kept.
        assert_eq!(FloatValue::new(1.0001), FloatValue::new(1.0));
        assert_ne!(FloatValue::new(1.001), FloatValue::new(1.0));
    }

    #[test]
    fn float_value_eq_implies_equal_hash() {
        // Eq + Hash must agree — the type exists purely to be hash-able.
        for (a, b) in [
            (1.0_f32, 1.0004_f32),
            (-2.5, -2.5001),
            (0.0, -0.0),
            (f32::NAN, f32::NAN),
        ] {
            let (a, b) = (FloatValue::new(a), FloatValue::new(b));
            assert_eq!(a, b, "expected {a:?} == {b:?}");
            assert_eq!(hash_of(&a), hash_of(&b), "{a:?} == {b:?} but hashes differ");
        }
    }

    #[test]
    fn float_value_ord_agrees_with_get() {
        // Ord is derived on the encoded isize; it must stay monotonic w.r.t. get().
        let mut vals: Vec<FloatValue> = [3.5_f32, -1.0, 0.0, 100.25, -0.001, 2.0]
            .into_iter()
            .map(FloatValue::new)
            .collect();
        vals.sort();
        for w in vals.windows(2) {
            assert!(
                w[0].get() <= w[1].get(),
                "sort order disagrees with get(): {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    // -------------------------------------------------- FloatValue::const_new ---

    #[test]
    fn const_new_matches_the_documented_encoding() {
        assert_eq!(FP_PRECISION_MULTIPLIER, 1000.0);
        assert_eq!(FloatValue::const_new(0).number(), 0);
        assert_eq!(FloatValue::const_new(1).number(), 1000);
        assert_eq!(FloatValue::const_new(-1).number(), -1000);
        assert_eq!(FloatValue::const_new(0), FloatValue::default());
    }

    #[test]
    fn const_new_agrees_with_new_for_whole_numbers() {
        for n in [-1000_isize, -7, -1, 0, 1, 7, 1000, 65_536] {
            let c = FloatValue::const_new(n);
            let r = FloatValue::new(n as f32);
            assert_eq!(
                c, r,
                "const_new({n}) = {c:?} disagrees with new({n}.0) = {r:?}"
            );
        }
    }

    #[test]
    fn const_new_survives_the_largest_non_overflowing_inputs() {
        // `const_new` is a bare `value * 1000`, so isize::MAX/1000 is the last
        // input it can take without overflowing. Pin that boundary: anything at
        // or below it must be exact and must not panic.
        let hi = FloatValue::const_new(MAX_SAFE_CONST_NEW);
        assert_eq!(hi.number(), MAX_SAFE_CONST_NEW * 1000);
        assert!(hi.get().is_finite());

        let lo = FloatValue::const_new(MIN_SAFE_CONST_NEW);
        assert_eq!(lo.number(), MIN_SAFE_CONST_NEW * 1000);
        assert!(lo.get().is_finite());

        assert!(lo < hi);
    }

    // --------------------------------------- FloatValue::const_new_fractional ---

    #[test]
    fn const_new_fractional_zero_and_sign_handling() {
        assert_eq!(FloatValue::const_new_fractional(0, 0).number(), 0);
        // Negative pre_comma pulls the fraction negative too (-1.5, not -0.5).
        assert_eq!(FloatValue::const_new_fractional(-1, 5).number(), -1500);
        // Negative post_comma subtracts from a positive pre_comma.
        assert_eq!(FloatValue::const_new_fractional(1, -5).number(), 500);
        assert_eq!(FloatValue::const_new_fractional(0, -50).number(), -500);
    }

    #[test]
    fn const_new_fractional_never_panics_on_extreme_post_comma() {
        // post_comma is an unbounded isize; the digit-count ladder must not
        // divide by zero, overflow, or produce a non-finite decode.
        for post in [
            9_isize,
            99,
            999,
            9_999,
            99_999,
            999_999,
            9_999_999,
            99_999_999,
            999_999_999,
            isize::MAX,
        ] {
            let v = FloatValue::const_new_fractional(0, post);
            assert!(
                v.get().is_finite(),
                "const_new_fractional(0, {post}) decoded to a non-finite value"
            );
        }
    }

    #[test]
    fn const_new_fractional_truncates_to_three_decimals() {
        // Documented: only the first 3 digits of post_comma are used, truncated.
        assert_eq!(FloatValue::const_new_fractional(0, 5234).number(), 523);
        assert_eq!(FloatValue::const_new_fractional(1, 123_456).number(), 1123);
        // 10 digits is the largest post_comma the ladder still truncates correctly.
        assert_eq!(
            FloatValue::const_new_fractional(2, 1_234_567_890).number(),
            2123
        );
    }

    #[test]
    fn const_new_fractional_boundary_between_digit_buckets() {
        // Every `abs_post < 10^k` bucket edge: 9/10, 99/100, 999/1000.
        assert_eq!(FloatValue::const_new_fractional(0, 9).get(), 0.9);
        assert_eq!(FloatValue::const_new_fractional(0, 10).get(), 0.1);
        assert_eq!(FloatValue::const_new_fractional(0, 99).get(), 0.99);
        assert_eq!(FloatValue::const_new_fractional(0, 100).get(), 0.1);
        assert_eq!(FloatValue::const_new_fractional(0, 999).get(), 0.999);
    }

    #[test]
    fn const_new_fractional_cannot_express_a_leading_zero_fraction() {
        // The bucket is picked from the *digit count* of post_comma, so a leading
        // zero is unrepresentable in an integer argument: 0.05 has no spelling.
        // Both of the obvious attempts land on 0.5 instead. Pin the footgun so a
        // caller writing `(0, 50)` for "0.05em" is caught by this test, not by a
        // 10x-too-large margin on screen.
        assert_eq!(FloatValue::const_new_fractional(0, 5).get(), 0.5);
        assert_eq!(FloatValue::const_new_fractional(0, 50).get(), 0.5);
        assert_eq!(FloatValue::const_new_fractional(0, 500).get(), 0.5);
    }

    // ------------------------------------------------- FloatValue::interpolate ---

    #[test]
    fn interpolate_endpoints_are_exact() {
        let a = FloatValue::new(0.0);
        let b = FloatValue::new(10.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5).get(), 5.0);
        // Reversed direction.
        assert_eq!(b.interpolate(&a, 0.5).get(), 5.0);
    }

    #[test]
    fn interpolate_extrapolates_outside_zero_one() {
        // t is not clamped — assert the (documented-by-absence) extrapolation
        // rather than silently assuming a clamp that isn't there.
        let a = FloatValue::new(0.0);
        let b = FloatValue::new(10.0);
        assert_eq!(a.interpolate(&b, 2.0).get(), 20.0);
        assert_eq!(a.interpolate(&b, -1.0).get(), -10.0);
    }

    #[test]
    fn interpolate_with_nan_or_infinite_t_stays_finite() {
        let a = FloatValue::new(0.0);
        let b = FloatValue::new(10.0);

        // NaN t -> NaN interpolant -> `as isize` collapses to 0.
        assert_eq!(a.interpolate(&b, f32::NAN).number(), 0);

        // +inf t with a non-zero delta -> +inf -> saturates to isize::MAX.
        assert_eq!(a.interpolate(&b, f32::INFINITY).number(), isize::MAX);
        assert_eq!(a.interpolate(&b, f32::NEG_INFINITY).number(), isize::MIN);

        // inf * 0.0 delta is NaN -> collapses to 0 (self is NOT preserved here).
        assert_eq!(a.interpolate(&a, f32::INFINITY).number(), 0);

        for t in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            assert!(
                a.interpolate(&b, t).get().is_finite(),
                "interpolate(t = {t}) leaked a non-finite value"
            );
        }
    }

    #[test]
    fn interpolate_between_saturated_extremes_does_not_panic() {
        let lo = FloatValue::new(f32::NEG_INFINITY); // isize::MIN
        let hi = FloatValue::new(f32::INFINITY); // isize::MAX
        for t in [0.0, 0.5, 1.0, -1.0, 2.0, f32::NAN] {
            assert!(lo.interpolate(&hi, t).get().is_finite());
            assert!(hi.interpolate(&lo, t).get().is_finite());
        }
    }

    // -------------------------------------------------------- round-tripping ---

    #[test]
    fn float_value_round_trips_through_display_and_parse() {
        // encode == decode: every value that is exactly representable at 3
        // decimals must survive Display -> parse_float_value -> FloatValue.
        for v in [0.0_f32, 1.5, -2.25, 100.0, 0.001, -0.001, 999.999, -0.5] {
            let fv = FloatValue::new(v);
            let round_tripped = parse_float_value(&fv.to_string())
                .unwrap_or_else(|e| panic!("Display of {fv:?} did not re-parse: {e}"));
            assert_eq!(
                fv, round_tripped,
                "round-trip changed {fv:?} into {round_tripped:?}"
            );
        }
    }

    #[test]
    fn float_value_number_round_trips_through_get() {
        // number() is the compact-cache encoding; get() must be its exact inverse
        // (scaled) for values inside the f32-exact integer range.
        for raw in [0_isize, 1, -1, 1500, -1500, 999_999, -999_999] {
            let fv = FloatValue::new(raw as f32 / 1000.0);
            assert_eq!(fv.number(), raw, "number() lost the encoding for {raw}");
        }
    }

    #[test]
    fn float_value_display_and_debug_agree() {
        // Debug is hand-written to forward to Display; a divergence means the
        // manual impl drifted.
        for v in [0.0_f32, -1.25, 1e6, f32::INFINITY, f32::NAN] {
            let fv = FloatValue::new(v);
            assert_eq!(format!("{fv:?}"), format!("{fv}"));
            assert!(!format!("{fv}").is_empty());
            // Whatever we print must itself be a parseable float.
            assert!(fv.to_string().parse::<f32>().is_ok());
        }
        assert_eq!(FloatValue::default().to_string(), "0");
    }

    // ---------------------------------------------------------- SizeMetric ---

    #[test]
    fn size_metric_display_is_non_empty_and_unique() {
        use SizeMetric::{Cm, Em, In, Mm, Percent, Pt, Px, Rem, Vh, Vmax, Vmin, Vw};

        let all = [Px, Pt, Em, Rem, In, Cm, Mm, Percent, Vw, Vh, Vmin, Vmax];
        let mut seen = HashSet::new();
        for m in all {
            let s = m.to_string();
            assert!(!s.is_empty(), "{m:?} renders as an empty string");
            assert!(
                seen.insert(s.clone()),
                "two SizeMetric variants both render as {s:?} (copy-paste in Display)"
            );
        }
        assert_eq!(seen.len(), all.len());
    }

    #[test]
    fn size_metric_display_matches_the_css_unit_tokens() {
        assert_eq!(SizeMetric::Px.to_string(), "px");
        assert_eq!(SizeMetric::Pt.to_string(), "pt");
        assert_eq!(SizeMetric::Em.to_string(), "em");
        assert_eq!(SizeMetric::Rem.to_string(), "rem");
        assert_eq!(SizeMetric::In.to_string(), "in");
        assert_eq!(SizeMetric::Cm.to_string(), "cm");
        assert_eq!(SizeMetric::Mm.to_string(), "mm");
        assert_eq!(SizeMetric::Percent.to_string(), "%");
        assert_eq!(SizeMetric::Vw.to_string(), "vw");
        assert_eq!(SizeMetric::Vh.to_string(), "vh");
        assert_eq!(SizeMetric::Vmin.to_string(), "vmin");
        assert_eq!(SizeMetric::Vmax.to_string(), "vmax");
    }

    #[test]
    fn size_metric_default_is_px() {
        assert_eq!(SizeMetric::default(), SizeMetric::Px);
        assert_eq!(SizeMetric::default().to_string(), "px");
    }

    // -------------------------------------------------------- PercentageValue ---

    #[test]
    fn percentage_value_normalized_divides_by_a_hundred() {
        assert_eq!(PercentageValue::new(50.0).normalized(), 0.5);
        assert_eq!(PercentageValue::new(0.0).normalized(), 0.0);
        assert_eq!(PercentageValue::new(-25.0).normalized(), -0.25);
        assert_eq!(PercentageValue::const_new(100).normalized(), 1.0);
        assert_eq!(PercentageValue::default().normalized(), 0.0);
    }

    #[test]
    fn percentage_value_normalized_is_always_finite() {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            let n = PercentageValue::new(v).normalized();
            assert!(
                n.is_finite(),
                "PercentageValue::new({v}).normalized() leaked {n}"
            );
        }
        // NaN collapses to the default, exactly like FloatValue.
        assert_eq!(PercentageValue::new(f32::NAN), PercentageValue::default());
    }

    #[test]
    fn percentage_value_const_new_boundaries_do_not_panic() {
        assert_eq!(PercentageValue::const_new(0), PercentageValue::default());
        assert!(PercentageValue::const_new(MAX_SAFE_CONST_NEW)
            .normalized()
            .is_finite());
        assert!(PercentageValue::const_new(MIN_SAFE_CONST_NEW)
            .normalized()
            .is_finite());
        assert!(
            PercentageValue::const_new(MIN_SAFE_CONST_NEW)
                < PercentageValue::const_new(MAX_SAFE_CONST_NEW)
        );
    }

    #[test]
    fn percentage_value_const_new_fractional_matches_the_docs() {
        // 100% = const_new_fractional(100, 0); 50.5% = const_new_fractional(50, 5)
        assert_eq!(
            PercentageValue::const_new_fractional(100, 0).normalized(),
            1.0
        );
        assert!((PercentageValue::const_new_fractional(50, 5).normalized() - 0.505).abs() < 1e-5);
        assert_eq!(
            PercentageValue::const_new_fractional(100, 0),
            PercentageValue::const_new(100)
        );
    }

    #[test]
    fn percentage_value_interpolate_endpoints_and_nan() {
        let a = PercentageValue::new(0.0);
        let b = PercentageValue::new(100.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5).normalized(), 0.5);
        // NaN / inf t must not panic and must stay finite.
        assert_eq!(a.interpolate(&b, f32::NAN).normalized(), 0.0);
        assert!(a.interpolate(&b, f32::INFINITY).normalized().is_finite());
        assert!(a.interpolate(&b, f32::NEG_INFINITY).normalized().is_finite());
    }

    #[test]
    fn percentage_value_display_round_trips_through_the_parser() {
        for v in [0.0_f32, 50.0, 100.0, 150.0, -25.0, 75.5, 0.5] {
            let p = PercentageValue::new(v);
            let s = p.to_string();
            assert!(s.ends_with('%'), "Display lost the percent sign: {s:?}");
            let back = parse_percentage_value(&s)
                .unwrap_or_else(|e| panic!("Display of {p:?} ({s:?}) did not re-parse: {e}"));
            assert!(
                (back.normalized() - p.normalized()).abs() < 1e-4,
                "round-trip drifted: {p:?} -> {s:?} -> {back:?}"
            );
        }
    }

    // ----------------------------------------------------- parse_float_value ---

    #[test]
    fn parse_float_value_positive_control() {
        assert_eq!(parse_float_value("0").unwrap().number(), 0);
        assert_eq!(parse_float_value("1.5").unwrap().number(), 1500);
        assert_eq!(parse_float_value("-1.5").unwrap().number(), -1500);
        assert_eq!(parse_float_value("+2").unwrap().number(), 2000);
        // Rust's f32 parser accepts these shorthand forms.
        assert_eq!(parse_float_value(".5").unwrap().number(), 500);
        assert_eq!(parse_float_value("5.").unwrap().number(), 5000);
    }

    #[test]
    fn parse_float_value_rejects_empty_and_whitespace() {
        assert!(parse_float_value("").is_err());
        assert!(parse_float_value("   ").is_err());
        assert!(parse_float_value("\t\n\r ").is_err());
    }

    #[test]
    fn parse_float_value_rejects_garbage() {
        for input in [
            "abc", "1_000", "1,5", "0x10", "1.2.3", "--1", "1e", "e5", "5 5", "1/2", ";", "\0",
            "5;garbage", "50px", "5%",
        ] {
            assert!(
                parse_float_value(input).is_err(),
                "garbage input {input:?} was accepted"
            );
        }
    }

    #[test]
    fn parse_float_value_trims_but_does_not_tolerate_inner_junk() {
        assert_eq!(parse_float_value("  1.5  ").unwrap().number(), 1500);
        assert!(parse_float_value("1.5 garbage").is_err());
    }

    #[test]
    fn parse_float_value_boundary_numbers_saturate_instead_of_panicking() {
        // -0 must not leak a negative zero out of the encoding.
        assert_eq!(parse_float_value("-0").unwrap().number(), 0);
        assert!(parse_float_value("-0").unwrap().get().is_sign_positive());

        // Rust parses "NaN"/"inf" successfully; the encoding must then defuse them.
        assert_eq!(parse_float_value("NaN").unwrap().number(), 0);
        assert_eq!(parse_float_value("inf").unwrap().number(), isize::MAX);
        assert_eq!(parse_float_value("infinity").unwrap().number(), isize::MAX);
        assert_eq!(parse_float_value("-inf").unwrap().number(), isize::MIN);

        // Overflow of the f32 parse itself is Ok(inf) in Rust, then saturates.
        assert_eq!(parse_float_value("1e400").unwrap().number(), isize::MAX);
        assert_eq!(parse_float_value("-1e400").unwrap().number(), isize::MIN);
        // Underflow is Ok(0.0).
        assert_eq!(parse_float_value("1e-400").unwrap().number(), 0);

        // i64::MAX / f64::MAX as literals: no panic, still finite after decode.
        for input in [
            "9223372036854775807",
            "-9223372036854775808",
            "179769313486231570000000000000000000000000000000000",
        ] {
            let v = parse_float_value(input)
                .unwrap_or_else(|e| panic!("{input:?} should parse as f32, got {e}"));
            assert!(v.get().is_finite(), "{input:?} decoded to {}", v.get());
        }
    }

    #[test]
    fn parse_float_value_unicode_does_not_panic() {
        // Multi-byte input must be rejected, never sliced mid-codepoint.
        for input in [
            "\u{1F600}",  // emoji
            "5\u{1F600}", // digit + emoji
            "\u{0665}",   // ARABIC-INDIC DIGIT FIVE (is_numeric() == true)
            "5\u{0301}",  // digit + combining acute
            "\u{00BD}",   // ½ (No category, is_numeric() == true)
            "\u{FF15}",   // FULLWIDTH DIGIT FIVE
            "\u{200B}5",  // zero-width space + digit
            "\u{2212}5",  // U+2212 MINUS SIGN (not ASCII '-')
        ] {
            assert!(
                parse_float_value(input).is_err(),
                "non-ASCII input {input:?} was accepted as a float"
            );
        }
    }

    #[test]
    fn parse_float_value_extremely_long_input_terminates() {
        // 200k digits: must not hang, must not panic; Rust yields Ok(inf), which
        // then saturates in the encoding.
        let huge = "9".repeat(200_000);
        // Rejecting is acceptable too — just don't panic/hang on the huge input.
        if let Ok(v) = parse_float_value(&huge) {
            assert!(v.get().is_finite(), "200k digits decoded to {}", v.get());
        }

        // Long *garbage* must be rejected rather than scanned quadratically.
        let long_junk = "a".repeat(200_000);
        assert!(parse_float_value(&long_junk).is_err());
    }

    #[test]
    fn parse_float_value_deeply_nested_input_does_not_stack_overflow() {
        let nested = "(".repeat(10_000);
        assert!(parse_float_value(&nested).is_err());
        let nested_pair = format!("{}5{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_float_value(&nested_pair).is_err());
    }

    // ------------------------------------------------ parse_percentage_value ---

    #[test]
    fn parse_percentage_value_positive_control() {
        assert_eq!(parse_percentage_value("50%").unwrap().normalized(), 0.5);
        assert_eq!(parse_percentage_value("0%").unwrap().normalized(), 0.0);
        assert_eq!(parse_percentage_value("-25%").unwrap().normalized(), -0.25);
        // A bare number is a *ratio*, not a percent: "0.5" == "50%".
        assert_eq!(
            parse_percentage_value("0.5").unwrap(),
            parse_percentage_value("50%").unwrap()
        );
    }

    #[test]
    fn parse_percentage_value_bare_number_is_multiplied_by_a_hundred() {
        // Easy to misread: "50" (no sign) is 5000%, not 50%.
        assert_eq!(parse_percentage_value("50").unwrap().normalized(), 50.0);
        assert_ne!(
            parse_percentage_value("50").unwrap(),
            parse_percentage_value("50%").unwrap()
        );
    }

    #[test]
    fn parse_percentage_value_rejects_empty_and_whitespace() {
        assert!(matches!(
            parse_percentage_value(""),
            Err(PercentageParseError::ValueParseErr(_))
        ));
        assert!(matches!(
            parse_percentage_value("   "),
            Err(PercentageParseError::ValueParseErr(_))
        ));
        assert!(matches!(
            parse_percentage_value("\t\n"),
            Err(PercentageParseError::ValueParseErr(_))
        ));
        assert!(parse_percentage_value("%").is_err());
    }

    #[test]
    fn parse_percentage_value_rejects_garbage_without_panicking() {
        for input in [
            "abc", "fifty%", "%50", "50%%", "5 0 %", "--5%", "1.2.3%", ";", "\0", "NaN", "inf",
            "-inf",
        ] {
            assert!(
                parse_percentage_value(input).is_err(),
                "garbage input {input:?} was accepted"
            );
        }
    }

    #[test]
    fn parse_percentage_value_reports_invalid_units() {
        for (input, unit) in [("50px", "px"), ("50em", "em"), ("1.5rem", "rem")] {
            match parse_percentage_value(input) {
                Err(PercentageParseError::InvalidUnit(u)) => assert_eq!(u.as_str(), unit),
                other => panic!("{input:?} should be InvalidUnit({unit:?}), got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_percentage_value_trims_leading_and_trailing_whitespace() {
        assert_eq!(
            parse_percentage_value("  75.5%  ").unwrap().normalized(),
            0.755
        );
        // Whitespace *between* the number and the unit is trimmed as well.
        assert_eq!(parse_percentage_value("50 %").unwrap().normalized(), 0.5);
    }

    #[test]
    fn parse_percentage_value_boundary_numbers_stay_finite() {
        // -0 must not leak a negative zero.
        let neg_zero = parse_percentage_value("-0%").unwrap();
        assert_eq!(neg_zero.normalized(), 0.0);
        assert!(neg_zero.normalized().is_sign_positive());

        // Overflowing exponent parses to inf, then saturates in the encoding.
        let huge = parse_percentage_value("1e400%").unwrap();
        assert!(
            huge.normalized().is_finite(),
            "1e400% leaked {}",
            huge.normalized()
        );
        let huge_neg = parse_percentage_value("-1e400%").unwrap();
        assert!(huge_neg.normalized().is_finite());
        // Underflowing exponent parses to 0.
        assert_eq!(parse_percentage_value("1e-400%").unwrap().normalized(), 0.0);

        // i64::MAX-sized literal: no panic, still finite.
        let big = parse_percentage_value("9223372036854775807%").unwrap();
        assert!(big.normalized().is_finite());
    }

    #[test]
    fn parse_percentage_value_ascii_unicode_neighbours_do_not_panic() {
        // Multi-byte chars that are NOT `char::is_numeric()` are safe to slice
        // around; they must be rejected, not panic.
        for input in [
            "\u{1F600}",  // emoji only
            "50\u{1F600}", // digits then emoji -> InvalidUnit
            "\u{20AC}50",  // €50 -> unparseable number
            "abc\u{00E9}%",
            "\u{200B}%", // zero-width space
        ] {
            assert!(
                parse_percentage_value(input).is_err(),
                "{input:?} was accepted"
            );
        }
        // The emoji suffix is reported as an invalid unit, not a parse error.
        assert!(matches!(
            parse_percentage_value("50\u{1F600}"),
            Err(PercentageParseError::InvalidUnit(_))
        ));
    }

    #[test]
    fn parse_percentage_value_extremely_long_input_terminates() {
        let huge = format!("{}%", "9".repeat(200_000));
        if let Ok(v) = parse_percentage_value(&huge) { assert!(v.normalized().is_finite()) }
        let long_junk = format!("{}%", "a".repeat(200_000));
        assert!(parse_percentage_value(&long_junk).is_err());
    }

    #[test]
    fn parse_percentage_value_deeply_nested_input_does_not_stack_overflow() {
        assert!(parse_percentage_value(&"(".repeat(10_000)).is_err());
        // A numeric char buried behind 10k brackets: the scanner must still just
        // split and fail on the number, not recurse.
        let nested = format!("{}5%", "(".repeat(10_000));
        assert!(parse_percentage_value(&nested).is_err());
    }

    // --------------------------------------------- PercentageParseError glue ---

    #[test]
    fn percentage_parse_error_round_trips_through_owned() {
        let variants = [
            PercentageParseError::ValueParseErr(CssParseFloatError::Empty),
            PercentageParseError::ValueParseErr(CssParseFloatError::Invalid),
            PercentageParseError::NoPercentSign,
            PercentageParseError::InvalidUnit(String::new().into()),
            PercentageParseError::InvalidUnit("px".to_string().into()),
            // A unit that is itself multi-byte must survive the AzString clone.
            PercentageParseError::InvalidUnit("\u{1F600}".to_string().into()),
        ];
        for e in variants {
            let round_tripped = e.to_contained().to_shared();
            assert_eq!(
                e, round_tripped,
                "to_contained/to_shared is not the identity for {e:?}"
            );
        }
    }

    #[test]
    fn percentage_parse_error_owned_round_trips_through_shared() {
        let variants = [
            PercentageParseErrorOwned::ValueParseErr(CssParseFloatError::Invalid),
            PercentageParseErrorOwned::NoPercentSign,
            PercentageParseErrorOwned::InvalidUnit("vh".to_string().into()),
        ];
        for e in variants {
            assert_eq!(e.to_shared().to_contained(), e);
        }
    }

    #[test]
    fn percentage_parse_error_display_is_non_empty() {
        // Debug forwards to Display (impl_debug_as_display); neither may be empty
        // nor panic, including for an empty invalid unit.
        for e in [
            PercentageParseError::ValueParseErr(CssParseFloatError::Empty),
            PercentageParseError::NoPercentSign,
            PercentageParseError::InvalidUnit(String::new().into()),
        ] {
            let shown = e.to_string();
            assert!(!shown.is_empty(), "{e:?} renders as an empty message");
            assert_eq!(format!("{e:?}"), shown);
        }
    }

    // ------------------------------------------------------------ known bugs ---
    //
    // The two tests below assert the behaviour these functions *should* have.
    // They currently fail, so they are #[ignore]d rather than deleted or weakened
    // — run them with `cargo test -p azul-css -- --ignored` after fixing.

    #[test]
    fn known_bug_percentage_multibyte_numeric_char_panics() {
        // `char::is_numeric()` is true for Nd/Nl/No — including multi-byte chars
        // like '½' (U+00BD, 2 bytes) and '٥' (U+0665, 2 bytes). The scanner
        // records their *start* byte index, then slices at `split_pos + 1`, which
        // lands inside the codepoint => `input[split_pos..]` panics.
        //
        // Reachable from any author stylesheet (`width: ½%`), so this panics the
        // CSS parser on untrusted input.
        for input in ["\u{00BD}%", "\u{0665}%", "5\u{00BD}", "\u{FF15}%"] {
            assert!(
                parse_percentage_value(input).is_err(),
                "{input:?} should be rejected"
            );
        }
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn known_bug_const_new_fractional_huge_post_comma_escapes_the_fraction() {
        // The digit-count ladder's last arm divides by 10_000_000, which only
        // truncates a 10-digit post_comma down to 3 digits. An 11-digit value
        // keeps 4 digits, a 12-digit value keeps 5, ... so the "fractional" part
        // grows past 1.0 and corrupts the integer part.
        for post in [12_345_678_901_isize, 123_456_789_012, isize::MAX] {
            let frac = FloatValue::const_new_fractional(0, post).get();
            assert!(
                (0.0..1.0).contains(&frac),
                "const_new_fractional(0, {post}) produced {frac}, which is not a fraction"
            );
        }
    }
}
