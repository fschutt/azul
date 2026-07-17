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
    codegen::format::GetHash,
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
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
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
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
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
impl crate::codegen::format::FormatAsRustCode for StylePerspectiveOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StylePerspectiveOrigin {{ x: {}, y: {} }}",
            crate::codegen::format::format_pixel_value(&self.x),
            crate::codegen::format::format_pixel_value(&self.y)
        )
    }
}

// Formatting to Rust code for StyleTransformOrigin
impl crate::codegen::format::FormatAsRustCode for StyleTransformOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleTransformOrigin {{ x: {}, y: {} }}",
            crate::codegen::format::format_pixel_value(&self.x),
            crate::codegen::format::format_pixel_value(&self.y)
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
            Self::Hidden => "hidden",
            Self::Visible => "visible",
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
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
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
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// Formatting to Rust code for StyleTransformVec
impl crate::codegen::format::FormatAsRustCode for StyleTransformVec {
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
            Self::Matrix(m) => format!(
                "matrix({}, {}, {}, {}, {}, {})",
                m.a, m.b, m.c, m.d, m.tx, m.ty
            ),
            Self::Matrix3D(m) => format!(
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
            Self::Translate(t) => format!("translate({}, {})", t.x, t.y),
            Self::Translate3D(t) => format!("translate3d({}, {}, {})", t.x, t.y, t.z),
            Self::TranslateX(x) => format!("translateX({x})"),
            Self::TranslateY(y) => format!("translateY({y})"),
            Self::TranslateZ(z) => format!("translateZ({z})"),
            Self::Rotate(r) => format!("rotate({r})"),
            Self::Rotate3D(r) => {
                format!("rotate3d({}, {}, {}, {})", r.x, r.y, r.z, r.angle)
            }
            Self::RotateX(x) => format!("rotateX({x})"),
            Self::RotateY(y) => format!("rotateY({y})"),
            Self::RotateZ(z) => format!("rotateZ({z})"),
            Self::Scale(s) => format!("scale({}, {})", s.x, s.y),
            Self::Scale3D(s) => format!("scale3d({}, {}, {})", s.x, s.y, s.z),
            Self::ScaleX(x) => format!("scaleX({x})"),
            Self::ScaleY(y) => format!("scaleY({y})"),
            Self::ScaleZ(z) => format!("scaleZ({z})"),
            Self::Skew(sk) => format!("skew({}, {})", sk.x, sk.y),
            Self::SkewX(x) => format!("skewX({x})"),
            Self::SkewY(y) => format!("skewY({y})"),
            Self::Perspective(dist) => format!("perspective({dist})"),
        }
    }
}

/// Represents a CSS `matrix(a, b, c, d, tx, ty)` 2D transform function.
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

/// Represents a CSS `matrix3d(...)` 3D transform function (4x4 matrix).
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

/// Represents a CSS `translate(x, y)` 2D translation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate2D {
    pub x: PixelValue,
    pub y: PixelValue,
}

/// Represents a CSS `translate3d(x, y, z)` 3D translation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate3D {
    pub x: PixelValue,
    pub y: PixelValue,
    pub z: PixelValue,
}

/// Represents a CSS `rotate3d(x, y, z, angle)` 3D rotation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformRotate3D {
    pub x: FloatValue,
    pub y: FloatValue,
    pub z: FloatValue,
    pub angle: AngleValue,
}

/// Represents a CSS `scale(x, y)` 2D scaling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale2D {
    pub x: FloatValue,
    pub y: FloatValue,
}

/// Represents a CSS `scale3d(x, y, z)` 3D scaling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale3D {
    pub x: FloatValue,
    pub y: FloatValue,
    pub z: FloatValue,
}

/// Represents a CSS `skew(x, y)` 2D skew transformation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformSkew2D {
    pub x: AngleValue,
    pub y: AngleValue,
}

// -- Errors --

#[derive(Clone, PartialEq, Eq)]
pub enum CssStyleTransformParseError<'a> {
    InvalidTransform(&'a str),
    InvalidParenthesis(ParenthesisParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    NumberParseError(ParseFloatError),
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
// Written out (not impl_from!): ParseFloatError carries no lifetime, so the
// macro's `<'a>` would be used only by the target type (single_use_lifetimes).
impl From<ParseFloatError> for CssStyleTransformParseError<'_> {
    fn from(e: ParseFloatError) -> Self {
        Self::NumberParseError(e)
    }
}

impl From<PercentageParseError> for CssStyleTransformParseError<'_> {
    fn from(p: PercentageParseError) -> Self {
        Self::PercentageValueParseError(p)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssStyleTransformParseErrorOwned {
    InvalidTransform(AzString),
    InvalidParenthesis(ParenthesisParseErrorOwned),
    WrongNumberOfComponents(WrongComponentCountError),
    NumberParseError(crate::props::basic::error::ParseFloatError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
    AngleValueParseError(CssAngleValueParseErrorOwned),
    PercentageValueParseError(PercentageParseError),
}

impl CssStyleTransformParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssStyleTransformParseErrorOwned {
        match self {
            Self::InvalidTransform(s) => {
                CssStyleTransformParseErrorOwned::InvalidTransform((*s).to_string().into())
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
                input: (*input).to_string().into(),
            }),
            Self::NumberParseError(e) => {
                CssStyleTransformParseErrorOwned::NumberParseError(e.clone().into())
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
    #[must_use] pub fn to_shared(&self) -> CssStyleTransformParseError<'_> {
        match self {
            Self::InvalidTransform(s) => CssStyleTransformParseError::InvalidTransform(s),
            Self::InvalidParenthesis(e) => {
                CssStyleTransformParseError::InvalidParenthesis(e.to_shared())
            }
            Self::WrongNumberOfComponents(e) => CssStyleTransformParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: e.input.as_str(),
            },
            Self::NumberParseError(e) => CssStyleTransformParseError::NumberParseError(e.to_std()),
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

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssStyleTransformOriginParseErrorOwned {
    WrongNumberOfComponents(WrongComponentCountError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl CssStyleTransformOriginParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssStyleTransformOriginParseErrorOwned {
        match self {
            Self::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                expected: *expected,
                got: *got,
                input: (*input).to_string().into(),
            }),
            Self::PixelValueParseError(e) => {
                CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleTransformOriginParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssStyleTransformOriginParseError<'_> {
        match self {
            Self::WrongNumberOfComponents(e) => CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: e.input.as_str(),
            },
            Self::PixelValueParseError(e) => {
                CssStyleTransformOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssStylePerspectiveOriginParseErrorOwned {
    WrongNumberOfComponents(WrongComponentCountError),
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl CssStylePerspectiveOriginParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssStylePerspectiveOriginParseErrorOwned {
        match self {
            Self::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                expected: *expected,
                got: *got,
                input: (*input).to_string().into(),
            }),
            Self::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStylePerspectiveOriginParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssStylePerspectiveOriginParseError<'_> {
        match self {
            Self::WrongNumberOfComponents(e) => CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: e.expected,
                got: e.got,
                input: e.input.as_str(),
            },
            Self::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CssBackfaceVisibilityParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(CssBackfaceVisibilityParseError<'a>);
impl_display! { CssBackfaceVisibilityParseError<'a>, {
    InvalidValue(s) => format!("Invalid value for backface-visibility: \"{}\", expected \"visible\" or \"hidden\"", s),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssBackfaceVisibilityParseErrorOwned {
    InvalidValue(AzString),
}

impl CssBackfaceVisibilityParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBackfaceVisibilityParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                CssBackfaceVisibilityParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl CssBackfaceVisibilityParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssBackfaceVisibilityParseError<'_> {
        match self {
            Self::InvalidValue(s) => CssBackfaceVisibilityParseError::InvalidValue(s),
        }
    }
}

// -- Parsers --

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `transform-vec` value.
pub fn parse_style_transform_vec(
    input: &str,
) -> Result<StyleTransformVec, CssStyleTransformParseError<'_>> {
    crate::props::basic::parse::split_string_respect_whitespace(input)
        .iter()
        .map(|i| parse_style_transform(i))
        .collect::<Result<Vec<_>, _>>()
        .map(Into::into)
}

#[cfg(feature = "parser")]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `transform` value.
pub fn parse_style_transform(
    input: &str,
) -> Result<StyleTransform, CssStyleTransformParseError<'_>> {
    fn get_numbers(
        input: &str,
        expected: usize,
    ) -> Result<Vec<f32>, CssStyleTransformParseError<'_>> {
        let numbers: Vec<_> = input
            .split(',')
            .map(|s| s.trim().parse::<f32>())
            .collect::<Result<_, _>>()?;
        if numbers.len() == expected {
            Ok(numbers)
        } else {
            Err(CssStyleTransformParseError::WrongNumberOfComponents {
                expected,
                got: numbers.len(),
                input,
            })
        }
    }

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
                components.first()
                    .ok_or(CssStyleTransformParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 0,
                        input: transform_values,
                    })?
                    .trim(),
            )?;
            let y = match components.get(1) {
                Some(c) => parse_pixel_value(c.trim())?,
                None => PixelValue::px(0.0),
            };
            Ok(StyleTransform::Translate(StyleTransformTranslate2D {
                x,
                y,
            }))
        }
        "translate3d" => {
            let components: Vec<_> = transform_values.split(',').collect();
            let x = parse_pixel_value(
                components.first()
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
            let parts: Vec<_> = transform_values.splitn(4, ',').collect();
            if parts.len() != 4 {
                return Err(CssStyleTransformParseError::WrongNumberOfComponents {
                    expected: 4,
                    got: parts.len(),
                    input: transform_values,
                });
            }
            let x = parts[0].trim().parse::<f32>()?;
            let y = parts[1].trim().parse::<f32>()?;
            let z = parts[2].trim().parse::<f32>()?;
            let angle = parse_angle_value(parts[3].trim())?;
            Ok(StyleTransform::Rotate3D(StyleTransformRotate3D {
                x: FloatValue::new(x),
                y: FloatValue::new(y),
                z: FloatValue::new(z),
                angle,
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
            let parts: Vec<_> = transform_values.split(',').collect();
            if parts.is_empty() || parts.len() > 2 {
                return Err(CssStyleTransformParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: parts.len(),
                    input: transform_values,
                });
            }
            let x = parts[0].trim().parse::<f32>()?;
            let y = if parts.len() == 2 {
                parts[1].trim().parse::<f32>()?
            } else {
                x
            };
            Ok(StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::new(x),
                y: FloatValue::new(y),
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
            if components.is_empty() || components.len() > 2 {
                return Err(CssStyleTransformParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: components.len(),
                    input: transform_values,
                });
            }
            let x = parse_angle_value(components[0].trim())?;
            let y = match components.get(1) {
                Some(c) => parse_angle_value(c.trim())?,
                None => AngleValue::deg(0.0),
            };
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `transform-origin` value.
pub fn parse_style_transform_origin(
    input: &str,
) -> Result<StyleTransformOrigin, CssStyleTransformOriginParseError<'_>> {
    // Helper to parse position keywords or pixel values
    fn parse_position_component(
        s: &str,
        is_horizontal: bool,
    ) -> Result<PixelValue, CssPixelValueParseError<'_>> {
        match s.trim() {
            "left" if is_horizontal => Ok(PixelValue::percent(0.0)),
            "center" => Ok(PixelValue::percent(50.0)),
            "right" if is_horizontal => Ok(PixelValue::percent(100.0)),
            "top" if !is_horizontal => Ok(PixelValue::percent(0.0)),
            "bottom" if !is_horizontal => Ok(PixelValue::percent(100.0)),
            _ => parse_pixel_value(s),
        }
    }

    let components: Vec<_> = input.split_whitespace().collect();
    if components.len() != 2 {
        return Err(CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 2,
            got: components.len(),
            input,
        });
    }

    let x = parse_position_component(components[0], true)?;
    let y = parse_position_component(components[1], false)?;
    Ok(StyleTransformOrigin { x, y })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `perspective-origin` value.
pub fn parse_style_perspective_origin(
    input: &str,
) -> Result<StylePerspectiveOrigin, CssStylePerspectiveOriginParseError<'_>> {
    let components: Vec<_> = input.split_whitespace().collect();
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `backface-visibility` value.
pub fn parse_style_backface_visibility(
    input: &str,
) -> Result<StyleBackfaceVisibility, CssBackfaceVisibilityParseError<'_>> {
    match input.trim() {
        "visible" => Ok(StyleBackfaceVisibility::Visible),
        "hidden" => Ok(StyleBackfaceVisibility::Hidden),
        _ => Err(CssBackfaceVisibilityParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
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
        assert_eq!(result.x, PixelValue::percent(0.0));
        assert_eq!(result.y, PixelValue::percent(0.0));

        let result = parse_style_transform_origin("20px bottom").unwrap();
        assert_eq!(result.x, PixelValue::px(20.0));
        assert_eq!(result.y, PixelValue::percent(100.0));
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
        assert!(parse_style_transform("translate(1, 2, 3)").is_err());
        // Single-arg forms (CSS spec compliant)
        let scale1 = parse_style_transform("scale(2)").unwrap();
        if let StyleTransform::Scale(s) = scale1 {
            assert_eq!(s.x.get(), 2.0);
            assert_eq!(s.y.get(), 2.0);
        } else {
            panic!("Expected Scale");
        }
        let translate1 = parse_style_transform("translate(10px)").unwrap();
        if let StyleTransform::Translate(t) = translate1 {
            assert_eq!(t.x, PixelValue::px(10.0));
            assert_eq!(t.y, PixelValue::px(0.0));
        } else {
            panic!("Expected Translate");
        }
        let skew1 = parse_style_transform("skew(20deg)").unwrap();
        if let StyleTransform::Skew(s) = skew1 {
            assert_eq!(s.x, AngleValue::deg(20.0));
            assert_eq!(s.y, AngleValue::deg(0.0));
        } else {
            panic!("Expected Skew");
        }
        // rotate3d with angle unit
        let rot3d = parse_style_transform("rotate3d(1, 0, 0, 45deg)").unwrap();
        if let StyleTransform::Rotate3D(r) = rot3d {
            assert_eq!(r.x.get(), 1.0);
            assert_eq!(r.angle, AngleValue::deg(45.0));
        } else {
            panic!("Expected Rotate3D");
        }
        // Invalid value
        assert!(parse_style_transform("rotate(10px)").is_err());
        assert!(parse_style_transform("translateX(auto)").is_err());
    }
}

#[cfg(all(test, feature = "parser"))]
#[allow(clippy::too_many_lines, clippy::float_cmp)]
mod autotest_generated {
    // Tests compare parsed values against exact source literals, and deliberately
    // feed NaN/inf through the numeric encoders.

    use super::*;
    use crate::props::basic::length::SizeMetric;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// Every `FloatValue` is stored as an `isize`, so `get()` can never be
    /// NaN/inf no matter what went in. Used as a blanket invariant below.
    fn assert_encodable(pv: PixelValue) {
        assert!(pv.number.get().is_finite());
    }

    fn all_roundtrippable_transforms() -> Vec<StyleTransform> {
        vec![
            StyleTransform::Matrix(StyleTransformMatrix2D::default()),
            StyleTransform::Matrix3D(StyleTransformMatrix3D::default()),
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(10.0),
                y: PixelValue::px(-20.0),
            }),
            StyleTransform::Translate3D(StyleTransformTranslate3D {
                x: PixelValue::px(1.0),
                y: PixelValue::em(2.0),
                z: PixelValue::pt(-3.5),
            }),
            StyleTransform::TranslateX(PixelValue::percent(50.0)),
            StyleTransform::TranslateY(PixelValue::px(0.0)),
            StyleTransform::TranslateZ(PixelValue::rem(1.25)),
            StyleTransform::Rotate(AngleValue::deg(90.0)),
            StyleTransform::Rotate3D(StyleTransformRotate3D {
                x: FloatValue::new(1.0),
                y: FloatValue::new(0.0),
                z: FloatValue::new(0.0),
                angle: AngleValue::turn(0.25),
            }),
            StyleTransform::RotateX(AngleValue::rad(1.5)),
            StyleTransform::RotateY(AngleValue::grad(100.0)),
            StyleTransform::RotateZ(AngleValue::deg(-45.0)),
            StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::new(2.0),
                y: FloatValue::new(0.5),
            }),
            StyleTransform::Scale3D(StyleTransformScale3D {
                x: FloatValue::new(1.0),
                y: FloatValue::new(-1.0),
                z: FloatValue::new(0.25),
            }),
            StyleTransform::Skew(StyleTransformSkew2D {
                x: AngleValue::deg(20.0),
                y: AngleValue::deg(30.0),
            }),
            StyleTransform::SkewX(AngleValue::deg(-10.0)),
            StyleTransform::SkewY(AngleValue::deg(10.0)),
            StyleTransform::Perspective(PixelValue::px(500.0)),
        ]
    }

    // =====================================================================
    // parse_style_transform  --  malformed / boundary / unicode
    // =====================================================================

    #[test]
    fn transform_rejects_empty_and_whitespace_only_input() {
        for input in ["", "   ", "\t\n", "\r\n\t "] {
            let err = parse_style_transform(input).unwrap_err();
            assert!(
                matches!(
                    err,
                    CssStyleTransformParseError::InvalidParenthesis(
                        ParenthesisParseError::EmptyInput
                    )
                ),
                "expected EmptyInput for {input:?}, got {err}"
            );
        }
    }

    #[test]
    fn transform_rejects_garbage_without_panicking() {
        // No opening brace at all.
        assert!(matches!(
            parse_style_transform("garbage").unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::NoOpeningBraceFound
            )
        ));
        // Opening brace, no closing brace.
        assert!(matches!(
            parse_style_transform("rotate(90deg").unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::NoClosingBraceFound
            )
        ));
        // Known-but-miscased function name is NOT accepted (CSS is case-insensitive
        // for function names; azul's stopword table is case-sensitive).
        assert!(matches!(
            parse_style_transform("translatex(10px)").unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::StopWordNotFound("translatex")
            )
        ));
        assert!(matches!(
            parse_style_transform("ROTATE(90deg)").unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::StopWordNotFound("ROTATE")
            )
        ));
        // Random byte soup, none of which forms a grammar.
        for input in [
            "((((",
            ")",
            "()",
            "(rotate)",
            "rotate",
            ";;;",
            "\0(\0)",
            "-1",
            "rotate(,,,,)",
            "matrix(,)",
        ] {
            assert!(
                parse_style_transform(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn transform_never_hits_the_unreachable_arm_for_near_miss_stopwords() {
        // parse_style_transform ends in `_ => unreachable!()`; it is only sound as
        // long as parse_parentheses can never hand back a non-listed stopword.
        // Probe names that are prefixes/suffixes/case-variants of real ones.
        for name in [
            "translat", "translateXX", "xtranslateX", "rotate3", "rotate3D", "scale4d", "skewZ",
            "perspectives", "matrix2d", "MATRIX", "", " rotate",
        ] {
            let input = alloc::format!("{name}(1)");
            let res = parse_style_transform(&input);
            // Either a clean parse error or (for " rotate", which trims to "rotate")
            // a normal result - but never a panic.
            let _ = res;
        }
        // " rotate(1)" trims down to a valid rotate with a bare-number degree.
        assert_eq!(
            parse_style_transform("  rotate(1)  ").unwrap(),
            StyleTransform::Rotate(AngleValue::deg(1.0))
        );
    }

    #[test]
    fn transform_handles_non_ascii_and_multibyte_input() {
        // parse_parentheses slices `input[..first_open_brace]`; a multibyte char
        // right before the '(' must not split a UTF-8 boundary.
        for input in [
            "\u{1F600}",
            "\u{1F600}(1)",
            "rotate(\u{1F600})",
            "rotate\u{0301}(1deg)",
            "\u{1F600}rotate(1deg)",
            "translateX(\u{1F600}px)",
            "translate(\u{1F600}, \u{1F600})",
            "matrix3d(\u{4F60}\u{597D})",
            "sk\u{0435}wX(10deg)", // cyrillic 'е' homoglyph
        ] {
            assert!(
                parse_style_transform(input).is_err(),
                "expected Err for {input:?}"
            );
        }
        assert!(matches!(
            parse_style_transform("rotate(\u{1F600})").unwrap_err(),
            CssStyleTransformParseError::AngleValueParseError(
                CssAngleValueParseError::InvalidAngle("\u{1F600}")
            )
        ));
    }

    #[test]
    fn transform_boundary_numbers_saturate_instead_of_panicking() {
        // -0 collapses to +0 in the isize-backed encoding.
        assert_eq!(
            parse_style_transform("translateX(-0)").unwrap(),
            StyleTransform::TranslateX(PixelValue::px(0.0))
        );
        assert_eq!(
            parse_style_transform("translateX(0)").unwrap(),
            StyleTransform::TranslateX(PixelValue::px(0.0))
        );

        // NaN parses as a float (Rust accepts "NaN"), and the f32 -> isize cast
        // maps NaN to 0. So `translateX(NaN)` silently becomes `0px`.
        let StyleTransform::TranslateX(nan_px) = parse_style_transform("translateX(NaN)").unwrap()
        else {
            panic!("expected TranslateX");
        };
        assert_eq!(nan_px.number.number(), 0);
        assert_encodable(nan_px);

        // Infinities (literal, and via decimal overflow) saturate to isize::MAX/MIN.
        for input in ["translateX(inf)", "translateX(1e400)", "translateX(1e400px)"] {
            let StyleTransform::TranslateX(px) = parse_style_transform(input).unwrap() else {
                panic!("expected TranslateX for {input}");
            };
            assert_eq!(px.number.number(), isize::MAX, "{input}");
            assert_encodable(px);
        }
        let StyleTransform::TranslateX(neg) = parse_style_transform("translateX(-inf)").unwrap()
        else {
            panic!("expected TranslateX");
        };
        assert_eq!(neg.number.number(), isize::MIN);
        assert_encodable(neg);

        // i64::MAX / f64-scale magnitudes: fine, just saturated.
        for input in [
            "translateX(9223372036854775807px)",
            "translateX(-9223372036854775808px)",
            "translateX(1e-400px)",
            "translateX(0.0000000000001px)",
        ] {
            assert!(parse_style_transform(input).is_ok(), "{input}");
        }

        // Angles take the same path: NaN -> 0deg, inf -> saturated.
        assert_eq!(
            parse_style_transform("rotate(NaN)").unwrap(),
            StyleTransform::Rotate(AngleValue::deg(0.0))
        );
        let StyleTransform::Rotate(a) = parse_style_transform("rotate(infdeg)").unwrap() else {
            panic!("expected Rotate");
        };
        assert_eq!(a.number.number(), isize::MAX);

        // scaleX multiplies by 100 before encoding - inf * 100 must not trap.
        let StyleTransform::ScaleX(p) = parse_style_transform("scaleX(NaN)").unwrap() else {
            panic!("expected ScaleX");
        };
        assert_eq!(p, PercentageValue::new(0.0));
        assert!(parse_style_transform("scaleX(inf)").is_ok());
        assert!(parse_style_transform("scaleX(1e40)").is_ok());
    }

    #[test]
    fn transform_component_counts_are_enforced() {
        // matrix wants exactly 6.
        assert!(matches!(
            parse_style_transform("matrix(1,2,3,4,5)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 5,
                ..
            }
        ));
        assert!(matches!(
            parse_style_transform("matrix(1,2,3,4,5,6,7)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 7,
                ..
            }
        ));
        // matrix3d wants exactly 16.
        assert!(matches!(
            parse_style_transform("matrix3d(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 15,
                ..
            }
        ));
        // translate takes at most 2.
        assert!(matches!(
            parse_style_transform("translate(1px, 2px, 3px)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 3,
                ..
            }
        ));
        // scale takes at most 2.
        assert!(matches!(
            parse_style_transform("scale(1, 2, 3)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 3,
                ..
            }
        ));
        // skew takes at most 2.
        assert!(matches!(
            parse_style_transform("skew(1deg, 2deg, 3deg)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 3,
                ..
            }
        ));
        // rotate3d wants exactly 4 (splitn(4) makes >4 fold into the angle, which
        // then fails to parse as an angle).
        assert!(matches!(
            parse_style_transform("rotate3d(1, 0, 0)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 4,
                got: 3,
                ..
            }
        ));
        assert!(parse_style_transform("rotate3d(1, 0, 0, 45deg, 99)").is_err());
        // translate3d wants exactly 3 when short...
        assert!(matches!(
            parse_style_transform("translate3d(1px, 2px)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 3,
                got: 2,
                ..
            }
        ));
        // scale3d wants exactly 3.
        assert!(matches!(
            parse_style_transform("scale3d(1, 2)").unwrap_err(),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 3,
                got: 2,
                ..
            }
        ));
    }

    #[test]
    fn transform_translate3d_silently_ignores_extra_components() {
        // BUG (leniency): unlike matrix/scale3d (which go through get_numbers and
        // check the count), translate3d only indexes [0], [1], [2] and never
        // rejects a 4th+ component. Per CSS this must be a parse error.
        // Pinned as current behaviour so a future fix shows up as a diff here.
        let parsed = parse_style_transform("translate3d(1px, 2px, 3px, 4px, 5px)").unwrap();
        assert_eq!(
            parsed,
            StyleTransform::Translate3D(StyleTransformTranslate3D {
                x: PixelValue::px(1.0),
                y: PixelValue::px(2.0),
                z: PixelValue::px(3.0),
            })
        );
    }

    #[test]
    fn transform_ignores_junk_after_the_closing_paren() {
        // BUG (leniency): parse_parentheses uses find('(') .. rfind(')'), so any
        // trailing junk that contains no ')' is silently dropped. Per CSS,
        // "rotate(90deg)garbage" is invalid. Pinned as current behaviour.
        assert_eq!(
            parse_style_transform("rotate(90deg)garbage").unwrap(),
            StyleTransform::Rotate(AngleValue::deg(90.0))
        );
        assert_eq!(
            parse_style_transform("rotate(90deg) ;drop table").unwrap(),
            StyleTransform::Rotate(AngleValue::deg(90.0))
        );
        // ...but junk containing a ')' gets swallowed INTO the argument, which then
        // fails - so the leniency is content-dependent, not a clean "trim" rule.
        assert!(parse_style_transform("rotate(90deg))").is_err());
        assert!(parse_style_transform("rotate(90deg) rotate(1deg)").is_err());
    }

    #[test]
    fn transform_empty_argument_lists_are_rejected() {
        for input in [
            "matrix()",
            "matrix3d()",
            "translate()",
            "translate3d()",
            "translateX()",
            "translateY()",
            "translateZ()",
            "rotate()",
            "rotate3d()",
            "rotateX()",
            "rotateY()",
            "rotateZ()",
            "scale()",
            "scale3d()",
            "scaleX()",
            "scaleY()",
            "scaleZ()",
            "skew()",
            "skewX()",
            "skewY()",
            "perspective()",
        ] {
            assert!(
                parse_style_transform(input).is_err(),
                "expected Err for {input:?}"
            );
        }
        // Trailing-comma forms, too.
        for input in [
            "translate(10px,)",
            "translate3d(1px,2px,)",
            "scale(2,)",
            "skew(10deg,)",
            "matrix(1,2,3,4,5,)",
        ] {
            assert!(
                parse_style_transform(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn transform_wrong_unit_kinds_are_rejected() {
        // A length where an angle is expected, and vice versa.
        assert!(parse_style_transform("rotate(10px)").is_err());
        assert!(parse_style_transform("rotateX(10px)").is_err());
        assert!(parse_style_transform("skewY(10px)").is_err());
        assert!(parse_style_transform("translateX(10deg)").is_err());
        assert!(parse_style_transform("perspective(10deg)").is_err());
        // Keywords are not lengths.
        assert!(parse_style_transform("translateX(auto)").is_err());
        assert!(parse_style_transform("translateX(none)").is_err());
        // scaleX takes a bare number, NOT a percentage (see the round-trip test).
        assert!(parse_style_transform("scaleX(120%)").is_err());
    }

    #[test]
    fn transform_extremely_long_input_does_not_hang_or_panic() {
        // ~100k-digit number: must scan in linear time and saturate, not panic.
        let huge = alloc::format!("translateX({}px)", "9".repeat(100_000));
        let StyleTransform::TranslateX(px) = parse_style_transform(&huge).unwrap() else {
            panic!("expected TranslateX");
        };
        assert_eq!(px.number.number(), isize::MAX);

        // Long garbage of the same size must simply be an error.
        let junk = alloc::format!("translateX({})", "a".repeat(100_000));
        assert!(parse_style_transform(&junk).is_err());

        // Long stopword: no quadratic blowup in the stopword scan.
        let long_name = alloc::format!("{}(1px)", "x".repeat(100_000));
        assert!(parse_style_transform(&long_name).is_err());
    }

    #[test]
    fn transform_deeply_nested_parens_do_not_stack_overflow() {
        // parse_parentheses is iterative; prove there is no recursion by feeding it
        // 10k levels of nesting.
        let open_only = "rotate(".repeat(10_000);
        assert!(matches!(
            parse_style_transform(&open_only).unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::NoClosingBraceFound
            )
        ));

        let braces = "(".repeat(10_000);
        assert!(matches!(
            parse_style_transform(&braces).unwrap_err(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::StopWordNotFound("")
            )
        ));

        let balanced = alloc::format!(
            "translateX({}1px{})",
            "translateX(".repeat(1_000),
            ")".repeat(1_000)
        );
        assert!(parse_style_transform(&balanced).is_err());

        let closers = ")".repeat(10_000);
        assert!(parse_style_transform(&closers).is_err());
    }

    #[test]
    fn transform_valid_minimal_positive_control() {
        assert_eq!(
            parse_style_transform("rotate(0deg)").unwrap(),
            StyleTransform::Rotate(AngleValue::deg(0.0))
        );
    }

    // =====================================================================
    // parse_style_transform_vec
    // =====================================================================

    #[test]
    fn transform_vec_accepts_empty_and_whitespace_only_input_as_an_empty_list() {
        // NOTE: "" is NOT an error - split_string_respect_whitespace yields zero
        // tokens and the collect() succeeds with an empty Vec. Callers relying on
        // `parse_style_transform_vec("").is_err()` to reject an empty declaration
        // will not get one. Pinned as current behaviour.
        for input in ["", "   ", "\t\n", "\r \n \t"] {
            let v = parse_style_transform_vec(input).unwrap();
            assert_eq!(v.len(), 0, "{input:?}");
        }
    }

    #[test]
    fn transform_vec_propagates_the_first_error() {
        assert!(parse_style_transform_vec("translateX(10px) garbage").is_err());
        assert!(parse_style_transform_vec("garbage translateX(10px)").is_err());
        assert!(parse_style_transform_vec("translateX(10px) rotate(10px)").is_err());
        assert!(parse_style_transform_vec("\u{1F600}").is_err());
    }

    #[test]
    fn transform_vec_keeps_whitespace_inside_parens_together() {
        // "scale(2, 0.5)" contains a space at depth 1 - it must stay one token.
        let v = parse_style_transform_vec("scale(2, 0.5) matrix(1, 0, 0, 1, 0, 0)").unwrap();
        assert_eq!(v.len(), 2);
        assert!(matches!(v.as_slice()[0], StyleTransform::Scale(_)));
        assert!(matches!(v.as_slice()[1], StyleTransform::Matrix(_)));

        // Repeated / redundant whitespace collapses.
        let v = parse_style_transform_vec("  translateX(1px)\t\trotate(2deg)\n ").unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn transform_vec_extremely_long_list_does_not_hang() {
        let long = "translateX(1px) ".repeat(20_000);
        let v = parse_style_transform_vec(&long).unwrap();
        assert_eq!(v.len(), 20_000);
        for t in v.as_slice() {
            assert_eq!(*t, StyleTransform::TranslateX(PixelValue::px(1.0)));
        }
    }

    #[test]
    fn transform_vec_unbalanced_parens_do_not_underflow_the_depth_counter() {
        // split_string_respect_whitespace does `depth -= 1` on every ')' with no
        // floor; a run of closers drives it negative. Must not panic in debug.
        let closers = ")".repeat(10_000);
        assert!(parse_style_transform_vec(&closers).is_err());
        let mixed = alloc::format!("{} {}", ")".repeat(5_000), "(".repeat(5_000));
        assert!(parse_style_transform_vec(&mixed).is_err());
    }

    // =====================================================================
    // parse_style_transform_origin
    // =====================================================================

    #[test]
    fn transform_origin_requires_exactly_two_components() {
        for (input, got) in [("", 0), ("50%", 1), ("left", 1), ("50% 50% 50%", 3)] {
            let err = parse_style_transform_origin(input).unwrap_err();
            assert!(
                matches!(
                    err,
                    CssStyleTransformOriginParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: g,
                        ..
                    } if g == got
                ),
                "{input:?} -> {err}"
            );
        }
        // Whitespace-only collapses to zero components.
        assert!(matches!(
            parse_style_transform_origin("   \t\n ").unwrap_err(),
            CssStyleTransformOriginParseError::WrongNumberOfComponents { got: 0, .. }
        ));
    }

    #[test]
    fn transform_origin_keywords_are_position_sensitive() {
        assert_eq!(
            parse_style_transform_origin("left top").unwrap(),
            StyleTransformOrigin {
                x: PixelValue::percent(0.0),
                y: PixelValue::percent(0.0),
            }
        );
        assert_eq!(
            parse_style_transform_origin("right bottom").unwrap(),
            StyleTransformOrigin {
                x: PixelValue::percent(100.0),
                y: PixelValue::percent(100.0),
            }
        );
        assert_eq!(
            parse_style_transform_origin("center center").unwrap(),
            StyleTransformOrigin::default()
        );
        // BUG (spec deviation): CSS allows the keywords in either order
        // ("top left" == "left top"). Here the horizontal slot rejects
        // "top"/"bottom" and the vertical slot rejects "left"/"right", so the
        // swapped form is an error. Pinned as current behaviour.
        assert!(parse_style_transform_origin("top left").is_err());
        assert!(parse_style_transform_origin("bottom right").is_err());
        assert!(parse_style_transform_origin("left left").is_err());
        assert!(parse_style_transform_origin("top top").is_err());
    }

    #[test]
    fn transform_origin_garbage_and_unicode_do_not_panic() {
        for input in [
            "\u{1F600} \u{1F600}",
            "left \u{1F600}",
            "NaN NaN",
            "auto auto",
            "-- --",
            "10 20",   // bare numbers -> px, actually valid
            "1px;2px", // no whitespace -> 1 component
        ] {
            let _ = parse_style_transform_origin(input);
        }
        // Bare numbers fall through to parse_pixel_value's px default.
        assert_eq!(
            parse_style_transform_origin("10 20").unwrap(),
            StyleTransformOrigin {
                x: PixelValue::px(10.0),
                y: PixelValue::px(20.0),
            }
        );
        assert!(matches!(
            parse_style_transform_origin("\u{1F600} \u{1F600}").unwrap_err(),
            CssStyleTransformOriginParseError::PixelValueParseError(_)
        ));
    }

    #[test]
    fn transform_origin_boundary_numbers_saturate() {
        let o = parse_style_transform_origin("inf% -inf%").unwrap();
        assert_eq!(o.x.number.number(), isize::MAX);
        assert_eq!(o.y.number.number(), isize::MIN);
        assert_encodable(o.x);
        assert_encodable(o.y);

        // NaN -> 0, keeping the metric.
        let o = parse_style_transform_origin("NaNpx NaN%").unwrap();
        assert_eq!(o.x, PixelValue::px(0.0));
        assert_eq!(o.y, PixelValue::percent(0.0));

        let o = parse_style_transform_origin("-0px 1e400px").unwrap();
        assert_eq!(o.x.number.number(), 0);
        assert_eq!(o.y.number.number(), isize::MAX);
    }

    #[test]
    fn transform_origin_extremely_long_input_does_not_hang() {
        let many = "50% ".repeat(20_000);
        assert!(matches!(
            parse_style_transform_origin(&many).unwrap_err(),
            CssStyleTransformOriginParseError::WrongNumberOfComponents { got: 20_000, .. }
        ));
        let huge = alloc::format!("{}px 0px", "9".repeat(100_000));
        assert!(parse_style_transform_origin(&huge).is_ok());
    }

    #[test]
    fn transform_origin_round_trips_through_its_css_repr() {
        for origin in [
            StyleTransformOrigin::default(),
            StyleTransformOrigin {
                x: PixelValue::px(20.0),
                y: PixelValue::percent(100.0),
            },
            StyleTransformOrigin {
                x: PixelValue::em(-1.5),
                y: PixelValue::rem(2.25),
            },
            StyleTransformOrigin {
                x: PixelValue::px(0.0),
                y: PixelValue::px(0.0),
            },
        ] {
            let css = origin.print_as_css_value();
            let reparsed = parse_style_transform_origin(&css)
                .unwrap_or_else(|e| panic!("{css:?} did not re-parse: {e}"));
            assert_eq!(reparsed, origin, "round-trip failed for {css:?}");
        }
    }

    // =====================================================================
    // parse_style_perspective_origin
    // =====================================================================

    #[test]
    fn perspective_origin_requires_exactly_two_components() {
        for (input, got) in [("", 0), ("50%", 1), ("1px 2px 3px", 3)] {
            let err = parse_style_perspective_origin(input).unwrap_err();
            assert!(
                matches!(
                    err,
                    CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: g,
                        ..
                    } if g == got
                ),
                "{input:?} -> {err}"
            );
        }
    }

    #[test]
    fn perspective_origin_does_not_accept_position_keywords() {
        // BUG (spec deviation): CSS `perspective-origin` accepts the same
        // left/center/right/top/bottom keywords as `transform-origin`, but this
        // parser only takes pixel values. Pinned as current behaviour.
        assert!(parse_style_perspective_origin("left top").is_err());
        assert!(parse_style_perspective_origin("center center").is_err());
        assert!(matches!(
            parse_style_perspective_origin("center center").unwrap_err(),
            CssStylePerspectiveOriginParseError::PixelValueParseError(_)
        ));
    }

    #[test]
    fn perspective_origin_garbage_boundary_and_unicode() {
        for input in ["\u{1F600} \u{1F600}", "auto auto", "-- --", "px px"] {
            assert!(
                parse_style_perspective_origin(input).is_err(),
                "expected Err for {input:?}"
            );
        }
        let o = parse_style_perspective_origin("inf -inf").unwrap();
        assert_eq!(o.x.number.number(), isize::MAX);
        assert_eq!(o.y.number.number(), isize::MIN);
        assert_encodable(o.x);
        assert_encodable(o.y);

        let o = parse_style_perspective_origin("NaN -0").unwrap();
        assert_eq!(o.x, PixelValue::px(0.0));
        assert_eq!(o.y, PixelValue::px(0.0));

        let huge = alloc::format!("{}px 0px", "9".repeat(100_000));
        assert!(parse_style_perspective_origin(&huge).is_ok());
    }

    #[test]
    fn perspective_origin_round_trips_through_its_css_repr() {
        for origin in [
            StylePerspectiveOrigin::default(),
            StylePerspectiveOrigin {
                x: PixelValue::px(100.0),
                y: PixelValue::percent(50.0),
            },
            StylePerspectiveOrigin {
                x: PixelValue::pt(-3.5),
                y: PixelValue::cm(1.0),
            },
        ] {
            let css = origin.print_as_css_value();
            let reparsed = parse_style_perspective_origin(&css)
                .unwrap_or_else(|e| panic!("{css:?} did not re-parse: {e}"));
            assert_eq!(reparsed, origin, "round-trip failed for {css:?}");
        }
    }

    // =====================================================================
    // parse_style_backface_visibility
    // =====================================================================

    #[test]
    fn backface_visibility_accepts_only_the_two_keywords() {
        assert_eq!(
            parse_style_backface_visibility("visible").unwrap(),
            StyleBackfaceVisibility::Visible
        );
        assert_eq!(
            parse_style_backface_visibility("hidden").unwrap(),
            StyleBackfaceVisibility::Hidden
        );
        // Surrounding whitespace is trimmed.
        assert_eq!(
            parse_style_backface_visibility("  \t visible \n ").unwrap(),
            StyleBackfaceVisibility::Visible
        );
        // Everything else is rejected - including case variants, substrings,
        // both keywords at once and zero-width joiners.
        for input in [
            "",
            "   ",
            "Visible",
            "HIDDEN",
            "visible hidden",
            "vis",
            "visiblee",
            "none",
            "0",
            "NaN",
            "\u{1F600}",
            "visible\u{200B}",
            "hidden;",
        ] {
            assert!(
                parse_style_backface_visibility(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn backface_visibility_error_carries_the_untrimmed_input() {
        // The match is on `input.trim()` but the error is built from `input`,
        // so the original (untrimmed) slice is what shows up in the message.
        let err = parse_style_backface_visibility("  bogus  ").unwrap_err();
        assert_eq!(err, CssBackfaceVisibilityParseError::InvalidValue("  bogus  "));
        assert!(alloc::format!("{err}").contains("  bogus  "));
    }

    #[test]
    fn backface_visibility_extremely_long_input_does_not_hang() {
        let huge = "visible".repeat(100_000);
        assert!(parse_style_backface_visibility(&huge).is_err());
    }

    #[test]
    fn backface_visibility_round_trips_through_its_css_repr() {
        for v in [
            StyleBackfaceVisibility::Visible,
            StyleBackfaceVisibility::Hidden,
        ] {
            let css = v.print_as_css_value();
            assert_eq!(parse_style_backface_visibility(&css).unwrap(), v);
        }
        assert_eq!(
            StyleBackfaceVisibility::default(),
            StyleBackfaceVisibility::Visible
        );
    }

    // =====================================================================
    // StyleTransform / StyleTransformVec round-trips (encode == decode)
    // =====================================================================

    #[test]
    fn transform_print_parse_round_trip() {
        for t in all_roundtrippable_transforms() {
            let css = t.print_as_css_value();
            let reparsed = parse_style_transform(&css)
                .unwrap_or_else(|e| panic!("{css:?} did not re-parse: {e}"));
            assert_eq!(reparsed, t, "round-trip failed for {css:?}");
        }
    }

    #[test]
    fn transform_vec_print_parse_round_trip() {
        let v: StyleTransformVec = all_roundtrippable_transforms().into();
        let css = v.print_as_css_value();
        let reparsed = parse_style_transform_vec(&css)
            .unwrap_or_else(|e| panic!("{css:?} did not re-parse: {e}"));
        assert_eq!(reparsed.len(), v.len());
        assert_eq!(reparsed.as_slice(), v.as_slice());
    }

    #[test]
    fn scale_axis_print_does_not_round_trip() {
        // BUG: `StyleTransform::ScaleX/Y/Z` hold a `PercentageValue`, whose Display
        // appends a '%' ("scaleX(120%)"), but the parser reads the argument with a
        // bare `parse::<f32>()` and multiplies by 100. So print -> parse fails for
        // every ScaleX/ScaleY/ScaleZ, and the printed CSS is invalid per spec
        // (`scaleX()` takes a <number>, not a <percentage>).
        // Pinned as current behaviour; the fix is to print the raw number.
        for t in [
            StyleTransform::ScaleX(PercentageValue::new(120.0)),
            StyleTransform::ScaleY(PercentageValue::new(120.0)),
            StyleTransform::ScaleZ(PercentageValue::new(120.0)),
        ] {
            let css = t.print_as_css_value();
            assert!(css.contains('%'), "{css:?}");
            assert!(
                parse_style_transform(&css).is_err(),
                "{css:?} unexpectedly re-parsed - the ScaleX round-trip bug may be fixed"
            );
        }
        // The parser's own accepted form (a bare number) does work.
        assert_eq!(
            parse_style_transform("scaleX(1.2)").unwrap(),
            StyleTransform::ScaleX(PercentageValue::new(120.0))
        );
    }

    // =====================================================================
    // StyleTransformOrigin::interpolate / StylePerspectiveOrigin::interpolate
    // =====================================================================

    #[test]
    fn transform_origin_interpolate_endpoints_are_exact() {
        let a = StyleTransformOrigin {
            x: PixelValue::px(10.0),
            y: PixelValue::percent(0.0),
        };
        let b = StyleTransformOrigin {
            x: PixelValue::px(30.0),
            y: PixelValue::percent(100.0),
        };
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(
            a.interpolate(&b, 0.5),
            StyleTransformOrigin {
                x: PixelValue::px(20.0),
                y: PixelValue::percent(50.0),
            }
        );
        // Interpolating a value with itself is the identity for every finite t.
        for t in [-1.0, 0.0, 0.25, 1.0, 2.0, 1e30] {
            assert_eq!(a.interpolate(&a, t), a, "t = {t}");
        }
    }

    #[test]
    fn transform_origin_interpolate_extrapolates_outside_zero_one() {
        let a = StyleTransformOrigin {
            x: PixelValue::px(10.0),
            y: PixelValue::px(10.0),
        };
        let b = StyleTransformOrigin {
            x: PixelValue::px(30.0),
            y: PixelValue::px(30.0),
        };
        // t is NOT clamped.
        assert_eq!(a.interpolate(&b, -1.0).x, PixelValue::px(-10.0));
        assert_eq!(a.interpolate(&b, 2.0).x, PixelValue::px(50.0));
    }

    #[test]
    fn transform_origin_interpolate_with_nan_or_infinite_t_stays_defined() {
        let a = StyleTransformOrigin {
            x: PixelValue::px(10.0),
            y: PixelValue::percent(10.0),
        };
        let b = StyleTransformOrigin {
            x: PixelValue::px(30.0),
            y: PixelValue::percent(30.0),
        };

        // NaN t -> NaN value -> the f32->isize cast maps NaN to 0.
        let nan = a.interpolate(&b, f32::NAN);
        assert_eq!(nan.x.number.number(), 0);
        assert_eq!(nan.y.number.number(), 0);
        assert_eq!(nan.x.metric, SizeMetric::Px);
        assert_eq!(nan.y.metric, SizeMetric::Percent);
        assert_encodable(nan.x);
        assert_encodable(nan.y);

        // +inf t on an increasing range saturates to isize::MAX, -inf to isize::MIN.
        let pos = a.interpolate(&b, f32::INFINITY);
        assert_eq!(pos.x.number.number(), isize::MAX);
        assert_encodable(pos.x);
        let neg = a.interpolate(&b, f32::NEG_INFINITY);
        assert_eq!(neg.x.number.number(), isize::MIN);
        assert_encodable(neg.x);

        // inf * 0 (identical endpoints) is NaN, which collapses to 0.
        let degenerate = a.interpolate(&a, f32::INFINITY);
        assert_eq!(degenerate.x.number.number(), 0);
        assert_encodable(degenerate.x);
    }

    #[test]
    fn transform_origin_interpolate_between_saturated_extremes_does_not_panic() {
        let a = StyleTransformOrigin {
            x: PixelValue::px(f32::MAX),
            y: PixelValue::px(f32::MIN),
        };
        let b = StyleTransformOrigin {
            x: PixelValue::px(f32::MIN),
            y: PixelValue::px(f32::MAX),
        };
        // Both endpoints are already clamped to isize::MAX / isize::MIN.
        assert_eq!(a.x.number.number(), isize::MAX);
        assert_eq!(a.y.number.number(), isize::MIN);

        for t in [
            -1e30,
            -1.0,
            0.0,
            0.5,
            1.0,
            1e30,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let out = a.interpolate(&b, t);
            assert_encodable(out.x);
            assert_encodable(out.y);
        }
    }

    #[test]
    fn transform_origin_interpolate_across_metrics_falls_back_to_px() {
        let a = StyleTransformOrigin {
            x: PixelValue::px(0.0),
            y: PixelValue::px(0.0),
        };
        let b = StyleTransformOrigin {
            x: PixelValue::percent(100.0),
            y: PixelValue::em(2.0),
        };
        for t in [0.0, 0.5, 1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let out = a.interpolate(&b, t);
            assert_eq!(out.x.metric, SizeMetric::Px, "t = {t}");
            assert_eq!(out.y.metric, SizeMetric::Px, "t = {t}");
            assert_encodable(out.x);
            assert_encodable(out.y);
        }
    }

    #[test]
    fn perspective_origin_interpolate_matches_transform_origin_semantics() {
        let a = StylePerspectiveOrigin {
            x: PixelValue::px(10.0),
            y: PixelValue::percent(0.0),
        };
        let b = StylePerspectiveOrigin {
            x: PixelValue::px(30.0),
            y: PixelValue::percent(100.0),
        };
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(
            a.interpolate(&b, 0.5),
            StylePerspectiveOrigin {
                x: PixelValue::px(20.0),
                y: PixelValue::percent(50.0),
            }
        );
        // Default is 0px 0px, and interpolating it with itself is stable.
        let d = StylePerspectiveOrigin::default();
        assert_eq!(d.interpolate(&d, 0.5), d);

        // NaN / inf are defined, not panics.
        let nan = a.interpolate(&b, f32::NAN);
        assert_eq!(nan.x.number.number(), 0);
        assert_encodable(nan.x);
        let inf = a.interpolate(&b, f32::INFINITY);
        assert_eq!(inf.x.number.number(), isize::MAX);
        assert_encodable(inf.x);

        // Saturated extremes.
        let lo = StylePerspectiveOrigin {
            x: PixelValue::px(f32::MIN),
            y: PixelValue::px(f32::MIN),
        };
        let hi = StylePerspectiveOrigin {
            x: PixelValue::px(f32::MAX),
            y: PixelValue::px(f32::MAX),
        };
        for t in [0.0, 0.5, 1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let out = lo.interpolate(&hi, t);
            assert_encodable(out.x);
            assert_encodable(out.y);
        }
    }

    // =====================================================================
    // Error types: to_contained / to_shared round-trips + Display invariants
    // =====================================================================

    fn transform_errors() -> Vec<CssStyleTransformParseError<'static>> {
        vec![
            CssStyleTransformParseError::InvalidTransform("rotate"),
            // Edge: empty payload.
            CssStyleTransformParseError::InvalidTransform(""),
            CssStyleTransformParseError::InvalidTransform("\u{1F600}"),
            CssStyleTransformParseError::InvalidParenthesis(ParenthesisParseError::EmptyInput),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::UnclosedBraces,
            ),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::NoOpeningBraceFound,
            ),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::NoClosingBraceFound,
            ),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::StopWordNotFound("nope"),
            ),
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 5,
                input: "1,2,3,4,5",
            },
            // Edge: extreme counts + empty input.
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: usize::MAX,
                got: usize::MAX,
                input: "",
            },
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 0,
                got: 0,
                input: "",
            },
            CssStyleTransformParseError::NumberParseError("x".parse::<f32>().unwrap_err()),
            CssStyleTransformParseError::NumberParseError("".parse::<f32>().unwrap_err()),
            CssStyleTransformParseError::PixelValueParseError(
                CssPixelValueParseError::EmptyString,
            ),
            CssStyleTransformParseError::PixelValueParseError(
                CssPixelValueParseError::InvalidPixelValue("auto"),
            ),
            CssStyleTransformParseError::AngleValueParseError(
                CssAngleValueParseError::EmptyString,
            ),
            CssStyleTransformParseError::AngleValueParseError(CssAngleValueParseError::InvalidAngle(
                "",
            )),
            CssStyleTransformParseError::PercentageValueParseError(
                PercentageParseError::NoPercentSign,
            ),
            CssStyleTransformParseError::PercentageValueParseError(
                PercentageParseError::InvalidUnit(AzString::from("")),
            ),
        ]
    }

    #[test]
    fn transform_parse_error_round_trips_through_owned() {
        for err in transform_errors() {
            let owned = err.to_contained();
            let shared = owned.to_shared();
            assert_eq!(shared, err, "round-trip failed for {err}");
            // Re-owning the shared copy must be stable.
            assert_eq!(shared.to_contained(), owned);
        }
    }

    #[test]
    fn transform_parse_error_round_trips_errors_from_the_real_parsers() {
        // Errors that actually come out of the parsers (rather than hand-built).
        for input in [
            "",
            "garbage",
            "translatex(1px)",
            "matrix(1,2,3)",
            "rotate(10px)",
            "translateX(auto)",
            "scaleX(abc)",
            "translate3d(1px,2px)",
        ] {
            let err = parse_style_transform(input).unwrap_err();
            assert_eq!(err.to_contained().to_shared(), err, "for {input:?}");
            assert!(!alloc::format!("{err}").is_empty());
            // Debug is implemented as Display (impl_debug_as_display!).
            assert_eq!(alloc::format!("{err:?}"), alloc::format!("{err}"));
        }
    }

    #[test]
    fn transform_origin_parse_error_round_trips_through_owned() {
        let errs = [
            CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input: "",
            },
            CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: usize::MAX,
                got: usize::MAX,
                input: "\u{1F600}",
            },
            CssStyleTransformOriginParseError::PixelValueParseError(
                CssPixelValueParseError::EmptyString,
            ),
            CssStyleTransformOriginParseError::PixelValueParseError(
                CssPixelValueParseError::InvalidPixelValue(""),
            ),
        ];
        for err in errs {
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {err}");
            assert!(!alloc::format!("{err}").is_empty());
        }
        // ...and one straight out of the parser.
        let err = parse_style_transform_origin("top left").unwrap_err();
        assert_eq!(err.to_contained().to_shared(), err);
    }

    #[test]
    fn perspective_origin_parse_error_round_trips_through_owned() {
        let errs = [
            CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input: "",
            },
            CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: 0,
                got: usize::MAX,
                input: "\u{1F600}\u{0301}",
            },
            CssStylePerspectiveOriginParseError::PixelValueParseError(
                CssPixelValueParseError::EmptyString,
            ),
        ];
        for err in errs {
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {err}");
            assert!(!alloc::format!("{err}").is_empty());
        }
        let err = parse_style_perspective_origin("center center").unwrap_err();
        assert_eq!(err.to_contained().to_shared(), err);
    }

    #[test]
    fn backface_visibility_parse_error_round_trips_through_owned() {
        for payload in ["", "none", "\u{1F600}", "  bogus  "] {
            let err = CssBackfaceVisibilityParseError::InvalidValue(payload);
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {payload:?}");
            assert!(alloc::format!("{err}").contains(payload) || payload.is_empty());
        }
        let err = parse_style_backface_visibility("nope").unwrap_err();
        assert_eq!(err.to_contained().to_shared(), err);
    }

    #[test]
    fn owned_errors_borrow_from_themselves_not_from_the_original_input() {
        // to_contained() must deep-copy the &str payload: the owned error has to
        // outlive the input it was parsed from.
        let owned = {
            let input = String::from("translatex(1px)");
            parse_style_transform(&input).unwrap_err().to_contained()
        };
        assert_eq!(
            owned,
            CssStyleTransformParseErrorOwned::InvalidParenthesis(
                ParenthesisParseErrorOwned::StopWordNotFound(AzString::from("translatex"))
            )
        );
        // And the re-shared borrow points at the owned buffer.
        assert!(matches!(
            owned.to_shared(),
            CssStyleTransformParseError::InvalidParenthesis(
                ParenthesisParseError::StopWordNotFound("translatex")
            )
        ));
    }

    #[test]
    fn wrong_number_of_components_preserves_counts_across_the_owned_conversion() {
        let err = CssStyleTransformParseError::WrongNumberOfComponents {
            expected: usize::MAX,
            got: 0,
            input: "\u{1F600}",
        };
        let CssStyleTransformParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
            expected,
            got,
            input,
        }) = err.to_contained()
        else {
            panic!("expected WrongNumberOfComponents");
        };
        assert_eq!(expected, usize::MAX);
        assert_eq!(got, 0);
        assert_eq!(input.as_str(), "\u{1F600}");
    }

    // =====================================================================
    // parse_float_value (re-exported into this module's parse path)
    // =====================================================================

    #[test]
    fn float_value_parse_helper_saturates_and_rejects_garbage() {
        assert_eq!(parse_float_value("1.5").unwrap(), FloatValue::new(1.5));
        assert_eq!(parse_float_value("  -0  ").unwrap(), FloatValue::new(0.0));
        assert_eq!(parse_float_value("inf").unwrap().number(), isize::MAX);
        assert_eq!(parse_float_value("-inf").unwrap().number(), isize::MIN);
        assert_eq!(parse_float_value("NaN").unwrap().number(), 0);
        assert!(parse_float_value("").is_err());
        assert!(parse_float_value("abc").is_err());
        assert!(parse_float_value("\u{1F600}").is_err());
    }
}
