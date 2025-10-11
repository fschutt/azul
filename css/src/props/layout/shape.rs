//! CSS properties for flowing content around shapes (CSS Shapes Module).

use alloc::string::{String, ToString};

use crate::props::{
    basic::{
        length::{parse_float_value, FloatValue},
        pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
    },
    formatter::PrintAsCssValue,
};

// For now, shape-outside is a string, since parsing basic-shape is complex.
// A full implementation would parse circle(), polygon(), etc. into enums.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ShapeOutside {
    None,
    Shape(String), // Placeholder for basic-shape, shape-box, url()
}

impl Default for ShapeOutside {
    fn default() -> Self {
        Self::None
    }
}

impl PrintAsCssValue for ShapeOutside {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(s) => s.clone(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ShapeMargin {
    pub inner: PixelValue,
}

impl Default for ShapeMargin {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl PrintAsCssValue for ShapeMargin {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ShapeImageThreshold {
    pub inner: FloatValue,
}

impl Default for ShapeImageThreshold {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(0),
        }
    }
}

impl PrintAsCssValue for ShapeImageThreshold {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for ShapeOutside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ShapeOutside::None => String::from("ShapeOutside::None"),
            ShapeOutside::Shape(s) => format!("ShapeOutside::Shape(String::from({:?}))", s),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ShapeMargin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ShapeMargin {{ inner: {} }}",
            crate::format_rust_code::format_pixel_value(&self.inner)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for ShapeImageThreshold {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ShapeImageThreshold {{ inner: {} }}",
            crate::format_rust_code::format_float_value(&self.inner)
        )
    }
}

// --- PARSERS ---
#[cfg(feature = "parser")]
mod parser {
    use core::num::ParseFloatError;

    use super::*;

    // A simplified parser for shape-outside.
    pub fn parse_shape_outside(input: &str) -> Result<ShapeOutside, ()> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ShapeOutside::None)
        } else {
            Ok(ShapeOutside::Shape(trimmed.to_string()))
        }
    }

    // Parsers for margin and threshold
    pub fn parse_shape_margin(input: &str) -> Result<ShapeMargin, CssPixelValueParseError> {
        Ok(ShapeMargin {
            inner: parse_pixel_value(input)?,
        })
    }

    pub fn parse_shape_image_threshold(
        input: &str,
    ) -> Result<ShapeImageThreshold, ParseFloatError> {
        let val = parse_float_value(input)?;
        // value should be clamped between 0.0 and 1.0
        let clamped = val.get().max(0.0).min(1.0);
        Ok(ShapeImageThreshold {
            inner: FloatValue::new(clamped),
        })
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shape_simplified() {
        assert_eq!(parse_shape_outside("none").unwrap(), ShapeOutside::None);
        assert_eq!(
            parse_shape_outside("circle(50%)").unwrap(),
            ShapeOutside::Shape("circle(50%)".to_string())
        );
        assert_eq!(
            parse_shape_margin("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert_eq!(parse_shape_image_threshold("0.5").unwrap().inner.get(), 0.5);
    }
}
