//! Contains utilities to convert strings (CSS strings) to servo types

use alloc::{string::String, vec::Vec};
use core::{
    fmt,
    num::{ParseFloatError, ParseIntError},
};

use crate::{
    AngleMetric, AngleValue, AzString, BackgroundPositionHorizontal, BackgroundPositionVertical,
    BorderStyle, BoxShadowClipMode, ColorU, CombinedCssPropertyType, ConicGradient, CssProperty,
    CssPropertyType, CssPropertyValue, Direction, DirectionCorner, DirectionCorners, ExtendMode,
    FloatValue, LayoutAlignContent, LayoutAlignItems, LayoutBorderBottomWidth,
    LayoutBorderLeftWidth, LayoutBorderRightWidth, LayoutBorderTopWidth, LayoutBottom,
    LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
    LayoutFlexWrap, LayoutFloat, LayoutHeight, LayoutJustifyContent, LayoutLeft,
    LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop, LayoutMaxHeight,
    LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPaddingBottom,
    LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutRight,
    LayoutTop, LayoutWidth, LinearColorStop, LinearGradient, NormalizedLinearColorStop,
    NormalizedRadialColorStop, OptionPercentageValue, PercentageValue, PixelValue,
    PixelValueNoPercent, RadialColorStop, RadialGradient, RadialGradientSize, ScrollbarStyle,
    Shape, SizeMetric, StyleBackfaceVisibility, StyleBackgroundContent, StyleBackgroundContentVec,
    StyleBackgroundPosition, StyleBackgroundPositionVec, StyleBackgroundRepeat,
    StyleBackgroundRepeatVec, StyleBackgroundSize, StyleBackgroundSizeVec, StyleBorderBottomColor,
    StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleBorderBottomStyle,
    StyleBorderLeftColor, StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
    StyleBorderSide, StyleBorderTopColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
    StyleBorderTopStyle, StyleBoxShadow, StyleCursor, StyleFilter, StyleFilterVec, StyleFontFamily,
    StyleFontFamilyVec, StyleFontSize, StyleLetterSpacing, StyleLineHeight, StyleMixBlendMode,
    StyleOpacity, StylePerspectiveOrigin, StyleTabWidth, StyleTextAlign, StyleTextColor,
    StyleTransform, StyleTransformOrigin, StyleTransformVec, StyleWordSpacing,
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

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $(
                        $return::$enum_type => write!(f, $identifier_string),
                    )+
                }
            }
        }
    };
    ($fn:ident, $return:ident, $([$identifier_string:expr, $enum_type:ident]),+) => {
        multi_type_parser!($fn, stringify!($return), $return,
            concat!(
                "# extern crate azul_css;", "\r\n",
                "# use azul_css::parser::", stringify!($fn), ";", "\r\n",
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
    (
        $fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr
    ) => {
        ///Parses a `
        #[doc = $return_str]
        ///` attribute from a `&str`
        ///
        ///# Example
        ///
        ///```rust
        #[doc = $import_str]
        #[doc = $test_str]
        ///```
        pub fn $fn<'a>(input: &'a str) -> Result<$return, CssPixelValueParseError<'a>> {
            parse_pixel_value(input).and_then(|e| Ok($return { inner: e }))
        }

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.inner.format_as_css_value(f)
            }
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!(
            $fn,
            stringify!($fn),
            $return,
            stringify!($return),
            concat!(
                "# extern crate azul_css;",
                "\r\n",
                "# use azul_css::parser::",
                stringify!($fn),
                ";",
                "\r\n",
                "# use azul_css::{PixelValue, ",
                stringify!($return),
                "};"
            ),
            concat!(
                "assert_eq!(",
                stringify!($fn),
                "(\"5px\"), Ok(",
                stringify!($return),
                " { inner: PixelValue::px(5.0) }));"
            )
        );
    };
}

/// Main parsing function, takes a stringified key / value pair and either
/// returns the parsed value or an error
///
/// ```rust
/// # extern crate azul_css;
///
/// # use azul_css_parser;
/// # use azul_css::{LayoutWidth, PixelValue, CssPropertyType, CssPropertyValue, CssProperty};
/// assert_eq!(
///     azul_css::parser::parse_css_property(CssPropertyType::Width, "500px"),
///     Ok(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth { inner: PixelValue::px(500.0) })))
/// )
/// ```
pub fn parse_css_property<'a>(
    key: CssPropertyType,
    value: &'a str,
) -> Result<CssProperty, CssParsingError<'a>> {
    use self::CssPropertyType::*;
    let value = value.trim();
    Ok(match value {
        "auto" => CssProperty::auto(key),
        "none" => CssProperty::none(key),
        "initial" => CssProperty::initial(key).into(),
        "inherit" => CssProperty::inherit(key).into(),
        value => match key {
            TextColor => parse_style_text_color(value)?.into(),
            FontSize => parse_style_font_size(value)?.into(),
            FontFamily => parse_style_font_family(value)?.into(),
            TextAlign => parse_layout_text_align(value)?.into(),
            LetterSpacing => parse_style_letter_spacing(value)?.into(),
            LineHeight => parse_style_line_height(value)?.into(),
            WordSpacing => parse_style_word_spacing(value)?.into(),
            TabWidth => parse_style_tab_width(value)?.into(),
            Cursor => parse_style_cursor(value)?.into(),

            Display => parse_layout_display(value)?.into(),
            Float => parse_layout_float(value)?.into(),
            BoxSizing => parse_layout_box_sizing(value)?.into(),
            Width => parse_layout_width(value)?.into(),
            Height => parse_layout_height(value)?.into(),
            MinWidth => parse_layout_min_width(value)?.into(),
            MinHeight => parse_layout_min_height(value)?.into(),
            MaxWidth => parse_layout_max_width(value)?.into(),
            MaxHeight => parse_layout_max_height(value)?.into(),
            Position => parse_layout_position(value)?.into(),
            Top => parse_layout_top(value)?.into(),
            Right => parse_layout_right(value)?.into(),
            Left => parse_layout_left(value)?.into(),
            Bottom => parse_layout_bottom(value)?.into(),
            FlexWrap => parse_layout_wrap(value)?.into(),
            FlexDirection => parse_layout_direction(value)?.into(),
            FlexGrow => parse_layout_flex_grow(value)?.into(),
            FlexShrink => parse_layout_flex_shrink(value)?.into(),
            JustifyContent => parse_layout_justify_content(value)?.into(),
            AlignItems => parse_layout_align_items(value)?.into(),
            AlignContent => parse_layout_align_content(value)?.into(),

            BackgroundContent => parse_style_background_content_multiple(value)?.into(),
            BackgroundPosition => parse_style_background_position_multiple(value)?.into(),
            BackgroundSize => parse_style_background_size_multiple(value)?.into(),
            BackgroundRepeat => parse_style_background_repeat_multiple(value)?.into(),

            OverflowX => {
                CssProperty::OverflowX(CssPropertyValue::Exact(parse_layout_overflow(value)?))
                    .into()
            }
            OverflowY => {
                CssProperty::OverflowY(CssPropertyValue::Exact(parse_layout_overflow(value)?))
                    .into()
            }

            PaddingTop => parse_layout_padding_top(value)?.into(),
            PaddingLeft => parse_layout_padding_left(value)?.into(),
            PaddingRight => parse_layout_padding_right(value)?.into(),
            PaddingBottom => parse_layout_padding_bottom(value)?.into(),

            MarginTop => parse_layout_margin_top(value)?.into(),
            MarginLeft => parse_layout_margin_left(value)?.into(),
            MarginRight => parse_layout_margin_right(value)?.into(),
            MarginBottom => parse_layout_margin_bottom(value)?.into(),

            BorderTopLeftRadius => parse_style_border_top_left_radius(value)?.into(),
            BorderTopRightRadius => parse_style_border_top_right_radius(value)?.into(),
            BorderBottomLeftRadius => parse_style_border_bottom_left_radius(value)?.into(),
            BorderBottomRightRadius => parse_style_border_bottom_right_radius(value)?.into(),

            BorderTopColor => StyleBorderTopColor {
                inner: parse_css_color(value)?,
            }
            .into(),
            BorderRightColor => StyleBorderRightColor {
                inner: parse_css_color(value)?,
            }
            .into(),
            BorderLeftColor => StyleBorderLeftColor {
                inner: parse_css_color(value)?,
            }
            .into(),
            BorderBottomColor => StyleBorderBottomColor {
                inner: parse_css_color(value)?,
            }
            .into(),

            BorderTopStyle => StyleBorderTopStyle {
                inner: parse_style_border_style(value)?,
            }
            .into(),
            BorderRightStyle => StyleBorderRightStyle {
                inner: parse_style_border_style(value)?,
            }
            .into(),
            BorderLeftStyle => StyleBorderLeftStyle {
                inner: parse_style_border_style(value)?,
            }
            .into(),
            BorderBottomStyle => StyleBorderBottomStyle {
                inner: parse_style_border_style(value)?,
            }
            .into(),

            BorderTopWidth => parse_style_border_top_width(value)?.into(),
            BorderRightWidth => parse_style_border_right_width(value)?.into(),
            BorderLeftWidth => parse_style_border_left_width(value)?.into(),
            BorderBottomWidth => parse_style_border_bottom_width(value)?.into(),

            BoxShadowLeft => {
                CssProperty::BoxShadowLeft(CssPropertyValue::Exact(parse_style_box_shadow(value)?))
                    .into()
            }
            BoxShadowRight => {
                CssProperty::BoxShadowRight(CssPropertyValue::Exact(parse_style_box_shadow(value)?))
                    .into()
            }
            BoxShadowTop => {
                CssProperty::BoxShadowTop(CssPropertyValue::Exact(parse_style_box_shadow(value)?))
                    .into()
            }
            BoxShadowBottom => CssProperty::BoxShadowBottom(CssPropertyValue::Exact(
                parse_style_box_shadow(value)?,
            ))
            .into(),

            ScrollbarStyle => parse_scrollbar_style(value)?.into(), /* TODO: stub - always */
            // returns default style
            Opacity => parse_style_opacity(value)?.into(),
            Transform => parse_style_transform_vec(value)?.into(),
            TransformOrigin => parse_style_transform_origin(value)?.into(),
            PerspectiveOrigin => parse_style_perspective_origin(value)?.into(),
            BackfaceVisibility => parse_style_backface_visibility(value)?.into(),

            MixBlendMode => parse_style_mix_blend_mode(value)?.into(),
            Filter => {
                CssProperty::Filter(CssPropertyValue::Exact(parse_style_filter_vec(value)?)).into()
            }
            BackdropFilter => {
                CssProperty::BackdropFilter(CssPropertyValue::Exact(parse_style_filter_vec(value)?))
                    .into()
            }
            TextShadow => {
                CssProperty::TextShadow(CssPropertyValue::Exact(parse_style_box_shadow(value)?))
                    .into()
            }
        },
    })
}

/// Parses a combined CSS property or a CSS property shorthand, for example "margin"
/// (as a shorthand for setting all four properties of "margin-top", "margin-bottom",
/// "margin-left" and "margin-right")
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css_parser;
/// # use azul_css::*;
/// assert_eq!(
///     azul_css::parser::parse_combined_css_property(
///         CombinedCssPropertyType::BorderRadius,
///         "10px"
///     ),
///     Ok(vec![
///         CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(
///             StyleBorderTopLeftRadius::px(10.0)
///         )),
///         CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(
///             StyleBorderTopRightRadius::px(10.0)
///         )),
///         CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(
///             StyleBorderBottomLeftRadius::px(10.0)
///         )),
///         CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(
///             StyleBorderBottomRightRadius::px(10.0)
///         )),
///     ])
/// )
/// ```
pub fn parse_combined_css_property<'a>(
    key: CombinedCssPropertyType,
    value: &'a str,
) -> Result<Vec<CssProperty>, CssParsingError<'a>> {
    use self::CombinedCssPropertyType::*;

    macro_rules! convert_value {
        ($thing:expr, $prop_type:ident, $wrapper:ident) => {
            match $thing {
                PixelValueWithAuto::None => CssProperty::none(CssPropertyType::$prop_type),
                PixelValueWithAuto::Initial => CssProperty::initial(CssPropertyType::$prop_type),
                PixelValueWithAuto::Inherit => CssProperty::inherit(CssPropertyType::$prop_type),
                PixelValueWithAuto::Auto => CssProperty::auto(CssPropertyType::$prop_type),
                PixelValueWithAuto::Exact(x) => {
                    CssProperty::$prop_type($wrapper { inner: x }.into())
                }
            }
        };
    }

    let keys = match key {
        BorderRadius => {
            vec![
                CssPropertyType::BorderTopLeftRadius,
                CssPropertyType::BorderTopRightRadius,
                CssPropertyType::BorderBottomLeftRadius,
                CssPropertyType::BorderBottomRightRadius,
            ]
        }
        Overflow => {
            vec![CssPropertyType::OverflowX, CssPropertyType::OverflowY]
        }
        Padding => {
            vec![
                CssPropertyType::PaddingTop,
                CssPropertyType::PaddingBottom,
                CssPropertyType::PaddingLeft,
                CssPropertyType::PaddingRight,
            ]
        }
        Margin => {
            vec![
                CssPropertyType::MarginTop,
                CssPropertyType::MarginBottom,
                CssPropertyType::MarginLeft,
                CssPropertyType::MarginRight,
            ]
        }
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
        }
        BorderLeft => {
            vec![
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderLeftWidth,
            ]
        }
        BorderRight => {
            vec![
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderRightWidth,
            ]
        }
        BorderTop => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderTopWidth,
            ]
        }
        BorderBottom => {
            vec![
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderBottomWidth,
            ]
        }
        BoxShadow => {
            vec![
                CssPropertyType::BoxShadowLeft,
                CssPropertyType::BoxShadowRight,
                CssPropertyType::BoxShadowTop,
                CssPropertyType::BoxShadowBottom,
            ]
        }
        BackgroundColor => {
            vec![CssPropertyType::BackgroundContent]
        }
        BackgroundImage => {
            vec![CssPropertyType::BackgroundContent]
        }
    };

    match value {
        "auto" => return Ok(keys.into_iter().map(|ty| CssProperty::auto(ty)).collect()),
        "none" => return Ok(keys.into_iter().map(|ty| CssProperty::none(ty)).collect()),
        "initial" => {
            return Ok(keys
                .into_iter()
                .map(|ty| CssProperty::initial(ty))
                .collect());
        }
        "inherit" => {
            return Ok(keys
                .into_iter()
                .map(|ty| CssProperty::inherit(ty))
                .collect());
        }
        _ => {}
    };

    match key {
        BorderRadius => {
            let border_radius = parse_style_border_radius(value)?;
            Ok(vec![
                CssProperty::BorderTopLeftRadius(
                    StyleBorderTopLeftRadius {
                        inner: border_radius.top_left,
                    }
                    .into(),
                ),
                CssProperty::BorderTopRightRadius(
                    StyleBorderTopRightRadius {
                        inner: border_radius.top_right,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomLeftRadius(
                    StyleBorderBottomLeftRadius {
                        inner: border_radius.bottom_left,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomRightRadius(
                    StyleBorderBottomRightRadius {
                        inner: border_radius.bottom_right,
                    }
                    .into(),
                ),
            ])
        }
        Overflow => {
            let overflow = parse_layout_overflow(value)?;
            Ok(vec![
                CssProperty::OverflowX(overflow.into()),
                CssProperty::OverflowY(overflow.into()),
            ])
        }
        Padding => {
            let padding = parse_layout_padding(value)?;
            Ok(vec![
                convert_value!(padding.top, PaddingTop, LayoutPaddingTop),
                convert_value!(padding.bottom, PaddingBottom, LayoutPaddingBottom),
                convert_value!(padding.left, PaddingLeft, LayoutPaddingLeft),
                convert_value!(padding.right, PaddingRight, LayoutPaddingRight),
            ])
        }
        Margin => {
            let margin = parse_layout_margin(value)?;
            Ok(vec![
                convert_value!(margin.top, MarginTop, LayoutMarginTop),
                convert_value!(margin.bottom, MarginBottom, LayoutMarginBottom),
                convert_value!(margin.left, MarginLeft, LayoutMarginLeft),
                convert_value!(margin.right, MarginRight, LayoutMarginRight),
            ])
        }
        Border => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderTopColor(
                    StyleBorderTopColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderRightColor(
                    StyleBorderRightColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftColor(
                    StyleBorderLeftColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderTopStyle(
                    StyleBorderTopStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftStyle(
                    StyleBorderLeftStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderTopWidth(
                    LayoutBorderTopWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftWidth(
                    LayoutBorderLeftWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderLeft => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderLeftColor(
                    StyleBorderLeftColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftStyle(
                    StyleBorderLeftStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftWidth(
                    LayoutBorderLeftWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderRight => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderRightColor(
                    StyleBorderRightColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderTop => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderTopColor(
                    StyleBorderTopColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderTopStyle(
                    StyleBorderTopStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderTopWidth(
                    LayoutBorderTopWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderBottom => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BoxShadow => {
            let box_shadow = parse_style_box_shadow(value)?;
            Ok(vec![
                CssProperty::BoxShadowLeft(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowRight(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowTop(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowBottom(CssPropertyValue::Exact(box_shadow)),
            ])
        }
        BackgroundColor => {
            let color = parse_css_color(value)?;
            let vec: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            Ok(vec![CssProperty::BackgroundContent(vec.into())])
        }
        BackgroundImage => {
            let background_content = parse_style_background_content(value)?;
            let vec: StyleBackgroundContentVec = vec![background_content].into();
            Ok(vec![CssProperty::BackgroundContent(vec.into())])
        }
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
    PixelParseError(CssPixelValueParseError<'a>),
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
    BackgroundPositionParseError(CssBackgroundPositionParseError<'a>),
    TransformParseError(CssStyleTransformParseError<'a>),
    TransformOriginParseError(CssStyleTransformOriginParseError<'a>),
    PerspectiveOriginParseError(CssStylePerspectiveOriginParseError<'a>),
    Opacity(OpacityParseError<'a>),
    Scrollbar(CssScrollbarStyleParseError<'a>),
    Filter(CssStyleFilterParseError<'a>),
}

impl_debug_as_display!(CssParsingError<'a>);
impl_display! { CssParsingError<'a>, {
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
    BackgroundPositionParseError(e) => format!("{}", e),
    TransformParseError(e) => format!("{}", e),
    TransformOriginParseError(e) => format!("{}", e),
    PerspectiveOriginParseError(e) => format!("{}", e),
    Opacity(e) => format!("{}", e),
    Scrollbar(e) => format!("{}", e),
    Filter(e) => format!("{}", e),
}}

impl_from!(
    CssBorderParseError<'a>,
    CssParsingError::CssBorderParseError
);
impl_from!(
    CssShadowParseError<'a>,
    CssParsingError::CssShadowParseError
);
impl_from!(CssColorParseError<'a>, CssParsingError::CssColorParseError);
impl_from!(InvalidValueErr<'a>, CssParsingError::InvalidValueErr);
impl_from!(
    CssPixelValueParseError<'a>,
    CssParsingError::PixelParseError
);
impl_from!(CssImageParseError<'a>, CssParsingError::CssImageParseError);
impl_from!(
    CssStyleFontFamilyParseError<'a>,
    CssParsingError::CssStyleFontFamilyParseError
);
impl_from!(
    CssBackgroundParseError<'a>,
    CssParsingError::CssBackgroundParseError
);
impl_from!(
    CssStyleBorderRadiusParseError<'a>,
    CssParsingError::CssStyleBorderRadiusParseError
);
impl_from!(
    LayoutPaddingParseError<'a>,
    CssParsingError::PaddingParseError
);
impl_from!(
    LayoutMarginParseError<'a>,
    CssParsingError::MarginParseError
);
impl_from!(
    FlexShrinkParseError<'a>,
    CssParsingError::FlexShrinkParseError
);
impl_from!(FlexGrowParseError<'a>, CssParsingError::FlexGrowParseError);
impl_from!(
    CssBackgroundPositionParseError<'a>,
    CssParsingError::BackgroundPositionParseError
);
impl_from!(
    CssStyleTransformParseError<'a>,
    CssParsingError::TransformParseError
);
impl_from!(
    CssStyleTransformOriginParseError<'a>,
    CssParsingError::TransformOriginParseError
);
impl_from!(
    CssStylePerspectiveOriginParseError<'a>,
    CssParsingError::PerspectiveOriginParseError
);
impl_from!(OpacityParseError<'a>, CssParsingError::Opacity);
impl_from!(CssScrollbarStyleParseError<'a>, CssParsingError::Scrollbar);
impl_from!(CssStyleFilterParseError<'a>, CssParsingError::Filter);

impl<'a> From<PercentageParseError> for CssParsingError<'a> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::PercentageParseError(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParsingErrorOwned {
    CssBorderParseError(CssBorderParseErrorOwned),
    CssShadowParseError(CssShadowParseErrorOwned),
    InvalidValueErr(InvalidValueErrOwned),
    PixelParseError(CssPixelValueParseErrorOwned),
    PercentageParseError(PercentageParseError),
    CssImageParseError(CssImageParseErrorOwned),
    CssStyleFontFamilyParseError(CssStyleFontFamilyParseErrorOwned),
    CssBackgroundParseError(CssBackgroundParseErrorOwned),
    CssColorParseError(CssColorParseErrorOwned),
    CssStyleBorderRadiusParseError(CssStyleBorderRadiusParseErrorOwned),
    PaddingParseError(LayoutPaddingParseErrorOwned),
    MarginParseError(LayoutMarginParseErrorOwned),
    FlexShrinkParseError(FlexShrinkParseErrorOwned),
    FlexGrowParseError(FlexGrowParseErrorOwned),
    BackgroundPositionParseError(CssBackgroundPositionParseErrorOwned),
    TransformParseError(CssStyleTransformParseErrorOwned),
    TransformOriginParseError(CssStyleTransformOriginParseErrorOwned),
    PerspectiveOriginParseError(CssStylePerspectiveOriginParseErrorOwned),
    Opacity(OpacityParseErrorOwned),
    Scrollbar(CssScrollbarStyleParseErrorOwned),
    Filter(CssStyleFilterParseErrorOwned),
}

// Implement `to_contained` and `to_shared` for CssParsingError
impl<'a> CssParsingError<'a> {
    pub fn to_contained(&self) -> CssParsingErrorOwned {
        match self {
            CssParsingError::CssBorderParseError(e) => {
                CssParsingErrorOwned::CssBorderParseError(e.to_contained())
            }
            CssParsingError::CssShadowParseError(e) => {
                CssParsingErrorOwned::CssShadowParseError(e.to_contained())
            }
            CssParsingError::InvalidValueErr(e) => {
                CssParsingErrorOwned::InvalidValueErr(e.to_contained())
            }
            CssParsingError::PixelParseError(e) => {
                CssParsingErrorOwned::PixelParseError(e.to_contained())
            }
            CssParsingError::PercentageParseError(e) => {
                CssParsingErrorOwned::PercentageParseError(e.clone())
            }
            CssParsingError::CssImageParseError(e) => {
                CssParsingErrorOwned::CssImageParseError(e.to_contained())
            }
            CssParsingError::CssStyleFontFamilyParseError(e) => {
                CssParsingErrorOwned::CssStyleFontFamilyParseError(e.to_contained())
            }
            CssParsingError::CssBackgroundParseError(e) => {
                CssParsingErrorOwned::CssBackgroundParseError(e.to_contained())
            }
            CssParsingError::CssColorParseError(e) => {
                CssParsingErrorOwned::CssColorParseError(e.to_contained())
            }
            CssParsingError::CssStyleBorderRadiusParseError(e) => {
                CssParsingErrorOwned::CssStyleBorderRadiusParseError(e.to_contained())
            }
            CssParsingError::PaddingParseError(e) => {
                CssParsingErrorOwned::PaddingParseError(e.to_contained())
            }
            CssParsingError::MarginParseError(e) => {
                CssParsingErrorOwned::MarginParseError(e.to_contained())
            }
            CssParsingError::FlexShrinkParseError(e) => {
                CssParsingErrorOwned::FlexShrinkParseError(e.to_contained())
            }
            CssParsingError::FlexGrowParseError(e) => {
                CssParsingErrorOwned::FlexGrowParseError(e.to_contained())
            }
            CssParsingError::BackgroundPositionParseError(e) => {
                CssParsingErrorOwned::BackgroundPositionParseError(e.to_contained())
            }
            CssParsingError::TransformParseError(e) => {
                CssParsingErrorOwned::TransformParseError(e.to_contained())
            }
            CssParsingError::TransformOriginParseError(e) => {
                CssParsingErrorOwned::TransformOriginParseError(e.to_contained())
            }
            CssParsingError::PerspectiveOriginParseError(e) => {
                CssParsingErrorOwned::PerspectiveOriginParseError(e.to_contained())
            }
            CssParsingError::Opacity(e) => CssParsingErrorOwned::Opacity(e.to_contained()),
            CssParsingError::Scrollbar(e) => CssParsingErrorOwned::Scrollbar(e.to_contained()),
            CssParsingError::Filter(e) => CssParsingErrorOwned::Filter(e.to_contained()),
        }
    }
}

impl CssParsingErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParsingError<'a> {
        match self {
            CssParsingErrorOwned::CssBorderParseError(e) => {
                CssParsingError::CssBorderParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssShadowParseError(e) => {
                CssParsingError::CssShadowParseError(e.to_shared())
            }
            CssParsingErrorOwned::InvalidValueErr(e) => {
                CssParsingError::InvalidValueErr(e.to_shared())
            }
            CssParsingErrorOwned::PixelParseError(e) => {
                CssParsingError::PixelParseError(e.to_shared())
            }
            CssParsingErrorOwned::PercentageParseError(e) => {
                CssParsingError::PercentageParseError(e.clone())
            }
            CssParsingErrorOwned::CssImageParseError(e) => {
                CssParsingError::CssImageParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssStyleFontFamilyParseError(e) => {
                CssParsingError::CssStyleFontFamilyParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssBackgroundParseError(e) => {
                CssParsingError::CssBackgroundParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssColorParseError(e) => {
                CssParsingError::CssColorParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssStyleBorderRadiusParseError(e) => {
                CssParsingError::CssStyleBorderRadiusParseError(e.to_shared())
            }
            CssParsingErrorOwned::PaddingParseError(e) => {
                CssParsingError::PaddingParseError(e.to_shared())
            }
            CssParsingErrorOwned::MarginParseError(e) => {
                CssParsingError::MarginParseError(e.to_shared())
            }
            CssParsingErrorOwned::FlexShrinkParseError(e) => {
                CssParsingError::FlexShrinkParseError(e.to_shared())
            }
            CssParsingErrorOwned::FlexGrowParseError(e) => {
                CssParsingError::FlexGrowParseError(e.to_shared())
            }
            CssParsingErrorOwned::BackgroundPositionParseError(e) => {
                CssParsingError::BackgroundPositionParseError(e.to_shared())
            }
            CssParsingErrorOwned::TransformParseError(e) => {
                CssParsingError::TransformParseError(e.to_shared())
            }
            CssParsingErrorOwned::TransformOriginParseError(e) => {
                CssParsingError::TransformOriginParseError(e.to_shared())
            }
            CssParsingErrorOwned::PerspectiveOriginParseError(e) => {
                CssParsingError::PerspectiveOriginParseError(e.to_shared())
            }
            CssParsingErrorOwned::Opacity(e) => CssParsingError::Opacity(e.to_shared()),
            CssParsingErrorOwned::Scrollbar(e) => CssParsingError::Scrollbar(e.to_shared()),
            CssParsingErrorOwned::Filter(e) => CssParsingError::Filter(e.to_shared()),
        }
    }
}

/// Simple "invalid value" error, used for
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of InvalidValueErr with String.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidValueErrOwned(pub String);

impl<'a> InvalidValueErr<'a> {
    pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned(self.0.to_string())
    }
}

impl InvalidValueErrOwned {
    pub fn to_shared<'a>(&'a self) -> InvalidValueErr<'a> {
        InvalidValueErr(&self.0)
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleBorderRadiusParseError<'a> {
    TooManyValues(&'a str),
    CssPixelValueParseError(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssStyleBorderRadiusParseError<'a>);
impl_display! { CssStyleBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values: \"{}\"", val),
    CssPixelValueParseError(e) => format!("{}", e),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleBorderRadiusParseError::CssPixelValueParseError
);

// Owned version
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleBorderRadiusParseErrorOwned {
    TooManyValues(String),
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStyleBorderRadiusParseError<'a> {
    pub fn to_contained(&self) -> CssStyleBorderRadiusParseErrorOwned {
        match self {
            CssStyleBorderRadiusParseError::TooManyValues(s) => {
                CssStyleBorderRadiusParseErrorOwned::TooManyValues(s.to_string())
            }
            CssStyleBorderRadiusParseError::CssPixelValueParseError(e) => {
                CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleBorderRadiusParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleBorderRadiusParseError<'a> {
        match self {
            CssStyleBorderRadiusParseErrorOwned::TooManyValues(s) => {
                CssStyleBorderRadiusParseError::TooManyValues(s)
            }
            CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e) => {
                CssStyleBorderRadiusParseError::CssPixelValueParseError(e.to_shared())
            }
        }
    }
}

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
impl_display! {CssColorParseError<'a>, {
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

impl_from!(
    CssDirectionParseError<'a>,
    CssColorParseError::DirectionParseError
);

#[derive(Debug, Clone, PartialEq)]
pub enum CssColorParseErrorOwned {
    InvalidColor(String),
    InvalidFunctionName(String),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(String),
    UnclosedColor(String),
    EmptyInput,
    DirectionParseError(CssDirectionParseErrorOwned),
    UnsupportedDirection(String),
    InvalidPercentage(PercentageParseError),
}

impl<'a> CssColorParseError<'a> {
    pub fn to_contained(&self) -> CssColorParseErrorOwned {
        match self {
            CssColorParseError::InvalidColor(s) => {
                CssColorParseErrorOwned::InvalidColor(s.to_string())
            }
            CssColorParseError::InvalidFunctionName(s) => {
                CssColorParseErrorOwned::InvalidFunctionName(s.to_string())
            }
            CssColorParseError::InvalidColorComponent(n) => {
                CssColorParseErrorOwned::InvalidColorComponent(*n)
            }
            CssColorParseError::IntValueParseErr(e) => {
                CssColorParseErrorOwned::IntValueParseErr(e.clone())
            }
            CssColorParseError::FloatValueParseErr(e) => {
                CssColorParseErrorOwned::FloatValueParseErr(e.clone())
            }
            CssColorParseError::FloatValueOutOfRange(n) => {
                CssColorParseErrorOwned::FloatValueOutOfRange(*n)
            }
            CssColorParseError::MissingColorComponent(c) => {
                CssColorParseErrorOwned::MissingColorComponent(*c)
            }
            CssColorParseError::ExtraArguments(s) => {
                CssColorParseErrorOwned::ExtraArguments(s.to_string())
            }
            CssColorParseError::UnclosedColor(s) => {
                CssColorParseErrorOwned::UnclosedColor(s.to_string())
            }
            CssColorParseError::EmptyInput => CssColorParseErrorOwned::EmptyInput,
            CssColorParseError::DirectionParseError(e) => {
                CssColorParseErrorOwned::DirectionParseError(e.to_contained())
            }
            CssColorParseError::UnsupportedDirection(s) => {
                CssColorParseErrorOwned::UnsupportedDirection(s.to_string())
            }
            CssColorParseError::InvalidPercentage(e) => {
                CssColorParseErrorOwned::InvalidPercentage(e.clone())
            }
        }
    }
}

impl CssColorParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssColorParseError<'a> {
        match self {
            CssColorParseErrorOwned::InvalidColor(s) => CssColorParseError::InvalidColor(s),
            CssColorParseErrorOwned::InvalidFunctionName(s) => {
                CssColorParseError::InvalidFunctionName(s)
            }
            CssColorParseErrorOwned::InvalidColorComponent(n) => {
                CssColorParseError::InvalidColorComponent(*n)
            }
            CssColorParseErrorOwned::IntValueParseErr(e) => {
                CssColorParseError::IntValueParseErr(e.clone())
            }
            CssColorParseErrorOwned::FloatValueParseErr(e) => {
                CssColorParseError::FloatValueParseErr(e.clone())
            }
            CssColorParseErrorOwned::FloatValueOutOfRange(n) => {
                CssColorParseError::FloatValueOutOfRange(*n)
            }
            CssColorParseErrorOwned::MissingColorComponent(c) => {
                CssColorParseError::MissingColorComponent(*c)
            }
            CssColorParseErrorOwned::ExtraArguments(s) => CssColorParseError::ExtraArguments(s),
            CssColorParseErrorOwned::UnclosedColor(s) => CssColorParseError::UnclosedColor(s),
            CssColorParseErrorOwned::EmptyInput => CssColorParseError::EmptyInput,
            CssColorParseErrorOwned::DirectionParseError(e) => {
                CssColorParseError::DirectionParseError(e.to_shared())
            }
            CssColorParseErrorOwned::UnsupportedDirection(s) => {
                CssColorParseError::UnsupportedDirection(s)
            }
            CssColorParseErrorOwned::InvalidPercentage(e) => {
                CssColorParseError::InvalidPercentage(e.clone())
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum CssImageParseError<'a> {
    UnclosedQuotes(&'a str),
}

impl_debug_as_display!(CssImageParseError<'a>);
impl_display! {CssImageParseError<'a>, {
    UnclosedQuotes(e) => format!("Unclosed quotes: \"{}\"", e),
}}

/// Owned version of CssImageParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssImageParseErrorOwned {
    UnclosedQuotes(String),
}

impl<'a> CssImageParseError<'a> {
    pub fn to_contained(&self) -> CssImageParseErrorOwned {
        match self {
            CssImageParseError::UnclosedQuotes(s) => {
                CssImageParseErrorOwned::UnclosedQuotes(s.to_string())
            }
        }
    }
}

impl CssImageParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssImageParseError<'a> {
        match self {
            CssImageParseErrorOwned::UnclosedQuotes(s) => {
                CssImageParseError::UnclosedQuotes(s.as_str())
            }
        }
    }
}

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
    ThicknessParseError(CssPixelValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}
impl_debug_as_display!(CssBorderParseError<'a>);
impl_display! { CssBorderParseError<'a>, {
    MissingThickness(e) => format!("Missing border thickness: \"{}\"", e),
    InvalidBorderStyle(e) => format!("Invalid style: {}", e.0),
    InvalidBorderDeclaration(e) => format!("Invalid declaration: \"{}\"", e),
    ThicknessParseError(e) => format!("Invalid thickness: {}", e),
    ColorParseError(e) => format!("Invalid color: {}", e),
}}

/// Owned version of CssBorderParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderParseErrorOwned {
    MissingThickness(String),
    InvalidBorderStyle(InvalidValueErrOwned),
    InvalidBorderDeclaration(String),
    ThicknessParseError(CssPixelValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssBorderParseError<'a> {
    pub fn to_contained(&self) -> CssBorderParseErrorOwned {
        match self {
            CssBorderParseError::MissingThickness(s) => {
                CssBorderParseErrorOwned::MissingThickness(s.to_string())
            }
            CssBorderParseError::InvalidBorderStyle(e) => {
                CssBorderParseErrorOwned::InvalidBorderStyle(e.to_contained())
            }
            CssBorderParseError::InvalidBorderDeclaration(s) => {
                CssBorderParseErrorOwned::InvalidBorderDeclaration(s.to_string())
            }
            CssBorderParseError::ThicknessParseError(e) => {
                CssBorderParseErrorOwned::ThicknessParseError(e.to_contained())
            }
            CssBorderParseError::ColorParseError(e) => {
                CssBorderParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssBorderParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBorderParseError<'a> {
        match self {
            CssBorderParseErrorOwned::MissingThickness(s) => {
                CssBorderParseError::MissingThickness(s.as_str())
            }
            CssBorderParseErrorOwned::InvalidBorderStyle(e) => {
                CssBorderParseError::InvalidBorderStyle(e.to_shared())
            }
            CssBorderParseErrorOwned::InvalidBorderDeclaration(s) => {
                CssBorderParseError::InvalidBorderDeclaration(s.as_str())
            }
            CssBorderParseErrorOwned::ThicknessParseError(e) => {
                CssBorderParseError::ThicknessParseError(e.to_shared())
            }
            CssBorderParseErrorOwned::ColorParseError(e) => {
                CssBorderParseError::ColorParseError(e.to_shared())
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssShadowParseError<'a> {
    InvalidSingleStatement(&'a str),
    TooManyComponents(&'a str),
    ValueParseErr(CssPixelValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}
impl_debug_as_display!(CssShadowParseError<'a>);
impl_display! { CssShadowParseError<'a>, {
    InvalidSingleStatement(e) => format!("Invalid single statement: \"{}\"", e),
    TooManyComponents(e) => format!("Too many components: \"{}\"", e),
    ValueParseErr(e) => format!("Invalid value: {}", e),
    ColorParseError(e) => format!("Invalid color-value: {}", e),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    CssShadowParseError::ValueParseErr
);
impl_from!(CssColorParseError<'a>, CssShadowParseError::ColorParseError);

/// Owned version of CssShadowParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssShadowParseErrorOwned {
    InvalidSingleStatement(String),
    TooManyComponents(String),
    ValueParseErr(CssPixelValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssShadowParseError<'a> {
    pub fn to_contained(&self) -> CssShadowParseErrorOwned {
        match self {
            CssShadowParseError::InvalidSingleStatement(s) => {
                CssShadowParseErrorOwned::InvalidSingleStatement(s.to_string())
            }
            CssShadowParseError::TooManyComponents(s) => {
                CssShadowParseErrorOwned::TooManyComponents(s.to_string())
            }
            CssShadowParseError::ValueParseErr(e) => {
                CssShadowParseErrorOwned::ValueParseErr(e.to_contained())
            }
            CssShadowParseError::ColorParseError(e) => {
                CssShadowParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssShadowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssShadowParseError<'a> {
        match self {
            CssShadowParseErrorOwned::InvalidSingleStatement(s) => {
                CssShadowParseError::InvalidSingleStatement(s.as_str())
            }
            CssShadowParseErrorOwned::TooManyComponents(s) => {
                CssShadowParseError::TooManyComponents(s.as_str())
            }
            CssShadowParseErrorOwned::ValueParseErr(e) => {
                CssShadowParseError::ValueParseErr(e.to_shared())
            }
            CssShadowParseErrorOwned::ColorParseError(e) => {
                CssShadowParseError::ColorParseError(e.to_shared())
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct StyleBorderRadius {
    // TODO: Should technically be PixelSize because the border radius doesn't have to be uniform
    // but the parsing for that is complicated...
    pub top_left: PixelValue,
    pub top_right: PixelValue,
    pub bottom_left: PixelValue,
    pub bottom_right: PixelValue,
}

impl Default for StyleBorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

impl StyleBorderRadius {
    pub const fn zero() -> Self {
        Self::uniform(PixelValue::zero())
    }

    pub const fn uniform(value: PixelValue) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_left: value,
            bottom_right: value,
        }
    }
}

/// parse the border-radius like "5px 10px" or "5px 10px 6px 10px"
pub fn parse_style_border_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderRadius, CssStyleBorderRadiusParseError<'a>> {
    let mut components = input.split_whitespace();
    let len = components.clone().count();

    match len {
        1 => {
            // One value - border-radius: 15px;
            // (the value applies to all four corners, which are rounded equally:

            let uniform_radius = parse_pixel_value(components.next().unwrap())?;
            Ok(StyleBorderRadius::uniform(uniform_radius))
        }
        2 => {
            // Two values - border-radius: 15px 50px;
            // (first value applies to top-left and bottom-right corners,
            // and the second value applies to top-right and bottom-left corners):

            let top_left_bottom_right = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left: top_left_bottom_right,
                bottom_right: top_left_bottom_right,
                top_right: top_right_bottom_left,
                bottom_left: top_right_bottom_left,
            })
        }
        3 => {
            // Three values - border-radius: 15px 50px 30px;
            // (first value applies to top-left corner,
            // second value applies to top-right and bottom-left corners,
            // and third value applies to bottom-right corner):
            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left,
                bottom_right,
                top_right: top_right_bottom_left,
                bottom_left: top_right_bottom_left,
            })
        }
        4 => {
            // Four values - border-radius: 15px 50px 30px 5px;
            //
            // first value applies to top-left corner,
            // second value applies to top-right corner,
            // third value applies to bottom-right corner,
            // fourth value applies to bottom-left corner

            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left,
                bottom_right,
                top_right,
                bottom_left,
            })
        }
        _ => Err(CssStyleBorderRadiusParseError::TooManyValues(input)),
    }
}

#[derive(Clone, PartialEq)]
pub enum CssPixelValueParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str, SizeMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidPixelValue(&'a str),
}

impl_debug_as_display!(CssPixelValueParseError<'a>);

impl_display! { CssPixelValueParseError<'a>, {
    EmptyString => format!("Missing [px / pt / em / %] value"),
    NoValueGiven(input, metric) => format!("Expected floating-point pixel value, got: \"{}{}\"", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
    InvalidPixelValue(s) => format!("Invalid pixel value: \"{}\"", s),
}}

/// Owned version of CssPixelValueParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssPixelValueParseErrorOwned {
    EmptyString,
    NoValueGiven(String, SizeMetric),
    ValueParseErr(ParseFloatError, String),
    InvalidPixelValue(String),
}

impl<'a> CssPixelValueParseError<'a> {
    pub fn to_contained(&self) -> CssPixelValueParseErrorOwned {
        match self {
            CssPixelValueParseError::EmptyString => CssPixelValueParseErrorOwned::EmptyString,
            CssPixelValueParseError::NoValueGiven(s, metric) => {
                CssPixelValueParseErrorOwned::NoValueGiven(s.to_string(), *metric)
            }
            CssPixelValueParseError::ValueParseErr(err, s) => {
                CssPixelValueParseErrorOwned::ValueParseErr(err.clone(), s.to_string())
            }
            CssPixelValueParseError::InvalidPixelValue(s) => {
                CssPixelValueParseErrorOwned::InvalidPixelValue(s.to_string())
            }
        }
    }
}

impl CssPixelValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssPixelValueParseError<'a> {
        match self {
            CssPixelValueParseErrorOwned::EmptyString => CssPixelValueParseError::EmptyString,
            CssPixelValueParseErrorOwned::NoValueGiven(s, metric) => {
                CssPixelValueParseError::NoValueGiven(s.as_str(), *metric)
            }
            CssPixelValueParseErrorOwned::ValueParseErr(err, s) => {
                CssPixelValueParseError::ValueParseErr(err.clone(), s.as_str())
            }
            CssPixelValueParseErrorOwned::InvalidPixelValue(s) => {
                CssPixelValueParseError::InvalidPixelValue(s.as_str())
            }
        }
    }
}

/// parses an angle value like `30deg`, `1.64rad`, `100%`, etc.
fn parse_pixel_value_inner<'a>(
    input: &'a str,
    match_values: &[(&'static str, SizeMetric)],
) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssPixelValueParseError::EmptyString);
    }

    for (match_val, metric) in match_values {
        if input.ends_with(match_val) {
            let value = &input[..input.len() - match_val.len()];
            let value = value.trim();
            if value.is_empty() {
                return Err(CssPixelValueParseError::NoValueGiven(input, *metric));
            }
            match value.parse::<f32>() {
                Ok(o) => {
                    return Ok(PixelValue::from_metric(*metric, o));
                }
                Err(e) => {
                    return Err(CssPixelValueParseError::ValueParseErr(e, value));
                }
            }
        }
    }

    Err(CssPixelValueParseError::InvalidPixelValue(input))
}

pub fn parse_pixel_value<'a>(input: &'a str) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    parse_pixel_value_inner(
        input,
        &[
            ("px", SizeMetric::Px),
            ("em", SizeMetric::Em),
            ("pt", SizeMetric::Pt),
            ("%", SizeMetric::Percent),
        ],
    )
}

pub fn parse_pixel_value_no_percent<'a>(
    input: &'a str,
) -> Result<PixelValueNoPercent, CssPixelValueParseError<'a>> {
    Ok(PixelValueNoPercent {
        inner: parse_pixel_value_inner(
            input,
            &[
                ("px", SizeMetric::Px),
                ("em", SizeMetric::Em),
                ("pt", SizeMetric::Pt),
            ],
        )?,
    })
}

#[derive(Clone, PartialEq, Eq)]
pub enum PercentageParseError {
    ValueParseErr(ParseFloatError),
    NoPercentSign,
    InvalidUnit(AzString),
}

impl_debug_as_display!(PercentageParseError);
impl_from!(ParseFloatError, PercentageParseError::ValueParseErr);

impl_display! { PercentageParseError, {
    ValueParseErr(e) => format!("\"{}\"", e),
    NoPercentSign => format!("No percent sign after number"),
    InvalidUnit(u) => format!("Error parsing percentage: invalid unit \"{}\"", u.as_str()),
}}

// Parse "1.2" or "120%" (similar to parse_pixel_value)
pub fn parse_percentage_value(input: &str) -> Result<PercentageValue, PercentageParseError> {
    let mut split_pos = 0;
    for (idx, ch) in input.char_indices() {
        if ch.is_numeric() || ch == '.' {
            split_pos = idx;
        }
    }

    split_pos += 1;

    let unit = &input[split_pos..];
    let mut number = input[..split_pos]
        .parse::<f32>()
        .map_err(|e| PercentageParseError::ValueParseErr(e))?;

    match unit {
        "" => {
            number *= 100.0;
        } // 0.5 => 50%
        "%" => {} // 50% => PercentageValue(50.0)
        other => {
            return Err(PercentageParseError::InvalidUnit(other.to_string().into()));
        }
    }

    Ok(PercentageValue::new(number))
}

/// Parse any valid CSS color, INCLUDING THE HASH
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
/// "#00FF00" -> ColorF { r: 0, g: 255, b: 0 })
pub fn parse_css_color<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let input = input.trim();
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        use self::ParenthesisParseError::*;

        match parse_parentheses(input, &["rgba", "rgb", "hsla", "hsl"]) {
            Ok((stopword, inner_value)) => match stopword {
                "rgba" => parse_color_rgb(inner_value, true),
                "rgb" => parse_color_rgb(inner_value, false),
                "hsla" => parse_color_hsl(inner_value, true),
                "hsl" => parse_color_hsl(inner_value, false),
                _ => unreachable!(),
            },
            Err(e) => match e {
                UnclosedBraces => Err(CssColorParseError::UnclosedColor(input)),
                EmptyInput => Err(CssColorParseError::EmptyInput),
                StopWordNotFound(stopword) => {
                    Err(CssColorParseError::InvalidFunctionName(stopword))
                }
                NoClosingBraceFound => Err(CssColorParseError::UnclosedColor(input)),
                NoOpeningBraceFound => parse_color_builtin(input),
            },
        }
    }
}

/// Formats a ColorU in hex format
pub fn css_color_to_string(color: ColorU, prefix_hash: bool) -> String {
    let prefix = if prefix_hash { "#" } else { "" };
    let alpha = if color.a == 255 {
        String::new()
    } else {
        format!("{:02x}", color.a)
    };
    format!(
        "{}{:02x}{:02x}{:02x}{}",
        prefix, color.r, color.g, color.b, alpha
    )
}

pub fn parse_float_value(input: &str) -> Result<FloatValue, ParseFloatError> {
    Ok(FloatValue::new(input.trim().parse::<f32>()?))
}

pub fn parse_style_text_color<'a>(
    input: &'a str,
) -> Result<StyleTextColor, CssColorParseError<'a>> {
    parse_css_color(input).and_then(|ok| Ok(StyleTextColor { inner: ok }))
}

/// Parse a built-in background color
///
/// "blue" -> "00FF00" -> ColorF { r: 0, g: 255, b: 0 })
pub fn parse_color_builtin<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let (r, g, b, a) = match input {
        "AliceBlue" | "aliceblue" => (240, 248, 255, 255),
        "AntiqueWhite" | "antiquewhite" => (250, 235, 215, 255),
        "Aqua" | "aqua" => (0, 255, 255, 255),
        "Aquamarine" | "aquamarine" => (127, 255, 212, 255),
        "Azure" | "azure" => (240, 255, 255, 255),
        "Beige" | "beige" => (245, 245, 220, 255),
        "Bisque" | "bisque" => (255, 228, 196, 255),
        "Black" | "black" => (0, 0, 0, 255),
        "BlanchedAlmond" | "blanchedalmond" => (255, 235, 205, 255),
        "Blue" | "blue" => (0, 0, 255, 255),
        "BlueViolet" | "blueviolet" => (138, 43, 226, 255),
        "Brown" | "brown" => (165, 42, 42, 255),
        "BurlyWood" | "burlywood" => (222, 184, 135, 255),
        "CadetBlue" | "cadetblue" => (95, 158, 160, 255),
        "Chartreuse" | "chartreuse" => (127, 255, 0, 255),
        "Chocolate" | "chocolate" => (210, 105, 30, 255),
        "Coral" | "coral" => (255, 127, 80, 255),
        "CornflowerBlue" | "cornflowerblue" => (100, 149, 237, 255),
        "Cornsilk" | "cornsilk" => (255, 248, 220, 255),
        "Crimson" | "crimson" => (220, 20, 60, 255),
        "Cyan" | "cyan" => (0, 255, 255, 255),
        "DarkBlue" | "darkblue" => (0, 0, 139, 255),
        "DarkCyan" | "darkcyan" => (0, 139, 139, 255),
        "DarkGoldenRod" | "darkgoldenrod" => (184, 134, 11, 255),
        "DarkGray" | "darkgray" => (169, 169, 169, 255),
        "DarkGrey" | "darkgrey" => (169, 169, 169, 255),
        "DarkGreen" | "darkgreen" => (0, 100, 0, 255),
        "DarkKhaki" | "darkkhaki" => (189, 183, 107, 255),
        "DarkMagenta" | "darkmagenta" => (139, 0, 139, 255),
        "DarkOliveGreen" | "darkolivegreen" => (85, 107, 47, 255),
        "DarkOrange" | "darkorange" => (255, 140, 0, 255),
        "DarkOrchid" | "darkorchid" => (153, 50, 204, 255),
        "DarkRed" | "darkred" => (139, 0, 0, 255),
        "DarkSalmon" | "darksalmon" => (233, 150, 122, 255),
        "DarkSeaGreen" | "darkseagreen" => (143, 188, 143, 255),
        "DarkSlateBlue" | "darkslateblue" => (72, 61, 139, 255),
        "DarkSlateGray" | "darkslategray" => (47, 79, 79, 255),
        "DarkSlateGrey" | "darkslategrey" => (47, 79, 79, 255),
        "DarkTurquoise" | "darkturquoise" => (0, 206, 209, 255),
        "DarkViolet" | "darkviolet" => (148, 0, 211, 255),
        "DeepPink" | "deeppink" => (255, 20, 147, 255),
        "DeepSkyBlue" | "deepskyblue" => (0, 191, 255, 255),
        "DimGray" | "dimgray" => (105, 105, 105, 255),
        "DimGrey" | "dimgrey" => (105, 105, 105, 255),
        "DodgerBlue" | "dodgerblue" => (30, 144, 255, 255),
        "FireBrick" | "firebrick" => (178, 34, 34, 255),
        "FloralWhite" | "floralwhite" => (255, 250, 240, 255),
        "ForestGreen" | "forestgreen" => (34, 139, 34, 255),
        "Fuchsia" | "fuchsia" => (255, 0, 255, 255),
        "Gainsboro" | "gainsboro" => (220, 220, 220, 255),
        "GhostWhite" | "ghostwhite" => (248, 248, 255, 255),
        "Gold" | "gold" => (255, 215, 0, 255),
        "GoldenRod" | "goldenrod" => (218, 165, 32, 255),
        "Gray" | "gray" => (128, 128, 128, 255),
        "Grey" | "grey" => (128, 128, 128, 255),
        "Green" | "green" => (0, 128, 0, 255),
        "GreenYellow" | "greenyellow" => (173, 255, 47, 255),
        "HoneyDew" | "honeydew" => (240, 255, 240, 255),
        "HotPink" | "hotpink" => (255, 105, 180, 255),
        "IndianRed" | "indianred" => (205, 92, 92, 255),
        "Indigo" | "indigo" => (75, 0, 130, 255),
        "Ivory" | "ivory" => (255, 255, 240, 255),
        "Khaki" | "khaki" => (240, 230, 140, 255),
        "Lavender" | "lavender" => (230, 230, 250, 255),
        "LavenderBlush" | "lavenderblush" => (255, 240, 245, 255),
        "LawnGreen" | "lawngreen" => (124, 252, 0, 255),
        "LemonChiffon" | "lemonchiffon" => (255, 250, 205, 255),
        "LightBlue" | "lightblue" => (173, 216, 230, 255),
        "LightCoral" | "lightcoral" => (240, 128, 128, 255),
        "LightCyan" | "lightcyan" => (224, 255, 255, 255),
        "LightGoldenRodYellow" | "lightgoldenrodyellow" => (250, 250, 210, 255),
        "LightGray" | "lightgray" => (211, 211, 211, 255),
        "LightGrey" | "lightgrey" => (144, 238, 144, 255),
        "LightGreen" | "lightgreen" => (211, 211, 211, 255),
        "LightPink" | "lightpink" => (255, 182, 193, 255),
        "LightSalmon" | "lightsalmon" => (255, 160, 122, 255),
        "LightSeaGreen" | "lightseagreen" => (32, 178, 170, 255),
        "LightSkyBlue" | "lightskyblue" => (135, 206, 250, 255),
        "LightSlateGray" | "lightslategray" => (119, 136, 153, 255),
        "LightSlateGrey" | "lightslategrey" => (119, 136, 153, 255),
        "LightSteelBlue" | "lightsteelblue" => (176, 196, 222, 255),
        "LightYellow" | "lightyellow" => (255, 255, 224, 255),
        "Lime" | "lime" => (0, 255, 0, 255),
        "LimeGreen" | "limegreen" => (50, 205, 50, 255),
        "Linen" | "linen" => (250, 240, 230, 255),
        "Magenta" | "magenta" => (255, 0, 255, 255),
        "Maroon" | "maroon" => (128, 0, 0, 255),
        "MediumAquaMarine" | "mediumaquamarine" => (102, 205, 170, 255),
        "MediumBlue" | "mediumblue" => (0, 0, 205, 255),
        "MediumOrchid" | "mediumorchid" => (186, 85, 211, 255),
        "MediumPurple" | "mediumpurple" => (147, 112, 219, 255),
        "MediumSeaGreen" | "mediumseagreen" => (60, 179, 113, 255),
        "MediumSlateBlue" | "mediumslateblue" => (123, 104, 238, 255),
        "MediumSpringGreen" | "mediumspringgreen" => (0, 250, 154, 255),
        "MediumTurquoise" | "mediumturquoise" => (72, 209, 204, 255),
        "MediumVioletRed" | "mediumvioletred" => (199, 21, 133, 255),
        "MidnightBlue" | "midnightblue" => (25, 25, 112, 255),
        "MintCream" | "mintcream" => (245, 255, 250, 255),
        "MistyRose" | "mistyrose" => (255, 228, 225, 255),
        "Moccasin" | "moccasin" => (255, 228, 181, 255),
        "NavajoWhite" | "navajowhite" => (255, 222, 173, 255),
        "Navy" | "navy" => (0, 0, 128, 255),
        "OldLace" | "oldlace" => (253, 245, 230, 255),
        "Olive" | "olive" => (128, 128, 0, 255),
        "OliveDrab" | "olivedrab" => (107, 142, 35, 255),
        "Orange" | "orange" => (255, 165, 0, 255),
        "OrangeRed" | "orangered" => (255, 69, 0, 255),
        "Orchid" | "orchid" => (218, 112, 214, 255),
        "PaleGoldenRod" | "palegoldenrod" => (238, 232, 170, 255),
        "PaleGreen" | "palegreen" => (152, 251, 152, 255),
        "PaleTurquoise" | "paleturquoise" => (175, 238, 238, 255),
        "PaleVioletRed" | "palevioletred" => (219, 112, 147, 255),
        "PapayaWhip" | "papayawhip" => (255, 239, 213, 255),
        "PeachPuff" | "peachpuff" => (255, 218, 185, 255),
        "Peru" | "peru" => (205, 133, 63, 255),
        "Pink" | "pink" => (255, 192, 203, 255),
        "Plum" | "plum" => (221, 160, 221, 255),
        "PowderBlue" | "powderblue" => (176, 224, 230, 255),
        "Purple" | "purple" => (128, 0, 128, 255),
        "RebeccaPurple" | "rebeccapurple" => (102, 51, 153, 255),
        "Red" | "red" => (255, 0, 0, 255),
        "RosyBrown" | "rosybrown" => (188, 143, 143, 255),
        "RoyalBlue" | "royalblue" => (65, 105, 225, 255),
        "SaddleBrown" | "saddlebrown" => (139, 69, 19, 255),
        "Salmon" | "salmon" => (250, 128, 114, 255),
        "SandyBrown" | "sandybrown" => (244, 164, 96, 255),
        "SeaGreen" | "seagreen" => (46, 139, 87, 255),
        "SeaShell" | "seashell" => (255, 245, 238, 255),
        "Sienna" | "sienna" => (160, 82, 45, 255),
        "Silver" | "silver" => (192, 192, 192, 255),
        "SkyBlue" | "skyblue" => (135, 206, 235, 255),
        "SlateBlue" | "slateblue" => (106, 90, 205, 255),
        "SlateGray" | "slategray" => (112, 128, 144, 255),
        "SlateGrey" | "slategrey" => (112, 128, 144, 255),
        "Snow" | "snow" => (255, 250, 250, 255),
        "SpringGreen" | "springgreen" => (0, 255, 127, 255),
        "SteelBlue" | "steelblue" => (70, 130, 180, 255),
        "Tan" | "tan" => (210, 180, 140, 255),
        "Teal" | "teal" => (0, 128, 128, 255),
        "Thistle" | "thistle" => (216, 191, 216, 255),
        "Tomato" | "tomato" => (255, 99, 71, 255),
        "Turquoise" | "turquoise" => (64, 224, 208, 255),
        "Violet" | "violet" => (238, 130, 238, 255),
        "Wheat" | "wheat" => (245, 222, 179, 255),
        "White" | "white" => (255, 255, 255, 255),
        "WhiteSmoke" | "whitesmoke" => (245, 245, 245, 255),
        "Yellow" | "yellow" => (255, 255, 0, 255),
        "YellowGreen" | "yellowgreen" => (154, 205, 50, 255),
        "Transparent" | "transparent" => (255, 255, 255, 0),
        _ => {
            return Err(CssColorParseError::InvalidColor(input));
        }
    };
    Ok(ColorU { r, g, b, a })
}

/// Parse a color of the form `rgb([0-255], [0-255], [0-255])`, or `rgba([0-255], [0-255], [0-255],
/// [0.0-1.0])` without the leading `rgb[a](` or trailing `)`. Alpha defaults to 255.
pub fn parse_color_rgb<'a>(
    input: &'a str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
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
pub fn parse_color_rgb_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn component_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<u8, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
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
        a: 255,
    })
}

/// Parse a color of the form 'hsl([0.0-360.0]deg, [0-100]%, [0-100]%)', or 'hsla([0.0-360.0]deg,
/// [0-100]%, [0-100]%, [0.0-1.0])' without the leading 'hsl[a](' or trailing ')'. Alpha defaults to
/// 255.
pub fn parse_color_hsl<'a>(
    input: &'a str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
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
pub fn parse_color_hsl_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn angle_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        let dir = parse_direction(c)?;
        match dir {
            Direction::Angle(deg) => Ok(deg.to_degrees()),
            Direction::FromTo(_) => return Err(CssColorParseError::UnsupportedDirection(c)),
        }
    }

    #[inline]
    fn percent_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }

        let parsed_percent =
            parse_percentage(c).map_err(|e| CssColorParseError::InvalidPercentage(e))?;

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

fn parse_alpha_component<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<u8, CssColorParseError<'a>> {
    let a = components
        .next()
        .ok_or(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ))?;
    if a.is_empty() {
        return Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ));
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
pub fn parse_color_no_hash<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn from_hex<'a>(c: u8) -> Result<u8, CssColorParseError<'a>> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(CssColorParseError::InvalidColorComponent(c)),
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

            Ok(ColorU { r, g, b, a: 255 })
        }
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

            Ok(ColorU { r, g, b, a })
        }
        6 => {
            let input = u32::from_str_radix(input, 16)
                .map_err(|e| CssColorParseError::IntValueParseErr(e))?;
            Ok(ColorU {
                r: ((input >> 16) & 255) as u8,
                g: ((input >> 8) & 255) as u8,
                b: (input & 255) as u8,
                a: 255,
            })
        }
        8 => {
            let input = u32::from_str_radix(input, 16)
                .map_err(|e| CssColorParseError::IntValueParseErr(e))?;
            Ok(ColorU {
                r: ((input >> 24) & 255) as u8,
                g: ((input >> 16) & 255) as u8,
                b: ((input >> 8) & 255) as u8,
                a: (input & 255) as u8,
            })
        }
        _ => Err(CssColorParseError::InvalidColor(input)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseError<'a> {
    CssPixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display! { LayoutPaddingParseError<'a>, {
    CssPixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - padding property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - padding property has a minimum of 1 value."),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    LayoutPaddingParseError::CssPixelValueParseError
);

/// Owned version of LayoutPaddingParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseErrorOwned {
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

impl<'a> LayoutPaddingParseError<'a> {
    pub fn to_contained(&self) -> LayoutPaddingParseErrorOwned {
        match self {
            LayoutPaddingParseError::CssPixelValueParseError(e) => {
                LayoutPaddingParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
            LayoutPaddingParseError::TooManyValues => LayoutPaddingParseErrorOwned::TooManyValues,
            LayoutPaddingParseError::TooFewValues => LayoutPaddingParseErrorOwned::TooFewValues,
        }
    }
}

impl LayoutPaddingParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutPaddingParseError<'a> {
        match self {
            LayoutPaddingParseErrorOwned::CssPixelValueParseError(e) => {
                LayoutPaddingParseError::CssPixelValueParseError(e.to_shared())
            }
            LayoutPaddingParseErrorOwned::TooManyValues => LayoutPaddingParseError::TooManyValues,
            LayoutPaddingParseErrorOwned::TooFewValues => LayoutPaddingParseError::TooFewValues,
        }
    }
}

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPadding {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

/// Parse a padding value such as
///
/// "10px 10px"
pub fn parse_layout_padding<'a>(input: &'a str) -> Result<LayoutPadding, LayoutPaddingParseError> {
    let mut input_iter = input.split_whitespace();
    let first = parse_pixel_value_with_auto(
        input_iter
            .next()
            .ok_or(LayoutPaddingParseError::TooFewValues)?,
    )?;
    let second = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                bottom: first,
                left: first,
                right: first,
            });
        }
    })?;
    let third = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                bottom: first,
                left: second,
                right: second,
            });
        }
    })?;
    let fourth = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                left: second,
                right: second,
                bottom: third,
            });
        }
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
    CssPixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display! { LayoutMarginParseError<'a>, {
    CssPixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - margin property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - margin property has a minimum of 1 value."),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    LayoutMarginParseError::CssPixelValueParseError
);

/// Owned version of LayoutMarginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseErrorOwned {
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

impl<'a> LayoutMarginParseError<'a> {
    pub fn to_contained(&self) -> LayoutMarginParseErrorOwned {
        match self {
            LayoutMarginParseError::CssPixelValueParseError(e) => {
                LayoutMarginParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
            LayoutMarginParseError::TooManyValues => LayoutMarginParseErrorOwned::TooManyValues,
            LayoutMarginParseError::TooFewValues => LayoutMarginParseErrorOwned::TooFewValues,
        }
    }
}

impl LayoutMarginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutMarginParseError<'a> {
        match self {
            LayoutMarginParseErrorOwned::CssPixelValueParseError(e) => {
                LayoutMarginParseError::CssPixelValueParseError(e.to_shared())
            }
            LayoutMarginParseErrorOwned::TooManyValues => LayoutMarginParseError::TooManyValues,
            LayoutMarginParseErrorOwned::TooFewValues => LayoutMarginParseError::TooFewValues,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PixelValueWithAuto {
    None,
    Initial,
    Inherit,
    Auto,
    Exact(PixelValue),
}

/// Parses a pixel value, but also tries values like "auto", "initial", "inherit" and "none"
pub fn parse_pixel_value_with_auto<'a>(
    input: &'a str,
) -> Result<PixelValueWithAuto, CssPixelValueParseError<'a>> {
    let input = input.trim();
    match input {
        "none" => Ok(PixelValueWithAuto::None),
        "initial" => Ok(PixelValueWithAuto::Initial),
        "inherit" => Ok(PixelValueWithAuto::Inherit),
        "auto" => Ok(PixelValueWithAuto::Auto),
        e => Ok(PixelValueWithAuto::Exact(parse_pixel_value(e)?)),
    }
}

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMargin {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

pub fn parse_layout_margin<'a>(input: &'a str) -> Result<LayoutMargin, LayoutMarginParseError> {
    match parse_layout_padding(input) {
        Ok(padding) => Ok(LayoutMargin {
            top: padding.top,
            left: padding.left,
            right: padding.right,
            bottom: padding.bottom,
        }),
        Err(LayoutPaddingParseError::CssPixelValueParseError(e)) => Err(e.into()),
        Err(LayoutPaddingParseError::TooManyValues) => Err(LayoutMarginParseError::TooManyValues),
        Err(LayoutPaddingParseError::TooFewValues) => Err(LayoutMarginParseError::TooFewValues),
    }
}

const DEFAULT_BORDER_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
// Default border thickness on the web seems to be 3px
const DEFAULT_BORDER_THICKNESS: PixelValue = PixelValue::const_px(3);

use core::str::CharIndices;

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
pub fn parse_style_border<'a>(input: &'a str) -> Result<StyleBorderSide, CssBorderParseError<'a>> {
    use self::CssBorderParseError::*;

    let input = input.trim();

    // The first argument can either be a style or a pixel value

    let mut char_iter = input.char_indices();
    let first_arg_end =
        take_until_next_whitespace(&mut char_iter).ok_or(MissingThickness(input))?;
    let first_arg_str = &input[0..first_arg_end];

    advance_until_next_char(&mut char_iter);

    let second_argument_end = take_until_next_whitespace(&mut char_iter);
    let (border_width, border_width_str_end, border_style);

    match second_argument_end {
        None => {
            // First argument is the one and only argument, therefore has to be a style such as
            // "double"
            border_style =
                parse_style_border_style(first_arg_str).map_err(|e| InvalidBorderStyle(e))?;
            return Ok(StyleBorderSide {
                border_style,
                border_width: DEFAULT_BORDER_THICKNESS,
                border_color: DEFAULT_BORDER_COLOR,
            });
        }
        Some(end) => {
            // First argument is a pixel value, second argument is the border style
            border_width = parse_pixel_value(first_arg_str).map_err(|e| ThicknessParseError(e))?;
            let border_style_str = &input[first_arg_end..end];
            border_style =
                parse_style_border_style(border_style_str).map_err(|e| InvalidBorderStyle(e))?;
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

/// Parses a CSS box-shadow, such as "5px 10px inset"
pub fn parse_style_box_shadow<'a>(
    input: &'a str,
) -> Result<StyleBoxShadow, CssShadowParseError<'a>> {
    let mut input_iter = input.split_whitespace();
    let count = input_iter.clone().count();

    let mut box_shadow = StyleBoxShadow {
        offset: [
            PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
        ],
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        blur_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        spread_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
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
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;
        }
        3 => {
            // box-shadow: 5px 10px inset; (h_offset, v_offset, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                // box-shadow: 5px 10px #888888; (h_offset, v_offset, color)
                let color = parse_css_color(input_iter.next().unwrap())?;
                box_shadow.color = color;
            }
        }
        4 => {
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
                box_shadow.blur_radius = blur.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        5 => {
            // box-shadow: 5px 10px 5px 10px #888888; (h_offset, v_offset, blur, spread, color)
            // box-shadow: 5px 10px 5px #888888 inset; (h_offset, v_offset, blur, color, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            if !is_inset {
                let spread = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
                box_shadow.spread_radius = spread.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        6 => {
            // box-shadow: 5px 10px 5px 10px #888888 inset; (h_offset, v_offset, blur, spread,
            // color, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            let spread = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
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
    ConicGradient(CssConicGradientParseError<'a>),
    ShapeParseError(CssShapeParseError<'a>),
    ImageParseError(CssImageParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssBackgroundParseError<'a>);
impl_display! { CssBackgroundParseError<'a>, {
    Error(e) => e,
    InvalidBackground(val) => format!("Invalid background value: \"{}\"", val),
    UnclosedGradient(val) => format!("Unclosed gradient: \"{}\"", val),
    NoDirection(val) => format!("Gradient has no direction: \"{}\"", val),
    TooFewGradientStops(val) => format!("Failed to parse gradient due to too few gradient steps: \"{}\"", val),
    DirectionParseError(e) => format!("Failed to parse gradient direction: \"{}\"", e),
    GradientParseError(e) => format!("Failed to parse gradient: {}", e),
    ConicGradient(e) => format!("Failed to parse conic gradient: {}", e),
    ShapeParseError(e) => format!("Failed to parse shape of radial gradient: {}", e),
    ImageParseError(e) => format!("Failed to parse image() value: {}", e),
    ColorParseError(e) => format!("Failed to parse color value: {}", e),
}}

impl_from!(
    ParenthesisParseError<'a>,
    CssBackgroundParseError::InvalidBackground
);
impl_from!(
    CssDirectionParseError<'a>,
    CssBackgroundParseError::DirectionParseError
);
impl_from!(
    CssGradientStopParseError<'a>,
    CssBackgroundParseError::GradientParseError
);
impl_from!(
    CssShapeParseError<'a>,
    CssBackgroundParseError::ShapeParseError
);
impl_from!(
    CssImageParseError<'a>,
    CssBackgroundParseError::ImageParseError
);
impl_from!(
    CssColorParseError<'a>,
    CssBackgroundParseError::ColorParseError
);
impl_from!(
    CssConicGradientParseError<'a>,
    CssBackgroundParseError::ConicGradient
);

/// Owned version of CssBackgroundParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundParseErrorOwned {
    Error(String),
    InvalidBackground(ParenthesisParseErrorOwned),
    UnclosedGradient(String),
    NoDirection(String),
    TooFewGradientStops(String),
    DirectionParseError(CssDirectionParseErrorOwned),
    GradientParseError(CssGradientStopParseErrorOwned),
    ConicGradient(CssConicGradientParseErrorOwned),
    ShapeParseError(CssShapeParseErrorOwned),
    ImageParseError(CssImageParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssBackgroundParseError<'a> {
    pub fn to_contained(&self) -> CssBackgroundParseErrorOwned {
        match self {
            CssBackgroundParseError::Error(s) => CssBackgroundParseErrorOwned::Error(s.to_string()),
            CssBackgroundParseError::InvalidBackground(e) => {
                CssBackgroundParseErrorOwned::InvalidBackground(e.to_contained())
            }
            CssBackgroundParseError::UnclosedGradient(s) => {
                CssBackgroundParseErrorOwned::UnclosedGradient(s.to_string())
            }
            CssBackgroundParseError::NoDirection(s) => {
                CssBackgroundParseErrorOwned::NoDirection(s.to_string())
            }
            CssBackgroundParseError::TooFewGradientStops(s) => {
                CssBackgroundParseErrorOwned::TooFewGradientStops(s.to_string())
            }
            CssBackgroundParseError::DirectionParseError(e) => {
                CssBackgroundParseErrorOwned::DirectionParseError(e.to_contained())
            }
            CssBackgroundParseError::GradientParseError(e) => {
                CssBackgroundParseErrorOwned::GradientParseError(e.to_contained())
            }
            CssBackgroundParseError::ConicGradient(e) => {
                CssBackgroundParseErrorOwned::ConicGradient(e.to_contained())
            }
            CssBackgroundParseError::ShapeParseError(e) => {
                CssBackgroundParseErrorOwned::ShapeParseError(e.to_contained())
            }
            CssBackgroundParseError::ImageParseError(e) => {
                CssBackgroundParseErrorOwned::ImageParseError(e.to_contained())
            }
            CssBackgroundParseError::ColorParseError(e) => {
                CssBackgroundParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssBackgroundParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBackgroundParseError<'a> {
        match self {
            CssBackgroundParseErrorOwned::Error(s) => CssBackgroundParseError::Error(s.as_str()),
            CssBackgroundParseErrorOwned::InvalidBackground(e) => {
                CssBackgroundParseError::InvalidBackground(e.to_shared())
            }
            CssBackgroundParseErrorOwned::UnclosedGradient(s) => {
                CssBackgroundParseError::UnclosedGradient(s.as_str())
            }
            CssBackgroundParseErrorOwned::NoDirection(s) => {
                CssBackgroundParseError::NoDirection(s.as_str())
            }
            CssBackgroundParseErrorOwned::TooFewGradientStops(s) => {
                CssBackgroundParseError::TooFewGradientStops(s.as_str())
            }
            CssBackgroundParseErrorOwned::DirectionParseError(e) => {
                CssBackgroundParseError::DirectionParseError(e.to_shared())
            }
            CssBackgroundParseErrorOwned::GradientParseError(e) => {
                CssBackgroundParseError::GradientParseError(e.to_shared())
            }
            CssBackgroundParseErrorOwned::ConicGradient(e) => {
                CssBackgroundParseError::ConicGradient(e.to_shared())
            }
            CssBackgroundParseErrorOwned::ShapeParseError(e) => {
                CssBackgroundParseError::ShapeParseError(e.to_shared())
            }
            CssBackgroundParseErrorOwned::ImageParseError(e) => {
                CssBackgroundParseError::ImageParseError(e.to_shared())
            }
            CssBackgroundParseErrorOwned::ColorParseError(e) => {
                CssBackgroundParseError::ColorParseError(e.to_shared())
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GradientType {
    LinearGradient,
    RepeatingLinearGradient,
    RadialGradient,
    RepeatingRadialGradient,
    ConicGradient,
    RepeatingConicGradient,
}

impl GradientType {
    pub const fn get_extend_mode(&self) -> ExtendMode {
        match self {
            GradientType::LinearGradient => ExtendMode::Clamp,
            GradientType::RadialGradient => ExtendMode::Clamp,
            GradientType::ConicGradient => ExtendMode::Clamp,
            GradientType::RepeatingRadialGradient => ExtendMode::Repeat,
            GradientType::RepeatingLinearGradient => ExtendMode::Repeat,
            GradientType::RepeatingConicGradient => ExtendMode::Repeat,
        }
    }
}

// parses multiple backgrounds, such as "linear-gradient(red, green), radial-gradient(blue, yellow)"
pub fn parse_style_background_content_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundContentVec, CssBackgroundParseError<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_content(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

// parses multiple background-positions
pub fn parse_style_background_position_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundPositionVec, CssBackgroundPositionParseError<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_position(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

// parses multiple background-size
pub fn parse_style_background_size_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundSizeVec, InvalidValueErr<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_size(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

// parses multiple background-repeat
pub fn parse_style_background_repeat_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundRepeatVec, InvalidValueErr<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_repeat(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

// parses a background, such as "linear-gradient(red, green)"
pub fn parse_style_background_content<'a>(
    input: &'a str,
) -> Result<StyleBackgroundContent, CssBackgroundParseError<'a>> {
    match parse_parentheses(
        input,
        &[
            "linear-gradient",
            "repeating-linear-gradient",
            "radial-gradient",
            "repeating-radial-gradient",
            "conic-gradient",
            "repeating-conic-gradient",
            "image",
        ],
    ) {
        Ok((background_type, brace_contents)) => {
            let gradient_type = match background_type {
                "linear-gradient" => GradientType::LinearGradient,
                "repeating-linear-gradient" => GradientType::RepeatingLinearGradient,
                "radial-gradient" => GradientType::RadialGradient,
                "repeating-radial-gradient" => GradientType::RepeatingRadialGradient,
                "conic-gradient" => GradientType::ConicGradient,
                "repeating-conic-gradient" => GradientType::RepeatingConicGradient,
                "image" => {
                    return Ok(StyleBackgroundContent::Image(parse_image(brace_contents)?));
                }
                other => {
                    return Err(CssBackgroundParseError::Error(other)); /* unreachable */
                }
            };

            parse_gradient(brace_contents, gradient_type)
        }
        Err(_) => Ok(StyleBackgroundContent::Color(parse_css_color(input)?)),
    }
}

#[derive(Clone, PartialEq)]
pub enum CssConicGradientParseError<'a> {
    Position(CssBackgroundPositionParseError<'a>),
    Angle(CssAngleValueParseError<'a>),
    NoAngle(&'a str),
}

impl_debug_as_display!(CssConicGradientParseError<'a>);
impl_display! { CssConicGradientParseError<'a>, {
    Position(val) => format!("Invalid position attribute: \"{}\"", val),
    Angle(val) => format!("Invalid angle value: \"{}\"", val),
    NoAngle(val) => format!("Expected angle: \"{}\"", val),
}}

impl_from!(
    CssAngleValueParseError<'a>,
    CssConicGradientParseError::Angle
);
impl_from!(
    CssBackgroundPositionParseError<'a>,
    CssConicGradientParseError::Position
);

/// Owned version of CssConicGradientParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssConicGradientParseErrorOwned {
    Position(CssBackgroundPositionParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    NoAngle(String),
}

impl<'a> CssConicGradientParseError<'a> {
    pub fn to_contained(&self) -> CssConicGradientParseErrorOwned {
        match self {
            CssConicGradientParseError::Position(e) => {
                CssConicGradientParseErrorOwned::Position(e.to_contained())
            }
            CssConicGradientParseError::Angle(e) => {
                CssConicGradientParseErrorOwned::Angle(e.to_contained())
            }
            CssConicGradientParseError::NoAngle(s) => {
                CssConicGradientParseErrorOwned::NoAngle(s.to_string())
            }
        }
    }
}

impl CssConicGradientParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssConicGradientParseError<'a> {
        match self {
            CssConicGradientParseErrorOwned::Position(e) => {
                CssConicGradientParseError::Position(e.to_shared())
            }
            CssConicGradientParseErrorOwned::Angle(e) => {
                CssConicGradientParseError::Angle(e.to_shared())
            }
            CssConicGradientParseErrorOwned::NoAngle(s) => {
                CssConicGradientParseError::NoAngle(s.as_str())
            }
        }
    }
}

// parse a conic gradient first item such as "from 0.25turn at 50% 30%"
pub fn parse_conic_first_item<'a>(
    input: &'a str,
) -> Result<Option<(AngleValue, StyleBackgroundPosition)>, CssConicGradientParseError<'a>> {
    let input = input.trim();
    if !input.starts_with("from") {
        return Ok(None);
    }
    let input = &input["from".len()..];
    let mut iter = input.split_whitespace();

    let angle = parse_angle_value(
        iter.next()
            .ok_or(CssConicGradientParseError::NoAngle(input))?,
    )?;

    if !(iter.next() == Some("at")) {
        return Ok(Some((angle, StyleBackgroundPosition::default())));
    }

    let remaining = iter.next().unwrap_or("");
    let position = parse_style_background_position(&remaining)?;

    Ok(Some((angle, position)))
}

#[derive(Clone, PartialEq)]
pub enum CssScrollbarStyleParseError<'a> {
    Invalid(&'a str),
}

impl_debug_as_display!(CssScrollbarStyleParseError<'a>);
impl_display! { CssScrollbarStyleParseError<'a>, {
    Invalid(e) => format!("Invalid scrollbar style: \"{}\"", e),
}}

/// Owned version of CssScrollbarStyleParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssScrollbarStyleParseErrorOwned {
    Invalid(String),
}

impl<'a> CssScrollbarStyleParseError<'a> {
    pub fn to_contained(&self) -> CssScrollbarStyleParseErrorOwned {
        match self {
            CssScrollbarStyleParseError::Invalid(s) => {
                CssScrollbarStyleParseErrorOwned::Invalid(s.to_string())
            }
        }
    }
}

impl CssScrollbarStyleParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssScrollbarStyleParseError<'a> {
        match self {
            CssScrollbarStyleParseErrorOwned::Invalid(s) => {
                CssScrollbarStyleParseError::Invalid(s.as_str())
            }
        }
    }
}

pub fn parse_scrollbar_style<'a>(
    input: &'a str,
) -> Result<ScrollbarStyle, CssScrollbarStyleParseError<'a>> {
    Ok(ScrollbarStyle::default()) // TODO!
}

#[derive(Clone, PartialEq)]
pub enum CssStyleFilterParseError<'a> {
    InvalidFilter(&'a str),
    InvalidParenthesis(ParenthesisParseError<'a>),
    Shadow(CssShadowParseError<'a>),
    BlendMode(InvalidValueErr<'a>),
    Color(CssColorParseError<'a>),
    Opacity(PercentageParseError),
    BlurError(CssStyleBlurParseError<'a>),
    ColorMatrixError(CssStyleColorMatrixParseError<'a>),
    FilterOffsetError(CssStyleFilterOffsetParseError<'a>),
    CompositeFilterError(CssStyleCompositeFilterParseError<'a>),
}

impl_debug_as_display!(CssStyleFilterParseError<'a>);
impl_display! { CssStyleFilterParseError<'a>, {
    InvalidFilter(e) => format!("Invalid filter property: \"{}\"", e),
    InvalidParenthesis(e) => format!("Invalid filter property - parenthesis error: {}", e),
    Shadow(e) => format!("Error parsing drop-shadow() contents: {}", e),
    BlendMode(e) => format!("Error parsing blend() contents: invalid value \"{}\"", e.0),
    Color(e) => format!("Error parsing flood() contents: {}", e),
    Opacity(e) => format!("Error parsing opacity() contents: {}", e),
    BlurError(e) => format!("Error parsing blur() contents: {}", e),
    ColorMatrixError(e) => format!("Error parsing color-matrix() contents: {}", e),
    FilterOffsetError(e) => format!("Error parsing offset() contents: {}", e),
    CompositeFilterError(e) => format!("Error parsing composite() contents: {}", e),
}}

impl_from!(
    ParenthesisParseError<'a>,
    CssStyleFilterParseError::InvalidParenthesis
);
impl_from!(InvalidValueErr<'a>, CssStyleFilterParseError::BlendMode);
impl_from!(
    CssStyleBlurParseError<'a>,
    CssStyleFilterParseError::BlurError
);
impl_from!(CssColorParseError<'a>, CssStyleFilterParseError::Color);
impl_from!(
    CssStyleColorMatrixParseError<'a>,
    CssStyleFilterParseError::ColorMatrixError
);
impl_from!(
    CssStyleFilterOffsetParseError<'a>,
    CssStyleFilterParseError::FilterOffsetError
);
impl_from!(
    CssStyleCompositeFilterParseError<'a>,
    CssStyleFilterParseError::CompositeFilterError
);
impl_from!(CssShadowParseError<'a>, CssStyleFilterParseError::Shadow);

impl<'a> From<PercentageParseError> for CssStyleFilterParseError<'a> {
    fn from(p: PercentageParseError) -> CssStyleFilterParseError<'a> {
        CssStyleFilterParseError::Opacity(p)
    }
}

/// Owned version of CssStyleFilterParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFilterParseErrorOwned {
    InvalidFilter(String),
    InvalidParenthesis(ParenthesisParseErrorOwned),
    Shadow(CssShadowParseErrorOwned),
    BlendMode(InvalidValueErrOwned),
    Color(CssColorParseErrorOwned),
    Opacity(PercentageParseError),
    BlurError(CssStyleBlurParseErrorOwned),
    ColorMatrixError(CssStyleColorMatrixParseErrorOwned),
    FilterOffsetError(CssStyleFilterOffsetParseErrorOwned),
    CompositeFilterError(CssStyleCompositeFilterParseErrorOwned),
}

impl<'a> CssStyleFilterParseError<'a> {
    pub fn to_contained(&self) -> CssStyleFilterParseErrorOwned {
        match self {
            CssStyleFilterParseError::InvalidFilter(s) => {
                CssStyleFilterParseErrorOwned::InvalidFilter(s.to_string())
            }
            CssStyleFilterParseError::InvalidParenthesis(e) => {
                CssStyleFilterParseErrorOwned::InvalidParenthesis(e.to_contained())
            }
            CssStyleFilterParseError::Shadow(e) => {
                CssStyleFilterParseErrorOwned::Shadow(e.to_contained())
            }
            CssStyleFilterParseError::BlendMode(e) => {
                CssStyleFilterParseErrorOwned::BlendMode(e.to_contained())
            }
            CssStyleFilterParseError::Color(e) => {
                CssStyleFilterParseErrorOwned::Color(e.to_contained())
            }
            CssStyleFilterParseError::Opacity(e) => {
                CssStyleFilterParseErrorOwned::Opacity(e.clone())
            }
            CssStyleFilterParseError::BlurError(e) => {
                CssStyleFilterParseErrorOwned::BlurError(e.to_contained())
            }
            CssStyleFilterParseError::ColorMatrixError(e) => {
                CssStyleFilterParseErrorOwned::ColorMatrixError(e.to_contained())
            }
            CssStyleFilterParseError::FilterOffsetError(e) => {
                CssStyleFilterParseErrorOwned::FilterOffsetError(e.to_contained())
            }
            CssStyleFilterParseError::CompositeFilterError(e) => {
                CssStyleFilterParseErrorOwned::CompositeFilterError(e.to_contained())
            }
        }
    }
}

impl CssStyleFilterParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleFilterParseError<'a> {
        match self {
            CssStyleFilterParseErrorOwned::InvalidFilter(s) => {
                CssStyleFilterParseError::InvalidFilter(s.as_str())
            }
            CssStyleFilterParseErrorOwned::InvalidParenthesis(e) => {
                CssStyleFilterParseError::InvalidParenthesis(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::Shadow(e) => {
                CssStyleFilterParseError::Shadow(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::BlendMode(e) => {
                CssStyleFilterParseError::BlendMode(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::Color(e) => {
                CssStyleFilterParseError::Color(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::Opacity(e) => {
                CssStyleFilterParseError::Opacity(e.clone())
            }
            CssStyleFilterParseErrorOwned::BlurError(e) => {
                CssStyleFilterParseError::BlurError(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::ColorMatrixError(e) => {
                CssStyleFilterParseError::ColorMatrixError(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::FilterOffsetError(e) => {
                CssStyleFilterParseError::FilterOffsetError(e.to_shared())
            }
            CssStyleFilterParseErrorOwned::CompositeFilterError(e) => {
                CssStyleFilterParseError::CompositeFilterError(e.to_shared())
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleBlurParseError<'a> {
    Invalid(&'a str),
    Pixel(CssPixelValueParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
}

impl_debug_as_display!(CssStyleBlurParseError<'a>);
impl_display! { CssStyleBlurParseError<'a>, {
    Invalid(s) => format!("Invalid: {}", s),
    Pixel(e) => format!("Error parsing pixel value: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
}}
impl_from!(CssPixelValueParseError<'a>, CssStyleBlurParseError::Pixel);

/// Owned version of CssStyleBlurParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleBlurParseErrorOwned {
    Invalid(String),
    Pixel(CssPixelValueParseErrorOwned),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
}

impl<'a> CssStyleBlurParseError<'a> {
    pub fn to_contained(&self) -> CssStyleBlurParseErrorOwned {
        match self {
            CssStyleBlurParseError::Invalid(s) => {
                CssStyleBlurParseErrorOwned::Invalid(s.to_string())
            }
            CssStyleBlurParseError::Pixel(e) => {
                CssStyleBlurParseErrorOwned::Pixel(e.to_contained())
            }
            CssStyleBlurParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleBlurParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
        }
    }
}

impl CssStyleBlurParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleBlurParseError<'a> {
        match self {
            CssStyleBlurParseErrorOwned::Invalid(s) => CssStyleBlurParseError::Invalid(s.as_str()),
            CssStyleBlurParseErrorOwned::Pixel(e) => CssStyleBlurParseError::Pixel(e.to_shared()),
            CssStyleBlurParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleBlurParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleColorMatrixParseError<'a> {
    Invalid(&'a str),
    Float(ParseFloatError),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
}

impl_debug_as_display!(CssStyleColorMatrixParseError<'a>);
impl_display! { CssStyleColorMatrixParseError<'a>, {
    Invalid(s) => format!("Invalid: {}", s),
    Float(e) => format!("Error parsing floating-point value: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
}}

impl<'a> From<ParseFloatError> for CssStyleColorMatrixParseError<'a> {
    fn from(p: ParseFloatError) -> CssStyleColorMatrixParseError<'a> {
        CssStyleColorMatrixParseError::Float(p)
    }
}

/// Owned version of CssStyleColorMatrixParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleColorMatrixParseErrorOwned {
    Invalid(String),
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
            CssStyleColorMatrixParseError::Invalid(s) => {
                CssStyleColorMatrixParseErrorOwned::Invalid(s.to_string())
            }
            CssStyleColorMatrixParseError::Float(e) => {
                CssStyleColorMatrixParseErrorOwned::Float(e.clone())
            }
            CssStyleColorMatrixParseError::WrongNumberOfComponents {
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
            CssStyleColorMatrixParseErrorOwned::Invalid(s) => {
                CssStyleColorMatrixParseError::Invalid(s.as_str())
            }
            CssStyleColorMatrixParseErrorOwned::Float(e) => {
                CssStyleColorMatrixParseError::Float(e.clone())
            }
            CssStyleColorMatrixParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleColorMatrixParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleFilterOffsetParseError<'a> {
    Invalid(&'a str),
    Pixel(CssPixelValueParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
}

impl_debug_as_display!(CssStyleFilterOffsetParseError<'a>);
impl_display! { CssStyleFilterOffsetParseError<'a>, {
    Invalid(s) => format!("Invalid: {}", s),
    Pixel(e) => format!("Error parsing pixel value: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleFilterOffsetParseError::Pixel
);

/// Owned version of CssStyleFilterOffsetParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFilterOffsetParseErrorOwned {
    Invalid(String),
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
            CssStyleFilterOffsetParseError::Invalid(s) => {
                CssStyleFilterOffsetParseErrorOwned::Invalid(s.to_string())
            }
            CssStyleFilterOffsetParseError::Pixel(e) => {
                CssStyleFilterOffsetParseErrorOwned::Pixel(e.to_contained())
            }
            CssStyleFilterOffsetParseError::WrongNumberOfComponents {
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
            CssStyleFilterOffsetParseErrorOwned::Invalid(s) => {
                CssStyleFilterOffsetParseError::Invalid(s.as_str())
            }
            CssStyleFilterOffsetParseErrorOwned::Pixel(e) => {
                CssStyleFilterOffsetParseError::Pixel(e.to_shared())
            }
            CssStyleFilterOffsetParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleFilterOffsetParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssStyleCompositeFilterParseError<'a> {
    Invalid(&'a str),
    Float(ParseFloatError),
    InvalidParenthesis(ParenthesisParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
}

impl_debug_as_display!(CssStyleCompositeFilterParseError<'a>);
impl_display! { CssStyleCompositeFilterParseError<'a>, {
    Invalid(s) => format!("Invalid: {}", s),
    Float(e) => format!("Error parsing floating-point value: {}", e),
    InvalidParenthesis(e) => format!("Invalid arithmetic() property - parenthesis error: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Expected {} components, got {}: \"{}\"", expected, got, input),
}}
impl_from!(
    ParenthesisParseError<'a>,
    CssStyleCompositeFilterParseError::InvalidParenthesis
);

impl<'a> From<ParseFloatError> for CssStyleCompositeFilterParseError<'a> {
    fn from(p: ParseFloatError) -> CssStyleCompositeFilterParseError<'a> {
        CssStyleCompositeFilterParseError::Float(p)
    }
}

/// Owned version of CssStyleCompositeFilterParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleCompositeFilterParseErrorOwned {
    Invalid(String),
    Float(ParseFloatError),
    InvalidParenthesis(ParenthesisParseErrorOwned),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
}

impl<'a> CssStyleCompositeFilterParseError<'a> {
    pub fn to_contained(&self) -> CssStyleCompositeFilterParseErrorOwned {
        match self {
            CssStyleCompositeFilterParseError::Invalid(s) => {
                CssStyleCompositeFilterParseErrorOwned::Invalid(s.to_string())
            }
            CssStyleCompositeFilterParseError::Float(e) => {
                CssStyleCompositeFilterParseErrorOwned::Float(e.clone())
            }
            CssStyleCompositeFilterParseError::InvalidParenthesis(e) => {
                CssStyleCompositeFilterParseErrorOwned::InvalidParenthesis(e.to_contained())
            }
            CssStyleCompositeFilterParseError::WrongNumberOfComponents {
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
            CssStyleCompositeFilterParseErrorOwned::Invalid(s) => {
                CssStyleCompositeFilterParseError::Invalid(s.as_str())
            }
            CssStyleCompositeFilterParseErrorOwned::Float(e) => {
                CssStyleCompositeFilterParseError::Float(e.clone())
            }
            CssStyleCompositeFilterParseErrorOwned::InvalidParenthesis(e) => {
                CssStyleCompositeFilterParseError::InvalidParenthesis(e.to_shared())
            }
            CssStyleCompositeFilterParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleCompositeFilterParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
        }
    }
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

#[derive(Clone, PartialEq)]
pub enum CssStyleTransformParseError<'a> {
    InvalidTransform(&'a str),
    InvalidParenthesis(ParenthesisParseError<'a>),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    PixelValueParseError(CssPixelValueParseError<'a>),
    AngleValueParseError(CssAngleValueParseError<'a>),
    PercentageValueParseError(PercentageParseError),
}

impl_debug_as_display!(CssStyleTransformParseError<'a>);
impl_display! { CssStyleTransformParseError<'a>, {
    InvalidTransform(e) => format!("Invalid transform property: \"{}\"", e),
    InvalidParenthesis(e) => format!("Invalid transform property - parenthesis error: {}", e),
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components on transform property: expected {} components, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid pixel value: {}", e),
    AngleValueParseError(e) => format!("Invalid angle value: {}", e),
    PercentageValueParseError(e) => format!("Invalid transform property - error parsing percentage: {}", e),
}}

impl_from!(
    ParenthesisParseError<'a>,
    CssStyleTransformParseError::InvalidParenthesis
);
impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleTransformParseError::PixelValueParseError
);
impl_from!(
    CssAngleValueParseError<'a>,
    CssStyleTransformParseError::AngleValueParseError
);

impl<'a> From<PercentageParseError> for CssStyleTransformParseError<'a> {
    fn from(p: PercentageParseError) -> CssStyleTransformParseError<'a> {
        CssStyleTransformParseError::PercentageValueParseError(p)
    }
}

/// Owned version of CssStyleTransformParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleTransformParseErrorOwned {
    InvalidTransform(String),
    InvalidParenthesis(ParenthesisParseErrorOwned),
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
    PixelValueParseError(CssPixelValueParseErrorOwned),
    AngleValueParseError(CssAngleValueParseErrorOwned),
    PercentageValueParseError(PercentageParseError),
}

impl<'a> CssStyleTransformParseError<'a> {
    pub fn to_contained(&self) -> CssStyleTransformParseErrorOwned {
        match self {
            CssStyleTransformParseError::InvalidTransform(s) => {
                CssStyleTransformParseErrorOwned::InvalidTransform(s.to_string())
            }
            CssStyleTransformParseError::InvalidParenthesis(e) => {
                CssStyleTransformParseErrorOwned::InvalidParenthesis(e.to_contained())
            }
            CssStyleTransformParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
            CssStyleTransformParseError::PixelValueParseError(e) => {
                CssStyleTransformParseErrorOwned::PixelValueParseError(e.to_contained())
            }
            CssStyleTransformParseError::AngleValueParseError(e) => {
                CssStyleTransformParseErrorOwned::AngleValueParseError(e.to_contained())
            }
            CssStyleTransformParseError::PercentageValueParseError(e) => {
                CssStyleTransformParseErrorOwned::PercentageValueParseError(e.clone())
            }
        }
    }
}

impl CssStyleTransformParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleTransformParseError<'a> {
        match self {
            CssStyleTransformParseErrorOwned::InvalidTransform(s) => {
                CssStyleTransformParseError::InvalidTransform(s.as_str())
            }
            CssStyleTransformParseErrorOwned::InvalidParenthesis(e) => {
                CssStyleTransformParseError::InvalidParenthesis(e.to_shared())
            }
            CssStyleTransformParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
            CssStyleTransformParseErrorOwned::PixelValueParseError(e) => {
                CssStyleTransformParseError::PixelValueParseError(e.to_shared())
            }
            CssStyleTransformParseErrorOwned::AngleValueParseError(e) => {
                CssStyleTransformParseError::AngleValueParseError(e.to_shared())
            }
            CssStyleTransformParseErrorOwned::PercentageValueParseError(e) => {
                CssStyleTransformParseError::PercentageValueParseError(e.clone())
            }
        }
    }
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

#[derive(Clone, PartialEq)]
pub enum CssStyleTransformOriginParseError<'a> {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    PixelValueParseError(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssStyleTransformOriginParseError<'a>);
impl_display! { CssStyleTransformOriginParseError<'a>, {
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components on transform property: expected {} components, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid transform property: {}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleTransformOriginParseError::PixelValueParseError
);

/// Owned version of CssStyleTransformOriginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleTransformOriginParseErrorOwned {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStyleTransformOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStyleTransformOriginParseErrorOwned {
        match self {
            CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
            CssStyleTransformOriginParseError::PixelValueParseError(e) => {
                CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleTransformOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleTransformOriginParseError<'a> {
        match self {
            CssStyleTransformOriginParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStyleTransformOriginParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
            CssStyleTransformOriginParseErrorOwned::PixelValueParseError(e) => {
                CssStyleTransformOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

pub fn parse_style_transform_origin<'a>(
    input: &'a str,
) -> Result<StyleTransformOrigin, CssStyleTransformOriginParseError<'a>> {
    let input = input.trim();
    let mut iter = input.split(",");

    let x = parse_pixel_value(iter.next().ok_or(
        CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 0,
            input,
        },
    )?)?;
    let y = parse_pixel_value(iter.next().ok_or(
        CssStyleTransformOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 1,
            input,
        },
    )?)?;

    Ok(StyleTransformOrigin { x, y })
}

#[derive(Clone, PartialEq)]
pub enum CssStylePerspectiveOriginParseError<'a> {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: &'a str,
    },
    PixelValueParseError(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssStylePerspectiveOriginParseError<'a>);
impl_display! { CssStylePerspectiveOriginParseError<'a>, {
    WrongNumberOfComponents { expected, got, input } => format!("Invalid number of components on transform property: expected {} components, got {}: \"{}\"", expected, got, input),
    PixelValueParseError(e) => format!("Invalid transform property: {}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssStylePerspectiveOriginParseError::PixelValueParseError
);

/// Owned version of CssStylePerspectiveOriginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStylePerspectiveOriginParseErrorOwned {
    WrongNumberOfComponents {
        expected: usize,
        got: usize,
        input: String,
    },
    PixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStylePerspectiveOriginParseError<'a> {
    pub fn to_contained(&self) -> CssStylePerspectiveOriginParseErrorOwned {
        match self {
            CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.to_string(),
            },
            CssStylePerspectiveOriginParseError::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStylePerspectiveOriginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStylePerspectiveOriginParseError<'a> {
        match self {
            CssStylePerspectiveOriginParseErrorOwned::WrongNumberOfComponents {
                expected,
                got,
                input,
            } => CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
                expected: *expected,
                got: *got,
                input: input.as_str(),
            },
            CssStylePerspectiveOriginParseErrorOwned::PixelValueParseError(e) => {
                CssStylePerspectiveOriginParseError::PixelValueParseError(e.to_shared())
            }
        }
    }
}

pub fn parse_style_perspective_origin<'a>(
    input: &'a str,
) -> Result<StylePerspectiveOrigin, CssStylePerspectiveOriginParseError<'a>> {
    let input = input.trim();
    let mut iter = input.split(",");

    let x = parse_pixel_value(iter.next().ok_or(
        CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 0,
            input,
        },
    )?)?;
    let y = parse_pixel_value(iter.next().ok_or(
        CssStylePerspectiveOriginParseError::WrongNumberOfComponents {
            expected: 6,
            got: 1,
            input,
        },
    )?)?;

    Ok(StylePerspectiveOrigin { x, y })
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundPositionParseError<'a> {
    NoPosition(&'a str),
    TooManyComponents(&'a str),
    FirstComponentWrong(CssPixelValueParseError<'a>),
    SecondComponentWrong(CssPixelValueParseError<'a>),
}

impl_display! {CssBackgroundPositionParseError<'a>, {
    NoPosition(e) => format!("First background position missing: \"{}\"", e),
    TooManyComponents(e) => format!("background-position can only have one or two components, not more: \"{}\"", e),
    FirstComponentWrong(e) => format!("Failed to parse first component: \"{}\"", e),
    SecondComponentWrong(e) => format!("Failed to parse second component: \"{}\"", e),
}}

/// Owned version of CssBackgroundPositionParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundPositionParseErrorOwned {
    NoPosition(String),
    TooManyComponents(String),
    FirstComponentWrong(CssPixelValueParseErrorOwned),
    SecondComponentWrong(CssPixelValueParseErrorOwned),
}

impl<'a> CssBackgroundPositionParseError<'a> {
    pub fn to_contained(&self) -> CssBackgroundPositionParseErrorOwned {
        match self {
            CssBackgroundPositionParseError::NoPosition(s) => {
                CssBackgroundPositionParseErrorOwned::NoPosition(s.to_string())
            }
            CssBackgroundPositionParseError::TooManyComponents(s) => {
                CssBackgroundPositionParseErrorOwned::TooManyComponents(s.to_string())
            }
            CssBackgroundPositionParseError::FirstComponentWrong(e) => {
                CssBackgroundPositionParseErrorOwned::FirstComponentWrong(e.to_contained())
            }
            CssBackgroundPositionParseError::SecondComponentWrong(e) => {
                CssBackgroundPositionParseErrorOwned::SecondComponentWrong(e.to_contained())
            }
        }
    }
}

impl CssBackgroundPositionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBackgroundPositionParseError<'a> {
        match self {
            CssBackgroundPositionParseErrorOwned::NoPosition(s) => {
                CssBackgroundPositionParseError::NoPosition(s.as_str())
            }
            CssBackgroundPositionParseErrorOwned::TooManyComponents(s) => {
                CssBackgroundPositionParseError::TooManyComponents(s.as_str())
            }
            CssBackgroundPositionParseErrorOwned::FirstComponentWrong(e) => {
                CssBackgroundPositionParseError::FirstComponentWrong(e.to_shared())
            }
            CssBackgroundPositionParseErrorOwned::SecondComponentWrong(e) => {
                CssBackgroundPositionParseError::SecondComponentWrong(e.to_shared())
            }
        }
    }
}

pub fn parse_background_position_horizontal<'a>(
    input: &'a str,
) -> Result<BackgroundPositionHorizontal, CssPixelValueParseError<'a>> {
    Ok(match input {
        "left" => BackgroundPositionHorizontal::Left,
        "center" => BackgroundPositionHorizontal::Center,
        "right" => BackgroundPositionHorizontal::Right,
        other => BackgroundPositionHorizontal::Exact(parse_pixel_value(other)?),
    })
}

pub fn parse_background_position_vertical<'a>(
    input: &'a str,
) -> Result<BackgroundPositionVertical, CssPixelValueParseError<'a>> {
    Ok(match input {
        "top" => BackgroundPositionVertical::Top,
        "center" => BackgroundPositionVertical::Center,
        "bottom" => BackgroundPositionVertical::Bottom,
        other => BackgroundPositionVertical::Exact(parse_pixel_value(other)?),
    })
}

pub fn parse_style_background_position<'a>(
    input: &'a str,
) -> Result<StyleBackgroundPosition, CssBackgroundPositionParseError<'a>> {
    use self::CssBackgroundPositionParseError::*;

    let input = input.trim();
    let mut whitespace_iter = input.split_whitespace();

    let first = whitespace_iter.next().ok_or(NoPosition(input))?;
    let second = whitespace_iter.next();

    if whitespace_iter.next().is_some() {
        return Err(TooManyComponents(input));
    }

    let horizontal =
        parse_background_position_horizontal(first).map_err(|e| FirstComponentWrong(e))?;

    let vertical = match second {
        Some(second) => {
            parse_background_position_vertical(second).map_err(|e| SecondComponentWrong(e))?
        }
        None => BackgroundPositionVertical::Center,
    };

    Ok(StyleBackgroundPosition {
        horizontal,
        vertical,
    })
}

fn split_string_respect_comma<'a>(input: &'a str) -> Vec<&'a str> {
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
                '(' => {
                    depth += 1;
                }
                ')' => {
                    depth -= 1;
                }
                c => {
                    if c == target_char && depth == 0 {
                        character_was_found = true;
                        break;
                    }
                }
            }
        }

        if last_character == 0 {
            // No more split by `,`
            None
        } else {
            Some((last_character, character_was_found))
        }
    }

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

    comma_separated_items
}

// parses a single gradient such as "to right, 50px"
pub fn parse_gradient<'a>(
    input: &'a str,
    background_type: GradientType,
) -> Result<StyleBackgroundContent, CssBackgroundParseError<'a>> {
    let input = input.trim();

    // Splitting the input by "," doesn't work since rgba() might contain commas
    let comma_separated_items = split_string_respect_comma(input);

    let mut brace_iterator = comma_separated_items.iter();

    // "50deg", "to right bottom", etc.
    let first_brace_item = match brace_iterator.next() {
        Some(s) => s,
        None => return Err(CssBackgroundParseError::NoDirection(input)),
    };

    let is_linear_gradient = background_type == GradientType::LinearGradient
        || background_type == GradientType::RepeatingLinearGradient;

    let is_radial_gradient = background_type == GradientType::RadialGradient
        || background_type == GradientType::RepeatingRadialGradient;

    let is_conic_gradient = background_type == GradientType::ConicGradient
        || background_type == GradientType::RepeatingConicGradient;

    if is_linear_gradient {
        let mut linear_gradient = LinearGradient::default();
        let mut linear_gradient_stops = Vec::new();
        if let Ok(dir) = parse_direction(first_brace_item) {
            linear_gradient.direction = dir;
        } else {
            linear_gradient_stops.push(parse_linear_color_stop(first_brace_item)?);
        }
        linear_gradient.extend_mode = background_type.get_extend_mode();
        while let Some(next_brace_item) = brace_iterator.next() {
            linear_gradient_stops.push(parse_linear_color_stop(next_brace_item)?);
        }
        linear_gradient.stops =
            LinearColorStop::get_normalized_linear_stops(&linear_gradient_stops).into();
        Ok(StyleBackgroundContent::LinearGradient(linear_gradient))
    } else if is_radial_gradient {
        let mut radial_gradient = RadialGradient::default();
        let mut radial_gradient_stops = Vec::new();
        if let Ok(sh) = parse_shape(first_brace_item) {
            radial_gradient.shape = sh;
        } else {
            radial_gradient_stops.push(parse_linear_color_stop(first_brace_item)?);
        }
        radial_gradient.extend_mode = background_type.get_extend_mode();
        while let Some(next_brace_item) = brace_iterator.next() {
            radial_gradient_stops.push(parse_linear_color_stop(next_brace_item)?);
        }
        radial_gradient.stops =
            LinearColorStop::get_normalized_linear_stops(&radial_gradient_stops).into();
        Ok(StyleBackgroundContent::RadialGradient(radial_gradient))
    } else
    /* if is_conic_gradient */
    {
        let mut conic_gradient = ConicGradient::default();
        let mut conic_gradient_stops = Vec::new();
        if let Some((angle, center)) = parse_conic_first_item(first_brace_item)? {
            conic_gradient.center = center;
            conic_gradient.angle = angle;
        } else {
            conic_gradient_stops.push(parse_radial_color_stop(first_brace_item)?);
        }
        conic_gradient.extend_mode = background_type.get_extend_mode();
        while let Some(next_brace_item) = brace_iterator.next() {
            conic_gradient_stops.push(parse_radial_color_stop(next_brace_item)?);
        }
        conic_gradient.stops =
            RadialColorStop::get_normalized_radial_stops(&conic_gradient_stops).into();
        Ok(StyleBackgroundContent::ConicGradient(conic_gradient))
    }
}

impl<'a> From<QuoteStripped<'a>> for AzString {
    fn from(input: QuoteStripped<'a>) -> Self {
        use alloc::string::ToString;
        input.0.to_string().into()
    }
}

/// A string that has been stripped of the beginning and ending quote
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct QuoteStripped<'a>(pub &'a str);

pub fn parse_image<'a>(input: &'a str) -> Result<AzString, CssImageParseError<'a>> {
    Ok(strip_quotes(input)?.into())
}

/// Strip quotes from an input, given that both quotes use either `"` or `'`, but not both.
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::parser::{strip_quotes, QuoteStripped, UnclosedQuotesError};
/// assert_eq!(
///     strip_quotes("\"Helvetica\""),
///     Ok(QuoteStripped("Helvetica"))
/// );
/// assert_eq!(strip_quotes("'Arial'"), Ok(QuoteStripped("Arial")));
/// assert_eq!(
///     strip_quotes("\"Arial'"),
///     Err(UnclosedQuotesError("\"Arial'"))
/// );
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
        if !quote_contents.ends_with('\'') {
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
    Angle(CssAngleValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssGradientStopParseError<'a>);
impl_display! { CssGradientStopParseError<'a>, {
    Error(e) => e,
    Percentage(e) => format!("Failed to parse offset percentage: {}", e),
    Angle(e) => format!("Failed to parse angle: {}", e),
    ColorParseError(e) => format!("{}", e),
}}

impl_from!(
    CssColorParseError<'a>,
    CssGradientStopParseError::ColorParseError
);

/// Owned version of CssGradientStopParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssGradientStopParseErrorOwned {
    Error(String),
    Percentage(PercentageParseError),
    Angle(CssAngleValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssGradientStopParseError<'a> {
    pub fn to_contained(&self) -> CssGradientStopParseErrorOwned {
        match self {
            CssGradientStopParseError::Error(s) => {
                CssGradientStopParseErrorOwned::Error(s.to_string())
            }
            CssGradientStopParseError::Percentage(e) => {
                CssGradientStopParseErrorOwned::Percentage(e.clone())
            }
            CssGradientStopParseError::Angle(e) => {
                CssGradientStopParseErrorOwned::Angle(e.to_contained())
            }
            CssGradientStopParseError::ColorParseError(e) => {
                CssGradientStopParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssGradientStopParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssGradientStopParseError<'a> {
        match self {
            CssGradientStopParseErrorOwned::Error(s) => {
                CssGradientStopParseError::Error(s.as_str())
            }
            CssGradientStopParseErrorOwned::Percentage(e) => {
                CssGradientStopParseError::Percentage(e.clone())
            }
            CssGradientStopParseErrorOwned::Angle(e) => {
                CssGradientStopParseError::Angle(e.to_shared())
            }
            CssGradientStopParseErrorOwned::ColorParseError(e) => {
                CssGradientStopParseError::ColorParseError(e.to_shared())
            }
        }
    }
}

// parses "red" , "red 5%"
pub fn parse_linear_color_stop<'a>(
    input: &'a str,
) -> Result<LinearColorStop, CssGradientStopParseError<'a>> {
    use self::CssGradientStopParseError::*;

    let input = input.trim();

    // Color functions such as "rgba(...)" can contain spaces, so we parse right-to-left.
    let (color_str, percentage_str) = match (input.rfind(')'), input.rfind(char::is_whitespace)) {
        (Some(closing_brace), None) if closing_brace < input.len() - 1 => {
            // percentage after closing brace, eg. "rgb(...)50%"
            (
                &input[..=closing_brace],
                Some(&input[(closing_brace + 1)..]),
            )
        }
        (None, Some(last_ws)) => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        }
        (Some(closing_brace), Some(last_ws)) if closing_brace < last_ws => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        }
        _ => {
            // no percentage
            (input, None)
        }
    };

    let color = parse_css_color(color_str)?;
    let offset = match percentage_str {
        None => OptionPercentageValue::None,
        Some(s) => {
            OptionPercentageValue::Some(parse_percentage_value(s).map_err(|e| Percentage(e))?)
        }
    };

    Ok(LinearColorStop { offset, color })
}

// parses "red" , "red 5%"
pub fn parse_radial_color_stop<'a>(
    input: &'a str,
) -> Result<RadialColorStop, CssGradientStopParseError<'a>> {
    use self::CssGradientStopParseError::*;
    use crate::OptionAngleValue;

    let input = input.trim();

    // Color functions such as "rgba(...)" can contain spaces, so we parse right-to-left.
    let (color_str, percentage_str) = match (input.rfind(')'), input.rfind(char::is_whitespace)) {
        (Some(closing_brace), None) if closing_brace < input.len() - 1 => {
            // percentage after closing brace, eg. "rgb(...)50%"
            (
                &input[..=closing_brace],
                Some(&input[(closing_brace + 1)..]),
            )
        }
        (None, Some(last_ws)) => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        }
        (Some(closing_brace), Some(last_ws)) if closing_brace < last_ws => {
            // percentage after last whitespace, eg. "... 50%"
            (&input[..=last_ws], Some(&input[(last_ws + 1)..]))
        }
        _ => {
            // no percentage
            (input, None)
        }
    };

    let color = parse_css_color(color_str)?;
    let offset = match percentage_str {
        None => OptionAngleValue::None,
        Some(s) => OptionAngleValue::Some(parse_angle_value(s).map_err(|e| Angle(e))?),
    };

    Ok(RadialColorStop { offset, color })
}

// parses "5%" -> 5
pub fn parse_percentage(input: &str) -> Result<PercentageValue, PercentageParseError> {
    let percent_location = input
        .rfind('%')
        .ok_or(PercentageParseError::NoPercentSign)?;
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

impl_display! {CssDirectionParseError<'a>, {
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

/// Owned version of CssDirectionParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssDirectionParseErrorOwned {
    Error(String),
    InvalidArguments(String),
    ParseFloat(ParseFloatError),
    CornerError(CssDirectionCornerParseErrorOwned),
}

impl<'a> CssDirectionParseError<'a> {
    pub fn to_contained(&self) -> CssDirectionParseErrorOwned {
        match self {
            CssDirectionParseError::Error(s) => CssDirectionParseErrorOwned::Error(s.to_string()),
            CssDirectionParseError::InvalidArguments(s) => {
                CssDirectionParseErrorOwned::InvalidArguments(s.to_string())
            }
            CssDirectionParseError::ParseFloat(e) => {
                CssDirectionParseErrorOwned::ParseFloat(e.clone())
            }
            CssDirectionParseError::CornerError(e) => {
                CssDirectionParseErrorOwned::CornerError(e.to_contained())
            }
        }
    }
}

impl CssDirectionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssDirectionParseError<'a> {
        match self {
            CssDirectionParseErrorOwned::Error(s) => CssDirectionParseError::Error(s.as_str()),
            CssDirectionParseErrorOwned::InvalidArguments(s) => {
                CssDirectionParseError::InvalidArguments(s.as_str())
            }
            CssDirectionParseErrorOwned::ParseFloat(e) => {
                CssDirectionParseError::ParseFloat(e.clone())
            }
            CssDirectionParseErrorOwned::CornerError(e) => {
                CssDirectionParseError::CornerError(e.to_shared())
            }
        }
    }
}

/// Parses an `direction` such as `"50deg"` or `"to right bottom"` (in the context of gradients)
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::parser::parse_direction;
/// # use azul_css::{Direction, DirectionCorners, AngleValue};
/// use azul_css::DirectionCorner::*;
///
/// assert_eq!(
///     parse_direction("to right bottom"),
///     Ok(Direction::FromTo(DirectionCorners {
///         from: TopLeft,
///         to: BottomRight
///     }))
/// );
/// assert_eq!(
///     parse_direction("to right"),
///     Ok(Direction::FromTo(DirectionCorners {
///         from: Left,
///         to: Right
///     }))
/// );
/// assert_eq!(
///     parse_direction("50deg"),
///     Ok(Direction::Angle(AngleValue::deg(50.0)))
/// );
/// ```
pub fn parse_direction<'a>(input: &'a str) -> Result<Direction, CssDirectionParseError<'a>> {
    let input_iter = input.split_whitespace();
    let count = input_iter.clone().count();
    let mut first_input_iter = input_iter.clone();

    // "50deg" | "to" | "right"
    let first_input = first_input_iter
        .next()
        .ok_or(CssDirectionParseError::Error(input))?;

    if let Some(angle) = parse_angle_value(first_input).ok() {
        Ok(Direction::Angle(angle))
    } else {
        // the input is not an angle

        if first_input != "to" {
            return Err(CssDirectionParseError::InvalidArguments(input));
        }

        let second_input = first_input_iter
            .next()
            .ok_or(CssDirectionParseError::Error(input))?;
        let end = parse_direction_corner(second_input)?;

        return match count {
            2 => {
                // "to right"
                let start = end.opposite();
                Ok(Direction::FromTo(DirectionCorners {
                    from: start,
                    to: end,
                }))
            }
            3 => {
                // "to bottom right"
                let beginning = end;
                let third_input = first_input_iter
                    .next()
                    .ok_or(CssDirectionParseError::Error(input))?;
                let new_end = parse_direction_corner(third_input)?;
                // "Bottom, Right" -> "BottomRight"
                let new_end = beginning
                    .combine(&new_end)
                    .ok_or(CssDirectionParseError::Error(input))?;
                let start = new_end.opposite();
                Ok(Direction::FromTo(DirectionCorners {
                    from: start,
                    to: new_end,
                }))
            }
            _ => Err(CssDirectionParseError::InvalidArguments(input)),
        };
    }
}

#[derive(Clone, PartialEq)]
pub enum CssAngleValueParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str, AngleMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidAngle(&'a str),
}

impl_debug_as_display!(CssAngleValueParseError<'a>);

impl_display! { CssAngleValueParseError<'a>, {
    EmptyString => format!("Missing [rad / deg / turn / %] value"),
    NoValueGiven(input, metric) => format!("Expected floating-point angle value, got: \"{}{}\"", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
    InvalidAngle(s) => format!("Invalid angle value: \"{}\"", s),
}}

/// Owned version of CssAngleValueParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssAngleValueParseErrorOwned {
    EmptyString,
    NoValueGiven(String, AngleMetric),
    ValueParseErr(ParseFloatError, String),
    InvalidAngle(String),
}

impl<'a> CssAngleValueParseError<'a> {
    pub fn to_contained(&self) -> CssAngleValueParseErrorOwned {
        match self {
            CssAngleValueParseError::EmptyString => CssAngleValueParseErrorOwned::EmptyString,
            CssAngleValueParseError::NoValueGiven(s, metric) => {
                CssAngleValueParseErrorOwned::NoValueGiven(s.to_string(), *metric)
            }
            CssAngleValueParseError::ValueParseErr(err, s) => {
                CssAngleValueParseErrorOwned::ValueParseErr(err.clone(), s.to_string())
            }
            CssAngleValueParseError::InvalidAngle(s) => {
                CssAngleValueParseErrorOwned::InvalidAngle(s.to_string())
            }
        }
    }
}

impl CssAngleValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssAngleValueParseError<'a> {
        match self {
            CssAngleValueParseErrorOwned::EmptyString => CssAngleValueParseError::EmptyString,
            CssAngleValueParseErrorOwned::NoValueGiven(s, metric) => {
                CssAngleValueParseError::NoValueGiven(s.as_str(), *metric)
            }
            CssAngleValueParseErrorOwned::ValueParseErr(err, s) => {
                CssAngleValueParseError::ValueParseErr(err.clone(), s.as_str())
            }
            CssAngleValueParseErrorOwned::InvalidAngle(s) => {
                CssAngleValueParseError::InvalidAngle(s.as_str())
            }
        }
    }
}

/// parses an angle value like `30deg`, `1.64rad`, `100%`, etc.
pub fn parse_angle_value<'a>(input: &'a str) -> Result<AngleValue, CssAngleValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssAngleValueParseError::EmptyString);
    }

    let match_values = &[
        ("deg", AngleMetric::Degree),
        ("turn", AngleMetric::Turn),
        ("grad", AngleMetric::Grad),
        ("rad", AngleMetric::Radians),
        ("%", AngleMetric::Percent),
    ];

    for (match_val, metric) in match_values {
        if input.ends_with(match_val) {
            let value = &input[..input.len() - match_val.len()];
            let value = value.trim();
            if value.is_empty() {
                return Err(CssAngleValueParseError::NoValueGiven(input, *metric));
            }
            match value.parse::<f32>() {
                Ok(o) => {
                    return Ok(AngleValue::from_metric(*metric, o));
                }
                Err(e) => {
                    return Err(CssAngleValueParseError::ValueParseErr(e, value));
                }
            }
        }
    }

    match input.parse::<f32>() {
        Ok(o) => Ok(AngleValue::from_metric(AngleMetric::Percent, o * 100.0)),
        Err(e) => Err(CssAngleValueParseError::InvalidAngle(input)),
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssDirectionCornerParseError<'a> {
    InvalidDirection(&'a str),
}

impl_display! { CssDirectionCornerParseError<'a>, {
    InvalidDirection(val) => format!("Invalid direction: \"{}\"", val),
}}

/// Owned version of CssDirectionCornerParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssDirectionCornerParseErrorOwned {
    InvalidDirection(String),
}

impl<'a> CssDirectionCornerParseError<'a> {
    pub fn to_contained(&self) -> CssDirectionCornerParseErrorOwned {
        match self {
            CssDirectionCornerParseError::InvalidDirection(s) => {
                CssDirectionCornerParseErrorOwned::InvalidDirection(s.to_string())
            }
        }
    }
}

impl CssDirectionCornerParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssDirectionCornerParseError<'a> {
        match self {
            CssDirectionCornerParseErrorOwned::InvalidDirection(s) => {
                CssDirectionCornerParseError::InvalidDirection(s.as_str())
            }
        }
    }
}

pub fn parse_direction_corner<'a>(
    input: &'a str,
) -> Result<DirectionCorner, CssDirectionCornerParseError<'a>> {
    match input {
        "right" => Ok(DirectionCorner::Right),
        "left" => Ok(DirectionCorner::Left),
        "top" => Ok(DirectionCorner::Top),
        "bottom" => Ok(DirectionCorner::Bottom),
        _ => Err(CssDirectionCornerParseError::InvalidDirection(input)),
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssShapeParseError<'a> {
    ShapeErr(InvalidValueErr<'a>),
}

impl_display! {CssShapeParseError<'a>, {
    ShapeErr(e) => format!("\"{}\"", e.0),
}}

/// Owned version of CssShapeParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssShapeParseErrorOwned {
    ShapeErr(InvalidValueErrOwned),
}

impl<'a> CssShapeParseError<'a> {
    pub fn to_contained(&self) -> CssShapeParseErrorOwned {
        match self {
            CssShapeParseError::ShapeErr(err) => {
                CssShapeParseErrorOwned::ShapeErr(err.to_contained())
            }
        }
    }
}

impl CssShapeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssShapeParseError<'a> {
        match self {
            CssShapeParseErrorOwned::ShapeErr(err) => CssShapeParseError::ShapeErr(err.to_shared()),
        }
    }
}

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
typed_pixel_value_parser!(
    parse_style_border_bottom_left_radius,
    StyleBorderBottomLeftRadius
);
typed_pixel_value_parser!(
    parse_style_border_top_right_radius,
    StyleBorderTopRightRadius
);
typed_pixel_value_parser!(
    parse_style_border_bottom_right_radius,
    StyleBorderBottomRightRadius
);

typed_pixel_value_parser!(parse_style_border_top_width, LayoutBorderTopWidth);
typed_pixel_value_parser!(parse_style_border_bottom_width, LayoutBorderBottomWidth);
typed_pixel_value_parser!(parse_style_border_right_width, LayoutBorderRightWidth);
typed_pixel_value_parser!(parse_style_border_left_width, LayoutBorderLeftWidth);

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display! {FlexGrowParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-grow: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

impl<'a> FlexGrowParseError<'a> {
    pub fn to_contained(&self) -> FlexGrowParseErrorOwned {
        match self {
            FlexGrowParseError::ParseFloat(err, s) => {
                FlexGrowParseErrorOwned::ParseFloat(err.clone(), s.to_string())
            }
        }
    }
}

impl FlexGrowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexGrowParseError<'a> {
        match self {
            FlexGrowParseErrorOwned::ParseFloat(err, s) => {
                FlexGrowParseError::ParseFloat(err.clone(), s)
            }
        }
    }
}

pub fn parse_layout_flex_grow<'a>(
    input: &'a str,
) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexGrow { inner: o }),
        Err(e) => Err(FlexGrowParseError::ParseFloat(e, input)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

impl_display! {FlexShrinkParseError<'a>, {
    ParseFloat(e, orig_str) => format!("flex-shrink: Could not parse floating-point value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

/// Owned version of FlexShrinkParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

impl<'a> FlexShrinkParseError<'a> {
    pub fn to_contained(&self) -> FlexShrinkParseErrorOwned {
        match self {
            FlexShrinkParseError::ParseFloat(err, s) => {
                FlexShrinkParseErrorOwned::ParseFloat(err.clone(), s.to_string())
            }
        }
    }
}

impl FlexShrinkParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexShrinkParseError<'a> {
        match self {
            FlexShrinkParseErrorOwned::ParseFloat(err, s) => {
                FlexShrinkParseError::ParseFloat(err.clone(), s.as_str())
            }
        }
    }
}

pub fn parse_layout_flex_shrink<'a>(
    input: &'a str,
) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexShrink { inner: o }),
        Err(e) => Err(FlexShrinkParseError::ParseFloat(e, input)),
    }
}

pub fn parse_style_tab_width(input: &str) -> Result<StyleTabWidth, PercentageParseError> {
    parse_percentage_value(input).and_then(|e| Ok(StyleTabWidth { inner: e }))
}

pub fn parse_style_line_height(input: &str) -> Result<StyleLineHeight, PercentageParseError> {
    parse_percentage_value(input).and_then(|e| Ok(StyleLineHeight { inner: e }))
}

typed_pixel_value_parser!(parse_style_font_size, StyleFontSize);

#[derive(Debug, Clone, PartialEq)]
pub enum OpacityParseError<'a> {
    ParsePercentage(PercentageParseError, &'a str),
}

impl_display! {OpacityParseError<'a>, {
    ParsePercentage(e, orig_str) => format!("opacity: Could not parse percentage value: \"{}\" - Error: \"{}\"", orig_str, e),
}}

/// Owned version of OpacityParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum OpacityParseErrorOwned {
    ParsePercentage(PercentageParseError, String),
}

impl<'a> OpacityParseError<'a> {
    pub fn to_contained(&self) -> OpacityParseErrorOwned {
        match self {
            OpacityParseError::ParsePercentage(err, s) => {
                OpacityParseErrorOwned::ParsePercentage(err.clone(), s.to_string())
            }
        }
    }
}

impl OpacityParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> OpacityParseError<'a> {
        match self {
            OpacityParseErrorOwned::ParsePercentage(err, s) => {
                OpacityParseError::ParsePercentage(err.clone(), s.as_str())
            }
        }
    }
}

pub fn parse_style_opacity<'a>(input: &'a str) -> Result<StyleOpacity, OpacityParseError<'a>> {
    parse_percentage_value(input)
        .map_err(|e| OpacityParseError::ParsePercentage(e, input))
        .and_then(|e| Ok(StyleOpacity { inner: e }))
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssStyleFontFamilyParseError<'a> {
    InvalidStyleFontFamily(&'a str),
    UnclosedQuotes(&'a str),
}

impl_display! {CssStyleFontFamilyParseError<'a>, {
    InvalidStyleFontFamily(val) => format!("Invalid font-family: \"{}\"", val),
    UnclosedQuotes(val) => format!("Unclosed quotes: \"{}\"", val),
}}

impl<'a> From<UnclosedQuotesError<'a>> for CssStyleFontFamilyParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssStyleFontFamilyParseError::UnclosedQuotes(err.0)
    }
}

/// Owned version of CssStyleFontFamilyParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFontFamilyParseErrorOwned {
    InvalidStyleFontFamily(String),
    UnclosedQuotes(String),
}

impl<'a> CssStyleFontFamilyParseError<'a> {
    pub fn to_contained(&self) -> CssStyleFontFamilyParseErrorOwned {
        match self {
            CssStyleFontFamilyParseError::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s.to_string())
            }
            CssStyleFontFamilyParseError::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(s.to_string())
            }
        }
    }
}

impl CssStyleFontFamilyParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleFontFamilyParseError<'a> {
        match self {
            CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseError::InvalidStyleFontFamily(s.as_str())
            }
            CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseError::UnclosedQuotes(s.as_str())
            }
        }
    }
}

/// Parses a `StyleFontFamily` declaration from a `&str`
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::parser::parse_style_font_family;
/// # use azul_css::{StyleFontFamily, StyleFontFamilyVec};
/// let input = "\"Helvetica\", 'Arial', Times New Roman";
/// let fonts: StyleFontFamilyVec = vec![
///     StyleFontFamily::Native("Helvetica".into()),
///     StyleFontFamily::Native("Arial".into()),
///     StyleFontFamily::Native("Times New Roman".into()),
/// ]
/// .into();
///
/// assert_eq!(parse_style_font_family(input), Ok(fonts));
/// ```
pub fn parse_style_font_family<'a>(
    input: &'a str,
) -> Result<StyleFontFamilyVec, CssStyleFontFamilyParseError<'a>> {
    use alloc::string::ToString;

    let multiple_fonts = input.split(',');
    let mut fonts = Vec::with_capacity(1);

    for font in multiple_fonts {
        let font = font.trim();
        let font = font.trim_matches('\'');
        let font = font.trim_matches('\"');
        let font = font.trim();
        fonts.push(StyleFontFamily::System(font.to_string().into()));
    }

    Ok(fonts.into())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum ParenthesisParseError<'a> {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(&'a str),
    EmptyInput,
}

impl_display! { ParenthesisParseError<'a>, {
    UnclosedBraces => format!("Unclosed parenthesis"),
    NoOpeningBraceFound => format!("Expected value in parenthesis (missing \"(\")"),
    NoClosingBraceFound => format!("Missing closing parenthesis (missing \")\")"),
    StopWordNotFound(e) => format!("Stopword not found, found: \"{}\"", e),
    EmptyInput => format!("Empty parenthesis"),
}}

/// Owned version of ParenthesisParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum ParenthesisParseErrorOwned {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(String),
    EmptyInput,
}

impl<'a> ParenthesisParseError<'a> {
    pub fn to_contained(&self) -> ParenthesisParseErrorOwned {
        match self {
            ParenthesisParseError::UnclosedBraces => ParenthesisParseErrorOwned::UnclosedBraces,
            ParenthesisParseError::NoOpeningBraceFound => {
                ParenthesisParseErrorOwned::NoOpeningBraceFound
            }
            ParenthesisParseError::NoClosingBraceFound => {
                ParenthesisParseErrorOwned::NoClosingBraceFound
            }
            ParenthesisParseError::StopWordNotFound(s) => {
                ParenthesisParseErrorOwned::StopWordNotFound(s.to_string())
            }
            ParenthesisParseError::EmptyInput => ParenthesisParseErrorOwned::EmptyInput,
        }
    }
}

impl ParenthesisParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> ParenthesisParseError<'a> {
        match self {
            ParenthesisParseErrorOwned::UnclosedBraces => ParenthesisParseError::UnclosedBraces,
            ParenthesisParseErrorOwned::NoOpeningBraceFound => {
                ParenthesisParseError::NoOpeningBraceFound
            }
            ParenthesisParseErrorOwned::NoClosingBraceFound => {
                ParenthesisParseError::NoClosingBraceFound
            }
            ParenthesisParseErrorOwned::StopWordNotFound(s) => {
                ParenthesisParseError::StopWordNotFound(s.as_str())
            }
            ParenthesisParseErrorOwned::EmptyInput => ParenthesisParseError::EmptyInput,
        }
    }
}

/// Checks wheter a given input is enclosed in parentheses, prefixed
/// by a certain number of stopwords.
///
/// On success, returns what the stopword was + the string inside the braces
/// on failure returns None.
///
/// ```rust
/// # use azul_css::parser::parse_parentheses;
/// # use azul_css::parser::ParenthesisParseError::*;
/// // Search for the nearest "abc()" brace
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc"]),
///     Ok(("abc", "def(g)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["def"]),
///     Err(StopWordNotFound("abc"))
/// );
/// assert_eq!(
///     parse_parentheses("def(ghi(j))", &["def"]),
///     Ok(("def", "ghi(j)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc", "def"]),
///     Ok(("abc", "def(g)"))
/// );
/// ```
pub fn parse_parentheses<'a>(
    input: &'a str,
    stopwords: &[&'static str],
) -> Result<(&'static str, &'a str), ParenthesisParseError<'a>> {
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

    Ok((
        validated_stopword,
        &input[(first_open_brace + 1)..last_closing_brace],
    ))
}

multi_type_parser!(
    parse_style_mix_blend_mode,
    StyleMixBlendMode,
    ["normal", Normal],
    ["multiply", Multiply],
    ["screen", Screen],
    ["overlay", Overlay],
    ["darken", Darken],
    ["lighten", Lighten],
    ["color-dodge", ColorDodge],
    ["color-burn", ColorBurn],
    ["hard-light", HardLight],
    ["soft-light", SoftLight],
    ["difference", Difference],
    ["exclusion", Exclusion],
    ["hue", Hue],
    ["saturation", Saturation],
    ["color", Color],
    ["luminosity", Luminosity]
);

multi_type_parser!(
    parse_style_border_style,
    BorderStyle,
    ["none", None],
    ["solid", Solid],
    ["double", Double],
    ["dotted", Dotted],
    ["dashed", Dashed],
    ["hidden", Hidden],
    ["groove", Groove],
    ["ridge", Ridge],
    ["inset", Inset],
    ["outset", Outset]
);

multi_type_parser!(
    parse_style_cursor,
    StyleCursor,
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
    ["zoom-out", ZoomOut]
);

multi_type_parser!(
    parse_style_backface_visibility,
    StyleBackfaceVisibility,
    ["hidden", Hidden],
    ["visible", Visible]
);

pub fn parse_style_background_size<'a>(
    input: &'a str,
) -> Result<StyleBackgroundSize, InvalidValueErr<'a>> {
    let input = input.trim();
    match input {
        "contain" => Ok(StyleBackgroundSize::Contain),
        "cover" => Ok(StyleBackgroundSize::Cover),
        other => {
            let other = other.trim();
            let mut iter = other.split_whitespace();
            let x_pos = iter.next().ok_or(InvalidValueErr(input))?;
            let x_pos = parse_pixel_value(x_pos).map_err(|_| InvalidValueErr(input))?;
            let y_pos = iter.next().ok_or(InvalidValueErr(input))?;
            let y_pos = parse_pixel_value(y_pos).map_err(|_| InvalidValueErr(input))?;
            Ok(StyleBackgroundSize::ExactSize([x_pos, y_pos]))
        }
    }
}

impl FormatAsCssValue for StyleBackgroundSize {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleBackgroundSize::Contain => write!(f, "contain"),
            StyleBackgroundSize::Cover => write!(f, "cover"),
            StyleBackgroundSize::ExactSize([x, y]) => write!(f, "{} {}", x, y),
        }
    }
}

multi_type_parser!(
    parse_style_background_repeat,
    StyleBackgroundRepeat,
    ["no-repeat", NoRepeat],
    ["repeat", Repeat],
    ["repeat-x", RepeatX],
    ["repeat-y", RepeatY]
);

multi_type_parser!(
    parse_layout_display,
    LayoutDisplay,
    ["none", None],
    ["block", Block],
    ["inline", Inline],
    ["inline-block", InlineBlock],
    ["flex", Flex],
    ["inline-flex", InlineFlex]
);

multi_type_parser!(
    parse_layout_float,
    LayoutFloat,
    ["left", Left],
    ["right", Right]
);

multi_type_parser!(
    parse_layout_box_sizing,
    LayoutBoxSizing,
    ["content-box", ContentBox],
    ["border-box", BorderBox]
);

multi_type_parser!(
    parse_layout_direction,
    LayoutFlexDirection,
    ["row", Row],
    ["row-reverse", RowReverse],
    ["column", Column],
    ["column-reverse", ColumnReverse]
);

multi_type_parser!(
    parse_layout_wrap,
    LayoutFlexWrap,
    ["wrap", Wrap],
    ["nowrap", NoWrap]
);

multi_type_parser!(
    parse_layout_justify_content,
    LayoutJustifyContent,
    ["flex-start", Start],
    ["flex-end", End],
    ["center", Center],
    ["space-between", SpaceBetween],
    ["space-around", SpaceAround],
    ["space-evenly", SpaceEvenly]
);

multi_type_parser!(
    parse_layout_align_items,
    LayoutAlignItems,
    ["flex-start", FlexStart],
    ["flex-end", FlexEnd],
    ["stretch", Stretch],
    ["center", Center]
);

multi_type_parser!(
    parse_layout_align_content,
    LayoutAlignContent,
    ["flex-start", Start],
    ["flex-end", End],
    ["stretch", Stretch],
    ["center", Center],
    ["space-between", SpaceBetween],
    ["space-around", SpaceAround]
);

multi_type_parser!(parse_shape, Shape, ["circle", Circle], ["ellipse", Ellipse]);

multi_type_parser!(
    parse_layout_position,
    LayoutPosition,
    ["static", Static],
    ["fixed", Fixed],
    ["absolute", Absolute],
    ["relative", Relative]
);

multi_type_parser!(
    parse_layout_overflow,
    LayoutOverflow,
    ["auto", Auto],
    ["scroll", Scroll],
    ["visible", Visible],
    ["hidden", Hidden]
);

multi_type_parser!(
    parse_layout_text_align,
    StyleTextAlign,
    ["center", Center],
    ["left", Left],
    ["right", Right]
);
