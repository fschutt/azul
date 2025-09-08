use crate::{css_properties::*, parser::*};

/// Represents a `perspective-origin` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StylePerspectiveOrigin {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl StylePerspectiveOrigin {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl Default for StylePerspectiveOrigin {
    fn default() -> Self {
        StylePerspectiveOrigin {
            x: PixelValue::const_px(0),
            y: PixelValue::const_px(0),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStylePerspectiveOriginParseError<'a> {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    PixelValueParseError(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssStylePerspectiveOriginParseError<'a>);
impl_display! { CssStylePerspectiveOriginParseError<'a>, {
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components on transform property: expected {} components, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid transform property: {}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssStylePerspectiveOriginParseError::PixelValueParseError
);

/// Owned version of CssStylePerspectiveOriginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStylePerspectiveOriginParseErrorOwned {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStylePerspectiveOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStylePerspectiveOriginParseErrorOwned {
        match self {
            CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
            CssStylePerspectiveOriginParseError::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStylePerspectiveOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStylePerspectiveOriginParseError<'a> {
        match self {
            CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
            CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

pub fn parse_style_perspective_origin<'a>(
    input: &'a str,
) -> Result<StylePerspectiveOrigin, CssStylePerspectiveOriginParseError<'a>> {
    let input = input.trim();
    let mut iter = input.split(",");

    let x = parse_pixel_value(iter.next().ok_or(
        CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 0,
            input,
        },
    )?)?;
    let y = parse_pixel_value(iter.next().ok_or(
        CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 1,
            input,
        },
    )?)?;

    Ok(StylePerspectiveOrigin { x, y })
}
