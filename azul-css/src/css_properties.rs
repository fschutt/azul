//! Provides a public API with datatypes used to describe style properties of DOM nodes.

// The following types are present in webrender, however, azul-css should not
// depend on webrender, just to have the same types, azul-css should be a standalone crate.

/// Only used for calculations: Rectangle (x, y, width, height) in layout space.
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}
/// Only used for calculations: Size (width, height) in layout space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutSize {
    pub width: f32,
    pub height: f32,
}
/// Only used for calculations: Point coordinate (x, y) in layout space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutPoint {
    pub x: f32,
    pub y: f32,
}

impl LayoutSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Represents a parsed pair of `5px, 10px` values - useful for border radius calculation
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct PixelSize {
    pub width: PixelValue,
    pub height: PixelValue,
}

impl PixelSize {
    pub fn new(width: PixelValue, height: PixelValue) -> Self {
        Self { width, height }
    }

    pub fn zero() -> Self {
        Self::new(PixelValue::px(0.0), PixelValue::px(0.0))
    }
}

/// Offsets of the border-width calculations
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct LayoutSideOffsets {
    pub top: FloatValue,
    pub right: FloatValue,
    pub bottom: FloatValue,
    pub left: FloatValue,
}

/// u8-based color, range 0 to 255 (similar to webrenders ColorU)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct ColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders ColorF)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl From<ColorU> for ColorF {
    fn from(input: ColorU) -> ColorF {
        ColorF {
            r: (input.r as f32) / 255.0,
            g: (input.g as f32) / 255.0,
            b: (input.b as f32) / 255.0,
            a: (input.a as f32) / 255.0,
        }
    }
}

impl From<ColorF> for ColorU {
    fn from(input: ColorF) -> ColorU {
        ColorU {
            r: (input.r.min(1.0) * 255.0) as u8,
            g: (input.g.min(1.0) * 255.0) as u8,
            b: (input.b.min(1.0) * 255.0) as u8,
            a: (input.a.min(1.0) * 255.0) as u8,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct BorderRadius {
    pub top_left: PixelSize,
    pub top_right: PixelSize,
    pub bottom_left: PixelSize,
    pub bottom_right: PixelSize,
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

impl BorderRadius {
    pub fn zero() -> Self {
        Self::uniform(PixelSize::zero())
    }

    pub fn uniform(value: PixelSize) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_left: value,
            bottom_right: value,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum BorderDetails {
    Normal(NormalBorder),
    NinePatch(NinePatchBorder),
}

/// Represents a normal `border` property (no image border / nine-patch border)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct NormalBorder {
    pub left: BorderSide,
    pub right: BorderSide,
    pub top: BorderSide,
    pub bottom: BorderSide,
    pub radius: Option<BorderRadius>,
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct BorderSide {
    pub color: ColorU,
    pub style: BorderStyle,
}

/// What direction should a `box-shadow` be clipped in (inset or outset)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum BoxShadowClipMode {
    Outset,
    Inset,
}

/// Whether a `gradient` should be repeated or clamped to the edges.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum ExtendMode {
    Clamp,
    Repeat,
}

/// Style of a `border`: solid, double, dash, ridge, etc.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum BorderStyle {
    None,
    Solid,
    Double,
    Dotted,
    Dashed,
    Hidden,
    Groove,
    Ridge,
    Inset,
    Outset,
}
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct NinePatchBorder {
    // not implemented or parse-able yet, so no fields!
}

/// Currently hard-coded: Height of one em in pixels
pub const EM_HEIGHT: f32 = 16.0;
/// WebRender measures in points, not in pixels!
pub const PT_TO_PX: f32 = 96.0 / 72.0;

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
macro_rules! impl_pixel_value {
    ($struct:ident) => {
        impl $struct {
            #[inline]
            pub fn px(value: f32) -> Self {
                $struct(PixelValue::px(value))
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                $struct(PixelValue::em(value))
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                $struct(PixelValue::pt(value))
            }
        }
    };
}

pub const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &'static str); 51] = [
    (CssPropertyType::BorderRadius, "border-radius"),
    (CssPropertyType::BackgroundColor, "background-color"),
    (CssPropertyType::TextColor, "color"),
    (CssPropertyType::Background, "background"),
    (CssPropertyType::FontSize, "font-size"),
    (CssPropertyType::FontFamily, "font-family"),
    (CssPropertyType::TextAlign, "text-align"),
    (CssPropertyType::LetterSpacing, "letter-spacing"),
    (CssPropertyType::LineHeight, "line-height"),
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
    (CssPropertyType::Overflow, "overflow"),
    (CssPropertyType::OverflowX, "overflow-x"),
    (CssPropertyType::OverflowY, "overflow-y"),
    (CssPropertyType::Padding, "padding"),
    (CssPropertyType::PaddingTop, "padding-top"),
    (CssPropertyType::PaddingLeft, "padding-left"),
    (CssPropertyType::PaddingRight, "padding-right"),
    (CssPropertyType::PaddingBottom, "padding-bottom"),
    (CssPropertyType::Margin, "margin"),
    (CssPropertyType::MarginTop, "margin-top"),
    (CssPropertyType::MarginLeft, "margin-left"),
    (CssPropertyType::MarginRight, "margin-right"),
    (CssPropertyType::MarginBottom, "margin-bottom"),
    (CssPropertyType::Border, "border"),
    (CssPropertyType::BorderTop, "border-top"),
    (CssPropertyType::BorderLeft, "border-left"),
    (CssPropertyType::BorderRight, "border-right"),
    (CssPropertyType::BorderBottom, "border-bottom"),
    (CssPropertyType::BoxShadow, "box-shadow"),
    (CssPropertyType::BoxShadowTop, "box-shadow-top"),
    (CssPropertyType::BoxShadowLeft, "box-shadow-left"),
    (CssPropertyType::BoxShadowRight, "box-shadow-right"),
    (CssPropertyType::BoxShadowBottom, "box-shadow-bottom"),
];

/// Returns a map useful for parsing the keys of CSS stylesheets
pub fn get_css_key_map() -> BTreeMap<&'static str, CssPropertyType> {
    CSS_PROPERTY_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect()
}

/// Same as CssProperty, but without any data. Used to identify the
/// key of the CSS key-value pair without parsing the value
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyType {
    BorderRadius,
    BackgroundColor,
    TextColor,
    Background,
    FontSize,
    FontFamily,
    TextAlign,
    LetterSpacing,
    LineHeight,
    Cursor,
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

    Overflow,
    OverflowX,
    OverflowY,

    Padding,
    PaddingTop,
    PaddingLeft,
    PaddingRight,
    PaddingBottom,

    Margin,
    MarginTop,
    MarginLeft,
    MarginRight,
    MarginBottom,

    Border,
    BorderTop,
    BorderLeft,
    BorderRight,
    BorderBottom,

    BoxShadow,
    BoxShadowTop,
    BoxShadowLeft,
    BoxShadowRight,
    BoxShadowBottom,
}

impl CssPropertyType {
    /// Parses a CSS key, such as `width` from a string:
    ///
    /// ```rust
    /// # use azul_css::{CssPropertyType, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(Some(CssPropertyType::Width), CssPropertyType::from_str("width", &map));
    /// assert_eq!(Some(CssPropertyType::JustifyContent), CssPropertyType::from_str("justify-content", &map));
    /// assert_eq!(None, CssPropertyType::from_str("asdfasdfasdf", &map));
    /// ```
    pub fn from_str(input: &str, map: &BTreeMap<&'static str, Self>) -> Option<Self> {
        let input = input.trim();
        map.get(input).and_then(|x| Some(*x))
    }

    pub fn to_str(&self, map: &BTreeMap<&'static str, Self>) -> &'static str {
        map.iter()
            .find(|(_, v)| *v == self)
            .and_then(|(k, _)| Some(k))
            .unwrap()
    }

    /// Returns whether this property will be inherited during cascading
    pub fn is_inheritable(&self) -> bool {
        /// Returns whether this property can trigger a re-layout
        use self::CssPropertyType::*;
        match self {
            TextColor | FontFamily | FontSize | LineHeight | TextAlign => true,
            _ => false,
        }
    }

    /// Returns whether this property can trigger a re-layout
    pub fn can_trigger_relayout(&self) -> bool {
        use self::CssPropertyType::*;

        // Since the border can be larger than the content,
        // in which case the content needs to be re-layouted, assume true for Border

        // FontFamily, FontSize, LetterSpacing and LineHeight can affect
        // the text layout and therefore the screen layout

        match self {
            BorderRadius | BackgroundColor | TextColor | Background | TextAlign | BoxShadow
            | BoxShadowTop | BoxShadowLeft | BoxShadowBottom | BoxShadowRight | Cursor => false,
            _ => true,
        }
    }
}

impl fmt::Display for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let key = CSS_PROPERTY_KEY_MAP
            .iter()
            .find(|(v, _)| *v == *self)
            .and_then(|(k, _)| Some(k))
            .unwrap();
        write!(f, "{}", key)
    }
}

/// A property that can be used to style DOM nodes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CssProperty {
    BorderRadius(StyleBorderRadius),
    BackgroundColor(StyleBackgroundColor),
    TextColor(StyleTextColor),
    Border(StyleBorder),
    Background(StyleBackground),
    FontSize(StyleFontSize),
    FontFamily(StyleFontFamily),
    TextAlign(StyleTextAlignmentHorz),
    LetterSpacing(StyleLetterSpacing),
    BoxShadow(StyleBoxShadow),
    LineHeight(StyleLineHeight),
    Cursor(StyleCursor),
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

impl CssProperty {
    pub fn get_type(&self) -> CssPropertyType {
        match &self {
            CssProperty::BorderRadius(_) => CssPropertyType::BorderRadius,
            CssProperty::BackgroundColor(_) => CssPropertyType::BackgroundColor,
            CssProperty::TextColor(_) => CssPropertyType::TextColor,
            CssProperty::Border(_) => CssPropertyType::Border,
            CssProperty::Background(_) => CssPropertyType::Background,
            CssProperty::FontSize(_) => CssPropertyType::FontSize,
            CssProperty::FontFamily(_) => CssPropertyType::FontFamily,
            CssProperty::TextAlign(_) => CssPropertyType::TextAlign,
            CssProperty::LetterSpacing(_) => CssPropertyType::LetterSpacing,
            CssProperty::BoxShadow(_) => CssPropertyType::BoxShadow,
            CssProperty::LineHeight(_) => CssPropertyType::LineHeight,
            CssProperty::Cursor(_) => CssPropertyType::Cursor,
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
            CssProperty::Padding(_) => CssPropertyType::Padding,
            CssProperty::Margin(_) => CssPropertyType::Margin,
            CssProperty::FlexWrap(_) => CssPropertyType::FlexWrap,
            CssProperty::FlexDirection(_) => CssPropertyType::FlexDirection,
            CssProperty::FlexGrow(_) => CssPropertyType::FlexGrow,
            CssProperty::FlexShrink(_) => CssPropertyType::FlexShrink,
            CssProperty::JustifyContent(_) => CssPropertyType::JustifyContent,
            CssProperty::AlignItems(_) => CssPropertyType::AlignItems,
            CssProperty::AlignContent(_) => CssPropertyType::AlignContent,
            CssProperty::Overflow(_) => CssPropertyType::Overflow,
        }
    }
}

impl_from!(StyleBorderRadius, CssProperty::BorderRadius);
impl_from!(StyleBackground, CssProperty::Background);
impl_from!(StyleBoxShadow, CssProperty::BoxShadow);
impl_from!(StyleBorder, CssProperty::Border);
impl_from!(StyleFontSize, CssProperty::FontSize);
impl_from!(StyleFontFamily, CssProperty::FontFamily);
impl_from!(StyleTextAlignmentHorz, CssProperty::TextAlign);
impl_from!(StyleLineHeight, CssProperty::LineHeight);
impl_from!(StyleLetterSpacing, CssProperty::LetterSpacing);
impl_from!(StyleBackgroundColor, CssProperty::BackgroundColor);
impl_from!(StyleTextColor, CssProperty::TextColor);
impl_from!(StyleCursor, CssProperty::Cursor);

impl_from!(LayoutOverflow, CssProperty::Overflow);
impl_from!(LayoutWidth, CssProperty::Width);
impl_from!(LayoutHeight, CssProperty::Height);
impl_from!(LayoutMinWidth, CssProperty::MinWidth);
impl_from!(LayoutMinHeight, CssProperty::MinHeight);
impl_from!(LayoutMaxWidth, CssProperty::MaxWidth);
impl_from!(LayoutMaxHeight, CssProperty::MaxHeight);

impl_from!(LayoutPosition, CssProperty::Position);
impl_from!(LayoutTop, CssProperty::Top);
impl_from!(LayoutBottom, CssProperty::Bottom);
impl_from!(LayoutRight, CssProperty::Right);
impl_from!(LayoutLeft, CssProperty::Left);

impl_from!(LayoutPadding, CssProperty::Padding);
impl_from!(LayoutMargin, CssProperty::Margin);

impl_from!(LayoutWrap, CssProperty::FlexWrap);
impl_from!(LayoutDirection, CssProperty::FlexDirection);
impl_from!(LayoutFlexGrow, CssProperty::FlexGrow);
impl_from!(LayoutFlexShrink, CssProperty::FlexShrink);
impl_from!(LayoutJustifyContent, CssProperty::JustifyContent);
impl_from!(LayoutAlignItems, CssProperty::AlignItems);
impl_from!(LayoutAlignContent, CssProperty::AlignContent);

/// Multiplier for floating point accuracy. Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 5 digits, i.e
/// `1.00001 == 1.0`
pub const FP_PRECISION_MULTIPLIER: f32 = 10000.0;

/// FloatValue, but associated with a certain metric (i.e. px, em, etc.)
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

impl PixelValue {
    #[inline]
    pub fn px(value: f32) -> Self {
        Self::from_metric(SizeMetric::Px, value)
    }

    #[inline]
    pub fn em(value: f32) -> Self {
        Self::from_metric(SizeMetric::Em, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
        Self {
            metric: metric,
            number: value.into(),
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels(&self) -> f32 {
        match self.metric {
            SizeMetric::Px => self.number.get(),
            SizeMetric::Pt => (self.number.get()) * PT_TO_PX,
            SizeMetric::Em => (self.number.get()) * EM_HEIGHT,
        }
    }
}

/// Wrapper around FloatValue, represents a percentage instead
/// of just being a regular floating-point value, i.e `5` = `5%`
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct PercentageValue {
    number: FloatValue,
}

impl PercentageValue {
    pub fn new(value: f32) -> Self {
        Self {
            number: value.into(),
        }
    }

    pub fn get(&self) -> f32 {
        self.number.get()
    }
}

/// Wrapper around an f32 value that is internally casted to an isize, in order to
/// provide hash-ability (to avoid numerical instability).
#[derive(Debug, PartialEq, Copy, Clone, Hash, Eq, Ord, PartialOrd)]
pub struct FloatValue {
    number: isize,
}

impl FloatValue {
    pub fn new(value: f32) -> Self {
        Self {
            number: (value * FP_PRECISION_MULTIPLIER) as isize,
        }
    }

    pub fn get(&self) -> f32 {
        self.number as f32 / FP_PRECISION_MULTIPLIER
    }
}

impl From<f32> for FloatValue {
    fn from(val: f32) -> Self {
        Self::new(val)
    }
}

/// Enum representing the metric associated with a number (px, pt, em, etc.)
#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq, Ord, PartialOrd)]
pub enum SizeMetric {
    Px,
    Pt,
    Em,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorderRadius(pub BorderRadius);

impl StyleBorderRadius {
    pub fn zero() -> Self {
        StyleBorderRadius(BorderRadius::zero())
    }
}

/// Represents a `background-color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBackgroundColor(pub ColorU);

impl Default for StyleBackgroundColor {
    fn default() -> Self {
        // Transparent color
        StyleBackgroundColor(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        })
    }
}

/// Represents a `color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleTextColor(pub ColorU);

/// Represents a `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutPadding {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

// $struct_name has to have top, left, right, bottom properties
macro_rules! merge_struct {
    ($struct_name:ident) => {
        impl $struct_name {
            pub fn merge(a: &mut Option<$struct_name>, b: &$struct_name) {
                if let Some(ref mut existing) = a {
                    if b.top.is_some() {
                        existing.top = b.top;
                    }
                    if b.bottom.is_some() {
                        existing.bottom = b.bottom;
                    }
                    if b.left.is_some() {
                        existing.left = b.left;
                    }
                    if b.right.is_some() {
                        existing.right = b.right;
                    }
                } else {
                    *a = Some(*b);
                }
            }
        }
    };
}

macro_rules! struct_all {
    ($struct_name:ident, $field_type:ty) => {
        impl $struct_name {
            /// Sets all of the fields (top, left, right, bottom) to `Some(field)`
            pub fn all(field: $field_type) -> Self {
                Self {
                    top: Some(field),
                    right: Some(field),
                    left: Some(field),
                    bottom: Some(field),
                }
            }
        }
    };
}

merge_struct!(LayoutPadding);
merge_struct!(LayoutMargin);
struct_all!(LayoutPadding, PixelValue);
struct_all!(LayoutMargin, PixelValue);

/// Represents a parsed `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutMargin {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

/// Wrapper for the `overflow-{x,y}` + `overflow` property
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutOverflow {
    pub horizontal: TextOverflowBehaviour,
    pub vertical: TextOverflowBehaviour,
}

impl LayoutOverflow {
    // "merges" two LayoutOverflow properties
    pub fn merge(a: &mut Option<Self>, b: &Self) {
        fn merge_property(p: &mut TextOverflowBehaviour, other: &TextOverflowBehaviour) {
            if *other == TextOverflowBehaviour::NotModified {
                return;
            }
            *p = *other;
        }

        if let Some(ref mut existing_overflow) = a {
            merge_property(&mut existing_overflow.horizontal, &b.horizontal);
            merge_property(&mut existing_overflow.vertical, &b.vertical);
        } else {
            *a = Some(*b)
        }
    }

    pub fn allows_horizontal_overflow(&self) -> bool {
        self.horizontal.can_overflow()
    }

    pub fn allows_vertical_overflow(&self) -> bool {
        self.vertical.can_overflow()
    }

    // If this overflow setting should show the horizontal scrollbar
    pub fn allows_horizontal_scrollbar(&self) -> bool {
        self.allows_horizontal_overflow()
    }

    pub fn allows_vertical_scrollbar(&self) -> bool {
        self.allows_vertical_overflow()
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorder {
    pub top: Option<StyleBorderSide>,
    pub left: Option<StyleBorderSide>,
    pub bottom: Option<StyleBorderSide>,
    pub right: Option<StyleBorderSide>,
}

merge_struct!(StyleBorder);
struct_all!(StyleBorder, StyleBorderSide);

impl StyleBorder {
    /// Returns the merged offsets and details for the top, left,
    /// right and bottom styles - necessary, so we can combine `border-top`,
    /// `border-left`, etc. into one border
    pub fn get_webrender_border(
        &self,
        border_radius: Option<StyleBorderRadius>,
    ) -> Option<(LayoutSideOffsets, BorderDetails)> {
        match (self.top, self.left, self.bottom, self.right) {
            (None, None, None, None) => None,
            (top, left, bottom, right) => {
                // Widths
                let border_width_top = top
                    .and_then(|top| Some(top.border_width.to_pixels()))
                    .unwrap_or(0.0);
                let border_width_bottom = bottom
                    .and_then(|bottom| Some(bottom.border_width.to_pixels()))
                    .unwrap_or(0.0);
                let border_width_left = left
                    .and_then(|left| Some(left.border_width.to_pixels()))
                    .unwrap_or(0.0);
                let border_width_right = right
                    .and_then(|right| Some(right.border_width.to_pixels()))
                    .unwrap_or(0.0);

                // Color
                let border_color_top = top
                    .and_then(|top| Some(top.border_color.into()))
                    .unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_bottom = bottom
                    .and_then(|bottom| Some(bottom.border_color.into()))
                    .unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_left = left
                    .and_then(|left| Some(left.border_color.into()))
                    .unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_right = right
                    .and_then(|right| Some(right.border_color.into()))
                    .unwrap_or(DEFAULT_BORDER_COLOR);

                // Styles
                let border_style_top = top
                    .and_then(|top| Some(top.border_style))
                    .unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_bottom = bottom
                    .and_then(|bottom| Some(bottom.border_style))
                    .unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_left = left
                    .and_then(|left| Some(left.border_style))
                    .unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_right = right
                    .and_then(|right| Some(right.border_style))
                    .unwrap_or(DEFAULT_BORDER_STYLE);

                let border_widths = LayoutSideOffsets {
                    top: FloatValue::new(border_width_top),
                    right: FloatValue::new(border_width_right),
                    bottom: FloatValue::new(border_width_bottom),
                    left: FloatValue::new(border_width_left),
                };
                let border_details = BorderDetails::Normal(NormalBorder {
                    top: BorderSide {
                        color: border_color_top.into(),
                        style: border_style_top,
                    },
                    left: BorderSide {
                        color: border_color_left.into(),
                        style: border_style_left,
                    },
                    right: BorderSide {
                        color: border_color_right.into(),
                        style: border_style_right,
                    },
                    bottom: BorderSide {
                        color: border_color_bottom.into(),
                        style: border_style_bottom,
                    },
                    radius: border_radius.and_then(|r| Some(r.0)),
                });

                Some((border_widths, border_details))
            }
        }
    }
}

const DEFAULT_BORDER_STYLE: BorderStyle = BorderStyle::Solid;
const DEFAULT_BORDER_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

/// Represents a `box-shadow` attribute.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBoxShadow {
    pub top: Option<Option<BoxShadowPreDisplayItem>>,
    pub left: Option<Option<BoxShadowPreDisplayItem>>,
    pub bottom: Option<Option<BoxShadowPreDisplayItem>>,
    pub right: Option<Option<BoxShadowPreDisplayItem>>,
}

merge_struct!(StyleBoxShadow);
struct_all!(StyleBoxShadow, Option<BoxShadowPreDisplayItem>);

// missing StyleBorderRadius & LayoutRect
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BoxShadowPreDisplayItem {
    pub offset: [PixelValue; 2],
    pub color: ColorU,
    pub blur_radius: PixelValue,
    pub spread_radius: PixelValue,
    pub clip_mode: BoxShadowClipMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleBackground {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    Image(CssImageId),
    NoBackground,
}

impl<'a> From<CssImageId> for StyleBackground {
    fn from(id: CssImageId) -> Self {
        StyleBackground::Image(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinearGradient {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RadialGradient {
    pub shape: Shape,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    Angle(FloatValue),
    FromTo(DirectionCorner, DirectionCorner),
}

impl Direction {
    /// Calculates the points of the gradient stops for angled linear gradients
    pub fn to_points(&self, rect: &LayoutRect) -> (LayoutPoint, LayoutPoint) {
        match self {
            Direction::Angle(deg) => {
                // note: assumes that the LayoutRect has positive sides

                // see: https://hugogiraudel.com/2013/02/04/css-gradients/

                let deg = deg.get(); // FloatValue -> f32

                let deg = -deg; // negate winding direction

                let width_half = rect.size.width as usize / 2;
                let height_half = rect.size.height as usize / 2;

                // hypotenuse_len is the length of the center of the rect to the corners
                let hypotenuse_len =
                    (((width_half * width_half) + (height_half * height_half)) as f64).sqrt();

                // The corner also serves to determine what quadrant we're in
                // Get the quadrant (corner) the angle is in and get the degree associated
                // with that corner.

                let angle_to_top_left =
                    (height_half as f64 / width_half as f64).atan().to_degrees();

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
                } else
                /* deg > 270.0 && deg < 360.0 */
                {
                    // top right corner
                    270.0 + angle_to_top_left
                };

                // assuming deg = 36deg, then degree_diff_to_corner = 9deg
                let degree_diff_to_corner = ending_point_degrees - deg as f64;

                // Searched_len is the distance between the center of the rect and the
                // ending point of the gradient
                let searched_len =
                    (hypotenuse_len * degree_diff_to_corner.to_radians().cos()).abs();

                // TODO: This searched_len is incorrect...

                // Once we have the length, we can simply rotate the length by the angle,
                // then translate it to the center of the rect
                let dx = deg.to_radians().sin() * searched_len as f32;
                let dy = deg.to_radians().cos() * searched_len as f32;

                let start_point_location = LayoutPoint {
                    x: width_half as f32 + dx,
                    y: height_half as f32 + dy,
                };
                let end_point_location = LayoutPoint {
                    x: width_half as f32 - dx,
                    y: height_half as f32 - dy,
                };

                (start_point_location, end_point_location)
            }
            Direction::FromTo(from, to) => (from.to_point(rect), to.to_point(rect)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    Ellipse,
    Circle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum StyleCursor {
    /// `alias`
    Alias,
    /// `all-scroll`
    AllScroll,
    /// `cell`
    Cell,
    /// `col-resize`
    ColResize,
    /// `context-menu`
    ContextMenu,
    /// `copy`
    Copy,
    /// `crosshair`
    Crosshair,
    /// `default` - note: called "arrow" in winit
    Default,
    /// `e-resize`
    EResize,
    /// `ew-resize`
    EwResize,
    /// `grab`
    Grab,
    /// `grabbing`
    Grabbing,
    /// `help`
    Help,
    /// `move`
    Move,
    /// `n-resize`
    NResize,
    /// `ns-resize`
    NsResize,
    /// `nesw-resize`
    NeswResize,
    /// `nwse-resize`
    NwseResize,
    /// `pointer` - note: called "hand" in winit
    Pointer,
    /// `progress`
    Progress,
    /// `row-resize`
    RowResize,
    /// `s-resize`
    SResize,
    /// `se-resize`
    SeResize,
    /// `text`
    Text,
    /// `unset`
    Unset,
    /// `vertical-text`
    VerticalText,
    /// `w-resize`
    WResize,
    /// `wait`
    Wait,
    /// `zoom-in`
    ZoomIn,
    /// `zoom-out`
    ZoomOut,
}

impl Default for StyleCursor {
    fn default() -> StyleCursor {
        StyleCursor::Default
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
            _ => None,
        }
    }

    pub fn to_point(&self, rect: &LayoutRect) -> LayoutPoint {
        use self::DirectionCorner::*;
        match *self {
            Right => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height / 2.0,
            },
            Left => LayoutPoint {
                x: 0.0,
                y: rect.size.height / 2.0,
            },
            Top => LayoutPoint {
                x: rect.size.width / 2.0,
                y: 0.0,
            },
            Bottom => LayoutPoint {
                x: rect.size.width / 2.0,
                y: rect.size.height,
            },
            TopRight => LayoutPoint {
                x: rect.size.width,
                y: 0.0,
            },
            TopLeft => LayoutPoint { x: 0.0, y: 0.0 },
            BottomRight => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height,
            },
            BottomLeft => LayoutPoint {
                x: 0.0,
                y: rect.size.height,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum BackgroundType {
    LinearGradient,
    RepeatingLinearGradient,
    RadialGradient,
    RepeatingRadialGradient,
    Image,
}

/// Note: In theory, we could take a reference here,
/// but this leads to horrible lifetime issues.
///
/// Ownership allows the `Css` struct to be independent
/// of the original source text. For example, when parsing a style
/// from CSS, the original string can be deallocated afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CssImageId(pub String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct GradientStopPre {
    pub offset: Option<PercentageValue>, // this is set to None if there was no offset that could be parsed
    pub color: ColorU,
}

/// Represents a `width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutWidth(pub PixelValue);
/// Represents a `min-width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMinWidth(pub PixelValue);
/// Represents a `max-width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMaxWidth(pub PixelValue);
/// Represents a `height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutHeight(pub PixelValue);
/// Represents a `min-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMinHeight(pub PixelValue);
/// Represents a `max-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMaxHeight(pub PixelValue);

/// Represents a `top` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutTop(pub PixelValue);
/// Represents a `left` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutLeft(pub PixelValue);
/// Represents a `right` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutRight(pub PixelValue);
/// Represents a `bottom` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutBottom(pub PixelValue);

/// Represents a `flex-grow` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutFlexGrow(pub FloatValue);
/// Represents a `flex-shrink` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutFlexShrink(pub FloatValue);

/// Represents a `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Represents a `line-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleLineHeight(pub PercentageValue);
/// Represents a `letter-spacing` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleLetterSpacing(pub PixelValue);

/// Same as the `LayoutDirection`, but without the `-reverse` properties, used in the layout solver,
/// makes decisions based on horizontal / vertical direction easier to write.
/// Use `LayoutDirection::get_axis()` to get the axis for a given `LayoutDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Represents a `position` attribute - default: `Static`
///
/// NOTE: No inline positioning is supported.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Represents a `flex-wrap` attribute - default: `Wrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutWrap {
    Wrap,
    NoWrap,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        LayoutWrap::Wrap
    }
}

/// Represents a `justify-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Represents a `align-items` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Represents a `align-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Represents a `overflow` attribute
///
/// NOTE: This is split into `NotModified` and `Modified`
/// in order to be able to "merge" `overflow-x` and `overflow-y`
/// into one property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TextOverflowBehaviour {
    NotModified,
    Modified(TextOverflowBehaviourInner),
}

impl TextOverflowBehaviour {
    pub fn can_overflow(&self) -> bool {
        use self::TextOverflowBehaviour::*;
        use self::TextOverflowBehaviourInner::*;
        match self {
            Modified(m) => match m {
                Scroll | Auto => true,
                Hidden | Visible => false,
            },
            // default: allow horizontal overflow
            NotModified => false,
        }
    }

    pub fn clips_children(&self) -> bool {
        use self::TextOverflowBehaviour::*;
        use self::TextOverflowBehaviourInner::*;
        match self {
            Modified(m) => match m {
                Scroll | Auto | Hidden => true,
                Visible => false,
            },
            // default: allow horizontal overflow
            NotModified => false,
        }
    }
}

impl Default for TextOverflowBehaviour {
    fn default() -> Self {
        TextOverflowBehaviour::NotModified
    }
}

/// Represents a `overflow-x` or `overflow-y` property, see
/// [`TextOverflowBehaviour`](./struct.TextOverflowBehaviour.html) - default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum StyleTextAlignmentHorz {
    Left,
    Center,
    Right,
}

impl Default for StyleTextAlignmentHorz {
    fn default() -> Self {
        StyleTextAlignmentHorz::Center
    }
}

/// Vertical text alignment enum (top, center, bottom) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum StyleTextAlignmentVert {
    Top,
    Center,
    Bottom,
}

impl Default for StyleTextAlignmentVert {
    fn default() -> Self {
        StyleTextAlignmentVert::Center
    }
}

/// Stylistic options of the rectangle that don't influence the layout
/// (todo: border-box?)
#[derive(Default, Debug, Clone, PartialEq, Hash)]
pub struct RectStyle {
    /// Background color of this rectangle
    pub background_color: Option<StyleBackgroundColor>,
    /// Shadow color
    pub box_shadow: Option<StyleBoxShadow>,
    /// Gradient (location) + stops
    pub background: Option<StyleBackground>,
    /// Border
    pub border: Option<StyleBorder>,
    /// Border radius
    pub border_radius: Option<StyleBorderRadius>,
    /// Font size
    pub font_size: Option<StyleFontSize>,
    /// Font name / family
    pub font_family: Option<StyleFontFamily>,
    /// Text color
    pub font_color: Option<StyleTextColor>,
    /// Text alignment
    pub text_align: Option<StyleTextAlignmentHorz>,
    /// Text overflow behaviour
    pub overflow: Option<LayoutOverflow>,
    /// `line-height` property
    pub line_height: Option<StyleLineHeight>,
    /// `letter-spacing` property (modifies the width and height)
    pub letter_spacing: Option<StyleLetterSpacing>,
}

impl_pixel_value!(StyleLetterSpacing);

// Layout constraints for a given rectangle, such as "width", "min-width", "height", etc.
#[derive(Default, Debug, Copy, Clone, PartialEq, Hash)]
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

impl_pixel_value!(LayoutWidth);

impl_pixel_value!(LayoutHeight);
impl_pixel_value!(LayoutMinHeight);
impl_pixel_value!(LayoutMinWidth);
impl_pixel_value!(LayoutMaxWidth);
impl_pixel_value!(LayoutMaxHeight);

impl_pixel_value!(LayoutTop);
impl_pixel_value!(LayoutBottom);
impl_pixel_value!(LayoutRight);
impl_pixel_value!(LayoutLeft);

/// Represents a `font-size` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleFontSize(pub PixelValue);

impl_pixel_value!(StyleFontSize);

impl StyleFontSize {
    pub fn to_pixels(&self) -> f32 {
        self.0.to_pixels()
    }
}

/// Represents a `font-family` attribute
#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub struct StyleFontFamily {
    // fonts in order of precedence, i.e. "Webly Sleeky UI", "monospace", etc.
    pub fonts: Vec<FontId>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum FontId {
    BuiltinFont(String),
    ExternalFont(String),
}
