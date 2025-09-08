//! Main CSS property types and enumerations

use alloc::{collections::BTreeMap, string::String};

// Import all the property types from our organized modules
use crate::props::basic::{angle::AngleValue, color::ColorU, value::PixelValue};
use crate::props::layout::{
    dimensions::*, display::LayoutDisplay, flex::*, overflow::LayoutOverflow, position::*,
    spacing::*,
};
use crate::props::style::{
    text::*,
    // TODO: Import other style properties as they're implemented
};

/// Main CSS property enum that encompasses all possible CSS properties
#[derive(Debug, Clone, PartialEq)]
pub enum CssProperty {
    // Layout properties
    Display(LayoutDisplay),
    Width(LayoutWidth),
    Height(LayoutHeight),
    MinWidth(LayoutMinWidth),
    MaxWidth(LayoutMaxWidth),
    MinHeight(LayoutMinHeight),
    MaxHeight(LayoutMaxHeight),
    Position(LayoutPosition),
    Top(LayoutTop),
    Right(LayoutRight),
    Bottom(LayoutBottom),
    Left(LayoutLeft),
    PaddingTop(LayoutPaddingTop),
    PaddingRight(LayoutPaddingRight),
    PaddingBottom(LayoutPaddingBottom),
    PaddingLeft(LayoutPaddingLeft),
    MarginTop(LayoutMarginTop),
    MarginRight(LayoutMarginRight),
    MarginBottom(LayoutMarginBottom),
    MarginLeft(LayoutMarginLeft),
    FlexDirection(LayoutFlexDirection),
    FlexWrap(LayoutFlexWrap),
    JustifyContent(LayoutJustifyContent),
    AlignItems(LayoutAlignItems),
    AlignContent(LayoutAlignContent),
    FlexGrow(LayoutFlexGrow),
    FlexShrink(LayoutFlexShrink),
    Overflow(LayoutOverflow),
    BoxSizing(LayoutBoxSizing),

    // Style properties
    TextColor(StyleTextColor),
    FontSize(StyleFontSize),
    TextAlign(StyleTextAlign),
    LineHeight(StyleLineHeight),
    LetterSpacing(StyleLetterSpacing),
    // TODO: Add other style properties as modules are completed
}

/// CSS property types for identification and mapping
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyType {
    // Layout property types
    Display,
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
    Position,
    Top,
    Right,
    Bottom,
    Left,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignContent,
    FlexGrow,
    FlexShrink,
    Overflow,
    BoxSizing,

    // Style property types
    TextColor,
    FontSize,
    TextAlign,
    LineHeight,
    LetterSpacing,
    // TODO: Add other property types
}

/// Combined CSS property types (shorthand properties)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CombinedCssPropertyType {
    /// Combined padding property
    Padding,
    /// Combined margin property
    Margin,
    /// Combined border property
    Border,
    /// Combined border-left property
    BorderLeft,
    /// Combined border-right property
    BorderRight,
    /// Combined border-top property
    BorderTop,
    /// Combined border-bottom property
    BorderBottom,
    /// Combined border-radius property
    BorderRadius,
    /// Combined overflow property
    Overflow,
    /// Combined box-shadow property
    BoxShadow,
    /// Combined background-color property
    BackgroundColor,
    /// Combined background-image property
    BackgroundImage,
}

/// CSS property key mappings for parsing
pub const CSS_PROPERTY_KEY_MAP: &[(CssPropertyType, &'static str)] = &[
    // Layout properties
    (CssPropertyType::Display, "display"),
    (CssPropertyType::Width, "width"),
    (CssPropertyType::Height, "height"),
    (CssPropertyType::MinWidth, "min-width"),
    (CssPropertyType::MaxWidth, "max-width"),
    (CssPropertyType::MinHeight, "min-height"),
    (CssPropertyType::MaxHeight, "max-height"),
    (CssPropertyType::Position, "position"),
    (CssPropertyType::Top, "top"),
    (CssPropertyType::Right, "right"),
    (CssPropertyType::Bottom, "bottom"),
    (CssPropertyType::Left, "left"),
    (CssPropertyType::PaddingTop, "padding-top"),
    (CssPropertyType::PaddingRight, "padding-right"),
    (CssPropertyType::PaddingBottom, "padding-bottom"),
    (CssPropertyType::PaddingLeft, "padding-left"),
    (CssPropertyType::MarginTop, "margin-top"),
    (CssPropertyType::MarginRight, "margin-right"),
    (CssPropertyType::MarginBottom, "margin-bottom"),
    (CssPropertyType::MarginLeft, "margin-left"),
    (CssPropertyType::FlexDirection, "flex-direction"),
    (CssPropertyType::FlexWrap, "flex-wrap"),
    (CssPropertyType::JustifyContent, "justify-content"),
    (CssPropertyType::AlignItems, "align-items"),
    (CssPropertyType::AlignContent, "align-content"),
    (CssPropertyType::FlexGrow, "flex-grow"),
    (CssPropertyType::FlexShrink, "flex-shrink"),
    (CssPropertyType::Overflow, "overflow"),
    (CssPropertyType::BoxSizing, "box-sizing"),
    // Style properties
    (CssPropertyType::TextColor, "color"),
    (CssPropertyType::FontSize, "font-size"),
    (CssPropertyType::TextAlign, "text-align"),
    (CssPropertyType::LineHeight, "line-height"),
    (CssPropertyType::LetterSpacing, "letter-spacing"),
];

/// Combined CSS property key mappings
pub const COMBINED_CSS_PROPERTIES_KEY_MAP: &[(CombinedCssPropertyType, &'static str)] = &[
    (CombinedCssPropertyType::Padding, "padding"),
    (CombinedCssPropertyType::Margin, "margin"),
    (CombinedCssPropertyType::Border, "border"),
    (CombinedCssPropertyType::BorderLeft, "border-left"),
    (CombinedCssPropertyType::BorderRight, "border-right"),
    (CombinedCssPropertyType::BorderTop, "border-top"),
    (CombinedCssPropertyType::BorderBottom, "border-bottom"),
    (CombinedCssPropertyType::BorderRadius, "border-radius"),
    (CombinedCssPropertyType::Overflow, "overflow"),
    (CombinedCssPropertyType::BoxShadow, "box-shadow"),
    (CombinedCssPropertyType::BackgroundColor, "background-color"),
    (CombinedCssPropertyType::BackgroundImage, "background-image"),
];

/// CSS key mapping structure
pub struct CssKeyMap {
    pub normal_properties: BTreeMap<String, CssPropertyType>,
    pub combined_properties: BTreeMap<String, CombinedCssPropertyType>,
}

impl CssProperty {
    /// Get the property type for this CSS property
    pub fn get_type(&self) -> CssPropertyType {
        use CssProperty::*;
        match self {
            Display(_) => CssPropertyType::Display,
            Width(_) => CssPropertyType::Width,
            Height(_) => CssPropertyType::Height,
            MinWidth(_) => CssPropertyType::MinWidth,
            MaxWidth(_) => CssPropertyType::MaxWidth,
            MinHeight(_) => CssPropertyType::MinHeight,
            MaxHeight(_) => CssPropertyType::MaxHeight,
            Position(_) => CssPropertyType::Position,
            Top(_) => CssPropertyType::Top,
            Right(_) => CssPropertyType::Right,
            Bottom(_) => CssPropertyType::Bottom,
            Left(_) => CssPropertyType::Left,
            PaddingTop(_) => CssPropertyType::PaddingTop,
            PaddingRight(_) => CssPropertyType::PaddingRight,
            PaddingBottom(_) => CssPropertyType::PaddingBottom,
            PaddingLeft(_) => CssPropertyType::PaddingLeft,
            MarginTop(_) => CssPropertyType::MarginTop,
            MarginRight(_) => CssPropertyType::MarginRight,
            MarginBottom(_) => CssPropertyType::MarginBottom,
            MarginLeft(_) => CssPropertyType::MarginLeft,
            FlexDirection(_) => CssPropertyType::FlexDirection,
            FlexWrap(_) => CssPropertyType::FlexWrap,
            JustifyContent(_) => CssPropertyType::JustifyContent,
            AlignItems(_) => CssPropertyType::AlignItems,
            AlignContent(_) => CssPropertyType::AlignContent,
            FlexGrow(_) => CssPropertyType::FlexGrow,
            FlexShrink(_) => CssPropertyType::FlexShrink,
            Overflow(_) => CssPropertyType::Overflow,
            BoxSizing(_) => CssPropertyType::BoxSizing,
            TextColor(_) => CssPropertyType::TextColor,
            FontSize(_) => CssPropertyType::FontSize,
            TextAlign(_) => CssPropertyType::TextAlign,
            LineHeight(_) => CssPropertyType::LineHeight,
            LetterSpacing(_) => CssPropertyType::LetterSpacing,
        }
    }

    /// Interpolate between two CSS properties (for animations)
    pub fn interpolate(&self, other: &Self, t: f32) -> Option<Self> {
        // Simple implementation - only handle color properties for now
        match (self, other) {
            (CssProperty::TextColor(a), CssProperty::TextColor(b)) => {
                Some(CssProperty::TextColor(StyleTextColor {
                    inner: a.inner.interpolate(&b.inner, t),
                }))
            }
            _ => None, // Most properties don't interpolate
        }
    }
}

/// Get the CSS key map for property lookup
pub fn get_css_key_map() -> CssKeyMap {
    let mut normal_properties = BTreeMap::new();
    let mut combined_properties = BTreeMap::new();

    // Build normal properties map
    for &(prop_type, key) in CSS_PROPERTY_KEY_MAP {
        normal_properties.insert(key.to_string(), prop_type);
    }

    // Build combined properties map
    for &(prop_type, key) in COMBINED_CSS_PROPERTIES_KEY_MAP {
        combined_properties.insert(key.to_string(), prop_type);
    }

    CssKeyMap {
        normal_properties,
        combined_properties,
    }
}
