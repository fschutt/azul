use core::fmt;
use std::num::ParseFloatError;

use crate::corety::AzString;

/// Multiplier for floating point accuracy. Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 5 digits, i.e
/// `1.00001 == 1.0`
pub const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
pub const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

/// Wrapper around FloatValue, represents a percentage instead
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}%", self.normalized() * 100.0)
    }
}

impl PercentageValue {
    /// Same as `PercentageValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_new(value: isize) -> Self {
        Self {
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a PercentageValue from a fractional number in const context.
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
    pub const fn const_new_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self {
            number: FloatValue::const_new_fractional(pre_comma, post_comma),
        }
    }

    #[inline]
    pub fn new(value: f32) -> Self {
        Self {
            number: value.into(),
        }
    }

    // NOTE: no get() function, to avoid confusion with "150%"

    #[inline]
    pub fn normalized(&self) -> f32 {
        self.number.get() / 100.0
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
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
    pub number: isize,
}

impl fmt::Display for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl ::core::fmt::Debug for FloatValue {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Default for FloatValue {
    fn default() -> Self {
        const DEFAULT_FLV: FloatValue = FloatValue::const_new(0);
        DEFAULT_FLV
    }
}

impl FloatValue {
    /// Same as `FloatValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_new(value: isize) -> Self {
        Self {
            number: value * FP_PRECISION_MULTIPLIER_CONST,
        }
    }

    /// Creates a FloatValue from a fractional number in const context.
    ///
    /// This is needed because f32 operations are not allowed in const fn,
    /// but we still want to represent values like 1.5, 0.83, etc.
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
    pub const fn const_new_fractional(pre_comma: isize, post_comma: isize) -> Self {
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
        } else if abs_post < 10000 {
            // 4+ digits: take first 3 (e.g., 5234 → 523 → 0.523)
            (abs_post / 10, 1000)
        } else if abs_post < 100000 {
            (abs_post / 100, 1000)
        } else if abs_post < 1000000 {
            (abs_post / 1000, 1000)
        } else if abs_post < 10000000 {
            (abs_post / 10000, 1000)
        } else if abs_post < 100000000 {
            (abs_post / 100000, 1000)
        } else if abs_post < 1000000000 {
            (abs_post / 1000000, 1000)
        } else {
            // For very large values (>= 1 billion), cap at reasonable precision
            // This ensures compatibility with 32-bit isize on WASM
            (abs_post / 10000000, 1000)
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
    pub fn new(value: f32) -> Self {
        Self {
            number: (value * FP_PRECISION_MULTIPLIER) as isize,
        }
    }

    #[inline]
    pub fn get(&self) -> f32 {
        self.number as f32 / FP_PRECISION_MULTIPLIER
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
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
pub enum SizeMetric {
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

impl Default for SizeMetric {
    fn default() -> Self {
        SizeMetric::Px
    }
}

impl fmt::Display for SizeMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SizeMetric::*;
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

pub fn parse_float_value(input: &str) -> Result<FloatValue, ParseFloatError> {
    Ok(FloatValue::new(input.trim().parse::<f32>()?))
}

#[derive(Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum PercentageParseError {
    ValueParseErr(ParseFloatError),
    NoPercentSign,
    InvalidUnit(AzString),
}

impl_debug_as_display!(PercentageParseError);
impl_from!(ParseFloatError, PercentageParseError::ValueParseErr);

impl_display! { PercentageParseError, {
    ValueParseErr(e) => format!("\"{}\"", e),
    NoPercentSign => format!("No percent sign after number"),
    InvalidUnit(u) => format!("Error parsing percentage: invalid unit \"{}\"", u.as_str()),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum PercentageParseErrorOwned {
    ValueParseErr(ParseFloatError),
    NoPercentSign,
    InvalidUnit(AzString),
}

impl PercentageParseError {
    pub fn to_contained(&self) -> PercentageParseErrorOwned {
        match self {
            Self::ValueParseErr(e) => PercentageParseErrorOwned::ValueParseErr(e.clone()),
            Self::NoPercentSign => PercentageParseErrorOwned::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseErrorOwned::InvalidUnit(u.clone()),
        }
    }
}

impl PercentageParseErrorOwned {
    pub fn to_shared(&self) -> PercentageParseError {
        match self {
            Self::ValueParseErr(e) => PercentageParseError::ValueParseErr(e.clone()),
            Self::NoPercentSign => PercentageParseError::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseError::InvalidUnit(u.clone()),
        }
    }
}

/// Parse "1.2" or "120%" (similar to parse_pixel_value)
pub fn parse_percentage_value(input: &str) -> Result<PercentageValue, PercentageParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(PercentageParseError::ValueParseErr(
            "empty string".parse::<f32>().unwrap_err(),
        ));
    }

    let mut split_pos = 0;
    let mut found_numeric = false;
    for (idx, ch) in input.char_indices() {
        if ch.is_numeric() || ch == '.' || ch == '-' {
            split_pos = idx;
            found_numeric = true;
        }
    }

    if !found_numeric {
        return Err(PercentageParseError::ValueParseErr(
            "no numeric value".parse::<f32>().unwrap_err(),
        ));
    }

    split_pos += 1;

    let unit = input[split_pos..].trim();
    let mut number = input[..split_pos]
        .trim()
        .parse::<f32>()
        .map_err(|e| PercentageParseError::ValueParseErr(e))?;

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
        let val = FloatValue::const_new_fractional(1, 123456);
        assert!((val.get() - 1.123).abs() < 0.001);

        // 7 digits: 9876543 → 987 → 0.987
        let val = FloatValue::const_new_fractional(0, 9876543);
        assert!((val.get() - 0.987).abs() < 0.001);

        // 10 digits
        let val = FloatValue::const_new_fractional(2, 1234567890);
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
