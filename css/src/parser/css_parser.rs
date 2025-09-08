//! Contains utilities to convert strings (CSS strings) to servo types

use alloc::{string::String, vec::Vec};
use core::{
    fmt,
    num::{ParseFloatError, ParseIntError},
};

use crate::{
    AzString, BackgroundPositionHorizontal, BackgroundPositionVertical,
    BorderStyle, ColorU, ConicGradient, CssPropertyValue, Direction, DirectionCorner, DirectionCorners,
    LayoutAlignContent, LayoutAlignItems, LayoutBorderBottomWidth,
    LayoutBorderLeftWidth, LayoutBorderRightWidth, LayoutBorderTopWidth, LayoutBottom,
    LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
    LayoutFlexWrap, LayoutFloat, LayoutHeight, LayoutJustifyContent, LayoutLeft,
    LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop, LayoutMaxHeight,
    LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPaddingBottom,
    LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutRight,
    LayoutTop, LayoutWidth, LinearGradient, NormalizedLinearColorStop,
    NormalizedRadialColorStop, RadialGradient, ScrollbarStyle,
    Shape, StyleBackfaceVisibility, StyleBackgroundContent, StyleBackgroundContentVec,
    StyleBackgroundPosition, StyleBackgroundPositionVec, StyleBackgroundRepeat,
    StyleBackgroundRepeatVec, StyleBackgroundSize, StyleBackgroundSizeVec, StyleBorderBottomColor,
    StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleBorderBottomStyle,
    StyleBorderLeftColor, StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
    StyleBorderSide, StyleBorderTopColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
    StyleBorderTopStyle, StyleBoxShadow, StyleCursor, StyleDirection, StyleFilter, StyleFilterVec,
    StyleFontFamily, StyleFontFamilyVec, StyleFontSize, StyleHyphens, StyleLetterSpacing,
    StyleLineHeight, StyleMixBlendMode, StyleOpacity, StylePerspectiveOrigin, StyleTabWidth,
    StyleTextAlign, StyleTextColor, StyleTransform, StyleTransformOrigin, StyleTransformVec,
    StyleWhiteSpace, StyleWordSpacing,
};

pub trait FormatAsCssValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

impl FormatAsCssValue for StylePerspectiveOrigin {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}

impl FormatAsCssValue for StyleTransformOrigin {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}

impl FormatAsCssValue for AngleValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl FormatAsCssValue for PixelValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl FormatAsCssValue for StyleTransform {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleTransform::Matrix(m) => write!(
                f,
                "matrix({}, {}, {}, {}, {}, {}",
                m.a, m.b, m.c, m.d, m.tx, m.ty
            ),
            StyleTransform::Matrix3D(m) => write!(
                f,
                "matrix3d({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
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
            StyleTransform::Rotate(r) => write!(f, "rotate({})", r),
            StyleTransform::Rotate3D(r) => {
                write!(f, "rotate3d({}, {}, {}, {})", r.x, r.y, r.z, r.angle)
            }
            StyleTransform::RotateX(x) => write!(f, "rotateX({})", x),
            StyleTransform::RotateY(y) => write!(f, "rotateY({})", y),
            StyleTransform::RotateZ(z) => write!(f, "rotateZ({})", z),
            StyleTransform::Scale(s) => write!(f, "scale({}, {})", s.x, s.y),
            StyleTransform::Scale3D(s) => write!(f, "scale3d({}, {}, {})", s.x, s.y, s.z),
            StyleTransform::ScaleX(x) => write!(f, "scaleX({})", x),
            StyleTransform::ScaleY(y) => write!(f, "scaleY({})", y),
            StyleTransform::ScaleZ(z) => write!(f, "scaleZ({})", z),
            StyleTransform::Skew(sk) => write!(f, "skew({}, {})", sk.x, sk.y),
            StyleTransform::SkewX(x) => write!(f, "skewX({})", x),
            StyleTransform::SkewY(y) => write!(f, "skewY({})", y),
            StyleTransform::Perspective(dist) => write!(f, "perspective({})", dist),
        }
    }
}
