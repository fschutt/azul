//! CSS property types for angles (degrees, radians, etc.).

use alloc::string::{String, ToString};
use core::{fmt, num::ParseFloatError};
use crate::corety::AzString;

use crate::props::basic::error::ParseFloatErrorWithInput;

use crate::props::{basic::length::FloatValue, formatter::PrintAsCssValue};

/// Enum representing the metric associated with an angle (deg, rad, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum AngleMetric {
    #[default]
    Degree,
    Radians,
    Grad,
    Turn,
    Percent,
}


impl fmt::Display for AngleMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::AngleMetric::{Degree, Radians, Grad, Turn, Percent};
        match self {
            Degree => write!(f, "deg"),
            Radians => write!(f, "rad"),
            Grad => write!(f, "grad"),
            Turn => write!(f, "turn"),
            Percent => write!(f, "%"),
        }
    }
}

/// `FloatValue`, but associated with a certain metric (i.e. deg, rad, etc.)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AngleValue {
    pub metric: AngleMetric,
    pub number: FloatValue,
}

impl_option!(
    AngleValue,
    OptionAngleValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Debug for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl PrintAsCssValue for AngleValue {
    fn print_as_css_value(&self) -> String {
        format!("{self}")
    }
}

impl AngleValue {
    /// Returns an angle of zero degrees.
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        const ZERO_DEG: AngleValue = AngleValue::const_deg(0);
        ZERO_DEG
    }

    /// Creates a const angle value in degrees from an integer.
    #[inline]
    #[must_use] pub const fn const_deg(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Degree, value)
    }

    /// Creates a const angle value in radians from an integer.
    #[inline]
    #[must_use] pub const fn const_rad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Radians, value)
    }

    /// Creates a const angle value in gradians from an integer.
    #[inline]
    #[must_use] pub const fn const_grad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Grad, value)
    }

    /// Creates a const angle value in turns from an integer.
    #[inline]
    #[must_use] pub const fn const_turn(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Turn, value)
    }

    /// Creates a const angle value in percent from an integer.
    #[inline]
    #[must_use] pub const fn const_percent(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Percent, value)
    }

    /// Creates a const angle value with the given metric from an integer.
    #[inline]
    #[must_use] pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a const angle value with the given metric from a fractional number.
    ///
    /// # Arguments
    /// * `metric` - The angle metric (Degree, Radians, etc.)
    /// * `pre_comma` - The integer part (e.g., 45 for 45.5deg)
    /// * `post_comma` - The fractional part as digits (e.g., 5 for 0.5deg)
    #[inline]
    #[must_use] pub const fn const_from_metric_fractional(metric: AngleMetric, pre_comma: isize, post_comma: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new_fractional(pre_comma, post_comma),
        }
    }

    /// Creates an angle value in degrees.
    #[inline]
    #[must_use] pub fn deg(value: f32) -> Self {
        Self::from_metric(AngleMetric::Degree, value)
    }

    /// Creates an angle value in radians.
    #[inline]
    #[must_use] pub fn rad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Radians, value)
    }

    /// Creates an angle value in gradians.
    #[inline]
    #[must_use] pub fn grad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Grad, value)
    }

    /// Creates an angle value in turns.
    #[inline]
    #[must_use] pub fn turn(value: f32) -> Self {
        Self::from_metric(AngleMetric::Turn, value)
    }

    /// Creates an angle value in percent.
    #[inline]
    #[must_use] pub fn percent(value: f32) -> Self {
        Self::from_metric(AngleMetric::Percent, value)
    }

    /// Creates an angle value with the given metric.
    #[inline]
    #[must_use] pub fn from_metric(metric: AngleMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    /// Convert to degrees, normalized to [0, 360) range.
    /// Note: 360.0 becomes 0.0 due to modulo operation.
    /// For conic gradients where 360.0 is meaningful, use `to_degrees_raw()`.
    #[inline]
    #[must_use] pub fn to_degrees(&self) -> f32 {
        let mut val = self.to_degrees_raw() % 360.0;
        if val < 0.0 {
            val += 360.0;
        }
        val
    }

    /// Convert to degrees without normalization (raw value).
    /// Use this for conic gradients where 360.0 is a meaningful distinct value from 0.0.
    #[inline]
    #[must_use] pub fn to_degrees_raw(&self) -> f32 {
        match self.metric {
            AngleMetric::Degree => self.number.get(),
            AngleMetric::Grad => self.number.get() / 400.0 * 360.0,
            AngleMetric::Radians => self.number.get().to_degrees(),
            AngleMetric::Turn => self.number.get() * 360.0,
            AngleMetric::Percent => self.number.get() / 100.0 * 360.0,
        }
    }
}

// -- Parser

/// Error returned when parsing a CSS angle value from a string.
#[derive(Clone, PartialEq, Eq)]
pub enum CssAngleValueParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str, AngleMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidAngle(&'a str),
}

impl_debug_as_display!(CssAngleValueParseError<'a>);
impl_display! { CssAngleValueParseError<'a>, {
    EmptyString => format!("Missing [rad / deg / turn / %] value"),
    NoValueGiven(input, metric) => format!("Expected floating-point angle value, got: \"{}{}\"", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
    InvalidAngle(s) => format!("Invalid angle value: \"{}\"", s),
}}

/// Wrapper for `NoValueGiven` error in angle parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct AngleNoValueGivenError {
    pub value: AzString,
    pub metric: AngleMetric,
}

/// Owned version of [`CssAngleValueParseError`] for FFI and storage.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssAngleValueParseErrorOwned {
    EmptyString,
    NoValueGiven(AngleNoValueGivenError),
    ValueParseErr(ParseFloatErrorWithInput),
    InvalidAngle(AzString),
}

impl CssAngleValueParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssAngleValueParseErrorOwned {
        match self {
            CssAngleValueParseError::EmptyString => CssAngleValueParseErrorOwned::EmptyString,
            CssAngleValueParseError::NoValueGiven(s, metric) => {
                CssAngleValueParseErrorOwned::NoValueGiven(AngleNoValueGivenError { value: (*s).to_string().into(), metric: *metric })
            }
            CssAngleValueParseError::ValueParseErr(err, s) => {
                CssAngleValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput { error: err.clone().into(), input: (*s).to_string().into() })
            }
            CssAngleValueParseError::InvalidAngle(s) => {
                CssAngleValueParseErrorOwned::InvalidAngle((*s).to_string().into())
            }
        }
    }
}

impl CssAngleValueParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssAngleValueParseError<'_> {
        match self {
            Self::EmptyString => CssAngleValueParseError::EmptyString,
            Self::NoValueGiven(e) => {
                CssAngleValueParseError::NoValueGiven(e.value.as_str(), e.metric)
            }
            Self::ValueParseErr(e) => {
                CssAngleValueParseError::ValueParseErr(e.error.to_std(), e.input.as_str())
            }
            Self::InvalidAngle(s) => {
                CssAngleValueParseError::InvalidAngle(s.as_str())
            }
        }
    }
}

/// Parse a CSS angle value string (e.g. `"90deg"`, `"1.57rad"`, `"0.5turn"`, `"50%"`).
/// A bare number without a unit suffix is interpreted as degrees.
#[cfg(feature = "parser")]
pub fn parse_angle_value(input: &str) -> Result<AngleValue, CssAngleValueParseError<'_>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssAngleValueParseError::EmptyString);
    }

    let match_values = &[
        ("deg", AngleMetric::Degree),
        ("turn", AngleMetric::Turn),
        ("grad", AngleMetric::Grad),
        ("rad", AngleMetric::Radians),
        ("%", AngleMetric::Percent),
    ];

    for (match_val, metric) in match_values {
        if input.ends_with(match_val) {
            let value = &input[..input.len() - match_val.len()];
            let value = value.trim();
            if value.is_empty() {
                return Err(CssAngleValueParseError::NoValueGiven(input, *metric));
            }
            match value.parse::<f32>() {
                Ok(o) => return Ok(AngleValue::from_metric(*metric, o)),
                Err(e) => return Err(CssAngleValueParseError::ValueParseErr(e, value)),
            }
        }
    }

    match input.parse::<f32>() {
        Ok(o) => Ok(AngleValue::from_metric(AngleMetric::Degree, o)), // bare number is degrees
        Err(_) => Err(CssAngleValueParseError::InvalidAngle(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_angle_value_deg() {
        assert_eq!(parse_angle_value("90deg").unwrap(), AngleValue::deg(90.0));
        assert_eq!(
            parse_angle_value("-45.5deg").unwrap(),
            AngleValue::deg(-45.5)
        );
        // Bare number defaults to degrees
        assert_eq!(parse_angle_value("180").unwrap(), AngleValue::deg(180.0));
    }

    #[test]
    fn test_parse_angle_value_rad() {
        assert_eq!(parse_angle_value("1.57rad").unwrap(), AngleValue::rad(1.57));
        assert_eq!(
            parse_angle_value(" -3.14rad ").unwrap(),
            AngleValue::rad(-3.14)
        );
    }

    #[test]
    fn test_parse_angle_value_grad() {
        assert_eq!(
            parse_angle_value("100grad").unwrap(),
            AngleValue::grad(100.0)
        );
        assert_eq!(
            parse_angle_value("400grad").unwrap(),
            AngleValue::grad(400.0)
        );
    }

    #[test]
    fn test_parse_angle_value_turn() {
        assert_eq!(
            parse_angle_value("0.25turn").unwrap(),
            AngleValue::turn(0.25)
        );
        assert_eq!(parse_angle_value("1turn").unwrap(), AngleValue::turn(1.0));
    }

    #[test]
    fn test_parse_angle_value_percent() {
        assert_eq!(parse_angle_value("50%").unwrap(), AngleValue::percent(50.0));
    }

    #[test]
    fn test_parse_angle_value_errors() {
        assert!(parse_angle_value("").is_err());
        assert!(parse_angle_value("deg").is_err());
        assert!(parse_angle_value("90 degs").is_err());
        assert!(parse_angle_value("ninety-deg").is_err());
        assert!(parse_angle_value("1.57 rads").is_err());
    }

    #[test]
    fn test_to_degrees_conversion() {
        assert_eq!(AngleValue::deg(90.0).to_degrees(), 90.0);
        // Use 0.1 tolerance due to FloatValue fixed-point precision (multiplier = 1000.0)
        assert!((AngleValue::rad(core::f32::consts::PI).to_degrees() - 180.0).abs() < 0.1);
        assert_eq!(AngleValue::grad(100.0).to_degrees(), 90.0);
        assert_eq!(AngleValue::turn(0.5).to_degrees(), 180.0);
        assert_eq!(AngleValue::deg(-90.0).to_degrees(), 270.0);
        assert_eq!(AngleValue::deg(450.0).to_degrees(), 90.0);
    }
}
