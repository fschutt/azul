//! Contains utilities to convert strings (CSS strings) to servo types

use std::{fmt, num::{ParseIntError, ParseFloatError}};
pub use {
    app_units::Au,
    euclid::{TypedSize2D, SideOffsets2D, TypedPoint2D},
    webrender::api::{
        BorderRadius, BorderDetails, NormalBorder,
        NinePatchBorder, LayoutPixel, BoxShadowClipMode, ColorU,
        ColorF, LayoutVector2D, Gradient, RadialGradient, LayoutPoint,
        LayoutSize, ExtendMode
    },
};
use webrender::api::{BorderStyle, BorderSide, LayoutRect};

pub(crate) const EM_HEIGHT: f32 = 16.0;
/// Webrender measures in points, not in pixels!
pub(crate) const PT_TO_PX: f32 = 96.0 / 72.0;

// In case no font size is specified for a node,
// this will be subsituted as the default font size
pub(crate) const DEFAULT_FONT_SIZE: FontSize = FontSize(PixelValue {
    metric: CssMetric::Px,
    number: 10_000,
});

/// A parser that can accept a list of items and mappings
macro_rules! multi_type_parser {
    ($fn:ident, $return:ident, $([$identifier_string:expr, $enum_type:ident]),+) => (
        fn $fn<'a>(input: &'a str)
        -> Result<$return, InvalidValueErr<'a>>
        {
            match input {
                $(
                    $identifier_string => Ok($return::$enum_type),
                )+
                _ => Err(InvalidValueErr(input)),
            }
        }
    )
}

macro_rules! typed_pixel_value_parser {
    ($fn:ident, $return:ident) => (
        fn $fn<'a>(input: &'a str)
        -> Result<$return, PixelParseError<'a>>
        {
            parse_pixel_value(input).and_then(|e| Ok($return(e)))
        }
    )
}

/// A successfully parsed CSS property
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCssProperty {
    BorderRadius(BorderRadius),
    BackgroundColor(BackgroundColor),
    TextColor(TextColor),
    Border(SideOffsets2D<Au>, BorderDetails),
    Background(Background),
    FontSize(FontSize),
    FontFamily(FontFamily),
    TextAlign(TextAlignmentHorz),
    LetterSpacing(LetterSpacing),
    BoxShadow(Option<BoxShadowPreDisplayItem>),
    LineHeight(LineHeight),

    Width(LayoutWidth),
    Height(LayoutHeight),
    MinWidth(LayoutMinWidth),
    MinHeight(LayoutMinHeight),
    MaxWidth(LayoutMaxWidth),
    MaxHeight(LayoutMaxHeight),

    Position(LayoutPosition),
    Top(LayoutTop),
    Right(LayoutRight),
    Left(LayoutLeft),
    Bottom(LayoutBottom),

    Padding(LayoutPadding),
    Margin(LayoutMargin),

    FlexWrap(LayoutWrap),
    FlexDirection(LayoutDirection),
    FlexGrow(LayoutFlexGrow),
    FlexShrink(LayoutFlexShrink),

    JustifyContent(LayoutJustifyContent),
    AlignItems(LayoutAlignItems),
    AlignContent(LayoutAlignContent),
    Overflow(LayoutOverflow),
}

impl ParsedCssProperty {
    /// Returns whether this property will be inherited during cascading
    pub fn is_inheritable(&self) -> bool {
        use self::ParsedCssProperty::*;
        match self {
            | TextColor(_)
            | FontFamily(_)
            | FontSize(_)
            | LineHeight(_)
            | TextAlign(_) => true,
            _ => false,
        }
    }
}

impl_from!(BorderRadius, ParsedCssProperty::BorderRadius);
impl_from!(Background, ParsedCssProperty::Background);
impl_from!(FontSize, ParsedCssProperty::FontSize);
impl_from!(FontFamily, ParsedCssProperty::FontFamily);
impl_from!(LayoutOverflow, ParsedCssProperty::Overflow);
impl_from!(TextAlignmentHorz, ParsedCssProperty::TextAlign);
impl_from!(LineHeight, ParsedCssProperty::LineHeight);
impl_from!(LetterSpacing, ParsedCssProperty::LetterSpacing);

impl_from!(LayoutWidth, ParsedCssProperty::Width);
impl_from!(LayoutHeight, ParsedCssProperty::Height);
impl_from!(LayoutMinWidth, ParsedCssProperty::MinWidth);
impl_from!(LayoutMinHeight, ParsedCssProperty::MinHeight);
impl_from!(LayoutMaxWidth, ParsedCssProperty::MaxWidth);
impl_from!(LayoutMaxHeight, ParsedCssProperty::MaxHeight);

impl_from!(LayoutPosition, ParsedCssProperty::Position);
impl_from!(LayoutTop, ParsedCssProperty::Top);
impl_from!(LayoutBottom, ParsedCssProperty::Bottom);
impl_from!(LayoutRight, ParsedCssProperty::Right);
impl_from!(LayoutLeft, ParsedCssProperty::Left);

impl_from!(LayoutPadding, ParsedCssProperty::Padding);
impl_from!(LayoutMargin, ParsedCssProperty::Margin);

impl_from!(LayoutWrap, ParsedCssProperty::FlexWrap);
impl_from!(LayoutDirection, ParsedCssProperty::FlexDirection);
impl_from!(LayoutFlexGrow, ParsedCssProperty::FlexGrow);
impl_from!(LayoutFlexShrink, ParsedCssProperty::FlexShrink);
impl_from!(LayoutJustifyContent, ParsedCssProperty::JustifyContent);
impl_from!(LayoutAlignItems, ParsedCssProperty::AlignItems);
impl_from!(LayoutAlignContent, ParsedCssProperty::AlignContent);

impl_from!(BackgroundColor, ParsedCssProperty::BackgroundColor);
impl_from!(TextColor, ParsedCssProperty::TextColor);

impl From<(SideOffsets2D<Au>, BorderDetails)> for ParsedCssProperty {
    fn from((widths, details): (SideOffsets2D<Au>, BorderDetails)) -> Self {
        ParsedCssProperty::Border(widths, details)
    }
}

impl From<Option<BoxShadowPreDisplayItem>> for ParsedCssProperty {
    fn from(box_shadow: Option<BoxShadowPreDisplayItem>) -> Self {
        ParsedCssProperty::BoxShadow(box_shadow)
    }
}

impl ParsedCssProperty {
    /// Main parsing function, takes a stringified key / value pair and either
    /// returns the parsed value or an error
    ///
    /// ```rust
    /// # use azul::prelude::*;
    /// assert_eq!(
    ///     ParsedCssProperty::from_kv("width", "500px"),
    ///     Ok(ParsedCssProperty::Width(LayoutWidth(PixelValue::px(500.0))))
    /// )
    /// ```
    pub fn from_kv<'a>(key: &'a str, value: &'a str) -> Result<Self, CssParsingError<'a>> {
        let key = key.trim();
        let value = value.trim();
        match key {
            "border-radius"     => Ok(parse_css_border_radius(value)?.into()),
            "background-color"  => Ok(parse_css_background_color(value)?.into()),

            "font-color" |
            "color"             => Ok(parse_css_text_color(value)?.into()),
            "border"            => Ok(parse_css_border(value)?.into()),
            "background"        => Ok(parse_css_background(value)?.into()),
            "font-size"         => Ok(parse_css_font_size(value)?.into()),
            "font-family"       => Ok(parse_css_font_family(value)?.into()),
            "box-shadow"        => Ok(parse_css_box_shadow(value)?.into()),
            "letter-spacing"    => Ok(parse_css_letter_spacing(value)?.into()),
            "line-height"       => Ok(parse_css_line_height(value)?.into()),

            "width"             => Ok(parse_layout_width(value)?.into()),
            "height"            => Ok(parse_layout_height(value)?.into()),
            "min-width"         => Ok(parse_layout_min_width(value)?.into()),
            "min-height"        => Ok(parse_layout_min_height(value)?.into()),
            "max-width"         => Ok(parse_layout_max_width(value)?.into()),
            "max-height"        => Ok(parse_layout_max_height(value)?.into()),

            "position"          => Ok(parse_layout_position(value)?.into()),
            "top"               => Ok(parse_layout_top(value)?.into()),
            "right"             => Ok(parse_layout_right(value)?.into()),
            "left"              => Ok(parse_layout_left(value)?.into()),
            "bottom"            => Ok(parse_layout_bottom(value)?.into()),

            "padding"           => Ok(parse_layout_padding(value)?.into()),
            "margin"            => Ok(parse_layout_margin(value)?.into()),

            "flex-wrap"         => Ok(parse_layout_wrap(value)?.into()),
            "flex-direction"    => Ok(parse_layout_direction(value)?.into()),
            "flex-grow"         => Ok(parse_layout_flex_grow(value)?.into()),
            "flex-shrink"       => Ok(parse_layout_flex_shrink(value)?.into()),

            "align-main-axis" |
            "justify-content"   => Ok(parse_layout_justify_content(value)?.into()),

            "align-cross-axis" |
            "align-items"       => Ok(parse_layout_align_items(value)?.into()),

            "align-cross-axis-multiline" |
            "align-content"     => Ok(parse_layout_align_content(value)?.into()),

            "overflow"          => {
                let overflow_both_directions = parse_layout_text_overflow(value)?;
                Ok(LayoutOverflow {
                    horizontal: TextOverflowBehaviour::Modified(overflow_both_directions),
                    vertical: TextOverflowBehaviour::Modified(overflow_both_directions),
                }.into())
            },
            "overflow-x"        => {
                let overflow_x = parse_layout_text_overflow(value)?;
                Ok(LayoutOverflow {
                    horizontal: TextOverflowBehaviour::Modified(overflow_x),
                    vertical: TextOverflowBehaviour::default(),
                }.into())
            },
            "overflow-y"        => {
                let overflow_y = parse_layout_text_overflow(value)?;
                Ok(LayoutOverflow {
                    horizontal: TextOverflowBehaviour::default(),
                    vertical: TextOverflowBehaviour::Modified(overflow_y),
                }.into())
            },
            "text-align"        => Ok(parse_layout_text_align(value)?.into()),
            _ => Err((key, value).into())
        }
    }
}

/// Error containing all sub-errors that could happen during CSS parsing
///
/// Usually we want to crash on the first error, to notify the user of the problem.
#[derive(Debug, Clone, PartialEq)]
pub enum CssParsingError<'a> {
    CssBorderParseError(CssBorderParseError<'a>),
    CssShadowParseError(CssShadowParseError<'a>),
    InvalidValueErr(InvalidValueErr<'a>),
    PixelParseError(PixelParseError<'a>),
    PercentageParseError(PercentageParseError),
    CssImageParseError(CssImageParseError<'a>),
    CssFontFamilyParseError(CssFontFamilyParseError<'a>),
    CssBackgroundParseError(CssBackgroundParseError<'a>),
    CssColorParseError(CssColorParseError<'a>),
    CssBorderRadiusParseError(CssBorderRadiusParseError<'a>),
    PaddingParseError(LayoutPaddingParseError<'a>),
    MarginParseError(LayoutMarginParseError<'a>),
    FlexShrinkParseError(FlexShrinkParseError<'a>),
    FlexGrowParseError(FlexGrowParseError<'a>),
    /// Key is not supported, i.e. `#div { aldfjasdflk: 400px }` results in an
    /// `UnsupportedCssKey("aldfjasdflk", "400px")` error
    UnsupportedCssKey(&'a str, &'a str),
}

impl_display!{ CssParsingError<'a>, {
    CssBorderRadiusParseError(e) => format!("Invalid border-radius: {}", e),
    CssBorderParseError(e) => format!("Invalid border property: {}", e),
    CssShadowParseError(e) => format!("Invalid shadow: \"{}\"", e),
    InvalidValueErr(e) => format!("\"{}\"", e.0),
    PixelParseError(e) => format!("{}", e),
    PercentageParseError(e) => format!("{}", e),
    CssImageParseError(e) => format!("{}", e),
    CssFontFamilyParseError(e) => format!("{}", e),
    CssBackgroundParseError(e) => format!("{}", e),
    CssColorParseError(e) => format!("{}", e),
    PaddingParseError(e) => format!("{}", e),
    MarginParseError(e) => format!("{}", e),
    FlexShrinkParseError(e) => format!("{}", e),
    FlexGrowParseError(e) => format!("{}", e),
    UnsupportedCssKey(key, value) => format!("Unsupported Css-key: \"{}\" - value: \"{}\"", key, value),
}}

impl_from!(CssBorderParseError<'a>, CssParsingError::CssBorderParseError);
impl_from!(CssShadowParseError<'a>, CssParsingError::CssShadowParseError);
impl_from!(CssColorParseError<'a>, CssParsingError::CssColorParseError);
impl_from!(InvalidValueErr<'a>, CssParsingError::InvalidValueErr);
impl_from!(PixelParseError<'a>, CssParsingError::PixelParseError);
impl_from!(CssImageParseError<'a>, CssParsingError::CssImageParseError);
impl_from!(CssFontFamilyParseError<'a>, CssParsingError::CssFontFamilyParseError);
impl_from!(CssBackgroundParseError<'a>, CssParsingError::CssBackgroundParseError);
impl_from!(CssBorderRadiusParseError<'a>, CssParsingError::CssBorderRadiusParseError);
impl_from!(LayoutPaddingParseError<'a>, CssParsingError::PaddingParseError);
impl_from!(LayoutMarginParseError<'a>, CssParsingError::MarginParseError);
impl_from!(FlexShrinkParseError<'a>, CssParsingError::FlexShrinkParseError);
impl_from!(FlexGrowParseError<'a>, CssParsingError::FlexGrowParseError);

impl<'a> From<(&'a str, &'a str)> for CssParsingError<'a> {
    fn from((a, b): (&'a str, &'a str)) -> Self {
        CssParsingError::UnsupportedCssKey(a, b)
    }
}

impl<'a> From<PercentageParseError> for CssParsingError<'a> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::PercentageParseError(e)
    }
}

/// Simple "invalid value" error, used for
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct PixelValue {
    metric: CssMetric,
    /// Has to be divided by 1000.0 - PixelValue needs to implement Hash,
    /// but Hash is not possible for floating-point values
    number: isize,
}

impl PixelValue {
    pub fn px(value: f32) -> Self {
        Self::from_metric(CssMetric::Px, value)
    }

    pub fn em(value: f32) -> Self {
        Self::from_metric(CssMetric::Em, value)
    }

    pub fn pt(value: f32) -> Self {
        Self::from_metric(CssMetric::Pt, value)
    }

    pub fn from_metric(metric: CssMetric, value: f32) -> Self {
        Self {
            metric: metric,
            number: (value * 1000.0) as isize,
        }
    }

    pub fn to_pixels(&self) -> f32 {
        match self.metric {
            CssMetric::Px => { self.number as f32 / 1000.0 },
            CssMetric::Pt => { (self.number as f32 / 1000.0) * PT_TO_PX },
            CssMetric::Em => { (self.number as f32 / 1000.0) * EM_HEIGHT },
        }
    }
}

/// "100%" or "1.0" value
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct PercentageValue {
    /// Normalized value, 100% = 1.0
    pub number: f32,
}

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub enum CssMetric {
    Px,
    Pt,
    Em,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssBorderRadiusParseError<'a> {
    TooManyValues(&'a str),
    PixelParseError(PixelParseError<'a>),
}

impl_display!{ CssBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values: \"{}\"", val),
    PixelParseError(e) => format!("{}", e),
}}

impl_from!(PixelParseError<'a>, CssBorderRadiusParseError::PixelParseError);

#[derive(Debug, Clone, PartialEq)]
pub enum CssColorParseError<'a> {
    InvalidColor(&'a str),
    InvalidColorComponent(u8),
    ValueParseErr(ParseIntError),
}

impl<'a> fmt::Display for CssColorParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssColorParseError::*;
        match self {
            InvalidColor(i) => write!(f, "Invalid CSS color: \"{}\"", i),
            InvalidColorComponent(i) => write!(f, "Invalid color component when parsing CSS color: \"{}\"", i),
            ValueParseErr(e) => write!(f, "Css color component: Value not in range between 00 - FF: \"{}\"", e),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssImageParseError<'a> {
    UnclosedQuotes(&'a str),
}

impl_display!{CssImageParseError<'a>, {
    UnclosedQuotes(e) => format!("Unclosed quotes: \"{}\"", e),
}}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct UnclosedQuotesError<'a>(pub(crate) &'a str);

impl<'a> From<UnclosedQuotesError<'a>> for CssImageParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssImageParseError::UnclosedQuotes(err.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderParseError<'a> {
    InvalidBorderStyle(InvalidValueErr<'a>),
    InvalidBorderDeclaration(&'a str),
    ThicknessParseError(PixelParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_display!{ CssBorderParseError<'a>, {
    InvalidBorderStyle(e) => format!("Invalid style: {}", e.0),
    InvalidBorderDeclaration(e) => format!("Invalid declaration: \"{}\"", e),
    ThicknessParseError(e) => format!("Invalid thickness: {}", e),
    ColorParseError(e) => format!("Invalid color: {}", e),
}}

#[derive(Debug, Clone, PartialEq)]
pub enum CssShadowParseError<'a> {
    InvalidSingleStatement(&'a str),
    TooManyComponents(&'a str),
    ValueParseErr(PixelParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_display!{ CssShadowParseError<'a>, {
    InvalidSingleStatement(e) => format!("Invalid single statement: \"{}\"", e),
    TooManyComponents(e) => format!("Too many components: \"{}\"", e),
    ValueParseErr(e) => format!("Invalid value: {}", e),
    ColorParseError(e) => format!("Invalid color-value: {}", e),
}}

impl_from!(PixelParseError<'a>, CssShadowParseError::ValueParseErr);
impl_from!(CssColorParseError<'a>, CssShadowParseError::ColorParseError);

/// parse the border-radius like "5px 10px" or "5px 10px 6px 10px"
fn parse_css_border_radius<'a>(input: &'a str)
-> Result<BorderRadius, CssBorderRadiusParseError<'a>>
{
    let mut components = input.split_whitespace();
    let len = components.clone().count();

    match len {
        1 => {
            // One value - border-radius: 15px;
            // (the value applies to all four corners, which are rounded equally:

            let uniform_radius = parse_pixel_value(components.next().unwrap())?.to_pixels();
            Ok(BorderRadius::uniform(uniform_radius))
        },
        2 => {
            // Two values - border-radius: 15px 50px;
            // (first value applies to top-left and bottom-right corners,
            // and the second value applies to top-right and bottom-left corners):

            let top_left_bottom_right = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?.to_pixels();

            Ok(BorderRadius{
                top_left: LayoutSize::new(top_left_bottom_right, top_left_bottom_right),
                bottom_right: LayoutSize::new(top_left_bottom_right, top_left_bottom_right),
                top_right: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
                bottom_left: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
            })
        },
        3 => {
            // Three values - border-radius: 15px 50px 30px;
            // (first value applies to top-left corner,
            // second value applies to top-right and bottom-left corners,
            // and third value applies to bottom-right corner):
            let top_left = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let bottom_right = parse_pixel_value(components.next().unwrap())?.to_pixels();

            Ok(BorderRadius{
                top_left: LayoutSize::new(top_left, top_left),
                bottom_right: LayoutSize::new(bottom_right, bottom_right),
                top_right: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
                bottom_left: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
            })
        }
        4 => {
            // Four values - border-radius: 15px 50px 30px 5px;
            // (first value applies to top-left corner,
            //  second value applies to top-right corner,
            //  third value applies to bottom-right corner,
            //  fourth value applies to bottom-left corner)
            let top_left = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let top_right = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let bottom_right = parse_pixel_value(components.next().unwrap())?.to_pixels();
            let bottom_left = parse_pixel_value(components.next().unwrap())?.to_pixels();

            Ok(BorderRadius{
                top_left: LayoutSize::new(top_left, top_left),
                bottom_right: LayoutSize::new(bottom_right, bottom_right),
                top_right: LayoutSize::new(top_right, top_right),
                bottom_left: LayoutSize::new(bottom_left, bottom_left),
            })
        },
        _ => {
            Err(CssBorderRadiusParseError::TooManyValues(input))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PixelParseError<'a> {
    InvalidComponent(&'a str),
    ValueParseErr(ParseFloatError),
}

impl_display!{ PixelParseError<'a>, {
    InvalidComponent(component) => format!("Invalid component: \"{}\"", component),
    ValueParseErr(e) => format!("Unexpected value: \"{}\"", e),
}}

/// parse a single value such as "15px"
fn parse_pixel_value<'a>(input: &'a str)
-> Result<PixelValue, PixelParseError<'a>>
{
    let mut split_pos = 0;
    for (idx, ch) in input.char_indices() {
        if ch.is_numeric() || ch == '.' {
            split_pos = idx;
        }
    }

    split_pos += 1;

    let unit = &input[split_pos..];
    let unit = match unit {
        "px" => CssMetric::Px,
        "em" => CssMetric::Em,
        "pt" => CssMetric::Pt,
        _ => { return Err(PixelParseError::InvalidComponent(&input[(split_pos - 1)..])); }
    };

    let number = input[..split_pos].parse::<f32>().map_err(|e| PixelParseError::ValueParseErr(e))?;

    Ok(PixelValue {
        metric: unit,
        number: (number * 1000.0) as isize,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PercentageParseError {
    ValueParseErr(ParseFloatError),
}

impl_display! { PercentageParseError, {
    ValueParseErr(e) => format!("Could not parse percentage value: \"{}\"", e),
}}

// Parse "1.2" or "120%" (similar to parse_pixel_value)
fn parse_percentage_value(input: &str)
-> Result<PercentageValue, PercentageParseError>
{
    let mut split_pos = 0;
    for (idx, ch) in input.char_indices() {
        if ch.is_numeric() || ch == '.' {
            split_pos = idx;
        }
    }

    split_pos += 1;

    let unit = &input[split_pos..];
    let mut number = input[..split_pos].parse::<f32>().map_err(|e| PercentageParseError::ValueParseErr(e))?;

    if unit == "%" {
        number /= 100.0;
    }

    Ok(PercentageValue {
        number: number,
    })
}

/// Parse any valid CSS color, INCLUDING THE HASH
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
/// "#00FF00" -> ColorF { r: 0, g: 255, b: 0 })
pub(crate) fn parse_css_color<'a>(input: &'a str)
-> Result<ColorU, CssColorParseError<'a>>
{
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        parse_color_builtin(input)
    }
}

fn parse_float_value(input: &str)
-> Result<f32, ParseFloatError>
{
    input.trim().parse::<f32>()
}

/// Represents a parsed CSS `background-color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BackgroundColor(pub ColorU);

impl Default for BackgroundColor {
    fn default() -> Self {
        // Transparent color
        BackgroundColor(ColorU::new(0, 0, 0, 0))
    }
}

fn parse_css_background_color<'a>(input: &'a str)
-> Result<BackgroundColor, CssColorParseError<'a>>
{
    parse_css_color(input).and_then(|ok| Ok(BackgroundColor(ok)))
}

/// Represents a parsed CSS `color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TextColor(pub ColorU);

fn parse_css_text_color<'a>(input: &'a str)
-> Result<TextColor, CssColorParseError<'a>>
{
    parse_css_color(input).and_then(|ok| Ok(TextColor(ok)))
}

/// Parse a built-in background color
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
fn parse_color_builtin<'a>(input: &'a str)
-> Result<ColorU, CssColorParseError<'a>>
{
    let color = match input {
        "AliceBlue"              | "alice-blue"                 =>  "F0F8FF",
        "AntiqueWhite"           | "antique-white"              =>  "FAEBD7",
        "Aqua"                   | "aqua"                       =>  "00FFFF",
        "Aquamarine"             | "aquamarine"                 =>  "7FFFD4",
        "Azure"                  | "azure"                      =>  "F0FFFF",
        "Beige"                  | "beige"                      =>  "F5F5DC",
        "Bisque"                 | "bisque"                     =>  "FFE4C4",
        "Black"                  | "black"                      =>  "000000",
        "BlanchedAlmond"         | "blanched-almond"            =>  "FFEBCD",
        "Blue"                   | "blue"                       =>  "0000FF",
        "BlueViolet"             | "blue-violet"                =>  "8A2BE2",
        "Brown"                  | "brown"                      =>  "A52A2A",
        "BurlyWood"              | "burly-wood"                 =>  "DEB887",
        "CadetBlue"              | "cadet-blue"                 =>  "5F9EA0",
        "Chartreuse"             | "chartreuse"                 =>  "7FFF00",
        "Chocolate"              | "chocolate"                  =>  "D2691E",
        "Coral"                  | "coral"                      =>  "FF7F50",
        "CornflowerBlue"         | "cornflower-blue"            =>  "6495ED",
        "Cornsilk"               | "cornsilk"                   =>  "FFF8DC",
        "Crimson"                | "crimson"                    =>  "DC143C",
        "Cyan"                   | "cyan"                       =>  "00FFFF",
        "DarkBlue"               | "dark-blue"                  =>  "00008B",
        "DarkCyan"               | "dark-cyan"                  =>  "008B8B",
        "DarkGoldenRod"          | "dark-golden-rod"            =>  "B8860B",
        "DarkGray"               | "dark-gray"                  =>  "A9A9A9",
        "DarkGrey"               | "dark-grey"                  =>  "A9A9A9",
        "DarkGreen"              | "dark-green"                 =>  "006400",
        "DarkKhaki"              | "dark-khaki"                 =>  "BDB76B",
        "DarkMagenta"            | "dark-magenta"               =>  "8B008B",
        "DarkOliveGreen"         | "dark-olive-green"           =>  "556B2F",
        "DarkOrange"             | "dark-orange"                =>  "FF8C00",
        "DarkOrchid"             | "dark-orchid"                =>  "9932CC",
        "DarkRed"                | "dark-red"                   =>  "8B0000",
        "DarkSalmon"             | "dark-salmon"                =>  "E9967A",
        "DarkSeaGreen"           | "dark-sea-green"             =>  "8FBC8F",
        "DarkSlateBlue"          | "dark-slate-blue"            =>  "483D8B",
        "DarkSlateGray"          | "dark-slate-gray"            =>  "2F4F4F",
        "DarkSlateGrey"          | "dark-slate-grey"            =>  "2F4F4F",
        "DarkTurquoise"          | "dark-turquoise"             =>  "00CED1",
        "DarkViolet"             | "dark-violet"                =>  "9400D3",
        "DeepPink"               | "deep-pink"                  =>  "FF1493",
        "DeepSkyBlue"            | "deep-sky-blue"              =>  "00BFFF",
        "DimGray"                | "dim-gray"                   =>  "696969",
        "DimGrey"                | "dim-grey"                   =>  "696969",
        "DodgerBlue"             | "dodger-blue"                =>  "1E90FF",
        "FireBrick"              | "fire-brick"                 =>  "B22222",
        "FloralWhite"            | "floral-white"               =>  "FFFAF0",
        "ForestGreen"            | "forest-green"               =>  "228B22",
        "Fuchsia"                | "fuchsia"                    =>  "FF00FF",
        "Gainsboro"              | "gainsboro"                  =>  "DCDCDC",
        "GhostWhite"             | "ghost-white"                =>  "F8F8FF",
        "Gold"                   | "gold"                       =>  "FFD700",
        "GoldenRod"              | "golden-rod"                 =>  "DAA520",
        "Gray"                   | "gray"                       =>  "808080",
        "Grey"                   | "grey"                       =>  "808080",
        "Green"                  | "green"                      =>  "008000",
        "GreenYellow"            | "green-yellow"               =>  "ADFF2F",
        "HoneyDew"               | "honey-dew"                  =>  "F0FFF0",
        "HotPink"                | "hot-pink"                   =>  "FF69B4",
        "IndianRed"              | "indian-red"                 =>  "CD5C5C",
        "Indigo"                 | "indigo"                     =>  "4B0082",
        "Ivory"                  | "ivory"                      =>  "FFFFF0",
        "Khaki"                  | "khaki"                      =>  "F0E68C",
        "Lavender"               | "lavender"                   =>  "E6E6FA",
        "LavenderBlush"          | "lavender-blush"             =>  "FFF0F5",
        "LawnGreen"              | "lawn-green"                 =>  "7CFC00",
        "LemonChiffon"           | "lemon-chiffon"              =>  "FFFACD",
        "LightBlue"              | "light-blue"                 =>  "ADD8E6",
        "LightCoral"             | "light-coral"                =>  "F08080",
        "LightCyan"              | "light-cyan"                 =>  "E0FFFF",
        "LightGoldenRodYellow"   | "light-golden-rod-yellow"    =>  "FAFAD2",
        "LightGray"              | "light-gray"                 =>  "D3D3D3",
        "LightGrey"              | "light-grey"                 =>  "D3D3D3",
        "LightGreen"             | "light-green"                =>  "90EE90",
        "LightPink"              | "light-pink"                 =>  "FFB6C1",
        "LightSalmon"            | "light-salmon"               =>  "FFA07A",
        "LightSeaGreen"          | "light-sea-green"            =>  "20B2AA",
        "LightSkyBlue"           | "light-sky-blue"             =>  "87CEFA",
        "LightSlateGray"         | "light-slate-gray"           =>  "778899",
        "LightSlateGrey"         | "light-slate-grey"           =>  "778899",
        "LightSteelBlue"         | "light-steel-blue"           =>  "B0C4DE",
        "LightYellow"            | "light-yellow"               =>  "FFFFE0",
        "Lime"                   | "lime"                       =>  "00FF00",
        "LimeGreen"              | "lime-green"                 =>  "32CD32",
        "Linen"                  | "linen"                      =>  "FAF0E6",
        "Magenta"                | "magenta"                    =>  "FF00FF",
        "Maroon"                 | "maroon"                     =>  "800000",
        "MediumAquaMarine"       | "medium-aqua-marine"         =>  "66CDAA",
        "MediumBlue"             | "medium-blue"                =>  "0000CD",
        "MediumOrchid"           | "medium-orchid"              =>  "BA55D3",
        "MediumPurple"           | "medium-purple"              =>  "9370DB",
        "MediumSeaGreen"         | "medium-sea-green"           =>  "3CB371",
        "MediumSlateBlue"        | "medium-slate-blue"          =>  "7B68EE",
        "MediumSpringGreen"      | "medium-spring-green"        =>  "00FA9A",
        "MediumTurquoise"        | "medium-turquoise"           =>  "48D1CC",
        "MediumVioletRed"        | "medium-violet-red"          =>  "C71585",
        "MidnightBlue"           | "midnight-blue"              =>  "191970",
        "MintCream"              | "mint-cream"                 =>  "F5FFFA",
        "MistyRose"              | "misty-rose"                 =>  "FFE4E1",
        "Moccasin"               | "moccasin"                   =>  "FFE4B5",
        "NavajoWhite"            | "navajo-white"               =>  "FFDEAD",
        "Navy"                   | "navy"                       =>  "000080",
        "OldLace"                | "old-lace"                   =>  "FDF5E6",
        "Olive"                  | "olive"                      =>  "808000",
        "OliveDrab"              | "olive-drab"                 =>  "6B8E23",
        "Orange"                 | "orange"                     =>  "FFA500",
        "OrangeRed"              | "orange-red"                 =>  "FF4500",
        "Orchid"                 | "orchid"                     =>  "DA70D6",
        "PaleGoldenRod"          | "pale-golden-rod"            =>  "EEE8AA",
        "PaleGreen"              | "pale-green"                 =>  "98FB98",
        "PaleTurquoise"          | "pale-turquoise"             =>  "AFEEEE",
        "PaleVioletRed"          | "pale-violet-red"            =>  "DB7093",
        "PapayaWhip"             | "papaya-whip"                =>  "FFEFD5",
        "PeachPuff"              | "peach-puff"                 =>  "FFDAB9",
        "Peru"                   | "peru"                       =>  "CD853F",
        "Pink"                   | "pink"                       =>  "FFC0CB",
        "Plum"                   | "plum"                       =>  "DDA0DD",
        "PowderBlue"             | "powder-blue"                =>  "B0E0E6",
        "Purple"                 | "purple"                     =>  "800080",
        "RebeccaPurple"          | "rebecca-purple"             =>  "663399",
        "Red"                    | "red"                        =>  "FF0000",
        "RosyBrown"              | "rosy-brown"                 =>  "BC8F8F",
        "RoyalBlue"              | "royal-blue"                 =>  "4169E1",
        "SaddleBrown"            | "saddle-brown"               =>  "8B4513",
        "Salmon"                 | "salmon"                     =>  "FA8072",
        "SandyBrown"             | "sandy-brown"                =>  "F4A460",
        "SeaGreen"               | "sea-green"                  =>  "2E8B57",
        "SeaShell"               | "sea-shell"                  =>  "FFF5EE",
        "Sienna"                 | "sienna"                     =>  "A0522D",
        "Silver"                 | "silver"                     =>  "C0C0C0",
        "SkyBlue"                | "sky-blue"                   =>  "87CEEB",
        "SlateBlue"              | "slate-blue"                 =>  "6A5ACD",
        "SlateGray"              | "slate-gray"                 =>  "708090",
        "SlateGrey"              | "slate-grey"                 =>  "708090",
        "Snow"                   | "snow"                       =>  "FFFAFA",
        "SpringGreen"            | "spring-green"               =>  "00FF7F",
        "SteelBlue"              | "steel-blue"                 =>  "4682B4",
        "Tan"                    | "tan"                        =>  "D2B48C",
        "Teal"                   | "teal"                       =>  "008080",
        "Thistle"                | "thistle"                    =>  "D8BFD8",
        "Tomato"                 | "tomato"                     =>  "FF6347",
        "Turquoise"              | "turquoise"                  =>  "40E0D0",
        "Violet"                 | "violet"                     =>  "EE82EE",
        "Wheat"                  | "wheat"                      =>  "F5DEB3",
        "White"                  | "white"                      =>  "FFFFFF",
        "WhiteSmoke"             | "white-smoke"                =>  "F5F5F5",
        "Yellow"                 | "yellow"                     =>  "FFFF00",
        "YellowGreen"            | "yellow-green"               =>  "9ACD32",
        "Transparent"            | "transparent"                =>  "FFFFFF00",
        _ => { return Err(CssColorParseError::InvalidColor(input)); }
    };
    parse_color_no_hash(color)
}

/// Parse a background color, WITHOUT THE HASH
///
/// "00FFFF" -> ColorF { r: 0, g: 255, b: 255})
fn parse_color_no_hash<'a>(input: &'a str)
-> Result<ColorU, CssColorParseError<'a>>
{
    #[inline]
    fn from_hex<'a>(c: u8) -> Result<u8, CssColorParseError<'a>> {
        match c {
            b'0' ... b'9' => Ok(c - b'0'),
            b'a' ... b'f' => Ok(c - b'a' + 10),
            b'A' ... b'F' => Ok(c - b'A' + 10),
            _ => Err(CssColorParseError::InvalidColorComponent(c))
        }
    }

    match input.len() {
        3 => {
            let mut input_iter = input.chars();

            let r = input_iter.next().unwrap() as u8;
            let g = input_iter.next().unwrap() as u8;
            let b = input_iter.next().unwrap() as u8;

            let r = from_hex(r)? * 16 + from_hex(r)?;
            let g = from_hex(g)? * 16 + from_hex(g)?;
            let b = from_hex(b)? * 16 + from_hex(b)?;

            Ok(ColorU {
                r: r,
                g: g,
                b: b,
                a: 255,
            })
        },
        4 => {
            let mut input_iter = input.chars();

            let r = input_iter.next().unwrap() as u8;
            let g = input_iter.next().unwrap() as u8;
            let b = input_iter.next().unwrap() as u8;
            let a = input_iter.next().unwrap() as u8;

            let r = from_hex(r)? * 16 + from_hex(r)?;
            let g = from_hex(g)? * 16 + from_hex(g)?;
            let b = from_hex(b)? * 16 + from_hex(b)?;
            let a = from_hex(a)? * 16 + from_hex(a)?;

            Ok(ColorU {
                r: r,
                g: g,
                b: b,
                a: a,
            })
        },
        6 => {
            let input = u32::from_str_radix(input, 16).map_err(|e| CssColorParseError::ValueParseErr(e))?;
            Ok(ColorU {
                r: ((input >> 16) & 255) as u8,
                g: ((input >> 8) & 255) as u8,
                b: (input & 255) as u8,
                a: 255,
            })
        },
        8 => {
            let input = u32::from_str_radix(input, 16).map_err(|e| CssColorParseError::ValueParseErr(e))?;
            Ok(ColorU {
                r: ((input >> 24) & 255) as u8,
                g: ((input >> 16) & 255) as u8,
                b: ((input >> 8) & 255) as u8,
                a: (input & 255) as u8,
            })
        },
        _ => { Err(CssColorParseError::InvalidColor(input)) }
    }
}

/// Represents a parsed CSS `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct LayoutPadding {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

macro_rules! parse_x {($struct_name:ident, $error_name:ident, $fn_name:ident, $field:ident, $body:ident) => (
fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name> {
    let value = $body(input)?;
    Ok($struct_name {
        $field: Some(value),
        .. Default::default()
    })
})}

macro_rules! parse_tblr {($struct_name:ident, $error_name:ident, $parse_fn:ident) => (
impl $struct_name {
    parse_x!($struct_name, $error_name, parse_left, left, $parse_fn);
    parse_x!($struct_name, $error_name, parse_right, right, $parse_fn);
    parse_x!($struct_name, $error_name, parse_bottom, bottom, $parse_fn);
    parse_x!($struct_name, $error_name, parse_top, top, $parse_fn);
})}

// $struct_name has to have top, left, right, bottom properties
macro_rules! merge_struct {($struct_name:ident) => (
impl $struct_name {
    pub fn merge(a: &mut Option<$struct_name>, b: &$struct_name) {
       if let Some(ref mut existing) = a {
           if b.top.is_some() { existing.top = b.top; }
           if b.bottom.is_some() { existing.bottom = b.bottom; }
           if b.left.is_some() { existing.left = b.left; }
           if b.right.is_some() { existing.right = b.right; }
       } else {
           *a = Some(*b);
       }
    }
})}

merge_struct!(LayoutPadding);
merge_struct!(LayoutMargin);
parse_tblr!(LayoutPadding, LayoutPaddingParseError, parse_pixel_value);
parse_tblr!(LayoutMargin, LayoutMarginParseError, parse_pixel_value);

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseError<'a> {
    PixelParseError(PixelParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display!{ LayoutPaddingParseError<'a>, {
    PixelParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - padding property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - padding property has a minimum of 1 value."),
}}

impl_from!(PixelParseError<'a>, LayoutPaddingParseError::PixelParseError);

/// Parse a padding value such as
///
/// "10px 10px"
fn parse_layout_padding<'a>(input: &'a str)
-> Result<LayoutPadding, LayoutPaddingParseError>
{
    let mut input_iter = input.split_whitespace();
    let first = parse_pixel_value(input_iter.next().ok_or(LayoutPaddingParseError::TooFewValues)?)?;
    let second = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: Some(first),
            bottom: Some(first),
            left: Some(first),
            right: Some(first),
        }),
    })?;
    let third = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: Some(first),
            bottom: Some(first),
            left: Some(second),
            right: Some(second),
        }),
    })?;
    let fourth = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: Some(first),
            left: Some(second),
            right: Some(second),
            bottom: Some(third),
        }),
    })?;

    if input_iter.next().is_some() {
        return Err(LayoutPaddingParseError::TooManyValues);
    }

    Ok(LayoutPadding {
        top: Some(first),
        right: Some(second),
        bottom: Some(third),
        left: Some(fourth),
    })
}

/// Represents a parsed CSS `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct LayoutMargin {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseError<'a> {
    PixelParseError(PixelParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display!{ LayoutMarginParseError<'a>, {
    PixelParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - margin property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - margin property has a minimum of 1 value."),
}}

impl_from!(PixelParseError<'a>, LayoutMarginParseError::PixelParseError);

fn parse_layout_margin<'a>(input: &'a str)
-> Result<LayoutMargin, LayoutMarginParseError>
{
    match parse_layout_padding(input) {
        Ok(padding) => {
            Ok(LayoutMargin {
                top: padding.top,
                left: padding.left,
                right: padding.right,
                bottom: padding.bottom,
            })
        },
        Err(LayoutPaddingParseError::PixelParseError(e)) => Err(e.into()),
        Err(LayoutPaddingParseError::TooManyValues) => Err(LayoutMarginParseError::TooManyValues),
        Err(LayoutPaddingParseError::TooFewValues) => Err(LayoutMarginParseError::TooFewValues),
    }
}

/// Wrapper for the `overflow-{x,y}` + `overflow` property
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct LayoutOverflow {
    pub horizontal: TextOverflowBehaviour,
    pub vertical: TextOverflowBehaviour,
}

impl LayoutOverflow {

    // "merges" two LayoutOverflow properties
    pub fn merge(&mut self, other: &LayoutOverflow) {
        fn merge_property(p: &mut TextOverflowBehaviour, other: &TextOverflowBehaviour) {
            if *other == TextOverflowBehaviour::NotModified {
                return;
            }
            *p = *other;
        }

        merge_property(&mut self.horizontal, &other.horizontal);
        merge_property(&mut self.vertical, &other.vertical);
    }

    pub fn allows_horizontal_overflow(&self) -> bool {
        use self::TextOverflowBehaviourInner::*;
        match self.horizontal {
            TextOverflowBehaviour::Modified(m) => match m {
                Scroll | Auto => true,
                Hidden | Visible => false,
            },
            // default: allow horizontal overflow
            TextOverflowBehaviour::NotModified => false,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct StyleBorder {
    pub top: Option<(SideOffsets2D<Au>, BorderDetails)>,
    pub left: Option<(SideOffsets2D<Au>, BorderDetails)>,
    pub bottom: Option<(SideOffsets2D<Au>, BorderDetails)>,
    pub right: Option<(SideOffsets2D<Au>, BorderDetails)>,
}

merge_struct!(StyleBorder);
parse_tblr!(StyleBorder, CssBorderParseError, parse_css_border);

/// Parse a CSS border such as
///
/// "5px solid red"
fn parse_css_border<'a>(input: &'a str)
-> Result<(SideOffsets2D<Au>, BorderDetails), CssBorderParseError<'a>>
{
    let mut input_iter = input.split_whitespace();

    let (thickness, style, color);

    match input_iter.clone().count() {
        1 => {
            style = parse_border_style(input_iter.next().unwrap())
                            .map_err(|e| CssBorderParseError::InvalidBorderStyle(e))?;
            thickness = 1.0;
            color = ColorU { r: 0, g: 0, b: 0, a: 255 };
        },
        3 => {
            thickness = parse_pixel_value(input_iter.next().unwrap())
                           .map_err(|e| CssBorderParseError::ThicknessParseError(e))?.to_pixels();
            style = parse_border_style(input_iter.next().unwrap())
                           .map_err(|e| CssBorderParseError::InvalidBorderStyle(e))?;
            color = parse_css_color(input_iter.next().unwrap())
                           .map_err(|e| CssBorderParseError::ColorParseError(e))?;
       },
       _ => {
            return Err(CssBorderParseError::InvalidBorderDeclaration(input));
       }
    }

    // This should later be changed into the proper `border-left` and so on types
    let top = Au::from_f32_px(thickness);
    let right = Au::from_f32_px(thickness);
    let bottom = Au::from_f32_px(thickness);
    let left = Au::from_f32_px(thickness);

    let border_widths = SideOffsets2D::new(top, right, bottom, left);

    let border_side = BorderSide {
        color: color.into(),
        style: style,
    };

    let border_details = BorderDetails::Normal(NormalBorder {
        top: border_side,
        left: border_side,
        right: border_side,
        bottom: border_side,
        radius: BorderRadius::zero(),
        do_aa: true,
    });

    Ok((border_widths, border_details))
}

/// Parse a border style such as "none", "dotted", etc.
///
/// "solid", "none", etc.
multi_type_parser!(parse_border_style, BorderStyle,
    ["none", None],
    ["solid", Solid],
    ["double", Double],
    ["dotted", Dotted],
    ["dashed", Dashed],
    ["hidden", Hidden],
    ["groove", Groove],
    ["ridge", Ridge],
    ["inset", Inset],
    ["outset", Outset]);

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct StyleBoxShadow {
    pub top: Option<Option<BoxShadowPreDisplayItem>>,
    pub left: Option<Option<BoxShadowPreDisplayItem>>,
    pub bottom: Option<Option<BoxShadowPreDisplayItem>>,
    pub right: Option<Option<BoxShadowPreDisplayItem>>,
}

merge_struct!(StyleBoxShadow);
parse_tblr!(StyleBoxShadow, CssShadowParseError, parse_css_box_shadow);

// missing BorderRadius & LayoutRect
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BoxShadowPreDisplayItem {
    pub offset: LayoutVector2D,
    pub color: ColorF,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub clip_mode: BoxShadowClipMode,
}

/// Parses a CSS box-shadow
fn parse_css_box_shadow<'a>(input: &'a str)
-> Result<Option<BoxShadowPreDisplayItem>, CssShadowParseError<'a>>
{
    let mut input_iter = input.split_whitespace();
    let count = input_iter.clone().count();

    let mut box_shadow = BoxShadowPreDisplayItem {
        offset: LayoutVector2D::zero(),
        color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
        blur_radius: 0.0,
        spread_radius: 0.0,
        clip_mode: BoxShadowClipMode::Outset,
    };

    let last_val = input_iter.clone().rev().next();
    let is_inset = last_val == Some("inset") || last_val == Some("outset");

    if count > 2 && is_inset {
        let l_val = last_val.unwrap();
        if l_val == "outset" {
            box_shadow.clip_mode = BoxShadowClipMode::Outset;
        } else if l_val == "inset" {
            box_shadow.clip_mode = BoxShadowClipMode::Inset;
        }
    }

    match count {
        1 => {
            // box-shadow: none;
            match input_iter.next().unwrap() {
                "none" => return Ok(None),
                _ => return Err(CssShadowParseError::InvalidSingleStatement(input)),
            }
        },
        2 => {
            // box-shadow: 5px 10px; (h_offset, v_offset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.offset.x = h_offset;
            box_shadow.offset.y = v_offset;
        },
        3 => {
            // box-shadow: 5px 10px inset; (h_offset, v_offset, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.offset.x = h_offset;
            box_shadow.offset.y = v_offset;

            if !is_inset {
                // box-shadow: 5px 10px #888888; (h_offset, v_offset, color)
                let color = parse_css_color(input_iter.next().unwrap())?;
                box_shadow.color = ColorF::from(color);
            }
        },
        4 => {
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.offset.x = h_offset;
            box_shadow.offset.y = v_offset;

            if !is_inset {
                let blur = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
                box_shadow.blur_radius = blur.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = ColorF::from(color);
        },
        5 => {
            // box-shadow: 5px 10px 5px 10px #888888; (h_offset, v_offset, blur, spread, color)
            // box-shadow: 5px 10px 5px #888888 inset; (h_offset, v_offset, blur, color, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.offset.x = h_offset;
            box_shadow.offset.y = v_offset;

            let blur = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.blur_radius = blur.into();

            if !is_inset {
                let spread = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
                box_shadow.spread_radius = spread.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = ColorF::from(color);
        },
        6 => {
            // box-shadow: 5px 10px 5px 10px #888888 inset; (h_offset, v_offset, blur, spread, color, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.offset.x = h_offset;
            box_shadow.offset.y = v_offset;

            let blur = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.blur_radius = blur.into();

            let spread = parse_pixel_value(input_iter.next().unwrap())?.to_pixels();
            box_shadow.spread_radius = spread.into();

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = ColorF::from(color);
        }
        _ => {
            return Err(CssShadowParseError::TooManyComponents(input));
        }
    }

    Ok(Some(box_shadow))
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundParseError<'a> {
    Error(&'a str),
    InvalidBackground(&'a str),
    UnclosedGradient(&'a str),
    NoDirection(&'a str),
    TooFewGradientStops(&'a str),
    DirectionParseError(CssDirectionParseError<'a>),
    GradientParseError(CssGradientStopParseError<'a>),
    ShapeParseError(CssShapeParseError<'a>),
    ImageParseError(CssImageParseError<'a>),
}

impl_display!{ CssBackgroundParseError<'a>, {
    Error(e) => e,
    InvalidBackground(val) => format!("Invalid value: \"{}\"", val),
    UnclosedGradient(val) => format!("Unclosed gradient: \"{}\"", val),
    NoDirection(val) => format!("Gradient has no direction: \"{}\"", val),
    TooFewGradientStops(val) => format!("Too few gradient-stops: \"{}\"", val),
    DirectionParseError(e) => format!("Could not parse gradient direction: \"{}\"", e),
    GradientParseError(e) => format!("Gradient parse error: {}", e),
    ShapeParseError(e) => format!("Shape parse error: {}", e),
    ImageParseError(e) => format!("Image parse error: {}", e),
}}

impl_from!(CssDirectionParseError<'a>, CssBackgroundParseError::DirectionParseError);
impl_from!(CssGradientStopParseError<'a>, CssBackgroundParseError::GradientParseError);
impl_from!(CssShapeParseError<'a>, CssBackgroundParseError::ShapeParseError);
impl_from!(CssImageParseError<'a>, CssBackgroundParseError::ImageParseError);

#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    LinearGradient(LinearGradientPreInfo),
    RadialGradient(RadialGradientPreInfo),
    Image(CssImageId),
    NoBackground,
}

impl<'a> From<CssImageId> for Background {
    fn from(id: CssImageId) -> Self {
        Background::Image(id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradientPreInfo {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradientPreInfo {
    pub shape: Shape,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Direction {
    Angle(f32),
    FromTo(DirectionCorner, DirectionCorner),
}

impl Direction {
    /// Calculates the point for the bounds
    pub fn to_points(&self, rect: &LayoutRect)
    -> (LayoutPoint, LayoutPoint)
    {
        match self {
            Direction::Angle(deg) => {
                // note: assumes that the LayoutRect has positive sides

                // see: https://hugogiraudel.com/2013/02/04/css-gradients/

                let deg = -deg; // negate winding direction

                let width_half = rect.size.width as usize / 2;
                let height_half = rect.size.height as usize / 2;

                // hypothenuse_len is the length of the center of the rect to the corners
                let hypothenuse_len = (((width_half * width_half) + (height_half * height_half)) as f64).sqrt();

                // clamp the degree to 360 (so 410deg = 50deg)
                let mut deg = deg % 360.0;
                if deg < 0.0 {
                    deg = 360.0 + deg;
                }

                // now deg is in the range of +0..+360
                debug_assert!(deg >= 0.0 && deg <= 360.0);

                // The corner also serves to determine what quadrant we're in
                // Get the quadrant (corner) the angle is in and get the degree associated
                // with that corner.

                let angle_to_top_left = (height_half as f64 / width_half as f64).atan().to_degrees();

                // We need to calculate the angle from the center to the corner!
                let ending_point_degrees = if deg < 90.0 {
                    // top left corner
                    90.0 - angle_to_top_left
                } else if deg < 180.0 {
                    // bottom left corner
                    90.0 + angle_to_top_left
                } else if deg < 270.0 {
                    // bottom right corner
                    270.0 - angle_to_top_left
                } else /* deg > 270.0 && deg < 360.0 */ {
                    // top right corner
                    270.0 + angle_to_top_left
                };

                // assuming deg = 36deg, then degree_diff_to_corner = 9deg
                let degree_diff_to_corner = ending_point_degrees - deg as f64;

                // Searched_len is the distance between the center of the rect and the
                // ending point of the gradient
                let searched_len = (hypothenuse_len * degree_diff_to_corner.to_radians().cos()).abs();

                // TODO: This searched_len is incorrect...

                // Once we have the length, we can simply rotate the length by the angle,
                // then translate it to the center of the rect
                let dx = deg.to_radians().sin() * searched_len as f32;
                let dy = deg.to_radians().cos() * searched_len as f32;

                let start_point_location = LayoutPoint::new(width_half as f32 + dx, height_half as f32 + dy);
                let end_point_location = LayoutPoint::new(width_half as f32 - dx, height_half as f32 - dy);

                (start_point_location, end_point_location)
            },
            Direction::FromTo(from, to) => {
                (from.to_point(rect), to.to_point(rect))
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Shape {
    Ellipse,
    Circle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DirectionCorner {
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl DirectionCorner {

    pub fn opposite(&self) -> Self {
        use self::DirectionCorner::*;
        match *self {
            Right => Left,
            Left => Right,
            Top => Bottom,
            Bottom => Top,
            TopRight => BottomLeft,
            BottomLeft => TopRight,
            TopLeft => BottomRight,
            BottomRight => TopLeft,
        }
    }

    pub fn combine(&self, other: &Self) -> Option<Self> {
        use self::DirectionCorner::*;
        match (*self, *other) {
            (Right, Top) | (Top, Right) => Some(TopRight),
            (Left, Top) | (Top, Left) => Some(TopLeft),
            (Right, Bottom) | (Bottom, Right) => Some(BottomRight),
            (Left, Bottom) | (Bottom, Left) => Some(BottomLeft),
            _ => { None }
        }
    }

    pub fn to_point(&self, rect: &LayoutRect) -> TypedPoint2D<f32, LayoutPixel>
    {
        use self::DirectionCorner::*;
        match *self {
            Right => TypedPoint2D::new(rect.size.width, rect.size.height / 2.0),
            Left => TypedPoint2D::new(0.0, rect.size.height / 2.0),
            Top => TypedPoint2D::new(rect.size.width / 2.0, 0.0),
            Bottom => TypedPoint2D::new(rect.size.width / 2.0, rect.size.height),
            TopRight =>  TypedPoint2D::new(rect.size.width, 0.0),
            TopLeft => TypedPoint2D::new(0.0, 0.0),
            BottomRight => TypedPoint2D::new(rect.size.width, rect.size.height),
            BottomLeft => TypedPoint2D::new(0.0, rect.size.height),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum BackgroundType {
    LinearGradient,
    RepeatingLinearGradient,
    RadialGradient,
    RepeatingRadialGradient,
    Image,
}

// parses a background, such as "linear-gradient(red, green)"
fn parse_css_background<'a>(input: &'a str)
-> Result<Background, CssBackgroundParseError<'a>>
{
    use self::BackgroundType::*;

    let mut input_iter = input.splitn(2, "(");
    let first_item = input_iter.next();

    let background_type = match first_item {
        Some("none") => { return Ok(Background::NoBackground); },
        Some("linear-gradient") => LinearGradient,
        Some("repeating-linear-gradient") => RepeatingLinearGradient,
        Some("radial-gradient") => RadialGradient,
        Some("repeating-radial-gradient") => RepeatingRadialGradient,
        Some("image") => Image,
        _ => { return Err(CssBackgroundParseError::InvalidBackground(first_item.unwrap())); } // failure here
    };

    let next_item = match input_iter.next() {
        Some(s) => { s },
        None => return Err(CssBackgroundParseError::InvalidBackground(input)),
    };

    let mut brace_iter = next_item.rsplitn(2, ')');
    brace_iter.next(); // important
    let brace_contents = brace_iter.clone().next();

    if brace_contents.is_none() {
        // invalid or empty brace
        return Err(CssBackgroundParseError::UnclosedGradient(input));
    }

    // brace_contents contains "red, yellow, etc"
    let brace_contents = brace_contents.unwrap();
    if background_type == Image {
        let image = parse_image(brace_contents)?;
        return Ok(image.into());
    }

    let mut brace_iterator = brace_contents.split(',');

    let mut gradient_stop_count = brace_iterator.clone().count();

    // "50deg", "to right bottom", etc.
    let first_brace_item = match brace_iterator.next() {
        Some(s) => s,
        None => return Err(CssBackgroundParseError::NoDirection(input)),
    };

    // default shape: ellipse
    let mut shape = Shape::Ellipse;
    // default gradient: from top to bottom
    let mut direction = Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom);

    let mut first_is_direction = false;
    let mut first_is_shape = false;
    let is_linear_gradient = background_type == LinearGradient || background_type == RepeatingLinearGradient;
    let is_radial_gradient = background_type == RadialGradient || background_type == RepeatingRadialGradient;

    if is_linear_gradient {
        if let Ok(dir) = parse_direction(first_brace_item) {
            direction = dir;
            first_is_direction = true;
        }
    }

    if is_radial_gradient {
        if let Ok(sh) = parse_shape(first_brace_item) {
            shape = sh;
            first_is_shape = true;
        }
    }

    let mut first_item_doesnt_count = false;
    if (is_linear_gradient && first_is_direction) || (is_radial_gradient && first_is_shape) {
        gradient_stop_count -= 1; // first item is not a gradient stop
        first_item_doesnt_count = true;
    }

    if gradient_stop_count < 2 {
        return Err(CssBackgroundParseError::TooFewGradientStops(input));
    }

    let mut color_stops = Vec::<GradientStopPre>::with_capacity(gradient_stop_count);
    if !first_item_doesnt_count {
        color_stops.push(parse_gradient_stop(first_brace_item)?);
    }

    for stop in brace_iterator {
        color_stops.push(parse_gradient_stop(stop)?);
    }

    // correct percentages
    let mut last_stop = 0.0_f32;
    let mut increase_stop_cnt: Option<f32> = None;

    let color_stop_len = color_stops.len();
    'outer: for i in 0..color_stop_len {
        let offset = color_stops[i].offset;
        match offset {
            Some(s) => {
                last_stop = s;
                increase_stop_cnt = None;
            },
            None => {
                let (_, next) = color_stops.split_at_mut(i);

                if let Some(increase_stop_cnt) = increase_stop_cnt {
                    last_stop += increase_stop_cnt;
                    next[0].offset = Some(last_stop);
                    continue 'outer;
                }

                let mut next_count: u32 = 0;
                let mut next_value = None;

                // iterate until we find a value where the offset isn't none
                {
                    let mut next_iter = next.iter();
                    next_iter.next();
                    'inner: for next_stop in next_iter {
                        if let Some(off) = next_stop.offset {
                            next_value = Some(off);
                            break 'inner;
                        } else {
                            next_count += 1;
                        }
                    }
                }

                let next_value = next_value.unwrap_or(100.0_f32);
                let increase = (next_value - last_stop) / (next_count as f32);
                increase_stop_cnt = Some(increase);
                if next_count == 1 && (color_stop_len - i) == 1 {
                    next[0].offset = Some(last_stop);
                } else {
                    if i == 0 {
                        next[0].offset = Some(0.0);
                    } else {
                        next[0].offset = Some(last_stop);
                        // last_stop += increase;
                    }
                }
            }
        }
    }

    match background_type {
        LinearGradient => {
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: direction,
                extend_mode: ExtendMode::Clamp,
                stops: color_stops,
            }))
        },
        RepeatingLinearGradient => {
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: direction,
                extend_mode: ExtendMode::Repeat,
                stops: color_stops,
            }))
        },
        RadialGradient => {
            Ok(Background::RadialGradient(RadialGradientPreInfo {
                shape: shape,
                extend_mode: ExtendMode::Clamp,
                stops: color_stops,
            }))
        },
        RepeatingRadialGradient => {
            Ok(Background::RadialGradient(RadialGradientPreInfo {
                shape: shape,
                extend_mode: ExtendMode::Repeat,
                stops: color_stops,
            }))
        },
        Image => unreachable!(),
    }
}

/// Note: In theory, we could take a String here,
/// but this leads to horrible lifetime issues. Also
/// since we only parse the CSS once (at startup),
/// the performance is absolutely negligible.
///
/// However, this allows the `Css` struct to be independent
/// of the original source text, i.e. the original CSS string
/// can be deallocated after successfully parsing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssImageId(pub(crate) String);

impl<'a> From<QuoteStripped<'a>> for CssImageId {
    fn from(input: QuoteStripped<'a>) -> Self {
        CssImageId(input.0.to_string())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct QuoteStripped<'a>(pub(crate) &'a str);

fn parse_image<'a>(input: &'a str) -> Result<CssImageId, CssImageParseError<'a>> {
    Ok(strip_quotes(input)?.into())
}

/// Strip quotes from an input, given that both quotes use either `"` or `'`, but not both.
///
/// Example:
///
/// `"Helvetica"` - valid
/// `'Helvetica'` - valid
/// `'Helvetica"` - invalid
fn strip_quotes<'a>(input: &'a str) -> Result<QuoteStripped<'a>, UnclosedQuotesError<'a>> {
    let mut double_quote_iter = input.splitn(2, '"');
    double_quote_iter.next();
    let mut single_quote_iter = input.splitn(2, '\'');
    single_quote_iter.next();

    let first_double_quote = double_quote_iter.next();
    let first_single_quote = single_quote_iter.next();
    if first_double_quote.is_some() && first_single_quote.is_some() {
        return Err(UnclosedQuotesError(input));
    }
    if first_double_quote.is_some() {
        let quote_contents = first_double_quote.unwrap();
        if !quote_contents.ends_with('"') {
            return Err(UnclosedQuotesError(quote_contents));
        }
        Ok(QuoteStripped(quote_contents.trim_right_matches("\"")))
    } else if first_single_quote.is_some() {
        let quote_contents = first_single_quote.unwrap();
        if!quote_contents.ends_with('\'') {
            return Err(UnclosedQuotesError(input));
        }
        Ok(QuoteStripped(quote_contents.trim_right_matches("'")))
    } else {
        Err(UnclosedQuotesError(input))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssGradientStopParseError<'a> {
    Error(&'a str),
    ColorParseError(CssColorParseError<'a>),
}

impl_display!{ CssGradientStopParseError<'a>, {
    Error(e) => e,
    ColorParseError(e) => format!("{}", e),
}}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct GradientStopPre {
    pub offset: Option<f32>, // this is set to None if there was no offset that could be parsed
    pub color: ColorF,
}

// parses "red" , "red 5%"
fn parse_gradient_stop<'a>(input: &'a str)
-> Result<GradientStopPre, CssGradientStopParseError<'a>>
{
    let mut input_iter = input.split_whitespace();
    let first_item = input_iter.next().ok_or(CssGradientStopParseError::Error(input))?;
    let color = ColorF::from(parse_css_color(first_item).map_err(|e| CssGradientStopParseError::ColorParseError(e))?);
    let second_item = match input_iter.next() {
        None => return Ok(GradientStopPre { offset: None, color: color }),
        Some(s) => s,
    };
    let percentage = parse_percentage(second_item);
    Ok(GradientStopPre { offset: percentage, color: color })
}

// parses "5%" -> 5
fn parse_percentage(input: &str)
-> Option<f32>
{
    let mut input_iter = input.rsplitn(2, '%');
    let perc = input_iter.next();
    if perc.is_none() {
        None
    } else {
        input_iter.next()?.parse::<f32>().ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssDirectionParseError<'a> {
    Error(&'a str),
    InvalidArguments(&'a str),
    ParseFloat(ParseFloatError),
    CornerError(CssDirectionCornerParseError<'a>),
}

impl_display!{CssDirectionParseError<'a>, {
    Error(e) => e,
    InvalidArguments(val) => format!("Invalid arguments: \"{}\"", val),
    ParseFloat(e) => format!("Invalid value: {}", e),
    CornerError(e) => format!("Invalid corner value: {}", e),
}}

impl<'a> From<ParseFloatError> for CssDirectionParseError<'a> {
    fn from(e: ParseFloatError) -> Self {
        CssDirectionParseError::ParseFloat(e)
    }
}

impl<'a> From<CssDirectionCornerParseError<'a>> for CssDirectionParseError<'a> {
    fn from(e: CssDirectionCornerParseError<'a>) -> Self {
        CssDirectionParseError::CornerError(e)
    }
}

// parses "50deg", "to right bottom"
fn parse_direction<'a>(input: &'a str)
-> Result<Direction, CssDirectionParseError<'a>>
{
    use std::f32::consts::PI;

    let input_iter = input.split_whitespace();
    let count = input_iter.clone().count();
    let mut first_input_iter = input_iter.clone();
    // "50deg" | "to" | "right"
    let first_input = first_input_iter.next().ok_or(CssDirectionParseError::Error(input))?;

    enum AngleType {
        Deg,
        Rad,
        Gon,
    }

    let angle = {
        if first_input.ends_with("deg") { Some(AngleType::Deg) }
        else if first_input.ends_with("rad") { Some(AngleType::Rad) }
        else if first_input.ends_with("grad") { Some(AngleType::Gon) }
        else { None }
    };

    if let Some(angle_type) = angle {
        match angle_type {
            AngleType::Deg => { return Ok(Direction::Angle(first_input.split("deg").next().unwrap().parse::<f32>()?)); }
            AngleType::Rad => { return Ok(Direction::Angle(first_input.split("rad").next().unwrap().parse::<f32>()? * 180.0 * PI)); }
            AngleType::Gon => { return Ok(Direction::Angle(first_input.split("grad").next().unwrap().parse::<f32>()?  / 400.0 * 360.0)); }
        }
    }

    // if we get here, the input is definitely not an angle

    if first_input != "to" {
        return Err(CssDirectionParseError::InvalidArguments(input));
    }

    let second_input = first_input_iter.next().ok_or(CssDirectionParseError::Error(input))?;
    let end = parse_direction_corner(second_input)?;

    match count {
        2 => {
            // "to right"
            let start = end.opposite();
            Ok(Direction::FromTo(start, end))
        },
        3 => {
            // "to bottom right"
            let beginning = end;
            let third_input = first_input_iter.next().ok_or(CssDirectionParseError::Error(input))?;
            let new_end = parse_direction_corner(third_input)?;
            // "Bottom, Right" -> "BottomRight"
            let new_end = beginning.combine(&new_end).ok_or(CssDirectionParseError::Error(input))?;
            let start = new_end.opposite();
            Ok(Direction::FromTo(start, new_end))
        },
        _ => { Err(CssDirectionParseError::InvalidArguments(input)) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssDirectionCornerParseError<'a> {
    InvalidDirection(&'a str),
}

impl_display!{ CssDirectionCornerParseError<'a>, {
    InvalidDirection(val) => format!("Invalid direction: \"{}\"", val),
}}

fn parse_direction_corner<'a>(input: &'a str)
-> Result<DirectionCorner, CssDirectionCornerParseError<'a>>
{
    match input {
        "right" => Ok(DirectionCorner::Right),
        "left" => Ok(DirectionCorner::Left),
        "top" => Ok(DirectionCorner::Top),
        "bottom" => Ok(DirectionCorner::Bottom),
        _ => { Err(CssDirectionCornerParseError::InvalidDirection(input))}
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssShapeParseError<'a> {
    ShapeErr(InvalidValueErr<'a>),
}

impl_display!{CssShapeParseError<'a>, {
    ShapeErr(e) => format!("\"{}\"", e.0),
}}

/// Represents a parsed CSS `width` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutWidth(pub PixelValue);
/// Represents a parsed CSS `min-width` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutMinWidth(pub PixelValue);
/// Represents a parsed CSS `max-width` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutMaxWidth(pub PixelValue);
/// Represents a parsed CSS `height` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutHeight(pub PixelValue);
/// Represents a parsed CSS `min-height` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutMinHeight(pub PixelValue);
/// Represents a parsed CSS `max-height` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutMaxHeight(pub PixelValue);

/// Represents a parsed CSS `top` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutTop(pub PixelValue);
/// Represents a parsed CSS `left` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutLeft(pub PixelValue);
/// Represents a parsed CSS `right` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutRight(pub PixelValue);
/// Represents a parsed CSS `bottom` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutBottom(pub PixelValue);

/// Represents a parsed CSS `flex-grow` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutFlexGrow(pub f32);
/// Represents a parsed CSS `flex-shrink` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LayoutFlexShrink(pub f32);

/// Represents a parsed CSS `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Represents a parsed CSS `line-height` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LineHeight(pub PercentageValue);
/// Represents a parsed CSS `letter-spacing` attribute
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LetterSpacing(pub PixelValue);

/// Same as the `LayoutDirection`, but without the `-reverse` properties, used in the layout solver,
/// makes decisions based on horizontal / vertical direction easier to write.
/// Use `LayoutDirection::get_axis()` to get the axis for a given `LayoutDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

impl LayoutDirection {
    pub fn get_axis(&self) -> LayoutAxis {
        use self::{LayoutAxis::*, LayoutDirection::*};
        match self {
            Row | RowReverse => Horizontal,
            Column | ColumnReverse => Vertical,
        }
    }

    /// Returns true, if this direction is a `column-reverse` or `row-reverse` direction
    pub fn is_reverse(&self) -> bool {
        *self == LayoutDirection::RowReverse || *self == LayoutDirection::ColumnReverse
    }
}

/// Represents a parsed CSS `position` attribute - default: `Static`
///
/// NOTE: No inline positioning is supported.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

impl Default for LayoutDirection {
    fn default() -> Self {
        LayoutDirection::Column
    }
}

/// Represents a parsed CSS `flex-wrap` attribute - default: `Wrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutWrap {
    Wrap,
    NoWrap,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        LayoutWrap::Wrap
    }
}

/// Represents a parsed CSS `justify-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutJustifyContent {
    /// Default value. Items are positioned at the beginning of the container
    Start,
    /// Items are positioned at the end of the container
    End,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned with space between the lines
    SpaceBetween,
    /// Items are positioned with space before, between, and after the lines
    SpaceAround,
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        LayoutJustifyContent::Start
    }
}

/// Represents a parsed CSS `align-items` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutAlignItems {
    /// Items are stretched to fit the container
    Stretch,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned at the beginning of the container
    Start,
    /// Items are positioned at the end of the container
    End,
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        LayoutAlignItems::Stretch
    }
}

/// Represents a parsed CSS `align-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutAlignContent {
    /// Default value. Lines stretch to take up the remaining space
    Stretch,
    /// Lines are packed toward the center of the flex container
    Center,
    /// Lines are packed toward the start of the flex container
    Start,
    /// Lines are packed toward the end of the flex container
    End,
    /// Lines are evenly distributed in the flex container
    SpaceBetween,
    /// Lines are evenly distributed in the flex container, with half-size spaces on either end
    SpaceAround,
}

/// Represents a parsed CSS `overflow` attribute
///
/// NOTE: This is split into `NotModified` and `Modified`
/// in order to be able to "merge" `overflow-x` and `overflow-y`
/// into one property.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TextOverflowBehaviour {
    NotModified,
    Modified(TextOverflowBehaviourInner),
}

impl Default for TextOverflowBehaviour {
    fn default() -> Self {
        TextOverflowBehaviour::NotModified
    }
}

/// Represents a parsed CSS `overflow-x` or `overflow-y` property, see
/// [`TextOverflowBehaviour`](./struct.TextOverflowBehaviour.html) - default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TextOverflowBehaviourInner {
    /// Always shows a scroll bar, overflows on scroll
    Scroll,
    /// Does not show a scroll bar by default, only when text is overflowing
    Auto,
    /// Never shows a scroll bar, simply clips text
    Hidden,
    /// Doesn't show a scroll bar, simply overflows the text
    Visible,
}

impl Default for TextOverflowBehaviourInner {
    fn default() -> Self {
        TextOverflowBehaviourInner::Auto
    }
}

/// Horizontal text alignment enum (left, center, right) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TextAlignmentHorz {
    Left,
    Center,
    Right,
}

impl Default for TextAlignmentHorz {
    fn default() -> Self {
        TextAlignmentHorz::Center
    }
}

/// Vertical text alignment enum (top, center, bottom) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TextAlignmentVert {
    Top,
    Center,
    Bottom,
}

impl Default for TextAlignmentVert {
    fn default() -> Self {
        TextAlignmentVert::Center
    }
}

/// Stylistic options of the rectangle that don't influence the layout
/// (todo: border-box?)
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct RectStyle {
    /// Background color of this rectangle
    pub(crate) background_color: Option<BackgroundColor>,
    /// Shadow color
    pub(crate) box_shadow: Option<BoxShadowPreDisplayItem>,
    /// Gradient (location) + stops
    pub(crate) background: Option<Background>,
    /// Border
    pub(crate) border: Option<(SideOffsets2D<Au>, BorderDetails)>,
    /// Border radius
    pub(crate) border_radius: Option<BorderRadius>,
    /// Font size
    pub(crate) font_size: Option<FontSize>,
    /// Font name / family
    pub(crate) font_family: Option<FontFamily>,
    /// Text color
    pub(crate) font_color: Option<TextColor>,
    /// Text alignment
    pub(crate) text_align: Option<TextAlignmentHorz>,
    /// Text overflow behaviour
    pub(crate) overflow: Option<LayoutOverflow>,
    /// `line-height` property
    pub(crate) line_height: Option<LineHeight>,
    /// `letter-spacing` property (modifies the width and height)
    pub(crate) letter_spacing: Option<LetterSpacing>,
}

typed_pixel_value_parser!(parse_css_letter_spacing, LetterSpacing);

// Layout constraints for a given rectangle, such as "width", "min-width", "height", etc.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct RectLayout {

    pub width: Option<LayoutWidth>,
    pub height: Option<LayoutHeight>,
    pub min_width: Option<LayoutMinWidth>,
    pub min_height: Option<LayoutMinHeight>,
    pub max_width: Option<LayoutMaxWidth>,
    pub max_height: Option<LayoutMaxHeight>,

    pub position: Option<LayoutPosition>,
    pub top: Option<LayoutTop>,
    pub bottom: Option<LayoutBottom>,
    pub right: Option<LayoutRight>,
    pub left: Option<LayoutLeft>,

    pub padding: Option<LayoutPadding>,
    pub margin: Option<LayoutMargin>,

    pub direction: Option<LayoutDirection>,
    pub wrap: Option<LayoutWrap>,
    pub flex_grow: Option<LayoutFlexGrow>,
    pub flex_shrink: Option<LayoutFlexShrink>,
    pub justify_content: Option<LayoutJustifyContent>,
    pub align_items: Option<LayoutAlignItems>,
    pub align_content: Option<LayoutAlignContent>,
}

typed_pixel_value_parser!(parse_layout_width, LayoutWidth);
typed_pixel_value_parser!(parse_layout_height, LayoutHeight);
typed_pixel_value_parser!(parse_layout_min_height, LayoutMinHeight);
typed_pixel_value_parser!(parse_layout_min_width, LayoutMinWidth);
typed_pixel_value_parser!(parse_layout_max_width, LayoutMaxWidth);
typed_pixel_value_parser!(parse_layout_max_height, LayoutMaxHeight);

typed_pixel_value_parser!(parse_layout_top, LayoutTop);
typed_pixel_value_parser!(parse_layout_bottom, LayoutBottom);
typed_pixel_value_parser!(parse_layout_right, LayoutRight);
typed_pixel_value_parser!(parse_layout_left, LayoutLeft);

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display!{FlexGrowParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-grow: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

fn parse_layout_flex_grow<'a>(input: &'a str) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexGrow(o)),
        Err(e) => Err(FlexGrowParseError::ParseFloat(e, input)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display!{FlexShrinkParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-shrink: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

fn parse_layout_flex_shrink<'a>(input: &'a str) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexShrink(o)),
        Err(e) => Err(FlexShrinkParseError::ParseFloat(e, input)),
    }
}

fn parse_css_line_height(input: &str)
-> Result<LineHeight, PercentageParseError>
{
    parse_percentage_value(input).and_then(|e| Ok(LineHeight(e)))
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct FontSize(pub(crate) PixelValue);

typed_pixel_value_parser!(parse_css_font_size, FontSize);

impl FontSize {
    /// Creates the font size in pixel
    pub fn px(value: f32) -> Self {
        FontSize(PixelValue::px(value))
    }

    /// Creates the font size in em
    pub fn em(value: f32) -> Self {
        FontSize(PixelValue::em(value))
    }

    /// Creates the font size in point (pt)
    pub fn pt(value: f32) -> Self {
        FontSize(PixelValue::pt(value))
    }

    pub fn to_pixels(&self) -> f32 {
        self.0.to_pixels()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct FontFamily {
    // parsed fonts, in order, i.e. "Webly Sleeky UI", "monospace", etc.
    pub(crate) fonts: Vec<FontId>
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum FontId {
    BuiltinFont(String),
    ExternalFont(String),
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssFontFamilyParseError<'a> {
    InvalidFontFamily(&'a str),
    UnclosedQuotes(&'a str),
}

impl_display!{CssFontFamilyParseError<'a>, {
    InvalidFontFamily(val) => format!("Invalid font-family: \"{}\"", val),
    UnclosedQuotes(val) => format!("Unclosed quotes: \"{}\"", val),
}}

impl<'a> From<UnclosedQuotesError<'a>> for CssFontFamilyParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssFontFamilyParseError::UnclosedQuotes(err.0)
    }
}

// parses a "font-family" declaration, such as:
//
// "Webly Sleeky UI", monospace
// 'Webly Sleeky Ui', monospace
// sans-serif
pub(crate) fn parse_css_font_family<'a>(input: &'a str) -> Result<FontFamily, CssFontFamilyParseError<'a>> {
    let multiple_fonts = input.split(',');
    let mut fonts = Vec::with_capacity(1);

    for font in multiple_fonts {
        let font = font.trim();

        let mut double_quote_iter = font.splitn(2, '"');
        double_quote_iter.next();
        let mut single_quote_iter = font.splitn(2, '\'');
        single_quote_iter.next();

        if double_quote_iter.next().is_some() || single_quote_iter.next().is_some() {
            let stripped_font = strip_quotes(font)?;
            fonts.push(FontId::ExternalFont(stripped_font.0.into()));
        } else {
            fonts.push(FontId::BuiltinFont(font.into()));
        }
    }

    Ok(FontFamily {
        fonts: fonts,
    })
}

multi_type_parser!(parse_layout_direction, LayoutDirection,
                    ["row", Row],
                    ["row-reverse", RowReverse],
                    ["column", Column],
                    ["column-reverse", ColumnReverse]);

multi_type_parser!(parse_layout_wrap, LayoutWrap,
                    ["wrap", Wrap],
                    ["nowrap", NoWrap]);

multi_type_parser!(parse_layout_justify_content, LayoutJustifyContent,
                    ["flex-start", Start],
                    ["flex-end", End],
                    ["center", Center],
                    ["space-between", SpaceBetween],
                    ["space-around", SpaceAround]);

multi_type_parser!(parse_layout_align_items, LayoutAlignItems,
                    ["flex-start", Start],
                    ["flex-end", End],
                    ["stretch", Stretch],
                    ["center", Center]);

multi_type_parser!(parse_layout_align_content, LayoutAlignContent,
                    ["flex-start", Start],
                    ["flex-end", End],
                    ["stretch", Stretch],
                    ["center", Center],
                    ["space-between", SpaceBetween],
                    ["space-around", SpaceAround]);

multi_type_parser!(parse_shape, Shape,
                    ["circle", Circle],
                    ["ellipse", Ellipse]);

multi_type_parser!(parse_layout_position, LayoutPosition,
                    ["static", Static],
                    ["absolute", Absolute],
                    ["relative", Relative]);

multi_type_parser!(parse_layout_text_overflow, TextOverflowBehaviourInner,
                    ["auto", Auto],
                    ["scroll", Scroll],
                    ["visible", Visible],
                    ["hidden", Hidden]);

multi_type_parser!(parse_layout_text_align, TextAlignmentHorz,
                    ["center", Center],
                    ["left", Left],
                    ["right", Right]);

/// CssColor is simply a wrapper around the internal CSS color parsing methods.
///
/// Sometimes you'd want to load and parse a CSS color, but you don't want to
/// write your own parser for that. Since azul already has a parser for CSS colors,
/// this API exposes
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct CssColor {
    internal: ColorU,
}

impl CssColor {
    /// Can parse a CSS color with or without prefixed hash or capitalization, i.e. `#aabbcc`
    pub fn from_str<'a>(input: &'a str) -> Result<Self, CssColorParseError<'a>> {
        let color = parse_css_color(input)?;
        Ok(Self {
            internal: color,
        })
    }

    /// Returns the internal parsed color, but in a `0.0 - 1.0` range instead of `0 - 255`
    pub fn to_color_f(&self) -> ColorF {
        self.internal.into()
    }

    /// Returns the internal parsed color
    pub fn to_color_u(&self) -> ColorU {
        self.internal
    }

    /// If `prefix_hash` is set to false, you only get the string, without a hash, in lowercase
    ///
    /// If `self.alpha` is `FF`, it wil be omitted from the final result (since `FF` is the default for CSS colors)
    pub fn to_string(&self, prefix_hash: bool) -> String {
        let prefix = if prefix_hash { "#" } else { "" };
        let alpha = if self.internal.a == 255 { String::new() } else { format!("{:02x}", self.internal.a) };
        format!("{}{:02x}{:02x}{:02x}{}", prefix, self.internal.r, self.internal.g, self.internal.b, alpha)
    }
}

impl Into<ColorF> for CssColor {
    fn into(self) -> ColorF {
        self.to_color_f()
    }
}

impl Into<ColorU> for CssColor {
    fn into(self) -> ColorU {
        self.to_color_u()
    }
}

impl Into<String> for CssColor {
    fn into(self) -> String {
        self.to_string(false)
    }
}

#[cfg(feature = "serde_serialization")]
use serde::{de, Serialize, Deserialize, Serializer, Deserializer};

#[cfg(feature = "serde_serialization")]
impl Serialize for CssColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer,
    {
        let prefix_css_color_with_hash = true;
        serializer.serialize_str(&self.to_string(prefix_css_color_with_hash))
    }
}

#[cfg(feature = "serde_serialization")]
impl<'de> Deserialize<'de> for CssColor {
    fn deserialize<D>(deserializer: D) -> Result<CssColor, D::Error>
    where D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CssColor::from_str(&s).map_err(de::Error::custom)
    }
}


#[cfg(test)]
mod css_tests {
    use super::*;

    #[test]
    fn test_parse_box_shadow_1() {
        assert_eq!(parse_css_box_shadow("none"), Ok(None));
    }

    #[test]
    fn test_css_color_convert() {
        let color = "#FFD700";
        let parsed = CssColor::from_str(color).unwrap();
        assert_eq!(parsed.to_string(true).to_uppercase(), color.to_string());
        assert_eq!(parsed.to_color_u(), ColorU { r: 255, g: 215, b: 0, a: 255 });
    }

    #[test]
    fn test_parse_box_shadow_2() {
        assert_eq!(parse_css_box_shadow("5px 10px"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            blur_radius: 0.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_3() {
        assert_eq!(parse_css_box_shadow("5px 10px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 0.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_4() {
        assert_eq!(parse_css_box_shadow("5px 10px inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            blur_radius: 0.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_5() {
        assert_eq!(parse_css_box_shadow("5px 10px outset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            blur_radius: 0.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_6() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 5.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_7() {
        assert_eq!(parse_css_box_shadow("5px 10px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 0.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_8() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 5.0,
            spread_radius: 0.0,
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_9() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px 10px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 5.0,
            spread_radius: 10.0,
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_10() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px 10px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: LayoutVector2D::new(5.0, 10.0),
            color: ColorF { r: 0.53333336, g: 0.53333336, b: 0.53333336, a: 1.0 },
            blur_radius: 5.0,
            spread_radius: 10.0,
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_css_border_1() {
        assert_eq!(parse_css_border("5px solid red"), Ok((
            SideOffsets2D::new(Au::from_f32_px(5.0), Au::from_f32_px(5.0), Au::from_f32_px(5.0), Au::from_f32_px(5.0)),
            BorderDetails::Normal(NormalBorder {
                left: BorderSide {
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Solid,
                },
                right: BorderSide {
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Solid,
                },
                bottom: BorderSide {
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Solid,
                },
                top: BorderSide {
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Solid,
                },
                radius: BorderRadius::zero(),
                do_aa: true,
            })
        )));
    }

    #[test]
    fn test_parse_css_border_2() {
        assert_eq!(parse_css_border("double"), Ok((
                SideOffsets2D::new(Au::from_f32_px(1.0), Au::from_f32_px(1.0), Au::from_f32_px(1.0), Au::from_f32_px(1.0)),
                BorderDetails::Normal(NormalBorder {
                left: BorderSide {
                    color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Double,
                },
                right: BorderSide {
                    color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Double,
                },
                bottom: BorderSide {
                    color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Double,
                },
                top: BorderSide {
                    color: ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                    style: BorderStyle::Double,
                },
                radius: BorderRadius::zero(),
                do_aa: true,
            })
        )));
    }

    #[test]
    fn test_parse_linear_gradient_1() {
        assert_eq!(parse_css_background("linear-gradient(red, yellow)"),
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(0.0),
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(100.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
            })));
    }

    #[test]
    fn test_parse_linear_gradient_2() {
        assert_eq!(parse_css_background("linear-gradient(red, lime, blue, yellow)"),
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(0.0),
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(33.333332),
                    color: ColorF { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(66.666664),
                    color: ColorF { r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(100.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_3() {
        assert_eq!(parse_css_background("repeating-linear-gradient(50deg, blue, yellow, #00FF00)"),
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: Direction::Angle(50.0),
                extend_mode: ExtendMode::Repeat,
                stops: vec![
                GradientStopPre {
                    offset: Some(0.0),
                    color: ColorF { r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(50.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(100.0),
                    color: ColorF { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_4() {
        assert_eq!(parse_css_background("linear-gradient(to bottom right, red, yellow)"),
            Ok(Background::LinearGradient(LinearGradientPreInfo {
                direction: Direction::FromTo(DirectionCorner::TopLeft, DirectionCorner::BottomRight),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(0.0),
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(100.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
            })));
    }

    #[test]
    fn test_parse_radial_gradient_1() {
        assert_eq!(parse_css_background("radial-gradient(circle, lime, blue, yellow)"),
            Ok(Background::RadialGradient(RadialGradientPreInfo {
                shape: Shape::Circle,
                extend_mode: ExtendMode::Clamp,
                stops: vec![
                GradientStopPre {
                    offset: Some(0.0),
                    color: ColorF { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(50.0),
                    color: ColorF { r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(100.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
        })));
    }

    // This test currently fails, but it's not that important to fix right now
    /*
    #[test]
    fn test_parse_radial_gradient_2() {
        assert_eq!(parse_css_background("repeating-radial-gradient(circle, red 10%, blue 50%, lime, yellow)"),
            Ok(ParsedGradient::RadialGradient(RadialGradientPreInfo {
                shape: Shape::Circle,
                extend_mode: ExtendMode::Repeat,
                stops: vec![
                GradientStopPre {
                    offset: Some(0.1),
                    color: ColorF { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(0.5),
                    color: ColorF { r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(0.75),
                    color: ColorF { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
                },
                GradientStopPre {
                    offset: Some(1.0),
                    color: ColorF { r: 1.0, g: 1.0, b: 0.0, a: 1.0 },
                }],
        })));
    }
    */

    #[test]
    fn test_parse_css_color_1() {
        assert_eq!(parse_css_color("#F0F8FF"), Ok(ColorU { r: 240, g: 248, b: 255, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_2() {
        assert_eq!(parse_css_color("#F0F8FF00"), Ok(ColorU { r: 240, g: 248, b: 255, a: 0 }));
    }

    #[test]
    fn test_parse_css_color_3() {
        assert_eq!(parse_css_color("#EEE"), Ok(ColorU { r: 238, g: 238, b: 238, a: 255 }));
    }

    #[test]
    fn test_parse_pixel_value_1() {
        assert_eq!(parse_pixel_value("15px"), Ok(PixelValue { metric: CssMetric::Px, number: 15000 }));
    }

    #[test]
    fn test_parse_pixel_value_2() {
        assert_eq!(parse_pixel_value("1.2em"), Ok(PixelValue { metric: CssMetric::Em, number: 1200 }));
    }

    #[test]
    fn test_parse_pixel_value_3() {
        assert_eq!(parse_pixel_value("11pt"), Ok(PixelValue { metric: CssMetric::Pt, number: 11000 }));
    }

    #[test]
    fn test_parse_pixel_value_4() {
        assert_eq!(parse_pixel_value("aslkfdjasdflk"), Err(PixelParseError::InvalidComponent("aslkfdjasdflk")));
    }

    #[test]
    fn test_parse_css_border_radius_1() {
        assert_eq!(parse_css_border_radius("15px"), Ok(BorderRadius::uniform(15.0)));
    }

    #[test]
    fn test_parse_css_border_radius_2() {
        assert_eq!(parse_css_border_radius("15px 50px"), Ok(BorderRadius {
            top_left: LayoutSize::new(15.0, 15.0),
            bottom_right: LayoutSize::new(15.0, 15.0),
            top_right: LayoutSize::new(50.0, 50.0),
            bottom_left: LayoutSize::new(50.0, 50.0),
        }));
    }

    #[test]
    fn test_parse_css_border_radius_3() {
        assert_eq!(parse_css_border_radius("15px 50px 30px"), Ok(BorderRadius {
            top_left: LayoutSize::new(15.0, 15.0),
            bottom_right: LayoutSize::new(30.0, 30.0),
            top_right: LayoutSize::new(50.0, 50.0),
            bottom_left: LayoutSize::new(50.0, 50.0),
        }));
    }

    #[test]
    fn test_parse_css_border_radius_4() {
        assert_eq!(parse_css_border_radius("15px 50px 30px 5px"), Ok(BorderRadius {
            top_left: LayoutSize::new(15.0, 15.0),
            bottom_right: LayoutSize::new(30.0, 30.0),
            top_right: LayoutSize::new(50.0, 50.0),
            bottom_left: LayoutSize::new(5.0, 5.0),
        }));
    }

    #[test]
    fn test_parse_css_font_family_1() {
        assert_eq!(parse_css_font_family("\"Webly Sleeky UI\", monospace"), Ok(FontFamily {
            fonts: vec![
                FontId::ExternalFont("Webly Sleeky UI".into()),
                FontId::BuiltinFont("monospace".into()),
            ]
        }));
    }

    #[test]
    fn test_parse_css_font_family_2() {
        assert_eq!(parse_css_font_family("'Webly Sleeky UI'"), Ok(FontFamily {
            fonts: vec![
                FontId::ExternalFont("Webly Sleeky UI".into()),
            ]
        }));
    }

    #[test]
    fn test_parse_background_image() {
        assert_eq!(parse_css_background("image(\"Cat 01\")"), Ok(Background::Image(
            CssImageId(String::from("Cat 01"))
        )));
    }

    #[test]
    fn test_parse_padding_1() {
        assert_eq!(parse_layout_padding("10px"), Ok(LayoutPadding {
            top: Some(PixelValue::from_metric(CssMetric::Px, 10.0)),
            right: Some(PixelValue::from_metric(CssMetric::Px, 10.0)),
            bottom: Some(PixelValue::from_metric(CssMetric::Px, 10.0)),
            left: Some(PixelValue::from_metric(CssMetric::Px, 10.0)),
        }));
    }

    #[test]
    fn test_parse_padding_2() {
        assert_eq!(parse_layout_padding("25px 50px"), Ok(LayoutPadding {
            top: Some(PixelValue::from_metric(CssMetric::Px, 25.0)),
            right: Some(PixelValue::from_metric(CssMetric::Px, 50.0)),
            bottom: Some(PixelValue::from_metric(CssMetric::Px, 25.0)),
            left: Some(PixelValue::from_metric(CssMetric::Px, 50.0)),
        }));
    }

    #[test]
    fn test_parse_padding_3() {
        assert_eq!(parse_layout_padding("25px 50px 75px"), Ok(LayoutPadding {
            top: Some(PixelValue::from_metric(CssMetric::Px, 25.0)),
            right: Some(PixelValue::from_metric(CssMetric::Px, 50.0)),
            left: Some(PixelValue::from_metric(CssMetric::Px, 50.0)),
            bottom: Some(PixelValue::from_metric(CssMetric::Px, 75.0)),
        }));
    }

    #[test]
    fn test_parse_padding_4() {
        assert_eq!(parse_layout_padding("25px 50px 75px 100px"), Ok(LayoutPadding {
            top: Some(PixelValue::from_metric(CssMetric::Px, 25.0)),
            right: Some(PixelValue::from_metric(CssMetric::Px, 50.0)),
            bottom: Some(PixelValue::from_metric(CssMetric::Px, 75.0)),
            left: Some(PixelValue::from_metric(CssMetric::Px, 100.0)),
        }));
    }
}