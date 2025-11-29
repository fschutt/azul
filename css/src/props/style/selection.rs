//! CSS properties for styling text selections (-azul-selection-*).

use alloc::string::String;

use crate::props::{
    basic::color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
    formatter::PrintAsCssValue,
};

// --- -azul-selection-background-color ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionBackgroundColor {
    pub inner: ColorU,
}

impl Default for SelectionBackgroundColor {
    fn default() -> Self {
        // A common default selection color
        Self {
            inner: ColorU::new(173, 214, 255, 255),
        }
    }
}

impl PrintAsCssValue for SelectionBackgroundColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl crate::format_rust_code::FormatAsRustCode for SelectionBackgroundColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "SelectionBackgroundColor {{ inner: {} }}",
            crate::format_rust_code::format_color_value(&self.inner)
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_selection_background_color(
    input: &str,
) -> Result<SelectionBackgroundColor, CssColorParseError> {
    parse_css_color(input).map(|inner| SelectionBackgroundColor { inner })
}

// --- -azul-selection-color ---

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionColor {
    pub inner: ColorU,
}

impl Default for SelectionColor {
    fn default() -> Self {
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl PrintAsCssValue for SelectionColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl crate::format_rust_code::FormatAsRustCode for SelectionColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "SelectionColor {{ inner: {} }}",
            crate::format_rust_code::format_color_value(&self.inner)
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_selection_color(input: &str) -> Result<SelectionColor, CssColorParseError> {
    parse_css_color(input).map(|inner| SelectionColor { inner })
}

// --- -azul-selection-radius ---

use crate::props::basic::{
    pixel::{parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    SizeMetric,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SelectionRadius {
    pub inner: PixelValue,
}

impl Default for SelectionRadius {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl PrintAsCssValue for SelectionRadius {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

impl crate::format_rust_code::FormatAsRustCode for SelectionRadius {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // Use the Display implementation of PixelValue to get a string like "5px" or "1em"
        format!(
            "SelectionRadius {{ inner: PixelValue::from_metric(SizeMetric::{:?}, {}) }}",
            self.inner.metric,
            self.inner.number.get()
        )
    }
}

#[cfg(feature = "parser")]
pub fn parse_selection_radius(input: &str) -> Result<SelectionRadius, CssPixelValueParseError> {
    parse_pixel_value(input).map(|inner| SelectionRadius { inner })
}
