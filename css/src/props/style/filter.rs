//! CSS properties for graphical effects like blur, drop-shadow, etc.
//!
//! Defines [`StyleFilter`] and [`StyleFilterVec`] for CSS filter functions
//! (blur, opacity, drop-shadow, color-matrix, brightness, contrast, etc.).
//! Filters are applied via the `WebRender` compositor (`compositor2`) or the
//! software CPU renderer (`cpurender`).

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
    codegen::format::GetHash,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBlur {
    pub width: PixelValue,
    pub height: PixelValue,
}

/// Color matrix with 20 float values for color transformation.
/// Layout: 4 rows × 5 columns (RGBA + offset for each channel)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    #[must_use] pub const fn to_array(&self) -> [FloatValue; 20] {
        [
            self.m0, self.m1, self.m2, self.m3, self.m4, self.m5, self.m6, self.m7, self.m8,
            self.m9, self.m10, self.m11, self.m12, self.m13, self.m14, self.m15, self.m16,
            self.m17, self.m18, self.m19,
        ]
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFilterOffset {
    pub x: PixelValue,
    pub y: PixelValue,
}

/// Arithmetic coefficients for composite filter (k1, k2, k3, k4).
/// Result = k1*i1*i2 + k2*i1 + k3*i2 + k4
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ArithmeticCoefficients {
    pub k1: FloatValue,
    pub k2: FloatValue,
    pub k3: FloatValue,
    pub k4: FloatValue,
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// Formatting to Rust code for StyleFilterVec
impl crate::codegen::format::FormatAsRustCode for StyleFilterVec {
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
            Self::Blend(mode) => format!("blend({})", mode.print_as_css_value()),
            Self::Flood(c) => format!("flood({})", c.to_hash()),
            Self::Blur(c) => {
                if c.width == c.height {
                    format!("blur({})", c.width)
                } else {
                    format!("blur({} {})", c.width, c.height)
                }
            }
            Self::Opacity(c) => format!("opacity({c})"),
            Self::ColorMatrix(c) => format!(
                "color-matrix({})",
                c.to_array()
                    .iter()
                    .map(|s| format!("{s}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::DropShadow(shadow) => {
                format!("drop-shadow({})", shadow.print_as_css_value())
            }
            Self::ComponentTransfer => "component-transfer".to_string(),
            Self::Offset(o) => format!("offset({} {})", o.x, o.y),
            Self::Composite(c) => format!("composite({})", c.print_as_css_value()),
            Self::Brightness(v) => format!("brightness({v})"),
            Self::Contrast(v) => format!("contrast({v})"),
            Self::Grayscale(v) => format!("grayscale({v})"),
            Self::HueRotate(a) => format!("hue-rotate({a})"),
            Self::Invert(v) => format!("invert({v})"),
            Self::Saturate(v) => format!("saturate({v})"),
            Self::Sepia(v) => format!("sepia({v})"),
        }
    }
}

impl PrintAsCssValue for StyleCompositeFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Over => "over".to_string(),
            Self::In => "in".to_string(),
            Self::Atop => "atop".to_string(),
            Self::Out => "out".to_string(),
            Self::Xor => "xor".to_string(),
            Self::Lighter => "lighter".to_string(),
            Self::Arithmetic(fv) => {
                format!("arithmetic {} {} {} {}", fv.k1, fv.k2, fv.k3, fv.k4)
            }
        }
    }
}

// --- PARSER ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
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
        Brightness(PercentageParseError),
        Contrast(PercentageParseError),
        Saturate(PercentageParseError),
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
        Brightness(e) => format!("Error parsing brightness(): {}", e),
        Contrast(e) => format!("Error parsing contrast(): {}", e),
        Saturate(e) => format!("Error parsing saturate(): {}", e),
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

    impl From<PercentageParseError> for CssStyleFilterParseError<'_> {
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
        Brightness(PercentageParseError),
        Contrast(PercentageParseError),
        Saturate(PercentageParseError),
        Blur(CssStyleBlurParseErrorOwned),
        ColorMatrix(CssStyleColorMatrixParseErrorOwned),
        Offset(CssStyleFilterOffsetParseErrorOwned),
        Composite(CssStyleCompositeFilterParseErrorOwned),
        Angle(CssAngleValueParseErrorOwned),
    }

    impl CssStyleFilterParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CssStyleFilterParseErrorOwned {
            match self {
                Self::InvalidFilter(s) => {
                    CssStyleFilterParseErrorOwned::InvalidFilter((*s).to_string().into())
                }
                Self::InvalidParenthesis(e) => {
                    CssStyleFilterParseErrorOwned::InvalidParenthesis(e.to_contained())
                }
                Self::Shadow(e) => CssStyleFilterParseErrorOwned::Shadow(e.to_contained()),
                Self::BlendMode(e) => CssStyleFilterParseErrorOwned::BlendMode(e.to_contained()),
                Self::Color(e) => CssStyleFilterParseErrorOwned::Color(e.to_contained()),
                Self::Opacity(e) => CssStyleFilterParseErrorOwned::Opacity(e.clone()),
                Self::Brightness(e) => CssStyleFilterParseErrorOwned::Brightness(e.clone()),
                Self::Contrast(e) => CssStyleFilterParseErrorOwned::Contrast(e.clone()),
                Self::Saturate(e) => CssStyleFilterParseErrorOwned::Saturate(e.clone()),
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
        #[must_use] pub fn to_shared(&self) -> CssStyleFilterParseError<'_> {
            match self {
                Self::InvalidFilter(s) => CssStyleFilterParseError::InvalidFilter(s),
                Self::InvalidParenthesis(e) => {
                    CssStyleFilterParseError::InvalidParenthesis(e.to_shared())
                }
                Self::Shadow(e) => CssStyleFilterParseError::Shadow(e.to_shared()),
                Self::BlendMode(e) => CssStyleFilterParseError::BlendMode(e.to_shared()),
                Self::Color(e) => CssStyleFilterParseError::Color(e.to_shared()),
                Self::Opacity(e) => CssStyleFilterParseError::Opacity(e.clone()),
                Self::Brightness(e) => CssStyleFilterParseError::Brightness(e.clone()),
                Self::Contrast(e) => CssStyleFilterParseError::Contrast(e.clone()),
                Self::Saturate(e) => CssStyleFilterParseError::Saturate(e.clone()),
                Self::Blur(e) => CssStyleFilterParseError::Blur(e.to_shared()),
                Self::ColorMatrix(e) => CssStyleFilterParseError::ColorMatrix(e.to_shared()),
                Self::Offset(e) => CssStyleFilterParseError::Offset(e.to_shared()),
                Self::Composite(e) => CssStyleFilterParseError::Composite(e.to_shared()),
                Self::Angle(e) => CssStyleFilterParseError::Angle(e.to_shared()),
            }
        }
    }

    // -- Sub-Errors for each filter function --

    #[derive(Clone, PartialEq, Eq)]
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum CssStyleBlurParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        TooManyComponents(AzString),
    }

    impl CssStyleBlurParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CssStyleBlurParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleBlurParseErrorOwned::Pixel(e.to_contained()),
                Self::TooManyComponents(s) => {
                    CssStyleBlurParseErrorOwned::TooManyComponents((*s).to_string().into())
                }
            }
        }
    }

    impl CssStyleBlurParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> CssStyleBlurParseError<'_> {
            match self {
                Self::Pixel(e) => CssStyleBlurParseError::Pixel(e.to_shared()),
                Self::TooManyComponents(s) => CssStyleBlurParseError::TooManyComponents(s),
            }
        }
    }

    #[derive(Clone, PartialEq, Eq)]
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
    impl From<ParseFloatError> for CssStyleColorMatrixParseError<'_> {
        fn from(p: ParseFloatError) -> Self {
            Self::Float(p)
        }
    }
    #[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum CssStyleColorMatrixParseErrorOwned {
        Float(crate::props::basic::error::ParseFloatError),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl CssStyleColorMatrixParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CssStyleColorMatrixParseErrorOwned {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseErrorOwned::Float(e.clone().into()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleColorMatrixParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                    expected: *expected,
                    got: *got,
                    input: (*input).to_string().into(),
                }),
            }
        }
    }

    impl CssStyleColorMatrixParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> CssStyleColorMatrixParseError<'_> {
            match self {
                Self::Float(e) => CssStyleColorMatrixParseError::Float(e.to_std()),
                Self::WrongNumberOfComponents(e) => CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: e.input.as_str(),
                },
            }
        }
    }

    #[derive(Clone, PartialEq, Eq)]
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum CssStyleFilterOffsetParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl CssStyleFilterOffsetParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CssStyleFilterOffsetParseErrorOwned {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseErrorOwned::Pixel(e.to_contained()),
                Self::WrongNumberOfComponents {
                    expected,
                    got,
                    input,
                } => CssStyleFilterOffsetParseErrorOwned::WrongNumberOfComponents(WrongComponentCountError {
                    expected: *expected,
                    got: *got,
                    input: (*input).to_string().into(),
                }),
            }
        }
    }

    impl CssStyleFilterOffsetParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> CssStyleFilterOffsetParseError<'_> {
            match self {
                Self::Pixel(e) => CssStyleFilterOffsetParseError::Pixel(e.to_shared()),
                Self::WrongNumberOfComponents(e) => CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: e.input.as_str(),
                },
            }
        }
    }

    #[derive(Clone, PartialEq, Eq)]
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
    impl From<ParseFloatError> for CssStyleCompositeFilterParseError<'_> {
        fn from(p: ParseFloatError) -> Self {
            Self::Float(p)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum CssStyleCompositeFilterParseErrorOwned {
        Invalid(InvalidValueErrOwned),
        Float(crate::props::basic::error::ParseFloatError),
        WrongNumberOfComponents(WrongComponentCountError),
    }

    impl CssStyleCompositeFilterParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> CssStyleCompositeFilterParseErrorOwned {
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
                    input: (*input).to_string().into(),
                }),
            }
        }
    }

    impl CssStyleCompositeFilterParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> CssStyleCompositeFilterParseError<'_> {
            match self {
                Self::Invalid(e) => CssStyleCompositeFilterParseError::Invalid(e.to_shared()),
                Self::Float(e) => CssStyleCompositeFilterParseError::Float(e.to_std()),
                Self::WrongNumberOfComponents(e) => CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                    expected: e.expected,
                    got: e.got,
                    input: e.input.as_str(),
                },
            }
        }
    }

    // -- Parser Implementation --

    /// Parses a space-separated list of filter functions.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `filter-vec` value.
    pub fn parse_style_filter_vec(
        input: &str,
    ) -> Result<StyleFilterVec, CssStyleFilterParseError<'_>> {
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
    fn parse_one_filter_function(
        input: &str,
    ) -> Result<(StyleFilter, &str), CssStyleFilterParseError<'_>> {
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
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `filter` value.
    pub fn parse_style_filter(
        input: &str,
    ) -> Result<StyleFilter, CssStyleFilterParseError<'_>> {
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
                if !(0.0..=1.0).contains(&normalized) {
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
            "brightness" => {
                let val = parse_percentage_value(filter_values)?;
                if val.normalized() < 0.0 {
                    return Err(CssStyleFilterParseError::Brightness(
                        PercentageParseError::InvalidUnit(filter_values.to_string().into()),
                    ));
                }
                Ok(StyleFilter::Brightness(val))
            }
            "contrast" => {
                let val = parse_percentage_value(filter_values)?;
                if val.normalized() < 0.0 {
                    return Err(CssStyleFilterParseError::Contrast(
                        PercentageParseError::InvalidUnit(filter_values.to_string().into()),
                    ));
                }
                Ok(StyleFilter::Contrast(val))
            }
            "grayscale" => Ok(StyleFilter::Grayscale(parse_percentage_value(filter_values)?)),
            "hue-rotate" => Ok(StyleFilter::HueRotate(parse_angle_value(filter_values)?)),
            "invert" => Ok(StyleFilter::Invert(parse_percentage_value(filter_values)?)),
            "saturate" => {
                let val = parse_percentage_value(filter_values)?;
                if val.normalized() < 0.0 {
                    return Err(CssStyleFilterParseError::Saturate(
                        PercentageParseError::InvalidUnit(filter_values.to_string().into()),
                    ));
                }
                Ok(StyleFilter::Saturate(val))
            }
            "sepia" => Ok(StyleFilter::Sepia(parse_percentage_value(filter_values)?)),
            _ => unreachable!(),
        }
    }

    fn parse_style_blur(input: &str) -> Result<StyleBlur, CssStyleBlurParseError<'_>> {
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

    fn parse_color_matrix(
        input: &str,
    ) -> Result<StyleColorMatrix, CssStyleColorMatrixParseError<'_>> {
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

    fn parse_filter_offset(
        input: &str,
    ) -> Result<StyleFilterOffset, CssStyleFilterOffsetParseError<'_>> {
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

    fn parse_filter_composite(
        input: &str,
    ) -> Result<StyleCompositeFilter, CssStyleCompositeFilterParseError<'_>> {
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

    #[cfg(test)]
    #[allow(clippy::too_many_lines)]
    mod autotest_generated {
        // Parsed values are compared against the exact source literals.
        #![allow(clippy::float_cmp)]

        #[allow(clippy::wildcard_imports)]
        use super::*;
        use alloc::{
            string::{String, ToString},
            vec,
            vec::Vec,
        };

        use crate::props::{
            basic::{
                angle::{AngleMetric, AngleValue, CssAngleValueParseError},
                color::{ColorU, CssColorParseError},
                error::InvalidValueErr,
                length::{FloatValue, PercentageParseError, PercentageValue, SizeMetric},
                parse::ParenthesisParseError,
                pixel::{CssPixelValueParseError, PixelValue},
            },
            formatter::PrintAsCssValue,
            style::{
                box_shadow::CssShadowParseError,
                effects::StyleMixBlendMode,
                filter::{
                    ArithmeticCoefficients, StyleBlur, StyleColorMatrix, StyleCompositeFilter,
                    StyleFilter, StyleFilterOffset,
                },
            },
        };

        // ------------------------------------------------------------------
        // helpers
        // ------------------------------------------------------------------

        /// The 20 components of the identity color matrix, as CSS source.
        const IDENTITY_MATRIX_SRC: &str = "1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0";

        fn matrix_of(v: [FloatValue; 20]) -> StyleColorMatrix {
            StyleColorMatrix {
                m0: v[0],
                m1: v[1],
                m2: v[2],
                m3: v[3],
                m4: v[4],
                m5: v[5],
                m6: v[6],
                m7: v[7],
                m8: v[8],
                m9: v[9],
                m10: v[10],
                m11: v[11],
                m12: v[12],
                m13: v[13],
                m14: v[14],
                m15: v[15],
                m16: v[16],
                m17: v[17],
                m18: v[18],
                m19: v[19],
            }
        }

        /// A `core::num::ParseFloatError` (the `Invalid` kind).
        fn float_err() -> core::num::ParseFloatError {
            "x".parse::<f32>().unwrap_err()
        }

        // ==================================================================
        // StyleColorMatrix::to_array  (getter)
        // ==================================================================

        #[test]
        fn color_matrix_to_array_preserves_field_order() {
            let mut fields = [FloatValue::const_new(0); 20];
            for (i, f) in fields.iter_mut().enumerate() {
                *f = FloatValue::const_new(i as isize);
            }
            let arr = matrix_of(fields).to_array();
            assert_eq!(arr.len(), 20);
            for (i, v) in arr.iter().enumerate() {
                assert_eq!(
                    *v,
                    FloatValue::const_new(i as isize),
                    "to_array() scrambled index {i}"
                );
            }
        }

        #[test]
        fn color_matrix_to_array_on_extreme_values_stays_finite() {
            // Every one of these is a value the *encoding* has to defuse: the
            // getter itself must not panic and must not leak NaN/inf back out.
            let extremes = [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                f32::MIN_POSITIVE,
                f32::EPSILON,
                -0.0,
                0.0,
                1e30,
                -1e30,
                1e-30,
                -1e-30,
                1.0,
                -1.0,
                0.5,
                -0.5,
                255.0,
                -255.0,
                12345.678,
            ];
            let mut fields = [FloatValue::const_new(0); 20];
            for (f, e) in fields.iter_mut().zip(extremes) {
                *f = FloatValue::new(e);
            }
            let m = matrix_of(fields);
            for (i, v) in m.to_array().iter().enumerate() {
                assert!(
                    v.get().is_finite(),
                    "m{i} decoded to a non-finite {}",
                    v.get()
                );
            }
            // NaN is encoded as zero, +/-inf saturate to the isize extremes.
            assert_eq!(m.m0.number(), 0, "NaN did not encode to zero");
            assert_eq!(m.m1.number(), isize::MAX);
            assert_eq!(m.m2.number(), isize::MIN);
            // to_array() is a pure copy: it must agree with the fields.
            assert_eq!(m.to_array()[19], m.m19);
        }

        #[test]
        fn color_matrix_to_array_matches_the_parsed_identity_matrix() {
            let m = parse_color_matrix(IDENTITY_MATRIX_SRC).unwrap();
            let arr = m.to_array();
            let one = FloatValue::const_new(1);
            let zero = FloatValue::const_new(0);
            for (i, v) in arr.iter().enumerate() {
                // Diagonal of the 4x5 matrix: indices 0, 6, 12, 18.
                let expected = if i % 6 == 0 && i <= 18 { one } else { zero };
                assert_eq!(*v, expected, "identity matrix wrong at index {i}");
            }
        }

        // ==================================================================
        // parse_style_filter  (parser, public)
        // ==================================================================

        #[test]
        fn parse_style_filter_empty_and_whitespace_only_input() {
            for input in ["", "   ", "\t\n", "\r\n\t "] {
                assert!(
                    matches!(
                        parse_style_filter(input),
                        Err(CssStyleFilterParseError::InvalidParenthesis(
                            ParenthesisParseError::EmptyInput
                        ))
                    ),
                    "{input:?} should be EmptyInput"
                );
            }
        }

        #[test]
        fn parse_style_filter_garbage_returns_err_and_never_panics() {
            for garbage in [
                "!!!",
                ";",
                "\0",
                "()",
                ")(",
                "((((",
                "))))",
                ")",
                "(",
                "blur",
                "blur(",
                "blur)5px(",
                "()()",
                "5px",
                "-",
                "#",
                "blur[5px]",
                "blur{5px}",
                "blur(5px))(",
                ",,,,",
                "\u{FFFD}",
            ] {
                assert!(
                    parse_style_filter(garbage).is_err(),
                    "garbage {garbage:?} was accepted"
                );
            }
        }

        #[test]
        fn parse_style_filter_valid_minimal_positive_control() {
            // One known-good minimal input per stopword. This also proves the
            // `_ => unreachable!()` arm cannot be reached: every stopword
            // parse_parentheses() can hand back is matched.
            assert_eq!(
                parse_style_filter("blend(normal)").unwrap(),
                StyleFilter::Blend(StyleMixBlendMode::Normal)
            );
            assert_eq!(
                parse_style_filter("flood(red)").unwrap(),
                StyleFilter::Flood(ColorU::RED)
            );
            assert_eq!(
                parse_style_filter("blur(5px)").unwrap(),
                StyleFilter::Blur(StyleBlur {
                    width: PixelValue::px(5.0),
                    height: PixelValue::px(5.0),
                })
            );
            assert_eq!(
                parse_style_filter("opacity(50%)").unwrap(),
                StyleFilter::Opacity(PercentageValue::new(50.0))
            );
            assert_eq!(
                parse_style_filter("color-matrix(1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0)")
                    .unwrap(),
                StyleFilter::ColorMatrix(parse_color_matrix(IDENTITY_MATRIX_SRC).unwrap())
            );
            assert!(matches!(
                parse_style_filter("drop-shadow(1px 2px)").unwrap(),
                StyleFilter::DropShadow(_)
            ));
            assert_eq!(
                parse_style_filter("component-transfer()").unwrap(),
                StyleFilter::ComponentTransfer
            );
            assert_eq!(
                parse_style_filter("offset(1px 2px)").unwrap(),
                StyleFilter::Offset(StyleFilterOffset {
                    x: PixelValue::px(1.0),
                    y: PixelValue::px(2.0),
                })
            );
            assert_eq!(
                parse_style_filter("composite(over)").unwrap(),
                StyleFilter::Composite(StyleCompositeFilter::Over)
            );
            assert_eq!(
                parse_style_filter("brightness(100%)").unwrap(),
                StyleFilter::Brightness(PercentageValue::new(100.0))
            );
            assert_eq!(
                parse_style_filter("contrast(100%)").unwrap(),
                StyleFilter::Contrast(PercentageValue::new(100.0))
            );
            assert_eq!(
                parse_style_filter("grayscale(0%)").unwrap(),
                StyleFilter::Grayscale(PercentageValue::new(0.0))
            );
            assert_eq!(
                parse_style_filter("hue-rotate(90deg)").unwrap(),
                StyleFilter::HueRotate(AngleValue::deg(90.0))
            );
            assert_eq!(
                parse_style_filter("invert(0%)").unwrap(),
                StyleFilter::Invert(PercentageValue::new(0.0))
            );
            assert_eq!(
                parse_style_filter("saturate(100%)").unwrap(),
                StyleFilter::Saturate(PercentageValue::new(100.0))
            );
            assert_eq!(
                parse_style_filter("sepia(0%)").unwrap(),
                StyleFilter::Sepia(PercentageValue::new(0.0))
            );
        }

        #[test]
        fn parse_style_filter_function_names_are_case_sensitive_and_space_sensitive() {
            // NOTE (spec deviation): CSS function names are ASCII
            // case-insensitive and `filter: BLUR(5px)` is legal. This parser
            // compares the stopword byte-for-byte, so the uppercase spelling is
            // rejected. Pinned as-is so the behaviour cannot change silently.
            for input in ["BLUR(5px)", "Blur(5px)", "Hue-Rotate(90deg)"] {
                assert!(
                    matches!(
                        parse_style_filter(input),
                        Err(CssStyleFilterParseError::InvalidParenthesis(
                            ParenthesisParseError::StopWordNotFound(_)
                        ))
                    ),
                    "{input:?} unexpectedly parsed"
                );
            }
            // Whitespace between the name and '(' is correctly rejected (CSS
            // forbids it for functional notation).
            assert!(parse_style_filter("blur (5px)").is_err());
        }

        #[test]
        fn parse_style_filter_leading_and_trailing_whitespace_is_trimmed() {
            assert_eq!(
                parse_style_filter("   blur(5px)  \n").unwrap(),
                parse_style_filter("blur(5px)").unwrap()
            );
        }

        #[test]
        fn parse_style_filter_swallows_trailing_junk_but_the_vec_parser_rejects_it() {
            // parse_parentheses() locates the payload with find('(') + rfind(')'),
            // so anything *after* the last ')' is silently discarded here.
            assert_eq!(
                parse_style_filter("blur(5px)garbage").unwrap(),
                parse_style_filter("blur(5px)").unwrap()
            );
            assert_eq!(
                parse_style_filter("blur(5px);\u{1F600}").unwrap(),
                parse_style_filter("blur(5px)").unwrap()
            );
            // The vec parser cuts at the *balanced* ')' and then has to parse the
            // remainder, so the same junk is rejected there. Deterministic, but
            // the two entry points disagree.
            assert!(parse_style_filter_vec("blur(5px)garbage").is_err());
            assert!(parse_style_filter_vec("blur(5px);garbage").is_err());
        }

        #[test]
        fn parse_style_filter_rejects_two_functions_at_once() {
            // rfind(')') would run the payload of the *last* function into the
            // first one; the inner parse must catch that.
            assert!(parse_style_filter("blur(5px) blur(2px)").is_err());
            assert!(parse_style_filter("blur(5px) drop-shadow(1px 1px)").is_err());
        }

        #[test]
        fn parse_style_filter_component_transfer_ignores_its_arguments() {
            // Any payload at all is accepted and dropped on the floor.
            for input in [
                "component-transfer()",
                "component-transfer(anything at all)",
                "component-transfer(\u{1F600})",
                "component-transfer(   )",
            ] {
                assert_eq!(
                    parse_style_filter(input).unwrap(),
                    StyleFilter::ComponentTransfer,
                    "{input:?}"
                );
            }
        }

        #[test]
        fn parse_style_filter_component_transfer_does_not_round_trip() {
            // BUG (pinned): print_as_css_value() emits the bare keyword
            // "component-transfer" (no parens), but the parser requires a '(' —
            // so printing a StyleFilter and re-parsing it loses this variant.
            let printed = StyleFilter::ComponentTransfer.print_as_css_value();
            assert_eq!(printed, "component-transfer");
            assert!(matches!(
                parse_style_filter(&printed),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::NoOpeningBraceFound
                ))
            ));
            assert!(parse_style_filter_vec(&printed).is_err());
        }

        #[test]
        fn parse_style_filter_opacity_range_is_enforced_at_both_ends() {
            // In range.
            for input in ["opacity(0)", "opacity(0%)", "opacity(1)", "opacity(100%)", "opacity(-0%)"] {
                let f = parse_style_filter(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
                let StyleFilter::Opacity(v) = f else {
                    panic!("{input:?} did not parse as Opacity")
                };
                assert!(
                    (0.0..=1.0).contains(&v.normalized()),
                    "{input:?} -> {}",
                    v.normalized()
                );
            }
            // Out of range / not a number at all.
            for input in [
                "opacity(2)",
                "opacity(101%)",
                "opacity(-1%)",
                "opacity(-0.5)",
                "opacity(1e400%)",
                "opacity(NaN)",
                "opacity(inf)",
                "opacity(-inf)",
                "opacity()",
                "opacity(   )",
                "opacity(abc)",
            ] {
                assert!(
                    parse_style_filter(input).is_err(),
                    "{input:?} was accepted as an opacity"
                );
            }
        }

        #[test]
        fn parse_style_filter_negative_brightness_contrast_saturate_are_rejected() {
            for input in [
                "brightness(-50%)",
                "brightness(-0.5)",
                "contrast(-10%)",
                "contrast(-0.0001)",
                "saturate(-20%)",
                "saturate(-1)",
            ] {
                assert!(parse_style_filter(input).is_err(), "{input:?} was accepted");
            }
            // ...but zero and negative-zero are fine, and there is no *upper*
            // bound (brightness(500%) is legal CSS).
            assert!(parse_style_filter("brightness(0%)").is_ok());
            assert!(parse_style_filter("brightness(-0%)").is_ok());
            assert!(parse_style_filter("contrast(0)").is_ok());
            assert!(parse_style_filter("saturate(500%)").is_ok());
        }

        #[test]
        fn parse_style_filter_grayscale_invert_sepia_are_range_unchecked() {
            // NOTE (spec deviation): CSS clamps grayscale/invert/sepia to
            // [0%, 100%] and rejects negatives. This parser range-checks only
            // opacity/brightness/contrast/saturate, so these pass through
            // unclamped. Pinned so the leniency is visible.
            assert_eq!(
                parse_style_filter("grayscale(-50%)").unwrap(),
                StyleFilter::Grayscale(PercentageValue::new(-50.0))
            );
            assert_eq!(
                parse_style_filter("invert(500%)").unwrap(),
                StyleFilter::Invert(PercentageValue::new(500.0))
            );
            assert_eq!(
                parse_style_filter("sepia(-1)").unwrap(),
                StyleFilter::Sepia(PercentageValue::new(-100.0))
            );
        }

        #[test]
        fn parse_style_filter_boundary_numbers_saturate_instead_of_leaking_inf() {
            // f32 overflow parses as inf in Rust; the FloatValue encoding must
            // defuse it into a finite (saturated) value.
            let StyleFilter::Blur(b) = parse_style_filter("blur(1e400px)").unwrap() else {
                panic!("not a blur")
            };
            assert!(b.width.number.get().is_finite(), "blur width leaked inf");
            assert_eq!(b.width.number.number(), isize::MAX);
            assert_eq!(b.width, b.height);

            let StyleFilter::Brightness(v) = parse_style_filter("brightness(1e400%)").unwrap()
            else {
                panic!("not a brightness")
            };
            assert!(v.normalized().is_finite(), "brightness leaked inf");

            // Underflow collapses to zero rather than erroring.
            let StyleFilter::Blur(tiny) = parse_style_filter("blur(1e-400px)").unwrap() else {
                panic!("not a blur")
            };
            assert_eq!(tiny.width.number.number(), 0);

            // i64::MAX / i64::MIN sized literals.
            for input in [
                "blur(9223372036854775807px)",
                "blur(-9223372036854775808px)",
                "offset(9223372036854775807px 0px)",
            ] {
                let parsed = parse_style_filter(input);
                assert!(parsed.is_ok(), "{input:?} rejected: {parsed:?}");
            }
        }

        #[test]
        fn parse_style_filter_unicode_input_does_not_panic() {
            for input in [
                "\u{1F600}(5px)",         // emoji function name
                "blur(5px\u{1F600})",     // emoji in the payload
                "blur(\u{FF15}px)",       // FULLWIDTH DIGIT FIVE
                "blur(5\u{0301}px)",      // digit + combining acute
                "blur(\u{2212}5px)",      // U+2212 MINUS SIGN, not ASCII '-'
                "\u{FC}ber(5px)",         // multi-byte name
                "flood(\u{1F600})",
                "composite(\u{1F600})",
                "hue-rotate(90\u{00B0})", // DEGREE SIGN instead of "deg"
                "blur(\u{200B}5px)",      // zero-width space
            ] {
                assert!(
                    parse_style_filter(input).is_err(),
                    "unicode input {input:?} was accepted"
                );
            }
        }

        #[test]
        fn parse_style_filter_extremely_long_input_terminates() {
            // 200k digits: Rust's f32 parser yields inf, which then saturates.
            let long_number = format!("blur({}px)", "9".repeat(200_000));
            let StyleFilter::Blur(b) = parse_style_filter(&long_number).unwrap() else {
                panic!("not a blur")
            };
            assert!(b.width.number.get().is_finite());

            // 200k of pure junk must be rejected, not scanned quadratically.
            let long_junk = format!("blur({})", "a".repeat(200_000));
            assert!(parse_style_filter(&long_junk).is_err());

            // A 200k-char function *name* is just a stopword miss.
            let long_name = format!("{}(5px)", "b".repeat(200_000));
            assert!(parse_style_filter(&long_name).is_err());
        }

        #[test]
        fn parse_style_filter_deeply_nested_input_does_not_stack_overflow() {
            // The paren scanners are iterative (find/rfind + a balance counter),
            // so 10k levels must terminate with an error, not blow the stack.
            let open_only = "(".repeat(10_000);
            assert!(parse_style_filter(&open_only).is_err());

            let nested = format!("blur({}{})", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_style_filter(&nested).is_err());

            let nested_vec = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_style_filter_vec(&nested_vec).is_err());
        }

        // ==================================================================
        // parse_style_filter_vec  (parser, public)
        // ==================================================================

        #[test]
        fn parse_style_filter_vec_empty_and_whitespace_only_yield_an_empty_vec() {
            // NOTE: unlike parse_style_filter(), the vec parser treats "nothing
            // at all" as a successful empty list rather than an error.
            for input in ["", "   ", "\t\n", "\r\n \t"] {
                let v = parse_style_filter_vec(input)
                    .unwrap_or_else(|e| panic!("{input:?} should be an empty vec, got {e}"));
                assert_eq!(v.len(), 0, "{input:?}");
            }
        }

        #[test]
        fn parse_style_filter_vec_valid_minimal_positive_control() {
            let v = parse_style_filter_vec("blur(5px)").unwrap();
            assert_eq!(v.len(), 1);
            assert_eq!(
                v.as_slice()[0],
                StyleFilter::Blur(StyleBlur {
                    width: PixelValue::px(5.0),
                    height: PixelValue::px(5.0),
                })
            );

            // Separators are optional: the parser resumes right after ')'.
            let packed = parse_style_filter_vec("blur(5px)opacity(0.5)").unwrap();
            assert_eq!(packed.len(), 2);
            assert_eq!(
                packed.as_slice()[1],
                StyleFilter::Opacity(PercentageValue::new(50.0))
            );

            // Interior whitespace of any flavour is tolerated.
            let spaced = parse_style_filter_vec("  blur(5px) \n\t opacity(0.5)  ").unwrap();
            assert_eq!(spaced.len(), 2);
        }

        #[test]
        fn parse_style_filter_vec_rejects_junk_between_and_around_functions() {
            for input in [
                "blur(5px);opacity(0.5)",
                "blur(5px),opacity(0.5)",
                "blur(5px) opacity(0.5) garbage",
                "garbage blur(5px)",
                "blur(5px) )",
                "component-transfer",
                "\u{1F600}",
            ] {
                assert!(
                    parse_style_filter_vec(input).is_err(),
                    "{input:?} was accepted"
                );
            }
        }

        #[test]
        fn parse_style_filter_vec_unclosed_paren_is_an_unclosed_braces_error() {
            assert!(matches!(
                parse_style_filter_vec("blur(5px"),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::UnclosedBraces
                ))
            ));
            assert!(matches!(
                parse_style_filter_vec("blur(5px) opacity(0.5"),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::UnclosedBraces
                ))
            ));
        }

        #[test]
        fn parse_style_filter_vec_many_filters_terminates_and_keeps_order() {
            // Every iteration must consume at least one byte, otherwise the
            // `while !remaining.is_empty()` loop would spin forever.
            let n = 5_000;
            let src = "blur(1px)opacity(0.5)".repeat(n);
            let v = parse_style_filter_vec(&src).unwrap();
            assert_eq!(v.len(), n * 2);
            assert_eq!(
                v.as_slice()[0],
                StyleFilter::Blur(StyleBlur {
                    width: PixelValue::px(1.0),
                    height: PixelValue::px(1.0),
                })
            );
            assert_eq!(
                v.as_slice()[v.len() - 1],
                StyleFilter::Opacity(PercentageValue::new(50.0))
            );
        }

        #[test]
        fn parse_style_filter_vec_round_trips_through_print_as_css_value() {
            // encode(decode(x)) == x for every variant that has a printable,
            // re-parsable form (i.e. everything except ComponentTransfer, see
            // parse_style_filter_component_transfer_does_not_round_trip).
            let src = "blur(5px) blur(2px 4px) flood(#ff0000ff) opacity(50%) blend(multiply) \
                       offset(10px 20px) hue-rotate(90deg) grayscale(100%) invert(25%) \
                       sepia(60%) brightness(150%) contrast(200%) saturate(50%) \
                       composite(over) composite(arithmetic 1 2 3 4) \
                       drop-shadow(10px 5px 5px #888888ff) \
                       color-matrix(1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0)";

            let parsed = parse_style_filter_vec(src).unwrap();
            assert_eq!(parsed.len(), 17);

            let printed = parsed.print_as_css_value();
            let reparsed = parse_style_filter_vec(&printed)
                .unwrap_or_else(|e| panic!("re-parsing {printed:?} failed: {e}"));

            assert_eq!(parsed, reparsed, "round-trip changed the filter list");
            assert_eq!(printed, reparsed.print_as_css_value(), "printing is unstable");
        }

        #[test]
        fn parse_style_filter_single_round_trips_for_every_printable_variant() {
            for src in [
                "blend(color-dodge)",
                "flood(#01020304)",
                "blur(0px)",
                "blur(1px 2px)",
                "opacity(0%)",
                "opacity(100%)",
                "color-matrix(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0)",
                "drop-shadow(1px 2px)",
                "offset(-3px 4px)",
                "composite(xor)",
                "composite(arithmetic 0 -1 2.5 3)",
                "brightness(0%)",
                "contrast(300%)",
                "grayscale(50%)",
                "hue-rotate(-45deg)",
                "invert(100%)",
                "saturate(0%)",
                "sepia(10%)",
            ] {
                let parsed = parse_style_filter(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
                let printed = parsed.print_as_css_value();
                let reparsed = parse_style_filter(&printed)
                    .unwrap_or_else(|e| panic!("{src:?} printed as {printed:?}, reparse: {e}"));
                assert_eq!(parsed, reparsed, "{src:?} -> {printed:?} did not round-trip");
            }
        }

        // ==================================================================
        // parse_one_filter_function  (parser, private)
        // ==================================================================

        #[test]
        fn parse_one_filter_function_empty_and_whitespace_only_input() {
            assert!(matches!(
                parse_one_filter_function(""),
                Err(CssStyleFilterParseError::InvalidFilter(""))
            ));
            assert!(matches!(
                parse_one_filter_function("   "),
                Err(CssStyleFilterParseError::InvalidFilter("   "))
            ));
            assert!(matches!(
                parse_one_filter_function("\t\n"),
                Err(CssStyleFilterParseError::InvalidFilter("\t\n"))
            ));
        }

        #[test]
        fn parse_one_filter_function_returns_the_unconsumed_rest_verbatim() {
            let (filter, rest) = parse_one_filter_function("blur(5px) rest here").unwrap();
            assert_eq!(
                filter,
                StyleFilter::Blur(StyleBlur {
                    width: PixelValue::px(5.0),
                    height: PixelValue::px(5.0),
                })
            );
            assert_eq!(rest, " rest here");

            let (_, empty_rest) = parse_one_filter_function("blur(5px)").unwrap();
            assert_eq!(empty_rest, "");
        }

        #[test]
        fn parse_one_filter_function_balances_nested_parens_before_splitting() {
            // The first ')' is *not* the terminator here: the balance counter has
            // to walk past the one closing rgb().
            let (filter, rest) = parse_one_filter_function("flood(rgb(255, 0, 0))rest").unwrap();
            assert_eq!(filter, StyleFilter::Flood(ColorU::RED));
            assert_eq!(rest, "rest");
        }

        #[test]
        fn parse_one_filter_function_unbalanced_parens_are_rejected() {
            assert!(matches!(
                parse_one_filter_function("blur(5px"),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::UnclosedBraces
                ))
            ));
            assert!(matches!(
                parse_one_filter_function("flood(rgb(255,0,0)"),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::UnclosedBraces
                ))
            ));
            // No '(' at all -> InvalidFilter, carrying the whole input.
            assert!(matches!(
                parse_one_filter_function("component-transfer"),
                Err(CssStyleFilterParseError::InvalidFilter("component-transfer"))
            ));
            assert!(matches!(
                parse_one_filter_function(")"),
                Err(CssStyleFilterParseError::InvalidFilter(")"))
            ));
        }

        #[test]
        fn parse_one_filter_function_splits_on_char_boundaries_with_unicode() {
            // close_paren+1 must land on a char boundary or this slice panics.
            let (filter, rest) = parse_one_filter_function("blur(5px)\u{1F600}\u{00E9}").unwrap();
            assert!(matches!(filter, StyleFilter::Blur(_)));
            assert_eq!(rest, "\u{1F600}\u{00E9}");

            // Multi-byte characters *before* the '(' are equally safe.
            assert!(parse_one_filter_function("bl\u{FC}r(5px)").is_err());
            assert!(parse_one_filter_function("\u{1F600}(5px)").is_err());
        }

        #[test]
        fn parse_one_filter_function_long_and_nested_input_terminates() {
            let long = format!("blur({}px)trailing", "9".repeat(100_000));
            let (filter, rest) = parse_one_filter_function(&long).unwrap();
            assert!(matches!(filter, StyleFilter::Blur(_)));
            assert_eq!(rest, "trailing");

            // 10k unclosed levels: iterative balance counter, no recursion.
            let nested = format!("blur({}", "(".repeat(10_000));
            assert!(matches!(
                parse_one_filter_function(&nested),
                Err(CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::UnclosedBraces
                ))
            ));
        }

        // ==================================================================
        // parse_style_blur  (parser, private)
        // ==================================================================

        #[test]
        fn parse_style_blur_one_value_is_used_for_both_axes() {
            let b = parse_style_blur("5px").unwrap();
            assert_eq!(b.width, PixelValue::px(5.0));
            assert_eq!(b.height, PixelValue::px(5.0));

            let b2 = parse_style_blur("2px 4px").unwrap();
            assert_eq!(b2.width, PixelValue::px(2.0));
            assert_eq!(b2.height, PixelValue::px(4.0));

            // split_whitespace(), so any run of whitespace separates.
            let b3 = parse_style_blur("  2px \t\n 4px  ").unwrap();
            assert_eq!(b3, b2);
        }

        #[test]
        fn parse_style_blur_empty_or_whitespace_only_is_an_empty_pixel_value() {
            assert!(matches!(
                parse_style_blur(""),
                Err(CssStyleBlurParseError::Pixel(
                    CssPixelValueParseError::EmptyString
                ))
            ));
            assert!(matches!(
                parse_style_blur("   \t\n"),
                Err(CssStyleBlurParseError::Pixel(
                    CssPixelValueParseError::EmptyString
                ))
            ));
        }

        #[test]
        fn parse_style_blur_three_or_more_components_is_too_many() {
            assert!(matches!(
                parse_style_blur("1px 2px 3px"),
                Err(CssStyleBlurParseError::TooManyComponents("1px 2px 3px"))
            ));
            let four = "1px 2px 3px 4px";
            assert!(matches!(
                parse_style_blur(four),
                Err(CssStyleBlurParseError::TooManyComponents(f)) if f == four
            ));
            // The check runs before the pixel parse, so garbage in slot 3 still
            // reports TooManyComponents rather than a pixel error.
            assert!(matches!(
                parse_style_blur("1px 2px garbage"),
                Err(CssStyleBlurParseError::TooManyComponents(_))
            ));
        }

        #[test]
        fn parse_style_blur_garbage_returns_err_and_never_panics() {
            for garbage in [
                "abc", "px", "5xx", "-", ".", "5..5px", "1,5px", ";", "\0", "5px;", "()",
            ] {
                assert!(
                    parse_style_blur(garbage).is_err(),
                    "garbage {garbage:?} was accepted"
                );
            }
        }

        #[test]
        fn parse_style_blur_accepts_negative_and_unitless_radii() {
            // NOTE (spec deviation): CSS requires a non-negative <length> for
            // blur() and forbids unitless non-zero numbers. Both are accepted
            // here; a negative blur radius reaches the compositor unchecked.
            let neg = parse_style_blur("-5px").unwrap();
            assert_eq!(neg.width, PixelValue::px(-5.0));
            let unitless = parse_style_blur("5").unwrap();
            assert_eq!(unitless.width, PixelValue::px(5.0));
            assert_eq!(unitless.width.metric, SizeMetric::Px);
        }

        #[test]
        fn parse_style_blur_nan_and_inf_are_encoded_not_propagated() {
            // Rust's f32 parser accepts "NaN"/"inf"; parse_pixel_value() has no
            // unit suffix to strip, so they reach FloatValue, which defuses them.
            let nan = parse_style_blur("NaN").unwrap();
            assert_eq!(nan.width.number.number(), 0, "NaN leaked into the encoding");
            assert!(nan.width.number.get().is_finite());

            let inf = parse_style_blur("inf").unwrap();
            assert_eq!(inf.width.number.number(), isize::MAX);
            assert!(inf.width.number.get().is_finite());

            let neg_inf = parse_style_blur("-inf").unwrap();
            assert_eq!(neg_inf.width.number.number(), isize::MIN);
            assert!(neg_inf.width.number.get().is_finite());

            // Both axes independently.
            let mixed = parse_style_blur("NaN inf").unwrap();
            assert_eq!(mixed.width.number.number(), 0);
            assert_eq!(mixed.height.number.number(), isize::MAX);
        }

        #[test]
        fn parse_style_blur_boundary_numbers_stay_finite() {
            for input in [
                "0px",
                "-0px",
                "1e400px",
                "-1e400px",
                "1e-400px",
                "9223372036854775807px",
                "-9223372036854775808px",
                "0.0000000000000000000001px",
            ] {
                let b = parse_style_blur(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
                assert!(
                    b.width.number.get().is_finite() && b.height.number.get().is_finite(),
                    "{input:?} leaked a non-finite value"
                );
            }
            // -0 must not surface as a negative zero.
            assert!(parse_style_blur("-0px")
                .unwrap()
                .width
                .number
                .get()
                .is_sign_positive());
        }

        #[test]
        fn parse_style_blur_unicode_does_not_panic() {
            for input in [
                "\u{1F600}",
                "5\u{1F600}",
                "\u{FF15}px",
                "5\u{0301}px",
                "\u{2212}5px",
                "5px \u{1F600}",
            ] {
                assert!(
                    parse_style_blur(input).is_err(),
                    "unicode {input:?} was accepted"
                );
            }
        }

        #[test]
        fn parse_style_blur_long_and_nested_input_terminates() {
            let long = format!("{}px", "9".repeat(200_000));
            assert!(parse_style_blur(&long).unwrap().width.number.get().is_finite());

            let long_junk = "a".repeat(200_000);
            assert!(parse_style_blur(&long_junk).is_err());

            // 10k tokens -> TooManyComponents, found without any recursion.
            let many = "1px ".repeat(10_000);
            assert!(matches!(
                parse_style_blur(&many),
                Err(CssStyleBlurParseError::TooManyComponents(_))
            ));

            let nested = format!("{}5px{}", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_style_blur(&nested).is_err());
        }

        // ==================================================================
        // parse_color_matrix  (parser, private)
        // ==================================================================

        #[test]
        fn parse_color_matrix_valid_minimal_positive_control() {
            let m = parse_color_matrix(IDENTITY_MATRIX_SRC).unwrap();
            assert_eq!(m.m0, FloatValue::const_new(1));
            assert_eq!(m.m1, FloatValue::const_new(0));
            assert_eq!(m.m6, FloatValue::const_new(1));
            assert_eq!(m.m12, FloatValue::const_new(1));
            assert_eq!(m.m18, FloatValue::const_new(1));
            assert_eq!(m.m19, FloatValue::const_new(0));

            // Any whitespace works as a separator, and the 20 values land in order.
            let ordered = (0..20)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("\n\t ");
            let m2 = parse_color_matrix(&ordered).unwrap();
            for (i, v) in m2.to_array().iter().enumerate() {
                assert_eq!(*v, FloatValue::const_new(i as isize), "index {i}");
            }
        }

        #[test]
        fn parse_color_matrix_wrong_component_count_is_reported_exactly() {
            // Empty / whitespace-only -> zero components.
            for input in ["", "   ", "\t\n"] {
                match parse_color_matrix(input) {
                    Err(CssStyleColorMatrixParseError::WrongNumberOfComponents {
                        expected,
                        got,
                        input: reported,
                    }) => {
                        assert_eq!(expected, 20);
                        assert_eq!(got, 0);
                        assert_eq!(reported, input);
                    }
                    other => panic!("{input:?} should be 0-of-20, got {other:?}"),
                }
            }

            // 19 and 21 components.
            let nineteen = "1 ".repeat(19);
            assert!(matches!(
                parse_color_matrix(nineteen.trim()),
                Err(CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: 19,
                    ..
                })
            ));
            let twentyone = "1 ".repeat(21);
            assert!(matches!(
                parse_color_matrix(twentyone.trim()),
                Err(CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: 21,
                    ..
                })
            ));
        }

        #[test]
        fn parse_color_matrix_garbage_component_is_a_float_error() {
            let with_junk = "1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 abc";
            assert!(matches!(
                parse_color_matrix(with_junk),
                Err(CssStyleColorMatrixParseError::Float(_))
            ));
            // Units are not floats.
            let with_px = "1px 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0";
            assert!(matches!(
                parse_color_matrix(with_px),
                Err(CssStyleColorMatrixParseError::Float(_))
            ));
            // A percentage is not a float either.
            let with_pct = "50% 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0";
            assert!(parse_color_matrix(with_pct).is_err());
        }

        #[test]
        fn parse_color_matrix_boundary_numbers_saturate() {
            let src = "NaN inf -inf 1e400 -1e400 1e-400 -0 0 1 -1 \
                       9223372036854775807 -9223372036854775808 0.5 -0.5 \
                       3.4028235e38 -3.4028235e38 1.1754944e-38 0 0 0";
            let m = parse_color_matrix(src).unwrap();
            for (i, v) in m.to_array().iter().enumerate() {
                assert!(
                    v.get().is_finite(),
                    "component {i} decoded to a non-finite {}",
                    v.get()
                );
            }
            assert_eq!(m.m0.number(), 0, "NaN did not encode to zero");
            assert_eq!(m.m1.number(), isize::MAX, "inf did not saturate");
            assert_eq!(m.m2.number(), isize::MIN, "-inf did not saturate");
            assert_eq!(m.m3.number(), isize::MAX, "1e400 did not saturate");
            assert_eq!(m.m4.number(), isize::MIN);
            assert_eq!(m.m5.number(), 0, "1e-400 did not underflow to zero");
            assert!(m.m6.get().is_sign_positive(), "-0 leaked a negative zero");
        }

        #[test]
        fn parse_color_matrix_unicode_does_not_panic() {
            let emoji = "\u{1F600} 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0";
            assert!(parse_color_matrix(emoji).is_err());
            // ARABIC-INDIC DIGIT FIVE is `char::is_numeric()` but not an f32.
            let arabic = "\u{0665} 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0";
            assert!(parse_color_matrix(arabic).is_err());
            // 20 emoji: right count, wrong type.
            let all_emoji = "\u{1F600} ".repeat(20);
            assert!(matches!(
                parse_color_matrix(all_emoji.trim()),
                Err(CssStyleColorMatrixParseError::Float(_))
            ));
        }

        #[test]
        fn parse_color_matrix_long_and_nested_input_terminates() {
            // One 100k-digit component: parses to inf, then saturates.
            let huge = format!("{} 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0", "9".repeat(100_000));
            let m = parse_color_matrix(&huge).unwrap();
            assert_eq!(m.m0.number(), isize::MAX);
            assert!(m.m0.get().is_finite());

            // 100k components: counted, rejected, no allocation blow-up.
            let many = "1 ".repeat(100_000);
            assert!(matches!(
                parse_color_matrix(many.trim()),
                Err(CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: 100_000,
                    ..
                })
            ));

            // 10k nested brackets in one component: rejected, no stack overflow.
            let nested = format!(
                "{}1{} 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0",
                "(".repeat(10_000),
                ")".repeat(10_000)
            );
            assert!(parse_color_matrix(&nested).is_err());
        }

        // ==================================================================
        // parse_filter_offset  (parser, private)
        // ==================================================================

        #[test]
        fn parse_filter_offset_valid_minimal_positive_control() {
            assert_eq!(
                parse_filter_offset("10px 20px").unwrap(),
                StyleFilterOffset {
                    x: PixelValue::px(10.0),
                    y: PixelValue::px(20.0),
                }
            );
            // Mixed units and negatives are fine.
            assert_eq!(
                parse_filter_offset("-1.5em \t 20%").unwrap(),
                StyleFilterOffset {
                    x: PixelValue::em(-1.5),
                    y: PixelValue::percent(20.0),
                }
            );
        }

        #[test]
        fn parse_filter_offset_requires_exactly_two_components() {
            for (input, got) in [("", 0), ("   ", 0), ("1px", 1), ("1px 2px 3px", 3)] {
                match parse_filter_offset(input) {
                    Err(CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                        expected,
                        got: g,
                        input: reported,
                    }) => {
                        assert_eq!(expected, 2, "{input:?}");
                        assert_eq!(g, got, "{input:?}");
                        assert_eq!(reported, input);
                    }
                    other => panic!("{input:?} should be {got}-of-2, got {other:?}"),
                }
            }
        }

        #[test]
        fn parse_filter_offset_garbage_and_unicode_return_pixel_errors() {
            for input in [
                "abc def",
                "1px abc",
                "abc 1px",
                "\u{1F600} \u{1F600}",
                "1px \u{FF15}px",
                "; ;",
                "\0 \0",
            ] {
                assert!(
                    matches!(
                        parse_filter_offset(input),
                        Err(CssStyleFilterOffsetParseError::Pixel(_))
                    ),
                    "{input:?} should be a pixel error"
                );
            }
        }

        #[test]
        fn parse_filter_offset_boundary_numbers_stay_finite() {
            for input in [
                "0px 0px",
                "-0px -0px",
                "1e400px -1e400px",
                "1e-400px 1e-400px",
                "9223372036854775807px -9223372036854775808px",
                "NaN NaN",
                "inf -inf",
            ] {
                let o = parse_filter_offset(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
                assert!(
                    o.x.number.get().is_finite() && o.y.number.get().is_finite(),
                    "{input:?} leaked a non-finite offset"
                );
            }
            // NaN is silently accepted as a zero offset (no unit to reject it).
            let nan = parse_filter_offset("NaN NaN").unwrap();
            assert_eq!(nan.x.number.number(), 0);
            assert_eq!(nan.y.number.number(), 0);
        }

        #[test]
        fn parse_filter_offset_long_input_terminates() {
            let long = format!("{}px 0px", "9".repeat(200_000));
            assert!(parse_filter_offset(&long).unwrap().x.number.get().is_finite());

            let many = "1px ".repeat(100_000);
            assert!(matches!(
                parse_filter_offset(many.trim()),
                Err(CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: 100_000,
                    ..
                })
            ));

            let nested = format!("{}{} 0px", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_filter_offset(&nested).is_err());
        }

        // ==================================================================
        // parse_filter_composite  (parser, private)
        // ==================================================================

        #[test]
        fn parse_filter_composite_valid_minimal_positive_control() {
            for (input, expected) in [
                ("over", StyleCompositeFilter::Over),
                ("in", StyleCompositeFilter::In),
                ("atop", StyleCompositeFilter::Atop),
                ("out", StyleCompositeFilter::Out),
                ("xor", StyleCompositeFilter::Xor),
                ("lighter", StyleCompositeFilter::Lighter),
            ] {
                assert_eq!(parse_filter_composite(input).unwrap(), expected, "{input:?}");
                // Surrounding whitespace is eaten by split_whitespace().
                let padded = format!("  \t{input}\n ");
                assert_eq!(parse_filter_composite(&padded).unwrap(), expected);
            }

            assert_eq!(
                parse_filter_composite("arithmetic 1 2 3 4").unwrap(),
                StyleCompositeFilter::Arithmetic(ArithmeticCoefficients {
                    k1: FloatValue::const_new(1),
                    k2: FloatValue::const_new(2),
                    k3: FloatValue::const_new(3),
                    k4: FloatValue::const_new(4),
                })
            );
        }

        #[test]
        fn parse_filter_composite_empty_whitespace_and_garbage_are_invalid_operators() {
            for input in ["", "   ", "\t\n", "OVER", "Over", "arithmetics", ";", "\0", "\u{1F600}"] {
                assert!(
                    matches!(
                        parse_filter_composite(input),
                        Err(CssStyleCompositeFilterParseError::Invalid(_))
                    ),
                    "{input:?} should be an invalid operator"
                );
            }
            // The offending operator is echoed back verbatim.
            assert!(matches!(
                parse_filter_composite(""),
                Err(CssStyleCompositeFilterParseError::Invalid(InvalidValueErr("")))
            ));
            assert!(matches!(
                parse_filter_composite("nope 1 2 3 4"),
                Err(CssStyleCompositeFilterParseError::Invalid(InvalidValueErr("nope")))
            ));
        }

        #[test]
        fn parse_filter_composite_arithmetic_missing_coefficients_report_how_many_were_found() {
            for (input, got) in [
                ("arithmetic", 0),
                ("arithmetic 1", 1),
                ("arithmetic 1 2", 2),
                ("arithmetic 1 2 3", 3),
            ] {
                match parse_filter_composite(input) {
                    Err(CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                        expected,
                        got: g,
                        input: reported,
                    }) => {
                        assert_eq!(expected, 4, "{input:?}");
                        assert_eq!(g, got, "{input:?}");
                        assert_eq!(reported, input);
                    }
                    other => panic!("{input:?} should be {got}-of-4, got {other:?}"),
                }
            }
        }

        #[test]
        fn parse_filter_composite_arithmetic_ignores_extra_coefficients() {
            // NOTE: there is no TooManyComponents check here (unlike blur()), so
            // trailing junk after the 4th coefficient is silently dropped —
            // including junk that is not even a number.
            let coeffs = parse_filter_composite("arithmetic 1 2 3 4 5 6 garbage").unwrap();
            assert_eq!(
                coeffs,
                StyleCompositeFilter::Arithmetic(ArithmeticCoefficients {
                    k1: FloatValue::const_new(1),
                    k2: FloatValue::const_new(2),
                    k3: FloatValue::const_new(3),
                    k4: FloatValue::const_new(4),
                })
            );
        }

        #[test]
        fn parse_filter_composite_arithmetic_garbage_coefficient_is_a_float_error() {
            for input in [
                "arithmetic a 2 3 4",
                "arithmetic 1 2 3 zzz",
                "arithmetic 1px 2 3 4",
                "arithmetic 1 2 3 \u{1F600}",
                "arithmetic 1,2 3 4 5",
            ] {
                assert!(
                    matches!(
                        parse_filter_composite(input),
                        Err(CssStyleCompositeFilterParseError::Float(_))
                    ),
                    "{input:?} should be a float error"
                );
            }
        }

        #[test]
        fn parse_filter_composite_arithmetic_boundary_numbers_saturate() {
            let c = parse_filter_composite("arithmetic NaN inf -inf 1e400").unwrap();
            let StyleCompositeFilter::Arithmetic(k) = c else {
                panic!("not arithmetic")
            };
            assert_eq!(k.k1.number(), 0, "NaN did not encode to zero");
            assert_eq!(k.k2.number(), isize::MAX, "inf did not saturate");
            assert_eq!(k.k3.number(), isize::MIN, "-inf did not saturate");
            assert_eq!(k.k4.number(), isize::MAX, "1e400 did not saturate");
            for v in [k.k1, k.k2, k.k3, k.k4] {
                assert!(v.get().is_finite(), "coefficient leaked {}", v.get());
            }

            // -0 must not survive as a negative zero.
            let zeros = parse_filter_composite("arithmetic -0 0 -0.0 1e-400").unwrap();
            let StyleCompositeFilter::Arithmetic(z) = zeros else {
                panic!("not arithmetic")
            };
            for v in [z.k1, z.k2, z.k3, z.k4] {
                assert_eq!(v.number(), 0);
                assert!(v.get().is_sign_positive());
            }
        }

        #[test]
        fn parse_filter_composite_long_and_nested_input_terminates() {
            let long_operator = "a".repeat(200_000);
            assert!(matches!(
                parse_filter_composite(&long_operator),
                Err(CssStyleCompositeFilterParseError::Invalid(_))
            ));

            // A 100k-digit coefficient saturates rather than hanging.
            let long_coeff = format!("arithmetic {} 0 0 0", "9".repeat(100_000));
            let StyleCompositeFilter::Arithmetic(k) =
                parse_filter_composite(&long_coeff).unwrap()
            else {
                panic!("not arithmetic")
            };
            assert_eq!(k.k1.number(), isize::MAX);

            // 100k coefficients: the first 4 win, the rest are ignored.
            let many = format!("arithmetic {}", "1 ".repeat(100_000));
            assert!(matches!(
                parse_filter_composite(&many),
                Ok(StyleCompositeFilter::Arithmetic(_))
            ));

            let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
            assert!(parse_filter_composite(&nested).is_err());
        }

        // ==================================================================
        // Error getters: to_contained() / to_shared() round-trips
        // ==================================================================

        fn all_filter_errors() -> Vec<CssStyleFilterParseError<'static>> {
            vec![
                CssStyleFilterParseError::InvalidFilter(""),
                CssStyleFilterParseError::InvalidFilter("blurry(5px)"),
                CssStyleFilterParseError::InvalidFilter("\u{1F600}"),
                CssStyleFilterParseError::InvalidParenthesis(ParenthesisParseError::UnclosedBraces),
                CssStyleFilterParseError::InvalidParenthesis(
                    ParenthesisParseError::StopWordNotFound("nope"),
                ),
                CssStyleFilterParseError::Shadow(CssShadowParseError::TooManyOrTooFewComponents(
                    "1px",
                )),
                CssStyleFilterParseError::BlendMode(InvalidValueErr("bogus")),
                CssStyleFilterParseError::Color(CssColorParseError::InvalidColor("nope")),
                CssStyleFilterParseError::Color(CssColorParseError::EmptyInput),
                CssStyleFilterParseError::Opacity(PercentageParseError::NoPercentSign),
                CssStyleFilterParseError::Brightness(PercentageParseError::InvalidUnit(
                    "px".to_string().into(),
                )),
                CssStyleFilterParseError::Contrast(PercentageParseError::InvalidUnit(
                    String::new().into(),
                )),
                CssStyleFilterParseError::Saturate(PercentageParseError::InvalidUnit(
                    "\u{1F600}".to_string().into(),
                )),
                CssStyleFilterParseError::Blur(CssStyleBlurParseError::TooManyComponents(
                    "1px 2px 3px",
                )),
                CssStyleFilterParseError::Blur(CssStyleBlurParseError::Pixel(
                    CssPixelValueParseError::EmptyString,
                )),
                CssStyleFilterParseError::ColorMatrix(
                    CssStyleColorMatrixParseError::WrongNumberOfComponents {
                        expected: 20,
                        got: 0,
                        input: "",
                    },
                ),
                CssStyleFilterParseError::ColorMatrix(CssStyleColorMatrixParseError::Float(
                    float_err(),
                )),
                CssStyleFilterParseError::Offset(CssStyleFilterOffsetParseError::Pixel(
                    CssPixelValueParseError::InvalidPixelValue("abc"),
                )),
                CssStyleFilterParseError::Offset(
                    CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                        expected: 2,
                        got: 3,
                        input: "1px 2px 3px",
                    },
                ),
                CssStyleFilterParseError::Composite(CssStyleCompositeFilterParseError::Invalid(
                    InvalidValueErr(""),
                )),
                CssStyleFilterParseError::Composite(CssStyleCompositeFilterParseError::Float(
                    float_err(),
                )),
                CssStyleFilterParseError::Composite(
                    CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                        expected: 4,
                        got: 2,
                        input: "arithmetic 1 2",
                    },
                ),
                CssStyleFilterParseError::Angle(CssAngleValueParseError::EmptyString),
                CssStyleFilterParseError::Angle(CssAngleValueParseError::InvalidAngle("90\u{00B0}")),
                CssStyleFilterParseError::Angle(CssAngleValueParseError::NoValueGiven(
                    "deg",
                    AngleMetric::Degree,
                )),
            ]
        }

        #[test]
        fn filter_parse_error_round_trips_through_owned() {
            for e in all_filter_errors() {
                let owned = e.to_contained();
                assert_eq!(
                    e,
                    owned.to_shared(),
                    "to_contained()/to_shared() is not the identity for {e:?}"
                );
            }
        }

        #[test]
        fn filter_parse_error_to_contained_keeps_extreme_payloads_intact() {
            let huge = "x".repeat(100_000);
            let e = CssStyleFilterParseError::InvalidFilter(&huge);
            let owned = e.to_contained();
            let CssStyleFilterParseErrorOwned::InvalidFilter(ref s) = owned else {
                panic!("wrong variant")
            };
            assert_eq!(s.as_str().len(), 100_000);
            assert_eq!(owned.to_shared(), e);

            // Empty and multi-byte payloads survive the AzString hop unchanged.
            for payload in ["", "\u{1F600}\u{0301}", "\0", "  "] {
                let e = CssStyleFilterParseError::InvalidFilter(payload);
                assert_eq!(e.to_contained().to_shared(), e, "{payload:?}");
            }
        }

        #[test]
        fn filter_parse_error_display_never_panics() {
            for e in all_filter_errors() {
                // impl_debug_as_display!: both formats must render.
                let via_display = format!("{e}");
                let via_debug = format!("{e:?}");
                assert!(!via_display.is_empty());
                assert_eq!(via_display, via_debug);
                // ...and so must the owned form.
                assert!(!format!("{:?}", e.to_contained()).is_empty());
            }
        }

        #[test]
        fn blur_parse_error_round_trips_through_owned() {
            let huge = "1px ".repeat(50_000);
            let cases = [
                CssStyleBlurParseError::Pixel(CssPixelValueParseError::EmptyString),
                CssStyleBlurParseError::Pixel(CssPixelValueParseError::NoValueGiven(
                    "px",
                    SizeMetric::Px,
                )),
                CssStyleBlurParseError::Pixel(CssPixelValueParseError::InvalidPixelValue("abc")),
                CssStyleBlurParseError::TooManyComponents(""),
                CssStyleBlurParseError::TooManyComponents("1px 2px 3px"),
                CssStyleBlurParseError::TooManyComponents("\u{1F600}"),
                CssStyleBlurParseError::TooManyComponents(&huge),
            ];
            for e in cases {
                assert_eq!(e.to_contained().to_shared(), e, "{e:?}");
                assert!(!format!("{e}").is_empty());
            }
        }

        #[test]
        fn color_matrix_parse_error_round_trips_through_owned() {
            let huge = "1 ".repeat(50_000);
            let cases = [
                CssStyleColorMatrixParseError::Float(float_err()),
                CssStyleColorMatrixParseError::Float("".parse::<f32>().unwrap_err()),
                CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: 0,
                    input: "",
                },
                CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: 20,
                    got: usize::MAX,
                    input: "\u{1F600}",
                },
                CssStyleColorMatrixParseError::WrongNumberOfComponents {
                    expected: usize::MAX,
                    got: 50_000,
                    input: &huge,
                },
            ];
            for e in cases {
                assert_eq!(e.to_contained().to_shared(), e, "{e:?}");
                assert!(!format!("{e}").is_empty());
            }
        }

        #[test]
        fn filter_offset_parse_error_round_trips_through_owned() {
            let cases = [
                CssStyleFilterOffsetParseError::Pixel(CssPixelValueParseError::EmptyString),
                CssStyleFilterOffsetParseError::Pixel(CssPixelValueParseError::InvalidPixelValue(
                    "\u{1F600}",
                )),
                CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: 0,
                    input: "",
                },
                CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                    expected: 2,
                    got: usize::MAX,
                    input: "1px 2px 3px",
                },
            ];
            for e in cases {
                assert_eq!(e.to_contained().to_shared(), e, "{e:?}");
                assert!(!format!("{e}").is_empty());
            }
        }

        #[test]
        fn composite_parse_error_round_trips_through_owned() {
            let cases = [
                CssStyleCompositeFilterParseError::Invalid(InvalidValueErr("")),
                CssStyleCompositeFilterParseError::Invalid(InvalidValueErr("\u{1F600}")),
                CssStyleCompositeFilterParseError::Float(float_err()),
                CssStyleCompositeFilterParseError::Float("".parse::<f32>().unwrap_err()),
                CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                    expected: 4,
                    got: 0,
                    input: "arithmetic",
                },
                CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                    expected: 4,
                    got: usize::MAX,
                    input: "",
                },
            ];
            for e in cases {
                assert_eq!(e.to_contained().to_shared(), e, "{e:?}");
                assert!(!format!("{e}").is_empty());
            }
        }

        #[test]
        fn parse_errors_from_real_inputs_round_trip_through_owned() {
            // The same identity, but on errors the parsers actually produce.
            for input in [
                "",
                "blurry(5px)",
                "blur(5px",
                "blur(5px 10px 15px)",
                "opacity(2)",
                "brightness(-1)",
                "color-matrix(1 2 3)",
                "offset(1px)",
                "composite(nope)",
                "composite(arithmetic 1 2)",
                "hue-rotate(abc)",
                "flood(notacolor)",
                "drop-shadow(1px)",
                "blend(nope)",
                "\u{1F600}(5px)",
            ] {
                let Err(e) = parse_style_filter(input) else {
                    continue;
                };
                assert_eq!(
                    e.to_contained().to_shared(),
                    e,
                    "round-trip failed for the error of {input:?}"
                );
                assert!(!format!("{e}").is_empty(), "{input:?} has an empty Display");
            }
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
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
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
    fn test_parse_standard_css_filters() {
        let brightness = parse_style_filter("brightness(150%)").unwrap();
        if let StyleFilter::Brightness(v) = brightness {
            assert!((v.normalized() - 1.5).abs() < 0.001);
        } else {
            panic!("expected Brightness");
        }

        let contrast = parse_style_filter("contrast(200%)").unwrap();
        if let StyleFilter::Contrast(v) = contrast {
            assert!((v.normalized() - 2.0).abs() < 0.001);
        } else {
            panic!("expected Contrast");
        }

        let grayscale = parse_style_filter("grayscale(100%)").unwrap();
        if let StyleFilter::Grayscale(v) = grayscale {
            assert!((v.normalized() - 1.0).abs() < 0.001);
        } else {
            panic!("expected Grayscale");
        }

        let hue = parse_style_filter("hue-rotate(90deg)").unwrap();
        assert!(matches!(hue, StyleFilter::HueRotate(_)));

        let invert = parse_style_filter("invert(75%)").unwrap();
        if let StyleFilter::Invert(v) = invert {
            assert!((v.normalized() - 0.75).abs() < 0.001);
        } else {
            panic!("expected Invert");
        }

        let saturate = parse_style_filter("saturate(50%)").unwrap();
        if let StyleFilter::Saturate(v) = saturate {
            assert!((v.normalized() - 0.5).abs() < 0.001);
        } else {
            panic!("expected Saturate");
        }

        let sepia = parse_style_filter("sepia(60%)").unwrap();
        if let StyleFilter::Sepia(v) = sepia {
            assert!((v.normalized() - 0.6).abs() < 0.001);
        } else {
            panic!("expected Sepia");
        }
    }

    #[test]
    fn test_negative_values_rejected() {
        assert!(parse_style_filter("brightness(-50%)").is_err());
        assert!(parse_style_filter("contrast(-10%)").is_err());
        assert!(parse_style_filter("saturate(-20%)").is_err());
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
