//! CSS properties for flowing content around shapes (CSS Shapes Module).

use alloc::string::{String, ToString};

use crate::{
    props::{
        basic::{
            length::{parse_float_value, FloatValue},
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
        },
        formatter::PrintAsCssValue,
    },
    shape::CssShape,
};

/// CSS shape-outside property for wrapping text around shapes
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ShapeOutside {
    None,
    Shape(CssShape),
}

impl Eq for ShapeOutside {}
impl core::hash::Hash for ShapeOutside {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let ShapeOutside::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ShapeOutside {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeOutside {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (ShapeOutside::None, ShapeOutside::None) => core::cmp::Ordering::Equal,
            (ShapeOutside::None, ShapeOutside::Shape(_)) => core::cmp::Ordering::Less,
            (ShapeOutside::Shape(_), ShapeOutside::None) => core::cmp::Ordering::Greater,
            (ShapeOutside::Shape(a), ShapeOutside::Shape(b)) => a.cmp(b),
        }
    }
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
            Self::Shape(shape) => format!("{:?}", shape), // TODO: Proper CSS formatting
        }
    }
}

/// CSS shape-inside property for flowing text within shapes
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ShapeInside {
    None,
    Shape(CssShape),
}

impl Eq for ShapeInside {}
impl core::hash::Hash for ShapeInside {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let ShapeInside::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ShapeInside {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeInside {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (ShapeInside::None, ShapeInside::None) => core::cmp::Ordering::Equal,
            (ShapeInside::None, ShapeInside::Shape(_)) => core::cmp::Ordering::Less,
            (ShapeInside::Shape(_), ShapeInside::None) => core::cmp::Ordering::Greater,
            (ShapeInside::Shape(a), ShapeInside::Shape(b)) => a.cmp(b),
        }
    }
}

impl Default for ShapeInside {
    fn default() -> Self {
        Self::None
    }
}

impl PrintAsCssValue for ShapeInside {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(shape) => format!("{:?}", shape), // TODO: Proper CSS formatting
        }
    }
}

/// CSS clip-path property for clipping element rendering
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ClipPath {
    None,
    Shape(CssShape),
}

impl Eq for ClipPath {}
impl core::hash::Hash for ClipPath {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let ClipPath::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ClipPath {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ClipPath {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (ClipPath::None, ClipPath::None) => core::cmp::Ordering::Equal,
            (ClipPath::None, ClipPath::Shape(_)) => core::cmp::Ordering::Less,
            (ClipPath::Shape(_), ClipPath::None) => core::cmp::Ordering::Greater,
            (ClipPath::Shape(a), ClipPath::Shape(b)) => a.cmp(b),
        }
    }
}

impl Default for ClipPath {
    fn default() -> Self {
        Self::None
    }
}

impl PrintAsCssValue for ClipPath {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(shape) => format!("{:?}", shape), // TODO: Proper CSS formatting
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
            ShapeOutside::Shape(_s) => String::from("ShapeOutside::Shape(/* ... */)"), // TODO
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ShapeInside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ShapeInside::None => String::from("ShapeInside::None"),
            ShapeInside::Shape(_s) => String::from("ShapeInside::Shape(/* ... */)"), // TODO
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ClipPath {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ClipPath::None => String::from("ClipPath::None"),
            ClipPath::Shape(_s) => String::from("ClipPath::Shape(/* ... */)"), // TODO
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
pub mod parser {
    use core::num::ParseFloatError;

    use super::*;
    use crate::shape_parser::{parse_shape, ShapeParseError};

    /// Parser for shape-outside property
    pub fn parse_shape_outside(input: &str) -> Result<ShapeOutside, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ShapeOutside::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ShapeOutside::Shape(shape))
        }
    }

    /// Parser for shape-inside property
    pub fn parse_shape_inside(input: &str) -> Result<ShapeInside, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ShapeInside::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ShapeInside::Shape(shape))
        }
    }

    /// Parser for clip-path property
    pub fn parse_clip_path(input: &str) -> Result<ClipPath, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ClipPath::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ClipPath::Shape(shape))
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
    fn test_parse_shape_properties() {
        // Test shape-outside
        assert!(matches!(
            parse_shape_outside("none").unwrap(),
            ShapeOutside::None
        ));
        assert!(matches!(
            parse_shape_outside("circle(50px)").unwrap(),
            ShapeOutside::Shape(_)
        ));

        // Test shape-inside
        assert!(matches!(
            parse_shape_inside("none").unwrap(),
            ShapeInside::None
        ));
        assert!(matches!(
            parse_shape_inside("circle(100px at 50px 50px)").unwrap(),
            ShapeInside::Shape(_)
        ));

        // Test clip-path
        assert!(matches!(parse_clip_path("none").unwrap(), ClipPath::None));
        assert!(matches!(
            parse_clip_path("polygon(0 0, 100px 0, 100px 100px, 0 100px)").unwrap(),
            ClipPath::Shape(_)
        ));

        // Test existing properties
        assert_eq!(
            parse_shape_margin("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert_eq!(parse_shape_image_threshold("0.5").unwrap().inner.get(), 0.5);
    }
}
