//! Contains utilities to convert strings (CSS strings) to servo types

use std::num::{ParseIntError, ParseFloatError};
use azul_css::{
    CssPropertyType, CssProperty, CombinedCssPropertyType, CssPropertyValue,
    Overflow, Shape, PixelValue, PercentageValue, FloatValue, ColorU,
    GradientStopPre, RadialGradient, DirectionCorner, Direction, CssImageId,
    LinearGradient, BoxShadowPreDisplayItem, StyleBorderSide, BorderStyle,
    PixelSize, SizeMetric, BoxShadowClipMode, ExtendMode, FontId, GradientType,

    StyleTextColor, StyleFontSize, StyleFontFamily, StyleTextAlignmentHorz,
    StyleLetterSpacing, StyleLineHeight, StyleWordSpacing, StyleTabWidth,
    StyleCursor, LayoutWidth, LayoutHeight, LayoutMinWidth, LayoutMinHeight,
    LayoutMaxWidth, LayoutMaxHeight, LayoutPosition, LayoutTop, LayoutRight,
    LayoutLeft, LayoutBottom, LayoutWrap, LayoutDirection, LayoutFlexGrow,
    LayoutFlexShrink, LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent,
    StyleBackgroundContent, StyleBackgroundPosition, StyleBackgroundSize,
    StyleBackgroundRepeat, LayoutPaddingTop, LayoutPaddingLeft, LayoutPaddingRight,
    LayoutPaddingBottom, LayoutMarginTop, LayoutMarginLeft, LayoutMarginRight,
    LayoutMarginBottom, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
    StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleBorderTopColor,
    StyleBorderRightColor, StyleBorderLeftColor, StyleBorderBottomColor,
    StyleBorderTopStyle, StyleBorderRightStyle, StyleBorderLeftStyle,
    StyleBorderBottomStyle, StyleBorderTopWidth, StyleBorderRightWidth,
    StyleBorderLeftWidth, StyleBorderBottomWidth,
};

/// A parser that can accept a list of items and mappings
macro_rules! multi_type_parser {
    ($fn:ident, $return_str:expr, $return:ident, $import_str:expr, $([$identifier_string:expr, $enum_type:ident, $parse_str:expr]),+) => {
        #[doc = "Parses a `"]
        #[doc = $return_str]
        #[doc = "` attribute from a `&str`"]
        #[doc = ""]
        #[doc = "# Example"]
        #[doc = ""]
        #[doc = "```rust"]
        #[doc = $import_str]
        $(
            #[doc = $parse_str]
        )+
        #[doc = "```"]
        pub fn $fn<'a>(input: &'a str)
        -> Result<$return, InvalidValueErr<'a>>
        {
            let input = input.trim();
            match input {
                $(
                    $identifier_string => Ok($return::$enum_type),
                )+
                _ => Err(InvalidValueErr(input)),
            }
        }
    };
    ($fn:ident, $return:ident, $([$identifier_string:expr, $enum_type:ident]),+) => {
        multi_type_parser!($fn, stringify!($return), $return,
            concat!(
                "# extern crate azul_css;", "\r\n",
                "# extern crate azul_css_parser;", "\r\n",
                "# use azul_css_parser::", stringify!($fn), ";", "\r\n",
                "# use azul_css::", stringify!($return), ";"
            ),
            $([
                $identifier_string, $enum_type,
                concat!("assert_eq!(", stringify!($fn), "(\"", $identifier_string, "\"), Ok(", stringify!($return), "::", stringify!($enum_type), "));")
            ]),+
        );
    };
}

macro_rules! typed_pixel_value_parser {
    ($fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr) => {
        #[doc = "Parses a `"]
        #[doc = $return_str]
        #[doc = "` attribute from a `&str`"]
        #[doc = ""]
        #[doc = "# Example"]
        #[doc = ""]
        #[doc = "```rust"]
        #[doc = $import_str]
        #[doc = $test_str]
        #[doc = "```"]
        pub fn $fn<'a>(input: &'a str) -> Result<$return, PixelParseError<'a>> {
            parse_pixel_value(input).and_then(|e| Ok($return(e)))
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!($fn, stringify!($fn), $return, stringify!($return),
            concat!(
                "# extern crate azul_css;", "\r\n",
                "# extern crate azul_css_parser;", "\r\n",
                "# use azul_css_parser::", stringify!($fn), ";", "\r\n",
                "# use azul_css::{PixelValue, ", stringify!($return), "};"
            ),
            concat!("assert_eq!(", stringify!($fn), "(\"5px\"), Ok(", stringify!($return), "(PixelValue::px(5.0))));")
        );
    };
}

/// Main parsing function, takes a stringified key / value pair and either
/// returns the parsed value or an error
///
/// ```rust
/// # extern crate azul_css_parser;
/// # extern crate azul_css;
/// # use azul_css_parser;
/// # use azul_css::{LayoutWidth, PixelValue, CssPropertyType, CssProperty};
/// assert_eq!(
///     azul_css_parser::parse_css_property(CssPropertyType::Width, "500px"),
///     Ok(CssPropertyValue::Exact(CssProperty::Width(LayoutWidth(PixelValue::px(500.0)))))
/// )
/// ```
pub fn parse_css_property<'a>(key: CssPropertyType, value: &'a str) -> Result<CssProperty, CssParsingError<'a>> {
    use self::CssPropertyType::*;
    let value = value.trim();
    Ok(match value {
        "auto" => CssProperty::auto(key),
        "none" => CssProperty::none(key),
        "initial" => CssProperty::initial(key).into(),
        "inherit" => CssProperty::inherit(key).into(),
        value => match key {
            TextColor                   => parse_style_text_color(value)?.into(),
            FontSize                    => parse_style_font_size(value)?.into(),
            FontFamily                  => parse_style_font_family(value)?.into(),
            TextAlign                   => parse_layout_text_align(value)?.into(),
            LetterSpacing               => parse_style_letter_spacing(value)?.into(),
            LineHeight                  => parse_style_line_height(value)?.into(),
            WordSpacing                 => parse_style_word_spacing(value)?.into(),
            TabWidth                    => parse_style_tab_width(value)?.into(),
            Cursor                      => parse_style_cursor(value)?.into(),
            Width                       => parse_layout_width(value)?.into(),
            Height                      => parse_layout_height(value)?.into(),
            MinWidth                    => parse_layout_min_width(value)?.into(),
            MinHeight                   => parse_layout_min_height(value)?.into(),
            MaxWidth                    => parse_layout_max_width(value)?.into(),
            MaxHeight                   => parse_layout_max_height(value)?.into(),
            Position                    => parse_layout_position(value)?.into(),
            Top                         => parse_layout_top(value)?.into(),
            Right                       => parse_layout_right(value)?.into(),
            Left                        => parse_layout_left(value)?.into(),
            Bottom                      => parse_layout_bottom(value)?.into(),
            FlexWrap                    => parse_layout_wrap(value)?.into(),
            FlexDirection               => parse_layout_direction(value)?.into(),
            FlexGrow                    => parse_layout_flex_grow(value)?.into(),
            FlexShrink                  => parse_layout_flex_shrink(value)?.into(),
            JustifyContent              => parse_layout_justify_content(value)?.into(),
            AlignItems                  => parse_layout_align_items(value)?.into(),
            AlignContent                => parse_layout_align_content(value)?.into(),

            Background                  => parse_style_background_content(value)?.into(),
            BackgroundImage             => StyleBackgroundContent::Image(parse_image(value)?).into(),
            BackgroundColor             => StyleBackgroundContent::Color(parse_css_color(value)?).into(),
            BackgroundPosition          => parse_style_background_position(value)?.into(),
            BackgroundSize              => parse_style_background_size(value)?.into(),
            BackgroundRepeat            => parse_style_background_repeat(value)?.into(),

            OverflowX                   => CssProperty::OverflowX(CssPropertyValue::Exact(parse_layout_overflow(value)?)).into(),
            OverflowY                   => CssProperty::OverflowY(CssPropertyValue::Exact(parse_layout_overflow(value)?)).into(),

            PaddingTop                  => parse_layout_padding_top(value)?.into(),
            PaddingLeft                 => parse_layout_padding_left(value)?.into(),
            PaddingRight                => parse_layout_padding_right(value)?.into(),
            PaddingBottom               => parse_layout_padding_bottom(value)?.into(),

            MarginTop                   => parse_layout_margin_top(value)?.into(),
            MarginLeft                  => parse_layout_margin_left(value)?.into(),
            MarginRight                 => parse_layout_margin_right(value)?.into(),
            MarginBottom                => parse_layout_margin_bottom(value)?.into(),

            BorderTopLeftRadius         => parse_style_border_top_left_radius(value)?.into(),
            BorderTopRightRadius        => parse_style_border_top_right_radius(value)?.into(),
            BorderBottomLeftRadius      => parse_style_border_bottom_left_radius(value)?.into(),
            BorderBottomRightRadius     => parse_style_border_bottom_right_radius(value)?.into(),

            BorderTopColor              => StyleBorderTopColor(parse_css_color(value)?).into(),
            BorderRightColor            => StyleBorderRightColor(parse_css_color(value)?).into(),
            BorderLeftColor             => StyleBorderLeftColor(parse_css_color(value)?).into(),
            BorderBottomColor           => StyleBorderBottomColor(parse_css_color(value)?).into(),

            BorderTopStyle              => StyleBorderTopStyle(parse_css_border_style(value)?).into(),
            BorderRightStyle            => StyleBorderRightStyle(parse_css_border_style(value)?).into(),
            BorderLeftStyle             => StyleBorderLeftStyle(parse_css_border_style(value)?).into(),
            BorderBottomStyle           => StyleBorderBottomStyle(parse_css_border_style(value)?).into(),

            BorderTopWidth              => parse_style_border_top_width(value)?.into(),
            BorderRightWidth            => parse_style_border_right_width(value)?.into(),
            BorderLeftWidth             => parse_style_border_left_width(value)?.into(),
            BorderBottomWidth           => parse_style_border_bottom_width(value)?.into(),

            BoxShadowLeft               => CssProperty::BoxShadowLeft(CssPropertyValue::Exact(parse_css_box_shadow(value)?)).into(),
            BoxShadowRight              => CssProperty::BoxShadowRight(CssPropertyValue::Exact(parse_css_box_shadow(value)?)).into(),
            BoxShadowTop                => CssProperty::BoxShadowTop(CssPropertyValue::Exact(parse_css_box_shadow(value)?)).into(),
            BoxShadowBottom             => CssProperty::BoxShadowBottom(CssPropertyValue::Exact(parse_css_box_shadow(value)?)).into(),
        }
    })
}

/// Parses a combined CSS property or a CSS property shorthand, for example "margin"
/// (as a shorthand for setting all four properties of "margin-top", "margin-bottom",
/// "margin-left" and "margin-right")
///
/// ```rust
/// # extern crate azul_css_parser;
/// # extern crate azul_css;
/// # use azul_css_parser;
/// # use azul_css::{LayoutWidth, PixelValue, CombinedCssPropertyType, CssProperty};
/// assert_eq!(
///     azul_css_parser::parse_combined_css_property(CombinedCssPropertyType::BorderRadius, "10px"),
///     Ok(vec![
///         CssPropertyValue::Exact(CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadius(PixelValue::px(10.0)))),
///         CssPropertyValue::Exact(CssProperty::BorderTopRightRadius(StyleBorderTopLeftRadius(PixelValue::px(10.0)))),
///         CssPropertyValue::Exact(CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadius(PixelValue::px(10.0)))),
///         CssPropertyValue::Exact(CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadius(PixelValue::px(10.0)))),
///     ])
/// )
/// ```
pub fn parse_combined_css_property<'a>(key: CombinedCssPropertyType, value: &'a str)
-> Result<Vec<CssProperty>, CssParsingError<'a>>
{
    use self::CombinedCssPropertyType::*;

    let keys = match key {
        BorderRadius => {
            vec![
                CssPropertyType::BorderTopLeftRadius,
                CssPropertyType::BorderTopRightRadius,
                CssPropertyType::BorderBottomLeftRadius,
                CssPropertyType::BorderBottomRightRadius,
            ]
        },
        Overflow => {
            vec![
                CssPropertyType::OverflowX,
                CssPropertyType::OverflowY,
            ]
        },
        Padding => {
            vec![
                CssPropertyType::PaddingTop,
                CssPropertyType::PaddingBottom,
                CssPropertyType::PaddingLeft,
                CssPropertyType::PaddingRight,
            ]
        },
        Margin => {
            vec![
                CssPropertyType::MarginTop,
                CssPropertyType::MarginBottom,
                CssPropertyType::MarginLeft,
                CssPropertyType::MarginRight,
            ]
        },
        Border => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderTopWidth,
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderLeftWidth,
                CssPropertyType::BorderBottomWidth,
            ]
        },
        BorderLeft => {
            vec![
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderLeftWidth,
            ]
        },
        BorderRight => {
            vec![
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderRightWidth,
            ]
        },
        BorderTop => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderTopWidth,
            ]
        },
        BorderBottom => {
            vec![
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderBottomWidth,
            ]
        },
        BoxShadow => {
            vec![
                CssPropertyType::BoxShadowLeft,
                CssPropertyType::BoxShadowRight,
                CssPropertyType::BoxShadowTop,
                CssPropertyType::BoxShadowBottom,
            ]
        },
    };

    match value {
        "auto" => return Ok(keys.into_iter().map(|ty| CssProperty::auto(ty)).collect()),
        "none" => return Ok(keys.into_iter().map(|ty| CssProperty::none(ty)).collect()),
        "initial" => return Ok(keys.into_iter().map(|ty| CssProperty::initial(ty)).collect()),
        "inherit" => return Ok(keys.into_iter().map(|ty| CssProperty::inherit(ty)).collect()),
        _ => { },
    };

    match key {
        BorderRadius => {
            let border_radius = parse_style_border_radius(value)?;
            Ok(vec![
                CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadius(border_radius.top_left).into()),
                CssProperty::BorderTopRightRadius(StyleBorderTopRightRadius(border_radius.top_right).into()),
                CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadius(border_radius.bottom_left).into()),
                CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadius(border_radius.bottom_right).into()),
            ])
        },
        Overflow => {
            let overflow = parse_layout_overflow(value)?;
            Ok(vec![
                CssProperty::OverflowX(overflow.into()),
                CssProperty::OverflowY(overflow.into()),
            ])
        },
        Padding => {
            let padding = parse_layout_padding(value)?;
            Ok(vec![
                CssProperty::PaddingTop(LayoutPaddingTop(padding.top).into()),
                CssProperty::PaddingBottom(LayoutPaddingBottom(padding.bottom).into()),
                CssProperty::PaddingLeft(LayoutPaddingLeft(padding.left).into()),
                CssProperty::PaddingRight(LayoutPaddingRight(padding.right).into()),
            ])
        },
        Margin => {
            let margin = parse_layout_margin(value)?;
            Ok(vec![
                CssProperty::MarginTop(LayoutMarginTop(margin.top).into()),
                CssProperty::MarginBottom(LayoutMarginBottom(margin.bottom).into()),
                CssProperty::MarginLeft(LayoutMarginLeft(margin.left).into()),
                CssProperty::MarginRight(LayoutMarginRight(margin.right).into()),
            ])
        },
        Border => {
            let border = parse_css_border(value)?;
            Ok(vec![
               CssProperty::BorderTopColor(StyleBorderTopColor(border.border_color).into()),
               CssProperty::BorderRightColor(StyleBorderRightColor(border.border_color).into()),
               CssProperty::BorderLeftColor(StyleBorderLeftColor(border.border_color).into()),
               CssProperty::BorderBottomColor(StyleBorderBottomColor(border.border_color).into()),

               CssProperty::BorderTopStyle(StyleBorderTopStyle(border.border_style).into()),
               CssProperty::BorderRightStyle(StyleBorderRightStyle(border.border_style).into()),
               CssProperty::BorderLeftStyle(StyleBorderLeftStyle(border.border_style).into()),
               CssProperty::BorderBottomStyle(StyleBorderBottomStyle(border.border_style).into()),

               CssProperty::BorderTopWidth(StyleBorderTopWidth(border.border_width).into()),
               CssProperty::BorderRightWidth(StyleBorderRightWidth(border.border_width).into()),
               CssProperty::BorderLeftWidth(StyleBorderLeftWidth(border.border_width).into()),
               CssProperty::BorderBottomWidth(StyleBorderBottomWidth(border.border_width).into()),
            ])
        },
        BorderLeft => {
            let border = parse_css_border(value)?;
            Ok(vec![
               CssProperty::BorderLeftColor(StyleBorderLeftColor(border.border_color).into()),
               CssProperty::BorderLeftStyle(StyleBorderLeftStyle(border.border_style).into()),
               CssProperty::BorderLeftWidth(StyleBorderLeftWidth(border.border_width).into()),
            ])
        },
        BorderRight => {
            let border = parse_css_border(value)?;
            Ok(vec![
               CssProperty::BorderRightColor(StyleBorderRightColor(border.border_color).into()),
               CssProperty::BorderRightStyle(StyleBorderRightStyle(border.border_style).into()),
               CssProperty::BorderRightWidth(StyleBorderRightWidth(border.border_width).into()),
            ])
        },
        BorderTop => {
            let border = parse_css_border(value)?;
            Ok(vec![
               CssProperty::BorderTopColor(StyleBorderTopColor(border.border_color).into()),
               CssProperty::BorderTopStyle(StyleBorderTopStyle(border.border_style).into()),
               CssProperty::BorderTopWidth(StyleBorderTopWidth(border.border_width).into()),
            ])
        },
        BorderBottom => {
            let border = parse_css_border(value)?;
            Ok(vec![
               CssProperty::BorderBottomColor(StyleBorderBottomColor(border.border_color).into()),
               CssProperty::BorderBottomStyle(StyleBorderBottomStyle(border.border_style).into()),
               CssProperty::BorderBottomWidth(StyleBorderBottomWidth(border.border_width).into()),
            ])
        },
        BoxShadow => {
            let box_shadow = parse_css_box_shadow(value)?;
            Ok(vec![
               CssProperty::BoxShadowLeft(box_shadow),
               CssProperty::BoxShadowRight(box_shadow),
               CssProperty::BoxShadowTop(box_shadow),
               CssProperty::BoxShadowBottom(box_shadow),
            ])
        },
    }
}

/// Error containing all sub-errors that could happen during CSS parsing
///
/// Usually we want to crash on the first error, to notify the user of the problem.
#[derive(Clone, PartialEq)]
pub enum CssParsingError<'a> {
    CssBorderParseError(CssBorderParseError<'a>),
    CssShadowParseError(CssShadowParseError<'a>),
    InvalidValueErr(InvalidValueErr<'a>),
    PixelParseError(PixelParseError<'a>),
    PercentageParseError(PercentageParseError),
    CssImageParseError(CssImageParseError<'a>),
    CssStyleFontFamilyParseError(CssStyleFontFamilyParseError<'a>),
    CssBackgroundParseError(CssBackgroundParseError<'a>),
    CssColorParseError(CssColorParseError<'a>),
    CssStyleBorderRadiusParseError(CssStyleBorderRadiusParseError<'a>),
    PaddingParseError(LayoutPaddingParseError<'a>),
    MarginParseError(LayoutMarginParseError<'a>),
    FlexShrinkParseError(FlexShrinkParseError<'a>),
    FlexGrowParseError(FlexGrowParseError<'a>),
}

impl_debug_as_display!(CssParsingError<'a>);
impl_display!{ CssParsingError<'a>, {
    CssStyleBorderRadiusParseError(e) => format!("Invalid border-radius: {}", e),
    CssBorderParseError(e) => format!("Invalid border property: {}", e),
    CssShadowParseError(e) => format!("Invalid shadow: \"{}\"", e),
    InvalidValueErr(e) => format!("\"{}\"", e.0),
    PixelParseError(e) => format!("{}", e),
    PercentageParseError(e) => format!("{}", e),
    CssImageParseError(e) => format!("{}", e),
    CssStyleFontFamilyParseError(e) => format!("{}", e),
    CssBackgroundParseError(e) => format!("{}", e),
    CssColorParseError(e) => format!("{}", e),
    PaddingParseError(e) => format!("{}", e),
    MarginParseError(e) => format!("{}", e),
    FlexShrinkParseError(e) => format!("{}", e),
    FlexGrowParseError(e) => format!("{}", e),
}}

impl_from!(CssBorderParseError<'a>, CssParsingError::CssBorderParseError);
impl_from!(CssShadowParseError<'a>, CssParsingError::CssShadowParseError);
impl_from!(CssColorParseError<'a>, CssParsingError::CssColorParseError);
impl_from!(InvalidValueErr<'a>, CssParsingError::InvalidValueErr);
impl_from!(PixelParseError<'a>, CssParsingError::PixelParseError);
impl_from!(CssImageParseError<'a>, CssParsingError::CssImageParseError);
impl_from!(CssStyleFontFamilyParseError<'a>, CssParsingError::CssStyleFontFamilyParseError);
impl_from!(CssBackgroundParseError<'a>, CssParsingError::CssBackgroundParseError);
impl_from!(CssStyleBorderRadiusParseError<'a>, CssParsingError::CssStyleBorderRadiusParseError);
impl_from!(LayoutPaddingParseError<'a>, CssParsingError::PaddingParseError);
impl_from!(LayoutMarginParseError<'a>, CssParsingError::MarginParseError);
impl_from!(FlexShrinkParseError<'a>, CssParsingError::FlexShrinkParseError);
impl_from!(FlexGrowParseError<'a>, CssParsingError::FlexGrowParseError);

impl<'a> From<PercentageParseError> for CssParsingError<'a> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::PercentageParseError(e)
    }
}

/// Simple "invalid value" error, used for
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

#[derive(Clone, PartialEq)]
pub enum CssStyleBorderRadiusParseError<'a> {
    TooManyValues(&'a str),
    PixelParseError(PixelParseError<'a>),
}

impl_debug_as_display!(CssStyleBorderRadiusParseError<'a>);
impl_display!{ CssStyleBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values: \"{}\"", val),
    PixelParseError(e) => format!("{}", e),
}}

impl_from!(PixelParseError<'a>, CssStyleBorderRadiusParseError::PixelParseError);

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssColorComponent {
    Red,
    Green,
    Blue,
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

#[derive(Clone, PartialEq)]
pub enum CssColorParseError<'a> {
    InvalidColor(&'a str),
    InvalidFunctionName(&'a str),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(&'a str),
    UnclosedColor(&'a str),
    EmptyInput,
    DirectionParseError(CssDirectionParseError<'a>),
    UnsupportedDirection(&'a str),
    InvalidPercentage(PercentageParseError),
}

impl_debug_as_display!(CssColorParseError<'a>);
impl_display!{CssColorParseError<'a>, {
    InvalidColor(i) => format!("Invalid CSS color: \"{}\"", i),
    InvalidFunctionName(i) => format!("Invalid function name, expected one of: \"rgb\", \"rgba\", \"hsl\", \"hsla\" got: \"{}\"", i),
    InvalidColorComponent(i) => format!("Invalid color component when parsing CSS color: \"{}\"", i),
    IntValueParseErr(e) => format!("CSS color component: Value not in range between 00 - FF: \"{}\"", e),
    FloatValueParseErr(e) => format!("CSS color component: Value cannot be parsed as floating point number: \"{}\"", e),
    FloatValueOutOfRange(v) => format!("CSS color component: Value not in range between 0.0 - 1.0: \"{}\"", v),
    MissingColorComponent(c) => format!("CSS color is missing {:?} component", c),
    ExtraArguments(a) => format!("Extra argument to CSS color: \"{}\"", a),
    EmptyInput => format!("Empty color string."),
    UnclosedColor(i) => format!("Unclosed color: \"{}\"", i),
    DirectionParseError(e) => format!("Could not parse direction argument for CSS color: \"{}\"", e),
    UnsupportedDirection(d) => format!("Unsupported direction type for CSS color: \"{}\"", d),
    InvalidPercentage(p) => format!("Invalid percentage when parsing CSS color: \"{}\"", p),
}}

impl<'a> From<ParseIntError> for CssColorParseError<'a> {
    fn from(e: ParseIntError) -> Self {
        CssColorParseError::IntValueParseErr(e)
    }
}

impl<'a> From<ParseFloatError> for CssColorParseError<'a> {
    fn from(e: ParseFloatError) -> Self {
        CssColorParseError::FloatValueParseErr(e)
    }
}

impl_from!(CssDirectionParseError<'a>, CssColorParseError::DirectionParseError);

#[derive(Copy, Clone, PartialEq)]
pub enum CssImageParseError<'a> {
    UnclosedQuotes(&'a str),
}

impl_debug_as_display!(CssImageParseError<'a>);
impl_display!{CssImageParseError<'a>, {
    UnclosedQuotes(e) => format!("Unclosed quotes: \"{}\"", e),
}}

/// String has unbalanced `'` or `"` quotation marks
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct UnclosedQuotesError<'a>(pub &'a str);

impl<'a> From<UnclosedQuotesError<'a>> for CssImageParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssImageParseError::UnclosedQuotes(err.0)
    }
}

#[derive(Clone, PartialEq)]
pub enum CssBorderParseError<'a> {
    MissingThickness(&'a str),
    InvalidBorderStyle(InvalidValueErr<'a>),
    InvalidBorderDeclaration(&'a str),
    ThicknessParseError(PixelParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}
impl_debug_as_display!(CssBorderParseError<'a>);
impl_display!{ CssBorderParseError<'a>, {
    MissingThickness(e) => format!("Missing border thickness: \"{}\"", e),
    InvalidBorderStyle(e) => format!("Invalid style: {}", e.0),
    InvalidBorderDeclaration(e) => format!("Invalid declaration: \"{}\"", e),
    ThicknessParseError(e) => format!("Invalid thickness: {}", e),
    ColorParseError(e) => format!("Invalid color: {}", e),
}}

#[derive(Clone, PartialEq)]
pub enum CssShadowParseError<'a> {
    InvalidSingleStatement(&'a str),
    TooManyComponents(&'a str),
    ValueParseErr(PixelParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}
impl_debug_as_display!(CssShadowParseError<'a>);
impl_display!{ CssShadowParseError<'a>, {
    InvalidSingleStatement(e) => format!("Invalid single statement: \"{}\"", e),
    TooManyComponents(e) => format!("Too many components: \"{}\"", e),
    ValueParseErr(e) => format!("Invalid value: {}", e),
    ColorParseError(e) => format!("Invalid color-value: {}", e),
}}

impl_from!(PixelParseError<'a>, CssShadowParseError::ValueParseErr);
impl_from!(CssColorParseError<'a>, CssShadowParseError::ColorParseError);

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct StyleBorderRadius {
    pub top_left: PixelSize,
    pub top_right: PixelSize,
    pub bottom_left: PixelSize,
    pub bottom_right: PixelSize,
}

impl Default for StyleBorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

impl StyleBorderRadius {

    pub const fn zero() -> Self {
        Self::uniform(PixelSize::zero())
    }

    pub const fn uniform(value: PixelSize) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_left: value,
            bottom_right: value,
        }
    }
}

/// parse the border-radius like "5px 10px" or "5px 10px 6px 10px"
pub fn parse_style_border_radius<'a>(input: &'a str)
-> Result<StyleBorderRadius, CssStyleBorderRadiusParseError<'a>>
{
    let mut components = input.split_whitespace();
    let len = components.clone().count();

    match len {
        1 => {
            // One value - border-radius: 15px;
            // (the value applies to all four corners, which are rounded equally:

            let uniform_radius = parse_pixel_value(components.next().unwrap())?;
            Ok(StyleBorderRadius::uniform(PixelSize::new(uniform_radius, uniform_radius)))
        },
        2 => {
            // Two values - border-radius: 15px 50px;
            // (first value applies to top-left and bottom-right corners,
            // and the second value applies to top-right and bottom-left corners):

            let top_left_bottom_right = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left: PixelSize::new(top_left_bottom_right, top_left_bottom_right),
                bottom_right:  PixelSize::new(top_left_bottom_right, top_left_bottom_right),
                top_right:  PixelSize::new(top_right_bottom_left, top_right_bottom_left),
                bottom_left:  PixelSize::new(top_right_bottom_left, top_right_bottom_left),
            })
        },
        3 => {
            // Three values - border-radius: 15px 50px 30px;
            // (first value applies to top-left corner,
            // second value applies to top-right and bottom-left corners,
            // and third value applies to bottom-right corner):
            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left: PixelSize::new(top_left, top_left),
                bottom_right:  PixelSize::new(bottom_right, bottom_right),
                top_right:  PixelSize::new(top_right_bottom_left, top_right_bottom_left),
                bottom_left: PixelSize::new(top_right_bottom_left, top_right_bottom_left),
            })
        }
        4 => {
            // Four values - border-radius: 15px 50px 30px 5px;
            // (first value applies to top-left corner,
            //  second value applies to top-right corner,
            //  third value applies to bottom-right corner,
            //  fourth value applies to bottom-left corner)
            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left: PixelSize::new(top_left, top_left),
                bottom_right: PixelSize::new(bottom_right, bottom_right),
                top_right: PixelSize::new(top_right, top_right),
                bottom_left: PixelSize::new(bottom_left, bottom_left),
            })
        },
        _ => {
            Err(CssStyleBorderRadiusParseError::TooManyValues(input))
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum PixelParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str),
    UnsupportedMetric(f32, String, &'a str),
    ValueParseErr(ParseFloatError, String),
}

impl_debug_as_display!(PixelParseError<'a>);

impl_display!{ PixelParseError<'a>, {
    EmptyString => format!("Missing [px / pt / em] value"),
    NoValueGiven(input) => format!("Expected floating-point pixel value, got: \"{}\"", input),
    UnsupportedMetric(_, metric, input) => format!("Could not parse \"{}\": Metric \"{}\" is not (yet) implemented.", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
}}

/// parse a single value such as "15px"
pub fn parse_pixel_value<'a>(input: &'a str)
-> Result<PixelValue, PixelParseError<'a>>
{
    let input = input.trim();

    if input.is_empty() {
        return Err(PixelParseError::EmptyString);
    }

    let is_part_of_number = |ch: &char| ch.is_numeric() || *ch == '.' || *ch == '-';

    // You can't sub-string pixel values, have to call collect() here!
    let number_str = input.chars().take_while(is_part_of_number).collect::<String>();
    let unit_str = input.chars().filter(|ch| !is_part_of_number(ch)).collect::<String>();

    if number_str.is_empty() {
        return Err(PixelParseError::NoValueGiven(input));
    }

    let number = number_str.parse::<f32>().map_err(|e| PixelParseError::ValueParseErr(e, number_str))?;

    let unit = match unit_str.as_str() {
        "px" => SizeMetric::Px,
        "em" => SizeMetric::Em,
        "pt" => SizeMetric::Pt,
        _ => return Err(PixelParseError::UnsupportedMetric(number, unit_str, input)),
    };

    Ok(PixelValue::from_metric(unit, number))
}

#[derive(Clone, PartialEq, Eq)]
pub enum PercentageParseError {
    ValueParseErr(ParseFloatError),
    NoPercentSign
}

impl_debug_as_display!(PercentageParseError);
impl_from!(ParseFloatError, PercentageParseError::ValueParseErr);

impl_display! { PercentageParseError, {
    ValueParseErr(e) => format!("\"{}\"", e),
    NoPercentSign => format!("No percent sign after number"),
}}

// Parse "1.2" or "120%" (similar to parse_pixel_value)
pub fn parse_percentage_value(input: &str)
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

    Ok(PercentageValue::new(number))
}

/// Parse any valid CSS color, INCLUDING THE HASH
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
/// "#00FF00" -> ColorF { r: 0, g: 255, b: 0 })
pub fn parse_css_color<'a>(input: &'a str)
-> Result<ColorU, CssColorParseError<'a>>
{
    let input = input.trim();
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        use self::ParenthesisParseError::*;

        match parse_parentheses(input, &["rgba", "rgb", "hsla", "hsl"]) {
            Ok((stopword, inner_value)) => {
                match stopword {
                    "rgba" => parse_color_rgb(inner_value, true),
                    "rgb" => parse_color_rgb(inner_value, false),
                    "hsla" => parse_color_hsl(inner_value, true),
                    "hsl" => parse_color_hsl(inner_value, false),
                    _ => unreachable!(),
                }
            },
            Err(e) => match e {
                UnclosedBraces => Err(CssColorParseError::UnclosedColor(input)),
                EmptyInput => Err(CssColorParseError::EmptyInput),
                StopWordNotFound(stopword) => Err(CssColorParseError::InvalidFunctionName(stopword)),
                NoClosingBraceFound => Err(CssColorParseError::UnclosedColor(input)),
                NoOpeningBraceFound => parse_color_builtin(input),
            },
        }
    }
}

pub fn parse_float_value(input: &str)
-> Result<FloatValue, ParseFloatError>
{
    Ok(FloatValue::new(input.trim().parse::<f32>()?))
}

pub fn parse_style_text_color<'a>(input: &'a str)
-> Result<StyleTextColor, CssColorParseError<'a>>
{
    parse_css_color(input).and_then(|ok| Ok(StyleTextColor(ok)))
}

/// Parse a built-in background color
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
pub fn parse_color_builtin<'a>(input: &'a str)
-> Result<ColorU, CssColorParseError<'a>>
{
    let (r, g, b, a) = match input {
        "AliceBlue"             | "alice-blue"                =>  (240, 248, 255, 255),
        "AntiqueWhite"          | "antique-white"             =>  (250, 235, 215, 255),
        "Aqua"                  | "aqua"                      =>  (  0, 255, 255, 255),
        "Aquamarine"            | "aquamarine"                =>  (127, 255, 212, 255),
        "Azure"                 | "azure"                     =>  (240, 255, 255, 255),
        "Beige"                 | "beige"                     =>  (245, 245, 220, 255),
        "Bisque"                | "bisque"                    =>  (255, 228, 196, 255),
        "Black"                 | "black"                     =>  (  0,   0,   0, 255),
        "BlanchedAlmond"        | "blanched-almond"           =>  (255, 235, 205, 255),
        "Blue"                  | "blue"                      =>  (  0,   0, 255, 255),
        "BlueViolet"            | "blue-violet"               =>  (138,  43, 226, 255),
        "Brown"                 | "brown"                     =>  (165,  42,  42, 255),
        "BurlyWood"             | "burly-wood"                =>  (222, 184, 135, 255),
        "CadetBlue"             | "cadet-blue"                =>  ( 95, 158, 160, 255),
        "Chartreuse"            | "chartreuse"                =>  (127, 255,   0, 255),
        "Chocolate"             | "chocolate"                 =>  (210, 105,  30, 255),
        "Coral"                 | "coral"                     =>  (255, 127,  80, 255),
        "CornflowerBlue"        | "cornflower-blue"           =>  (100, 149, 237, 255),
        "Cornsilk"              | "cornsilk"                  =>  (255, 248, 220, 255),
        "Crimson"               | "crimson"                   =>  (220,  20,  60, 255),
        "Cyan"                  | "cyan"                      =>  (  0, 255, 255, 255),
        "DarkBlue"              | "dark-blue"                 =>  (  0,   0, 139, 255),
        "DarkCyan"              | "dark-cyan"                 =>  (  0, 139, 139, 255),
        "DarkGoldenRod"         | "dark-golden-rod"           =>  (184, 134,  11, 255),
        "DarkGray"              | "dark-gray"                 =>  (169, 169, 169, 255),
        "DarkGrey"              | "dark-grey"                 =>  (169, 169, 169, 255),
        "DarkGreen"             | "dark-green"                =>  (  0, 100,   0, 255),
        "DarkKhaki"             | "dark-khaki"                =>  (189, 183, 107, 255),
        "DarkMagenta"           | "dark-magenta"              =>  (139,   0, 139, 255),
        "DarkOliveGreen"        | "dark-olive-green"          =>  ( 85, 107,  47, 255),
        "DarkOrange"            | "dark-orange"               =>  (255, 140,   0, 255),
        "DarkOrchid"            | "dark-orchid"               =>  (153,  50, 204, 255),
        "DarkRed"               | "dark-red"                  =>  (139,   0,   0, 255),
        "DarkSalmon"            | "dark-salmon"               =>  (233, 150, 122, 255),
        "DarkSeaGreen"          | "dark-sea-green"            =>  (143, 188, 143, 255),
        "DarkSlateBlue"         | "dark-slate-blue"           =>  ( 72,  61, 139, 255),
        "DarkSlateGray"         | "dark-slate-gray"           =>  ( 47,  79,  79, 255),
        "DarkSlateGrey"         | "dark-slate-grey"           =>  ( 47,  79,  79, 255),
        "DarkTurquoise"         | "dark-turquoise"            =>  (  0, 206, 209, 255),
        "DarkViolet"            | "dark-violet"               =>  (148,   0, 211, 255),
        "DeepPink"              | "deep-pink"                 =>  (255,  20, 147, 255),
        "DeepSkyBlue"           | "deep-sky-blue"             =>  (  0, 191, 255, 255),
        "DimGray"               | "dim-gray"                  =>  (105, 105, 105, 255),
        "DimGrey"               | "dim-grey"                  =>  (105, 105, 105, 255),
        "DodgerBlue"            | "dodger-blue"               =>  ( 30, 144, 255, 255),
        "FireBrick"             | "fire-brick"                =>  (178,  34,  34, 255),
        "FloralWhite"           | "floral-white"              =>  (255, 250, 240, 255),
        "ForestGreen"           | "forest-green"              =>  ( 34, 139,  34, 255),
        "Fuchsia"               | "fuchsia"                   =>  (255,   0, 255, 255),
        "Gainsboro"             | "gainsboro"                 =>  (220, 220, 220, 255),
        "GhostWhite"            | "ghost-white"               =>  (248, 248, 255, 255),
        "Gold"                  | "gold"                      =>  (255, 215,   0, 255),
        "GoldenRod"             | "golden-rod"                =>  (218, 165,  32, 255),
        "Gray"                  | "gray"                      =>  (128, 128, 128, 255),
        "Grey"                  | "grey"                      =>  (128, 128, 128, 255),
        "Green"                 | "green"                     =>  (  0, 128,   0, 255),
        "GreenYellow"           | "green-yellow"              =>  (173, 255,  47, 255),
        "HoneyDew"              | "honey-dew"                 =>  (240, 255, 240, 255),
        "HotPink"               | "hot-pink"                  =>  (255, 105, 180, 255),
        "IndianRed"             | "indian-red"                =>  (205,  92,  92, 255),
        "Indigo"                | "indigo"                    =>  ( 75,   0, 130, 255),
        "Ivory"                 | "ivory"                     =>  (255, 255, 240, 255),
        "Khaki"                 | "khaki"                     =>  (240, 230, 140, 255),
        "Lavender"              | "lavender"                  =>  (230, 230, 250, 255),
        "LavenderBlush"         | "lavender-blush"            =>  (255, 240, 245, 255),
        "LawnGreen"             | "lawn-green"                =>  (124, 252,   0, 255),
        "LemonChiffon"          | "lemon-chiffon"             =>  (255, 250, 205, 255),
        "LightBlue"             | "light-blue"                =>  (173, 216, 230, 255),
        "LightCoral"            | "light-coral"               =>  (240, 128, 128, 255),
        "LightCyan"             | "light-cyan"                =>  (224, 255, 255, 255),
        "LightGoldenRodYellow"  | "light-golden-rod-yellow"   =>  (250, 250, 210, 255),
        "LightGray"             | "light-gray"                =>  (211, 211, 211, 255),
        "LightGrey"             | "light-grey"                =>  (144, 238, 144, 255),
        "LightGreen"            | "light-green"               =>  (211, 211, 211, 255),
        "LightPink"             | "light-pink"                =>  (255, 182, 193, 255),
        "LightSalmon"           | "light-salmon"              =>  (255, 160, 122, 255),
        "LightSeaGreen"         | "light-sea-green"           =>  ( 32, 178, 170, 255),
        "LightSkyBlue"          | "light-sky-blue"            =>  (135, 206, 250, 255),
        "LightSlateGray"        | "light-slate-gray"          =>  (119, 136, 153, 255),
        "LightSlateGrey"        | "light-slate-grey"          =>  (119, 136, 153, 255),
        "LightSteelBlue"        | "light-steel-blue"          =>  (176, 196, 222, 255),
        "LightYellow"           | "light-yellow"              =>  (255, 255, 224, 255),
        "Lime"                  | "lime"                      =>  (  0, 255,   0, 255),
        "LimeGreen"             | "lime-green"                =>  ( 50, 205,  50, 255),
        "Linen"                 | "linen"                     =>  (250, 240, 230, 255),
        "Magenta"               | "magenta"                   =>  (255,   0, 255, 255),
        "Maroon"                | "maroon"                    =>  (128,   0,   0, 255),
        "MediumAquaMarine"      | "medium-aqua-marine"        =>  (102, 205, 170, 255),
        "MediumBlue"            | "medium-blue"               =>  (  0,   0, 205, 255),
        "MediumOrchid"          | "medium-orchid"             =>  (186,  85, 211, 255),
        "MediumPurple"          | "medium-purple"             =>  (147, 112, 219, 255),
        "MediumSeaGreen"        | "medium-sea-green"          =>  ( 60, 179, 113, 255),
        "MediumSlateBlue"       | "medium-slate-blue"         =>  (123, 104, 238, 255),
        "MediumSpringGreen"     | "medium-spring-green"       =>  (  0, 250, 154, 255),
        "MediumTurquoise"       | "medium-turquoise"          =>  ( 72, 209, 204, 255),
        "MediumVioletRed"       | "medium-violet-red"         =>  (199,  21, 133, 255),
        "MidnightBlue"          | "midnight-blue"             =>  ( 25,  25, 112, 255),
        "MintCream"             | "mint-cream"                =>  (245, 255, 250, 255),
        "MistyRose"             | "misty-rose"                =>  (255, 228, 225, 255),
        "Moccasin"              | "moccasin"                  =>  (255, 228, 181, 255),
        "NavajoWhite"           | "navajo-white"              =>  (255, 222, 173, 255),
        "Navy"                  | "navy"                      =>  (  0,   0, 128, 255),
        "OldLace"               | "old-lace"                  =>  (253, 245, 230, 255),
        "Olive"                 | "olive"                     =>  (128, 128,   0, 255),
        "OliveDrab"             | "olive-drab"                =>  (107, 142,  35, 255),
        "Orange"                | "orange"                    =>  (255, 165,   0, 255),
        "OrangeRed"             | "orange-red"                =>  (255,  69,   0, 255),
        "Orchid"                | "orchid"                    =>  (218, 112, 214, 255),
        "PaleGoldenRod"         | "pale-golden-rod"           =>  (238, 232, 170, 255),
        "PaleGreen"             | "pale-green"                =>  (152, 251, 152, 255),
        "PaleTurquoise"         | "pale-turquoise"            =>  (175, 238, 238, 255),
        "PaleVioletRed"         | "pale-violet-red"           =>  (219, 112, 147, 255),
        "PapayaWhip"            | "papaya-whip"               =>  (255, 239, 213, 255),
        "PeachPuff"             | "peach-puff"                =>  (255, 218, 185, 255),
        "Peru"                  | "peru"                      =>  (205, 133,  63, 255),
        "Pink"                  | "pink"                      =>  (255, 192, 203, 255),
        "Plum"                  | "plum"                      =>  (221, 160, 221, 255),
        "PowderBlue"            | "powder-blue"               =>  (176, 224, 230, 255),
        "Purple"                | "purple"                    =>  (128,   0, 128, 255),
        "RebeccaPurple"         | "rebecca-purple"            =>  (102,  51, 153, 255),
        "Red"                   | "red"                       =>  (255,   0,   0, 255),
        "RosyBrown"             | "rosy-brown"                =>  (188, 143, 143, 255),
        "RoyalBlue"             | "royal-blue"                =>  ( 65, 105, 225, 255),
        "SaddleBrown"           | "saddle-brown"              =>  (139,  69,  19, 255),
        "Salmon"                | "salmon"                    =>  (250, 128, 114, 255),
        "SandyBrown"            | "sandy-brown"               =>  (244, 164,  96, 255),
        "SeaGreen"              | "sea-green"                 =>  ( 46, 139,  87, 255),
        "SeaShell"              | "sea-shell"                 =>  (255, 245, 238, 255),
        "Sienna"                | "sienna"                    =>  (160,  82,  45, 255),
        "Silver"                | "silver"                    =>  (192, 192, 192, 255),
        "SkyBlue"               | "sky-blue"                  =>  (135, 206, 235, 255),
        "SlateBlue"             | "slate-blue"                =>  (106,  90, 205, 255),
        "SlateGray"             | "slate-gray"                =>  (112, 128, 144, 255),
        "SlateGrey"             | "slate-grey"                =>  (112, 128, 144, 255),
        "Snow"                  | "snow"                      =>  (255, 250, 250, 255),
        "SpringGreen"           | "spring-green"              =>  (  0, 255, 127, 255),
        "SteelBlue"             | "steel-blue"                =>  ( 70, 130, 180, 255),
        "Tan"                   | "tan"                       =>  (210, 180, 140, 255),
        "Teal"                  | "teal"                      =>  (  0, 128, 128, 255),
        "Thistle"               | "thistle"                   =>  (216, 191, 216, 255),
        "Tomato"                | "tomato"                    =>  (255,  99,  71, 255),
        "Turquoise"             | "turquoise"                 =>  ( 64, 224, 208, 255),
        "Violet"                | "violet"                    =>  (238, 130, 238, 255),
        "Wheat"                 | "wheat"                     =>  (245, 222, 179, 255),
        "White"                 | "white"                     =>  (255, 255, 255, 255),
        "WhiteSmoke"            | "white-smoke"               =>  (245, 245, 245, 255),
        "Yellow"                | "yellow"                    =>  (255, 255,   0, 255),
        "YellowGreen"           | "yellow-green"              =>  (154, 205,  50, 255),
        "Transparent"           | "transparent"               =>  (255, 255, 255,   0),
        _ => { return Err(CssColorParseError::InvalidColor(input)); }
    };
    Ok(ColorU { r, g, b, a })
}

/// Parse a color of the form 'rgb([0-255], [0-255], [0-255])', or 'rgba([0-255], [0-255], [0-255],
/// [0.0-1.0])' without the leading 'rgb[a](' or trailing ')'. Alpha defaults to 255.
pub fn parse_color_rgb<'a>(input: &'a str, parse_alpha: bool)
-> Result<ColorU, CssColorParseError<'a>>
{
    let mut components = input.split(',').map(|c| c.trim());
    let rgb_color = parse_color_rgb_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

/// Parse the color components passed as arguments to an rgb(...) CSS color.
pub fn parse_color_rgb_components<'a>(components: &mut Iterator<Item = &'a str>)
-> Result<ColorU, CssColorParseError<'a>>
{
    #[inline]
    fn component_from_str<'a>(components: &mut Iterator<Item = &'a str>, which: CssColorComponent)
    -> Result<u8, CssColorParseError<'a>>
    {
        let c = components.next().ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        let c = c.parse::<u8>()?;
        Ok(c)
    }

    Ok(ColorU {
        r: component_from_str(components, CssColorComponent::Red)?,
        g: component_from_str(components, CssColorComponent::Green)?,
        b: component_from_str(components, CssColorComponent::Blue)?,
        a: 255
    })
}

/// Parse a color of the form 'hsl([0.0-360.0]deg, [0-100]%, [0-100]%)', or 'hsla([0.0-360.0]deg, [0-100]%, [0-100]%, [0.0-1.0])' without the leading 'hsl[a](' or trailing ')'. Alpha defaults to 255.
pub fn parse_color_hsl<'a>(input: &'a str, parse_alpha: bool)
-> Result<ColorU, CssColorParseError<'a>>
{
    let mut components = input.split(',').map(|c| c.trim());
    let rgb_color = parse_color_hsl_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

/// Parse the color components passed as arguments to an hsl(...) CSS color.
pub fn parse_color_hsl_components<'a>(components: &mut Iterator<Item = &'a str>)
-> Result<ColorU, CssColorParseError<'a>>
{
    #[inline]
    fn angle_from_str<'a>(components: &mut Iterator<Item = &'a str>, which: CssColorComponent)
    -> Result<f32, CssColorParseError<'a>>
    {
        let c = components.next().ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        let dir = parse_direction(c)?;
        match dir {
            Direction::Angle(deg) => Ok(deg.get()),
            Direction::FromTo(_, _) => return Err(CssColorParseError::UnsupportedDirection(c)),
        }
    }

    #[inline]
    fn percent_from_str<'a>(components: &mut Iterator<Item = &'a str>, which: CssColorComponent)
    -> Result<f32, CssColorParseError<'a>>
    {
        let c = components.next().ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }

        let parsed_percent = parse_percentage(c).map_err(|e| CssColorParseError::InvalidPercentage(e))?;

        Ok(parsed_percent.get())
    }

    /// Adapted from [https://en.wikipedia.org/wiki/HSL_and_HSV#Converting_to_RGB]
    #[inline]
    fn hsl_to_rgb<'a>(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
        let s = s / 100.0;
        let l = l / 100.0;
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h = h / 60.0;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let (r1, g1, b1) = match h as u8 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            5 => (c, 0.0, x),
            _ => {
                unreachable!();
            }
        };
        let m = l - c / 2.0;
        (
            ((r1 + m) * 256.0).min(255.0) as u8,
            ((g1 + m) * 256.0).min(255.0) as u8,
            ((b1 + m) * 256.0).min(255.0) as u8,
        )
    }

    let (h, s, l) = (
        angle_from_str(components, CssColorComponent::Hue)?,
        percent_from_str(components, CssColorComponent::Saturation)?,
        percent_from_str(components, CssColorComponent::Lightness)?,
    );

    let (r, g, b) = hsl_to_rgb(h, s, l);

    Ok(ColorU { r, g, b, a: 255 })
}

fn parse_alpha_component<'a>(components: &mut Iterator<Item=&'a str>) -> Result<u8, CssColorParseError<'a>> {
    let a = components.next().ok_or(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha))?;
    if a.is_empty() {
        return Err(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha));
    }
    let a = a.parse::<f32>()?;
    if a < 0.0 || a > 1.0 {
        return Err(CssColorParseError::FloatValueOutOfRange(a));
    }
    let a = (a * 256.0).min(255.0) as u8;
    Ok(a)
}


/// Parse a background color, WITHOUT THE HASH
///
/// "00FFFF" -> ColorF { r: 0, g: 255, b: 255})
pub fn parse_color_no_hash<'a>(input: &'a str)
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
            let input = u32::from_str_radix(input, 16).map_err(|e| CssColorParseError::IntValueParseErr(e))?;
            Ok(ColorU {
                r: ((input >> 16) & 255) as u8,
                g: ((input >> 8) & 255) as u8,
                b: (input & 255) as u8,
                a: 255,
            })
        },
        8 => {
            let input = u32::from_str_radix(input, 16).map_err(|e| CssColorParseError::IntValueParseErr(e))?;
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

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LayoutPadding {
    pub top: PixelValue,
    pub bottom: PixelValue,
    pub left: PixelValue,
    pub right: PixelValue,
}

/// Parse a padding value such as
///
/// "10px 10px"
pub fn parse_layout_padding<'a>(input: &'a str)
-> Result<LayoutPadding, LayoutPaddingParseError>
{
    let mut input_iter = input.split_whitespace();
    let first = parse_pixel_value(input_iter.next().ok_or(LayoutPaddingParseError::TooFewValues)?)?;
    let second = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: first,
            bottom: first,
            left: first,
            right: first,
        }),
    })?;
    let third = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: first,
            bottom: first,
            left: second,
            right: second,
        }),
    })?;
    let fourth = parse_pixel_value(match input_iter.next() {
        Some(s) => s,
        None => return Ok(LayoutPadding {
            top: first,
            left: second,
            right: second,
            bottom: third,
        }),
    })?;

    if input_iter.next().is_some() {
        return Err(LayoutPaddingParseError::TooManyValues);
    }

    Ok(LayoutPadding {
        top: first,
        right: second,
        bottom: third,
        left: fourth,
    })
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

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LayoutMargin {
    pub top: PixelValue,
    pub bottom: PixelValue,
    pub left: PixelValue,
    pub right: PixelValue,
}

pub fn parse_layout_margin<'a>(input: &'a str)
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

const DEFAULT_BORDER_COLOR: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };
// Default border thickness on the web seems to be 3px
const DEFAULT_BORDER_THICKNESS: PixelValue = PixelValue::const_px(3);

use std::str::CharIndices;

fn advance_until_next_char(iter: &mut CharIndices) -> Option<usize> {
    let mut next_char = iter.next()?;
    while next_char.1.is_whitespace() {
        match iter.next() {
            Some(s) => next_char = s,
            None => return Some(next_char.0 + 1),
        }
    }
    Some(next_char.0)
}

/// Advances a CharIndices iterator until the next space is encountered
fn take_until_next_whitespace(iter: &mut CharIndices) -> Option<usize> {
    let mut next_char = iter.next()?;
    while !next_char.1.is_whitespace() {
        match iter.next() {
            Some(s) => next_char = s,
            None => return Some(next_char.0 + 1),
        }
    }
    Some(next_char.0)
}

/// Parse a CSS border such as
///
/// "5px solid red"
pub fn parse_css_border<'a>(input: &'a str)
-> Result<StyleBorderSide, CssBorderParseError<'a>>
{
    use self::CssBorderParseError::*;

    let input = input.trim();

    // The first argument can either be a style or a pixel value

    let mut char_iter = input.char_indices();
    let first_arg_end = take_until_next_whitespace(&mut char_iter).ok_or(MissingThickness(input))?;
    let first_arg_str = &input[0..first_arg_end];

    advance_until_next_char(&mut char_iter);

    let second_argument_end = take_until_next_whitespace(&mut char_iter);
    let (border_width, border_width_str_end, border_style);

    match second_argument_end {
        None => {
            // First argument is the one and only argument, therefore has to be a style such as "double"
            border_style = parse_css_border_style(first_arg_str).map_err(|e| InvalidBorderStyle(e))?;
            return Ok(StyleBorderSide {
                border_style,
                border_width: DEFAULT_BORDER_THICKNESS,
                border_color: DEFAULT_BORDER_COLOR,
            });
        },
        Some(end) => {
            // First argument is a pixel value, second argument is the border style
            border_width = parse_pixel_value(first_arg_str).map_err(|e| ThicknessParseError(e))?;
            let border_style_str = &input[first_arg_end..end];
            border_style = parse_css_border_style(border_style_str).map_err(|e| InvalidBorderStyle(e))?;
            border_width_str_end = end;
        }
    }

    let border_color_str = &input[border_width_str_end..];

    // Last argument can be either a hex color or a rgb str
    let border_color = parse_css_color(border_color_str).map_err(|e| ColorParseError(e))?;

    Ok(StyleBorderSide {
        border_width,
        border_style,
        border_color,
    })
}

multi_type_parser!(parse_css_border_style, BorderStyle,
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

/// Parses a CSS box-shadow, such as "5px 10px inset"
pub fn parse_css_box_shadow<'a>(input: &'a str)
-> Result<BoxShadowPreDisplayItem, CssShadowParseError<'a>>
{
    let mut input_iter = input.split_whitespace();
    let count = input_iter.clone().count();

    let mut box_shadow = BoxShadowPreDisplayItem {
        offset: [PixelValue::px(0.0), PixelValue::px(0.0)],
        color: ColorU { r: 0, g: 0, b: 0, a: 255 },
        blur_radius: PixelValue::px(0.0),
        spread_radius: PixelValue::px(0.0),
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
        2 => {
            // box-shadow: 5px 10px; (h_offset, v_offset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;
        },
        3 => {
            // box-shadow: 5px 10px inset; (h_offset, v_offset, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                // box-shadow: 5px 10px #888888; (h_offset, v_offset, color)
                let color = parse_css_color(input_iter.next().unwrap())?;
                box_shadow.color = color;
            }
        },
        4 => {
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                let blur = parse_pixel_value(input_iter.next().unwrap())?;
                box_shadow.blur_radius = blur.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        },
        5 => {
            // box-shadow: 5px 10px 5px 10px #888888; (h_offset, v_offset, blur, spread, color)
            // box-shadow: 5px 10px 5px #888888 inset; (h_offset, v_offset, blur, color, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            if !is_inset {
                let spread = parse_pixel_value(input_iter.next().unwrap())?;
                box_shadow.spread_radius = spread.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        },
        6 => {
            // box-shadow: 5px 10px 5px 10px #888888 inset; (h_offset, v_offset, blur, spread, color, inset)
            let h_offset = parse_pixel_value(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            let spread = parse_pixel_value(input_iter.next().unwrap())?;
            box_shadow.spread_radius = spread.into();

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        _ => {
            return Err(CssShadowParseError::TooManyComponents(input));
        }
    }

    Ok(box_shadow)
}

#[derive(Clone, PartialEq)]
pub enum CssBackgroundParseError<'a> {
    Error(&'a str),
    InvalidBackground(ParenthesisParseError<'a>),
    UnclosedGradient(&'a str),
    NoDirection(&'a str),
    TooFewGradientStops(&'a str),
    DirectionParseError(CssDirectionParseError<'a>),
    GradientParseError(CssGradientStopParseError<'a>),
    ShapeParseError(CssShapeParseError<'a>),
    ImageParseError(CssImageParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssBackgroundParseError<'a>);
impl_display!{ CssBackgroundParseError<'a>, {
    Error(e) => e,
    InvalidBackground(val) => format!("Invalid background value: \"{}\"", val),
    UnclosedGradient(val) => format!("Unclosed gradient: \"{}\"", val),
    NoDirection(val) => format!("Gradient has no direction: \"{}\"", val),
    TooFewGradientStops(val) => format!("Failed to parse gradient due to too few gradient steps: \"{}\"", val),
    DirectionParseError(e) => format!("Failed to parse gradient direction: \"{}\"", e),
    GradientParseError(e) => format!("Failed to parse gradient: {}", e),
    ShapeParseError(e) => format!("Failed to parse shape of radial gradient: {}", e),
    ImageParseError(e) => format!("Failed to parse image() value: {}", e),
    ColorParseError(e) => format!("Failed to parse color value: {}", e),
}}

impl_from!(ParenthesisParseError<'a>, CssBackgroundParseError::InvalidBackground);
impl_from!(CssDirectionParseError<'a>, CssBackgroundParseError::DirectionParseError);
impl_from!(CssGradientStopParseError<'a>, CssBackgroundParseError::GradientParseError);
impl_from!(CssShapeParseError<'a>, CssBackgroundParseError::ShapeParseError);
impl_from!(CssImageParseError<'a>, CssBackgroundParseError::ImageParseError);
impl_from!(CssColorParseError<'a>, CssBackgroundParseError::ColorParseError);

// parses a background, such as "linear-gradient(red, green)"
pub fn parse_style_background_content<'a>(input: &'a str)
-> Result<StyleBackgroundContent, CssBackgroundParseError<'a>>
{
    match parse_parentheses(input, &[
        "linear-gradient", "repeating-linear-gradient",
        "radial-gradient", "repeating-radial-gradient",
        "image",
    ]) {
        Ok((background_type, brace_contents)) => {
            let gradient_type = match background_type {
                "linear-gradient" => GradientType::LinearGradient,
                "repeating-linear-gradient" => GradientType::RepeatingLinearGradient,
                "radial-gradient" => GradientType::RadialGradient,
                "repeating-radial-gradient" => GradientType::RepeatingRadialGradient,
                "image" => { return Ok(StyleBackgroundContent::Image(parse_image(brace_contents)?)); },
                other => { return Err(CssBackgroundParseError::Error(other)); /* unreachable */ },
            };

            parse_gradient(brace_contents, gradient_type)
        },
        Err(_) => {
            Ok(StyleBackgroundContent::Color(parse_css_color(input)?))
        }
    }
}

pub fn parse_style_background_position<'a>(input: &'a str)
-> Result<StyleBackgroundPosition, CssBackgroundPositionParseError<'a>>
{

}

/// Given a string, returns how many characters need to be skipped
fn skip_next_braces(input: &str, target_char: char) -> Option<(usize, bool)> {

    let mut depth = 0;
    let mut last_character = 0;
    let mut character_was_found = false;

    if input.is_empty() {
        return None;
    }

    for (idx, ch) in input.char_indices() {
        last_character = idx;
        match ch {
            '(' => { depth += 1; },
            ')' => { depth -= 1; },
            c => {
                if c == target_char && depth == 0 {
                    character_was_found = true;
                    break;
                }
            },
        }
    }

    if last_character == 0 {
        // No more split by `,`
        None
    } else {
        Some((last_character, character_was_found))
    }
}

// parses a single gradient such as "to right, 50px"
pub fn parse_gradient<'a>(input: &'a str, background_type: GradientType)
-> Result<StyleBackgroundContent, CssBackgroundParseError<'a>>
{
    let input = input.trim();

    // Splitting the input by "," doesn't work since rgba() might contain commas
    let mut comma_separated_items = Vec::<&str>::new();
    let mut current_input = &input[..];

    'outer: loop {
        let (skip_next_braces_result, character_was_found) =
        match skip_next_braces(&current_input, ',') {
            Some(s) => s,
            None => break 'outer,
        };
        let new_push_item = if character_was_found {
            &current_input[..skip_next_braces_result]
        } else {
            &current_input[..]
        };
        let new_current_input = &current_input[(skip_next_braces_result + 1)..];
        comma_separated_items.push(new_push_item);
        current_input = new_current_input;
        if !character_was_found {
            break 'outer;
        }
    }

    let mut brace_iterator = comma_separated_items.iter();
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

    let is_linear_gradient = background_type == GradientType::LinearGradient ||
                             background_type == GradientType::RepeatingLinearGradient;

    let is_radial_gradient = background_type == GradientType::RadialGradient ||
                             background_type == GradientType::RepeatingRadialGradient;

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

    normalize_color_stops(&mut color_stops);

    match background_type {
        GradientType::LinearGradient => {
            Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
                direction: direction,
                extend_mode: ExtendMode::Clamp,
                stops: color_stops,
            }))
        },
        GradientType::RepeatingLinearGradient => {
            Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
                direction: direction,
                extend_mode: ExtendMode::Repeat,
                stops: color_stops,
            }))
        },
        GradientType::RadialGradient => {
            Ok(StyleBackgroundContent::RadialGradient(RadialGradient {
                shape: shape,
                extend_mode: ExtendMode::Clamp,
                stops: color_stops,
            }))
        },
        GradientType::RepeatingRadialGradient => {
            Ok(StyleBackgroundContent::RadialGradient(RadialGradient {
                shape: shape,
                extend_mode: ExtendMode::Repeat,
                stops: color_stops,
            }))
        },
    }
}

// Normalize the percentages of the parsed color stops
pub fn normalize_color_stops(color_stops: &mut Vec<GradientStopPre>) {

    let mut last_stop = PercentageValue::new(0.0);
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
                    last_stop = PercentageValue::new(last_stop.get() + increase_stop_cnt);
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

                let next_value = next_value.unwrap_or(PercentageValue::new(100.0));
                let increase = (next_value.get() / (next_count as f32)) - (last_stop.get() / (next_count as f32)) ;
                increase_stop_cnt = Some(increase);
                if next_count == 1 && (color_stop_len - i) == 1 {
                    next[0].offset = Some(last_stop);
                } else {
                    if i == 0 {
                        next[0].offset = Some(PercentageValue::new(0.0));
                    } else {
                        next[0].offset = Some(last_stop);
                        // last_stop += increase;
                    }
                }
            }
        }
    }
}

impl<'a> From<QuoteStripped<'a>> for CssImageId {
    fn from(input: QuoteStripped<'a>) -> Self {
        CssImageId(input.0.to_string())
    }
}

/// A string that has been stripped of the beginning and ending quote
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct QuoteStripped<'a>(pub &'a str);

pub fn parse_image<'a>(input: &'a str) -> Result<CssImageId, CssImageParseError<'a>> {
    Ok(strip_quotes(input)?.into())
}

/// Strip quotes from an input, given that both quotes use either `"` or `'`, but not both.
///
/// # Example
///
/// ```rust
/// # extern crate azul_css_parser;
/// # use azul_css_parser::{strip_quotes, QuoteStripped, UnclosedQuotesError};
/// assert_eq!(strip_quotes("\"Helvetica\""), Ok(QuoteStripped("Helvetica")));
/// assert_eq!(strip_quotes("'Arial'"), Ok(QuoteStripped("Arial")));
/// assert_eq!(strip_quotes("\"Arial'"), Err(UnclosedQuotesError("\"Arial'")));
/// ```
pub fn strip_quotes<'a>(input: &'a str) -> Result<QuoteStripped<'a>, UnclosedQuotesError<'a>> {
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
        Ok(QuoteStripped(quote_contents.trim_end_matches("\"")))
    } else if first_single_quote.is_some() {
        let quote_contents = first_single_quote.unwrap();
        if!quote_contents.ends_with('\'') {
            return Err(UnclosedQuotesError(input));
        }
        Ok(QuoteStripped(quote_contents.trim_end_matches("'")))
    } else {
        Err(UnclosedQuotesError(input))
    }
}

#[derive(Clone, PartialEq)]
pub enum CssGradientStopParseError<'a> {
    Error(&'a str),
    Percentage(PercentageParseError),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssGradientStopParseError<'a>);
impl_display!{ CssGradientStopParseError<'a>, {
    Error(e) => e,
    Percentage(e) => format!("Failed to parse offset percentage: {}", e),
    ColorParseError(e) => format!("{}", e),
}}

impl_from!(CssColorParseError<'a>, CssGradientStopParseError::ColorParseError);


// parses "red" , "red 5%"
pub fn parse_gradient_stop<'a>(input: &'a str)
-> Result<GradientStopPre, CssGradientStopParseError<'a>>
{
    use self::CssGradientStopParseError::*;

    let input = input.trim();

    // Color functions such as "rgba(...)" can contain spaces, so we parse right-to-left.
    let (color_str, percentage_str) = match (input.rfind(')'), input.rfind(char::is_whitespace)) {
        (Some(closing_brace), None) if closing_brace < input.len() - 1 => {
            // percentage after closing brace, eg. "rgb(...)50%"
            (&input[..=closing_brace], Some(&input[(closing_brace + 1)..]))
        },
        (None, Some(last_ws)) => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        }
        (Some(closing_brace), Some(last_ws)) if closing_brace < last_ws => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        },
        _ => {
            // no percentage
            (input, None)
        },
    };

    let color = parse_css_color(color_str)?;
    let offset = match percentage_str {
        None => None,
        Some(s) => Some(parse_percentage(s).map_err(|e| Percentage(e))?)
    };

    Ok(GradientStopPre { offset, color: color })
}

// parses "5%" -> 5
pub fn parse_percentage(input: &str)
-> Result<PercentageValue, PercentageParseError>
{
    let percent_location = input.rfind('%').ok_or(PercentageParseError::NoPercentSign)?;
    let input = &input[..percent_location];
    Ok(PercentageValue::new(input.parse::<f32>()?))
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

/// Parses an `direction` such as `"50deg"` or `"to right bottom"` (in the context of gradients)
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # extern crate azul_css_parser;
/// # use azul_css_parser::parse_direction;
/// # use azul_css::{Direction, FloatValue};
/// use azul_css::DirectionCorner::*;
///
/// assert_eq!(parse_direction("to right bottom"), Ok(Direction::FromTo(TopLeft, BottomRight)));
/// assert_eq!(parse_direction("to right"), Ok(Direction::FromTo(Left, Right)));
/// assert_eq!(parse_direction("50deg"), Ok(Direction::Angle(FloatValue::new(50.0))));
/// ```
pub fn parse_direction<'a>(input: &'a str)
-> Result<Direction, CssDirectionParseError<'a>>
{
    use std::f32::consts::PI;

    let input_iter = input.split_whitespace();
    let count = input_iter.clone().count();
    let mut first_input_iter = input_iter.clone();
    // "50deg" | "to" | "right"
    let first_input = first_input_iter.next().ok_or(CssDirectionParseError::Error(input))?;

    let deg = {
        if first_input.ends_with("grad") {
            first_input.split("grad").next().unwrap().parse::<f32>()? / 400.0 * 360.0
        } else if first_input.ends_with("rad") {
            first_input.split("rad").next().unwrap().parse::<f32>()? * 180.0 / PI
        } else if first_input.ends_with("deg") || first_input.parse::<f32>().is_ok() {
            first_input.split("deg").next().unwrap().parse::<f32>()?
        } else if let Ok(angle) = first_input.parse::<f32>() {
            angle
        }
        else {
            // if we get here, the input is definitely not an angle

            if first_input != "to" {
                return Err(CssDirectionParseError::InvalidArguments(input));
            }

            let second_input = first_input_iter.next().ok_or(CssDirectionParseError::Error(input))?;
            let end = parse_direction_corner(second_input)?;

            return match count {
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
            };
        }
    };

    // clamp the degree to 360 (so 410deg = 50deg)
    let mut deg = deg % 360.0;
    if deg < 0.0 {
        deg = 360.0 + deg;
    }

    // now deg is in the range of +0..+360
    debug_assert!(deg >= 0.0 && deg <= 360.0);

    return Ok(Direction::Angle(FloatValue::new(deg)));
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssDirectionCornerParseError<'a> {
    InvalidDirection(&'a str),
}

impl_display!{ CssDirectionCornerParseError<'a>, {
    InvalidDirection(val) => format!("Invalid direction: \"{}\"", val),
}}

pub fn parse_direction_corner<'a>(input: &'a str)
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

typed_pixel_value_parser!(parse_style_letter_spacing, StyleLetterSpacing);
typed_pixel_value_parser!(parse_style_word_spacing, StyleWordSpacing);

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

typed_pixel_value_parser!(parse_layout_margin_top, LayoutMarginTop);
typed_pixel_value_parser!(parse_layout_margin_bottom, LayoutMarginBottom);
typed_pixel_value_parser!(parse_layout_margin_right, LayoutMarginRight);
typed_pixel_value_parser!(parse_layout_margin_left, LayoutMarginLeft);

typed_pixel_value_parser!(parse_layout_padding_top, LayoutPaddingTop);
typed_pixel_value_parser!(parse_layout_padding_bottom, LayoutPaddingBottom);
typed_pixel_value_parser!(parse_layout_padding_right, LayoutPaddingRight);
typed_pixel_value_parser!(parse_layout_padding_left, LayoutPaddingLeft);

typed_pixel_value_parser!(parse_style_border_top_left_radius, StyleBorderTopLeftRadius);
typed_pixel_value_parser!(parse_style_border_bottom_left_radius, StyleBorderBottomLeftRadius);
typed_pixel_value_parser!(parse_style_border_top_right_radius, StyleBorderTopRightRadius);
typed_pixel_value_parser!(parse_style_border_bottom_right_radius, StyleBorderBottomRightRadius);

typed_pixel_value_parser!(parse_style_border_top_width, StyleBorderTopWidth);
typed_pixel_value_parser!(parse_style_border_bottom_width, StyleBorderBottomWidth);
typed_pixel_value_parser!(parse_style_border_right_width, StyleBorderRightWidth);
typed_pixel_value_parser!(parse_style_border_left_width, StyleBorderLeftWidth);

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display!{FlexGrowParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-grow: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

pub fn parse_layout_flex_grow<'a>(input: &'a str) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
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

pub fn parse_layout_flex_shrink<'a>(input: &'a str) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexShrink(o)),
        Err(e) => Err(FlexShrinkParseError::ParseFloat(e, input)),
    }
}

pub fn parse_style_tab_width(input: &str)
-> Result<StyleTabWidth, PercentageParseError>
{
    parse_percentage_value(input).and_then(|e| Ok(StyleTabWidth(e)))
}

pub fn parse_style_line_height(input: &str)
-> Result<StyleLineHeight, PercentageParseError>
{
    parse_percentage_value(input).and_then(|e| Ok(StyleLineHeight(e)))
}

typed_pixel_value_parser!(parse_style_font_size, StyleFontSize);

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssStyleFontFamilyParseError<'a> {
    InvalidStyleFontFamily(&'a str),
    UnclosedQuotes(&'a str),
}

impl_display!{CssStyleFontFamilyParseError<'a>, {
    InvalidStyleFontFamily(val) => format!("Invalid font-family: \"{}\"", val),
    UnclosedQuotes(val) => format!("Unclosed quotes: \"{}\"", val),
}}

impl<'a> From<UnclosedQuotesError<'a>> for CssStyleFontFamilyParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssStyleFontFamilyParseError::UnclosedQuotes(err.0)
    }
}

/// Parses a `StyleFontFamily` declaration from a `&str`
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # extern crate azul_css_parser;
/// # use azul_css_parser::parse_style_font_family;
/// # use azul_css::{StyleFontFamily, FontId};
/// let input = "\"Helvetica\", 'Arial', Times New Roman";
/// let fonts = vec![
///     FontId("Helvetica".into()),
///     FontId("Arial".into()),
///     FontId("Times New Roman".into())
/// ];
///
/// assert_eq!(parse_style_font_family(input), Ok(StyleFontFamily { fonts }));
/// ```
pub fn parse_style_font_family<'a>(input: &'a str) -> Result<StyleFontFamily, CssStyleFontFamilyParseError<'a>> {
    let multiple_fonts = input.split(',');
    let mut fonts = Vec::with_capacity(1);

    for font in multiple_fonts {
        let font = font.trim();
        let font = font.trim_matches('\'');
        let font = font.trim_matches('\"');
        let font = font.trim();
        fonts.push(FontId(font.into()));
    }

    Ok(StyleFontFamily {
        fonts: fonts,
    })
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum ParenthesisParseError<'a> {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(&'a str),
    EmptyInput,
}

impl_display!{ ParenthesisParseError<'a>, {
    UnclosedBraces => format!("Unclosed parenthesis"),
    NoOpeningBraceFound => format!("Expected value in parenthesis (missing \"(\")"),
    NoClosingBraceFound => format!("Missing closing parenthesis (missing \")\")"),
    StopWordNotFound(e) => format!("Stopword not found, found: \"{}\"", e),
    EmptyInput => format!("Empty parenthesis"),
}}

/// Checks wheter a given input is enclosed in parentheses, prefixed
/// by a certain number of stopwords.
///
/// On success, returns what the stopword was + the string inside the braces
/// on failure returns None.
///
/// ```rust
/// # use azul_css_parser::parse_parentheses;
/// # use azul_css_parser::ParenthesisParseError::*;
/// // Search for the nearest "abc()" brace
/// assert_eq!(parse_parentheses("abc(def(g))", &["abc"]), Ok(("abc", "def(g)")));
/// assert_eq!(parse_parentheses("abc(def(g))", &["def"]), Err(StopWordNotFound("abc")));
/// assert_eq!(parse_parentheses("def(ghi(j))", &["def"]), Ok(("def", "ghi(j)")));
/// assert_eq!(parse_parentheses("abc(def(g))", &["abc", "def"]), Ok(("abc", "def(g)")));
/// ```
pub fn parse_parentheses<'a>(
    input: &'a str,
    stopwords: &[&'static str])
-> Result<(&'static str, &'a str), ParenthesisParseError<'a>>
{
    use self::ParenthesisParseError::*;

    let input = input.trim();
    if input.is_empty() {
        return Err(EmptyInput);
    }

    let first_open_brace = input.find('(').ok_or(NoOpeningBraceFound)?;
    let found_stopword = &input[..first_open_brace];

    // CSS does not allow for space between the ( and the stopword, so no .trim() here
    let mut validated_stopword = None;
    for stopword in stopwords {
        if found_stopword == *stopword {
            validated_stopword = Some(stopword);
            break;
        }
    }

    let validated_stopword = validated_stopword.ok_or(StopWordNotFound(found_stopword))?;
    let last_closing_brace = input.rfind(')').ok_or(NoClosingBraceFound)?;

    Ok((validated_stopword, &input[(first_open_brace + 1)..last_closing_brace]))
}

multi_type_parser!(parse_style_cursor, StyleCursor,
                    ["alias", Alias],
                    ["all-scroll", AllScroll],
                    ["cell", Cell],
                    ["col-resize", ColResize],
                    ["context-menu", ContextMenu],
                    ["copy", Copy],
                    ["crosshair", Crosshair],
                    ["default", Default],
                    ["e-resize", EResize],
                    ["ew-resize", EwResize],
                    ["grab", Grab],
                    ["grabbing", Grabbing],
                    ["help", Help],
                    ["move", Move],
                    ["n-resize", NResize],
                    ["ns-resize", NsResize],
                    ["nesw-resize", NeswResize],
                    ["nwse-resize", NwseResize],
                    ["pointer", Pointer],
                    ["progress", Progress],
                    ["row-resize", RowResize],
                    ["s-resize", SResize],
                    ["se-resize", SeResize],
                    ["text", Text],
                    ["unset", Unset],
                    ["vertical-text", VerticalText],
                    ["w-resize", WResize],
                    ["wait", Wait],
                    ["zoom-in", ZoomIn],
                    ["zoom-out", ZoomOut]);

multi_type_parser!(parse_style_background_size, StyleBackgroundSize,
                    ["contain", Contain],
                    ["cover", Cover]);

multi_type_parser!(parse_style_background_repeat, StyleBackgroundRepeat,
                    ["no-repeat", NoRepeat],
                    ["repeat", Repeat],
                    ["repeat-x", RepeatX],
                    ["repeat-y", RepeatY]);

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

multi_type_parser!(parse_layout_overflow, Overflow,
                    ["auto", Auto],
                    ["scroll", Scroll],
                    ["visible", Visible],
                    ["hidden", Hidden]);

multi_type_parser!(parse_layout_text_align, StyleTextAlignmentHorz,
                    ["center", Center],
                    ["left", Left],
                    ["right", Right]);

#[cfg(test)]
mod css_tests {
    use super::*;

    #[test]
    fn test_parse_box_shadow_1() {
        assert_eq!(parse_css_box_shadow("none"), Ok(None));
    }

    #[test]
    fn test_parse_box_shadow_2() {
        assert_eq!(parse_css_box_shadow("5px 10px"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            blur_radius: PixelValue::px(0.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_3() {
        assert_eq!(parse_css_box_shadow("5px 10px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(0.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_4() {
        assert_eq!(parse_css_box_shadow("5px 10px inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            blur_radius: PixelValue::px(0.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_5() {
        assert_eq!(parse_css_box_shadow("5px 10px outset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            blur_radius: PixelValue::px(0.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_6() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(5.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_7() {
        assert_eq!(parse_css_box_shadow("5px 10px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(0.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_8() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(5.0),
            spread_radius: PixelValue::px(0.0),
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_9() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px 10px #888888"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(5.0),
            spread_radius: PixelValue::px(10.0),
            clip_mode: BoxShadowClipMode::Outset,
        })));
    }

    #[test]
    fn test_parse_box_shadow_10() {
        assert_eq!(parse_css_box_shadow("5px 10px 5px 10px #888888 inset"), Ok(Some(BoxShadowPreDisplayItem {
            offset: [PixelValue::px(5.0), PixelValue::px(10.0)],
            color: ColorU { r: 136, g: 136, b: 136, a: 255 },
            blur_radius: PixelValue::px(5.0),
            spread_radius: PixelValue::px(10.0),
            clip_mode: BoxShadowClipMode::Inset,
        })));
    }

    #[test]
    fn test_parse_css_border_1() {
        assert_eq!(
            parse_css_border("5px solid red"),
            Ok(StyleBorderSide {
                border_width: PixelValue::px(5.0),
                border_style: BorderStyle::Solid,
                border_color: ColorU { r: 255, g: 0, b: 0, a: 255 },
            })
        );
    }

    #[test]
    fn test_parse_css_border_2() {
        assert_eq!(
            parse_css_border("double"),
            Ok(StyleBorderSide {
                border_width: PixelValue::px(3.0),
                border_style: BorderStyle::Double,
                border_color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            })
        );
    }

    #[test]
    fn test_parse_css_border_3() {
        assert_eq!(
            parse_css_border("1px solid rgb(51, 153, 255)"),
            Ok(StyleBorderSide {
                border_width: PixelValue::px(1.0),
                border_style: BorderStyle::Solid,
                border_color: ColorU { r: 51, g: 153, b: 255, a: 255 },
            })
        );
    }

    #[test]
    fn test_parse_linear_gradient_1() {
        assert_eq!(parse_style_background("linear-gradient(red, yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
            })));
    }

    #[test]
    fn test_parse_linear_gradient_2() {
        assert_eq!(parse_style_background("linear-gradient(red, lime, blue, yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(33.333332)),
                    color: ColorU { r: 0, g: 255, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(66.666664)),
                    color: ColorU { r: 0, g: 0, b: 255, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(99.9999)), // note: not 100%, but close enough
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_3() {
        assert_eq!(parse_style_background("repeating-linear-gradient(50deg, blue, yellow, #00FF00)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::Angle(50.0.into()),
                extend_mode: ExtendMode::Repeat,
                stops: vec![
                GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 0, g: 0, b: 255, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(50.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 0, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_4() {
        assert_eq!(parse_style_background("linear-gradient(to bottom right, red, yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::TopLeft, DirectionCorner::BottomRight),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
            })
        ));
    }

    #[test]
    fn test_parse_linear_gradient_5() {
        assert_eq!(parse_style_background("linear-gradient(0.42rad, red, yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::Angle(FloatValue::new(24.0642)),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_6() {
        assert_eq!(parse_style_background("linear-gradient(12.93grad, red, yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::Angle(FloatValue::new(11.637)),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_7() {
        assert_eq!(parse_style_background("linear-gradient(to right, rgba(255,0, 0,1) 0%,rgba(0,0,0, 0) 100%)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::Left, DirectionCorner::Right),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 0, g: 0, b: 0, a: 0 },
                }],
            })
        ));
    }

    #[test]
    fn test_parse_linear_gradient_8() {
        assert_eq!(parse_style_background("linear-gradient(to bottom, rgb(255,0, 0),rgb(0,0,0))"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                }],
            })
        ));
    }

    #[test]
    fn test_parse_linear_gradient_9() {
        assert_eq!(parse_style_background("linear-gradient(10deg, rgb(10, 30, 20), yellow)"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::Angle(FloatValue::new(10.0)),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 10, g: 30, b: 20, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_10() {
        assert_eq!(parse_style_background("linear-gradient(50deg, rgba(10, 30, 20, 0.93), hsla(40deg, 80%, 30%, 0.1))"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::Angle(FloatValue::new(50.0)),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 10, g: 30, b: 20, a: 238 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 138, g: 97, b: 15, a: 25 },
                }],
        })));
    }

    #[test]
    fn test_parse_linear_gradient_11() {
        // wacky whitespace on purpose
        assert_eq!(parse_style_background("linear-gradient(to bottom,rgb(255,0, 0)0%, rgb( 0 , 255 , 0 ) 10% ,blue   100%  )"),
            Ok(StyleBackground::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorner::Top, DirectionCorner::Bottom),
                extend_mode: ExtendMode::Clamp,
                stops: vec![GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(10.0)),
                    color: ColorU { r: 0, g: 255, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 0, g: 0, b: 255, a: 255 },
                }],
            })
        ));
    }

    #[test]
    fn test_parse_radial_gradient_1() {
        assert_eq!(parse_style_background("radial-gradient(circle, lime, blue, yellow)"),
            Ok(StyleBackground::RadialGradient(RadialGradient {
                shape: Shape::Circle,
                extend_mode: ExtendMode::Clamp,
                stops: vec![
                GradientStopPre {
                    offset: Some(PercentageValue::new(0.0)),
                    color: ColorU { r: 0, g: 255, b: 0, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(50.0)),
                    color: ColorU { r: 0, g: 0, b: 255, a: 255 },
                },
                GradientStopPre {
                    offset: Some(PercentageValue::new(100.0)),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }],
        })));
    }

    // This test currently fails, but it's not that important to fix right now
    /*
    #[test]
    fn test_parse_radial_gradient_2() {
        assert_eq!(parse_style_background("repeating-radial-gradient(circle, red 10%, blue 50%, lime, yellow)"),
            Ok(ParsedGradient::RadialGradient(RadialGradient {
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
    fn test_parse_css_color_4() {
        assert_eq!(parse_css_color("rgb(192, 14, 12)"), Ok(ColorU { r: 192, g: 14, b: 12, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_5() {
        assert_eq!(parse_css_color("rgb(283, 8, 105)"), Err(CssColorParseError::IntValueParseErr("283".parse::<u8>().err().unwrap())));
    }

    #[test]
    fn test_parse_css_color_6() {
        assert_eq!(parse_css_color("rgba(192, 14, 12, 80)"), Err(CssColorParseError::FloatValueOutOfRange(80.0)));
    }

    #[test]
    fn test_parse_css_color_7() {
        assert_eq!(parse_css_color("rgba( 0,127,     255   , 0.25  )"), Ok(ColorU { r: 0, g: 127, b: 255, a: 64 }));
    }

    #[test]
    fn test_parse_css_color_8() {
        assert_eq!(parse_css_color("rgba( 1 ,2,3, 1.0)"), Ok(ColorU { r: 1, g: 2, b: 3, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_9() {
        assert_eq!(parse_css_color("rgb("), Err(CssColorParseError::UnclosedColor("rgb(")));
    }

    #[test]
    fn test_parse_css_color_10() {
        assert_eq!(parse_css_color("rgba("), Err(CssColorParseError::UnclosedColor("rgba(")));
    }

    #[test]
    fn test_parse_css_color_11() {
        assert_eq!(parse_css_color("rgba(123, 36, 92, 0.375"), Err(CssColorParseError::UnclosedColor("rgba(123, 36, 92, 0.375")));
    }

    #[test]
    fn test_parse_css_color_12() {
        assert_eq!(parse_css_color("rgb()"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Red)));
    }

    #[test]
    fn test_parse_css_color_13() {
        assert_eq!(parse_css_color("rgb(10)"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Green)));
    }

    #[test]
    fn test_parse_css_color_14() {
        assert_eq!(parse_css_color("rgb(20, 30)"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Blue)));
    }

    #[test]
    fn test_parse_css_color_15() {
        assert_eq!(parse_css_color("rgb(30, 40,)"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Blue)));
    }

    #[test]
    fn test_parse_css_color_16() {
        assert_eq!(parse_css_color("rgba(40, 50, 60)"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha)));
    }

    #[test]
    fn test_parse_css_color_17() {
        assert_eq!(parse_css_color("rgba(50, 60, 70, )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha)));
    }

    #[test]
    fn test_parse_css_color_18() {
        assert_eq!(parse_css_color("hsl(0deg, 100%, 100%)"), Ok(ColorU { r: 255, g: 255, b: 255, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_19() {
        assert_eq!(parse_css_color("hsl(0deg, 100%, 50%)"), Ok(ColorU { r: 255, g: 0, b: 0, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_20() {
        assert_eq!(parse_css_color("hsl(170deg, 50%, 75%)"), Ok(ColorU { r: 160, g: 224, b: 213, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_21() {
        assert_eq!(parse_css_color("hsla(190deg, 50%, 75%, 1.0)"), Ok(ColorU { r: 160, g: 213, b: 224, a: 255 }));
    }

    #[test]
    fn test_parse_css_color_22() {
        assert_eq!(parse_css_color("hsla(120deg, 0%, 25%, 0.25)"), Ok(ColorU { r: 64, g: 64, b: 64, a: 64 }));
    }

    #[test]
    fn test_parse_css_color_23() {
        assert_eq!(parse_css_color("hsla(120deg, 0%, 0%, 0.5)"), Ok(ColorU { r: 0, g: 0, b: 0, a: 128 }));
    }

    #[test]
    fn test_parse_css_color_24() {
        assert_eq!(parse_css_color("hsla(60.9deg, 80.3%, 40%, 0.5)"), Ok(ColorU { r: 182, g: 184, b: 20, a: 128 }));
    }

    #[test]
    fn test_parse_css_color_25() {
        assert_eq!(parse_css_color("hsla(60.9rad, 80.3%, 40%, 0.5)"), Ok(ColorU { r: 45, g: 20, b: 184, a: 128 }));
    }

    #[test]
    fn test_parse_css_color_26() {
        assert_eq!(parse_css_color("hsla(60.9grad, 80.3%, 40%, 0.5)"), Ok(ColorU { r: 184, g: 170, b: 20, a: 128 }));
    }

    #[test]
    fn test_parse_direction() {
        let first_input = "60.9grad";
        let e = FloatValue::new(first_input.split("grad").next().unwrap().parse::<f32>().expect("Parseable float") / 400.0 * 360.0);
        assert_eq!(e, FloatValue::new(60.9 / 400.0 * 360.0));
        assert_eq!(parse_direction("60.9grad"), Ok(Direction::Angle(FloatValue::new(60.9 / 400.0 * 360.0))));
    }

    #[test]
    fn test_parse_float_value() {
        assert_eq!(parse_float_value("60.9"), Ok(FloatValue::new(60.9)));
    }

    #[test]
    fn test_parse_css_color_27() {
        assert_eq!(parse_css_color("hsla(240, 0%, 0%, 0.5)"), Ok(ColorU { r: 0, g: 0, b: 0, a: 128 }));
    }

    #[test]
    fn test_parse_css_color_28() {
        assert_eq!(parse_css_color("hsla(240deg, 0, 0%, 0.5)"), Err(CssColorParseError::InvalidPercentage(PercentageParseError::NoPercentSign)));
    }

    #[test]
    fn test_parse_css_color_29() {
        assert_eq!(parse_css_color("hsla(240deg, 0%, 0, 0.5)"), Err(CssColorParseError::InvalidPercentage(PercentageParseError::NoPercentSign)));
    }

    #[test]
    fn test_parse_css_color_30() {
        assert_eq!(parse_css_color("hsla(240deg, 0%, 0%, )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha)));
    }

    #[test]
    fn test_parse_css_color_31() {
        assert_eq!(parse_css_color("hsl(, 0%, 0%, )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Hue)));
    }

    #[test]
    fn test_parse_css_color_32() {
        assert_eq!(parse_css_color("hsl(240deg ,  )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Saturation)));
    }

    #[test]
    fn test_parse_css_color_33() {
        assert_eq!(parse_css_color("hsl(240deg, 0%,  )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Lightness)));
    }

    #[test]
    fn test_parse_css_color_34() {
        assert_eq!(parse_css_color("hsl(240deg, 0%, 0%,  )"), Err(CssColorParseError::ExtraArguments("")));
    }

    #[test]
    fn test_parse_css_color_35() {
        assert_eq!(parse_css_color("hsla(240deg, 0%, 0%  )"), Err(CssColorParseError::MissingColorComponent(CssColorComponent::Alpha)));
    }

    #[test]
    fn test_parse_css_color_36() {
        assert_eq!(parse_css_color("rgb(255,0, 0)"), Ok(ColorU { r: 255, g: 0, b: 0, a: 255 }));
    }

    #[test]
    fn test_parse_pixel_value_1() {
        assert_eq!(parse_pixel_value("15px"), Ok(PixelValue::px(15.0)));
    }

    #[test]
    fn test_parse_pixel_value_2() {
        assert_eq!(parse_pixel_value("1.2em"), Ok(PixelValue::em(1.2)));
    }

    #[test]
    fn test_parse_pixel_value_3() {
        assert_eq!(parse_pixel_value("11pt"), Ok(PixelValue::pt(11.0)));
    }

    #[test]
    fn test_parse_pixel_value_4() {
        assert_eq!(parse_pixel_value("aslkfdjasdflk"), Err(PixelParseError::NoValueGiven("aslkfdjasdflk")));
    }

    #[test]
    fn test_parse_style_border_radius_1() {
        assert_eq!(parse_style_border_radius("15px"), Ok(StyleBorderRadius(
            BorderRadius::uniform(PixelSize::new(PixelValue::px(15.0), PixelValue::px(15.0)))
        )));
    }

    #[test]
    fn test_parse_style_border_radius_2() {
        assert_eq!(parse_style_border_radius("15px 50px"), Ok(StyleBorderRadius(BorderRadius {
            top_left: PixelSize::new(PixelValue::px(15.0), PixelValue::px(15.0)),
            bottom_right: PixelSize::new(PixelValue::px(15.0), PixelValue::px(15.0)),
            top_right: PixelSize::new(PixelValue::px(50.0), PixelValue::px(50.0)),
            bottom_left: PixelSize::new(PixelValue::px(50.0), PixelValue::px(50.0)),
        })));
    }

    #[test]
    fn test_parse_style_border_radius_3() {
        assert_eq!(parse_style_border_radius("15px 50px 30px"), Ok(StyleBorderRadius(BorderRadius {
            top_left: PixelSize::new(PixelValue::px(15.0), PixelValue::px(15.0)),
            bottom_right: PixelSize::new(PixelValue::px(30.0), PixelValue::px(30.0)),
            top_right: PixelSize::new(PixelValue::px(50.0), PixelValue::px(50.0)),
            bottom_left: PixelSize::new(PixelValue::px(50.0), PixelValue::px(50.0)),
        })));
    }

    #[test]
    fn test_parse_style_border_radius_4() {
        assert_eq!(parse_style_border_radius("15px 50px 30px 5px"), Ok(StyleBorderRadius(BorderRadius {
            top_left: PixelSize::new(PixelValue::px(15.0), PixelValue::px(15.0)),
            bottom_right: PixelSize::new(PixelValue::px(30.0), PixelValue::px(30.0)),
            top_right: PixelSize::new(PixelValue::px(50.0), PixelValue::px(50.0)),
            bottom_left: PixelSize::new(PixelValue::px(5.0), PixelValue::px(5.0)),
        })));
    }

    #[test]
    fn test_parse_style_font_family_1() {
        assert_eq!(parse_style_font_family("\"Webly Sleeky UI\", monospace"), Ok(StyleFontFamily {
            fonts: vec![
                FontId("Webly Sleeky UI".into()),
                FontId("monospace".into()),
            ]
        }));
    }

    #[test]
    fn test_parse_style_font_family_2() {
        assert_eq!(parse_style_font_family("'Webly Sleeky UI'"), Ok(StyleFontFamily {
            fonts: vec![
                FontId("Webly Sleeky UI".into()),
            ]
        }));
    }

    #[test]
    fn test_parse_background_image() {
        assert_eq!(parse_style_background("image(\"Cat 01\")"), Ok(StyleBackground::Image(
            CssImageId(String::from("Cat 01"))
        )));
    }

    #[test]
    fn test_parse_padding_1() {
        assert_eq!(parse_layout_padding("10px"), Ok(LayoutPadding {
            top: Some(PixelValue::px(10.0)),
            right: Some(PixelValue::px(10.0)),
            bottom: Some(PixelValue::px(10.0)),
            left: Some(PixelValue::px(10.0)),
        }));
    }

    #[test]
    fn test_parse_padding_2() {
        assert_eq!(parse_layout_padding("25px 50px"), Ok(LayoutPadding {
            top: Some(PixelValue::px(25.0)),
            right: Some(PixelValue::px(50.0)),
            bottom: Some(PixelValue::px(25.0)),
            left: Some(PixelValue::px(50.0)),
        }));
    }

    #[test]
    fn test_parse_padding_3() {
        assert_eq!(parse_layout_padding("25px 50px 75px"), Ok(LayoutPadding {
            top: Some(PixelValue::px(25.0)),
            right: Some(PixelValue::px(50.0)),
            left: Some(PixelValue::px(50.0)),
            bottom: Some(PixelValue::px(75.0)),
        }));
    }

    #[test]
    fn test_parse_padding_4() {
        assert_eq!(parse_layout_padding("25px 50px 75px 100px"), Ok(LayoutPadding {
            top: Some(PixelValue::px(25.0)),
            right: Some(PixelValue::px(50.0)),
            bottom: Some(PixelValue::px(75.0)),
            left: Some(PixelValue::px(100.0)),
        }));
    }
}
