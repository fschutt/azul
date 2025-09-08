use core::num::ParseFloatError;

use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFilter {
    Blend(StyleMixBlendMode),
    Flood(ColorU),
    Blur(StyleBlur),
    Opacity(PercentageValue),
    ColorMatrix(StyleColorMatrix),
    DropShadow(StyleBoxShadow),
    ComponentTransfer,
    Offset(StyleFilterOffset),
    Composite(StyleCompositeFilter),
}

impl_vec!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_clone!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_debug!(StyleFilter, StyleFilterVec);
impl_vec_eq!(StyleFilter, StyleFilterVec);
impl_vec_ord!(StyleFilter, StyleFilterVec);
impl_vec_hash!(StyleFilter, StyleFilterVec);
impl_vec_partialeq!(StyleFilter, StyleFilterVec);
impl_vec_partialord!(StyleFilter, StyleFilterVec);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBlur {
    pub width: PixelValue,
    pub height: PixelValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleColorMatrix {
    pub matrix: [FloatValue; 20],
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFilterOffset {
    pub x: PixelValue,
    pub y: PixelValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleCompositeFilter {
    Over,
    In,
    Atop,
    Out,
    Xor,
    Lighter,
    Arithmetic([FloatValue; 4]),
}

// parses multiple transform values
pub fn parse_style_filter_vec<'a>(
    input: &'a str,
) -> Result<StyleFilterVec, CssStyleFilterParseError<'a>> {
    let comma_separated_items = split_string_respect_comma(input);
    let vec = split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_filter(i))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(vec.into())
}

pub fn parse_style_filter<'a>(input: &'a str) -> Result<StyleFilter, CssStyleFilterParseError<'a>> {
    use crate::{StyleBlur, StyleColorMatrix, StyleCompositeFilter, StyleFilterOffset};

    let (filter_type, filter_values) = parse_parentheses(
        input,
        &[
            "blend",
            "flood",
            "blur",
            "opacity",
            "color-matrix",
            "drop-shadow",
            "component-transfer",
            "offset",
            "composite",
        ],
    )?;

    fn parse_style_blur<'a>(input: &'a str) -> Result<StyleBlur, CssStyleBlurParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let width = parse_pixel_value(iter.next().ok_or(
            CssStyleBlurParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let height = parse_pixel_value(iter.next().ok_or(
            CssStyleBlurParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleBlur { width, height })
    }

    fn parse_color_matrix<'a>(
        input: &'a str,
    ) -> Result<StyleColorMatrix, CssStyleColorMatrixParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");
        let mut array = [FloatValue::const_new(0); 20];

        for (val_idx, val) in array.iter_mut().enumerate() {
            *val = parse_float_value(iter.next().ok_or(
                CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: val_idx,
                    input,
                },
            )?)?;
        }

        Ok(StyleColorMatrix { matrix: array })
    }

    fn parse_filter_offset<'a>(
        input: &'a str,
    ) -> Result<StyleFilterOffset, CssStyleFilterOffsetParseError<'a>> {
        let input = input.trim();
        let mut iter = input.split(",");

        let x = parse_pixel_value(iter.next().ok_or(
            CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                expected: 2,
                got: 0,
                input,
            },
        )?)?;
        let y = parse_pixel_value(iter.next().ok_or(
            CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                expected: 2,
                got: 1,
                input,
            },
        )?)?;

        Ok(StyleFilterOffset { x, y })
    }

    fn parse_filter_composite<'a>(
        input: &'a str,
    ) -> Result<StyleCompositeFilter, CssStyleCompositeFilterParseError<'a>> {
        fn parse_arithmetic_composite_filter<'a>(
            input: &'a str,
        ) -> Result<[FloatValue; 4], CssStyleCompositeFilterParseError<'a>> {
            let input = input.trim();
            let mut iter = input.split(",");
            let mut array = [FloatValue::const_new(0); 4];

            for (val_idx, val) in array.iter_mut().enumerate() {
                *val = parse_float_value(iter.next().ok_or(
                    CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                        expected: 4,
                        got: val_idx,
                        input,
                    },
                )?)?;
            }

            Ok(array)
        }

        let (filter_composite_type, filter_composite_values) = parse_parentheses(
            input,
            &["over", "in", "atop", "out", "xor", "lighter", "arithmetic"],
        )?;

        match filter_composite_type {
            "over" => Ok(StyleCompositeFilter::Over),
            "in" => Ok(StyleCompositeFilter::In),
            "atop" => Ok(StyleCompositeFilter::Atop),
            "out" => Ok(StyleCompositeFilter::Out),
            "xor" => Ok(StyleCompositeFilter::Xor),
            "lighter" => Ok(StyleCompositeFilter::Lighter),
            "arithmetic" => Ok(StyleCompositeFilter::Arithmetic(
                parse_arithmetic_composite_filter(filter_composite_values)?,
            )),
            _ => unreachable!(),
        }
    }

    match filter_type {
        "blend" => Ok(StyleFilter::Blend(parse_style_mix_blend_mode(
            filter_values,
        )?)),
        "flood" => Ok(StyleFilter::Flood(parse_css_color(filter_values)?)),
        "blur" => Ok(StyleFilter::Blur(parse_style_blur(filter_values)?)),
        "opacity" => Ok(StyleFilter::Opacity(parse_percentage_value(filter_values)?)),
        "color-matrix" => Ok(StyleFilter::ColorMatrix(parse_color_matrix(filter_values)?)),
        "drop-shadow" => Ok(StyleFilter::DropShadow(parse_style_box_shadow(
            filter_values,
        )?)),
        "component-transfer" => Ok(StyleFilter::ComponentTransfer),
        "offset" => Ok(StyleFilter::Offset(parse_filter_offset(filter_values)?)),
        "composite" => Ok(StyleFilter::Composite(parse_filter_composite(
            filter_values,
        )?)),
        _ => unreachable!(),
    }
}
