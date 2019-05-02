//! Provides a public API with datatypes used to describe style properties of DOM nodes.

use std::collections::BTreeMap;
use std::fmt;

/// Currently hard-coded: Height of one em in pixels
const EM_HEIGHT: f32 = 16.0;
/// WebRender measures in points, not in pixels!
const PT_TO_PX: f32 = 96.0 / 72.0;

const COMBINED_CSS_PROPERTIES_KEY_MAP: [(CombinedCssProperty, &'static str);11] = [
    (CombinedCssProperty::Background, "background"),
    (CombinedCssProperty::BorderRadius, "border-radius"),
    (CombinedCssProperty::Overflow, "overflow"),
    (CombinedCssProperty::Padding, "padding"),
    (CombinedCssProperty::Margin, "margin"),
    (CombinedCssProperty::Border, "border"),
    (CombinedCssProperty::BorderLeft, "border-left"),
    (CombinedCssProperty::BorderRight, "border-right"),
    (CombinedCssProperty::BorderTop, "border-top"),
    (CombinedCssProperty::BorderBottom, "border-bottom"),
    (CombinedCssProperty::BoxShadow, "box-shadow"),
];

/// Map between CSS keys and a statically typed enum
const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &'static str);62] = [

    (CssPropertyType::TextColor,            "color"),
    (CssPropertyType::FontSize,             "font-size"),
    (CssPropertyType::FontFamily,           "font-family"),
    (CssPropertyType::TextAlign,            "text-align"),

    (CssPropertyType::LetterSpacing,        "letter-spacing"),
    (CssPropertyType::LineHeight,           "line-height"),
    (CssPropertyType::WordSpacing,          "word-spacing"),
    (CssPropertyType::TabWidth,             "tab-width"),
    (CssPropertyType::Cursor,               "cursor"),

    (CssPropertyType::Width,                "width"),
    (CssPropertyType::Height,               "height"),
    (CssPropertyType::MinWidth,             "min-width"),
    (CssPropertyType::MinHeight,            "min-height"),
    (CssPropertyType::MaxWidth,             "max-width"),
    (CssPropertyType::MaxHeight,            "max-height"),

    (CssPropertyType::Position,             "position"),
    (CssPropertyType::Top,                  "top"),
    (CssPropertyType::Right,                "right"),
    (CssPropertyType::Left,                 "left"),
    (CssPropertyType::Bottom,               "bottom"),

    (CssPropertyType::FlexWrap,             "flex-wrap"),
    (CssPropertyType::FlexDirection,        "flex-direction"),
    (CssPropertyType::FlexGrow,             "flex-grow"),
    (CssPropertyType::FlexShrink,           "flex-shrink"),
    (CssPropertyType::JustifyContent,       "justify-content"),
    (CssPropertyType::AlignItems,           "align-items"),
    (CssPropertyType::AlignContent,         "align-content"),

    (CssPropertyType::OverflowX,            "overflow-x"),
    (CssPropertyType::OverflowY,            "overflow-y"),

    (CssPropertyType::PaddingTop,           "padding-top"),
    (CssPropertyType::PaddingLeft,          "padding-left"),
    (CssPropertyType::PaddingRight,         "padding-right"),
    (CssPropertyType::PaddingBottom,        "padding-bottom"),

    (CssPropertyType::MarginTop,            "margin-top"),
    (CssPropertyType::MarginLeft,           "margin-left"),
    (CssPropertyType::MarginRight,          "margin-right"),
    (CssPropertyType::MarginBottom,         "margin-bottom"),

    (CssPropertyType::BackgroundAttachment, "background-attachment"),
    (CssPropertyType::BackgroundPosition,   "background-position"),
    (CssPropertyType::BackgroundSize,       "background-size"),
    (CssPropertyType::BackgroundRepeat,     "background-repeat"),
    (CssPropertyType::BackgroundImage,      "background-image"),

    (CssPropertyType::BorderTopLeftRadius,      "border-top-left-radius"),
    (CssPropertyType::BorderTopRightRadius,     "border-top-right-radius"),
    (CssPropertyType::BorderBottomLeftRadius,   "border-bottom-left-radius"),
    (CssPropertyType::BorderBottomRightRadius,  "border-bottom-right-radius"),

    (CssPropertyType::BorderTopColor,           "border-top-color"),
    (CssPropertyType::BorderRightColor,         "border-right-color"),
    (CssPropertyType::BorderLeftColor,          "border-left-color"),
    (CssPropertyType::BorderBottomColor,        "border-bottom-color"),

    (CssPropertyType::BorderTopStyle,           "border-top-style"),
    (CssPropertyType::BorderRightStyle,         "border-right-style"),
    (CssPropertyType::BorderLeftStyle,          "border-left-style"),
    (CssPropertyType::BorderBottomStyle,        "border-bottom-style"),

    (CssPropertyType::BorderTopWidth,           "border-top-width"),
    (CssPropertyType::BorderRightWidth,         "border-right-width"),
    (CssPropertyType::BorderLeftWidth,          "border-left-width"),
    (CssPropertyType::BorderBottomWidth,        "border-bottom-width"),

    (CssPropertyType::BoxShadowTop, "box-shadow-top"),
    (CssPropertyType::BoxShadowRight, "box-shadow-right"),
    (CssPropertyType::BoxShadowLeft, "box-shadow-left"),
    (CssPropertyType::BoxShadowBottom, "box-shadow-bottom"),
];

// The following types are present in webrender, however, azul-css should not
// depend on webrender, just to have the same types, azul-css should be a standalone crate.

/// Only used for calculations: Rectangle (x, y, width, height) in layout space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutRect { pub origin: LayoutPoint, pub size: LayoutSize }
/// Only used for calculations: Size (width, height) in layout space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutSize { pub width: f32, pub height: f32 }
/// Only used for calculations: Point coordinate (x, y) in layout space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutPoint { pub x: f32, pub y: f32 }

impl LayoutSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
        }
    }
    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Represents a parsed pair of `5px, 10px` values - useful for border radius calculation
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct PixelSize { pub width: PixelValue, pub height: PixelValue }

impl PixelSize {

    pub const fn new(width: PixelValue, height: PixelValue) -> Self {
        Self {
            width,
            height,
        }
    }

    pub const fn zero() -> Self {
        Self::new(PixelValue::const_px(0), PixelValue::const_px(0))
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
pub struct ColorU { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

impl ColorU {
    pub const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };
    pub const TRANSPARENT: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders ColorF)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }

impl ColorF {
    pub const WHITE: ColorF = ColorF { r: 1.0, g: 1.0, b: 1.0, a: 0.0 };
    pub const BLACK: ColorF = ColorF { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const TRANSPARENT: ColorF = ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
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
    pub radius: Option<StyleBorderRadius>,
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

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle::Soldid;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct NinePatchBorder {
    // not implemented or parse-able yet, so no fields!
}

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
macro_rules! impl_pixel_value {($struct:ident) => (
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

    impl ::std::fmt::Debug for $struct {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "{}({:?})", stringify!($struct), self.0)
        }
    }
)}

macro_rules! impl_percentage_value{($struct:ident) => (
    impl ::std::fmt::Debug for $struct {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "{}({:?})", stringify!($struct), self.0)
        }
    }
)}

macro_rules! impl_float_value{($struct:ident) => (
    impl ::std::fmt::Debug for $struct {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "{}({:?})", stringify!($struct), self.0)
        }
    }
)}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CombinedCssProperty {
    Background,
    BorderRadius,
    Overflow,
    Margin,
    Border,
    BorderLeft,
    BorderRight,
    BorderTop,
    BorderBottom,
    Padding,
    BoxShadow,
}

impl CombinedCssProperty {

    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_css::{CombinedCssProperty, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(Some(CssPropertyType::Border), CssPropertyType::from_str("border", &map));
    /// ```
    pub fn from_str(input: &str, map: &BTreeMap<&'static str, Self>) -> Option<Self> {
        let input = input.trim();
        map.get(input).map(|x| *x)
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
    pub fn to_str(&self, map: &BTreeMap<&'static str, Self>) -> &'static str {
        map.iter().find(|(_, v)| *v == self).map(|(k, _)| k).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CssKeyMap {
    // Contains all keys that have no shorthand
    pub non_shorthands: BTreeMap<&'static str, CssPropertyType>,
    // Contains all keys that act as a shorthand for other types
    pub shorthands: BTreeMap<&'static str, CombinedCssProperty>,
}

/// Returns a map useful for parsing the keys of CSS stylesheets
pub fn get_css_key_map() -> CssKeyMap {
    CssKeyMap {
        non_shorthands: CSS_PROPERTY_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect(),
        shorthands: COMBINED_CSS_PROPERTIES_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect(),
    }
}

/// Represents a CSS key (for example `"border-radius"` => `BorderRadius`).
/// You can also derive this key from a `CssProperty` by calling `CssProperty::get_type()`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyType {

    TextColor,
    FontSize,
    FontFamily,
    TextAlign,

    LetterSpacing,
    LineHeight,
    WordSpacing,
    TabWidth,
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

    BackgroundAttachment,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    BackgroundImage,

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
}

impl CssPropertyType {

    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
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

    /// Returns the original string that was used to construct this `CssPropertyType`.
    pub fn to_str(&self, map: &BTreeMap<&'static str, Self>) -> &'static str {
        map.iter().find(|(_, v)| *v == self).and_then(|(k, _)| Some(k)).unwrap()
    }

    /// Returns whether this property will be inherited during cascading
    pub fn is_inheritable(&self) -> bool {
        use self::CssPropertyType::*;
        match self {
            | TextColor
            | FontFamily
            | FontSize
            | LineHeight
            | TextAlign => true,
            _ => false,
        }
    }

    /// Returns whether this property can trigger a re-layout (important for incremental layout and caching layouted DOMs).
    pub fn can_trigger_relayout(&self) -> bool {

        use self::CssPropertyType::*;

        // Since the border can be larger than the content,
        // in which case the content needs to be re-layouted, assume true for Border

        // FontFamily, FontSize, LetterSpacing and LineHeight can affect
        // the text layout and therefore the screen layout

        match self {
            | TextColor
            | Cursor
            | BackgroundAttachment
            | BackgroundPosition
            | BackgroundSize
            | BackgroundRepeat
            | BackgroundImage
            | BorderTopLeftRadius
            | BorderTopRightRadius
            | BorderBottomLeftRadius
            | BorderBottomRightRadius
            | BorderTopColor
            | BorderRightColor
            | BorderLeftColor
            | BorderBottomColor
            | BorderTopStyle
            | BorderRightStyle
            | BorderLeftStyle
            | BorderBottomStyle
            | BoxShadowLeft
            | BoxShadowRight
            | BoxShadowTop
            | BoxShadowBottom
            => false,
            _ => true,
        }
    }
}

impl fmt::Display for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let key = CSS_PROPERTY_KEY_MAP.iter().find(|(v, _)| *v == *self).and_then(|(k, _)| Some(k)).unwrap();
        write!(f, "{}", key)
    }
}

/// Represents one parsed CSS key-value pair, such as `"width: 20px"` => `CssProperty::Width(LayoutWidth::px(20.0))`
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CssProperty {

    TextColor(StyleTextColor),
    FontSize(StyleFontSize),
    FontFamily(StyleFontFamily),
    TextAlign(StyleTextAlignmentHorz),

    LetterSpacing(StyleLetterSpacing),
    LineHeight(StyleLineHeight),
    WordSpacing(StyleWordSpacing),
    TabWidth(StyleTabWidth),
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

    FlexWrap(LayoutWrap),
    FlexDirection(LayoutDirection),
    FlexGrow(LayoutFlexGrow),
    FlexShrink(LayoutFlexShrink),
    JustifyContent(LayoutJustifyContent),
    AlignItems(LayoutAlignItems),
    AlignContent(LayoutAlignContent),

    BackgroundAttachment(StyleBackgroundAttachment),
    BackgroundPosition(StyleBackgroundPosition),
    BackgroundSize(StyleBackgroundSize),
    BackgroundRepeat(StyleBackgroundRepeat),

    OverflowX(Overflow),
    OverflowY(Overflow),

    PaddingTop(LayoutPaddingTop),
    PaddingLeft(LayoutPaddingLeft),
    PaddingRight(LayoutPaddingRight),
    PaddingBottom(LayoutPaddingBottom),

    MarginTop(LayoutMarginTop),
    MarginLeft(LayoutMarginLeft),
    MarginRight(LayoutMarginRight),
    MarginBottom(LayoutMarginBottom),

    BorderTopLeftRadius(StyleBorderTopLeftRadius),
    BorderTopRightRadius(StyleBorderTopRightRadius),
    BorderBottomLeftRadius(StyleBorderBottomLeftRadius),
    BorderBottomRightRadius(StyleBorderBottomRightRadius),

    BorderTopColor(StyleBorderTopColor),
    BorderRightColor(StyleBorderRightColor),
    BorderLeftColor(StyleBorderLeftColor),
    BorderBottomColor(StyleBorderBottomColor),

    BorderTopStyle(StyleBorderTopStyle),
    BorderRightStyle(StyleBorderRightStyle),
    BorderLeftStyle(StyleBorderLeftStyle),
    BorderBottomStyle(StyleBorderBottomStyle),

    BorderTopWidth(StyleBorderTopWidth),
    BorderRightWidth(StyleBorderRightWidth),
    BorderLeftWidth(StyleBorderLeftWidth),
    BorderBottomWidth(StyleBorderBottomWidth),

    BoxShadowLeft(BoxShadowPreDisplayItem),
    BoxShadowRight(BoxShadowPreDisplayItem),
    BoxShadowTop(BoxShadowPreDisplayItem),
    BoxShadowBottom(BoxShadowPreDisplayItem),
}

impl CssProperty {

    /// Return the type (key) of this property as a statically typed enum
    pub fn get_type(&self) -> CssPropertyType {
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
            CssProperty::BackgroundAttachment(_) => CssPropertyType::BackgroundAttachment,
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
        }
    }
}

impl_from!(StyleTextColor, CssProperty::TextColor);
impl_from!(StyleFontSize, CssProperty::FontSize);
impl_from!(StyleFontFamily, CssProperty::FontFamily);
impl_from!(StyleTextAlignmentHorz, CssProperty::TextAlign);
impl_from!(StyleLetterSpacing, CssProperty::LetterSpacing);
impl_from!(StyleLineHeight, CssProperty::LineHeight);
impl_from!(StyleWordSpacing, CssProperty::WordSpacing);
impl_from!(StyleTabWidth, CssProperty::TabWidth);
impl_from!(StyleCursor, CssProperty::Cursor);
impl_from!(LayoutWidth, CssProperty::Width);
impl_from!(LayoutHeight, CssProperty::Height);
impl_from!(LayoutMinWidth, CssProperty::MinWidth);
impl_from!(LayoutMinHeight, CssProperty::MinHeight);
impl_from!(LayoutMaxWidth, CssProperty::MaxWidth);
impl_from!(LayoutMaxHeight, CssProperty::MaxHeight);
impl_from!(LayoutPosition, CssProperty::Position);
impl_from!(LayoutTop, CssProperty::Top);
impl_from!(LayoutRight, CssProperty::Right);
impl_from!(LayoutLeft, CssProperty::Left);
impl_from!(LayoutBottom, CssProperty::Bottom);
impl_from!(LayoutWrap, CssProperty::FlexWrap);
impl_from!(LayoutDirection, CssProperty::FlexDirection);
impl_from!(LayoutFlexGrow, CssProperty::FlexGrow);
impl_from!(LayoutFlexShrink, CssProperty::FlexShrink);
impl_from!(LayoutJustifyContent, CssProperty::JustifyContent);
impl_from!(LayoutAlignItems, CssProperty::AlignItems);
impl_from!(LayoutAlignContent, CssProperty::AlignContent);
impl_from!(StyleBackgroundAttachment, CssProperty::BackgroundAttachment);
impl_from!(StyleBackgroundPosition, CssProperty::BackgroundPosition);
impl_from!(StyleBackgroundSize, CssProperty::BackgroundSize);
impl_from!(StyleBackgroundRepeat, CssProperty::BackgroundRepeat);
impl_from!(LayoutPaddingTop, CssProperty::PaddingTop);
impl_from!(LayoutPaddingLeft, CssProperty::PaddingLeft);
impl_from!(LayoutPaddingRight, CssProperty::PaddingRight);
impl_from!(LayoutPaddingBottom, CssProperty::PaddingBottom);
impl_from!(LayoutMarginTop, CssProperty::MarginTop);
impl_from!(LayoutMarginLeft, CssProperty::MarginLeft);
impl_from!(LayoutMarginRight, CssProperty::MarginRight);
impl_from!(LayoutMarginBottom, CssProperty::MarginBottom);
impl_from!(StyleBorderTopLeftRadius, CssProperty::BorderTopLeftRadius);
impl_from!(StyleBorderTopRightRadius, CssProperty::BorderTopRightRadius);
impl_from!(StyleBorderBottomLeftRadius, CssProperty::BorderBottomLeftRadius);
impl_from!(StyleBorderBottomRightRadius, CssProperty::BorderBottomRightRadius);
impl_from!(StyleBorderTopColor, CssProperty::BorderTopColor);
impl_from!(StyleBorderRightColor, CssProperty::BorderRightColor);
impl_from!(StyleBorderLeftColor, CssProperty::BorderLeftColor);
impl_from!(StyleBorderBottomColor, CssProperty::BorderBottomColor);
impl_from!(StyleBorderTopStyle, CssProperty::BorderTopStyle);
impl_from!(StyleBorderRightStyle, CssProperty::BorderRightStyle);
impl_from!(StyleBorderLeftStyle, CssProperty::BorderLeftStyle);
impl_from!(StyleBorderBottomStyle, CssProperty::BorderBottomStyle);
impl_from!(StyleBorderTopWidth, CssProperty::BorderTopWidth);
impl_from!(StyleBorderRightWidth, CssProperty::BorderRightWidth);
impl_from!(StyleBorderLeftWidth, CssProperty::BorderLeftWidth);
impl_from!(StyleBorderBottomWidth, CssProperty::BorderBottomWidth);

/// Multiplier for floating point accuracy. Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 5 digits, i.e
/// `1.00001 == 1.0`
const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

/// FloatValue, but associated with a certain metric (i.e. px, em, etc.)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

// Manual Debug implementation, because the auto-generated one is nearly unreadable
impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}{:?}", self.number, self.metric)
    }
}

impl fmt::Debug for SizeMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SizeMetric::*;
        match self {
            Px => write!(f, "px"),
            Pt => write!(f, "pt"),
            Em => write!(f, "pt"),
        }
    }
}

impl fmt::Debug for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl PixelValue {

    /// Same as `PixelValue::px()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_px(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Px, value)
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_em(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Em, value)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_pt(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric: metric,
            number: FloatValue::const_new(value),
        }
    }

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
            number: FloatValue::new(value),
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels(&self) -> f32 {
        match self.metric {
            SizeMetric::Px => { self.number.get() },
            SizeMetric::Pt => { (self.number.get()) * PT_TO_PX },
            SizeMetric::Em => { (self.number.get()) * EM_HEIGHT },
        }
    }
}

/// Wrapper around FloatValue, represents a percentage instead
/// of just being a regular floating-point value, i.e `5` = `5%`
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PercentageValue {
    number: FloatValue,
}

impl fmt::Debug for PercentageValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}%", self.get())
    }
}

impl PercentageValue {

    /// Same as `PercentageValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    pub const fn const_new(value: isize) -> Self {
        Self { number: FloatValue::const_new(value) }
    }

    pub fn new(value: f32) -> Self {
        Self { number: value.into() }
    }

    pub fn get(&self) -> f32 {
        self.number.get()
    }
}

/// Wrapper around an f32 value that is internally casted to an isize,
/// in order to provide hash-ability (to avoid numerical instability).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FloatValue {
    pub number: isize,
}

impl FloatValue {

    /// Same as `FloatValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    pub const fn const_new(value: isize)  -> Self {
        Self { number: value * FP_PRECISION_MULTIPLIER_CONST }
    }

    pub fn new(value: f32) -> Self {
        Self { number: (value * FP_PRECISION_MULTIPLIER) as isize }
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
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SizeMetric {
    Px,
    Pt,
    Em,
}

/// Represents a `background-size` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleBackgroundSize {
    ExactSize(PixelValue, PixelValue),
    Contain,
    Cover,
}

/// Represents a `background-position` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleBackgroundPosition {
    Corner(BackgroundPositionCorner),
    Exact(PixelValue),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackgroundPositionCorner {
    LeftTop,
    LeftCenter,
    LeftBottom,
    RightTop,
    RightCenter,
    RightBottom,
    CenterTop,
    CenterCenter,
    CenterBottom,
}

/// Represents a `background-repeat` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleBackgroundRepeat {
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY,
}

impl Default for StyleBackgroundRepeat {
    fn default() -> Self {
        StyleBackgroundRepeat::Repeat
    }
}

/// Represents a `color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleTextColor(pub ColorU);

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

macro_rules! struct_all {($struct_name:ident, $field_type:ty) => (
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
})}

/// Represents a `padding` attribute
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPadding {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

merge_struct!(LayoutPadding);
struct_all!(LayoutPadding, PixelValue);

/// Represents a parsed `padding` attribute
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMargin {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

merge_struct!(LayoutMargin);
struct_all!(LayoutMargin, PixelValue);

/// Wrapper for the `overflow-{x,y}` + `overflow` property
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutOverflow {
    pub horizontal: Option<Overflow>,
    pub vertical: Option<Overflow>,
}

impl LayoutOverflow {

    // "merges" two LayoutOverflow properties
    pub fn merge(a: &mut Option<Self>, b: &Self) {

        fn merge_property(p: &mut Option<Overflow>, other: &Option<Overflow>) {
            if *other == None {
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

    pub fn needs_horizontal_scrollbar(&self, currently_overflowing_horz: bool) -> bool {
        self.horizontal.unwrap_or_default().needs_scrollbar(currently_overflowing_horz)
    }

    pub fn needs_vertical_scrollbar(&self, currently_overflowing_vert: bool) -> bool {
        self.vertical.unwrap_or_default().needs_scrollbar(currently_overflowing_vert)
    }

    pub fn is_horizontal_overflow_visible(&self) -> bool {
        self.horizontal.unwrap_or_default().is_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(&self) -> bool {
        self.vertical.unwrap_or_default().is_overflow_visible()
    }
}

// -- TODO: Technically, border-radius can take two values for each corner!

/// Represents a `border-top-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderTopLeftRadius(pub PixelValue);
/// Represents a `border-left-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderBottomLeftRadius(pub PixelValue);
/// Represents a `border-right-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderTopRightRadius(pub PixelValue);
/// Represents a `border-bottom-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderBottomRightRadius(pub PixelValue);

impl_pixel_value!(StyleBorderTopLeftRadius);
impl_pixel_value!(StyleBorderBottomLeftRadius);
impl_pixel_value!(StyleBorderTopRightRadius);
impl_pixel_value!(StyleBorderBottomRightRadius);

/// Represents a `border-top-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderTopWidth(pub PixelValue);
/// Represents a `border-left-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderLeftWidth(pub PixelValue);
/// Represents a `border-right-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRightWidth(pub PixelValue);
/// Represents a `border-bottom-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderBottomWidth(pub PixelValue);

impl_pixel_value!(StyleBorderTopWidth);
impl_pixel_value!(StyleBorderLeftWidth);
impl_pixel_value!(StyleBorderRightWidth);
impl_pixel_value!(StyleBorderBottomWidth);

/// Represents a `border-top-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderTopStyle(pub BorderStyle);
/// Represents a `border-left-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderLeftStyle(pub BorderStyle);
/// Represents a `border-right-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRightStyle(pub BorderStyle);
/// Represents a `border-bottom-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderBottomStyle(pub BorderStyle);

/// Represents a `border-top-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderTopColor(pub ColorU);
/// Represents a `border-left-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderLeftColor(pub ColorU);
/// Represents a `border-right-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRightColor(pub ColorU);
/// Represents a `border-bottom-width` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderBottomColor(pub ColorU);

impl Default for StyleBorderTopColor { fn default() -> Self { StyleBorderTopColor(ColorU::BLACK) } }
impl Default for StyleBorderLeftColor { fn default() -> Self { StyleBorderLeftColor(ColorU::BLACK) } }
impl Default for StyleBorderRightColor { fn default() -> Self { StyleBorderRightColor(ColorU::BLACK) } }
impl Default for StyleBorderBottomColor { fn default() -> Self { StyleBorderBottomColor(ColorU::BLACK) } }

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorder {
    pub top: Option<StyleBorderSide>,
    pub left: Option<StyleBorderSide>,
    pub bottom: Option<StyleBorderSide>,
    pub right: Option<StyleBorderSide>,
}

merge_struct!(StyleBorder);
struct_all!(StyleBorder, StyleBorderSide);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

/// Represents a `box-shadow` attribute.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBoxShadow {
    pub top: Option<BoxShadowPreDisplayItem>,
    pub left: Option<BoxShadowPreDisplayItem>,
    pub bottom: Option<BoxShadowPreDisplayItem>,
    pub right: Option<BoxShadowPreDisplayItem>,
}

merge_struct!(StyleBoxShadow);
struct_all!(StyleBoxShadow, BoxShadowPreDisplayItem);

// missing StyleBorderRadius & LayoutRect
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoxShadowPreDisplayItem {
    pub offset: [PixelValue;2],
    pub color: ColorU,
    pub blur_radius: PixelValue,
    pub spread_radius: PixelValue,
    pub clip_mode: BoxShadowClipMode,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleBackgroundAttachment {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    Image(CssImageId),
    Color(ColorU),
}

impl StyleBackgroundAttachment {
    pub fn get_css_image_id(&self) -> Option<&CssImageId> {
        use self::StyleBackgroundAttachment::*;
        match self {
            Image(i) => Some(i),
            _ => None,
        }
    }
}

impl<'a> From<CssImageId> for StyleBackgroundAttachment {
    fn from(id: CssImageId) -> Self {
        StyleBackgroundAttachment::Image(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinearGradient {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadialGradient {
    pub shape: Shape,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

/// CSS direction (necessary for gradients). Can either be a fixed angle or
/// a direction ("to right" / "to left", etc.).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Angle(FloatValue),
    FromTo(DirectionCorner, DirectionCorner),
}

impl Direction {
    /// Calculates the points of the gradient stops for angled linear gradients
    pub fn to_points(&self, rect: &LayoutRect)
    -> (LayoutPoint, LayoutPoint)
    {
        match self {
            Direction::Angle(deg) => {
                // note: assumes that the LayoutRect has positive sides

                // see: https://hugogiraudel.com/2013/02/04/css-gradients/

                let deg = deg.get(); // FloatValue -> f32

                let deg = -deg; // negate winding direction

                let width_half = rect.size.width as usize / 2;
                let height_half = rect.size.height as usize / 2;

                // hypotenuse_len is the length of the center of the rect to the corners
                let hypotenuse_len = (((width_half * width_half) + (height_half * height_half)) as f64).sqrt();

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
                let searched_len = (hypotenuse_len * degree_diff_to_corner.to_radians().cos()).abs();

                // TODO: This searched_len is incorrect...

                // Once we have the length, we can simply rotate the length by the angle,
                // then translate it to the center of the rect
                let dx = deg.to_radians().sin() * searched_len as f32;
                let dy = deg.to_radians().cos() * searched_len as f32;

                let start_point_location = LayoutPoint { x: width_half as f32 + dx, y: height_half as f32 + dy };
                let end_point_location = LayoutPoint { x: width_half as f32 - dx, y: height_half as f32 - dy };

                (start_point_location, end_point_location)
            },
            Direction::FromTo(from, to) => {
                (from.to_point(rect), to.to_point(rect))
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shape {
    Ellipse,
    Circle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    pub fn to_point(&self, rect: &LayoutRect) -> LayoutPoint
    {
        use self::DirectionCorner::*;
        match *self {
            Right       => LayoutPoint { x: rect.size.width,          y: rect.size.height / 2.0     },
            Left        => LayoutPoint { x: 0.0,                      y: rect.size.height / 2.0     },
            Top         => LayoutPoint { x: rect.size.width / 2.0,    y: 0.0                        },
            Bottom      => LayoutPoint { x: rect.size.width / 2.0,    y: rect.size.height           },
            TopRight    => LayoutPoint { x: rect.size.width,          y: 0.0                        },
            TopLeft     => LayoutPoint { x: 0.0,                      y: 0.0                        },
            BottomRight => LayoutPoint { x: rect.size.width,          y: rect.size.height           },
            BottomLeft  => LayoutPoint { x: 0.0,                      y: rect.size.height           },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackgroundType {
    Color,
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CssImageId(pub String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GradientStopPre {
    // this is set to None if there was no offset that could be parsed
    pub offset: Option<PercentageValue>,
    pub color: ColorU,
}

/// Represents a `width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutWidth(pub PixelValue);
/// Represents a `min-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMinWidth(pub PixelValue);
/// Represents a `max-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMaxWidth(pub PixelValue);
/// Represents a `height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutHeight(pub PixelValue);
/// Represents a `min-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMinHeight(pub PixelValue);
/// Represents a `max-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMaxHeight(pub PixelValue);

impl_pixel_value!(LayoutWidth);
impl_pixel_value!(LayoutHeight);
impl_pixel_value!(LayoutMinHeight);
impl_pixel_value!(LayoutMinWidth);
impl_pixel_value!(LayoutMaxWidth);
impl_pixel_value!(LayoutMaxHeight);

/// Represents a `top` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutTop(pub PixelValue);
/// Represents a `left` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutLeft(pub PixelValue);
/// Represents a `right` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutRight(pub PixelValue);
/// Represents a `bottom` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutBottom(pub PixelValue);

impl_pixel_value!(LayoutTop);
impl_pixel_value!(LayoutBottom);
impl_pixel_value!(LayoutRight);
impl_pixel_value!(LayoutLeft);

/// Represents a `padding-top` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPaddingTop(pub PixelValue);
/// Represents a `padding-left` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPaddingLeft(pub PixelValue);
/// Represents a `padding-right` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPaddingRight(pub PixelValue);
/// Represents a `padding-bottom` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPaddingBottom(pub PixelValue);

impl_pixel_value!(LayoutPaddingTop);
impl_pixel_value!(LayoutPaddingBottom);
impl_pixel_value!(LayoutPaddingRight);
impl_pixel_value!(LayoutPaddingLeft);

/// Represents a `padding-top` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMarginTop(pub PixelValue);
/// Represents a `padding-left` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMarginLeft(pub PixelValue);
/// Represents a `padding-right` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMarginRight(pub PixelValue);
/// Represents a `padding-bottom` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMarginBottom(pub PixelValue);

impl_pixel_value!(LayoutMarginTop);
impl_pixel_value!(LayoutMarginBottom);
impl_pixel_value!(LayoutMarginRight);
impl_pixel_value!(LayoutMarginLeft);

/// Represents a `flex-grow` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutFlexGrow(pub FloatValue);
/// Represents a `flex-shrink` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutFlexShrink(pub FloatValue);

impl_float_value!(LayoutFlexGrow);
impl_float_value!(LayoutFlexShrink);

/// Represents a `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for LayoutDirection {
    fn default() -> Self {
        LayoutDirection::Column
    }
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

/// Represents a `line-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleLineHeight(pub PercentageValue);
/// Represents a `tab-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleTabWidth(pub PercentageValue);
/// Represents a `letter-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleLetterSpacing(pub PixelValue);
/// Represents a `word-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleWordSpacing(pub PixelValue);

impl_percentage_value!(StyleTabWidth);
impl_percentage_value!(StyleLineHeight);

/// Same as the `LayoutDirection`, but without the `-reverse` properties, used in the layout solver,
/// makes decisions based on horizontal / vertical direction easier to write.
/// Use `LayoutDirection::get_axis()` to get the axis for a given `LayoutDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

/// Represents a `position` attribute - default: `Static`
///
/// NOTE: No inline positioning is supported.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Represents a `flex-wrap` attribute - default: `Wrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Represents a `overflow-x` or `overflow-y` property, see
/// [`TextOverflowBehaviour`](./struct.TextOverflowBehaviour.html) - default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Overflow {
    /// Always shows a scroll bar, overflows on scroll
    Scroll,
    /// Does not show a scroll bar by default, only when text is overflowing
    Auto,
    /// Never shows a scroll bar, simply clips text
    Hidden,
    /// Doesn't show a scroll bar, simply overflows the text
    Visible,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Auto
    }
}

impl Overflow {

    /// Returns whether this overflow value needs to display the scrollbars.
    ///
    /// - `overflow:scroll` always shows the scrollbar
    /// - `overflow:auto` only shows the scrollbar when the content is currently overflowing
    /// - `overflow:hidden` and `overflow:visible` do not show any scrollbars
    pub fn needs_scrollbar(&self, currently_overflowing: bool) -> bool {
        use self::Overflow::*;
        match self {
            Scroll => true,
            Auto => currently_overflowing,
            Hidden | Visible => false,
        }
    }

    /// Returns whether this is an `overflow:visible` node
    /// (the only overflow type that doesn't clip its children)
    pub fn is_overflow_visible(&self) -> bool {
        *self == Overflow::Visible
    }
}

/// Horizontal text alignment enum (left, center, right) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RectStyle {
    pub background_attachment: Option<StyleBackgroundAttachment>,
    pub background_position: Option<StyleBackgroundSize>,
    pub background_size: Option<StyleBackgroundSize>,
    pub background_repeat: Option<StyleBackgroundRepeat>,
    pub box_shadow: Option<StyleBoxShadow>,
    pub border: Option<StyleBorder>,
    pub border_radius: Option<StyleBorderRadius>,
    pub font_size: Option<StyleFontSize>,
    pub font_family: Option<StyleFontFamily>,
    pub font_color: Option<StyleTextColor>,
    pub text_align: Option<StyleTextAlignmentHorz,>,
    pub overflow: Option<LayoutOverflow>,
    pub line_height: Option<StyleLineHeight>,
    pub letter_spacing: Option<StyleLetterSpacing>,
    pub word_spacing: Option<StyleWordSpacing>,
    pub tab_width: Option<StyleTabWidth>,
}

// Layout constraints for a given rectangle, such as "width", "min-width", "height", etc.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    pub overflow: Option<LayoutOverflow>,

    pub direction: Option<LayoutDirection>,
    pub wrap: Option<LayoutWrap>,
    pub flex_grow: Option<LayoutFlexGrow>,
    pub flex_shrink: Option<LayoutFlexShrink>,
    pub justify_content: Option<LayoutJustifyContent>,
    pub align_items: Option<LayoutAlignItems>,
    pub align_content: Option<LayoutAlignContent>,
}

impl_pixel_value!(StyleLetterSpacing);
impl_pixel_value!(StyleWordSpacing);

impl RectStyle {

    pub fn get_horizontal_scrollbar_style(&self) -> ScrollbarInfo {
        ScrollbarInfo::default()
    }

    pub fn get_vertical_scrollbar_style(&self) -> ScrollbarInfo {
        ScrollbarInfo::default()
    }
}

/// Holds info necessary for layouting / styling scrollbars (-webkit-scrollbar)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScrollbarInfo {
    /// Total width (or height for vertical scrollbars) of the scrollbar in pixels
    pub width: LayoutWidth,
    /// Padding of the scrollbar tracker, in pixels. The inner bar is `width - padding` pixels wide.
    pub padding: LayoutPadding,
    /// Style of the scrollbar background
    /// (`-webkit-scrollbar` / `-webkit-scrollbar-track` / `-webkit-scrollbar-track-piece` combined)
    pub track: RectStyle,
    /// Style of the scrollbar thumbs (the "up" / "down" arrows), (`-webkit-scrollbar-thumb`)
    pub thumb: RectStyle,
    /// Styles the directional buttons on the scrollbar (`-webkit-scrollbar-button`)
    pub button: RectStyle,
    /// If two scrollbars are present, addresses the (usually) bottom corner
    /// of the scrollable element, where two scrollbars might meet (`-webkit-scrollbar-corner`)
    pub corner: RectStyle,
    /// Addresses the draggable resizing handle that appears above the
    /// `corner` at the bottom corner of some elements (`-webkit-resizer`)
    pub resizer: RectStyle,
}

impl Default for ScrollbarInfo {
    fn default() -> Self {
        ScrollbarInfo {
            width: LayoutWidth(PixelValue::px(17.0)),
            padding: LayoutPadding {
                left: Some(PixelValue::px(2.0)),
                right: Some(PixelValue::px(2.0)),
                .. Default::default()
            },
            track: RectStyle {
                background_attachment: Some(StyleBackgroundAttachment::Color(ColorU {
                    r: 241, g: 241, b: 241, a: 255
                })),
                .. Default::default()
            },
            thumb: RectStyle {
                background_attachment: Some(StyleBackgroundAttachment::Color(ColorU {
                    r: 193, g: 193, b: 193, a: 255
                })),
                .. Default::default()
            },
            button: RectStyle {
                background_attachment: Some(StyleBackgroundAttachment::Color(ColorU {
                    r: 163, g: 163, b: 163, a: 255
                })),
                .. Default::default()
            },
            corner: RectStyle::default(),
            resizer: RectStyle::default(),
        }
    }
}

impl RectLayout {

    pub fn get_horizontal_padding(&self) -> f32 {
        let padding = self.padding.unwrap_or_default();
        padding.left.map(|l| l.to_pixels()).unwrap_or(0.0)
        + padding.right.map(|r| r.to_pixels()).unwrap_or(0.0)
    }

    pub fn get_vertical_padding(&self) -> f32 {
        let padding = self.padding.unwrap_or_default();
        padding.bottom.map(|l| l.to_pixels()).unwrap_or(0.0)
        + padding.top.map(|r| r.to_pixels()).unwrap_or(0.0)
    }

    pub fn get_horizontal_margin(&self) -> f32 {
        let margin = self.margin.unwrap_or_default();
        margin.left.map(|l| l.to_pixels()).unwrap_or(0.0)
        + margin.right.map(|r| r.to_pixels()).unwrap_or(0.0)
    }

    pub fn get_vertical_margin(&self) -> f32 {
        let margin = self.margin.unwrap_or_default();
        margin.bottom.map(|r| r.to_pixels()).unwrap_or(0.0)
        + margin.top.map(|l| l.to_pixels()).unwrap_or(0.0)
    }

    pub fn is_horizontal_overflow_visible(&self) -> bool {
        self.overflow.unwrap_or_default().is_horizontal_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(&self) -> bool {
        self.overflow.unwrap_or_default().is_vertical_overflow_visible()
    }
}

/// Represents a `font-size` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleFontSize(pub PixelValue);

impl_pixel_value!(StyleFontSize);

impl StyleFontSize {
    pub fn to_pixels(&self) -> f32 {
        self.0.to_pixels()
    }
}

/// Represents a `font-family` attribute
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleFontFamily {
    // fonts in order of precedence, i.e. "Webly Sleeky UI", "monospace", etc.
    pub fonts: Vec<FontId>
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub String);

impl FontId {
    pub fn get_str(&self) -> &str {
        &self.0
    }
}
