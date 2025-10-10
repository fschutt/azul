//! CSS property types for angles (degrees, radians, etc.).

use alloc::string::{String, ToString};
use core::{fmt, num::ParseFloatError};

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

    #[inline]
    pub fn to_degrees(&self) -> f32 {
        let val = match self.metric {
            AngleMetric::Degree => self.number.get(),
            AngleMetric::Grad => self.number.get() / 400.0 * 360.0,
            AngleMetric::Radians => self.number.get().to_degrees(),
            AngleMetric::Turn => self.number.get() * 360.0,
            AngleMetric::Percent => self.number.get() / 100.0 * 360.0,
        };

        let mut val = val % 360.0;
        if val < 0.0 {
            val = 360.0 + val;
        }
        val
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

#[derive(Debug, Clone, PartialEq)]
pub enum CssAngleValueParseErrorOwned {
    EmptyString,
    NoValueGiven(String, AngleMetric),
    ValueParseErr(ParseFloatError, String),
    InvalidAngle(String),
}

impl<'a> CssAngleValueParseError<'a> {
    pub fn to_contained(&self) -> CssAngleValueParseErrorOwned {
        match self {
            CssAngleValueParseError::EmptyString => CssAngleValueParseErrorOwned::EmptyString,
            CssAngleValueParseError::NoValueGiven(s, metric) => {
                CssAngleValueParseErrorOwned::NoValueGiven(s.to_string(), *metric)
            }
            CssAngleValueParseError::ValueParseErr(err, s) => {
                CssAngleValueParseErrorOwned::ValueParseErr(err.clone(), s.to_string())
            }
            CssAngleValueParseError::InvalidAngle(s) => {
                CssAngleValueParseErrorOwned::InvalidAngle(s.to_string())
            }
        }
    }
}

impl CssAngleValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssAngleValueParseError<'a> {
        match self {
            CssAngleValueParseErrorOwned::EmptyString => CssAngleValueParseError::EmptyString,
            CssAngleValueParseErrorOwned::NoValueGiven(s, metric) => {
                CssAngleValueParseError::NoValueGiven(s.as_str(), *metric)
            }
            CssAngleValueParseErrorOwned::ValueParseErr(err, s) => {
                CssAngleValueParseError::ValueParseErr(err.clone(), s.as_str())
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
