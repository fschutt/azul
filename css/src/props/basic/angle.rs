//! CSS property types for angles (degrees, radians, etc.).

use alloc::string::{String, ToString};
use core::{fmt, num::ParseFloatError};
use crate::corety::AzString;

use crate::props::basic::error::ParseFloatErrorWithInput;

use crate::props::{basic::length::FloatValue, formatter::PrintAsCssValue};

/// Enum representing the metric associated with an angle (deg, rad, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AngleMetric {
    Degree,
    Radians,
    Grad,
    Turn,
    Percent,
}

impl Default for AngleMetric {
    fn default() -> AngleMetric {
        AngleMetric::Degree
    }
}

impl fmt::Display for AngleMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AngleMetric::*;
        match self {
            Degree => write!(f, "deg"),
            Radians => write!(f, "rad"),
            Grad => write!(f, "grad"),
            Turn => write!(f, "turn"),
            Percent => write!(f, "%"),
        }
    }
}

/// FloatValue, but associated with a certain metric (i.e. deg, rad, etc.)
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl PrintAsCssValue for AngleValue {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl AngleValue {
    #[inline]
    pub const fn zero() -> Self {
        const ZERO_DEG: AngleValue = AngleValue::const_deg(0);
        ZERO_DEG
    }

    #[inline]
    pub const fn const_deg(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Degree, value)
    }

    #[inline]
    pub const fn const_rad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Radians, value)
    }

    #[inline]
    pub const fn const_grad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Grad, value)
    }

    #[inline]
    pub const fn const_turn(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Turn, value)
    }

    #[inline]
    pub fn const_percent(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Percent, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    #[inline]
    pub fn deg(value: f32) -> Self {
        Self::from_metric(AngleMetric::Degree, value)
    }

    #[inline]
    pub fn rad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Radians, value)
    }

    #[inline]
    pub fn grad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Grad, value)
    }

    #[inline]
    pub fn turn(value: f32) -> Self {
        Self::from_metric(AngleMetric::Turn, value)
    }

    #[inline]
    pub fn percent(value: f32) -> Self {
        Self::from_metric(AngleMetric::Percent, value)
    }

    #[inline]
    pub fn from_metric(metric: AngleMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    /// Convert to degrees, normalized to [0, 360) range.
    /// Note: 360.0 becomes 0.0 due to modulo operation.
    /// For conic gradients where 360.0 is meaningful, use `to_degrees_raw()`.
    #[inline]
    pub fn to_degrees(&self) -> f32 {
        let val = match self.metric {
            AngleMetric::Degree => self.number.get(),
            AngleMetric::Grad => self.number.get() / 400.0 * 360.0,
            AngleMetric::Radians => self.number.get() * 180.0 / core::f32::consts::PI,
            AngleMetric::Turn => self.number.get() * 360.0,
            AngleMetric::Percent => self.number.get() / 100.0 * 360.0,
        };

        let mut val = val % 360.0;
        if val < 0.0 {
            val = 360.0 + val;
        }
        val
    }

    /// Convert to degrees without normalization (raw value).
    /// Use this for conic gradients where 360.0 is a meaningful distinct value from 0.0.
    #[inline]
    pub fn to_degrees_raw(&self) -> f32 {
        match self.metric {
            AngleMetric::Degree => self.number.get(),
            AngleMetric::Grad => self.number.get() / 400.0 * 360.0,
            AngleMetric::Radians => self.number.get() * 180.0 / core::f32::consts::PI,
            AngleMetric::Turn => self.number.get() * 360.0,
            AngleMetric::Percent => self.number.get() / 100.0 * 360.0,
        }
    }
}

// -- Parser

#[derive(Clone, PartialEq)]
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

/// Wrapper for NoValueGiven error in angle parsing.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct AngleNoValueGivenError {
    pub value: String,
    pub metric: AngleMetric,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssAngleValueParseErrorOwned {
    EmptyString,
    NoValueGiven(AngleNoValueGivenError),
    ValueParseErr(ParseFloatErrorWithInput),
    InvalidAngle(AzString),
}

impl<'a> CssAngleValueParseError<'a> {
    pub fn to_contained(&self) -> CssAngleValueParseErrorOwned {
        match self {
            CssAngleValueParseError::EmptyString => CssAngleValueParseErrorOwned::EmptyString,
            CssAngleValueParseError::NoValueGiven(s, metric) => {
                CssAngleValueParseErrorOwned::NoValueGiven(AngleNoValueGivenError { value: s.to_string(), metric: *metric })
            }
            CssAngleValueParseError::ValueParseErr(err, s) => {
                CssAngleValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput { error: err.clone(), input: s.to_string() })
            }
            CssAngleValueParseError::InvalidAngle(s) => {
                CssAngleValueParseErrorOwned::InvalidAngle(s.to_string().into())
            }
        }
    }
}

impl CssAngleValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssAngleValueParseError<'a> {
        match self {
            CssAngleValueParseErrorOwned::EmptyString => CssAngleValueParseError::EmptyString,
            CssAngleValueParseErrorOwned::NoValueGiven(e) => {
                CssAngleValueParseError::NoValueGiven(e.value.as_str(), e.metric)
            }
            CssAngleValueParseErrorOwned::ValueParseErr(e) => {
                CssAngleValueParseError::ValueParseErr(e.error.clone(), e.input.as_str())
            }
            CssAngleValueParseErrorOwned::InvalidAngle(s) => {
                CssAngleValueParseError::InvalidAngle(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_angle_value<'a>(input: &'a str) -> Result<AngleValue, CssAngleValueParseError<'a>> {
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
