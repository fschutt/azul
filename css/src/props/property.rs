//! Defines the core `CssProperty` enum, which represents any single parsed CSS property,
//! as well as top-level functions for parsing CSS keys and values.

use alloc::{
    boxed::Box,
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

// Import all property types from their new locations
use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        font::*,
        font::{
            parse_style_font_family, CssStyleFontFamilyParseError,
            CssStyleFontFamilyParseErrorOwned, StyleFontFamilyVec,
        },
        length::{parse_float_value, parse_percentage_value, FloatValue, PercentageValue},
        pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
    },
    formatter::PrintAsCssValue,
    layout::{dimensions::*, display::*, flex::*, overflow::*, position::*, spacing::*},
    style::{
        background::*, border::*, border_radius::*, box_shadow::*, effects::*, filter::*,
        scrollbar::*, text::*, transform::*,
    },
};
use crate::{
    corety::AzString,
    css::CssPropertyValue,
    props::basic::{error::InvalidValueErr, pixel::PixelValueWithAuto},
};

const COMBINED_CSS_PROPERTIES_KEY_MAP: [(CombinedCssPropertyType, &'static str); 12] = [
    (CombinedCssPropertyType::BorderRadius, "border-radius"),
    (CombinedCssPropertyType::Overflow, "overflow"),
    (CombinedCssPropertyType::Padding, "padding"),
    (CombinedCssPropertyType::Margin, "margin"),
    (CombinedCssPropertyType::Border, "border"),
    (CombinedCssPropertyType::BorderLeft, "border-left"),
    (CombinedCssPropertyType::BorderRight, "border-right"),
    (CombinedCssPropertyType::BorderTop, "border-top"),
    (CombinedCssPropertyType::BorderBottom, "border-bottom"),
    (CombinedCssPropertyType::BoxShadow, "box-shadow"),
    (CombinedCssPropertyType::BackgroundColor, "background-color"),
    (CombinedCssPropertyType::BackgroundImage, "background-image"),
];

const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &'static str); 77] = [
    (CssPropertyType::Display, "display"),
    (CssPropertyType::Float, "float"),
    (CssPropertyType::BoxSizing, "box-sizing"),
    (CssPropertyType::TextColor, "color"),
    (CssPropertyType::FontSize, "font-size"),
    (CssPropertyType::FontFamily, "font-family"),
    (CssPropertyType::TextAlign, "text-align"),
    (CssPropertyType::LetterSpacing, "letter-spacing"),
    (CssPropertyType::LineHeight, "line-height"),
    (CssPropertyType::WordSpacing, "word-spacing"),
    (CssPropertyType::TabWidth, "tab-width"),
    (CssPropertyType::WhiteSpace, "white-space"),
    (CssPropertyType::Hyphens, "hyphens"),
    (CssPropertyType::Direction, "direction"),
    (CssPropertyType::Cursor, "cursor"),
    (CssPropertyType::Width, "width"),
    (CssPropertyType::Height, "height"),
    (CssPropertyType::MinWidth, "min-width"),
    (CssPropertyType::MinHeight, "min-height"),
    (CssPropertyType::MaxWidth, "max-width"),
    (CssPropertyType::MaxHeight, "max-height"),
    (CssPropertyType::Position, "position"),
    (CssPropertyType::Top, "top"),
    (CssPropertyType::Right, "right"),
    (CssPropertyType::Left, "left"),
    (CssPropertyType::Bottom, "bottom"),
    (CssPropertyType::FlexWrap, "flex-wrap"),
    (CssPropertyType::FlexDirection, "flex-direction"),
    (CssPropertyType::FlexGrow, "flex-grow"),
    (CssPropertyType::FlexShrink, "flex-shrink"),
    (CssPropertyType::JustifyContent, "justify-content"),
    (CssPropertyType::AlignItems, "align-items"),
    (CssPropertyType::AlignContent, "align-content"),
    (CssPropertyType::OverflowX, "overflow-x"),
    (CssPropertyType::OverflowY, "overflow-y"),
    (CssPropertyType::PaddingTop, "padding-top"),
    (CssPropertyType::PaddingLeft, "padding-left"),
    (CssPropertyType::PaddingRight, "padding-right"),
    (CssPropertyType::PaddingBottom, "padding-bottom"),
    (CssPropertyType::MarginTop, "margin-top"),
    (CssPropertyType::MarginLeft, "margin-left"),
    (CssPropertyType::MarginRight, "margin-right"),
    (CssPropertyType::MarginBottom, "margin-bottom"),
    (CssPropertyType::BackgroundContent, "background"),
    (CssPropertyType::BackgroundPosition, "background-position"),
    (CssPropertyType::BackgroundSize, "background-size"),
    (CssPropertyType::BackgroundRepeat, "background-repeat"),
    (
        CssPropertyType::BorderTopLeftRadius,
        "border-top-left-radius",
    ),
    (
        CssPropertyType::BorderTopRightRadius,
        "border-top-right-radius",
    ),
    (
        CssPropertyType::BorderBottomLeftRadius,
        "border-bottom-left-radius",
    ),
    (
        CssPropertyType::BorderBottomRightRadius,
        "border-bottom-right-radius",
    ),
    (CssPropertyType::BorderTopColor, "border-top-color"),
    (CssPropertyType::BorderRightColor, "border-right-color"),
    (CssPropertyType::BorderLeftColor, "border-left-color"),
    (CssPropertyType::BorderBottomColor, "border-bottom-color"),
    (CssPropertyType::BorderTopStyle, "border-top-style"),
    (CssPropertyType::BorderRightStyle, "border-right-style"),
    (CssPropertyType::BorderLeftStyle, "border-left-style"),
    (CssPropertyType::BorderBottomStyle, "border-bottom-style"),
    (CssPropertyType::BorderTopWidth, "border-top-width"),
    (CssPropertyType::BorderRightWidth, "border-right-width"),
    (CssPropertyType::BorderLeftWidth, "border-left-width"),
    (CssPropertyType::BorderBottomWidth, "border-bottom-width"),
    (CssPropertyType::BoxShadowTop, "-azul-box-shadow-top"),
    (CssPropertyType::BoxShadowRight, "-azul-box-shadow-right"),
    (CssPropertyType::BoxShadowLeft, "-azul-box-shadow-left"),
    (CssPropertyType::BoxShadowBottom, "-azul-box-shadow-bottom"),
    (CssPropertyType::ScrollbarStyle, "-azul-scrollbar-style"),
    (CssPropertyType::Opacity, "opacity"),
    (CssPropertyType::Transform, "transform"),
    (CssPropertyType::PerspectiveOrigin, "perspective-origin"),
    (CssPropertyType::TransformOrigin, "transform-origin"),
    (CssPropertyType::BackfaceVisibility, "backface-visibility"),
    (CssPropertyType::MixBlendMode, "mix-blend-mode"),
    (CssPropertyType::Filter, "filter"),
    (CssPropertyType::BackdropFilter, "backdrop-filter"),
    (CssPropertyType::TextShadow, "text-shadow"),
];

// Type aliases for `CssPropertyValue<T>`
pub type StyleBackgroundContentVecValue = CssPropertyValue<StyleBackgroundContentVec>;
pub type StyleBackgroundPositionVecValue = CssPropertyValue<StyleBackgroundPositionVec>;
pub type StyleBackgroundSizeVecValue = CssPropertyValue<StyleBackgroundSizeVec>;
pub type StyleBackgroundRepeatVecValue = CssPropertyValue<StyleBackgroundRepeatVec>;
pub type StyleFontSizeValue = CssPropertyValue<StyleFontSize>;
pub type StyleFontFamilyVecValue = CssPropertyValue<StyleFontFamilyVec>;
pub type StyleTextColorValue = CssPropertyValue<StyleTextColor>;
pub type StyleTextAlignValue = CssPropertyValue<StyleTextAlign>;
pub type StyleLineHeightValue = CssPropertyValue<StyleLineHeight>;
pub type StyleLetterSpacingValue = CssPropertyValue<StyleLetterSpacing>;
pub type StyleWordSpacingValue = CssPropertyValue<StyleWordSpacing>;
pub type StyleTabWidthValue = CssPropertyValue<StyleTabWidth>;
pub type StyleCursorValue = CssPropertyValue<StyleCursor>;
pub type StyleBoxShadowValue = CssPropertyValue<StyleBoxShadow>;
pub type StyleBorderTopColorValue = CssPropertyValue<StyleBorderTopColor>;
pub type StyleBorderLeftColorValue = CssPropertyValue<StyleBorderLeftColor>;
pub type StyleBorderRightColorValue = CssPropertyValue<StyleBorderRightColor>;
pub type StyleBorderBottomColorValue = CssPropertyValue<StyleBorderBottomColor>;
pub type StyleBorderTopStyleValue = CssPropertyValue<StyleBorderTopStyle>;
pub type StyleBorderLeftStyleValue = CssPropertyValue<StyleBorderLeftStyle>;
pub type StyleBorderRightStyleValue = CssPropertyValue<StyleBorderRightStyle>;
pub type StyleBorderBottomStyleValue = CssPropertyValue<StyleBorderBottomStyle>;
pub type StyleBorderTopLeftRadiusValue = CssPropertyValue<StyleBorderTopLeftRadius>;
pub type StyleBorderTopRightRadiusValue = CssPropertyValue<StyleBorderTopRightRadius>;
pub type StyleBorderBottomLeftRadiusValue = CssPropertyValue<StyleBorderBottomLeftRadius>;
pub type StyleBorderBottomRightRadiusValue = CssPropertyValue<StyleBorderBottomRightRadius>;
pub type StyleOpacityValue = CssPropertyValue<StyleOpacity>;
pub type StyleTransformVecValue = CssPropertyValue<StyleTransformVec>;
pub type StyleTransformOriginValue = CssPropertyValue<StyleTransformOrigin>;
pub type StylePerspectiveOriginValue = CssPropertyValue<StylePerspectiveOrigin>;
pub type StyleBackfaceVisibilityValue = CssPropertyValue<StyleBackfaceVisibility>;
pub type StyleMixBlendModeValue = CssPropertyValue<StyleMixBlendMode>;
pub type StyleFilterVecValue = CssPropertyValue<StyleFilterVec>;
pub type ScrollbarStyleValue = CssPropertyValue<ScrollbarStyle>;
pub type LayoutDisplayValue = CssPropertyValue<LayoutDisplay>;
pub type StyleHyphensValue = CssPropertyValue<StyleHyphens>;
pub type StyleDirectionValue = CssPropertyValue<StyleDirection>;
pub type StyleWhiteSpaceValue = CssPropertyValue<StyleWhiteSpace>;
pub type LayoutFloatValue = CssPropertyValue<LayoutFloat>;
pub type LayoutBoxSizingValue = CssPropertyValue<LayoutBoxSizing>;
pub type LayoutWidthValue = CssPropertyValue<LayoutWidth>;
pub type LayoutHeightValue = CssPropertyValue<LayoutHeight>;
pub type LayoutMinWidthValue = CssPropertyValue<LayoutMinWidth>;
pub type LayoutMinHeightValue = CssPropertyValue<LayoutMinHeight>;
pub type LayoutMaxWidthValue = CssPropertyValue<LayoutMaxWidth>;
pub type LayoutMaxHeightValue = CssPropertyValue<LayoutMaxHeight>;
pub type LayoutPositionValue = CssPropertyValue<LayoutPosition>;
pub type LayoutTopValue = CssPropertyValue<LayoutTop>;
pub type LayoutBottomValue = CssPropertyValue<LayoutBottom>;
pub type LayoutRightValue = CssPropertyValue<LayoutRight>;
pub type LayoutLeftValue = CssPropertyValue<LayoutLeft>;
pub type LayoutPaddingTopValue = CssPropertyValue<LayoutPaddingTop>;
pub type LayoutPaddingBottomValue = CssPropertyValue<LayoutPaddingBottom>;
pub type LayoutPaddingLeftValue = CssPropertyValue<LayoutPaddingLeft>;
pub type LayoutPaddingRightValue = CssPropertyValue<LayoutPaddingRight>;
pub type LayoutMarginTopValue = CssPropertyValue<LayoutMarginTop>;
pub type LayoutMarginBottomValue = CssPropertyValue<LayoutMarginBottom>;
pub type LayoutMarginLeftValue = CssPropertyValue<LayoutMarginLeft>;
pub type LayoutMarginRightValue = CssPropertyValue<LayoutMarginRight>;
pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
pub type LayoutBorderLeftWidthValue = CssPropertyValue<LayoutBorderLeftWidth>;
pub type LayoutBorderRightWidthValue = CssPropertyValue<LayoutBorderRightWidth>;
pub type LayoutBorderBottomWidthValue = CssPropertyValue<LayoutBorderBottomWidth>;
pub type LayoutOverflowValue = CssPropertyValue<LayoutOverflow>;
pub type LayoutFlexDirectionValue = CssPropertyValue<LayoutFlexDirection>;
pub type LayoutFlexWrapValue = CssPropertyValue<LayoutFlexWrap>;
pub type LayoutFlexGrowValue = CssPropertyValue<LayoutFlexGrow>;
pub type LayoutFlexShrinkValue = CssPropertyValue<LayoutFlexShrink>;
pub type LayoutJustifyContentValue = CssPropertyValue<LayoutJustifyContent>;
pub type LayoutAlignItemsValue = CssPropertyValue<LayoutAlignItems>;
pub type LayoutAlignContentValue = CssPropertyValue<LayoutAlignContent>;

/// Represents one parsed CSS key-value pair, such as `"width: 20px"` =>
/// `CssProperty::Width(LayoutWidth::px(20.0))`
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssProperty {
    TextColor(StyleTextColorValue),
    FontSize(StyleFontSizeValue),
    FontFamily(StyleFontFamilyVecValue),
    TextAlign(StyleTextAlignValue),
    LetterSpacing(StyleLetterSpacingValue),
    LineHeight(StyleLineHeightValue),
    WordSpacing(StyleWordSpacingValue),
    TabWidth(StyleTabWidthValue),
    WhiteSpace(StyleWhiteSpaceValue),
    Hyphens(StyleHyphensValue),
    Direction(StyleDirectionValue),
    Cursor(StyleCursorValue),
    Display(LayoutDisplayValue),
    Float(LayoutFloatValue),
    BoxSizing(LayoutBoxSizingValue),
    Width(LayoutWidthValue),
    Height(LayoutHeightValue),
    MinWidth(LayoutMinWidthValue),
    MinHeight(LayoutMinHeightValue),
    MaxWidth(LayoutMaxWidthValue),
    MaxHeight(LayoutMaxHeightValue),
    Position(LayoutPositionValue),
    Top(LayoutTopValue),
    Right(LayoutRightValue),
    Left(LayoutLeftValue),
    Bottom(LayoutBottomValue),
    FlexWrap(LayoutFlexWrapValue),
    FlexDirection(LayoutFlexDirectionValue),
    FlexGrow(LayoutFlexGrowValue),
    FlexShrink(LayoutFlexShrinkValue),
    JustifyContent(LayoutJustifyContentValue),
    AlignItems(LayoutAlignItemsValue),
    AlignContent(LayoutAlignContentValue),
    BackgroundContent(StyleBackgroundContentVecValue),
    BackgroundPosition(StyleBackgroundPositionVecValue),
    BackgroundSize(StyleBackgroundSizeVecValue),
    BackgroundRepeat(StyleBackgroundRepeatVecValue),
    OverflowX(LayoutOverflowValue),
    OverflowY(LayoutOverflowValue),
    PaddingTop(LayoutPaddingTopValue),
    PaddingLeft(LayoutPaddingLeftValue),
    PaddingRight(LayoutPaddingRightValue),
    PaddingBottom(LayoutPaddingBottomValue),
    MarginTop(LayoutMarginTopValue),
    MarginLeft(LayoutMarginLeftValue),
    MarginRight(LayoutMarginRightValue),
    MarginBottom(LayoutMarginBottomValue),
    BorderTopLeftRadius(StyleBorderTopLeftRadiusValue),
    BorderTopRightRadius(StyleBorderTopRightRadiusValue),
    BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue),
    BorderBottomRightRadius(StyleBorderBottomRightRadiusValue),
    BorderTopColor(StyleBorderTopColorValue),
    BorderRightColor(StyleBorderRightColorValue),
    BorderLeftColor(StyleBorderLeftColorValue),
    BorderBottomColor(StyleBorderBottomColorValue),
    BorderTopStyle(StyleBorderTopStyleValue),
    BorderRightStyle(StyleBorderRightStyleValue),
    BorderLeftStyle(StyleBorderLeftStyleValue),
    BorderBottomStyle(StyleBorderBottomStyleValue),
    BorderTopWidth(LayoutBorderTopWidthValue),
    BorderRightWidth(LayoutBorderRightWidthValue),
    BorderLeftWidth(LayoutBorderLeftWidthValue),
    BorderBottomWidth(LayoutBorderBottomWidthValue),
    BoxShadowLeft(StyleBoxShadowValue),
    BoxShadowRight(StyleBoxShadowValue),
    BoxShadowTop(StyleBoxShadowValue),
    BoxShadowBottom(StyleBoxShadowValue),
    ScrollbarStyle(ScrollbarStyleValue),
    Opacity(StyleOpacityValue),
    Transform(StyleTransformVecValue),
    TransformOrigin(StyleTransformOriginValue),
    PerspectiveOrigin(StylePerspectiveOriginValue),
    BackfaceVisibility(StyleBackfaceVisibilityValue),
    MixBlendMode(StyleMixBlendModeValue),
    Filter(StyleFilterVecValue),
    BackdropFilter(StyleFilterVecValue),
    TextShadow(StyleBoxShadowValue),
}

/// Represents a CSS key (for example `"border-radius"` => `BorderRadius`).
/// You can also derive this key from a `CssProperty` by calling `CssProperty::get_type()`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum CssPropertyType {
    TextColor,
    FontSize,
    FontFamily,
    TextAlign,
    LetterSpacing,
    LineHeight,
    WordSpacing,
    TabWidth,
    WhiteSpace,
    Hyphens,
    Direction,
    Cursor,
    Display,
    Float,
    BoxSizing,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Position,
    Top,
    Right,
    Left,
    Bottom,
    FlexWrap,
    FlexDirection,
    FlexGrow,
    FlexShrink,
    JustifyContent,
    AlignItems,
    AlignContent,
    BackgroundContent,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    OverflowX,
    OverflowY,
    PaddingTop,
    PaddingLeft,
    PaddingRight,
    PaddingBottom,
    MarginTop,
    MarginLeft,
    MarginRight,
    MarginBottom,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomLeftRadius,
    BorderBottomRightRadius,
    BorderTopColor,
    BorderRightColor,
    BorderLeftColor,
    BorderBottomColor,
    BorderTopStyle,
    BorderRightStyle,
    BorderLeftStyle,
    BorderBottomStyle,
    BorderTopWidth,
    BorderRightWidth,
    BorderLeftWidth,
    BorderBottomWidth,
    BoxShadowLeft,
    BoxShadowRight,
    BoxShadowTop,
    BoxShadowBottom,
    ScrollbarStyle,
    Opacity,
    Transform,
    TransformOrigin,
    PerspectiveOrigin,
    BackfaceVisibility,
    MixBlendMode,
    Filter,
    BackdropFilter,
    TextShadow,
}

// -- PARSING --

/// Master error type that aggregates all possible CSS parsing errors.
#[derive(Clone, PartialEq)]
pub enum CssParsingError<'a> {
    // Shorthand properties
    Border(CssBorderParseError<'a>),
    BorderRadius(CssStyleBorderRadiusParseError<'a>),
    Padding(LayoutPaddingParseError<'a>),
    Margin(LayoutMarginParseError<'a>),
    Overflow(crate::parser::InvalidValueErr<'a>),
    BoxShadow(CssShadowParseError<'a>),

    // Individual properties
    Color(CssColorParseError<'a>),
    PixelValue(CssPixelValueParseError<'a>),
    Percentage(PercentageParseError),
    FontFamily(CssStyleFontFamilyParseError<'a>),
    InvalidValue(crate::parser::InvalidValueErr<'a>),
    FlexGrow(FlexGrowParseError<'a>),
    FlexShrink(FlexShrinkParseError<'a>),
    Background(CssBackgroundParseError<'a>),
    BackgroundPosition(CssBackgroundPositionParseError<'a>),
    Opacity(OpacityParseError<'a>),
    Scrollbar(CssScrollbarStyleParseError<'a>),
    Transform(CssStyleTransformParseError<'a>),
    TransformOrigin(CssStyleTransformOriginParseError<'a>),
    PerspectiveOrigin(CssStylePerspectiveOriginParseError<'a>),
    Filter(CssStyleFilterParseError<'a>),
}

/// Owned version of `CssParsingError`.
#[derive(Debug, Clone, PartialEq)]
pub enum CssParsingErrorOwned {
    // Shorthand properties
    Border(CssBorderParseErrorOwned),
    BorderRadius(CssStyleBorderRadiusParseErrorOwned),
    Padding(LayoutPaddingParseErrorOwned),
    Margin(LayoutMarginParseErrorOwned),
    Overflow(crate::parser::InvalidValueErrOwned),
    BoxShadow(CssShadowParseErrorOwned),

    // Individual properties
    Color(CssColorParseErrorOwned),
    PixelValue(CssPixelValueParseErrorOwned),
    Percentage(PercentageParseError),
    FontFamily(CssStyleFontFamilyParseErrorOwned),
    InvalidValue(crate::parser::InvalidValueErrOwned),
    FlexGrow(FlexGrowParseErrorOwned),
    FlexShrink(FlexShrinkParseErrorOwned),
    Background(CssBackgroundParseErrorOwned),
    BackgroundPosition(CssBackgroundPositionParseErrorOwned),
    Opacity(OpacityParseErrorOwned),
    Scrollbar(CssScrollbarStyleParseErrorOwned),
    Transform(CssStyleTransformParseErrorOwned),
    TransformOrigin(CssStyleTransformOriginParseErrorOwned),
    PerspectiveOrigin(CssStylePerspectiveOriginParseErrorOwned),
    Filter(CssStyleFilterParseErrorOwned),
}

// -- PARSING ERROR IMPLEMENTATIONS --

impl_debug_as_display!(CssParsingError<'a>);
impl_display! { CssParsingError<'a>, {
    Border(e) => format!("Invalid border property: {}", e),
    BorderRadius(e) => format!("Invalid border-radius: {}", e),
    Padding(e) => format!("Invalid padding property: {}", e),
    Margin(e) => format!("Invalid margin property: {}", e),
    Overflow(e) => format!("Invalid overflow property: \"{}\"", e.0),
    BoxShadow(e) => format!("Invalid shadow property: {}", e),
    Color(e) => format!("Invalid color value: {}", e),
    PixelValue(e) => format!("Invalid pixel value: {}", e),
    Percentage(e) => format!("Invalid percentage value: {}", e),
    FontFamily(e) => format!("Invalid font-family value: {}", e),
    InvalidValue(e) => format!("Invalid value: \"{}\"", e.0),
    FlexGrow(e) => format!("Invalid flex-grow value: {}", e),
    FlexShrink(e) => format!("Invalid flex-shrink value: {}", e),
    Background(e) => format!("Invalid background property: {}", e),
    BackgroundPosition(e) => format!("Invalid background-position: {}", e),
    Opacity(e) => format!("Invalid opacity value: {}", e),
    Scrollbar(e) => format!("Invalid scrollbar style: {}", e),
    Transform(e) => format!("Invalid transform property: {}", e),
    TransformOrigin(e) => format!("Invalid transform-origin: {}", e),
    PerspectiveOrigin(e) => format!("Invalid perspective-origin: {}", e),
    Filter(e) => format!("Invalid filter property: {}", e),
}}

// From impls for CssParsingError
impl_from!(CssBorderParseError<'a>, CssParsingError::Border);
impl_from!(
    CssStyleBorderRadiusParseError<'a>,
    CssParsingError::BorderRadius
);
impl_from!(LayoutPaddingParseError<'a>, CssParsingError::Padding);
impl_from!(LayoutMarginParseError<'a>, CssParsingError::Margin);
impl_from!(CssShadowParseError<'a>, CssParsingError::BoxShadow);
impl_from!(CssColorParseError<'a>, CssParsingError::Color);
impl_from!(CssPixelValueParseError<'a>, CssParsingError::PixelValue);
impl_from!(
    CssStyleFontFamilyParseError<'a>,
    CssParsingError::FontFamily
);
impl_from!(FlexGrowParseError<'a>, CssParsingError::FlexGrow);
impl_from!(FlexShrinkParseError<'a>, CssParsingError::FlexShrink);
impl_from!(CssBackgroundParseError<'a>, CssParsingError::Background);
impl_from!(
    CssBackgroundPositionParseError<'a>,
    CssParsingError::BackgroundPosition
);
impl_from!(OpacityParseError<'a>, CssParsingError::Opacity);
impl_from!(CssScrollbarStyleParseError<'a>, CssParsingError::Scrollbar);
impl_from!(CssStyleTransformParseError<'a>, CssParsingError::Transform);
impl_from!(
    CssStyleTransformOriginParseError<'a>,
    CssParsingError::TransformOrigin
);
impl_from!(
    CssStylePerspectiveOriginParseError<'a>,
    CssParsingError::PerspectiveOrigin
);
impl_from!(CssStyleFilterParseError<'a>, CssParsingError::Filter);

impl<'a> From<crate::parser::InvalidValueErr<'a>> for CssParsingError<'a> {
    fn from(e: crate::parser::InvalidValueErr<'a>) -> Self {
        CssParsingError::InvalidValue(e)
    }
}

impl<'a> From<PercentageParseError> for CssParsingError<'a> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::Percentage(e)
    }
}

impl<'a> CssParsingError<'a> {
    pub fn to_contained(&self) -> CssParsingErrorOwned {
        match self {
            CssParsingError::Border(e) => CssParsingErrorOwned::Border(e.to_contained()),
            CssParsingError::BorderRadius(e) => {
                CssParsingErrorOwned::BorderRadius(e.to_contained())
            }
            CssParsingError::Padding(e) => CssParsingErrorOwned::Padding(e.to_contained()),
            CssParsingError::Margin(e) => CssParsingErrorOwned::Margin(e.to_contained()),
            CssParsingError::Overflow(e) => CssParsingErrorOwned::Overflow(e.to_contained()),
            CssParsingError::BoxShadow(e) => CssParsingErrorOwned::BoxShadow(e.to_contained()),
            CssParsingError::Color(e) => CssParsingErrorOwned::Color(e.to_contained()),
            CssParsingError::PixelValue(e) => CssParsingErrorOwned::PixelValue(e.to_contained()),
            CssParsingError::Percentage(e) => CssParsingErrorOwned::Percentage(e.clone()),
            CssParsingError::FontFamily(e) => CssParsingErrorOwned::FontFamily(e.to_contained()),
            CssParsingError::InvalidValue(e) => {
                CssParsingErrorOwned::InvalidValue(e.to_contained())
            }
            CssParsingError::FlexGrow(e) => CssParsingErrorOwned::FlexGrow(e.to_contained()),
            CssParsingError::FlexShrink(e) => CssParsingErrorOwned::FlexShrink(e.to_contained()),
            CssParsingError::Background(e) => CssParsingErrorOwned::Background(e.to_contained()),
            CssParsingError::BackgroundPosition(e) => {
                CssParsingErrorOwned::BackgroundPosition(e.to_contained())
            }
            CssParsingError::Opacity(e) => CssParsingErrorOwned::Opacity(e.to_contained()),
            CssParsingError::Scrollbar(e) => CssParsingErrorOwned::Scrollbar(e.to_contained()),
            CssParsingError::Transform(e) => CssParsingErrorOwned::Transform(e.to_contained()),
            CssParsingError::TransformOrigin(e) => {
                CssParsingErrorOwned::TransformOrigin(e.to_contained())
            }
            CssParsingError::PerspectiveOrigin(e) => {
                CssParsingErrorOwned::PerspectiveOrigin(e.to_contained())
            }
            CssParsingError::Filter(e) => CssParsingErrorOwned::Filter(e.to_contained()),
        }
    }
}

impl CssParsingErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParsingError<'a> {
        match self {
            CssParsingErrorOwned::Border(e) => CssParsingError::Border(e.to_shared()),
            CssParsingErrorOwned::BorderRadius(e) => CssParsingError::BorderRadius(e.to_shared()),
            CssParsingErrorOwned::Padding(e) => CssParsingError::Padding(e.to_shared()),
            CssParsingErrorOwned::Margin(e) => CssParsingError::Margin(e.to_shared()),
            CssParsingErrorOwned::Overflow(e) => CssParsingError::Overflow(e.to_shared()),
            CssParsingErrorOwned::BoxShadow(e) => CssParsingError::BoxShadow(e.to_shared()),
            CssParsingErrorOwned::Color(e) => CssParsingError::Color(e.to_shared()),
            CssParsingErrorOwned::PixelValue(e) => CssParsingError::PixelValue(e.to_shared()),
            CssParsingErrorOwned::Percentage(e) => CssParsingError::Percentage(e.clone()),
            CssParsingErrorOwned::FontFamily(e) => CssParsingError::FontFamily(e.to_shared()),
            CssParsingErrorOwned::InvalidValue(e) => CssParsingError::InvalidValue(e.to_shared()),
            CssParsingErrorOwned::FlexGrow(e) => CssParsingError::FlexGrow(e.to_shared()),
            CssParsingErrorOwned::FlexShrink(e) => CssParsingError::FlexShrink(e.to_shared()),
            CssParsingErrorOwned::Background(e) => CssParsingError::Background(e.to_shared()),
            CssParsingErrorOwned::BackgroundPosition(e) => {
                CssParsingError::BackgroundPosition(e.to_shared())
            }
            CssParsingErrorOwned::Opacity(e) => CssParsingError::Opacity(e.to_shared()),
            CssParsingErrorOwned::Scrollbar(e) => CssParsingError::Scrollbar(e.to_shared()),
            CssParsingErrorOwned::Transform(e) => CssParsingError::Transform(e.to_shared()),
            CssParsingErrorOwned::TransformOrigin(e) => {
                CssParsingError::TransformOrigin(e.to_shared())
            }
            CssParsingErrorOwned::PerspectiveOrigin(e) => {
                CssParsingError::PerspectiveOrigin(e.to_shared())
            }
            CssParsingErrorOwned::Filter(e) => CssParsingError::Filter(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_css_property<'a>(
    key: CssPropertyType,
    value: &'a str,
) -> Result<CssProperty, CssParsingError<'a>> {
    let value = value.trim();
    Ok(match value {
        "auto" => CssProperty::auto(key),
        "none" => CssProperty::none(key),
        "initial" => CssProperty::initial(key),
        "inherit" => CssProperty::inherit(key),
        value => match key {
            CssPropertyType::TextColor => parse_style_text_color(value)?.into(),
            CssPropertyType::FontSize => parse_style_font_size(value)?.into(),
            CssPropertyType::FontFamily => parse_style_font_family(value)?.into(),
            CssPropertyType::TextAlign => parse_style_text_align(value)?.into(),
            CssPropertyType::LetterSpacing => parse_style_letter_spacing(value)?.into(),
            CssPropertyType::LineHeight => parse_style_line_height(value)?.into(),
            CssPropertyType::WordSpacing => parse_style_word_spacing(value)?.into(),
            CssPropertyType::TabWidth => parse_style_tab_width(value)?.into(),
            CssPropertyType::WhiteSpace => parse_style_white_space(value)?.into(),
            CssPropertyType::Hyphens => parse_style_hyphens(value)?.into(),
            CssPropertyType::Direction => parse_style_direction(value)?.into(),
            CssPropertyType::Cursor => parse_style_cursor(value)?.into(),

            CssPropertyType::Display => parse_layout_display(value)?.into(),
            CssPropertyType::Float => parse_layout_float(value)?.into(),
            CssPropertyType::BoxSizing => parse_layout_box_sizing(value)?.into(),
            CssPropertyType::Width => parse_layout_width(value)?.into(),
            CssPropertyType::Height => parse_layout_height(value)?.into(),
            CssPropertyType::MinWidth => parse_layout_min_width(value)?.into(),
            CssPropertyType::MinHeight => parse_layout_min_height(value)?.into(),
            CssPropertyType::MaxWidth => parse_layout_max_width(value)?.into(),
            CssPropertyType::MaxHeight => parse_layout_max_height(value)?.into(),
            CssPropertyType::Position => parse_layout_position(value)?.into(),
            CssPropertyType::Top => parse_layout_top(value)?.into(),
            CssPropertyType::Right => parse_layout_right(value)?.into(),
            CssPropertyType::Left => parse_layout_left(value)?.into(),
            CssPropertyType::Bottom => parse_layout_bottom(value)?.into(),

            CssPropertyType::FlexWrap => parse_layout_flex_wrap(value)?.into(),
            CssPropertyType::FlexDirection => parse_layout_flex_direction(value)?.into(),
            CssPropertyType::FlexGrow => parse_layout_flex_grow(value)?.into(),
            CssPropertyType::FlexShrink => parse_layout_flex_shrink(value)?.into(),
            CssPropertyType::JustifyContent => parse_layout_justify_content(value)?.into(),
            CssPropertyType::AlignItems => parse_layout_align_items(value)?.into(),
            CssPropertyType::AlignContent => parse_layout_align_content(value)?.into(),

            CssPropertyType::BackgroundContent => {
                parse_style_background_content_multiple(value)?.into()
            }
            CssPropertyType::BackgroundPosition => {
                parse_style_background_position_multiple(value)?.into()
            }
            CssPropertyType::BackgroundSize => parse_style_background_size_multiple(value)?.into(),
            CssPropertyType::BackgroundRepeat => {
                parse_style_background_repeat_multiple(value)?.into()
            }

            CssPropertyType::OverflowX => {
                CssProperty::OverflowX(parse_layout_overflow(value)?.into())
            }
            CssPropertyType::OverflowY => {
                CssProperty::OverflowY(parse_layout_overflow(value)?.into())
            }

            CssPropertyType::PaddingTop => parse_layout_padding_top(value)?.into(),
            CssPropertyType::PaddingLeft => parse_layout_padding_left(value)?.into(),
            CssPropertyType::PaddingRight => parse_layout_padding_right(value)?.into(),
            CssPropertyType::PaddingBottom => parse_layout_padding_bottom(value)?.into(),

            CssPropertyType::MarginTop => parse_layout_margin_top(value)?.into(),
            CssPropertyType::MarginLeft => parse_layout_margin_left(value)?.into(),
            CssPropertyType::MarginRight => parse_layout_margin_right(value)?.into(),
            CssPropertyType::MarginBottom => parse_layout_margin_bottom(value)?.into(),

            CssPropertyType::BorderTopLeftRadius => {
                parse_style_border_top_left_radius(value)?.into()
            }
            CssPropertyType::BorderTopRightRadius => {
                parse_style_border_top_right_radius(value)?.into()
            }
            CssPropertyType::BorderBottomLeftRadius => {
                parse_style_border_bottom_left_radius(value)?.into()
            }
            CssPropertyType::BorderBottomRightRadius => {
                parse_style_border_bottom_right_radius(value)?.into()
            }

            CssPropertyType::BorderTopColor => parse_border_top_color(value)?.into(),
            CssPropertyType::BorderRightColor => parse_border_right_color(value)?.into(),
            CssPropertyType::BorderLeftColor => parse_border_left_color(value)?.into(),
            CssPropertyType::BorderBottomColor => parse_border_bottom_color(value)?.into(),

            CssPropertyType::BorderTopStyle => parse_border_top_style(value)?.into(),
            CssPropertyType::BorderRightStyle => parse_border_right_style(value)?.into(),
            CssPropertyType::BorderLeftStyle => parse_border_left_style(value)?.into(),
            CssPropertyType::BorderBottomStyle => parse_border_bottom_style(value)?.into(),

            CssPropertyType::BorderTopWidth => parse_border_top_width(value)?.into(),
            CssPropertyType::BorderRightWidth => parse_border_right_width(value)?.into(),
            CssPropertyType::BorderLeftWidth => parse_border_left_width(value)?.into(),
            CssPropertyType::BorderBottomWidth => parse_border_bottom_width(value)?.into(),

            CssPropertyType::BoxShadowLeft => {
                CssProperty::BoxShadowLeft(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowRight => {
                CssProperty::BoxShadowRight(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowTop => {
                CssProperty::BoxShadowTop(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowBottom => {
                CssProperty::BoxShadowBottom(parse_style_box_shadow(value)?.into())
            }

            CssPropertyType::ScrollbarStyle => parse_scrollbar_style(value)?.into(),
            CssPropertyType::Opacity => parse_style_opacity(value)?.into(),
            CssPropertyType::Transform => parse_style_transform_vec(value)?.into(),
            CssPropertyType::TransformOrigin => parse_style_transform_origin(value)?.into(),
            CssPropertyType::PerspectiveOrigin => parse_style_perspective_origin(value)?.into(),
            CssPropertyType::BackfaceVisibility => parse_style_backface_visibility(value)?.into(),

            CssPropertyType::MixBlendMode => parse_style_mix_blend_mode(value)?.into(),
            CssPropertyType::Filter => CssProperty::Filter(parse_style_filter_vec(value)?.into()),
            CssPropertyType::BackdropFilter => {
                CssProperty::BackdropFilter(parse_style_filter_vec(value)?.into())
            }
            CssPropertyType::TextShadow => {
                CssProperty::TextShadow(parse_style_box_shadow(value)?.into())
            }
        },
    })
}

// Re-add the From implementations for convenience
macro_rules! impl_from_css_prop {
    ($a:ident, $b:ident:: $enum_type:ident) => {
        impl From<$a> for $b {
            fn from(e: $a) -> Self {
                $b::$enum_type(CssPropertyValue::from(e))
            }
        }
    };
}

impl_from_css_prop!(StyleTextColor, CssProperty::TextColor);
impl_from_css_prop!(StyleFontSize, CssProperty::FontSize);
impl_from_css_prop!(StyleFontFamilyVec, CssProperty::FontFamily);
impl_from_css_prop!(StyleTextAlign, CssProperty::TextAlign);
impl_from_css_prop!(StyleLetterSpacing, CssProperty::LetterSpacing);
impl_from_css_prop!(StyleLineHeight, CssProperty::LineHeight);
impl_from_css_prop!(StyleWordSpacing, CssProperty::WordSpacing);
impl_from_css_prop!(StyleTabWidth, CssProperty::TabWidth);
impl_from_css_prop!(StyleCursor, CssProperty::Cursor);
impl_from_css_prop!(LayoutDisplay, CssProperty::Display);
impl_from_css_prop!(LayoutFloat, CssProperty::Float);
impl_from_css_prop!(LayoutBoxSizing, CssProperty::BoxSizing);
impl_from_css_prop!(LayoutWidth, CssProperty::Width);
impl_from_css_prop!(LayoutHeight, CssProperty::Height);
impl_from_css_prop!(LayoutMinWidth, CssProperty::MinWidth);
impl_from_css_prop!(LayoutMinHeight, CssProperty::MinHeight);
impl_from_css_prop!(LayoutMaxWidth, CssProperty::MaxWidth);
impl_from_css_prop!(LayoutMaxHeight, CssProperty::MaxHeight);
impl_from_css_prop!(LayoutPosition, CssProperty::Position);
impl_from_css_prop!(LayoutTop, CssProperty::Top);
impl_from_css_prop!(LayoutRight, CssProperty::Right);
impl_from_css_prop!(LayoutLeft, CssProperty::Left);
impl_from_css_prop!(LayoutBottom, CssProperty::Bottom);
impl_from_css_prop!(LayoutFlexWrap, CssProperty::FlexWrap);
impl_from_css_prop!(LayoutFlexDirection, CssProperty::FlexDirection);
impl_from_css_prop!(LayoutFlexGrow, CssProperty::FlexGrow);
impl_from_css_prop!(LayoutFlexShrink, CssProperty::FlexShrink);
impl_from_css_prop!(LayoutJustifyContent, CssProperty::JustifyContent);
impl_from_css_prop!(LayoutAlignItems, CssProperty::AlignItems);
impl_from_css_prop!(LayoutAlignContent, CssProperty::AlignContent);
impl_from_css_prop!(StyleBackgroundContentVec, CssProperty::BackgroundContent);
impl_from_css_prop!(StyleBackgroundPositionVec, CssProperty::BackgroundPosition);
impl_from_css_prop!(StyleBackgroundSizeVec, CssProperty::BackgroundSize);
impl_from_css_prop!(StyleBackgroundRepeatVec, CssProperty::BackgroundRepeat);
impl_from_css_prop!(LayoutPaddingTop, CssProperty::PaddingTop);
impl_from_css_prop!(LayoutPaddingLeft, CssProperty::PaddingLeft);
impl_from_css_prop!(LayoutPaddingRight, CssProperty::PaddingRight);
impl_from_css_prop!(LayoutPaddingBottom, CssProperty::PaddingBottom);
impl_from_css_prop!(LayoutMarginTop, CssProperty::MarginTop);
impl_from_css_prop!(LayoutMarginLeft, CssProperty::MarginLeft);
impl_from_css_prop!(LayoutMarginRight, CssProperty::MarginRight);
impl_from_css_prop!(LayoutMarginBottom, CssProperty::MarginBottom);
impl_from_css_prop!(StyleBorderTopLeftRadius, CssProperty::BorderTopLeftRadius);
impl_from_css_prop!(StyleBorderTopRightRadius, CssProperty::BorderTopRightRadius);
impl_from_css_prop!(
    StyleBorderBottomLeftRadius,
    CssProperty::BorderBottomLeftRadius
);
impl_from_css_prop!(
    StyleBorderBottomRightRadius,
    CssProperty::BorderBottomRightRadius
);
impl_from_css_prop!(StyleBorderTopColor, CssProperty::BorderTopColor);
impl_from_css_prop!(StyleBorderRightColor, CssProperty::BorderRightColor);
impl_from_css_prop!(StyleBorderLeftColor, CssProperty::BorderLeftColor);
impl_from_css_prop!(StyleBorderBottomColor, CssProperty::BorderBottomColor);
impl_from_css_prop!(StyleBorderTopStyle, CssProperty::BorderTopStyle);
impl_from_css_prop!(StyleBorderRightStyle, CssProperty::BorderRightStyle);
impl_from_css_prop!(StyleBorderLeftStyle, CssProperty::BorderLeftStyle);
impl_from_css_prop!(StyleBorderBottomStyle, CssProperty::BorderBottomStyle);
impl_from_css_prop!(LayoutBorderTopWidth, CssProperty::BorderTopWidth);
impl_from_css_prop!(LayoutBorderRightWidth, CssProperty::BorderRightWidth);
impl_from_css_prop!(LayoutBorderLeftWidth, CssProperty::BorderLeftWidth);
impl_from_css_prop!(LayoutBorderBottomWidth, CssProperty::BorderBottomWidth);
impl_from_css_prop!(ScrollbarStyle, CssProperty::ScrollbarStyle);
impl_from_css_prop!(StyleOpacity, CssProperty::Opacity);
impl_from_css_prop!(StyleTransformVec, CssProperty::Transform);
impl_from_css_prop!(StyleTransformOrigin, CssProperty::TransformOrigin);
impl_from_css_prop!(StylePerspectiveOrigin, CssProperty::PerspectiveOrigin);
impl_from_css_prop!(StyleBackfaceVisibility, CssProperty::BackfaceVisibility);
impl_from_css_prop!(StyleMixBlendMode, CssProperty::MixBlendMode);
impl_from_css_prop!(StyleHyphens, CssProperty::Hyphens);
impl_from_css_prop!(StyleDirection, CssProperty::Direction);
impl_from_css_prop!(StyleWhiteSpace, CssProperty::WhiteSpace);

impl CssProperty {
    pub fn key(&self) -> &'static str {
        self.get_type().to_str()
    }

    pub fn value(&self) -> String {
        match self {
            CssProperty::TextColor(v) => v.get_css_value_fmt(),
            CssProperty::FontSize(v) => v.get_css_value_fmt(),
            CssProperty::FontFamily(v) => v.get_css_value_fmt(),
            CssProperty::TextAlign(v) => v.get_css_value_fmt(),
            CssProperty::LetterSpacing(v) => v.get_css_value_fmt(),
            CssProperty::LineHeight(v) => v.get_css_value_fmt(),
            CssProperty::WordSpacing(v) => v.get_css_value_fmt(),
            CssProperty::TabWidth(v) => v.get_css_value_fmt(),
            CssProperty::Cursor(v) => v.get_css_value_fmt(),
            CssProperty::Display(v) => v.get_css_value_fmt(),
            CssProperty::Float(v) => v.get_css_value_fmt(),
            CssProperty::BoxSizing(v) => v.get_css_value_fmt(),
            CssProperty::Width(v) => v.get_css_value_fmt(),
            CssProperty::Height(v) => v.get_css_value_fmt(),
            CssProperty::MinWidth(v) => v.get_css_value_fmt(),
            CssProperty::MinHeight(v) => v.get_css_value_fmt(),
            CssProperty::MaxWidth(v) => v.get_css_value_fmt(),
            CssProperty::MaxHeight(v) => v.get_css_value_fmt(),
            CssProperty::Position(v) => v.get_css_value_fmt(),
            CssProperty::Top(v) => v.get_css_value_fmt(),
            CssProperty::Right(v) => v.get_css_value_fmt(),
            CssProperty::Left(v) => v.get_css_value_fmt(),
            CssProperty::Bottom(v) => v.get_css_value_fmt(),
            CssProperty::FlexWrap(v) => v.get_css_value_fmt(),
            CssProperty::FlexDirection(v) => v.get_css_value_fmt(),
            CssProperty::FlexGrow(v) => v.get_css_value_fmt(),
            CssProperty::FlexShrink(v) => v.get_css_value_fmt(),
            CssProperty::JustifyContent(v) => v.get_css_value_fmt(),
            CssProperty::AlignItems(v) => v.get_css_value_fmt(),
            CssProperty::AlignContent(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundContent(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundPosition(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundSize(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundRepeat(v) => v.get_css_value_fmt(),
            CssProperty::OverflowX(v) => v.get_css_value_fmt(),
            CssProperty::OverflowY(v) => v.get_css_value_fmt(),
            CssProperty::PaddingTop(v) => v.get_css_value_fmt(),
            CssProperty::PaddingLeft(v) => v.get_css_value_fmt(),
            CssProperty::PaddingRight(v) => v.get_css_value_fmt(),
            CssProperty::PaddingBottom(v) => v.get_css_value_fmt(),
            CssProperty::MarginTop(v) => v.get_css_value_fmt(),
            CssProperty::MarginLeft(v) => v.get_css_value_fmt(),
            CssProperty::MarginRight(v) => v.get_css_value_fmt(),
            CssProperty::MarginBottom(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopLeftRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopRightRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomLeftRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomRightRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomWidth(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowLeft(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowRight(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowTop(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowBottom(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarStyle(v) => v.get_css_value_fmt(),
            CssProperty::Opacity(v) => v.get_css_value_fmt(),
            CssProperty::Transform(v) => v.get_css_value_fmt(),
            CssProperty::TransformOrigin(v) => v.get_css_value_fmt(),
            CssProperty::PerspectiveOrigin(v) => v.get_css_value_fmt(),
            CssProperty::BackfaceVisibility(v) => v.get_css_value_fmt(),
            CssProperty::MixBlendMode(v) => v.get_css_value_fmt(),
            CssProperty::Filter(v) => v.get_css_value_fmt(),
            CssProperty::BackdropFilter(v) => v.get_css_value_fmt(),
            CssProperty::TextShadow(v) => v.get_css_value_fmt(),
            CssProperty::Hyphens(v) => v.get_css_value_fmt(),
            CssProperty::Direction(v) => v.get_css_value_fmt(),
            CssProperty::WhiteSpace(v) => v.get_css_value_fmt(),
        }
    }

    pub fn format_css(&self) -> String {
        format!("{}: {};", self.key(), self.value())
    }

    pub fn interpolate(
        &self,
        other: &Self,
        t: f32,
        interpolate_resolver: &InterpolateResolver,
    ) -> Self {
        if t <= 0.0 {
            return self.clone();
        } else if t >= 1.0 {
            return other.clone();
        }

        // Map from linear interpolation function to Easing curve
        let t: f32 = interpolate_resolver.interpolate_func.evaluate(t as f64);

        let t = t.max(0.0).min(1.0);

        match (self, other) {
            (CssProperty::TextColor(col_start), CssProperty::TextColor(col_end)) => {
                let col_start = col_start.get_property().copied().unwrap_or_default();
                let col_end = col_end.get_property().copied().unwrap_or_default();
                CssProperty::text_color(col_start.interpolate(&col_end, t))
            }
            (CssProperty::FontSize(fs_start), CssProperty::FontSize(fs_end)) => {
                let fs_start = fs_start.get_property().copied().unwrap_or_default();
                let fs_end = fs_end.get_property().copied().unwrap_or_default();
                CssProperty::font_size(fs_start.interpolate(&fs_end, t))
            }
            (CssProperty::LetterSpacing(ls_start), CssProperty::LetterSpacing(ls_end)) => {
                let ls_start = ls_start.get_property().copied().unwrap_or_default();
                let ls_end = ls_end.get_property().copied().unwrap_or_default();
                CssProperty::letter_spacing(ls_start.interpolate(&ls_end, t))
            }
            (CssProperty::LineHeight(lh_start), CssProperty::LineHeight(lh_end)) => {
                let lh_start = lh_start.get_property().copied().unwrap_or_default();
                let lh_end = lh_end.get_property().copied().unwrap_or_default();
                CssProperty::line_height(lh_start.interpolate(&lh_end, t))
            }
            (CssProperty::WordSpacing(ws_start), CssProperty::WordSpacing(ws_end)) => {
                let ws_start = ws_start.get_property().copied().unwrap_or_default();
                let ws_end = ws_end.get_property().copied().unwrap_or_default();
                CssProperty::word_spacing(ws_start.interpolate(&ws_end, t))
            }
            (CssProperty::TabWidth(tw_start), CssProperty::TabWidth(tw_end)) => {
                let tw_start = tw_start.get_property().copied().unwrap_or_default();
                let tw_end = tw_end.get_property().copied().unwrap_or_default();
                CssProperty::tab_width(tw_start.interpolate(&tw_end, t))
            }
            (CssProperty::Width(start), CssProperty::Width(end)) => {
                let start = start
                    .get_property()
                    .copied()
                    .unwrap_or(LayoutWidth::px(interpolate_resolver.current_rect_width));
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Width(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Height(start), CssProperty::Height(end)) => {
                let start = start
                    .get_property()
                    .copied()
                    .unwrap_or(LayoutHeight::px(interpolate_resolver.current_rect_height));
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Height(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MinWidth(start), CssProperty::MinWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MinWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MinHeight(start), CssProperty::MinHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MinHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MaxWidth(start), CssProperty::MaxWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MaxWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MaxHeight(start), CssProperty::MaxHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MaxHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Top(start), CssProperty::Top(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Top(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Right(start), CssProperty::Right(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Right(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Left(start), CssProperty::Left(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Left(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Bottom(start), CssProperty::Bottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Bottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::FlexGrow(start), CssProperty::FlexGrow(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::FlexGrow(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::FlexShrink(start), CssProperty::FlexShrink(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::FlexShrink(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingTop(start), CssProperty::PaddingTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingLeft(start), CssProperty::PaddingLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingRight(start), CssProperty::PaddingRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingBottom(start), CssProperty::PaddingBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginTop(start), CssProperty::MarginTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginLeft(start), CssProperty::MarginLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginRight(start), CssProperty::MarginRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginBottom(start), CssProperty::MarginBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderTopLeftRadius(start), CssProperty::BorderTopLeftRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (CssProperty::BorderTopRightRadius(start), CssProperty::BorderTopRightRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                CssProperty::BorderBottomLeftRadius(start),
                CssProperty::BorderBottomLeftRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                CssProperty::BorderBottomRightRadius(start),
                CssProperty::BorderBottomRightRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (CssProperty::BorderTopColor(start), CssProperty::BorderTopColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderRightColor(start), CssProperty::BorderRightColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderRightColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderLeftColor(start), CssProperty::BorderLeftColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderLeftColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderBottomColor(start), CssProperty::BorderBottomColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderTopWidth(start), CssProperty::BorderTopWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderRightWidth(start), CssProperty::BorderRightWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderRightWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderLeftWidth(start), CssProperty::BorderLeftWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderLeftWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderBottomWidth(start), CssProperty::BorderBottomWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Opacity(start), CssProperty::Opacity(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Opacity(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::TransformOrigin(start), CssProperty::TransformOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::TransformOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PerspectiveOrigin(start), CssProperty::PerspectiveOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PerspectiveOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            /*
            animate transform:
            CssProperty::Transform(CssPropertyValue<StyleTransformVec>),

            animate box shadow:
            CssProperty::BoxShadowLeft(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowRight(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowTop(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowBottom(CssPropertyValue<StyleBoxShadow>),

            animate background:
            CssProperty::BackgroundContent(CssPropertyValue<StyleBackgroundContentVec>),
            CssProperty::BackgroundPosition(CssPropertyValue<StyleBackgroundPositionVec>),
            CssProperty::BackgroundSize(CssPropertyValue<StyleBackgroundSizeVec>),
            */
            (_, _) => {
                // not animatable, fallback
                if t > 0.5 {
                    other.clone()
                } else {
                    self.clone()
                }
            }
        }
    }

    /// Return the type (key) of this property as a statically typed enum
    pub const fn get_type(&self) -> CssPropertyType {
        match &self {
            CssProperty::TextColor(_) => CssPropertyType::TextColor,
            CssProperty::FontSize(_) => CssPropertyType::FontSize,
            CssProperty::FontFamily(_) => CssPropertyType::FontFamily,
            CssProperty::TextAlign(_) => CssPropertyType::TextAlign,
            CssProperty::LetterSpacing(_) => CssPropertyType::LetterSpacing,
            CssProperty::LineHeight(_) => CssPropertyType::LineHeight,
            CssProperty::WordSpacing(_) => CssPropertyType::WordSpacing,
            CssProperty::TabWidth(_) => CssPropertyType::TabWidth,
            CssProperty::Cursor(_) => CssPropertyType::Cursor,
            CssProperty::Display(_) => CssPropertyType::Display,
            CssProperty::Float(_) => CssPropertyType::Float,
            CssProperty::BoxSizing(_) => CssPropertyType::BoxSizing,
            CssProperty::Width(_) => CssPropertyType::Width,
            CssProperty::Height(_) => CssPropertyType::Height,
            CssProperty::MinWidth(_) => CssPropertyType::MinWidth,
            CssProperty::MinHeight(_) => CssPropertyType::MinHeight,
            CssProperty::MaxWidth(_) => CssPropertyType::MaxWidth,
            CssProperty::MaxHeight(_) => CssPropertyType::MaxHeight,
            CssProperty::Position(_) => CssPropertyType::Position,
            CssProperty::Top(_) => CssPropertyType::Top,
            CssProperty::Right(_) => CssPropertyType::Right,
            CssProperty::Left(_) => CssPropertyType::Left,
            CssProperty::Bottom(_) => CssPropertyType::Bottom,
            CssProperty::FlexWrap(_) => CssPropertyType::FlexWrap,
            CssProperty::FlexDirection(_) => CssPropertyType::FlexDirection,
            CssProperty::FlexGrow(_) => CssPropertyType::FlexGrow,
            CssProperty::FlexShrink(_) => CssPropertyType::FlexShrink,
            CssProperty::JustifyContent(_) => CssPropertyType::JustifyContent,
            CssProperty::AlignItems(_) => CssPropertyType::AlignItems,
            CssProperty::AlignContent(_) => CssPropertyType::AlignContent,
            CssProperty::BackgroundContent(_) => CssPropertyType::BackgroundContent,
            CssProperty::BackgroundPosition(_) => CssPropertyType::BackgroundPosition,
            CssProperty::BackgroundSize(_) => CssPropertyType::BackgroundSize,
            CssProperty::BackgroundRepeat(_) => CssPropertyType::BackgroundRepeat,
            CssProperty::OverflowX(_) => CssPropertyType::OverflowX,
            CssProperty::OverflowY(_) => CssPropertyType::OverflowY,
            CssProperty::PaddingTop(_) => CssPropertyType::PaddingTop,
            CssProperty::PaddingLeft(_) => CssPropertyType::PaddingLeft,
            CssProperty::PaddingRight(_) => CssPropertyType::PaddingRight,
            CssProperty::PaddingBottom(_) => CssPropertyType::PaddingBottom,
            CssProperty::MarginTop(_) => CssPropertyType::MarginTop,
            CssProperty::MarginLeft(_) => CssPropertyType::MarginLeft,
            CssProperty::MarginRight(_) => CssPropertyType::MarginRight,
            CssProperty::MarginBottom(_) => CssPropertyType::MarginBottom,
            CssProperty::BorderTopLeftRadius(_) => CssPropertyType::BorderTopLeftRadius,
            CssProperty::BorderTopRightRadius(_) => CssPropertyType::BorderTopRightRadius,
            CssProperty::BorderBottomLeftRadius(_) => CssPropertyType::BorderBottomLeftRadius,
            CssProperty::BorderBottomRightRadius(_) => CssPropertyType::BorderBottomRightRadius,
            CssProperty::BorderTopColor(_) => CssPropertyType::BorderTopColor,
            CssProperty::BorderRightColor(_) => CssPropertyType::BorderRightColor,
            CssProperty::BorderLeftColor(_) => CssPropertyType::BorderLeftColor,
            CssProperty::BorderBottomColor(_) => CssPropertyType::BorderBottomColor,
            CssProperty::BorderTopStyle(_) => CssPropertyType::BorderTopStyle,
            CssProperty::BorderRightStyle(_) => CssPropertyType::BorderRightStyle,
            CssProperty::BorderLeftStyle(_) => CssPropertyType::BorderLeftStyle,
            CssProperty::BorderBottomStyle(_) => CssPropertyType::BorderBottomStyle,
            CssProperty::BorderTopWidth(_) => CssPropertyType::BorderTopWidth,
            CssProperty::BorderRightWidth(_) => CssPropertyType::BorderRightWidth,
            CssProperty::BorderLeftWidth(_) => CssPropertyType::BorderLeftWidth,
            CssProperty::BorderBottomWidth(_) => CssPropertyType::BorderBottomWidth,
            CssProperty::BoxShadowLeft(_) => CssPropertyType::BoxShadowLeft,
            CssProperty::BoxShadowRight(_) => CssPropertyType::BoxShadowRight,
            CssProperty::BoxShadowTop(_) => CssPropertyType::BoxShadowTop,
            CssProperty::BoxShadowBottom(_) => CssPropertyType::BoxShadowBottom,
            CssProperty::ScrollbarStyle(_) => CssPropertyType::ScrollbarStyle,
            CssProperty::Opacity(_) => CssPropertyType::Opacity,
            CssProperty::Transform(_) => CssPropertyType::Transform,
            CssProperty::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
            CssProperty::TransformOrigin(_) => CssPropertyType::TransformOrigin,
            CssProperty::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
            CssProperty::MixBlendMode(_) => CssPropertyType::MixBlendMode,
            CssProperty::Filter(_) => CssPropertyType::Filter,
            CssProperty::BackdropFilter(_) => CssPropertyType::BackdropFilter,
            CssProperty::TextShadow(_) => CssPropertyType::TextShadow,
            CssProperty::WhiteSpace(_) => CssPropertyType::WhiteSpace,
            CssProperty::Hyphens(_) => CssPropertyType::Hyphens,
            CssProperty::Direction(_) => CssPropertyType::Direction,
        }
    }

    // const constructors for easier API access

    pub const fn none(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, None)
    }
    pub const fn auto(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Auto)
    }
    pub const fn initial(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Initial)
    }
    pub const fn inherit(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Inherit)
    }

    pub const fn text_color(input: StyleTextColor) -> Self {
        CssProperty::TextColor(CssPropertyValue::Exact(input))
    }
    pub const fn font_size(input: StyleFontSize) -> Self {
        CssProperty::FontSize(CssPropertyValue::Exact(input))
    }
    pub const fn font_family(input: StyleFontFamilyVec) -> Self {
        CssProperty::FontFamily(CssPropertyValue::Exact(input))
    }
    pub const fn text_align(input: StyleTextAlign) -> Self {
        CssProperty::TextAlign(CssPropertyValue::Exact(input))
    }
    pub const fn letter_spacing(input: StyleLetterSpacing) -> Self {
        CssProperty::LetterSpacing(CssPropertyValue::Exact(input))
    }
    pub const fn line_height(input: StyleLineHeight) -> Self {
        CssProperty::LineHeight(CssPropertyValue::Exact(input))
    }
    pub const fn word_spacing(input: StyleWordSpacing) -> Self {
        CssProperty::WordSpacing(CssPropertyValue::Exact(input))
    }
    pub const fn tab_width(input: StyleTabWidth) -> Self {
        CssProperty::TabWidth(CssPropertyValue::Exact(input))
    }
    pub const fn cursor(input: StyleCursor) -> Self {
        CssProperty::Cursor(CssPropertyValue::Exact(input))
    }
    pub const fn display(input: LayoutDisplay) -> Self {
        CssProperty::Display(CssPropertyValue::Exact(input))
    }
    pub const fn float(input: LayoutFloat) -> Self {
        CssProperty::Float(CssPropertyValue::Exact(input))
    }
    pub const fn box_sizing(input: LayoutBoxSizing) -> Self {
        CssProperty::BoxSizing(CssPropertyValue::Exact(input))
    }
    pub const fn width(input: LayoutWidth) -> Self {
        CssProperty::Width(CssPropertyValue::Exact(input))
    }
    pub const fn height(input: LayoutHeight) -> Self {
        CssProperty::Height(CssPropertyValue::Exact(input))
    }
    pub const fn min_width(input: LayoutMinWidth) -> Self {
        CssProperty::MinWidth(CssPropertyValue::Exact(input))
    }
    pub const fn min_height(input: LayoutMinHeight) -> Self {
        CssProperty::MinHeight(CssPropertyValue::Exact(input))
    }
    pub const fn max_width(input: LayoutMaxWidth) -> Self {
        CssProperty::MaxWidth(CssPropertyValue::Exact(input))
    }
    pub const fn max_height(input: LayoutMaxHeight) -> Self {
        CssProperty::MaxHeight(CssPropertyValue::Exact(input))
    }
    pub const fn position(input: LayoutPosition) -> Self {
        CssProperty::Position(CssPropertyValue::Exact(input))
    }
    pub const fn top(input: LayoutTop) -> Self {
        CssProperty::Top(CssPropertyValue::Exact(input))
    }
    pub const fn right(input: LayoutRight) -> Self {
        CssProperty::Right(CssPropertyValue::Exact(input))
    }
    pub const fn left(input: LayoutLeft) -> Self {
        CssProperty::Left(CssPropertyValue::Exact(input))
    }
    pub const fn bottom(input: LayoutBottom) -> Self {
        CssProperty::Bottom(CssPropertyValue::Exact(input))
    }
    pub const fn flex_wrap(input: LayoutFlexWrap) -> Self {
        CssProperty::FlexWrap(CssPropertyValue::Exact(input))
    }
    pub const fn flex_direction(input: LayoutFlexDirection) -> Self {
        CssProperty::FlexDirection(CssPropertyValue::Exact(input))
    }
    pub const fn flex_grow(input: LayoutFlexGrow) -> Self {
        CssProperty::FlexGrow(CssPropertyValue::Exact(input))
    }
    pub const fn flex_shrink(input: LayoutFlexShrink) -> Self {
        CssProperty::FlexShrink(CssPropertyValue::Exact(input))
    }
    pub const fn justify_content(input: LayoutJustifyContent) -> Self {
        CssProperty::JustifyContent(CssPropertyValue::Exact(input))
    }
    pub const fn align_items(input: LayoutAlignItems) -> Self {
        CssProperty::AlignItems(CssPropertyValue::Exact(input))
    }
    pub const fn align_content(input: LayoutAlignContent) -> Self {
        CssProperty::AlignContent(CssPropertyValue::Exact(input))
    }
    pub const fn background_content(input: StyleBackgroundContentVec) -> Self {
        CssProperty::BackgroundContent(CssPropertyValue::Exact(input))
    }
    pub const fn background_position(input: StyleBackgroundPositionVec) -> Self {
        CssProperty::BackgroundPosition(CssPropertyValue::Exact(input))
    }
    pub const fn background_size(input: StyleBackgroundSizeVec) -> Self {
        CssProperty::BackgroundSize(CssPropertyValue::Exact(input))
    }
    pub const fn background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        CssProperty::BackgroundRepeat(CssPropertyValue::Exact(input))
    }
    pub const fn overflow_x(input: LayoutOverflow) -> Self {
        CssProperty::OverflowX(CssPropertyValue::Exact(input))
    }
    pub const fn overflow_y(input: LayoutOverflow) -> Self {
        CssProperty::OverflowY(CssPropertyValue::Exact(input))
    }
    pub const fn padding_top(input: LayoutPaddingTop) -> Self {
        CssProperty::PaddingTop(CssPropertyValue::Exact(input))
    }
    pub const fn padding_left(input: LayoutPaddingLeft) -> Self {
        CssProperty::PaddingLeft(CssPropertyValue::Exact(input))
    }
    pub const fn padding_right(input: LayoutPaddingRight) -> Self {
        CssProperty::PaddingRight(CssPropertyValue::Exact(input))
    }
    pub const fn padding_bottom(input: LayoutPaddingBottom) -> Self {
        CssProperty::PaddingBottom(CssPropertyValue::Exact(input))
    }
    pub const fn margin_top(input: LayoutMarginTop) -> Self {
        CssProperty::MarginTop(CssPropertyValue::Exact(input))
    }
    pub const fn margin_left(input: LayoutMarginLeft) -> Self {
        CssProperty::MarginLeft(CssPropertyValue::Exact(input))
    }
    pub const fn margin_right(input: LayoutMarginRight) -> Self {
        CssProperty::MarginRight(CssPropertyValue::Exact(input))
    }
    pub const fn margin_bottom(input: LayoutMarginBottom) -> Self {
        CssProperty::MarginBottom(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_color(input: StyleBorderTopColor) -> Self {
        CssProperty::BorderTopColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_color(input: StyleBorderRightColor) -> Self {
        CssProperty::BorderRightColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_color(input: StyleBorderLeftColor) -> Self {
        CssProperty::BorderLeftColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_color(input: StyleBorderBottomColor) -> Self {
        CssProperty::BorderBottomColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_style(input: StyleBorderTopStyle) -> Self {
        CssProperty::BorderTopStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_style(input: StyleBorderRightStyle) -> Self {
        CssProperty::BorderRightStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_style(input: StyleBorderLeftStyle) -> Self {
        CssProperty::BorderLeftStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        CssProperty::BorderBottomStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_width(input: LayoutBorderTopWidth) -> Self {
        CssProperty::BorderTopWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_width(input: LayoutBorderRightWidth) -> Self {
        CssProperty::BorderRightWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_width(input: LayoutBorderLeftWidth) -> Self {
        CssProperty::BorderLeftWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        CssProperty::BorderBottomWidth(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_left(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowLeft(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_right(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowRight(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_top(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowTop(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_bottom(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowBottom(CssPropertyValue::Exact(input))
    }
    pub const fn opacity(input: StyleOpacity) -> Self {
        CssProperty::Opacity(CssPropertyValue::Exact(input))
    }
    pub const fn transform(input: StyleTransformVec) -> Self {
        CssProperty::Transform(CssPropertyValue::Exact(input))
    }
    pub const fn transform_origin(input: StyleTransformOrigin) -> Self {
        CssProperty::TransformOrigin(CssPropertyValue::Exact(input))
    }
    pub const fn perspective_origin(input: StylePerspectiveOrigin) -> Self {
        CssProperty::PerspectiveOrigin(CssPropertyValue::Exact(input))
    }
    pub const fn backface_visiblity(input: StyleBackfaceVisibility) -> Self {
        CssProperty::BackfaceVisibility(CssPropertyValue::Exact(input))
    }

    // functions that downcast to the concrete CSS type (style)

    pub const fn as_background_content(&self) -> Option<&StyleBackgroundContentVecValue> {
        match self {
            CssProperty::BackgroundContent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_position(&self) -> Option<&StyleBackgroundPositionVecValue> {
        match self {
            CssProperty::BackgroundPosition(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_size(&self) -> Option<&StyleBackgroundSizeVecValue> {
        match self {
            CssProperty::BackgroundSize(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_repeat(&self) -> Option<&StyleBackgroundRepeatVecValue> {
        match self {
            CssProperty::BackgroundRepeat(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_size(&self) -> Option<&StyleFontSizeValue> {
        match self {
            CssProperty::FontSize(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_family(&self) -> Option<&StyleFontFamilyVecValue> {
        match self {
            CssProperty::FontFamily(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_color(&self) -> Option<&StyleTextColorValue> {
        match self {
            CssProperty::TextColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_align(&self) -> Option<&StyleTextAlignValue> {
        match self {
            CssProperty::TextAlign(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_line_height(&self) -> Option<&StyleLineHeightValue> {
        match self {
            CssProperty::LineHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_letter_spacing(&self) -> Option<&StyleLetterSpacingValue> {
        match self {
            CssProperty::LetterSpacing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_word_spacing(&self) -> Option<&StyleWordSpacingValue> {
        match self {
            CssProperty::WordSpacing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_tab_width(&self) -> Option<&StyleTabWidthValue> {
        match self {
            CssProperty::TabWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_cursor(&self) -> Option<&StyleCursorValue> {
        match self {
            CssProperty::Cursor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_left(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_right(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_top(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_bottom(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_color(&self) -> Option<&StyleBorderTopColorValue> {
        match self {
            CssProperty::BorderTopColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_color(&self) -> Option<&StyleBorderLeftColorValue> {
        match self {
            CssProperty::BorderLeftColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_color(&self) -> Option<&StyleBorderRightColorValue> {
        match self {
            CssProperty::BorderRightColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_color(&self) -> Option<&StyleBorderBottomColorValue> {
        match self {
            CssProperty::BorderBottomColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_style(&self) -> Option<&StyleBorderTopStyleValue> {
        match self {
            CssProperty::BorderTopStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_style(&self) -> Option<&StyleBorderLeftStyleValue> {
        match self {
            CssProperty::BorderLeftStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_style(&self) -> Option<&StyleBorderRightStyleValue> {
        match self {
            CssProperty::BorderRightStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_style(&self) -> Option<&StyleBorderBottomStyleValue> {
        match self {
            CssProperty::BorderBottomStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_left_radius(&self) -> Option<&StyleBorderTopLeftRadiusValue> {
        match self {
            CssProperty::BorderTopLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_right_radius(&self) -> Option<&StyleBorderTopRightRadiusValue> {
        match self {
            CssProperty::BorderTopRightRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_left_radius(&self) -> Option<&StyleBorderBottomLeftRadiusValue> {
        match self {
            CssProperty::BorderBottomLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_right_radius(
        &self,
    ) -> Option<&StyleBorderBottomRightRadiusValue> {
        match self {
            CssProperty::BorderBottomRightRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_opacity(&self) -> Option<&StyleOpacityValue> {
        match self {
            CssProperty::Opacity(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_transform(&self) -> Option<&StyleTransformVecValue> {
        match self {
            CssProperty::Transform(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_transform_origin(&self) -> Option<&StyleTransformOriginValue> {
        match self {
            CssProperty::TransformOrigin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_perspective_origin(&self) -> Option<&StylePerspectiveOriginValue> {
        match self {
            CssProperty::PerspectiveOrigin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_backface_visibility(&self) -> Option<&StyleBackfaceVisibilityValue> {
        match self {
            CssProperty::BackfaceVisibility(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_mix_blend_mode(&self) -> Option<&StyleMixBlendModeValue> {
        match self {
            CssProperty::MixBlendMode(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            CssProperty::Filter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_backdrop_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            CssProperty::BackdropFilter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_shadow(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::TextShadow(f) => Some(f),
            _ => None,
        }
    }

    // functions that downcast to the concrete CSS type (layout)

    pub const fn as_display(&self) -> Option<&LayoutDisplayValue> {
        match self {
            CssProperty::Display(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_float(&self) -> Option<&LayoutFloatValue> {
        match self {
            CssProperty::Float(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_sizing(&self) -> Option<&LayoutBoxSizingValue> {
        match self {
            CssProperty::BoxSizing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_width(&self) -> Option<&LayoutWidthValue> {
        match self {
            CssProperty::Width(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_height(&self) -> Option<&LayoutHeightValue> {
        match self {
            CssProperty::Height(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_min_width(&self) -> Option<&LayoutMinWidthValue> {
        match self {
            CssProperty::MinWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_min_height(&self) -> Option<&LayoutMinHeightValue> {
        match self {
            CssProperty::MinHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_max_width(&self) -> Option<&LayoutMaxWidthValue> {
        match self {
            CssProperty::MaxWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_max_height(&self) -> Option<&LayoutMaxHeightValue> {
        match self {
            CssProperty::MaxHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_position(&self) -> Option<&LayoutPositionValue> {
        match self {
            CssProperty::Position(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_top(&self) -> Option<&LayoutTopValue> {
        match self {
            CssProperty::Top(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_bottom(&self) -> Option<&LayoutBottomValue> {
        match self {
            CssProperty::Bottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_right(&self) -> Option<&LayoutRightValue> {
        match self {
            CssProperty::Right(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_left(&self) -> Option<&LayoutLeftValue> {
        match self {
            CssProperty::Left(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_top(&self) -> Option<&LayoutPaddingTopValue> {
        match self {
            CssProperty::PaddingTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_bottom(&self) -> Option<&LayoutPaddingBottomValue> {
        match self {
            CssProperty::PaddingBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_left(&self) -> Option<&LayoutPaddingLeftValue> {
        match self {
            CssProperty::PaddingLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_right(&self) -> Option<&LayoutPaddingRightValue> {
        match self {
            CssProperty::PaddingRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_top(&self) -> Option<&LayoutMarginTopValue> {
        match self {
            CssProperty::MarginTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_bottom(&self) -> Option<&LayoutMarginBottomValue> {
        match self {
            CssProperty::MarginBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_left(&self) -> Option<&LayoutMarginLeftValue> {
        match self {
            CssProperty::MarginLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_right(&self) -> Option<&LayoutMarginRightValue> {
        match self {
            CssProperty::MarginRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_width(&self) -> Option<&LayoutBorderTopWidthValue> {
        match self {
            CssProperty::BorderTopWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_width(&self) -> Option<&LayoutBorderLeftWidthValue> {
        match self {
            CssProperty::BorderLeftWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_width(&self) -> Option<&LayoutBorderRightWidthValue> {
        match self {
            CssProperty::BorderRightWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_width(&self) -> Option<&LayoutBorderBottomWidthValue> {
        match self {
            CssProperty::BorderBottomWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_overflow_x(&self) -> Option<&LayoutOverflowValue> {
        match self {
            CssProperty::OverflowX(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_overflow_y(&self) -> Option<&LayoutOverflowValue> {
        match self {
            CssProperty::OverflowY(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_direction(&self) -> Option<&LayoutFlexDirectionValue> {
        match self {
            CssProperty::FlexDirection(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_direction(&self) -> Option<&StyleDirectionValue> {
        match self {
            CssProperty::Direction(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_hyphens(&self) -> Option<&StyleHyphensValue> {
        match self {
            CssProperty::Hyphens(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_white_space(&self) -> Option<&StyleWhiteSpaceValue> {
        match self {
            CssProperty::WhiteSpace(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_wrap(&self) -> Option<&LayoutFlexWrapValue> {
        match self {
            CssProperty::FlexWrap(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_grow(&self) -> Option<&LayoutFlexGrowValue> {
        match self {
            CssProperty::FlexGrow(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_shrink(&self) -> Option<&LayoutFlexShrinkValue> {
        match self {
            CssProperty::FlexShrink(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_justify_content(&self) -> Option<&LayoutJustifyContentValue> {
        match self {
            CssProperty::JustifyContent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_align_items(&self) -> Option<&LayoutAlignItemsValue> {
        match self {
            CssProperty::AlignItems(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_align_content(&self) -> Option<&LayoutAlignContentValue> {
        match self {
            CssProperty::AlignContent(f) => Some(f),
            _ => None,
        }
    }

    pub fn is_initial(&self) -> bool {
        use self::CssProperty::*;
        match self {
            TextColor(c) => c.is_initial(),
            FontSize(c) => c.is_initial(),
            FontFamily(c) => c.is_initial(),
            TextAlign(c) => c.is_initial(),
            LetterSpacing(c) => c.is_initial(),
            LineHeight(c) => c.is_initial(),
            WordSpacing(c) => c.is_initial(),
            TabWidth(c) => c.is_initial(),
            Cursor(c) => c.is_initial(),
            Display(c) => c.is_initial(),
            Float(c) => c.is_initial(),
            BoxSizing(c) => c.is_initial(),
            Width(c) => c.is_initial(),
            Height(c) => c.is_initial(),
            MinWidth(c) => c.is_initial(),
            MinHeight(c) => c.is_initial(),
            MaxWidth(c) => c.is_initial(),
            MaxHeight(c) => c.is_initial(),
            Position(c) => c.is_initial(),
            Top(c) => c.is_initial(),
            Right(c) => c.is_initial(),
            Left(c) => c.is_initial(),
            Bottom(c) => c.is_initial(),
            FlexWrap(c) => c.is_initial(),
            FlexDirection(c) => c.is_initial(),
            FlexGrow(c) => c.is_initial(),
            FlexShrink(c) => c.is_initial(),
            JustifyContent(c) => c.is_initial(),
            AlignItems(c) => c.is_initial(),
            AlignContent(c) => c.is_initial(),
            BackgroundContent(c) => c.is_initial(),
            BackgroundPosition(c) => c.is_initial(),
            BackgroundSize(c) => c.is_initial(),
            BackgroundRepeat(c) => c.is_initial(),
            OverflowX(c) => c.is_initial(),
            OverflowY(c) => c.is_initial(),
            PaddingTop(c) => c.is_initial(),
            PaddingLeft(c) => c.is_initial(),
            PaddingRight(c) => c.is_initial(),
            PaddingBottom(c) => c.is_initial(),
            MarginTop(c) => c.is_initial(),
            MarginLeft(c) => c.is_initial(),
            MarginRight(c) => c.is_initial(),
            MarginBottom(c) => c.is_initial(),
            BorderTopLeftRadius(c) => c.is_initial(),
            BorderTopRightRadius(c) => c.is_initial(),
            BorderBottomLeftRadius(c) => c.is_initial(),
            BorderBottomRightRadius(c) => c.is_initial(),
            BorderTopColor(c) => c.is_initial(),
            BorderRightColor(c) => c.is_initial(),
            BorderLeftColor(c) => c.is_initial(),
            BorderBottomColor(c) => c.is_initial(),
            BorderTopStyle(c) => c.is_initial(),
            BorderRightStyle(c) => c.is_initial(),
            BorderLeftStyle(c) => c.is_initial(),
            BorderBottomStyle(c) => c.is_initial(),
            BorderTopWidth(c) => c.is_initial(),
            BorderRightWidth(c) => c.is_initial(),
            BorderLeftWidth(c) => c.is_initial(),
            BorderBottomWidth(c) => c.is_initial(),
            BoxShadowLeft(c) => c.is_initial(),
            BoxShadowRight(c) => c.is_initial(),
            BoxShadowTop(c) => c.is_initial(),
            BoxShadowBottom(c) => c.is_initial(),
            ScrollbarStyle(c) => c.is_initial(),
            Opacity(c) => c.is_initial(),
            Transform(c) => c.is_initial(),
            TransformOrigin(c) => c.is_initial(),
            PerspectiveOrigin(c) => c.is_initial(),
            BackfaceVisibility(c) => c.is_initial(),
            MixBlendMode(c) => c.is_initial(),
            Filter(c) => c.is_initial(),
            BackdropFilter(c) => c.is_initial(),
            TextShadow(c) => c.is_initial(),
            WhiteSpace(c) => c.is_initial(),
            Direction(c) => c.is_initial(),
            Hyphens(c) => c.is_initial(),
        }
    }

    pub const fn const_none(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, None)
    }
    pub const fn const_auto(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Auto)
    }
    pub const fn const_initial(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Initial)
    }
    pub const fn const_inherit(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Inherit)
    }

    pub const fn const_text_color(input: StyleTextColor) -> Self {
        CssProperty::TextColor(StyleTextColorValue::Exact(input))
    }
    pub const fn const_font_size(input: StyleFontSize) -> Self {
        CssProperty::FontSize(StyleFontSizeValue::Exact(input))
    }
    pub const fn const_font_family(input: StyleFontFamilyVec) -> Self {
        CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(input))
    }
    pub const fn const_text_align(input: StyleTextAlign) -> Self {
        CssProperty::TextAlign(StyleTextAlignValue::Exact(input))
    }
    pub const fn const_letter_spacing(input: StyleLetterSpacing) -> Self {
        CssProperty::LetterSpacing(StyleLetterSpacingValue::Exact(input))
    }
    pub const fn const_line_height(input: StyleLineHeight) -> Self {
        CssProperty::LineHeight(StyleLineHeightValue::Exact(input))
    }
    pub const fn const_word_spacing(input: StyleWordSpacing) -> Self {
        CssProperty::WordSpacing(StyleWordSpacingValue::Exact(input))
    }
    pub const fn const_tab_width(input: StyleTabWidth) -> Self {
        CssProperty::TabWidth(StyleTabWidthValue::Exact(input))
    }
    pub const fn const_cursor(input: StyleCursor) -> Self {
        CssProperty::Cursor(StyleCursorValue::Exact(input))
    }
    pub const fn const_display(input: LayoutDisplay) -> Self {
        CssProperty::Display(LayoutDisplayValue::Exact(input))
    }
    pub const fn const_float(input: LayoutFloat) -> Self {
        CssProperty::Float(LayoutFloatValue::Exact(input))
    }
    pub const fn const_box_sizing(input: LayoutBoxSizing) -> Self {
        CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(input))
    }
    pub const fn const_width(input: LayoutWidth) -> Self {
        CssProperty::Width(LayoutWidthValue::Exact(input))
    }
    pub const fn const_height(input: LayoutHeight) -> Self {
        CssProperty::Height(LayoutHeightValue::Exact(input))
    }
    pub const fn const_min_width(input: LayoutMinWidth) -> Self {
        CssProperty::MinWidth(LayoutMinWidthValue::Exact(input))
    }
    pub const fn const_min_height(input: LayoutMinHeight) -> Self {
        CssProperty::MinHeight(LayoutMinHeightValue::Exact(input))
    }
    pub const fn const_max_width(input: LayoutMaxWidth) -> Self {
        CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(input))
    }
    pub const fn const_max_height(input: LayoutMaxHeight) -> Self {
        CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(input))
    }
    pub const fn const_position(input: LayoutPosition) -> Self {
        CssProperty::Position(LayoutPositionValue::Exact(input))
    }
    pub const fn const_top(input: LayoutTop) -> Self {
        CssProperty::Top(LayoutTopValue::Exact(input))
    }
    pub const fn const_right(input: LayoutRight) -> Self {
        CssProperty::Right(LayoutRightValue::Exact(input))
    }
    pub const fn const_left(input: LayoutLeft) -> Self {
        CssProperty::Left(LayoutLeftValue::Exact(input))
    }
    pub const fn const_bottom(input: LayoutBottom) -> Self {
        CssProperty::Bottom(LayoutBottomValue::Exact(input))
    }
    pub const fn const_flex_wrap(input: LayoutFlexWrap) -> Self {
        CssProperty::FlexWrap(LayoutFlexWrapValue::Exact(input))
    }
    pub const fn const_flex_direction(input: LayoutFlexDirection) -> Self {
        CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(input))
    }
    pub const fn const_flex_grow(input: LayoutFlexGrow) -> Self {
        CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(input))
    }
    pub const fn const_flex_shrink(input: LayoutFlexShrink) -> Self {
        CssProperty::FlexShrink(LayoutFlexShrinkValue::Exact(input))
    }
    pub const fn const_justify_content(input: LayoutJustifyContent) -> Self {
        CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(input))
    }
    pub const fn const_align_items(input: LayoutAlignItems) -> Self {
        CssProperty::AlignItems(LayoutAlignItemsValue::Exact(input))
    }
    pub const fn const_align_content(input: LayoutAlignContent) -> Self {
        CssProperty::AlignContent(LayoutAlignContentValue::Exact(input))
    }
    pub const fn const_background_content(input: StyleBackgroundContentVec) -> Self {
        CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(input))
    }
    pub const fn const_background_position(input: StyleBackgroundPositionVec) -> Self {
        CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::Exact(input))
    }
    pub const fn const_background_size(input: StyleBackgroundSizeVec) -> Self {
        CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::Exact(input))
    }
    pub const fn const_background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::Exact(input))
    }
    pub const fn const_overflow_x(input: LayoutOverflow) -> Self {
        CssProperty::OverflowX(LayoutOverflowValue::Exact(input))
    }
    pub const fn const_overflow_y(input: LayoutOverflow) -> Self {
        CssProperty::OverflowY(LayoutOverflowValue::Exact(input))
    }
    pub const fn const_padding_top(input: LayoutPaddingTop) -> Self {
        CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(input))
    }
    pub const fn const_padding_left(input: LayoutPaddingLeft) -> Self {
        CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(input))
    }
    pub const fn const_padding_right(input: LayoutPaddingRight) -> Self {
        CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(input))
    }
    pub const fn const_padding_bottom(input: LayoutPaddingBottom) -> Self {
        CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(input))
    }
    pub const fn const_margin_top(input: LayoutMarginTop) -> Self {
        CssProperty::MarginTop(LayoutMarginTopValue::Exact(input))
    }
    pub const fn const_margin_left(input: LayoutMarginLeft) -> Self {
        CssProperty::MarginLeft(LayoutMarginLeftValue::Exact(input))
    }
    pub const fn const_margin_right(input: LayoutMarginRight) -> Self {
        CssProperty::MarginRight(LayoutMarginRightValue::Exact(input))
    }
    pub const fn const_margin_bottom(input: LayoutMarginBottom) -> Self {
        CssProperty::MarginBottom(LayoutMarginBottomValue::Exact(input))
    }
    pub const fn const_border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input))
    }
    pub const fn const_border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input))
    }
    pub const fn const_border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input))
    }
    pub const fn const_border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input))
    }
    pub const fn const_border_top_color(input: StyleBorderTopColor) -> Self {
        CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(input))
    }
    pub const fn const_border_right_color(input: StyleBorderRightColor) -> Self {
        CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(input))
    }
    pub const fn const_border_left_color(input: StyleBorderLeftColor) -> Self {
        CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(input))
    }
    pub const fn const_border_bottom_color(input: StyleBorderBottomColor) -> Self {
        CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(input))
    }
    pub const fn const_border_top_style(input: StyleBorderTopStyle) -> Self {
        CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(input))
    }
    pub const fn const_border_right_style(input: StyleBorderRightStyle) -> Self {
        CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(input))
    }
    pub const fn const_border_left_style(input: StyleBorderLeftStyle) -> Self {
        CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input))
    }
    pub const fn const_border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input))
    }
    pub const fn const_border_top_width(input: LayoutBorderTopWidth) -> Self {
        CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(input))
    }
    pub const fn const_border_right_width(input: LayoutBorderRightWidth) -> Self {
        CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(input))
    }
    pub const fn const_border_left_width(input: LayoutBorderLeftWidth) -> Self {
        CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(input))
    }
    pub const fn const_border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(input))
    }
    pub const fn const_box_shadow_left(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_right(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_top(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_bottom(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_opacity(input: StyleOpacity) -> Self {
        CssProperty::Opacity(StyleOpacityValue::Exact(input))
    }
    pub const fn const_transform(input: StyleTransformVec) -> Self {
        CssProperty::Transform(StyleTransformVecValue::Exact(input))
    }
    pub const fn const_transform_origin(input: StyleTransformOrigin) -> Self {
        CssProperty::TransformOrigin(StyleTransformOriginValue::Exact(input))
    }
    pub const fn const_perspective_origin(input: StylePerspectiveOrigin) -> Self {
        CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input))
    }
    pub const fn const_backface_visiblity(input: StyleBackfaceVisibility) -> Self {
        CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input))
    }
}
