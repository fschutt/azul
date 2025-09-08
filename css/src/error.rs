//! CSS parsing error types

use alloc::{string::String, vec::Vec};

/// Main error type for CSS parsing failures
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssParsingError<'a> {
    /// Error parsing a pixel value (e.g., "10px", "50%")
    PixelValue(CssPixelValueParseError<'a>),
    /// Error parsing a color value
    Color(CssColorParseError<'a>),
    /// Error parsing an angle value
    Angle(CssAngleValueParseError<'a>),
    /// Error parsing a direction value
    Direction(CssDirectionParseError<'a>),
    /// Error parsing a border property
    Border(CssBorderParseError<'a>),
    /// Error parsing a shadow property
    Shadow(CssShadowParseError<'a>),
    /// Error parsing a background property
    Background(CssBackgroundParseError<'a>),
    /// Error parsing a font property
    Font(CssFontParseError<'a>),
    /// Error parsing a transform property
    Transform(CssTransformParseError<'a>),
    /// Error parsing a filter property
    Filter(CssStyleFilterParseError<'a>),
    /// Error parsing a border radius property
    BorderRadius(CssStyleBorderRadiusParseError<'a>),
    /// Generic parsing error for unknown CSS values
    InvalidValue(&'a str),
    /// Error when a required property is missing
    MissingValue,
    /// Error when parsing encounters unexpected token
    UnexpectedValue(&'a str),
}

/// Owned version of CssParsingError for when lifetime management is needed
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CssParsingErrorOwned {
    pub error: String,
}

impl<'a> CssParsingError<'a> {
    /// Convert this error to an owned version
    pub fn to_owned(&self) -> CssParsingErrorOwned {
        CssParsingErrorOwned {
            error: format!("{:?}", self),
        }
    }
}

/// Error type for pixel value parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPixelValueParseError<'a> {
    /// Could not parse the numeric part
    InvalidNumber(&'a str),
    /// Unknown unit suffix
    InvalidUnit(&'a str),
    /// Missing unit
    NoUnit(&'a str),
    /// Value is out of valid range
    ValueOutOfRange(&'a str),
}

/// Error type for color value parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssColorParseError<'a> {
    /// Invalid color name
    InvalidColorName(&'a str),
    /// Invalid hex color format
    InvalidHexColor(&'a str),
    /// Invalid RGB/RGBA format
    InvalidRgbColor(&'a str),
    /// Invalid HSL/HSLA format
    InvalidHslColor(&'a str),
    /// Missing parentheses or brackets
    MissingParentheses(&'a str),
    /// Invalid number of arguments
    InvalidArgumentCount(&'a str),
}

/// Error type for angle value parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssAngleValueParseError<'a> {
    /// Could not parse the numeric part
    InvalidNumber(&'a str),
    /// Unknown angle unit
    InvalidUnit(&'a str),
    /// Missing unit
    NoUnit(&'a str),
}

/// Error type for direction parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssDirectionParseError<'a> {
    /// Invalid direction keyword
    InvalidDirection(&'a str),
    /// Invalid corner specification
    InvalidCorner(&'a str),
}

/// Error type for border parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssBorderParseError<'a> {
    /// Invalid border style
    InvalidBorderStyle(&'a str),
    /// Invalid border width
    InvalidBorderWidth(CssPixelValueParseError<'a>),
    /// Invalid border color
    InvalidBorderColor(CssColorParseError<'a>),
}

/// Error type for shadow parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssShadowParseError<'a> {
    /// Invalid shadow offset
    InvalidOffset(CssPixelValueParseError<'a>),
    /// Invalid blur radius
    InvalidBlurRadius(CssPixelValueParseError<'a>),
    /// Invalid spread radius
    InvalidSpreadRadius(CssPixelValueParseError<'a>),
    /// Invalid shadow color
    InvalidColor(CssColorParseError<'a>),
    /// Invalid shadow type (inset/outset)
    InvalidShadowType(&'a str),
}

/// Error type for background parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssBackgroundParseError<'a> {
    /// Invalid background color
    InvalidBackgroundColor(CssColorParseError<'a>),
    /// Invalid background position
    InvalidBackgroundPosition(&'a str),
    /// Invalid background size
    InvalidBackgroundSize(&'a str),
    /// Invalid background repeat
    InvalidBackgroundRepeat(&'a str),
    /// Invalid gradient syntax
    InvalidGradient(&'a str),
    /// Invalid image URL
    InvalidImageUrl(&'a str),
}

/// Error type for font parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssFontParseError<'a> {
    /// Invalid font family name
    InvalidFontFamily(&'a str),
    /// Invalid font size
    InvalidFontSize(CssPixelValueParseError<'a>),
    /// Invalid font weight
    InvalidFontWeight(&'a str),
    /// Invalid font style
    InvalidFontStyle(&'a str),
}

/// Error type for transform parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssTransformParseError<'a> {
    /// Invalid transform function
    InvalidTransformFunction(&'a str),
    /// Invalid transform arguments
    InvalidTransformArguments(&'a str),
    /// Invalid matrix values
    InvalidMatrixValues(&'a str),
    /// Invalid angle in rotate function
    InvalidRotateAngle(CssAngleValueParseError<'a>),
}

/// Error type for filter parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssStyleFilterParseError<'a> {
    /// Invalid filter function
    InvalidFilterFunction(&'a str),
    /// Invalid blur amount
    InvalidBlurAmount(CssPixelValueParseError<'a>),
    /// Invalid brightness value
    InvalidBrightnessValue(&'a str),
    /// Invalid contrast value
    InvalidContrastValue(&'a str),
    /// Invalid hue rotation
    InvalidHueRotation(CssAngleValueParseError<'a>),
    /// Invalid saturation value
    InvalidSaturationValue(&'a str),
}

/// Error type for border radius parsing
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssStyleBorderRadiusParseError<'a> {
    /// Invalid border radius value
    InvalidBorderRadius(CssPixelValueParseError<'a>),
    /// Too many values provided
    TooManyValues(&'a str),
    /// Invalid syntax
    InvalidSyntax(&'a str),
}

// Macro to implement From conversions for sub-errors
macro_rules! impl_from {
    ($error_type:ident, $sub_error:ident, $variant:ident) => {
        impl<'a> From<$sub_error<'a>> for $error_type<'a> {
            fn from(err: $sub_error<'a>) -> Self {
                $error_type::$variant(err)
            }
        }
    };
}

impl_from!(CssParsingError, CssPixelValueParseError, PixelValue);
impl_from!(CssParsingError, CssColorParseError, Color);
impl_from!(CssParsingError, CssAngleValueParseError, Angle);
impl_from!(CssParsingError, CssDirectionParseError, Direction);
impl_from!(CssParsingError, CssBorderParseError, Border);
impl_from!(CssParsingError, CssShadowParseError, Shadow);
impl_from!(CssParsingError, CssBackgroundParseError, Background);
impl_from!(CssParsingError, CssFontParseError, Font);
impl_from!(CssParsingError, CssTransformParseError, Transform);
impl_from!(CssParsingError, CssStyleFilterParseError, Filter);
impl_from!(
    CssParsingError,
    CssStyleBorderRadiusParseError,
    BorderRadius
);

impl_from!(
    CssBorderParseError,
    CssPixelValueParseError,
    InvalidBorderWidth
);
impl_from!(CssBorderParseError, CssColorParseError, InvalidBorderColor);
impl_from!(CssShadowParseError, CssPixelValueParseError, InvalidOffset);
impl_from!(CssShadowParseError, CssColorParseError, InvalidColor);
impl_from!(
    CssBackgroundParseError,
    CssColorParseError,
    InvalidBackgroundColor
);
impl_from!(CssFontParseError, CssPixelValueParseError, InvalidFontSize);
impl_from!(
    CssTransformParseError,
    CssAngleValueParseError,
    InvalidRotateAngle
);
impl_from!(
    CssStyleFilterParseError,
    CssPixelValueParseError,
    InvalidBlurAmount
);
impl_from!(
    CssStyleFilterParseError,
    CssAngleValueParseError,
    InvalidHueRotation
);
impl_from!(
    CssStyleBorderRadiusParseError,
    CssPixelValueParseError,
    InvalidBorderRadius
);
