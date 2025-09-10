//! Transform-related CSS properties

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash, impl_vec_ord,
    impl_vec_partialeq, impl_vec_partialord,
    props::{
        basic::{
            angle::AngleValue,
            value::{PercentageValue, PixelValue},
        },
        formatter::FormatAsCssValue,
    },
};

/// CSS transform property
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
    SkewX(PercentageValue),
    SkewY(PercentageValue),
    Perspective(PixelValue),
}

impl_vec!(
    StyleTransform,
    StyleTransformVec,
    StyleTransformVecDestructor
);
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

impl Default for StyleTransform {
    fn default() -> Self {
        StyleTransform::Scale(StyleTransformScale2D {
            x: PercentageValue::const_new(100),
            y: PercentageValue::const_new(100),
        })
    }
}

impl fmt::Display for StyleTransform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleTransform::Matrix(m) => write!(
                f,
                "matrix({}, {}, {}, {}, {}, {})",
                m.a, m.b, m.c, m.d, m.tx, m.ty
            ),
            StyleTransform::Matrix3D(m) => write!(
                f,
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
            StyleTransform::Translate(t) => write!(f, "translate({}, {})", t.x, t.y),
            StyleTransform::Translate3D(t) => write!(f, "translate3d({}, {}, {})", t.x, t.y, t.z),
            StyleTransform::TranslateX(x) => write!(f, "translateX({})", x),
            StyleTransform::TranslateY(y) => write!(f, "translateY({})", y),
            StyleTransform::TranslateZ(z) => write!(f, "translateZ({})", z),
            StyleTransform::Rotate(angle) => write!(f, "rotate({})", angle),
            StyleTransform::Rotate3D(r) => {
                write!(f, "rotate3d({}, {}, {}, {})", r.x, r.y, r.z, r.angle)
            }
            StyleTransform::RotateX(angle) => write!(f, "rotateX({})", angle),
            StyleTransform::RotateY(angle) => write!(f, "rotateY({})", angle),
            StyleTransform::RotateZ(angle) => write!(f, "rotateZ({})", angle),
            StyleTransform::Scale(s) => write!(f, "scale({}, {})", s.x, s.y),
            StyleTransform::Scale3D(s) => write!(f, "scale3d({}, {}, {})", s.x, s.y, s.z),
            StyleTransform::ScaleX(x) => write!(f, "scaleX({})", x),
            StyleTransform::ScaleY(y) => write!(f, "scaleY({})", y),
            StyleTransform::ScaleZ(z) => write!(f, "scaleZ({})", z),
            StyleTransform::Skew(s) => write!(f, "skew({}, {})", s.x, s.y),
            StyleTransform::SkewX(x) => write!(f, "skewX({})", x),
            StyleTransform::SkewY(y) => write!(f, "skewY({})", y),
            StyleTransform::Perspective(p) => write!(f, "perspective({})", p),
        }
    }
}

impl FormatAsCssValue for StyleTransform {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// CSS transform-origin property
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

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x.scale_for_dpi(scale_factor);
        self.y.scale_for_dpi(scale_factor);
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

impl FormatAsCssValue for StyleTransformOrigin {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {}",
            self.x.format_as_css_value(),
            self.y.format_as_css_value()
        )
    }
}

/// CSS perspective-origin property
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

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x.scale_for_dpi(scale_factor);
        self.y.scale_for_dpi(scale_factor);
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

impl FormatAsCssValue for StylePerspectiveOrigin {
    fn format_as_css_value(&self) -> String {
        format!(
            "{} {}",
            self.x.format_as_css_value(),
            self.y.format_as_css_value()
        )
    }
}

/// 2D transformation matrix
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix2D {
    pub a: PixelValue,
    pub b: PixelValue,
    pub c: PixelValue,
    pub d: PixelValue,
    pub tx: PixelValue,
    pub ty: PixelValue,
}

impl Default for StyleTransformMatrix2D {
    fn default() -> Self {
        Self {
            a: PixelValue::const_px(1),
            b: PixelValue::zero(),
            c: PixelValue::zero(),
            d: PixelValue::const_px(1),
            tx: PixelValue::zero(),
            ty: PixelValue::zero(),
        }
    }
}

/// 3D transformation matrix
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix3D {
    pub m11: PixelValue,
    pub m12: PixelValue,
    pub m13: PixelValue,
    pub m14: PixelValue,
    pub m21: PixelValue,
    pub m22: PixelValue,
    pub m23: PixelValue,
    pub m24: PixelValue,
    pub m31: PixelValue,
    pub m32: PixelValue,
    pub m33: PixelValue,
    pub m34: PixelValue,
    pub m41: PixelValue,
    pub m42: PixelValue,
    pub m43: PixelValue,
    pub m44: PixelValue,
}

impl Default for StyleTransformMatrix3D {
    fn default() -> Self {
        Self {
            m11: PixelValue::const_px(1),
            m12: PixelValue::zero(),
            m13: PixelValue::zero(),
            m14: PixelValue::zero(),
            m21: PixelValue::zero(),
            m22: PixelValue::const_px(1),
            m23: PixelValue::zero(),
            m24: PixelValue::zero(),
            m31: PixelValue::zero(),
            m32: PixelValue::zero(),
            m33: PixelValue::const_px(1),
            m34: PixelValue::zero(),
            m41: PixelValue::zero(),
            m42: PixelValue::zero(),
            m43: PixelValue::zero(),
            m44: PixelValue::const_px(1),
        }
    }
}

/// 2D translate transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate2D {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl Default for StyleTransformTranslate2D {
    fn default() -> Self {
        Self {
            x: PixelValue::zero(),
            y: PixelValue::zero(),
        }
    }
}

/// 3D translate transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate3D {
    pub x: PixelValue,
    pub y: PixelValue,
    pub z: PixelValue,
}

impl Default for StyleTransformTranslate3D {
    fn default() -> Self {
        Self {
            x: PixelValue::zero(),
            y: PixelValue::zero(),
            z: PixelValue::zero(),
        }
    }
}

/// 3D rotate transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformRotate3D {
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
    pub angle: AngleValue,
}

impl Default for StyleTransformRotate3D {
    fn default() -> Self {
        Self {
            x: PercentageValue::zero(),
            y: PercentageValue::zero(),
            z: PercentageValue::const_new(100),
            angle: AngleValue::default(),
        }
    }
}

/// 2D scale transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

impl Default for StyleTransformScale2D {
    fn default() -> Self {
        Self {
            x: PercentageValue::const_new(100),
            y: PercentageValue::const_new(100),
        }
    }
}

/// 3D scale transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale3D {
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
}

impl Default for StyleTransformScale3D {
    fn default() -> Self {
        Self {
            x: PercentageValue::const_new(100),
            y: PercentageValue::const_new(100),
            z: PercentageValue::const_new(100),
        }
    }
}

/// 2D skew transformation
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformSkew2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

impl Default for StyleTransformSkew2D {
    fn default() -> Self {
        Self {
            x: PercentageValue::zero(),
            y: PercentageValue::zero(),
        }
    }
}

#[cfg(feature = "parser")]
use crate::parser_ext::{
    parse_style_perspective_origin, parse_style_transform, parse_style_transform_origin,
    parse_style_transform_vec,
};
