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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `angle-value` value.
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
        if let Some(value) = input.strip_suffix(match_val) {
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

    // bare number is degrees
    input.parse::<f32>().map_or_else(
        |_| Err(CssAngleValueParseError::InvalidAngle(input)),
        |o| Ok(AngleValue::from_metric(AngleMetric::Degree, o)),
    )
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    // Tests assert parsed values equal the exact source literals; the rad inputs
    // (1.57, 3.14) are literal test data, not approximations of FRAC_PI_2/PI.
    #![allow(clippy::float_cmp, clippy::approx_constant)]
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

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
mod autotest_generated {
    use super::*;
    use crate::props::basic::error::{
        ParseFloatError as FfiParseFloatError, ParseFloatErrorWithInput,
    };

    /// `FloatValue` stores `f32 * 1000` truncated into an `isize`.
    const MULT: isize = 1000;

    /// Every `AngleMetric` variant, for exhaustive sweeps.
    const ALL_METRICS: [AngleMetric; 5] = [
        AngleMetric::Degree,
        AngleMetric::Radians,
        AngleMetric::Grad,
        AngleMetric::Turn,
        AngleMetric::Percent,
    ];

    // -------------------------------------------------------------------
    // serializers (Display / PrintAsCssValue)
    // -------------------------------------------------------------------

    #[test]
    fn autotest_angle_metric_display_is_non_empty_and_exact() {
        assert_eq!(AngleMetric::Degree.to_string(), "deg");
        assert_eq!(AngleMetric::Radians.to_string(), "rad");
        assert_eq!(AngleMetric::Grad.to_string(), "grad");
        assert_eq!(AngleMetric::Turn.to_string(), "turn");
        assert_eq!(AngleMetric::Percent.to_string(), "%");
        assert_eq!(AngleMetric::default(), AngleMetric::Degree);
        for m in ALL_METRICS {
            assert!(!m.to_string().is_empty(), "empty unit string for {m:?}");
        }
    }

    #[test]
    fn autotest_angle_value_display_default_and_zero() {
        assert_eq!(AngleValue::default().to_string(), "0deg");
        assert_eq!(AngleValue::zero().to_string(), "0deg");
        // Debug delegates to Display.
        assert_eq!(format!("{:?}", AngleValue::zero()), "0deg");
        assert_eq!(AngleValue::zero().print_as_css_value(), "0deg");
    }

    #[test]
    fn autotest_angle_value_display_never_emits_inf_or_nan() {
        // Saturating/clamping happens inside FloatValue, so no non-finite value
        // can ever reach the formatter -- assert the serializer stays CSS-safe.
        for m in ALL_METRICS {
            for v in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                f32::MIN_POSITIVE,
                -0.0,
            ] {
                let s = AngleValue::from_metric(m, v).to_string();
                assert!(!s.is_empty(), "empty serialization for {m:?} / {v}");
                assert!(!s.contains("inf"), "serialized infinity: {s}");
                assert!(!s.contains("NaN"), "serialized NaN: {s}");
                assert!(s.ends_with(&m.to_string()), "lost the unit suffix: {s}");
            }
        }
    }

    #[test]
    fn autotest_angle_value_nan_serializes_as_zero() {
        // NaN collapses to 0 (the `as isize` cast maps NaN -> 0), it is not preserved.
        assert_eq!(AngleValue::deg(f32::NAN).to_string(), "0deg");
        assert_eq!(AngleValue::rad(f32::NAN).to_string(), "0rad");
    }

    // -------------------------------------------------------------------
    // constructors
    // -------------------------------------------------------------------

    #[test]
    fn autotest_zero_is_the_neutral_element() {
        let z = AngleValue::zero();
        assert_eq!(z, AngleValue::default());
        assert_eq!(z, AngleValue::const_deg(0));
        assert_eq!(z, AngleValue::deg(0.0));
        assert_eq!(z.metric, AngleMetric::Degree);
        assert_eq!(z.number.number(), 0);
        assert_eq!(z.number.get(), 0.0);
        assert_eq!(z.to_degrees(), 0.0);
        assert_eq!(z.to_degrees_raw(), 0.0);
    }

    #[test]
    fn autotest_from_metric_fields_match_args() {
        for m in ALL_METRICS {
            let a = AngleValue::from_metric(m, 12.5);
            assert_eq!(a.metric, m);
            assert_eq!(a.number.get(), 12.5);
            assert_eq!(a.number.number(), 12_500);
        }
        // The per-metric helpers must agree with from_metric.
        assert_eq!(
            AngleValue::deg(1.5),
            AngleValue::from_metric(AngleMetric::Degree, 1.5)
        );
        assert_eq!(
            AngleValue::rad(1.5),
            AngleValue::from_metric(AngleMetric::Radians, 1.5)
        );
        assert_eq!(
            AngleValue::grad(1.5),
            AngleValue::from_metric(AngleMetric::Grad, 1.5)
        );
        assert_eq!(
            AngleValue::turn(1.5),
            AngleValue::from_metric(AngleMetric::Turn, 1.5)
        );
        assert_eq!(
            AngleValue::percent(1.5),
            AngleValue::from_metric(AngleMetric::Percent, 1.5)
        );
    }

    // -------------------------------------------------------------------
    // numeric: const constructors (isize -> fixed point)
    // -------------------------------------------------------------------

    #[test]
    fn autotest_const_ctors_zero_negative_and_metric() {
        for (built, metric) in [
            (AngleValue::const_deg(0), AngleMetric::Degree),
            (AngleValue::const_rad(0), AngleMetric::Radians),
            (AngleValue::const_grad(0), AngleMetric::Grad),
            (AngleValue::const_turn(0), AngleMetric::Turn),
            (AngleValue::const_percent(0), AngleMetric::Percent),
        ] {
            assert_eq!(built.metric, metric);
            assert_eq!(built.number.number(), 0);
        }
        assert_eq!(AngleValue::const_deg(-90).number.get(), -90.0);
        assert_eq!(AngleValue::const_rad(-3).number.number(), -3 * MULT);
        assert_eq!(AngleValue::const_turn(-1).to_degrees_raw(), -360.0);
    }

    #[test]
    fn autotest_const_from_metric_matches_specific_ctors() {
        for (m, specific) in [
            (AngleMetric::Degree, AngleValue::const_deg(7)),
            (AngleMetric::Radians, AngleValue::const_rad(7)),
            (AngleMetric::Grad, AngleValue::const_grad(7)),
            (AngleMetric::Turn, AngleValue::const_turn(7)),
            (AngleMetric::Percent, AngleValue::const_percent(7)),
        ] {
            assert_eq!(AngleValue::const_from_metric(m, 7), specific);
            assert_eq!(specific.number.number(), 7 * MULT);
        }
    }

    #[test]
    fn autotest_const_ctors_at_safe_isize_boundary() {
        // const_new multiplies by 1000, so |value| <= isize::MAX / 1000 is the
        // largest magnitude that cannot overflow. Assert exactness right at the edge.
        const MAX_SAFE: isize = isize::MAX / MULT;
        const MIN_SAFE: isize = isize::MIN / MULT;

        assert_eq!(
            AngleValue::const_deg(MAX_SAFE).number.number(),
            MAX_SAFE * MULT
        );
        assert_eq!(
            AngleValue::const_deg(MIN_SAFE).number.number(),
            MIN_SAFE * MULT
        );
        // ...and the round-trip back to f32 stays finite (no inf leaking into layout).
        assert!(AngleValue::const_deg(MAX_SAFE).number.get().is_finite());
        assert!(AngleValue::const_deg(MIN_SAFE).number.get().is_finite());
        assert!(AngleValue::const_turn(MAX_SAFE).to_degrees().is_finite());
        assert!(AngleValue::const_turn(MIN_SAFE).to_degrees().is_finite());
    }

    #[test]
    fn autotest_const_deg_isize_max_overflows_unchecked() {
        // Documents (does not bless) the unchecked `value * 1000` in FloatValue::const_new:
        // isize::MAX degrees panics on overflow in debug and wraps in release. Both are
        // accepted here; what must NOT happen is a silently plausible-looking angle.
        // black_box keeps const-propagation from turning this into a compile-time error.
        let huge = core::hint::black_box(isize::MAX);
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let res = std::panic::catch_unwind(move || AngleValue::const_deg(huge).number.number());
        std::panic::set_hook(prev);

        match res {
            Err(_) => {} // debug build: "attempt to multiply with overflow"
            Ok(n) => assert_eq!(
                n,
                isize::MAX.wrapping_mul(MULT),
                "release build must wrap, not produce a sanitized value"
            ),
        }
    }

    #[test]
    fn autotest_const_from_metric_fractional_digit_truncation() {
        let f = |pre, post| {
            AngleValue::const_from_metric_fractional(AngleMetric::Degree, pre, post)
                .number
                .number()
        };
        assert_eq!(f(0, 0), 0);
        assert_eq!(f(45, 5), 45_500); // 45.5
        assert_eq!(f(0, 83), 830); // 0.83
        assert_eq!(f(1, 523), 1_523); // 1.523
        // More than 3 fractional digits: truncated (not rounded) to 3.
        assert_eq!(f(2, 123456), 2_123); // 2.123456 -> 2.123, per the doc comment
        assert_eq!(f(0, 999_999_999), 999); // 0.999999999 -> 0.999
        assert_eq!(
            AngleValue::const_from_metric_fractional(AngleMetric::Turn, 0, 25).metric,
            AngleMetric::Turn
        );
    }

    #[test]
    fn autotest_const_fractional_sign_handling_and_negative_zero_trap() {
        let deg = |pre, post| {
            AngleValue::const_from_metric_fractional(AngleMetric::Degree, pre, post)
                .number
                .get()
        };
        assert_eq!(deg(-1, 5), -1.5); // negative pre drags the fraction negative
        assert_eq!(deg(0, -5), -0.5); // negative post encodes a negative fraction
        assert_eq!(deg(-1, -5), -1.5); // both negative must not double-negate
        // TRAP: isize has no -0, so `-0` is `0` and the sign is lost. -0.5deg is NOT
        // expressible as (-0, 5); it yields +0.5deg. Callers must use (0, -5).
        assert_eq!(deg(-0, 5), 0.5);
        assert_ne!(deg(-0, 5), -0.5);
    }

    // -------------------------------------------------------------------
    // numeric: f32 constructors (saturation / NaN / sub-precision)
    // -------------------------------------------------------------------

    #[test]
    fn autotest_f32_ctor_nan_collapses_to_zero() {
        for m in ALL_METRICS {
            let a = AngleValue::from_metric(m, f32::NAN);
            assert_eq!(a.number.number(), 0, "NaN did not clamp to 0 for {m:?}");
            assert_eq!(a.number.get(), 0.0);
            assert!(!a.number.get().is_nan());
            assert_eq!(a.to_degrees(), 0.0);
            assert_eq!(a.to_degrees_raw(), 0.0);
            // Consequence worth knowing: NaN is *equal* to zero after construction.
            assert_eq!(a, AngleValue::from_metric(m, 0.0));
        }
    }

    #[test]
    fn autotest_f32_ctor_infinities_saturate_to_isize_bounds() {
        assert_eq!(AngleValue::deg(f32::INFINITY).number.number(), isize::MAX);
        assert_eq!(
            AngleValue::deg(f32::NEG_INFINITY).number.number(),
            isize::MIN
        );
        // f32::MAX * 1000 overflows to +inf before the cast, so it saturates too.
        assert_eq!(AngleValue::deg(f32::MAX).number.number(), isize::MAX);
        assert_eq!(AngleValue::deg(f32::MIN).number.number(), isize::MIN);
        for m in ALL_METRICS {
            for v in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
                let a = AngleValue::from_metric(m, v);
                assert!(a.number.get().is_finite(), "{m:?} / {v} leaked a non-finite");
            }
        }
    }

    #[test]
    fn autotest_f32_ctor_truncates_toward_zero_below_precision() {
        // 3 decimal digits of precision; the 4th digit is truncated, not rounded,
        // and truncation is toward zero on both sides of 0.
        assert_eq!(AngleValue::deg(1.9999).number.number(), 1_999);
        assert_eq!(AngleValue::deg(-1.9999).number.number(), -1_999);
        assert_eq!(AngleValue::deg(0.0004).number.number(), 0);
        assert_eq!(AngleValue::deg(-0.0004).number.number(), 0);
        assert_eq!(AngleValue::deg(f32::EPSILON).number.number(), 0);
        assert_eq!(AngleValue::deg(f32::MIN_POSITIVE).number.number(), 0);
        // -0.0 must not become a negative encoded value.
        assert_eq!(AngleValue::deg(-0.0).number.number(), 0);
        assert_eq!(AngleValue::deg(-0.0), AngleValue::deg(0.0));
    }

    // -------------------------------------------------------------------
    // getters: to_degrees / to_degrees_raw
    // -------------------------------------------------------------------

    #[test]
    fn autotest_to_degrees_known_conversions() {
        assert_eq!(AngleValue::deg(90.0).to_degrees(), 90.0);
        assert_eq!(AngleValue::grad(100.0).to_degrees(), 90.0);
        assert_eq!(AngleValue::turn(0.25).to_degrees(), 90.0);
        assert_eq!(AngleValue::percent(50.0).to_degrees(), 180.0);
        assert_eq!(AngleValue::percent(25.0).to_degrees_raw(), 90.0);
        // rad goes through the 1/1000 quantization, so compare with a tolerance.
        assert!((AngleValue::rad(core::f32::consts::FRAC_PI_2).to_degrees() - 90.0).abs() < 0.1);
    }

    #[test]
    fn autotest_to_degrees_normalizes_but_raw_does_not() {
        // A full turn normalizes to 0 -- the documented 360 -> 0 collapse.
        assert_eq!(AngleValue::deg(360.0).to_degrees(), 0.0);
        assert_eq!(AngleValue::turn(1.0).to_degrees(), 0.0);
        assert_eq!(AngleValue::grad(400.0).to_degrees(), 0.0);
        assert_eq!(AngleValue::percent(100.0).to_degrees(), 0.0);
        // ...while the raw variant keeps 360 distinct from 0 (conic-gradient case).
        assert_eq!(AngleValue::deg(360.0).to_degrees_raw(), 360.0);
        assert_eq!(AngleValue::turn(1.0).to_degrees_raw(), 360.0);
        assert_eq!(AngleValue::grad(400.0).to_degrees_raw(), 360.0);
        assert_eq!(AngleValue::percent(100.0).to_degrees_raw(), 360.0);

        // Negative and out-of-range wrap into [0, 360).
        assert_eq!(AngleValue::deg(-90.0).to_degrees(), 270.0);
        assert_eq!(AngleValue::deg(-450.0).to_degrees(), 270.0);
        assert_eq!(AngleValue::deg(-0.5).to_degrees(), 359.5);
        assert_eq!(AngleValue::deg(720.0).to_degrees(), 0.0);
        assert_eq!(AngleValue::deg(450.0).to_degrees_raw(), 450.0);
        assert_eq!(AngleValue::turn(-2.0).to_degrees(), 0.0);
    }

    #[test]
    fn autotest_to_degrees_on_saturated_values_stays_finite_and_in_range() {
        // The nastiest inputs the type can hold: isize::MAX / isize::MIN encodings,
        // pushed through every unit conversion. Must never produce inf/NaN and must
        // honour the documented [0, 360) contract.
        for m in ALL_METRICS {
            for v in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN, f32::NAN] {
                let a = AngleValue::from_metric(m, v);
                let raw = a.to_degrees_raw();
                let norm = a.to_degrees();
                assert!(raw.is_finite(), "to_degrees_raw not finite: {m:?} / {v}");
                assert!(norm.is_finite(), "to_degrees not finite: {m:?} / {v}");
                assert!(
                    (0.0..360.0).contains(&norm),
                    "to_degrees out of [0,360): {m:?} / {v} -> {norm}"
                );
            }
        }
    }

    #[test]
    fn autotest_ord_is_metric_first_not_semantic_angle() {
        // Ord derives on (metric, number): the unit dominates. 1000deg sorts BEFORE
        // 0rad even though it is the larger angle -- do not use Ord to compare angles.
        assert!(AngleValue::deg(1000.0) < AngleValue::rad(0.0));
        assert!(AngleValue::turn(0.0) < AngleValue::percent(0.0));
        // Within one metric the ordering is numeric, as expected.
        assert!(AngleValue::deg(-1.0) < AngleValue::deg(1.0));
        // Eq/Hash agree with each other (no NaN poisoning, since NaN clamps to 0).
        use core::hash::{Hash, Hasher};
        let h = |a: AngleValue| {
            let mut s = std::collections::hash_map::DefaultHasher::new();
            a.hash(&mut s);
            s.finish()
        };
        assert_eq!(AngleValue::deg(1.0), AngleValue::deg(1.0));
        assert_eq!(h(AngleValue::deg(1.0)), h(AngleValue::deg(1.0)));
        assert_eq!(h(AngleValue::deg(f32::NAN)), h(AngleValue::deg(0.0)));
        assert_ne!(AngleValue::deg(1.0), AngleValue::rad(1.0));
    }

    // -------------------------------------------------------------------
    // parser (feature-gated, mirrors the #[cfg(feature = "parser")] on the fn)
    // -------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_empty_and_whitespace_only() {
        for input in ["", "   ", "\t\n\r", "\u{a0}", " \u{2003} "] {
            assert!(
                matches!(
                    parse_angle_value(input),
                    Err(CssAngleValueParseError::EmptyString)
                ),
                "expected EmptyString for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_unit_without_number() {
        for (input, metric) in [
            ("deg", AngleMetric::Degree),
            ("rad", AngleMetric::Radians),
            ("grad", AngleMetric::Grad),
            ("turn", AngleMetric::Turn),
            ("%", AngleMetric::Percent),
            ("  deg  ", AngleMetric::Degree),
        ] {
            match parse_angle_value(input) {
                Err(CssAngleValueParseError::NoValueGiven(_, m)) => assert_eq!(m, metric),
                other => panic!("expected NoValueGiven for {input:?}, got {other:?}"),
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_garbage_is_rejected_without_panicking() {
        for input in [
            "ninety",
            "!!!",
            "90 degs",
            "1.57 rads",
            "90degdeg",
            "--90deg",
            "1_0deg",
            "90;garbage",
            "deg90",
            "%50",
            "0x1Fdeg",
            "+-1turn",
            "9 0deg",
            "\0deg",
        ] {
            let res = parse_angle_value(input);
            assert!(res.is_err(), "garbage accepted: {input:?} -> {res:?}");
            // Error formatting must not panic either (it interpolates the input).
            assert!(!format!("{}", res.unwrap_err()).is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_uppercase_units_are_rejected() {
        // CSS units are ASCII case-insensitive; this parser is case-SENSITIVE.
        // That is a spec deviation, but it fails closed (Err), never panics.
        for input in ["90DEG", "1RAD", "0.5TURN", "100GRAD", "90Deg"] {
            assert!(
                parse_angle_value(input).is_err(),
                "case-insensitive unit unexpectedly accepted: {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_accepts_whitespace_between_number_and_unit() {
        // Lenient vs. the CSS grammar (no whitespace allowed inside a dimension token):
        // the unit suffix is stripped first, then the remainder is trimmed.
        assert_eq!(parse_angle_value("90 deg").unwrap(), AngleValue::deg(90.0));
        assert_eq!(parse_angle_value("90\tdeg").unwrap(), AngleValue::deg(90.0));
        assert_eq!(
            parse_angle_value("50 %").unwrap(),
            AngleValue::percent(50.0)
        );
        assert_eq!(parse_angle_value(" 90deg ").unwrap(), AngleValue::deg(90.0));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_accepts_float_keywords_and_neutralizes_them() {
        // "NaN"/"inf" are valid f32 literals, so they slip past the parser. They must
        // at least end up as defined, finite angles rather than poisoning layout.
        let nan = parse_angle_value("NaN").expect("f32::from_str accepts NaN");
        assert_eq!(nan.number.number(), 0);
        assert!(!nan.to_degrees().is_nan());

        let nan_rad = parse_angle_value("nanrad").expect("f32::from_str accepts nan");
        assert_eq!(nan_rad.metric, AngleMetric::Radians);
        assert_eq!(nan_rad.number.number(), 0);

        let inf = parse_angle_value("inf").expect("f32::from_str accepts inf");
        assert_eq!(inf.number.number(), isize::MAX);
        assert!(inf.to_degrees().is_finite());

        let neg_inf = parse_angle_value("-infdeg").expect("f32::from_str accepts -inf");
        assert_eq!(neg_inf.number.number(), isize::MIN);
        assert!(neg_inf.to_degrees_raw().is_finite());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_boundary_numbers_saturate() {
        assert_eq!(parse_angle_value("0").unwrap(), AngleValue::deg(0.0));
        assert_eq!(parse_angle_value("-0").unwrap().number.number(), 0);
        assert_eq!(parse_angle_value("+90deg").unwrap(), AngleValue::deg(90.0));
        assert_eq!(parse_angle_value(".5turn").unwrap(), AngleValue::turn(0.5));
        // i64::MAX / i64::MIN as bare degrees: overflow the fixed-point encoding and
        // must saturate rather than wrap into a bogus small angle.
        assert_eq!(
            parse_angle_value("9223372036854775807").unwrap().number.number(),
            isize::MAX
        );
        assert_eq!(
            parse_angle_value("-9223372036854775808").unwrap().number.number(),
            isize::MIN
        );
        // f32 exponent overflow -> inf -> saturates; underflow -> 0.
        assert_eq!(parse_angle_value("1e40deg").unwrap().number.number(), isize::MAX);
        assert_eq!(parse_angle_value("1e-40deg").unwrap().number.number(), 0);
        assert_eq!(parse_angle_value("0.0001deg").unwrap().number.number(), 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_extremely_long_input_terminates() {
        // 100k digits: linear-time float parse, no hang, saturating result.
        let long_digits = "9".repeat(100_000) + "deg";
        assert_eq!(
            parse_angle_value(&long_digits).unwrap().number.number(),
            isize::MAX
        );

        // 100k leading zeros still denote 1.
        let padded = "0".repeat(100_000) + "1deg";
        assert_eq!(parse_angle_value(&padded).unwrap(), AngleValue::deg(1.0));

        // 100k junk bytes: rejected, not truncated into something valid.
        let long_junk = "a".repeat(100_000);
        assert!(parse_angle_value(&long_junk).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_deeply_nested_brackets_does_not_stack_overflow() {
        // The parser is non-recursive; 10k nested brackets must simply be rejected.
        let nested = "(".repeat(10_000);
        assert!(parse_angle_value(&nested).is_err());
        let nested_unit = "[".repeat(10_000) + "deg";
        assert!(parse_angle_value(&nested_unit).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_unicode_input_never_panics() {
        // Multibyte input must not be sliced on a non-char boundary anywhere.
        for input in [
            "°",
            "90°",
            "\u{1F600}",
            "\u{1F600}deg",
            "９０deg",          // fullwidth digits
            "9\u{0301}0deg",    // combining acute accent
            "٩٠%",              // arabic-indic digits
            "\u{200b}90deg",    // zero-width space (not trimmed: not White_Space)
            "90de\u{0261}",     // latin small script g
        ] {
            let res = parse_angle_value(input);
            assert!(res.is_err(), "unicode garbage accepted: {input:?} -> {res:?}");
            assert!(!format!("{}", res.unwrap_err()).is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_parse_valid_minimal_positive_control() {
        assert_eq!(parse_angle_value("1deg").unwrap(), AngleValue::deg(1.0));
        assert_eq!(parse_angle_value("0").unwrap(), AngleValue::zero());
    }

    // -------------------------------------------------------------------
    // round-trip: encode == decode
    // -------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_round_trip_display_then_parse_all_metrics() {
        // Values chosen to be exactly representable in f32 *and* exact after the
        // x1000 fixed-point encoding, so the round-trip must be bit-exact.
        for m in ALL_METRICS {
            for v in [
                0.0_f32, 1.0, -1.0, 0.5, -0.25, 45.5, 90.0, 180.0, 359.0, 1000.0,
            ] {
                let angle = AngleValue::from_metric(m, v);
                let printed = angle.to_string();
                let reparsed = parse_angle_value(&printed)
                    .unwrap_or_else(|e| panic!("cannot re-parse own output {printed:?}: {e}"));
                assert_eq!(reparsed, angle, "round-trip changed value: {printed:?}");
                assert_eq!(reparsed.metric, m, "round-trip changed unit: {printed:?}");
                // print_as_css_value must agree with Display.
                assert_eq!(angle.print_as_css_value(), printed);
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_round_trip_metric_suffix_is_unambiguous() {
        // "1grad" must not be mis-lexed as "1g" + "rad" (suffix match order matters).
        for m in ALL_METRICS {
            let parsed = parse_angle_value(&format!("1{m}")).unwrap();
            assert_eq!(parsed.metric, m, "unit {m} did not round-trip");
            assert_eq!(parsed.number.get(), 1.0);
        }
        assert_eq!(parse_angle_value("1grad").unwrap().metric, AngleMetric::Grad);
        assert_eq!(
            parse_angle_value("1rad").unwrap().metric,
            AngleMetric::Radians
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_round_trip_quantization_is_idempotent() {
        // Re-encoding an already-quantized value must be a fixed point, otherwise
        // repeated serialize/parse cycles would drift.
        for v in [0.0_f32, 1.0, -1.0, 0.5, -0.25, 45.5, 359.0] {
            let once = AngleValue::deg(v);
            let twice = AngleValue::deg(once.number.get());
            assert_eq!(once, twice, "quantization not idempotent for {v}");
        }
    }

    // -------------------------------------------------------------------
    // error types: to_contained / to_shared
    // -------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_error_owned_round_trip_from_real_parse_failures() {
        for input in ["", "deg", "%", "xdeg", "zzz", "\u{1F600}rad"] {
            let err = parse_angle_value(input).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(
                owned.to_shared(),
                err,
                "to_contained/to_shared lost information for {input:?}"
            );
            // Both directions must be printable.
            assert!(!format!("{err}").is_empty());
            assert!(!format!("{owned:?}").is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn autotest_error_variants_are_the_expected_ones() {
        assert!(matches!(
            parse_angle_value("").unwrap_err(),
            CssAngleValueParseError::EmptyString
        ));
        assert!(matches!(
            parse_angle_value("turn").unwrap_err(),
            CssAngleValueParseError::NoValueGiven(_, AngleMetric::Turn)
        ));
        assert!(matches!(
            parse_angle_value("xdeg").unwrap_err(),
            CssAngleValueParseError::ValueParseErr(_, "x")
        ));
        assert!(matches!(
            parse_angle_value("zzz").unwrap_err(),
            CssAngleValueParseError::InvalidAngle("zzz")
        ));
    }

    #[test]
    fn autotest_error_to_shared_handles_empty_and_extreme_payloads() {
        // Hand-built owned errors (the FFI side can hand us anything, incl. empty
        // strings and the Empty float-error kind that the parser itself never emits).
        let cases = [
            CssAngleValueParseErrorOwned::EmptyString,
            CssAngleValueParseErrorOwned::NoValueGiven(AngleNoValueGivenError {
                value: String::new().into(),
                metric: AngleMetric::Percent,
            }),
            CssAngleValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput {
                error: FfiParseFloatError::Empty,
                input: String::new().into(),
            }),
            CssAngleValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput {
                error: FfiParseFloatError::Invalid,
                input: "\u{1F600}".to_string().into(),
            }),
            CssAngleValueParseErrorOwned::InvalidAngle(String::new().into()),
            CssAngleValueParseErrorOwned::InvalidAngle("\u{1F600}\u{0301}".to_string().into()),
        ];
        for owned in cases {
            let shared = owned.to_shared();
            assert!(!format!("{shared}").is_empty());
            // owned -> shared -> owned must be lossless, including the error *kind*.
            assert_eq!(shared.to_contained(), owned);
        }
    }
}
