//! CSS value formatting trait

use alloc::string::String;

/// Trait for formatting CSS property values back to CSS string representation
pub trait FormatAsCssValue {
    /// Format this value as a CSS string
    fn format_as_css_value(&self) -> String;
}

// Implement the trait for common primitive types
impl FormatAsCssValue for f32 {
    fn format_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl FormatAsCssValue for i32 {
    fn format_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl FormatAsCssValue for u32 {
    fn format_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl FormatAsCssValue for String {
    fn format_as_css_value(&self) -> String {
        self.clone()
    }
}

impl FormatAsCssValue for &str {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}
