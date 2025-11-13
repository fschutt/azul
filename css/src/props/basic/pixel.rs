use core::fmt;
use std::num::ParseFloatError;

use crate::props::{
    basic::{FloatValue, SizeMetric},
    formatter::FormatAsCssValue,
};

/// Currently hard-coded: Height of one em in pixels
pub const EM_HEIGHT: f32 = 16.0;
/// Conversion factor from points to pixels (1pt = 1/72 inch, 1in = 96px, therefore 1pt = 96/72 px)
pub const PT_TO_PX: f32 = 96.0 / 72.0;

/// A normalized percentage value (0.0 = 0%, 1.0 = 100%)
///
/// This type prevents double-division bugs by making it explicit that the value
/// is already normalized to the 0.0-1.0 range. When you have a `NormalizedPercentage`,
/// you should multiply it directly with the containing block size, NOT divide by 100 again.
///
/// # Example
/// ```rust
/// let percent = NormalizedPercentage::new(1.0); // 100%
/// let containing_block = 640.0;
/// let result = percent.resolve(containing_block); // 640.0, not 6.4!
/// ```
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct NormalizedPercentage(f32);

impl NormalizedPercentage {
    /// Create a new percentage value from a normalized float (0.0-1.0)
    ///
    /// # Arguments
    /// * `value` - A normalized percentage where 0.0 = 0% and 1.0 = 100%
    ///
    /// # Example
    /// ```rust
    /// let fifty_percent = NormalizedPercentage::new(0.5); // 50%
    /// let hundred_percent = NormalizedPercentage::new(1.0); // 100%
    /// ```
    #[inline]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// Create a percentage from an unnormalized value (0-100 scale)
    ///
    /// This divides by 100 internally, so you should use this when converting
    /// from CSS percentage syntax like "50%" which is stored as 50.0.
    ///
    /// # Example
    /// ```rust
    /// let percent = NormalizedPercentage::from_unnormalized(50.0); // 50% -> 0.5
    /// ```
    #[inline]
    pub fn from_unnormalized(value: f32) -> Self {
        Self(value / 100.0)
    }

    /// Get the raw normalized value (0.0-1.0)
    #[inline]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Resolve this percentage against a containing block size
    ///
    /// This multiplies the normalized percentage by the containing block size.
    /// For example, 50% (0.5) of 640px = 320px.
    ///
    /// # Example
    /// ```rust
    /// let percent = NormalizedPercentage::new(0.5); // 50%
    /// let size = percent.resolve(640.0); // 320.0
    /// ```
    #[inline]
    pub fn resolve(self, containing_block_size: f32) -> f32 {
        self.0 * containing_block_size
    }
}

impl fmt::Display for NormalizedPercentage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}%", self.0 * 100.0)
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

impl PixelValue {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.number = FloatValue::new(self.number.get() * scale_factor);
    }
}

impl FormatAsCssValue for PixelValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl crate::css::PrintAsCssValue for PixelValue {
    fn print_as_css_value(&self) -> String {
        format!("{}{}", self.number, self.metric)
    }
}

impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

// Manual Debug implementation, because the auto-generated one is nearly unreadable
impl fmt::Display for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl PixelValue {
    #[inline]
    pub const fn zero() -> Self {
        const ZERO_PX: PixelValue = PixelValue::const_px(0);
        ZERO_PX
    }

    /// Same as `PixelValue::px()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_px(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Px, value)
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_em(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Em, value)
    }

    /// Creates an em value from a fractional number in const context.
    ///
    /// # Arguments
    /// * `pre_comma` - The integer part (e.g., 1 for 1.5em)
    /// * `post_comma` - The fractional part as digits (e.g., 5 for 0.5em, 83 for 0.83em)
    ///
    /// # Examples
    /// ```
    /// // 1.5em = const_em_fractional(1, 5)
    /// // 0.83em = const_em_fractional(0, 83)
    /// // 1.17em = const_em_fractional(1, 17)
    /// ```
    #[inline]
    pub const fn const_em_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Em, pre_comma, post_comma)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_pt(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Pt, value)
    }

    /// Creates a pt value from a fractional number in const context.
    #[inline]
    pub const fn const_pt_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Pt, pre_comma, post_comma)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_percent(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Percent, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_in(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::In, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_cm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Cm, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_mm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a PixelValue from a fractional number in const context.
    ///
    /// # Arguments
    /// * `metric` - The size metric (Px, Em, Pt, etc.)
    /// * `pre_comma` - The integer part
    /// * `post_comma` - The fractional part as digits
    #[inline]
    pub const fn const_from_metric_fractional(metric: SizeMetric, pre_comma: isize, post_comma: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new_fractional(pre_comma, post_comma),
        }
    }

    #[inline]
    pub fn px(value: f32) -> Self {
        Self::from_metric(SizeMetric::Px, value)
    }

    #[inline]
    pub fn em(value: f32) -> Self {
        Self::from_metric(SizeMetric::Em, value)
    }

    #[inline]
    pub fn inch(value: f32) -> Self {
        Self::from_metric(SizeMetric::In, value)
    }

    #[inline]
    pub fn cm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Cm, value)
    }

    #[inline]
    pub fn mm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub fn percent(value: f32) -> Self {
        Self::from_metric(SizeMetric::Percent, value)
    }

    #[inline]
    pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.metric == other.metric {
            Self {
                metric: self.metric,
                number: self.number.interpolate(&other.number, t),
            }
        } else {
            // TODO: how to interpolate between different metrics
            // (interpolate between % and em? - currently impossible)
            let self_px_interp = self.to_pixels(0.0);
            let other_px_interp = other.to_pixels(0.0);
            Self::from_metric(
                SizeMetric::Px,
                self_px_interp + (other_px_interp - self_px_interp) * t,
            )
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels_no_percent(&self) -> Option<f32> {
        // to_pixels always assumes 96 DPI
        match self.metric {
            SizeMetric::Px => Some(self.number.get()),
            SizeMetric::Pt => Some(self.number.get() * PT_TO_PX),
            SizeMetric::Em => Some(self.number.get() * EM_HEIGHT),
            SizeMetric::In => Some(self.number.get() * 96.0),
            SizeMetric::Cm => Some(self.number.get() * 96.0 / 2.54),
            SizeMetric::Mm => Some(self.number.get() * 96.0 / 25.4),
            SizeMetric::Percent => None,
        }
    }

    /// Returns the value of the SizeMetric as a normalized percentage (0.0 = 0%, 1.0 = 100%)
    ///
    /// Returns `Some(NormalizedPercentage)` if this is a percentage value, `None` otherwise.
    /// The returned `NormalizedPercentage` is already normalized to 0.0-1.0 range,
    /// so you should multiply it directly with the containing block size.
    ///
    /// # Example
    /// ```rust
    /// let px = PixelValue::parse("100%").unwrap();
    /// if let Some(percent) = px.to_percent() {
    ///     let size = percent.resolve(640.0); // 640.0
    /// }
    /// ```
    #[inline]
    pub fn to_percent(&self) -> Option<NormalizedPercentage> {
        match self.metric {
            SizeMetric::Percent => Some(NormalizedPercentage::from_unnormalized(self.number.get())),
            _ => None,
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels(&self, percent_resolve: f32) -> f32 {
        // to_pixels always assumes 96 DPI
        match self.metric {
            SizeMetric::Percent => {
                NormalizedPercentage::from_unnormalized(self.number.get()).resolve(percent_resolve)
            }
            _ => self.to_pixels_no_percent().unwrap_or(0.0),
        }
    }
}

// border-width: thin = 0.5px
const THIN_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 500 },
};
// border-width: medium = 1.5px (default)
const MEDIUM_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 1500 },
};
// border-width: thick = 2.5px (default)
const THICK_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 2500 },
};

/// Same as PixelValue, but doesn't allow a "%" sign
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValueNoPercent {
    pub inner: PixelValue,
}

impl PixelValueNoPercent {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl_option!(
    PixelValueNoPercent,
    OptionPixelValueNoPercent,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for PixelValueNoPercent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ::core::fmt::Debug for PixelValueNoPercent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl PixelValueNoPercent {
    pub fn to_pixels(&self) -> f32 {
        self.inner.to_pixels(0.0)
    }

    #[inline]
    pub const fn zero() -> Self {
        const ZERO_PXNP: PixelValueNoPercent = PixelValueNoPercent {
            inner: PixelValue::zero(),
        };
        ZERO_PXNP
    }
}
impl From<PixelValue> for PixelValueNoPercent {
    fn from(e: PixelValue) -> Self {
        Self { inner: e }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssPixelValueParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str, SizeMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidPixelValue(&'a str),
}

impl_debug_as_display!(CssPixelValueParseError<'a>);

impl_display! { CssPixelValueParseError<'a>, {
    EmptyString => format!("Missing [px / pt / em / %] value"),
    NoValueGiven(input, metric) => format!("Expected floating-point pixel value, got: \"{}{}\"", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
    InvalidPixelValue(s) => format!("Invalid pixel value: \"{}\"", s),
}}

/// Owned version of CssPixelValueParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssPixelValueParseErrorOwned {
    EmptyString,
    NoValueGiven(String, SizeMetric),
    ValueParseErr(ParseFloatError, String),
    InvalidPixelValue(String),
}

impl<'a> CssPixelValueParseError<'a> {
    pub fn to_contained(&self) -> CssPixelValueParseErrorOwned {
        match self {
            CssPixelValueParseError::EmptyString => CssPixelValueParseErrorOwned::EmptyString,
            CssPixelValueParseError::NoValueGiven(s, metric) => {
                CssPixelValueParseErrorOwned::NoValueGiven(s.to_string(), *metric)
            }
            CssPixelValueParseError::ValueParseErr(err, s) => {
                CssPixelValueParseErrorOwned::ValueParseErr(err.clone(), s.to_string())
            }
            CssPixelValueParseError::InvalidPixelValue(s) => {
                CssPixelValueParseErrorOwned::InvalidPixelValue(s.to_string())
            }
        }
    }
}

impl CssPixelValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssPixelValueParseError<'a> {
        match self {
            CssPixelValueParseErrorOwned::EmptyString => CssPixelValueParseError::EmptyString,
            CssPixelValueParseErrorOwned::NoValueGiven(s, metric) => {
                CssPixelValueParseError::NoValueGiven(s.as_str(), *metric)
            }
            CssPixelValueParseErrorOwned::ValueParseErr(err, s) => {
                CssPixelValueParseError::ValueParseErr(err.clone(), s.as_str())
            }
            CssPixelValueParseErrorOwned::InvalidPixelValue(s) => {
                CssPixelValueParseError::InvalidPixelValue(s.as_str())
            }
        }
    }
}

/// parses an angle value like `30deg`, `1.64rad`, `100%`, etc.
fn parse_pixel_value_inner<'a>(
    input: &'a str,
    match_values: &[(&'static str, SizeMetric)],
) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssPixelValueParseError::EmptyString);
    }

    for (match_val, metric) in match_values {
        if input.ends_with(match_val) {
            let value = &input[..input.len() - match_val.len()];
            let value = value.trim();
            if value.is_empty() {
                return Err(CssPixelValueParseError::NoValueGiven(input, *metric));
            }
            match value.parse::<f32>() {
                Ok(o) => {
                    return Ok(PixelValue::from_metric(*metric, o));
                }
                Err(e) => {
                    return Err(CssPixelValueParseError::ValueParseErr(e, value));
                }
            }
        }
    }

    match input.trim().parse::<f32>() {
        Ok(o) => Ok(PixelValue::px(o)),
        Err(_) => Err(CssPixelValueParseError::InvalidPixelValue(input)),
    }
}

pub fn parse_pixel_value<'a>(input: &'a str) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    parse_pixel_value_inner(
        input,
        &[
            ("px", SizeMetric::Px),
            ("em", SizeMetric::Em),
            ("pt", SizeMetric::Pt),
            ("in", SizeMetric::In),
            ("mm", SizeMetric::Mm),
            ("cm", SizeMetric::Cm),
            ("%", SizeMetric::Percent),
        ],
    )
}

pub fn parse_pixel_value_no_percent<'a>(
    input: &'a str,
) -> Result<PixelValueNoPercent, CssPixelValueParseError<'a>> {
    Ok(PixelValueNoPercent {
        inner: parse_pixel_value_inner(
            input,
            &[
                ("px", SizeMetric::Px),
                ("em", SizeMetric::Em),
                ("pt", SizeMetric::Pt),
                ("in", SizeMetric::In),
                ("mm", SizeMetric::Mm),
                ("cm", SizeMetric::Cm),
            ],
        )?,
    })
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PixelValueWithAuto {
    None,
    Initial,
    Inherit,
    Auto,
    Exact(PixelValue),
}

/// Parses a pixel value, but also tries values like "auto", "initial", "inherit" and "none"
pub fn parse_pixel_value_with_auto<'a>(
    input: &'a str,
) -> Result<PixelValueWithAuto, CssPixelValueParseError<'a>> {
    let input = input.trim();
    match input {
        "none" => Ok(PixelValueWithAuto::None),
        "initial" => Ok(PixelValueWithAuto::Initial),
        "inherit" => Ok(PixelValueWithAuto::Inherit),
        "auto" => Ok(PixelValueWithAuto::Auto),
        e => Ok(PixelValueWithAuto::Exact(parse_pixel_value(e)?)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pixel_value() {
        assert_eq!(parse_pixel_value("10px").unwrap(), PixelValue::px(10.0));
        assert_eq!(parse_pixel_value("1.5em").unwrap(), PixelValue::em(1.5));
        assert_eq!(parse_pixel_value("-20pt").unwrap(), PixelValue::pt(-20.0));
        assert_eq!(parse_pixel_value("50%").unwrap(), PixelValue::percent(50.0));
        assert_eq!(parse_pixel_value("1in").unwrap(), PixelValue::inch(1.0));
        assert_eq!(parse_pixel_value("2.54cm").unwrap(), PixelValue::cm(2.54));
        assert_eq!(parse_pixel_value("10mm").unwrap(), PixelValue::mm(10.0));
        assert_eq!(parse_pixel_value("  0  ").unwrap(), PixelValue::px(0.0));
    }

    #[test]
    fn test_parse_pixel_value_no_percent() {
        assert_eq!(
            parse_pixel_value_no_percent("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert!(parse_pixel_value_no_percent("50%").is_err());
    }

    #[test]
    fn test_parse_pixel_value_with_auto() {
        assert_eq!(
            parse_pixel_value_with_auto("10px").unwrap(),
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );
        assert_eq!(
            parse_pixel_value_with_auto("auto").unwrap(),
            PixelValueWithAuto::Auto
        );
        assert_eq!(
            parse_pixel_value_with_auto("initial").unwrap(),
            PixelValueWithAuto::Initial
        );
        assert_eq!(
            parse_pixel_value_with_auto("inherit").unwrap(),
            PixelValueWithAuto::Inherit
        );
        assert_eq!(
            parse_pixel_value_with_auto("none").unwrap(),
            PixelValueWithAuto::None
        );
    }

    #[test]
    fn test_parse_pixel_value_errors() {
        assert!(parse_pixel_value("").is_err());
        // Modern CSS parsers can be liberal - unitless numbers treated as px
        assert!(parse_pixel_value("10").is_ok()); // Parsed as 10px
                                                  // This parser is liberal and trims whitespace, so "10 px" is accepted
        assert!(parse_pixel_value("10 px").is_ok()); // Liberal parsing accepts this
        assert!(parse_pixel_value("px").is_err());
        assert!(parse_pixel_value("ten-px").is_err());
    }
}
