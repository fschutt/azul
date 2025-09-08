use crate::{css_properties::*, parser::*};

/// Represents a `transform-origin` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformOrigin {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl StyleTransformOrigin {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl Default for StyleTransformOrigin {
    fn default() -> Self {
        StyleTransformOrigin {
            x: PixelValue::const_percent(50),
            y: PixelValue::const_percent(50),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleTransformOriginParseError<'a> {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    PixelValueParseError(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssStyleTransformOriginParseError<'a>);
impl_display! { CssStyleTransformOriginParseError<'a>, {
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components on transform property: expected {} components, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid transform property: {}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleTransformOriginParseError::PixelValueParseError
);

/// Owned version of CssStyleTransformOriginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleTransformOriginParseErrorOwned {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStyleTransformOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStyleTransformOriginParseErrorOwned {
        match self {
            CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
            CssStyleTransformOriginParseError::PixelValueParseError(e) => {
                CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleTransformOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleTransformOriginParseError<'a> {
        match self {
            CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
            CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e) => {
                CssStyleTransformOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

pub fn parse_style_transform_origin<'a>(
    input: &'a str,
) -> Result<StyleTransformOrigin, CssStyleTransformOriginParseError<'a>> {
    let input = input.trim();
    let mut iter = input.split(",");

    let x = parse_pixel_value(iter.next().ok_or(
        CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 0,
            input,
        },
    )?)?;
    let y = parse_pixel_value(iter.next().ok_or(
        CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 1,
            input,
        },
    )?)?;

    Ok(StyleTransformOrigin { x, y })
}
