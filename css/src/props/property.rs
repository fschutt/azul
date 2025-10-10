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
        font::{
            parse_style_font_family, CssStyleFontFamilyParseError,
            CssStyleFontFamilyParseErrorOwned, StyleFontFamilyVec,
        },
        value::{
            parse_percentage_value, parse_pixel_value, CssPixelValueParseError,
            CssPixelValueParseErrorOwned, FloatValue, PercentageParseError, PercentageValue,
            PixelValue,
        },
    },
    formatter::PrintAsCssValue,
    layout::{dimensions::*, display::*, flex::*, overflow::*, position::*, spacing::*},
    style::{
        background::*, border::*, border_radius::*, box_shadow::*, effects::*, filter::*, font::*,
        scrollbar::*, text::*, transform::*,
    },
};
use crate::{
    css::CssPropertyValue,
    parser::{impl_debug_as_display, impl_display, impl_from, InvalidValueErr, PixelValueWithAuto},
    AzString,
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

#[cfg(feature = "parser")]
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

    let value = value.trim();

    // Handle global keywords 'initial' and 'inherit'.
    // Other keywords like 'auto' or 'none' are context-dependent and are handled
    // by the individual parsers below.
    if value == "initial" || value == "inherit" {
        let keys = match key {
            BorderRadius => vec![
                CssPropertyType::BorderTopLeftRadius,
                CssPropertyType::BorderTopRightRadius,
                CssPropertyType::BorderBottomLeftRadius,
                CssPropertyType::BorderBottomRightRadius,
            ],
            Overflow => vec![CssPropertyType::OverflowX, CssPropertyType::OverflowY],
            Padding => vec![
                CssPropertyType::PaddingTop,
                CssPropertyType::PaddingRight,
                CssPropertyType::PaddingBottom,
                CssPropertyType::PaddingLeft,
            ],
            Margin => vec![
                CssPropertyType::MarginTop,
                CssPropertyType::MarginRight,
                CssPropertyType::MarginBottom,
                CssPropertyType::MarginLeft,
            ],
            Border => vec![
                CssPropertyType::BorderTopWidth,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderBottomWidth,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderLeftWidth,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderLeftColor,
            ],
            BorderLeft => vec![
                CssPropertyType::BorderLeftWidth,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderLeftColor,
            ],
            BorderRight => vec![
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderRightColor,
            ],
            BorderTop => vec![
                CssPropertyType::BorderTopWidth,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderTopColor,
            ],
            BorderBottom => vec![
                CssPropertyType::BorderBottomWidth,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderBottomColor,
            ],
            BoxShadow => vec![
                CssPropertyType::BoxShadowTop,
                CssPropertyType::BoxShadowRight,
                CssPropertyType::BoxShadowBottom,
                CssPropertyType::BoxShadowLeft,
            ],
            BackgroundColor | BackgroundImage => vec![CssPropertyType::BackgroundContent],
        };

        if value == "initial" {
            return Ok(keys.into_iter().map(CssProperty::initial).collect());
        } else {
            // "inherit"
            return Ok(keys.into_iter().map(CssProperty::inherit).collect());
        }
    }

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
                CssProperty::overflow_x(overflow),
                CssProperty::overflow_y(overflow),
            ])
        }
        Padding => {
            let padding = parse_layout_padding(value)?;
            Ok(vec![
                convert_value!(padding.top, PaddingTop, LayoutPaddingTop),
                convert_value!(padding.right, PaddingRight, LayoutPaddingRight),
                convert_value!(padding.bottom, PaddingBottom, LayoutPaddingBottom),
                convert_value!(padding.left, PaddingLeft, LayoutPaddingLeft),
            ])
        }
        Margin => {
            let margin = parse_layout_margin(value)?;
            Ok(vec![
                convert_value!(margin.top, MarginTop, LayoutMarginTop),
                convert_value!(margin.right, MarginRight, LayoutMarginRight),
                convert_value!(margin.bottom, MarginBottom, LayoutMarginBottom),
                convert_value!(margin.left, MarginLeft, LayoutMarginLeft),
            ])
        }
        Border => {
            let border = parse_border_side(value)?;
            Ok(vec![
                CssProperty::border_top_color(StyleBorderTopColor {
                    inner: border.border_color,
                }),
                CssProperty::border_right_color(StyleBorderRightColor {
                    inner: border.border_color,
                }),
                CssProperty::border_bottom_color(StyleBorderBottomColor {
                    inner: border.border_color,
                }),
                CssProperty::border_left_color(StyleBorderLeftColor {
                    inner: border.border_color,
                }),
                CssProperty::border_top_style(StyleBorderTopStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_right_style(StyleBorderRightStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_bottom_style(StyleBorderBottomStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_left_style(StyleBorderLeftStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_top_width(LayoutBorderTopWidth {
                    inner: border.border_width,
                }),
                CssProperty::border_right_width(LayoutBorderRightWidth {
                    inner: border.border_width,
                }),
                CssProperty::border_bottom_width(LayoutBorderBottomWidth {
                    inner: border.border_width,
                }),
                CssProperty::border_left_width(LayoutBorderLeftWidth {
                    inner: border.border_width,
                }),
            ])
        }
        BorderLeft => {
            let border = parse_border_side(value)?;
            Ok(vec![
                CssProperty::border_left_color(StyleBorderLeftColor {
                    inner: border.border_color,
                }),
                CssProperty::border_left_style(StyleBorderLeftStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_left_width(LayoutBorderLeftWidth {
                    inner: border.border_width,
                }),
            ])
        }
        BorderRight => {
            let border = parse_border_side(value)?;
            Ok(vec![
                CssProperty::border_right_color(StyleBorderRightColor {
                    inner: border.border_color,
                }),
                CssProperty::border_right_style(StyleBorderRightStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_right_width(LayoutBorderRightWidth {
                    inner: border.border_width,
                }),
            ])
        }
        BorderTop => {
            let border = parse_border_side(value)?;
            Ok(vec![
                CssProperty::border_top_color(StyleBorderTopColor {
                    inner: border.border_color,
                }),
                CssProperty::border_top_style(StyleBorderTopStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_top_width(LayoutBorderTopWidth {
                    inner: border.border_width,
                }),
            ])
        }
        BorderBottom => {
            let border = parse_border_side(value)?;
            Ok(vec![
                CssProperty::border_bottom_color(StyleBorderBottomColor {
                    inner: border.border_color,
                }),
                CssProperty::border_bottom_style(StyleBorderBottomStyle {
                    inner: border.border_style,
                }),
                CssProperty::border_bottom_width(LayoutBorderBottomWidth {
                    inner: border.border_width,
                }),
            ])
        }
        BoxShadow => {
            let box_shadow = parse_style_box_shadow(value)?;
            Ok(vec![
                CssProperty::box_shadow_left(box_shadow),
                CssProperty::box_shadow_right(box_shadow),
                CssProperty::box_shadow_top(box_shadow),
                CssProperty::box_shadow_bottom(box_shadow),
            ])
        }
        BackgroundColor => {
            let color = parse_css_color(value)?;
            let vec: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            Ok(vec![CssProperty::background_content(vec)])
        }
        BackgroundImage => {
            let background_content = parse_style_background_content(value)?;
            let vec: StyleBackgroundContentVec = vec![background_content].into();
            Ok(vec![CssProperty::background_content(vec)])
        }
    }
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
