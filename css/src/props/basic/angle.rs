//! CSS angle value types and parsing

use crate::error::CssAngleValueParseError;
use crate::props::basic::value::FloatValue;
use crate::props::formatter::FormatAsCssValue;
use alloc::{format, string::String};
use core::fmt;

/// Represents an angle with its unit
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct AngleValue {
    pub metric: AngleMetric,
    pub number: FloatValue,
}

/// Angle units for CSS angle values
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AngleMetric {
    /// Degrees (360 degrees = full circle)
    Deg,
    /// Radians (2π radians = full circle)
    Rad,
    /// Gradians (400 gradians = full circle)
    Grad,
    /// Turns (1 turn = full circle)
    Turn,
}

impl Default for AngleValue {
    fn default() -> Self {
        Self {
            metric: AngleMetric::Deg,
            number: FloatValue::new(0.0),
        }
    }
}

impl fmt::Display for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl fmt::Display for AngleMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AngleMetric::*;
        match self {
            Deg => write!(f, "deg"),
            Rad => write!(f, "rad"),
            Grad => write!(f, "grad"),
            Turn => write!(f, "turn"),
        }
    }
}

impl AngleValue {
    pub fn new(number: f32, metric: AngleMetric) -> Self {
        Self {
            metric,
            number: FloatValue::new(number),
        }
    }

    pub const fn zero() -> Self {
        Self {
            metric: AngleMetric::Deg,
            number: FloatValue::new(0.0),
        }
    }

    pub fn to_degrees(&self) -> f32 {
        let val = self.number.get();
        match self.metric {
            AngleMetric::Deg => val,
            AngleMetric::Rad => val * 180.0 / core::f32::consts::PI,
            AngleMetric::Grad => val * 0.9, // 400 grad = 360 deg
            AngleMetric::Turn => val * 360.0,
        }
    }

    pub fn to_radians(&self) -> f32 {
        let val = self.number.get();
        match self.metric {
            AngleMetric::Deg => val * core::f32::consts::PI / 180.0,
            AngleMetric::Rad => val,
            AngleMetric::Grad => val * core::f32::consts::PI / 200.0, // 400 grad = 2π rad
            AngleMetric::Turn => val * 2.0 * core::f32::consts::PI,
        }
    }

    pub fn normalize(&self) -> Self {
        let degrees = self.to_degrees() % 360.0;
        let normalized_degrees = if degrees < 0.0 {
            degrees + 360.0
        } else {
            degrees
        };
        Self::new(normalized_degrees, AngleMetric::Deg)
    }
}

impl FormatAsCssValue for AngleValue {
    fn format_as_css_value(&self) -> String {
        format!("{}{}", self.number.get(), self.metric)
    }
}

/// Parse a CSS angle value (e.g., "45deg", "1.5rad", "100grad", "0.25turn")
pub fn parse_angle_value<'a>(input: &'a str) -> Result<AngleValue, CssAngleValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssAngleValueParseError::NoUnit(input));
    }

    // Find the unit suffix
    let (number_str, metric) = if input.ends_with("deg") {
        (&input[..input.len() - 3], AngleMetric::Deg)
    } else if input.ends_with("rad") {
        (&input[..input.len() - 3], AngleMetric::Rad)
    } else if input.ends_with("grad") {
        (&input[..input.len() - 4], AngleMetric::Grad)
    } else if input.ends_with("turn") {
        (&input[..input.len() - 4], AngleMetric::Turn)
    } else {
        return Err(CssAngleValueParseError::NoUnit(input));
    };

    let number = number_str
        .trim()
        .parse::<f32>()
        .map_err(|_| CssAngleValueParseError::InvalidNumber(input))?;

    Ok(AngleValue::new(number, metric))
}
