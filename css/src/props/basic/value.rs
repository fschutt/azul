//! Basic CSS value types: pixels, percentages, floats, and size metrics

use crate::error::CssPixelValueParseError;
use crate::props::formatter::FormatAsCssValue;
use alloc::{format, string::String};
use core::fmt;

/// Fixed-point float value for consistent numeric representation
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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
#[derive(Copy, Clone, PartialEq, PartialOrd)]
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
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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

impl FloatValue {
    pub const fn new(value: f32) -> Self {
        Self {
            number: (value * 1000.0) as isize,
        }
    }

    pub fn get(&self) -> f32 {
        self.number as f32 / 1000.0
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

    #[inline]
    pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue {
                number: value * 1000,
            },
        }
    }

    #[inline]
    pub fn px(value: f32) -> Self {
        Self::from_metric(SizeMetric::Px, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub fn em(value: f32) -> Self {
        Self::from_metric(SizeMetric::Em, value)
    }

    #[inline]
    pub fn rem(value: f32) -> Self {
        Self::from_metric(SizeMetric::Rem, value)
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

    /// Convert this value to pixels, given a context for relative units
    pub fn to_pixels(
        &self,
        font_size_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> f32 {
        let value = self.number.get();
        match self.metric {
            SizeMetric::Px => value,
            SizeMetric::Pt => value * PT_TO_PX,
            SizeMetric::Em => value * font_size_px,
            SizeMetric::Rem => value * EM_HEIGHT,
            SizeMetric::In => value * 96.0, // 1 inch = 96px
            SizeMetric::Cm => value * 37.8, // 1cm ≈ 37.8px
            SizeMetric::Mm => value * 3.78, // 1mm ≈ 3.78px
            SizeMetric::Percent => value,   // Percentage needs context-specific handling
            SizeMetric::Vw => value * viewport_width_px / 100.0,
            SizeMetric::Vh => value * viewport_height_px / 100.0,
            SizeMetric::Vmin => value * viewport_width_px.min(viewport_height_px) / 100.0,
            SizeMetric::Vmax => value * viewport_width_px.max(viewport_height_px) / 100.0,
        }
    }

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.number = FloatValue::new(self.number.get() * scale_factor);
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
            .to_pixels(font_size_px, viewport_width_px, viewport_height_px)
    }
}

impl PercentageValue {
    pub fn new(value: f32) -> Self {
        Self {
            number: FloatValue::new(value),
        }
    }

    pub fn get(&self) -> f32 {
        self.number.get()
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
pub fn parse_float_value<'a>(input: &'a str) -> Result<FloatValue, CssPixelValueParseError<'a>> {
    let number = input
        .trim()
        .parse::<f32>()
        .map_err(|_| CssPixelValueParseError::InvalidNumber(input))?;
    Ok(FloatValue::new(number))
}
