//! CSS properties for 2D and 3D transformations.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use std::num::ParseFloatError;
use crate::corety::AzString;

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::WrongComponentCountError,
    length::parse_float_value,
    parse::{parse_parentheses, ParenthesisParseError, ParenthesisParseErrorOwned},
};
use crate::{
    format_rust_code::GetHash,
    props::{
        basic::{
            angle::{
                parse_angle_value, AngleValue, CssAngleValueParseError,
                CssAngleValueParseErrorOwned,
            },
            length::{PercentageParseError, PercentageValue},
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
            FloatValue,
        },
        formatter::PrintAsCssValue,
    },
};

// -- Data Structures --

/// Represents a `perspective-origin` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl PrintAsCssValue for StylePerspectiveOrigin {
    fn print_as_css_value(&self) -> String {
        format!("{} {}", self.x, self.y)
    }
}

/// Represents a `transform-origin` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformOrigin {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl Default for StyleTransformOrigin {
    fn default() -> Self {
        Self {
            x: PixelValue::const_percent(50),
            y: PixelValue::const_percent(50),
        }
    }
}

impl StyleTransformOrigin {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl PrintAsCssValue for StyleTransformOrigin {
    fn print_as_css_value(&self) -> String {
        format!("{} {}", self.x, self.y)
    }
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for StylePerspectiveOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StylePerspectiveOrigin {{ x: {}, y: {} }}",
            crate::format_rust_code::format_pixel_value(&self.x),
            crate::format_rust_code::format_pixel_value(&self.y)
        )
    }
}

// Formatting to Rust code for StyleTransformOrigin
impl crate::format_rust_code::FormatAsRustCode for StyleTransformOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleTransformOrigin {{ x: {}, y: {} }}",
            crate::format_rust_code::format_pixel_value(&self.x),
            crate::format_rust_code::format_pixel_value(&self.y)
        )
    }
}

/// Represents a `backface-visibility` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackfaceVisibility {
    #[default]
    Visible,
    Hidden,
}

impl PrintAsCssValue for StyleBackfaceVisibility {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleBackfaceVisibility::Hidden => "hidden",
            StyleBackfaceVisibility::Visible => "visible",
        })
    }
}

/// Represents one component of a `transform` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleTransform {
    Matrix(StyleTransformMatrix2D),
    Matrix3D(StyleTransformMatrix3D),
    Translate(StyleTransformTranslate2D),
    Translate3D(StyleTransformTranslate3D),
    TranslateX(PixelValue),
    TranslateY(PixelValue),
    TranslateZ(PixelValue),
    Rotate(AngleValue),
    Rotate3D(StyleTransformRotate3D),
    RotateX(AngleValue),
    RotateY(AngleValue),
    RotateZ(AngleValue),
    Scale(StyleTransformScale2D),
    Scale3D(StyleTransformScale3D),
    ScaleX(PercentageValue),
    ScaleY(PercentageValue),
    ScaleZ(PercentageValue),
    Skew(StyleTransformSkew2D),
    SkewX(AngleValue),
    SkewY(AngleValue),
    Perspective(PixelValue),
}

impl_option!(
    StyleTransform,
    OptionStyleTransform,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl_vec!(StyleTransform, StyleTransformVec, StyleTransformVecDestructor, StyleTransformVecDestructorType, StyleTransformVecSlice, OptionStyleTransform);
impl_vec_debug!(StyleTransform, StyleTransformVec);
impl_vec_partialord!(StyleTransform, StyleTransformVec);
impl_vec_ord!(StyleTransform, StyleTransformVec);
impl_vec_clone!(
    StyleTransform,
    StyleTransformVec,
    StyleTransformVecDestructor
);
impl_vec_partialeq!(StyleTransform, StyleTransformVec);
impl_vec_eq!(StyleTransform, StyleTransformVec);
impl_vec_hash!(StyleTransform, StyleTransformVec);

impl PrintAsCssValue for StyleTransformVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// Formatting to Rust code for StyleTransformVec
impl crate::format_rust_code::FormatAsRustCode for StyleTransformVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleTransformVec::from_const_slice(STYLE_TRANSFORM_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl PrintAsCssValue for StyleTransform {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleTransform::Matrix(m) => format!(
                "matrix({}, {}, {}, {}, {}, {})",
                m.a, m.b, m.c, m.d, m.tx, m.ty
            ),
            StyleTransform::Matrix3D(m) => format!(
                "matrix3d({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
                m.m11,
                m.m12,
                m.m13,
                m.m14,
                m.m21,
                m.m22,
                m.m23,
                m.m24,
                m.m31,
                m.m32,
                m.m33,
                m.m34,
                m.m41,
                m.m42,
                m.m43,
                m.m44
            ),
            StyleTransform::Translate(t) => format!("translate({}, {})", t.x, t.y),
            StyleTransform::Translate3D(t) => format!("translate3d({}, {}, {})", t.x, t.y, t.z),
            StyleTransform::TranslateX(x) => format!("translateX({})", x),
            StyleTransform::TranslateY(y) => format!("translateY({})", y),
            StyleTransform::TranslateZ(z) => format!("translateZ({})", z),
            StyleTransform::Rotate(r) => format!("rotate({})", r),
            StyleTransform::Rotate3D(r) => {
                format!("rotate3d({}, {}, {}, {})", r.x, r.y, r.z, r.angle)
            }
            StyleTransform::RotateX(x) => format!("rotateX({})", x),
            StyleTransform::RotateY(y) => format!("rotateY({})", y),
            StyleTransform::RotateZ(z) => format!("rotateZ({})", z),
            StyleTransform::Scale(s) => format!("scale({}, {})", s.x, s.y),
            StyleTransform::Scale3D(s) => format!("scale3d({}, {}, {})", s.x, s.y, s.z),
            StyleTransform::ScaleX(x) => format!("scaleX({})", x),
            StyleTransform::ScaleY(y) => format!("scaleY({})", y),
            StyleTransform::ScaleZ(z) => format!("scaleZ({})", z),
            StyleTransform::Skew(sk) => format!("skew({}, {})", sk.x, sk.y),
            StyleTransform::SkewX(x) => format!("skewX({})", x),
            StyleTransform::SkewY(y) => format!("skewY({})", y),
            StyleTransform::Perspective(dist) => format!("perspective({})", dist),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix2D {
    pub a: FloatValue,
    pub b: FloatValue,
    pub c: FloatValue,
    pub d: FloatValue,
    pub tx: FloatValue,
    pub ty: FloatValue,
}

impl Default for StyleTransformMatrix2D {
    fn default() -> Self {
        Self {
            a: FloatValue::const_new(1),
            b: FloatValue::const_new(0),
            c: FloatValue::const_new(0),
            d: FloatValue::const_new(1),
            tx: FloatValue::const_new(0),
            ty: FloatValue::const_new(0),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix3D {
    pub m11: FloatValue,
    pub m12: FloatValue,
    pub m13: FloatValue,
    pub m14: FloatValue,
    pub m21: FloatValue,
    pub m22: FloatValue,
    pub m23: FloatValue,
    pub m24: FloatValue,
    pub m31: FloatValue,
    pub m32: FloatValue,
    pub m33: FloatValue,
    pub m34: FloatValue,
    pub m41: FloatValue,
    pub m42: FloatValue,
    pub m43: FloatValue,
    pub m44: FloatValue,
}

impl Default for StyleTransformMatrix3D {
    fn default() -> Self {
        Self {
            m11: FloatValue::const_new(1),
            m12: FloatValue::const_new(0),
            m13: FloatValue::const_new(0),
            m14: FloatValue::const_new(0),
            m21: FloatValue::const_new(0),
            m22: FloatValue::const_new(1),
            m23: FloatValue::const_new(0),
            m24: FloatValue::const_new(0),
            m31: FloatValue::const_new(0),
            m32: FloatValue::const_new(0),
            m33: FloatValue::const_new(1),
            m34: FloatValue::const_new(0),
            m41: FloatValue::const_new(0),
            m42: FloatValue::const_new(0),
            m43: FloatValue::const_new(0),
            m44: FloatValue::const_new(1),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate2D {
    pub x: PixelValue,
    pub y: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate3D {
    pub x: PixelValue,
    pub y: PixelValue,
    pub z: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformRotate3D {
    pub x: FloatValue,
    pub y: FloatValue,
    pub z: FloatValue,
    pub angle: AngleValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale2D {
    pub x: FloatValue,
    pub y: FloatValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale3D {
    pub x: FloatValue,
    pub y: FloatValue,
    pub z: FloatValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformSkew2D {
    pub x: AngleValue,
    pub y: AngleValue,
}

// -- Errors --

#[derive(Clone, PartialEq)]
pub enum CssStyleTransformParseError<'a> {
    InvalidTransform(&'a str),
    InvalidParenthesis(ParenthesisParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    NumberParseError(core::num::ParseFloatError),
    PixelValueParseError(CssPixelValueParseError<'a>),
    AngleValueParseError(CssAngleValueParseError<'a>),
    PercentageValueParseError(PercentageParseError),
}

impl_debug_as_display!(CssStyleTransformParseError<'a>);
impl_display! { CssStyleTransformParseError<'a>, {
    InvalidTransform(e) => format!("Invalid transform property: \"{}\"", e),
    InvalidParenthesis(e) => format!("Invalid transform property - parenthesis error: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components: expected {}, got {}: \"{}\"", expected, got, input),
    NumberParseError(e) => format!("Could not parse number: {}", e),
    PixelValueParseError(e) => format!("Invalid pixel value: {}", e),
    AngleValueParseError(e) => format!("Invalid angle value: {}", e),
    PercentageValueParseError(e) => format!("Error parsing percentage: {}", e),
}}

impl_from! { ParenthesisParseError<'a>, CssStyleTransformParseError::InvalidParenthesis }
impl_from! { CssPixelValueParseError<'a>, CssStyleTransformParseError::PixelValueParseError }
impl_from! { CssAngleValueParseError<'a>, CssStyleTransformParseError::AngleValueParseError }
impl_from! { ParseFloatError, CssStyleTransformParseError<'a>::NumberParseError }

impl<'a> From<PercentageParseError> for CssStyleTransformParseError<'a> {
    fn from(p: PercentageParseError) -> Self {
        Self::PercentageValueParseError(p)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssStyleTransformParseErrorOwned {
    InvalidTransform(AzString),
    InvalidParenthesis(ParenthesisParseErrorOwned),
    WrongNumberOfComponents(WrongComponentCountError),
    NumberParseError(core::num::ParseFloatError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
    AngleValueParseError(CssAngleValueParseErrorOwned),
    PercentageValueParseError(PercentageParseError),
}

impl<'a> CssStyleTransformParseError<'a> {
    pub fn to_contained(&self) -> CssStyleTransformParseErrorOwned {
        match self {
            Self::InvalidTransform(s) => {
                CssStyleTransformParseErrorOwned::InvalidTransform(s.to_string().into())
            }
            Self::InvalidParenthesis(e) => {
                CssStyleTransformParseErrorOwned::InvalidParenthesis(e.to_contained())
            }
            Self::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            }),
            Self::NumberParseError(e) => {
                CssStyleTransformParseErrorOwned::NumberParseError(e.clone())
            }
            Self::PixelValueParseError(e) => {
                CssStyleTransformParseErrorOwned::PixelValueParseError(e.to_contained())
            }
            Self::AngleValueParseError(e) => {
                CssStyleTransformParseErrorOwned::AngleValueParseError(e.to_contained())
            }
            Self::PercentageValueParseError(e) => {
                CssStyleTransformParseErrorOwned::PercentageValueParseError(e.clone())
            }
        }
    }
}

impl CssStyleTransformParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleTransformParseError<'a> {
        match self {
            Self::InvalidTransform(s) => CssStyleTransformParseError::InvalidTransform(s),
            Self::InvalidParenthesis(e) => {
                CssStyleTransformParseError::InvalidParenthesis(e.to_shared())
            }
            Self::WrongNumberOfComponents(e) => CssStyleTransformParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: &e.input,
            },
            Self::NumberParseError(e) => CssStyleTransformParseError::NumberParseError(e.clone()),
            Self::PixelValueParseError(e) => {
                CssStyleTransformParseError::PixelValueParseError(e.to_shared())
            }
            Self::AngleValueParseError(e) => {
                CssStyleTransformParseError::AngleValueParseError(e.to_shared())
            }
            Self::PercentageValueParseError(e) => {
                CssStyleTransformParseError::PercentageValueParseError(e.clone())
            }
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
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components: expected {}, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid pixel value: {}", e),
}}
impl_from! { CssPixelValueParseError<'a>, CssStyleTransformOriginParseError::PixelValueParseError }

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssStyleTransformOriginParseErrorOwned {
    WrongNumberOfComponents(WrongComponentCountError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStyleTransformOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStyleTransformOriginParseErrorOwned {
        match self {
            Self::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            }),
            Self::PixelValueParseError(e) => {
                CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleTransformOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleTransformOriginParseError<'a> {
        match self {
            Self::WrongNumberOfComponents(e) => CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: &e.input,
            },
            Self::PixelValueParseError(e) => {
                CssStyleTransformOriginParseError::PixelValueParseError(e.to_shared())
            }
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
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components: expected {}, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid pixel value: {}", e),
}}
impl_from! { CssPixelValueParseError<'a>, CssStylePerspectiveOriginParseError::PixelValueParseError }

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssStylePerspectiveOriginParseErrorOwned {
    WrongNumberOfComponents(WrongComponentCountError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStylePerspectiveOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStylePerspectiveOriginParseErrorOwned {
        match self {
            Self::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            }),
            Self::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStylePerspectiveOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStylePerspectiveOriginParseError<'a> {
        match self {
            Self::WrongNumberOfComponents(e) => CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: &e.input,
            },
            Self::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssBackfaceVisibilityParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(CssBackfaceVisibilityParseError<'a>);
impl_display! { CssBackfaceVisibilityParseError<'a>, {
    InvalidValue(s) => format!("Invalid value for backface-visibility: \"{}\", expected \"visible\" or \"hidden\"", s),
}}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssBackfaceVisibilityParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> CssBackfaceVisibilityParseError<'a> {
    pub fn to_contained(&self) -> CssBackfaceVisibilityParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                CssBackfaceVisibilityParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

impl CssBackfaceVisibilityParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBackfaceVisibilityParseError<'a> {
        match self {
            Self::InvalidValue(s) => CssBackfaceVisibilityParseError::InvalidValue(s),
        }
    }
}

// -- Parsers --

#[cfg(feature = "parser")]
pub fn parse_style_transform_vec<'a>(
    input: &'a str,
) -> Result<StyleTransformVec, CssStyleTransformParseError<'a>> {
    crate::props::basic::parse::split_string_respect_whitespace(input)
        .iter()
        .map(|i| parse_style_transform(i))
        .collect::<Result<Vec<_>, _>>()
        .map(Into::into)
}

#[cfg(feature = "parser")]
pub fn parse_style_transform<'a>(
    input: &'a str,
) -> Result<StyleTransform, CssStyleTransformParseError<'a>> {
    let (transform_type, transform_values) = parse_parentheses(
        input,
        &[
            "matrix",
            "matrix3d",
            "translate",
            "translate3d",
            "translateX",
            "translateY",
            "translateZ",
            "rotate",
            "rotate3d",
            "rotateX",
            "rotateY",
            "rotateZ",
            "scale",
            "scale3d",
            "scaleX",
            "scaleY",
            "scaleZ",
            "skew",
            "skewX",
            "skewY",
            "perspective",
        ],
    )?;

    fn get_numbers<'a>(
        input: &'a str,
        expected: usize,
    ) -> Result<Vec<f32>, CssStyleTransformParseError<'a>> {
        let numbers: Vec<_> = input
            .split(',')
            .map(|s| s.trim().parse::<f32>())
            .collect::<Result<_, _>>()?;
        if numbers.len() != expected {
            Err(CssStyleTransformParseError::WrongNumberOfComponents {
                expected,
                got: numbers.len(),
                input,
            })
        } else {
            Ok(numbers)
        }
    }

    match transform_type {
        "matrix" => {
            let nums = get_numbers(transform_values, 6)?;
            Ok(StyleTransform::Matrix(StyleTransformMatrix2D {
                a: FloatValue::new(nums[0]),
                b: FloatValue::new(nums[1]),
                c: FloatValue::new(nums[2]),
                d: FloatValue::new(nums[3]),
                tx: FloatValue::new(nums[4]),
                ty: FloatValue::new(nums[5]),
            }))
        }
        "matrix3d" => {
            let nums = get_numbers(transform_values, 16)?;
            Ok(StyleTransform::Matrix3D(StyleTransformMatrix3D {
                m11: FloatValue::new(nums[0]),
                m12: FloatValue::new(nums[1]),
                m13: FloatValue::new(nums[2]),
                m14: FloatValue::new(nums[3]),
                m21: FloatValue::new(nums[4]),
                m22: FloatValue::new(nums[5]),
                m23: FloatValue::new(nums[6]),
                m24: FloatValue::new(nums[7]),
                m31: FloatValue::new(nums[8]),
                m32: FloatValue::new(nums[9]),
                m33: FloatValue::new(nums[10]),
                m34: FloatValue::new(nums[11]),
                m41: FloatValue::new(nums[12]),
                m42: FloatValue::new(nums[13]),
                m43: FloatValue::new(nums[14]),
                m44: FloatValue::new(nums[15]),
            }))
        }
        "translate" => {
            let components: Vec<_> = transform_values.split(',').collect();

            // translate() takes exactly 1 or 2 parameters (x, or x and y)
            if components.len() > 2 {
                return Err(CssStyleTransformParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: components.len(),
                    input: transform_values,
                });
            }

            let x = parse_pixel_value(
                components
                    .get(0)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 0,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            let y = parse_pixel_value(
                components
                    .get(1)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 1,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            Ok(StyleTransform::Translate(StyleTransformTranslate2D {
                x,
                y,
            }))
        }
        "translate3d" => {
            let components: Vec<_> = transform_values.split(',').collect();
            let x = parse_pixel_value(
                components
                    .get(0)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 3,
                        got: 0,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            let y = parse_pixel_value(
                components
                    .get(1)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 3,
                        got: 1,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            let z = parse_pixel_value(
                components
                    .get(2)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 3,
                        got: 2,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            Ok(StyleTransform::Translate3D(StyleTransformTranslate3D {
                x,
                y,
                z,
            }))
        }
        "translateX" => Ok(StyleTransform::TranslateX(parse_pixel_value(
            transform_values,
        )?)),
        "translateY" => Ok(StyleTransform::TranslateY(parse_pixel_value(
            transform_values,
        )?)),
        "translateZ" => Ok(StyleTransform::TranslateZ(parse_pixel_value(
            transform_values,
        )?)),
        "rotate" => Ok(StyleTransform::Rotate(parse_angle_value(transform_values)?)),
        "rotate3d" => {
            let nums = get_numbers(transform_values, 4)?;
            Ok(StyleTransform::Rotate3D(StyleTransformRotate3D {
                x: FloatValue::new(nums[0]),
                y: FloatValue::new(nums[1]),
                z: FloatValue::new(nums[2]),
                angle: AngleValue::deg(nums[3]),
            }))
        }
        "rotateX" => Ok(StyleTransform::RotateX(parse_angle_value(
            transform_values,
        )?)),
        "rotateY" => Ok(StyleTransform::RotateY(parse_angle_value(
            transform_values,
        )?)),
        "rotateZ" => Ok(StyleTransform::RotateZ(parse_angle_value(
            transform_values,
        )?)),
        "scale" => {
            let nums = get_numbers(transform_values, 2)?;
            Ok(StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::new(nums[0]),
                y: FloatValue::new(nums[1]),
            }))
        }
        "scale3d" => {
            let nums = get_numbers(transform_values, 3)?;
            Ok(StyleTransform::Scale3D(StyleTransformScale3D {
                x: FloatValue::new(nums[0]),
                y: FloatValue::new(nums[1]),
                z: FloatValue::new(nums[2]),
            }))
        }
        "scaleX" => Ok(StyleTransform::ScaleX(PercentageValue::new(
            transform_values.trim().parse::<f32>()? * 100.0,
        ))),
        "scaleY" => Ok(StyleTransform::ScaleY(PercentageValue::new(
            transform_values.trim().parse::<f32>()? * 100.0,
        ))),
        "scaleZ" => Ok(StyleTransform::ScaleZ(PercentageValue::new(
            transform_values.trim().parse::<f32>()? * 100.0,
        ))),
        "skew" => {
            let components: Vec<_> = transform_values.split(',').collect();
            let x = parse_angle_value(
                components
                    .get(0)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 0,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            let y = parse_angle_value(
                components
                    .get(1)
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 1,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            Ok(StyleTransform::Skew(StyleTransformSkew2D { x, y }))
        }
        "skewX" => Ok(StyleTransform::SkewX(parse_angle_value(transform_values)?)),
        "skewY" => Ok(StyleTransform::SkewY(parse_angle_value(transform_values)?)),
        "perspective" => Ok(StyleTransform::Perspective(parse_pixel_value(
            transform_values,
        )?)),
        _ => unreachable!(),
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_transform_origin<'a>(
    input: &'a str,
) -> Result<StyleTransformOrigin, CssStyleTransformOriginParseError<'a>> {
    let components: Vec<_> = input.trim().split_whitespace().collect();
    if components.len() != 2 {
        return Err(CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 2,
            got: components.len(),
            input,
        });
    }

    // Helper to parse position keywords or pixel values
    fn parse_position_component(
        s: &str,
        is_horizontal: bool,
    ) -> Result<PixelValue, CssPixelValueParseError> {
        match s.trim() {
            "left" if is_horizontal => Ok(PixelValue::percent(0.0)),
            "center" => Ok(PixelValue::percent(50.0)),
            "right" if is_horizontal => Ok(PixelValue::percent(100.0)),
            "top" if !is_horizontal => Ok(PixelValue::percent(0.0)),
            "bottom" if !is_horizontal => Ok(PixelValue::percent(100.0)),
            _ => parse_pixel_value(s),
        }
    }

    let x = parse_position_component(components[0], true)?;
    let y = parse_position_component(components[1], false)?;
    Ok(StyleTransformOrigin { x, y })
}

#[cfg(feature = "parser")]
pub fn parse_style_perspective_origin<'a>(
    input: &'a str,
) -> Result<StylePerspectiveOrigin, CssStylePerspectiveOriginParseError<'a>> {
    let components: Vec<_> = input.trim().split_whitespace().collect();
    if components.len() != 2 {
        return Err(
            CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: 2,
                got: components.len(),
                input,
            },
        );
    }
    let x = parse_pixel_value(components[0])?;
    let y = parse_pixel_value(components[1])?;
    Ok(StylePerspectiveOrigin { x, y })
}

#[cfg(feature = "parser")]
pub fn parse_style_backface_visibility<'a>(
    input: &'a str,
) -> Result<StyleBackfaceVisibility, CssBackfaceVisibilityParseError<'a>> {
    match input.trim() {
        "visible" => Ok(StyleBackfaceVisibility::Visible),
        "hidden" => Ok(StyleBackfaceVisibility::Hidden),
        _ => Err(CssBackfaceVisibilityParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transform_vec() {
        let result =
            parse_style_transform_vec("translateX(10px) rotate(90deg) scale(0.5, 0.5)").unwrap();
        assert_eq!(result.len(), 3);
        assert!(matches!(
            result.as_slice()[0],
            StyleTransform::TranslateX(_)
        ));
        assert!(matches!(result.as_slice()[1], StyleTransform::Rotate(_)));
        assert!(matches!(result.as_slice()[2], StyleTransform::Scale(_)));
    }

    #[test]
    fn test_parse_transform_functions() {
        // Translate
        assert_eq!(
            parse_style_transform("translateX(50%)").unwrap(),
            StyleTransform::TranslateX(PixelValue::percent(50.0))
        );
        let translate = parse_style_transform("translate(10px, -20px)").unwrap();
        if let StyleTransform::Translate(t) = translate {
            assert_eq!(t.x, PixelValue::px(10.0));
            assert_eq!(t.y, PixelValue::px(-20.0));
        } else {
            panic!("Expected Translate");
        }

        // Scale
        assert_eq!(
            parse_style_transform("scaleY(1.2)").unwrap(),
            StyleTransform::ScaleY(PercentageValue::new(120.0))
        );
        let scale = parse_style_transform("scale(2, 0.5)").unwrap();
        if let StyleTransform::Scale(s) = scale {
            assert_eq!(s.x.get(), 2.0);
            assert_eq!(s.y.get(), 0.5);
        } else {
            panic!("Expected Scale");
        }

        // Rotate
        assert_eq!(
            parse_style_transform("rotate(0.25turn)").unwrap(),
            StyleTransform::Rotate(AngleValue::turn(0.25))
        );

        // Skew
        assert_eq!(
            parse_style_transform("skewX(-10deg)").unwrap(),
            StyleTransform::SkewX(AngleValue::deg(-10.0))
        );
        let skew = parse_style_transform("skew(20deg, 30deg)").unwrap();
        if let StyleTransform::Skew(s) = skew {
            assert_eq!(s.x, AngleValue::deg(20.0));
            assert_eq!(s.y, AngleValue::deg(30.0));
        } else {
            panic!("Expected Skew");
        }
    }

    #[test]
    fn test_parse_transform_origin() {
        let result = parse_style_transform_origin("50% 50%").unwrap();
        assert_eq!(result.x, PixelValue::percent(50.0));
        assert_eq!(result.y, PixelValue::percent(50.0));

        let result = parse_style_transform_origin("left top").unwrap();
        assert_eq!(result.x, PixelValue::percent(0.0)); // keywords not yet supported, but parse as 0px
        assert_eq!(result.y, PixelValue::percent(0.0));

        let result = parse_style_transform_origin("20px bottom").unwrap();
        assert_eq!(result.x, PixelValue::px(20.0));
        assert_eq!(result.y, PixelValue::percent(100.0)); // keywords not yet supported
    }

    #[test]
    fn test_parse_backface_visibility() {
        assert_eq!(
            parse_style_backface_visibility("visible").unwrap(),
            StyleBackfaceVisibility::Visible
        );
        assert_eq!(
            parse_style_backface_visibility("hidden").unwrap(),
            StyleBackfaceVisibility::Hidden
        );
        assert!(parse_style_backface_visibility("none").is_err());
    }

    #[test]
    fn test_parse_transform_errors() {
        // Wrong function name
        assert!(parse_style_transform("translatex(10px)").is_err());
        // Wrong number of args
        assert!(parse_style_transform("scale(1)").is_err());
        assert!(parse_style_transform("translate(1, 2, 3)").is_err());
        // Invalid value
        assert!(parse_style_transform("rotate(10px)").is_err());
        assert!(parse_style_transform("translateX(auto)").is_err());
    }
}
