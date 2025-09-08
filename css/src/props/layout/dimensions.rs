//! Layout dimension properties: width, height, min/max variants

use crate::error::CssPixelValueParseError;
use crate::props::basic::value::PixelValue;
use crate::props::formatter::FormatAsCssValue;

/// Layout width property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutWidth {
    pub inner: PixelValue,
}

/// Layout height property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutHeight {
    pub inner: PixelValue,
}

/// Layout minimum width property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMinWidth {
    pub inner: PixelValue,
}

/// Layout maximum width property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMaxWidth {
    pub inner: PixelValue,
}

/// Layout minimum height property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMinHeight {
    pub inner: PixelValue,
}

/// Layout maximum height property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutMaxHeight {
    pub inner: PixelValue,
}

// Default implementations
impl Default for LayoutWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutHeight {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutMinWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutMaxWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutMinHeight {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl Default for LayoutMaxHeight {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

// FormatAsCssValue implementations
impl FormatAsCssValue for LayoutWidth {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutHeight {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutMinWidth {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutMaxWidth {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutMinHeight {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutMaxHeight {
    fn format_as_css_value(&self) -> alloc::string::String {
        self.inner.format_as_css_value()
    }
}

// Parsing functions
pub fn parse_layout_width<'a>(input: &'a str) -> Result<LayoutWidth, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutWidth { inner: pixel_value })
}

pub fn parse_layout_height<'a>(
    input: &'a str,
) -> Result<LayoutHeight, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutHeight { inner: pixel_value })
}

pub fn parse_layout_min_width<'a>(
    input: &'a str,
) -> Result<LayoutMinWidth, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutMinWidth { inner: pixel_value })
}

pub fn parse_layout_max_width<'a>(
    input: &'a str,
) -> Result<LayoutMaxWidth, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutMaxWidth { inner: pixel_value })
}

pub fn parse_layout_min_height<'a>(
    input: &'a str,
) -> Result<LayoutMinHeight, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutMinHeight { inner: pixel_value })
}

pub fn parse_layout_max_height<'a>(
    input: &'a str,
) -> Result<LayoutMaxHeight, CssPixelValueParseError<'a>> {
    let pixel_value = crate::props::basic::value::parse_pixel_value(input)?;
    Ok(LayoutMaxHeight { inner: pixel_value })
}

// Constructor implementations
impl LayoutWidth {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}

impl LayoutHeight {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}

impl LayoutMinWidth {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}

impl LayoutMaxWidth {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}

impl LayoutMinHeight {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}

impl LayoutMaxHeight {
    pub fn new(inner: PixelValue) -> Self {
        Self { inner }
    }

    pub fn px(value: f32) -> Self {
        Self::new(PixelValue::px(value))
    }
}
