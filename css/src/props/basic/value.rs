//! Basic CSS value types: pixels, percentages, floats, and size metrics

use alloc::{format, string::String};
use core::fmt;

use crate::{error::CssPixelValueParseError, props::formatter::FormatAsCssValue};

/// Fixed-point float value for consistent numeric representation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FloatValue {
    number: isize,
}

/// Size units for CSS length values
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SizeMetric {
    /// Pixels (absolute unit)
    Px,
    /// Points (absolute unit, 1pt = 1/72 inch)
    Pt,
    /// Em units (relative to font size)
    Em,
    /// Rem units (relative to root font size)
    Rem,
    /// Inches (absolute unit)
    In,
    /// Centimeters (absolute unit)
    Cm,
    /// Millimeters (absolute unit)
    Mm,
    /// Percentage (relative unit)
    Percent,
    /// Viewport width (relative unit)
    Vw,
    /// Viewport height (relative unit)
    Vh,
    /// Viewport minimum (relative unit)
    Vmin,
    /// Viewport maximum (relative unit)
    Vmax,
}

/// CSS pixel value with unit
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

/// CSS pixel value that doesn't allow percentage
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PixelValueNoPercent {
    pub inner: PixelValue,
}

/// CSS percentage value
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PercentageValue {
    pub number: FloatValue,
}

/// Pixel-based size (width and height)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PixelSize {
    pub width: PixelValue,
    pub height: PixelValue,
}

// Constants for conversions
pub const EM_HEIGHT: f32 = 16.0;
pub const PT_TO_PX: f32 = 96.0 / 72.0;

/// Multiplier for floating point accuracy. Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 5 digits, i.e
/// `1.00001 == 1.0`
const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

impl FloatValue {
    /// Same as `FloatValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_new(value: isize) -> Self {
        Self {
            number: value * FP_PRECISION_MULTIPLIER_CONST,
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

impl Default for FloatValue {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl fmt::Display for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl Default for PixelValue {
    fn default() -> Self {
        Self::zero()
    }
}

impl Default for PixelValueNoPercent {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for PercentageValue {
    fn default() -> Self {
        Self {
            number: FloatValue::new(0.0),
        }
    }
}

impl Default for PixelSize {
    fn default() -> Self {
        Self {
            width: PixelValue::zero(),
            height: PixelValue::zero(),
        }
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

impl fmt::Display for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl fmt::Display for PixelValueNoPercent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl fmt::Display for PercentageValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}%", self.number)
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

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.number = FloatValue::new(self.number.get() * scale_factor);
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_em(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Em, value)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_pt(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Pt, value)
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
            let self_px_interp = self.to_pixels(0.0, EM_HEIGHT, 0.0, 0.0);
            let other_px_interp = other.to_pixels(0.0, EM_HEIGHT, 0.0, 0.0);
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
            SizeMetric::Em | SizeMetric::Rem => Some(self.number.get() * EM_HEIGHT),
            SizeMetric::In => Some(self.number.get() * 96.0),
            SizeMetric::Cm => Some(self.number.get() * 96.0 / 2.54),
            SizeMetric::Mm => Some(self.number.get() * 96.0 / 25.4),
            // TODO - currently impossible to calculate
            SizeMetric::Vh | SizeMetric::Vmax | SizeMetric::Vmin | SizeMetric::Vw => None,
            SizeMetric::Percent => None,
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels(
        &self,
        percent_resolve: f32,
        font_size_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> f32 {
        // to_pixels always assumes 96 DPI
        match self.metric {
            SizeMetric::Percent => self.number.get() / 100.0 * percent_resolve,
            _ => self.to_pixels_no_percent().unwrap_or(0.0),
        }
    }
}

impl PixelValueNoPercent {
    pub fn new(inner: PixelValue) -> Result<Self, &'static str> {
        if inner.metric == SizeMetric::Percent {
            Err("PixelValueNoPercent cannot contain percentage values")
        } else {
            Ok(Self { inner })
        }
    }

    pub fn px(value: f32) -> Self {
        Self {
            inner: PixelValue::px(value),
        }
    }

    pub fn to_pixels(
        &self,
        font_size_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> f32 {
        self.inner
            .to_pixels(100.0, font_size_px, viewport_width_px, viewport_height_px)
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

    pub const fn zero() -> Self {
        Self::const_new(0)
    }

    #[inline]
    pub fn new(value: f32) -> Self {
        Self {
            number: FloatValue::new(value),
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

impl PixelSize {
    pub fn new(width: PixelValue, height: PixelValue) -> Self {
        Self { width, height }
    }

    pub fn zero() -> Self {
        Self::default()
    }
}

impl FormatAsCssValue for FloatValue {
    fn format_as_css_value(&self) -> String {
        format!("{}", self.get())
    }
}

impl FormatAsCssValue for PixelValue {
    fn format_as_css_value(&self) -> String {
        format!("{}{}", self.number.get(), self.metric)
    }
}

impl FormatAsCssValue for PixelValueNoPercent {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for PercentageValue {
    fn format_as_css_value(&self) -> String {
        format!("{}%", self.number.get())
    }
}

impl FormatAsCssValue for PixelSize {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {}",
            self.width.format_as_css_value(),
            self.height.format_as_css_value()
        )
    }
}

// Parsing functions

/// Parse a pixel value (e.g., "10px", "1.5em", "50%")
#[cfg(feature = "parser")]
pub fn parse_pixel_value<'a>(input: &'a str) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssPixelValueParseError::InvalidNumber(input));
    }

    // Handle unitless zero
    if input == "0" {
        return Ok(PixelValue::zero());
    }

    // Find where the number ends and the unit begins
    let mut split_pos = input.len();
    for (i, c) in input.char_indices().rev() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            split_pos = i + 1;
            break;
        }
    }

    if split_pos == 0 {
        return Err(CssPixelValueParseError::InvalidNumber(input));
    }

    let (number_str, unit_str) = input.split_at(split_pos);

    let number = number_str
        .parse::<f32>()
        .map_err(|_| CssPixelValueParseError::InvalidNumber(input))?;

    let metric = match unit_str {
        "px" => SizeMetric::Px,
        "pt" => SizeMetric::Pt,
        "em" => SizeMetric::Em,
        "rem" => SizeMetric::Rem,
        "in" => SizeMetric::In,
        "cm" => SizeMetric::Cm,
        "mm" => SizeMetric::Mm,
        "%" => SizeMetric::Percent,
        "vw" => SizeMetric::Vw,
        "vh" => SizeMetric::Vh,
        "vmin" => SizeMetric::Vmin,
        "vmax" => SizeMetric::Vmax,
        "" => {
            if number == 0.0 {
                SizeMetric::Px // Unitless zero defaults to pixels
            } else {
                return Err(CssPixelValueParseError::NoUnit(input));
            }
        }
        _ => return Err(CssPixelValueParseError::InvalidUnit(unit_str)),
    };

    Ok(PixelValue::from_metric(metric, number))
}

/// Parse a pixel value that doesn't allow percentages
#[cfg(feature = "parser")]
pub fn parse_pixel_value_no_percent<'a>(
    input: &'a str,
) -> Result<PixelValueNoPercent, CssPixelValueParseError<'a>> {
    let pixel_value = parse_pixel_value(input)?;
    if pixel_value.metric == SizeMetric::Percent {
        return Err(CssPixelValueParseError::InvalidUnit("%"));
    }
    Ok(PixelValueNoPercent { inner: pixel_value })
}

/// Parse a percentage value
#[cfg(feature = "parser")]
pub fn parse_percentage_value<'a>(
    input: &'a str,
) -> Result<PercentageValue, CssPixelValueParseError<'a>> {
    let input = input.trim();

    if !input.ends_with('%') {
        return Err(CssPixelValueParseError::InvalidUnit(input));
    }

    let number_str = &input[..input.len() - 1];
    let number = number_str
        .parse::<f32>()
        .map_err(|_| CssPixelValueParseError::InvalidNumber(input))?;

    Ok(PercentageValue::new(number))
}

/// Parse a float value
#[cfg(feature = "parser")]
pub fn parse_float_value<'a>(input: &'a str) -> Result<FloatValue, CssPixelValueParseError<'a>> {
    let number = input
        .trim()
        .parse::<f32>()
        .map_err(|_| CssPixelValueParseError::InvalidNumber(input))?;
    Ok(FloatValue::new(number))
}
