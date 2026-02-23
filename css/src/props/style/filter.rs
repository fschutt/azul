//! CSS properties for graphical effects like blur, drop-shadow, etc.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, num::ParseFloatError};

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::{InvalidValueErr, InvalidValueErrOwned, WrongComponentCountError},
    length::parse_float_value,
    parse::{parse_parentheses, ParenthesisParseError, ParenthesisParseErrorOwned},
};
use crate::{
    format_rust_code::GetHash,
    props::{
        basic::{
            angle::{AngleValue, parse_angle_value, CssAngleValueParseError, CssAngleValueParseErrorOwned},
            color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
            length::{FloatValue, PercentageParseError, PercentageValue},
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
        },
        formatter::PrintAsCssValue,
        style::{
            box_shadow::{
                parse_style_box_shadow, CssShadowParseError, CssShadowParseErrorOwned,
                StyleBoxShadow,
            },
            effects::{parse_style_mix_blend_mode, MixBlendModeParseError, StyleMixBlendMode},
        },
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
    // Standard CSS filter functions
    Brightness(PercentageValue),
    Contrast(PercentageValue),
    Grayscale(PercentageValue),
    HueRotate(AngleValue),
    Invert(PercentageValue),
    Saturate(PercentageValue),
    Sepia(PercentageValue),
}

impl_option!(
    StyleFilter,
    OptionStyleFilter,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor, StyleFilterVecDestructorType, StyleFilterVecSlice, OptionStyleFilter);
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

/// Color matrix with 20 float values for color transformation.
/// Layout: 4 rows × 5 columns (RGBA + offset for each channel)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleColorMatrix {
    pub m0: FloatValue,
    pub m1: FloatValue,
    pub m2: FloatValue,
    pub m3: FloatValue,
    pub m4: FloatValue,
    pub m5: FloatValue,
    pub m6: FloatValue,
    pub m7: FloatValue,
    pub m8: FloatValue,
    pub m9: FloatValue,
    pub m10: FloatValue,
    pub m11: FloatValue,
    pub m12: FloatValue,
    pub m13: FloatValue,
    pub m14: FloatValue,
    pub m15: FloatValue,
    pub m16: FloatValue,
    pub m17: FloatValue,
    pub m18: FloatValue,
    pub m19: FloatValue,
}

impl StyleColorMatrix {
    /// Returns the matrix values as a slice for iteration
    pub fn as_slice(&self) -> [FloatValue; 20] {
        [
            self.m0, self.m1, self.m2, self.m3, self.m4, self.m5, self.m6, self.m7, self.m8,
            self.m9, self.m10, self.m11, self.m12, self.m13, self.m14, self.m15, self.m16,
            self.m17, self.m18, self.m19,
        ]
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFilterOffset {
    pub x: PixelValue,
    pub y: PixelValue,
}

/// Arithmetic coefficients for composite filter (k1, k2, k3, k4).
/// Result = k1*i1*i2 + k2*i1 + k3*i2 + k4
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ArithmeticCoefficients {
    pub k1: FloatValue,
    pub k2: FloatValue,
    pub k3: FloatValue,
    pub k4: FloatValue,
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
    Arithmetic(ArithmeticCoefficients),
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

// Formatting to Rust code for StyleFilterVec
impl crate::format_rust_code::FormatAsRustCode for StyleFilterVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleFilterVec::from_const_slice(STYLE_FILTER_{}_ITEMS)",
            self.get_hash()
        )
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
                c.as_slice()
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
            StyleFilter::Brightness(v) => format!("brightness({})", v),
            StyleFilter::Contrast(v) => format!("contrast({})", v),
            StyleFilter::Grayscale(v) => format!("grayscale({})", v),
            StyleFilter::HueRotate(a) => format!("hue-rotate({})", a),
            StyleFilter::Invert(v) => format!("invert({})", v),
            StyleFilter::Saturate(v) => format!("saturate({})", v),
            StyleFilter::Sepia(v) => format!("sepia({})", v),
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
            StyleCompositeFilter::Arithmetic(fv) => {
                format!("arithmetic {} {} {} {}", fv.k1, fv.k2, fv.k3, fv.k4)
            }
        }
    }
}

// --- PARSER ---

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::props::basic::parse_percentage_value;
    use crate::corety::AzString;

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
        Angle(CssAngleValueParseError<'a>),
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
        Angle(e) => format!("Error parsing hue-rotate(): {}", e),
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
    impl_from!(CssAngleValueParseError<'a>, CssStyleFilterParseError::Angle);

    impl<'a> From<PercentageParseError> for CssStyleFilterParseError<'a> {
        fn from(p: PercentageParseError) -> Self {
            Self::Opacity(p)
        }
    }

    impl<'a> From<MixBlendModeParseError<'a>> for CssStyleFilterParseError<'a> {
        fn from(e: MixBlendModeParseError<'a>) -> Self {
            // Extract the InvalidValueErr from the MixBlendModeParseError
            match e {
                MixBlendModeParseError::InvalidValue(err) => Self::BlendMode(err),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum CssStyleFilterParseErrorOwned {
        InvalidFilter(AzString),
        InvalidParenthesis(ParenthesisParseErrorOwned),
        Shadow(CssShadowParseErrorOwned),
        BlendMode(InvalidValueErrOwned),
        Color(CssColorParseErrorOwned),
        Opacity(PercentageParseError),
        Blur(CssStyleBlurParseErrorOwned),
        ColorMatrix(CssStyleColorMatrixParseErrorOwned),
        Offset(CssStyleFilterOffsetParseErrorOwned),
        Composite(CssStyleCompositeFilterParseErrorOwned),
        Angle(CssAngleValueParseErrorOwned),
    }

    impl<'a> CssStyleFilterParseError<'a> {
        pub fn to_contained(&self) -> CssStyleFilterParseErrorOwned {
            match self {
                Self::InvalidFilter(s) => {
                    CssStyleFilterParseErrorOwned::InvalidFilter(s.to_string().into())
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
                Self::Angle(e) => CssStyleFilterParseErrorOwned::Angle(e.to_contained()),
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
                Self::Angle(e) => CssStyleFilterParseError::Angle(e.to_shared()),
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
    #[repr(C, u8)]
    pub enum CssStyleBlurParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        TooManyComponents(AzString),
    }

    impl<'a> CssStyleBlurParseError<'a> {
        pub fn to_contained(&self) -> CssStyleBlurParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleBlurParseErrorOwned::Pixel(e.to_contained()),
                Self::TooManyComponents(s) => {
                    CssStyleBlurParseErrorOwned::TooManyComponents(s.to_string().into())
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
    #[repr(C, u8)]
    pub enum CssStyleColorMatrixParseErrorOwned {
        Float(crate::props::basic::error::ParseFloatError),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl<'a> CssStyleColorMatrixParseError<'a> {
        pub fn to_contained(&self) -> CssStyleColorMatrixParseErrorOwned {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseErrorOwned::Float(e.clone().into()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleColorMatrixParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                }),
            }
        }
    }

    impl CssStyleColorMatrixParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleColorMatrixParseError<'a> {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseError::Float(e.to_std()),
                Self::WrongNumberOfComponents(e) => CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: &e.input,
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
    #[repr(C, u8)]
    pub enum CssStyleFilterOffsetParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl<'a> CssStyleFilterOffsetParseError<'a> {
        pub fn to_contained(&self) -> CssStyleFilterOffsetParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseErrorOwned::Pixel(e.to_contained()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleFilterOffsetParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                }),
            }
        }
    }

    impl CssStyleFilterOffsetParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleFilterOffsetParseError<'a> {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseError::Pixel(e.to_shared()),
                Self::WrongNumberOfComponents(e) => CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: &e.input,
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
    #[repr(C, u8)]
    pub enum CssStyleCompositeFilterParseErrorOwned {
        Invalid(InvalidValueErrOwned),
        Float(crate::props::basic::error::ParseFloatError),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl<'a> CssStyleCompositeFilterParseError<'a> {
        pub fn to_contained(&self) -> CssStyleCompositeFilterParseErrorOwned {
            match self {
                Self::Invalid(e) => {
                    CssStyleCompositeFilterParseErrorOwned::Invalid(e.to_contained())
                }
                Self::Float(e) => CssStyleCompositeFilterParseErrorOwned::Float(e.clone().into()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleCompositeFilterParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                    expected: *expected,
                    got: *got,
                    input: input.to_string(),
                }),
            }
        }
    }

    impl CssStyleCompositeFilterParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleCompositeFilterParseError<'a> {
            match self {
                Self::Invalid(e) => CssStyleCompositeFilterParseError::Invalid(e.to_shared()),
                Self::Float(e) => CssStyleCompositeFilterParseError::Float(e.to_std()),
                Self::WrongNumberOfComponents(e) => CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: &e.input,
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
                "brightness",
                "contrast",
                "grayscale",
                "hue-rotate",
                "invert",
                "saturate",
                "sepia",
            ],
        )?;

        match filter_type {
            "blend" => Ok(StyleFilter::Blend(parse_style_mix_blend_mode(
                filter_values,
            )?)),
            "flood" => Ok(StyleFilter::Flood(parse_css_color(filter_values)?)),
            "blur" => Ok(StyleFilter::Blur(parse_style_blur(filter_values)?)),
            "opacity" => {
                let val = parse_percentage_value(filter_values)?;
                // CSS filter opacity must be between 0 and 1 (or 0% to 100%)
                let normalized = val.normalized();
                if normalized < 0.0 || normalized > 1.0 {
                    return Err(CssStyleFilterParseError::Opacity(
                        PercentageParseError::InvalidUnit(filter_values.to_string().into()),
                    ));
                }
                Ok(StyleFilter::Opacity(val))
            }
            "color-matrix" => Ok(StyleFilter::ColorMatrix(parse_color_matrix(filter_values)?)),
            "drop-shadow" => Ok(StyleFilter::DropShadow(parse_style_box_shadow(
                filter_values,
            )?)),
            "component-transfer" => Ok(StyleFilter::ComponentTransfer),
            "offset" => Ok(StyleFilter::Offset(parse_filter_offset(filter_values)?)),
            "composite" => Ok(StyleFilter::Composite(parse_filter_composite(
                filter_values,
            )?)),
            "brightness" => Ok(StyleFilter::Brightness(parse_percentage_value(filter_values)?)),
            "contrast" => Ok(StyleFilter::Contrast(parse_percentage_value(filter_values)?)),
            "grayscale" => Ok(StyleFilter::Grayscale(parse_percentage_value(filter_values)?)),
            "hue-rotate" => Ok(StyleFilter::HueRotate(parse_angle_value(filter_values)?)),
            "invert" => Ok(StyleFilter::Invert(parse_percentage_value(filter_values)?)),
            "saturate" => Ok(StyleFilter::Saturate(parse_percentage_value(filter_values)?)),
            "sepia" => Ok(StyleFilter::Sepia(parse_percentage_value(filter_values)?)),
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

        let mut values = [FloatValue::const_new(0); 20];
        for (i, comp) in components.iter().enumerate() {
            values[i] = parse_float_value(comp)?;
        }

        Ok(StyleColorMatrix {
            m0: values[0],
            m1: values[1],
            m2: values[2],
            m3: values[3],
            m4: values[4],
            m5: values[5],
            m6: values[6],
            m7: values[7],
            m8: values[8],
            m9: values[9],
            m10: values[10],
            m11: values[11],
            m12: values[12],
            m13: values[13],
            m14: values[14],
            m15: values[15],
            m16: values[16],
            m17: values[17],
            m18: values[18],
            m19: values[19],
        })
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
                let mut values = [FloatValue::const_new(0); 4];
                for (i, val) in values.iter_mut().enumerate() {
                    let s = iter.next().ok_or(
                        CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                            expected: 4,
                            got: i,
                            input,
                        },
                    )?;
                    *val = parse_float_value(s)?;
                }
                Ok(StyleCompositeFilter::Arithmetic(ArithmeticCoefficients {
                    k1: values[0],
                    k2: values[1],
                    k3: values[2],
                    k4: values[3],
                }))
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

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::style::filter::parser::parse_style_filter;

    #[test]
    fn test_parse_single_filter_functions() {
        // Blur
        let blur = parse_style_filter("blur(5px)").unwrap();
        assert!(matches!(blur, StyleFilter::Blur(_)));
        if let StyleFilter::Blur(b) = blur {
            assert_eq!(b.width, PixelValue::px(5.0));
            assert_eq!(b.height, PixelValue::px(5.0));
        }

        // Blur with two values
        let blur2 = parse_style_filter("blur(2px 4px)").unwrap();
        if let StyleFilter::Blur(b) = blur2 {
            assert_eq!(b.width, PixelValue::px(2.0));
            assert_eq!(b.height, PixelValue::px(4.0));
        }

        // Drop Shadow
        let shadow = parse_style_filter("drop-shadow(10px 5px 5px #888)").unwrap();
        assert!(matches!(shadow, StyleFilter::DropShadow(_)));
        if let StyleFilter::DropShadow(s) = shadow {
            assert_eq!(s.offset_x.inner, PixelValue::px(10.0));
            assert_eq!(s.blur_radius.inner, PixelValue::px(5.0));
            assert_eq!(s.color, ColorU::new_rgb(0x88, 0x88, 0x88));
        }

        // Opacity
        let opacity = parse_style_filter("opacity(50%)").unwrap();
        assert!(matches!(opacity, StyleFilter::Opacity(_)));
        if let StyleFilter::Opacity(p) = opacity {
            assert_eq!(p.normalized(), 0.5);
        }

        // Flood
        let flood = parse_style_filter("flood(red)").unwrap();
        assert_eq!(flood, StyleFilter::Flood(ColorU::RED));

        // Composite
        let composite = parse_style_filter("composite(in)").unwrap();
        assert_eq!(composite, StyleFilter::Composite(StyleCompositeFilter::In));

        // Offset
        let offset = parse_style_filter("offset(10px 20%)").unwrap();
        if let StyleFilter::Offset(o) = offset {
            assert_eq!(o.x, PixelValue::px(10.0));
            assert_eq!(o.y, PixelValue::percent(20.0));
        }
    }

    #[test]
    fn test_parse_filter_vec() {
        let filters =
            parse_style_filter_vec("blur(5px) drop-shadow(10px 5px #888) opacity(0.8)").unwrap();
        assert_eq!(filters.len(), 3);
        assert!(matches!(filters.as_slice()[0], StyleFilter::Blur(_)));
        assert!(matches!(filters.as_slice()[1], StyleFilter::DropShadow(_)));
        assert!(matches!(filters.as_slice()[2], StyleFilter::Opacity(_)));
    }

    #[test]
    fn test_parse_filter_errors() {
        // Invalid function name
        assert!(parse_style_filter_vec("blurry(5px)").is_err());
        // Incorrect arguments
        assert!(parse_style_filter_vec("blur(5px 10px 15px)").is_err());
        assert!(parse_style_filter_vec("opacity(2)").is_err()); // opacity must be % or 0-1
                                                                // Unclosed parenthesis
        assert!(parse_style_filter_vec("blur(5px").is_err());
    }
}
