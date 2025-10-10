//! CSS properties for graphical effects like blur, drop-shadow, etc.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, num::ParseFloatError};

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::{InvalidValueErr, InvalidValueErrOwned},
    length::parse_float_value,
    parse::{parse_parentheses, ParenthesisParseError, ParenthesisParseErrorOwned},
};
use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        length::{FloatValue, PercentageParseError, PercentageValue},
        pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
    },
    formatter::PrintAsCssValue,
    style::{
        box_shadow::{
            parse_style_box_shadow, CssShadowParseError, CssShadowParseErrorOwned, StyleBoxShadow,
        },
        effects::{parse_style_mix_blend_mode, MixBlendModeParseError, StyleMixBlendMode},
    },
};

// --- TYPE DEFINITIONS ---

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

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

// --- PRINTING IMPLEMENTATIONS ---

impl PrintAsCssValue for StyleFilterVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl PrintAsCssValue for StyleFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleFilter::Blend(mode) => format!("blend({})", mode.print_as_css_value()),
            StyleFilter::Flood(c) => format!("flood({})", c.to_hash()),
            StyleFilter::Blur(c) => {
                if c.width == c.height {
                    format!("blur({})", c.width)
                } else {
                    format!("blur({} {})", c.width, c.height)
                }
            }
            StyleFilter::Opacity(c) => format!("opacity({})", c),
            StyleFilter::ColorMatrix(c) => format!(
                "color-matrix({})",
                c.matrix
                    .iter()
                    .map(|s| format!("{}", s))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            StyleFilter::DropShadow(shadow) => {
                format!("drop-shadow({})", shadow.print_as_css_value())
            }
            StyleFilter::ComponentTransfer => "component-transfer".to_string(),
            StyleFilter::Offset(o) => format!("offset({} {})", o.x, o.y),
            StyleFilter::Composite(c) => format!("composite({})", c.print_as_css_value()),
        }
    }
}

impl PrintAsCssValue for StyleCompositeFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleCompositeFilter::Over => "over".to_string(),
            StyleCompositeFilter::In => "in".to_string(),
            StyleCompositeFilter::Atop => "atop".to_string(),
            StyleCompositeFilter::Out => "out".to_string(),
            StyleCompositeFilter::Xor => "xor".to_string(),
            StyleCompositeFilter::Lighter => "lighter".to_string(),
            StyleCompositeFilter::Arithmetic(fv) => format!(
                "arithmetic {}",
                fv.iter()
                    .map(|s| format!("{}", s))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }
}

// --- PARSER ---

#[cfg(feature = "parser")]
mod parser {
    use super::*;
    use crate::props::basic::parse_percentage_value;

    // -- Top-level Filter Error --

    #[derive(Clone, PartialEq)]
    pub enum CssStyleFilterParseError<'a> {
        InvalidFilter(&'a str),
        InvalidParenthesis(ParenthesisParseError<'a>),
        Shadow(CssShadowParseError<'a>),
        BlendMode(InvalidValueErr<'a>),
        Color(CssColorParseError<'a>),
        Opacity(PercentageParseError),
        Blur(CssStyleBlurParseError<'a>),
        ColorMatrix(CssStyleColorMatrixParseError<'a>),
        Offset(CssStyleFilterOffsetParseError<'a>),
        Composite(CssStyleCompositeFilterParseError<'a>),
    }

    impl_debug_as_display!(CssStyleFilterParseError<'a>);
    impl_display! { CssStyleFilterParseError<'a>, {
        InvalidFilter(e) => format!("Invalid filter function: \"{}\"", e),
        InvalidParenthesis(e) => format!("Invalid filter syntax - parenthesis error: {}", e),
        Shadow(e) => format!("Error parsing drop-shadow(): {}", e),
        BlendMode(e) => format!("Error parsing blend(): invalid value \"{}\"", e.0),
        Color(e) => format!("Error parsing flood(): {}", e),
        Opacity(e) => format!("Error parsing opacity(): {}", e),
        Blur(e) => format!("Error parsing blur(): {}", e),
        ColorMatrix(e) => format!("Error parsing color-matrix(): {}", e),
        Offset(e) => format!("Error parsing offset(): {}", e),
        Composite(e) => format!("Error parsing composite(): {}", e),
    }}

    impl_from!(
        ParenthesisParseError<'a>,
        CssStyleFilterParseError::InvalidParenthesis
    );
    impl_from!(InvalidValueErr<'a>, CssStyleFilterParseError::BlendMode);
    impl_from!(CssStyleBlurParseError<'a>, CssStyleFilterParseError::Blur);
    impl_from!(CssColorParseError<'a>, CssStyleFilterParseError::Color);
    impl_from!(
        CssStyleColorMatrixParseError<'a>,
        CssStyleFilterParseError::ColorMatrix
    );
    impl_from!(
        CssStyleFilterOffsetParseError<'a>,
        CssStyleFilterParseError::Offset
    );
    impl_from!(
        CssStyleCompositeFilterParseError<'a>,
        CssStyleFilterParseError::Composite
    );
    impl_from!(CssShadowParseError<'a>, CssStyleFilterParseError::Shadow);

    impl<'a> From<PercentageParseError> for CssStyleFilterParseError<'a> {
        fn from(p: PercentageParseError) -> Self {
            Self::Opacity(p)
        }
    }

    impl<'a> From<MixBlendModeParseError<'a>> for CssStyleFilterParseError<'a> {
        fn from(e: MixBlendModeParseError<'a>) -> Self {
            Self::BlendMode(InvalidValueErr(e))
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleFilterParseErrorOwned {
        InvalidFilter(String),
        InvalidParenthesis(ParenthesisParseErrorOwned),
        Shadow(CssShadowParseErrorOwned),
        BlendMode(InvalidValueErrOwned),
        Color(CssColorParseErrorOwned),
        Opacity(PercentageParseError),
        Blur(CssStyleBlurParseErrorOwned),
        ColorMatrix(CssStyleColorMatrixParseErrorOwned),
        Offset(CssStyleFilterOffsetParseErrorOwned),
        Composite(CssStyleCompositeFilterParseErrorOwned),
    }

    impl<'a> CssStyleFilterParseError<'a> {
        pub fn to_contained(&self) -> CssStyleFilterParseErrorOwned {
            match self {
                Self::InvalidFilter(s) => {
                    CssStyleFilterParseErrorOwned::InvalidFilter(s.to_string())
                }
                Self::InvalidParenthesis(e) => {
                    CssStyleFilterParseErrorOwned::InvalidParenthesis(e.to_contained())
                }
                Self::Shadow(e) => CssStyleFilterParseErrorOwned::Shadow(e.to_contained()),
                Self::BlendMode(e) => CssStyleFilterParseErrorOwned::BlendMode(e.to_contained()),
                Self::Color(e) => CssStyleFilterParseErrorOwned::Color(e.to_contained()),
                Self::Opacity(e) => CssStyleFilterParseErrorOwned::Opacity(e.clone()),
                Self::Blur(e) => CssStyleFilterParseErrorOwned::Blur(e.to_contained()),
                Self::ColorMatrix(e) => {
                    CssStyleFilterParseErrorOwned::ColorMatrix(e.to_contained())
                }
                Self::Offset(e) => CssStyleFilterParseErrorOwned::Offset(e.to_contained()),
                Self::Composite(e) => CssStyleFilterParseErrorOwned::Composite(e.to_contained()),
            }
        }
    }

    impl CssStyleFilterParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleFilterParseError<'a> {
            match self {
                Self::InvalidFilter(s) => CssStyleFilterParseError::InvalidFilter(s),
                Self::InvalidParenthesis(e) => {
                    CssStyleFilterParseError::InvalidParenthesis(e.to_shared())
                }
                Self::Shadow(e) => CssStyleFilterParseError::Shadow(e.to_shared()),
                Self::BlendMode(e) => CssStyleFilterParseError::BlendMode(e.to_shared()),
                Self::Color(e) => CssStyleFilterParseError::Color(e.to_shared()),
                Self::Opacity(e) => CssStyleFilterParseError::Opacity(e.clone()),
                Self::Blur(e) => CssStyleFilterParseError::Blur(e.to_shared()),
                Self::ColorMatrix(e) => CssStyleFilterParseError::ColorMatrix(e.to_shared()),
                Self::Offset(e) => CssStyleFilterParseError::Offset(e.to_shared()),
                Self::Composite(e) => CssStyleFilterParseError::Composite(e.to_shared()),
            }
        }
    }

    // -- Sub-Errors for each filter function --

    #[derive(Clone, PartialEq)]
    pub enum CssStyleBlurParseError<'a> {
        Pixel(CssPixelValueParseError<'a>),
        TooManyComponents(&'a str),
    }

    impl_debug_as_display!(CssStyleBlurParseError<'a>);
    impl_display! { CssStyleBlurParseError<'a>, {
        Pixel(e) => format!("Invalid pixel value: {}", e),
        TooManyComponents(input) => format!("Expected 1 or 2 components, got more: \"{}\"", input),
    }}
    impl_from!(CssPixelValueParseError<'a>, CssStyleBlurParseError::Pixel);

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleBlurParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        TooManyComponents(String),
    }

    impl<'a> CssStyleBlurParseError<'a> {
        pub fn to_contained(&self) -> CssStyleBlurParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleBlurParseErrorOwned::Pixel(e.to_contained()),
                Self::TooManyComponents(s) => {
                    CssStyleBlurParseErrorOwned::TooManyComponents(s.to_string())
                }
            }
        }
    }

    impl CssStyleBlurParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleBlurParseError<'a> {
            match self {
                Self::Pixel(e) => CssStyleBlurParseError::Pixel(e.to_shared()),
                Self::TooManyComponents(s) => CssStyleBlurParseError::TooManyComponents(s),
            }
        }
    }

    #[derive(Clone, PartialEq)]
    pub enum CssStyleColorMatrixParseError<'a> {
        Float(ParseFloatError),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: &'a str,
        },
    }

    impl_debug_as_display!(CssStyleColorMatrixParseError<'a>);
    impl_display! { CssStyleColorMatrixParseError<'a>, {
        Float(e) => format!("Error parsing floating-point value: {}", e),
        WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
    }}
    impl<'a> From<ParseFloatError> for CssStyleColorMatrixParseError<'a> {
        fn from(p: ParseFloatError) -> Self {
            Self::Float(p)
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleColorMatrixParseErrorOwned {
        Float(ParseFloatError),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: String,
        },
    }

    impl<'a> CssStyleColorMatrixParseError<'a> {
        pub fn to_contained(&self) -> CssStyleColorMatrixParseErrorOwned {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseErrorOwned::Float(e.clone()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleColorMatrixParseErrorOwned::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                },
            }
        }
    }

    impl CssStyleColorMatrixParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleColorMatrixParseError<'a> {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseError::Float(e.clone()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input,
                },
            }
        }
    }

    #[derive(Clone, PartialEq)]
    pub enum CssStyleFilterOffsetParseError<'a> {
        Pixel(CssPixelValueParseError<'a>),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: &'a str,
        },
    }

    impl_debug_as_display!(CssStyleFilterOffsetParseError<'a>);
    impl_display! { CssStyleFilterOffsetParseError<'a>, {
        Pixel(e) => format!("Invalid pixel value: {}", e),
        WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
    }}
    impl_from!(
        CssPixelValueParseError<'a>,
        CssStyleFilterOffsetParseError::Pixel
    );

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleFilterOffsetParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: String,
        },
    }

    impl<'a> CssStyleFilterOffsetParseError<'a> {
        pub fn to_contained(&self) -> CssStyleFilterOffsetParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseErrorOwned::Pixel(e.to_contained()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleFilterOffsetParseErrorOwned::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                },
            }
        }
    }

    impl CssStyleFilterOffsetParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleFilterOffsetParseError<'a> {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseError::Pixel(e.to_shared()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input,
                },
            }
        }
    }

    #[derive(Clone, PartialEq)]
    pub enum CssStyleCompositeFilterParseError<'a> {
        Invalid(InvalidValueErr<'a>),
        Float(ParseFloatError),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: &'a str,
        },
    }

    impl_debug_as_display!(CssStyleCompositeFilterParseError<'a>);
    impl_display! { CssStyleCompositeFilterParseError<'a>, {
        Invalid(s) => format!("Invalid composite operator: {}", s.0),
        Float(e) => format!("Error parsing floating-point value for arithmetic(): {}", e),
        WrongNumberOfComponents { expected, got, input } => format!("Expected {} components for arithmetic(), got {}: \"{}\"", expected, got, input),
    }}
    impl_from!(
        InvalidValueErr<'a>,
        CssStyleCompositeFilterParseError::Invalid
    );
    impl<'a> From<ParseFloatError> for CssStyleCompositeFilterParseError<'a> {
        fn from(p: ParseFloatError) -> Self {
            Self::Float(p)
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleCompositeFilterParseErrorOwned {
        Invalid(InvalidValueErrOwned),
        Float(ParseFloatError),
        WrongNumberOfComponents {
            expected: usize,
            got: usize,
            input: String,
        },
    }

    impl<'a> CssStyleCompositeFilterParseError<'a> {
        pub fn to_contained(&self) -> CssStyleCompositeFilterParseErrorOwned {
            match self {
                Self::Invalid(e) => {
                    CssStyleCompositeFilterParseErrorOwned::Invalid(e.to_contained())
                }
                Self::Float(e) => CssStyleCompositeFilterParseErrorOwned::Float(e.clone()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleCompositeFilterParseErrorOwned::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                },
            }
        }
    }

    impl CssStyleCompositeFilterParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleCompositeFilterParseError<'a> {
            match self {
                Self::Invalid(e) => CssStyleCompositeFilterParseError::Invalid(e.to_shared()),
                Self::Float(e) => CssStyleCompositeFilterParseError::Float(e.clone()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                    expected: *expected,
                    got: *got,
                    input,
                },
            }
        }
    }

    // -- Parser Implementation --

    /// Parses a space-separated list of filter functions.
    pub fn parse_style_filter_vec<'a>(
        input: &'a str,
    ) -> Result<StyleFilterVec, CssStyleFilterParseError<'a>> {
        let mut filters = Vec::new();
        let mut remaining = input.trim();
        while !remaining.is_empty() {
            let (filter, rest) = parse_one_filter_function(remaining)?;
            filters.push(filter);
            remaining = rest.trim_start();
        }
        Ok(filters.into())
    }

    /// Parses one `function(...)` from the beginning of a string and returns the parsed
    /// filter and the rest of the string.
    fn parse_one_filter_function<'a>(
        input: &'a str,
    ) -> Result<(StyleFilter, &'a str), CssStyleFilterParseError<'a>> {
        let open_paren = input
            .find('(')
            .ok_or(CssStyleFilterParseError::InvalidFilter(input))?;
        let func_name = &input[..open_paren];

        let mut balance = 1;
        let mut close_paren = 0;
        for (i, c) in input.char_indices().skip(open_paren + 1) {
            if c == '(' {
                balance += 1;
            } else if c == ')' {
                balance -= 1;
                if balance == 0 {
                    close_paren = i;
                    break;
                }
            }
        }

        if balance != 0 {
            return Err(ParenthesisParseError::UnclosedBraces.into());
        }

        let full_function = &input[..=close_paren];
        let rest = &input[(close_paren + 1)..];

        let filter = parse_style_filter(full_function)?;
        Ok((filter, rest))
    }

    /// Parses a single filter function string, like `blur(5px)`.
    pub fn parse_style_filter<'a>(
        input: &'a str,
    ) -> Result<StyleFilter, CssStyleFilterParseError<'a>> {
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

    fn parse_style_blur<'a>(input: &'a str) -> Result<StyleBlur, CssStyleBlurParseError<'a>> {
        let mut iter = input.split_whitespace();
        let width_str = iter.next().unwrap_or("");
        let height_str = iter.next();

        if iter.next().is_some() {
            return Err(CssStyleBlurParseError::TooManyComponents(input));
        }

        let width = parse_pixel_value(width_str)?;
        let height = match height_str {
            Some(s) => parse_pixel_value(s)?,
            None => width, // If only one value is given, use it for both
        };

        Ok(StyleBlur { width, height })
    }

    fn parse_color_matrix<'a>(
        input: &'a str,
    ) -> Result<StyleColorMatrix, CssStyleColorMatrixParseError<'a>> {
        let components: Vec<_> = input.split_whitespace().collect();
        if components.len() != 20 {
            return Err(CssStyleColorMatrixParseError::WrongNumberOfComponents {
                expected: 20,
                got: components.len(),
                input,
            });
        }

        let mut matrix = [FloatValue::const_new(0); 20];
        for (i, comp) in components.iter().enumerate() {
            matrix[i] = parse_float_value(comp)?;
        }

        Ok(StyleColorMatrix { matrix })
    }

    fn parse_filter_offset<'a>(
        input: &'a str,
    ) -> Result<StyleFilterOffset, CssStyleFilterOffsetParseError<'a>> {
        let components: Vec<_> = input.split_whitespace().collect();
        if components.len() != 2 {
            return Err(CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                expected: 2,
                got: components.len(),
                input,
            });
        }

        let x = parse_pixel_value(components[0])?;
        let y = parse_pixel_value(components[1])?;

        Ok(StyleFilterOffset { x, y })
    }

    fn parse_filter_composite<'a>(
        input: &'a str,
    ) -> Result<StyleCompositeFilter, CssStyleCompositeFilterParseError<'a>> {
        let mut iter = input.split_whitespace();
        let operator = iter.next().unwrap_or("");

        match operator {
            "over" => Ok(StyleCompositeFilter::Over),
            "in" => Ok(StyleCompositeFilter::In),
            "atop" => Ok(StyleCompositeFilter::Atop),
            "out" => Ok(StyleCompositeFilter::Out),
            "xor" => Ok(StyleCompositeFilter::Xor),
            "lighter" => Ok(StyleCompositeFilter::Lighter),
            "arithmetic" => {
                let mut params = [FloatValue::const_new(0); 4];
                for (i, val) in params.iter_mut().enumerate() {
                    let s = iter.next().ok_or(
                        CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                            expected: 4,
                            got: i,
                            input,
                        },
                    )?;
                    *val = parse_float_value(s)?;
                }
                Ok(StyleCompositeFilter::Arithmetic(params))
            }
            _ => Err(InvalidValueErr(operator).into()),
        }
    }
}
#[cfg(feature = "parser")]
pub use parser::{
    parse_style_filter_vec, CssStyleBlurParseError, CssStyleBlurParseErrorOwned,
    CssStyleColorMatrixParseError, CssStyleColorMatrixParseErrorOwned,
    CssStyleCompositeFilterParseError, CssStyleCompositeFilterParseErrorOwned,
    CssStyleFilterOffsetParseError, CssStyleFilterOffsetParseErrorOwned, CssStyleFilterParseError,
    CssStyleFilterParseErrorOwned,
};
