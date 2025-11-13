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
    /// # Arguments
    /// * `pre_comma` - The integer part (e.g., 1 for 1.5)
    /// * `post_comma` - The fractional part as a single digit (e.g., 5 for 0.5)
    ///
    /// # Examples
    /// ```
    /// // 1.5 = const_new_fractional(1, 5)
    /// // 0.83 = const_new_fractional(0, 83)
    /// // 1.17 = const_new_fractional(1, 17)
    /// ```
    #[inline]
    pub const fn const_new_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self {
            number: pre_comma * FP_PRECISION_MULTIPLIER_CONST + post_comma * (FP_PRECISION_MULTIPLIER_CONST / 100),
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
    In,
    Cm,
    Mm,
    Percent,
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
            Em => write!(f, "pt"),
            In => write!(f, "in"),
            Cm => write!(f, "cm"),
            Mm => write!(f, "mm"),
            Percent => write!(f, "%"),
        }
    }
}

pub fn parse_float_value(input: &str) -> Result<FloatValue, ParseFloatError> {
    Ok(FloatValue::new(input.trim().parse::<f32>()?))
}

#[derive(Clone, PartialEq, Eq)]
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
pub enum PercentageParseErrorOwned {
    ValueParseErr(ParseFloatError),
    NoPercentSign,
    InvalidUnit(String),
}

impl PercentageParseError {
    pub fn to_contained(&self) -> PercentageParseErrorOwned {
        match self {
            Self::ValueParseErr(e) => PercentageParseErrorOwned::ValueParseErr(e.clone()),
            Self::NoPercentSign => PercentageParseErrorOwned::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseErrorOwned::InvalidUnit(u.as_str().to_string()),
        }
    }
}

impl PercentageParseErrorOwned {
    pub fn to_shared(&self) -> PercentageParseError {
        match self {
            Self::ValueParseErr(e) => PercentageParseError::ValueParseErr(e.clone()),
            Self::NoPercentSign => PercentageParseError::NoPercentSign,
            Self::InvalidUnit(u) => PercentageParseError::InvalidUnit(u.clone().into()),
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
}
