use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

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
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
    pub angle: AngleValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale3D {
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformSkew2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

// parses multiple transform values
pub fn parse_style_transform_vec<'a>(
    input: &'a str,
) -> Result<StyleTransformVec, CssStyleTransformParseError<'a>> {
    let comma_separated_items = split_string_respect_comma(input);
    let vec = split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_transform(i))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(vec.into())
}

pub fn parse_style_transform<'a>(
    input: &'a str,
) -> Result<StyleTransform, CssStyleTransformParseError<'a>> {
    use crate::{
        StyleTransformMatrix2D, StyleTransformMatrix3D, StyleTransformRotate3D,
        StyleTransformScale2D, StyleTransformScale3D, StyleTransformSkew2D,
        StyleTransformTranslate2D, StyleTransformTranslate3D,
    };

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

    fn parse_matrix<'a>(
        input: &'a str,
    ) -> Result<StyleTransformMatrix2D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let a = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 0,
                input,
            },
        )?)?;
        let b = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 1,
                input,
            },
        )?)?;
        let c = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 2,
                input,
            },
        )?)?;
        let d = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 3,
                input,
            },
        )?)?;
        let tx = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 4,
                input,
            },
        )?)?;
        let ty = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 6,
                got: 5,
                input,
            },
        )?)?;

        Ok(StyleTransformMatrix2D { a, b, c, d, tx, ty })
    }

    fn parse_matrix_3d<'a>(
        input: &'a str,
    ) -> Result<StyleTransformMatrix3D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        // I realize I could use a loop here, but that makes passing the variables to the
        // StyleTransformMatrix3D simpler
        let m11 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 0,
                input,
            },
        )?)?;
        let m12 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 1,
                input,
            },
        )?)?;
        let m13 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 2,
                input,
            },
        )?)?;
        let m14 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 3,
                input,
            },
        )?)?;
        let m21 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 4,
                input,
            },
        )?)?;
        let m22 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 5,
                input,
            },
        )?)?;
        let m23 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 6,
                input,
            },
        )?)?;
        let m24 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 7,
                input,
            },
        )?)?;
        let m31 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 8,
                input,
            },
        )?)?;
        let m32 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 9,
                input,
            },
        )?)?;
        let m33 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 10,
                input,
            },
        )?)?;
        let m34 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 11,
                input,
            },
        )?)?;
        let m41 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 12,
                input,
            },
        )?)?;
        let m42 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 13,
                input,
            },
        )?)?;
        let m43 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 14,
                input,
            },
        )?)?;
        let m44 = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 16,
                got: 15,
                input,
            },
        )?)?;

        Ok(StyleTransformMatrix3D {
            m11,
            m12,
            m13,
            m14,
            m21,
            m22,
            m23,
            m24,
            m31,
            m32,
            m33,
            m34,
            m41,
            m42,
            m43,
            m44,
        })
    }

    fn parse_translate<'a>(
        input: &'a str,
    ) -> Result<StyleTransformTranslate2D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleTransformTranslate2D { x, y })
    }

    fn parse_translate_3d<'a>(
        input: &'a str,
    ) -> Result<StyleTransformTranslate3D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 3,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 3,
                got: 1,
                input,
            },
        )?)?;
        let z = parse_pixel_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 3,
                got: 2,
                input,
            },
        )?)?;

        Ok(StyleTransformTranslate3D { x, y, z })
    }

    fn parse_rotate_3d<'a>(
        input: &'a str,
    ) -> Result<StyleTransformRotate3D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 4,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 4,
                got: 1,
                input,
            },
        )?)?;
        let z = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 4,
                got: 2,
                input,
            },
        )?)?;
        let angle = parse_angle_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 4,
                got: 3,
                input,
            },
        )?)?;

        Ok(StyleTransformRotate3D { x, y, z, angle })
    }

    fn parse_scale<'a>(
        input: &'a str,
    ) -> Result<StyleTransformScale2D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleTransformScale2D { x, y })
    }

    fn parse_scale_3d<'a>(
        input: &'a str,
    ) -> Result<StyleTransformScale3D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;
        let z = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleTransformScale3D { x, y, z })
    }

    fn parse_skew<'a>(
        input: &'a str,
    ) -> Result<StyleTransformSkew2D, CssStyleTransformParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_percentage_value(iter.next().ok_or(
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleTransformSkew2D { x, y })
    }

    match transform_type {
        "matrix" => Ok(StyleTransform::Matrix(parse_matrix(transform_values)?)),
        "matrix3d" => Ok(StyleTransform::Matrix3D(parse_matrix_3d(transform_values)?)),
        "translate" => Ok(StyleTransform::Translate(parse_translate(
            transform_values,
        )?)),
        "translate3d" => Ok(StyleTransform::Translate3D(parse_translate_3d(
            transform_values,
        )?)),
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
        "rotate3d" => Ok(StyleTransform::Rotate3D(parse_rotate_3d(transform_values)?)),
        "rotateX" => Ok(StyleTransform::RotateX(parse_angle_value(
            transform_values,
        )?)),
        "rotateY" => Ok(StyleTransform::RotateY(parse_angle_value(
            transform_values,
        )?)),
        "rotateZ" => Ok(StyleTransform::RotateZ(parse_angle_value(
            transform_values,
        )?)),
        "scale" => Ok(StyleTransform::Scale(parse_scale(transform_values)?)),
        "scale3d" => Ok(StyleTransform::Scale3D(parse_scale_3d(transform_values)?)),
        "scaleX" => Ok(StyleTransform::ScaleX(parse_percentage_value(
            transform_values,
        )?)),
        "scaleY" => Ok(StyleTransform::ScaleY(parse_percentage_value(
            transform_values,
        )?)),
        "scaleZ" => Ok(StyleTransform::ScaleZ(parse_percentage_value(
            transform_values,
        )?)),
        "skew" => Ok(StyleTransform::Skew(parse_skew(transform_values)?)),
        "skewX" => Ok(StyleTransform::SkewX(parse_percentage_value(
            transform_values,
        )?)),
        "skewY" => Ok(StyleTransform::SkewY(parse_percentage_value(
            transform_values,
        )?)),
        "perspective" => Ok(StyleTransform::Perspective(parse_pixel_value(
            transform_values,
        )?)),
        _ => unreachable!(),
    }
}
