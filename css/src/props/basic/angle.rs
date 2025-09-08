//! CSS angle value types and parsing

use alloc::{format, string::String};
use core::fmt;

use crate::{
    error::CssAngleValueParseError,
    props::{basic::value::FloatValue, formatter::FormatAsCssValue},
};

/// Represents an angle with its unit
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
            number: FloatValue::const_new(0),
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
#[cfg(feature = "parser")]
pub fn parse_angle_value<'a>(input: &'a str) -> Result<AngleValue, CssAngleValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssAngleValueParseError::NoUnit(input));
    }

    // Find the unit suffix
    let (number_str, metric) = if input.ends_with("deg") {
        (&input[..input.len() - 3], AngleMetric::Deg)
    } else if input.ends_with("grad") {
        // Note: has to be before checking for "rad"
        (&input[..input.len() - 4], AngleMetric::Grad)
    } else if input.ends_with("rad") {
        (&input[..input.len() - 3], AngleMetric::Rad)
    } else if input.ends_with("turn") {
        (&input[..input.len() - 4], AngleMetric::Turn)
    } else if let Ok(num) = input.parse::<f32>() {
        // CSS allows unitless values
        return Ok(AngleValue::new(num, AngleMetric::Deg));
    } else {
        return Err(CssAngleValueParseError::NoUnit(input));
    };

    let number = number_str
        .trim()
        .parse::<f32>()
        .map_err(|_| CssAngleValueParseError::InvalidNumber(input))?;

    Ok(AngleValue::new(number, metric))
}

#[cfg(test)]
mod tests {
    use core::f32::consts::PI;

    use super::*;

    // Helper for float comparisons
    fn assert_approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-6, "Expected {} to be close to {}", a, b);
    }

    // --- Tests for parse_angle_value ---

    #[test]
    fn test_parse_valid_angles() {
        assert_eq!(
            parse_angle_value("90deg").unwrap(),
            AngleValue::new(90.0, AngleMetric::Deg)
        );
        assert_eq!(
            parse_angle_value("-45.5deg").unwrap(),
            AngleValue::new(-45.5, AngleMetric::Deg)
        );
        assert_eq!(
            parse_angle_value("1.57rad").unwrap(),
            AngleValue::new(1.57, AngleMetric::Rad)
        );
        assert_eq!(
            parse_angle_value("200grad").unwrap(),
            AngleValue::new(200.0, AngleMetric::Grad)
        );
        assert_eq!(
            parse_angle_value("0.25turn").unwrap(),
            AngleValue::new(0.25, AngleMetric::Turn)
        );
    }

    #[test]
    fn test_parse_with_whitespace() {
        assert_eq!(
            parse_angle_value("  180deg  ").unwrap(),
            AngleValue::new(180.0, AngleMetric::Deg)
        );
        assert_eq!(
            parse_angle_value(" 1.2 rad ").unwrap(),
            AngleValue::new(1.2, AngleMetric::Rad)
        );
    }

    #[test]
    fn test_parse_invalid_number() {
        assert!(matches!(
            parse_angle_value("not-a-number-deg"),
            Err(CssAngleValueParseError::InvalidNumber(_))
        ));
        assert!(matches!(
            parse_angle_value("1.2.3rad"),
            Err(CssAngleValueParseError::InvalidNumber(_))
        ));
        assert!(matches!(
            parse_angle_value("deg"),
            Err(CssAngleValueParseError::InvalidNumber(_))
        ));
    }

    #[test]
    fn test_parse_no_unit() {
        assert!(matches!(
            parse_angle_value("123"),
            Err(CssAngleValueParseError::NoUnit(_))
        ));
        assert!(matches!(
            parse_angle_value("45de"),
            Err(CssAngleValueParseError::NoUnit(_))
        ));
    }

    #[test]
    fn test_parse_empty_input() {
        // The current implementation returns NoUnit, which is acceptable.
        assert!(matches!(
            parse_angle_value(""),
            Err(CssAngleValueParseError::NoUnit(_))
        ));
        assert!(matches!(
            parse_angle_value("   "),
            Err(CssAngleValueParseError::NoUnit(_))
        ));
    }

    // This test will FAIL with the current implementation due to the ordering bug.
    // It demonstrates the critique point.
    #[test]
    fn test_parse_grad_not_rad() {
        let result = parse_angle_value("200grad");
        // With the bug, it tries to parse "200g" as a number for `rad`, which fails.
        // A fixed implementation should pass this assert.
        assert_eq!(result.unwrap(), AngleValue::new(200.0, AngleMetric::Grad));
    }

    // --- Tests for AngleValue methods ---

    #[test]
    fn test_to_degrees() {
        assert_approx_eq(AngleValue::new(90.0, AngleMetric::Deg).to_degrees(), 90.0);
        assert_approx_eq(AngleValue::new(PI, AngleMetric::Rad).to_degrees(), 180.0);
        assert_approx_eq(
            AngleValue::new(200.0, AngleMetric::Grad).to_degrees(),
            180.0,
        );
        assert_approx_eq(AngleValue::new(0.5, AngleMetric::Turn).to_degrees(), 180.0);
    }

    #[test]
    fn test_to_radians() {
        assert_approx_eq(AngleValue::new(180.0, AngleMetric::Deg).to_radians(), PI);
        assert_approx_eq(
            AngleValue::new(PI / 2.0, AngleMetric::Rad).to_radians(),
            PI / 2.0,
        );
        assert_approx_eq(
            AngleValue::new(100.0, AngleMetric::Grad).to_radians(),
            PI / 2.0,
        );
        assert_approx_eq(
            AngleValue::new(1.0, AngleMetric::Turn).to_radians(),
            2.0 * PI,
        );
    }

    #[test]
    fn test_normalize() {
        // Positive value, needs wrapping
        let angle1 = AngleValue::new(450.0, AngleMetric::Deg); // 450deg = 90deg
        assert_approx_eq(angle1.normalize().number.get(), 90.0);

        // Negative value
        let angle2 = AngleValue::new(-90.0, AngleMetric::Deg); // -90deg = 270deg
        assert_approx_eq(angle2.normalize().number.get(), 270.0);

        // Value within range
        let angle3 = AngleValue::new(180.0, AngleMetric::Deg);
        assert_approx_eq(angle3.normalize().number.get(), 180.0);

        // Large negative value
        let angle4 = AngleValue::new(-750.0, AngleMetric::Grad); // -750 grad = -675 deg. -675 % 360 = -315. -315 + 360 = 45.
        assert_approx_eq(angle4.normalize().number.get(), 45.0);
    }

    #[test]
    fn test_display_and_format() {
        let angle = AngleValue::new(-12.5, AngleMetric::Turn);
        assert_eq!(angle.to_string(), "-12.5turn");
        assert_eq!(angle.format_as_css_value(), "-12.5turn");
    }

    #[test]
    fn test_default_is_zero_deg() {
        assert_eq!(
            AngleValue::default(),
            AngleValue::new(0.0, AngleMetric::Deg)
        );
        assert_eq!(AngleValue::default(), AngleValue::zero());
    }
}
