//! Provides a public API with datatypes used to describe style properties of DOM nodes.

use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    cmp::Ordering,
    ffi::c_void,
    fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use crate::{css::CssPropertyValue, AzString, OptionI16, OptionU16, OptionU32, U8Vec};

// Individual property types are now in their respective modules under `crate::properties::*`
// For example:
// use crate::properties::display::LayoutDisplayValue;
// use crate::properties::width::LayoutWidthValue;
// ... and so on for all refactored properties.

/// Currently hard-coded: Height of one em in pixels
pub const EM_HEIGHT: f32 = 16.0;
pub const PT_TO_PX: f32 = 96.0 / 72.0;

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

/// Map between CSS keys and a statically typed enum
const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &'static str); 74] = [
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

/// Only used for calculations: Rectangle (x, y, width, height) in layout space.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}

impl_option!(
    LayoutRect,
    OptionLayoutRect,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl_vec!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor);
impl_vec_clone!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor);
impl_vec_debug!(LayoutRect, LayoutRectVec);
impl_vec_mut!(LayoutRect, LayoutRectVec);
impl_vec_partialeq!(LayoutRect, LayoutRectVec);
impl_vec_partialord!(LayoutRect, LayoutRectVec);

impl fmt::Debug for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl LayoutRect {
    #[inline(always)]
    pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self {
        Self { origin, size }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(LayoutPoint::zero(), LayoutSize::zero())
    }
    #[inline(always)]
    pub const fn max_x(&self) -> isize {
        self.origin.x + self.size.width
    }
    #[inline(always)]
    pub const fn min_x(&self) -> isize {
        self.origin.x
    }
    #[inline(always)]
    pub const fn max_y(&self) -> isize {
        self.origin.y + self.size.height
    }
    #[inline(always)]
    pub const fn min_y(&self) -> isize {
        self.origin.y
    }
    #[inline(always)]
    pub const fn width(&self) -> isize {
        self.max_x() - self.min_x()
    }
    #[inline(always)]
    pub const fn height(&self) -> isize {
        self.max_y() - self.min_y()
    }

    pub const fn contains(&self, other: &LayoutPoint) -> bool {
        self.min_x() <= other.x
            && other.x < self.max_x()
            && self.min_y() <= other.y
            && other.y < self.max_y()
    }

    pub fn contains_f32(&self, other_x: f32, other_y: f32) -> bool {
        self.min_x() as f32 <= other_x
            && other_x < self.max_x() as f32
            && self.min_y() as f32 <= other_y
            && other_y < self.max_y() as f32
    }

    #[inline]
    pub const fn hit_test(&self, other: &LayoutPoint) -> Option<LayoutPoint> {
        let dx_left_edge = other.x - self.min_x();
        let dx_right_edge = self.max_x() - other.x;
        let dy_top_edge = other.y - self.min_y();
        let dy_bottom_edge = self.max_y() - other.y;
        if dx_left_edge > 0 && dx_right_edge > 0 && dy_top_edge > 0 && dy_bottom_edge > 0 {
            Some(LayoutPoint::new(dx_left_edge, dy_top_edge))
        } else {
            None
        }
    }

    #[inline]
    pub fn union<I: Iterator<Item = Self>>(mut rects: I) -> Option<Self> {
        let first = rects.next()?;

        let mut max_width = first.size.width;
        let mut max_height = first.size.height;
        let mut min_x = first.origin.x;
        let mut min_y = first.origin.y;

        while let Some(Self {
            origin: LayoutPoint { x, y },
            size: LayoutSize { width, height },
        }) = rects.next()
        {
            let cur_lower_right_x = x + width;
            let cur_lower_right_y = y + height;
            max_width = max_width.max(cur_lower_right_x - min_x);
            max_height = max_height.max(cur_lower_right_y - min_y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }

        Some(Self {
            origin: LayoutPoint { x: min_x, y: min_y },
            size: LayoutSize {
                width: max_width,
                height: max_height,
            },
        })
    }

    #[inline]
    pub fn get_scroll_rect<I: Iterator<Item = Self>>(&self, children: I) -> Option<Self> {
        let children_union = Self::union(children)?;
        Self::union([*self, children_union].iter().map(|r| *r))
    }

    #[inline(always)]
    pub const fn contains_rect(&self, b: &LayoutRect) -> bool {
        let a = self;
        let a_x = a.origin.x;
        let a_y = a.origin.y;
        let a_width = a.size.width;
        let a_height = a.size.height;
        let b_x = b.origin.x;
        let b_y = b.origin.y;
        let b_width = b.size.width;
        let b_height = b.size.height;
        b_x >= a_x
            && b_y >= a_y
            && b_x + b_width <= a_x + a_width
            && b_y + b_height <= a_y + a_height
    }
}

/// Only used for calculations: Size (width, height) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutSize {
    pub width: isize,
    pub height: isize,
}

impl_option!(
    LayoutSize,
    OptionLayoutSize,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]
);

impl fmt::Debug for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl LayoutSize {
    #[inline(always)]
    pub const fn new(width: isize, height: isize) -> Self {
        Self { width, height }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    #[inline]
    pub fn round(width: f32, height: f32) -> Self {
        Self {
            width: libm::roundf(width) as isize,
            height: libm::roundf(height) as isize,
        }
    }
}

/// Only used for calculations: Point coordinate (x, y) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutPoint {
    pub x: isize,
    pub y: isize,
}

impl fmt::Debug for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl LayoutPoint {
    #[inline(always)]
    pub const fn new(x: isize, y: isize) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
}

impl_option!(
    LayoutPoint,
    OptionLayoutPoint,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

/// Represents a parsed pair of `5px, 10px` values - useful for border radius calculation
#[derive(Default, Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct PixelSize {
    pub width: PixelValue,
    pub height: PixelValue,
}

impl PixelSize {
    pub const fn new(width: PixelValue, height: PixelValue) -> Self {
        Self { width, height }
    }

    pub const fn zero() -> Self {
        Self::new(PixelValue::const_px(0), PixelValue::const_px(0))
    }
}

/// Offsets of the border-width calculations
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct LayoutSideOffsets {
    pub top: FloatValue,
    pub right: FloatValue,
    pub bottom: FloatValue,
    pub left: FloatValue,
}

/// u8-based color, range 0 to 255 (similar to webrenders ColorU)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct ColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for ColorU {
    fn default() -> Self {
        ColorU::BLACK
    }
}

impl fmt::Display for ColorU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            self.a as f32 / 255.0
        )
    }
}

impl ColorU {
    pub const ALPHA_TRANSPARENT: u8 = 0;
    pub const ALPHA_OPAQUE: u8 = 255;

    pub const RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const GREEN: ColorU = ColorU {
        r: 0,
        g: 255,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLUE: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };
    pub const WHITE: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLACK: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const TRANSPARENT: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_TRANSPARENT,
    };

    pub const fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            r: libm::roundf(self.r as f32 + (other.r as f32 - self.r as f32) * t) as u8,
            g: libm::roundf(self.g as f32 + (other.g as f32 - self.g as f32) * t) as u8,
            b: libm::roundf(self.b as f32 + (other.b as f32 - self.b as f32) * t) as u8,
            a: libm::roundf(self.a as f32 + (other.a as f32 - self.a as f32) * t) as u8,
        }
    }

    pub const fn has_alpha(&self) -> bool {
        self.a != Self::ALPHA_OPAQUE
    }

    pub fn to_hash(&self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    pub fn write_hash(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders ColorF)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for ColorF {
    fn default() -> Self {
        ColorF::BLACK
    }
}

impl fmt::Display for ColorF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r * 255.0,
            self.g * 255.0,
            self.b * 255.0,
            self.a
        )
    }
}

impl ColorF {
    pub const ALPHA_TRANSPARENT: f32 = 0.0;
    pub const ALPHA_OPAQUE: f32 = 1.0;

    pub const WHITE: ColorF = ColorF {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLACK: ColorF = ColorF {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const TRANSPARENT: ColorF = ColorF {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_TRANSPARENT,
    };
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
    pub radius: Option<(
        StyleBorderTopLeftRadius,
        StyleBorderTopRightRadius,
        StyleBorderBottomLeftRadius,
        StyleBorderBottomRightRadius,
    )>,
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct BorderSide {
    pub color: ColorU,
    pub style: BorderStyle,
}

/// What direction should a `box-shadow` be clipped in (inset or outset)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub enum BoxShadowClipMode {
    Outset,
    Inset,
}

impl fmt::Display for BoxShadowClipMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BoxShadowClipMode::*;
        match self {
            Outset => write!(f, "outset"),
            Inset => write!(f, "inset"),
        }
    }
}

/// Whether a `gradient` should be repeated or clamped to the edges.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub enum ExtendMode {
    Clamp,
    Repeat,
}

impl Default for ExtendMode {
    fn default() -> Self {
        ExtendMode::Clamp
    }
}

/// Style of a `border`: solid, double, dash, ridge, etc.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
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

impl fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BorderStyle::*;
        match self {
            None => write!(f, "none"),
            Solid => write!(f, "solid"),
            Double => write!(f, "double"),
            Dotted => write!(f, "dotted"),
            Dashed => write!(f, "dashed"),
            Hidden => write!(f, "hidden"),
            Groove => write!(f, "groove"),
            Ridge => write!(f, "ridge"),
            Inset => write!(f, "inset"),
            Outset => write!(f, "outset"),
        }
    }
}

impl BorderStyle {
    pub fn normalize_border(self) -> Option<BorderStyleNoNone> {
        match self {
            BorderStyle::None => None,
            BorderStyle::Solid => Some(BorderStyleNoNone::Solid),
            BorderStyle::Double => Some(BorderStyleNoNone::Double),
            BorderStyle::Dotted => Some(BorderStyleNoNone::Dotted),
            BorderStyle::Dashed => Some(BorderStyleNoNone::Dashed),
            BorderStyle::Hidden => Some(BorderStyleNoNone::Hidden),
            BorderStyle::Groove => Some(BorderStyleNoNone::Groove),
            BorderStyle::Ridge => Some(BorderStyleNoNone::Ridge),
            BorderStyle::Inset => Some(BorderStyleNoNone::Inset),
            BorderStyle::Outset => Some(BorderStyleNoNone::Outset),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub enum BorderStyleNoNone {
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
        BorderStyle::Solid
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct NinePatchBorder {
    // not implemented or parse-able yet, so no fields!
}

#[macro_export]
macro_rules! derive_debug_zero {
    ($struct:ident) => {
        impl fmt::Debug for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:?}", self.inner)
            }
        }
    };
}

#[macro_export]
macro_rules! derive_display_zero {
    ($struct:ident) => {
        impl fmt::Display for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}", self.inner)
            }
        }
    };
}

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
#[macro_export]
macro_rules! impl_pixel_value {
    ($struct:ident) => {
        derive_debug_zero!($struct);
        derive_display_zero!($struct);

        impl $struct {
            #[inline]
            pub const fn zero() -> Self {
                Self {
                    inner: PixelValue::zero(),
                }
            }

            /// Same as `PixelValue::px()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_px(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_px(value),
                }
            }

            /// Same as `PixelValue::em()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_em(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_em(value),
                }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_pt(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_pt(value),
                }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_percent(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_percent(value),
                }
            }

            /// Same as `PixelValue::in()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_in(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_in(value),
                }
            }

            /// Same as `PixelValue::cm()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_cm(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_cm(value),
                }
            }

            /// Same as `PixelValue::cm()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_mm(value: isize) -> Self {
                Self {
                    inner: PixelValue::const_mm(value),
                }
            }

            #[inline]
            pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
                Self {
                    inner: PixelValue::const_from_metric(metric, value),
                }
            }

            #[inline]
            pub fn px(value: f32) -> Self {
                Self {
                    inner: PixelValue::px(value),
                }
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                Self {
                    inner: PixelValue::em(value),
                }
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                Self {
                    inner: PixelValue::pt(value),
                }
            }

            #[inline]
            pub fn percent(value: f32) -> Self {
                Self {
                    inner: PixelValue::percent(value),
                }
            }

            #[inline]
            pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
                Self {
                    inner: PixelValue::from_metric(metric, value),
                }
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                $struct {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_percentage_value {
    ($struct:ident) => {
        impl ::core::fmt::Display for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}%", self.inner.normalized() * 100.0)
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}%", self.inner.normalized() * 100.0)
            }
        }

        impl $struct {
            /// Same as `PercentageValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_new(value: isize) -> Self {
                Self {
                    inner: PercentageValue::const_new(value),
                }
            }

            #[inline]
            pub fn new(value: f32) -> Self {
                Self {
                    inner: PercentageValue::new(value),
                }
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                $struct {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_float_value {
    ($struct:ident) => {
        impl $struct {
            /// Same as `FloatValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            pub const fn const_new(value: isize) -> Self {
                Self {
                    inner: FloatValue::const_new(value),
                }
            }

            pub fn new(value: f32) -> Self {
                Self {
                    inner: FloatValue::new(value),
                }
            }

            pub fn get(&self) -> f32 {
                self.inner.get()
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }

        impl From<f32> for $struct {
            fn from(val: f32) -> Self {
                Self {
                    inner: FloatValue::from(val),
                }
            }
        }

        impl ::core::fmt::Display for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner.get())
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner.get())
            }
        }
    };
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CombinedCssPropertyType {
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
    BackgroundColor, // BackgroundContent::Colo
    BackgroundImage, // BackgroundContent::Colo
}

impl fmt::Display for CombinedCssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let key = COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .find(|(v, _)| *v == *self)
            .and_then(|(k, _)| Some(k))
            .unwrap();
        write!(f, "{}", key)
    }
}

impl CombinedCssPropertyType {
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.shorthands.get(input).map(|x| *x)
    }

    pub fn to_str(&self, map: &CssKeyMap) -> &'static str {
        map.shorthands
            .iter()
            .find(|(_, v)| *v == self)
            .map(|(k, _)| k)
            .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CssKeyMap {
    pub non_shorthands: BTreeMap<&'static str, CssPropertyType>,
    pub shorthands: BTreeMap<&'static str, CombinedCssPropertyType>,
}

impl CssKeyMap {
    pub fn get() -> Self {
        get_css_key_map()
    }
}

pub fn get_css_key_map() -> CssKeyMap {
    CssKeyMap {
        non_shorthands: CSS_PROPERTY_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect(),
        shorthands: COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .map(|(v, k)| (*k, *v))
            .collect(),
    }
}

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
    WhiteSpace,
    Direction,
    Hyphens,
}

impl CssPropertyType {
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.non_shorthands.get(input).and_then(|x| Some(*x))
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            CssPropertyType::TextColor => "color",
            CssPropertyType::FontSize => "font-size",
            CssPropertyType::FontFamily => "font-family",
            CssPropertyType::TextAlign => "text-align",
            CssPropertyType::LetterSpacing => "letter-spacing",
            CssPropertyType::LineHeight => "line-height",
            CssPropertyType::WordSpacing => "word-spacing",
            CssPropertyType::TabWidth => "tab-width",
            CssPropertyType::Cursor => "cursor",
            CssPropertyType::Display => "display",
            CssPropertyType::Float => "float",
            CssPropertyType::BoxSizing => "box-sizing",
            CssPropertyType::Width => "width",
            CssPropertyType::Height => "height",
            CssPropertyType::MinWidth => "min-width",
            CssPropertyType::MinHeight => "min-height",
            CssPropertyType::MaxWidth => "max-width",
            CssPropertyType::MaxHeight => "max-height",
            CssPropertyType::Position => "position",
            CssPropertyType::Top => "top",
            CssPropertyType::Right => "right",
            CssPropertyType::Left => "left",
            CssPropertyType::Bottom => "bottom",
            CssPropertyType::FlexWrap => "flex-wrap",
            CssPropertyType::FlexDirection => "flex-direction",
            CssPropertyType::FlexGrow => "flex-grow",
            CssPropertyType::FlexShrink => "flex-shrink",
            CssPropertyType::JustifyContent => "justify-content",
            CssPropertyType::AlignItems => "align-items",
            CssPropertyType::AlignContent => "align-content",
            CssPropertyType::BackgroundContent => "background",
            CssPropertyType::BackgroundPosition => "background-position",
            CssPropertyType::BackgroundSize => "background-size",
            CssPropertyType::BackgroundRepeat => "background-repeat",
            CssPropertyType::OverflowX => "overflow-x",
            CssPropertyType::OverflowY => "overflow-y",
            CssPropertyType::PaddingTop => "padding-top",
            CssPropertyType::PaddingLeft => "padding-left",
            CssPropertyType::PaddingRight => "padding-right",
            CssPropertyType::PaddingBottom => "padding-bottom",
            CssPropertyType::MarginTop => "margin-top",
            CssPropertyType::MarginLeft => "margin-left",
            CssPropertyType::MarginRight => "margin-right",
            CssPropertyType::MarginBottom => "margin-bottom",
            CssPropertyType::BorderTopLeftRadius => "border-top-left-radius",
            CssPropertyType::BorderTopRightRadius => "border-top-right-radius",
            CssPropertyType::BorderBottomLeftRadius => "border-bottom-left-radius",
            CssPropertyType::BorderBottomRightRadius => "border-bottom-right-radius",
            CssPropertyType::BorderTopColor => "border-top-color",
            CssPropertyType::BorderRightColor => "border-right-color",
            CssPropertyType::BorderLeftColor => "border-left-color",
            CssPropertyType::BorderBottomColor => "border-bottom-color",
            CssPropertyType::BorderTopStyle => "border-top-style",
            CssPropertyType::BorderRightStyle => "border-right-style",
            CssPropertyType::BorderLeftStyle => "border-left-style",
            CssPropertyType::BorderBottomStyle => "border-bottom-style",
            CssPropertyType::BorderTopWidth => "border-top-width",
            CssPropertyType::BorderRightWidth => "border-right-width",
            CssPropertyType::BorderLeftWidth => "border-left-width",
            CssPropertyType::BorderBottomWidth => "border-bottom-width",
            CssPropertyType::BoxShadowLeft => "-azul-box-shadow-left",
            CssPropertyType::BoxShadowRight => "-azul-box-shadow-right",
            CssPropertyType::BoxShadowTop => "-azul-box-shadow-top",
            CssPropertyType::BoxShadowBottom => "-azul-box-shadow-bottom",
            CssPropertyType::ScrollbarStyle => "-azul-scrollbar-style",
            CssPropertyType::Opacity => "opacity",
            CssPropertyType::Transform => "transform",
            CssPropertyType::TransformOrigin => "transform-origin",
            CssPropertyType::PerspectiveOrigin => "perspective-origin",
            CssPropertyType::BackfaceVisibility => "backface-visibility",
            CssPropertyType::MixBlendMode => "mix-blend-mode",
            CssPropertyType::Filter => "filter",
            CssPropertyType::BackdropFilter => "backdrop-filter",
            CssPropertyType::TextShadow => "text-shadow",
            CssPropertyType::WhiteSpace => "white-space",
            CssPropertyType::Hyphens => "hyphens",
            CssPropertyType::Direction => "direction",
        }
    }

    pub fn is_inheritable(&self) -> bool {
        use self::CssPropertyType::*;
        match self {
            TextColor | FontFamily | FontSize | LineHeight | TextAlign => true,
            _ => false,
        }
    }

    pub fn can_trigger_relayout(&self) -> bool {
        use self::CssPropertyType::*;
        match self {
            TextColor
            | Cursor
            | BackgroundContent
            | BackgroundPosition
            | BackgroundSize
            | BackgroundRepeat
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
            | ScrollbarStyle
            | Opacity
            | Transform
            | TransformOrigin
            | PerspectiveOrigin
            | BackfaceVisibility
            | MixBlendMode
            | Filter
            | BackdropFilter
            | TextShadow => false,
            _ => true,
        }
    }

    pub fn is_gpu_only_property(&self) -> bool {
        match self {
            CssPropertyType::Opacity |
            CssPropertyType::Transform => true,
            _ => false
        }
    }
}

impl fmt::Debug for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::Display for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

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
    Cursor(StyleCursorValue),
    Display(crate::properties::display::LayoutDisplayValue),
    Float(crate::properties::float::LayoutFloatValue),
    BoxSizing(crate::properties::box_sizing::LayoutBoxSizingValue),
    Width(crate::properties::width::LayoutWidthValue),
    Height(crate::properties::height::LayoutHeightValue),
    MinWidth(crate::properties::min_width::LayoutMinWidthValue),
    MinHeight(crate::properties::min_height::LayoutMinHeightValue),
    MaxWidth(crate::properties::max_width::LayoutMaxWidthValue),
    MaxHeight(crate::properties::max_height::LayoutMaxHeightValue),
    Position(crate::properties::position::LayoutPositionValue),
    Top(crate::properties::top::LayoutTopValue),
    Right(crate::properties::right::LayoutRightValue),
    Left(crate::properties::left::LayoutLeftValue),
    Bottom(crate::properties::bottom::LayoutBottomValue),
    FlexWrap(crate::properties::flex_wrap::LayoutFlexWrapValue),
    FlexDirection(crate::properties::flex_direction::LayoutFlexDirectionValue),
    FlexGrow(crate::properties::flex_grow::LayoutFlexGrowValue),
    FlexShrink(crate::properties::flex_shrink::LayoutFlexShrinkValue),
    JustifyContent(crate::properties::justify_content::LayoutJustifyContentValue),
    AlignItems(crate::properties::align_items::LayoutAlignItemsValue),
    AlignContent(crate::properties::align_content::LayoutAlignContentValue),
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
    Direction(StyleDirectionValue),
    Hyphens(StyleHyphensValue),
    WhiteSpace(StyleWhiteSpaceValue),
}

impl_option!(
    CssProperty,
    OptionCssProperty,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord]
);

macro_rules! css_property_from_type {
    ($prop_type:expr, $content_type:ident) => {{
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(StyleTextColorValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(StyleFontSizeValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(StyleFontFamilyVecValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(StyleTextAlignValue::$content_type),
            CssPropertyType::LetterSpacing => CssProperty::LetterSpacing(StyleLetterSpacingValue::$content_type),
            CssPropertyType::LineHeight => CssProperty::LineHeight(StyleLineHeightValue::$content_type),
            CssPropertyType::WordSpacing => CssProperty::WordSpacing(StyleWordSpacingValue::$content_type),
            CssPropertyType::TabWidth => CssProperty::TabWidth(StyleTabWidthValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(StyleCursorValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(crate::properties::display::LayoutDisplayValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(crate::properties::float::LayoutFloatValue::$content_type),
            CssPropertyType::BoxSizing => CssProperty::BoxSizing(crate::properties::box_sizing::LayoutBoxSizingValue::$content_type),
            CssPropertyType::Width => CssProperty::Width(crate::properties::width::LayoutWidthValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(crate::properties::height::LayoutHeightValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(crate::properties::min_width::LayoutMinWidthValue::$content_type),
            CssPropertyType::MinHeight => CssProperty::MinHeight(crate::properties::min_height::LayoutMinHeightValue::$content_type),
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(crate::properties::max_width::LayoutMaxWidthValue::$content_type),
            CssPropertyType::MaxHeight => CssProperty::MaxHeight(crate::properties::max_height::LayoutMaxHeightValue::$content_type),
            CssPropertyType::Position => CssProperty::Position(crate::properties::position::LayoutPositionValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(crate::properties::top::LayoutTopValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(crate::properties::right::LayoutRightValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(crate::properties::left::LayoutLeftValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(crate::properties::bottom::LayoutBottomValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(crate::properties::flex_wrap::LayoutFlexWrapValue::$content_type),
            CssPropertyType::FlexDirection => CssProperty::FlexDirection(crate::properties::flex_direction::LayoutFlexDirectionValue::$content_type),
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(crate::properties::flex_grow::LayoutFlexGrowValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(crate::properties::flex_shrink::LayoutFlexShrinkValue::$content_type),
            CssPropertyType::JustifyContent => CssProperty::JustifyContent(crate::properties::justify_content::LayoutJustifyContentValue::$content_type),
            CssPropertyType::AlignItems => CssProperty::AlignItems(crate::properties::align_items::LayoutAlignItemsValue::$content_type),
            CssPropertyType::AlignContent => CssProperty::AlignContent(crate::properties::align_content::LayoutAlignContentValue::$content_type),
            CssPropertyType::BackgroundContent => CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type),
            CssPropertyType::BackgroundPosition => CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::$content_type),
            CssPropertyType::BackgroundSize => CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::$content_type),
            CssPropertyType::BackgroundRepeat => CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::$content_type),
            CssPropertyType::OverflowX => CssProperty::OverflowX(LayoutOverflowValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(LayoutOverflowValue::$content_type),
            CssPropertyType::PaddingTop => CssProperty::PaddingTop(LayoutPaddingTopValue::$content_type),
            CssPropertyType::PaddingLeft => CssProperty::PaddingLeft(LayoutPaddingLeftValue::$content_type),
            CssPropertyType::PaddingRight => CssProperty::PaddingRight(LayoutPaddingRightValue::$content_type),
            CssPropertyType::PaddingBottom => CssProperty::PaddingBottom(LayoutPaddingBottomValue::$content_type),
            CssPropertyType::MarginTop => CssProperty::MarginTop(LayoutMarginTopValue::$content_type),
            CssPropertyType::MarginLeft => CssProperty::MarginLeft(LayoutMarginLeftValue::$content_type),
            CssPropertyType::MarginRight => CssProperty::MarginRight(LayoutMarginRightValue::$content_type),
            CssPropertyType::MarginBottom => CssProperty::MarginBottom(LayoutMarginBottomValue::$content_type),
            CssPropertyType::BorderTopLeftRadius => CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::$content_type),
            CssPropertyType::BorderTopRightRadius => CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::$content_type),
            CssPropertyType::BorderBottomLeftRadius => CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::$content_type),
            CssPropertyType::BorderBottomRightRadius => CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::$content_type),
            CssPropertyType::BorderTopColor => CssProperty::BorderTopColor(StyleBorderTopColorValue::$content_type),
            CssPropertyType::BorderRightColor => CssProperty::BorderRightColor(StyleBorderRightColorValue::$content_type),
            CssPropertyType::BorderLeftColor => CssProperty::BorderLeftColor(StyleBorderLeftColorValue::$content_type),
            CssPropertyType::BorderBottomColor => CssProperty::BorderBottomColor(StyleBorderBottomColorValue::$content_type),
            CssPropertyType::BorderTopStyle => CssProperty::BorderTopStyle(StyleBorderTopStyleValue::$content_type),
            CssPropertyType::BorderRightStyle => CssProperty::BorderRightStyle(StyleBorderRightStyleValue::$content_type),
            CssPropertyType::BorderLeftStyle => CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::$content_type),
            CssPropertyType::BorderBottomStyle => CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::$content_type),
            CssPropertyType::BorderTopWidth => CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::$content_type),
            CssPropertyType::BorderRightWidth => CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::$content_type),
            CssPropertyType::BorderLeftWidth => CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::$content_type),
            CssPropertyType::BorderBottomWidth => CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::$content_type),
            CssPropertyType::BoxShadowLeft => CssProperty::BoxShadowLeft(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowRight => CssProperty::BoxShadowRight(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowTop => CssProperty::BoxShadowTop(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowBottom => CssProperty::BoxShadowBottom(StyleBoxShadowValue::$content_type),
            CssPropertyType::ScrollbarStyle => CssProperty::ScrollbarStyle(ScrollbarStyleValue::$content_type),
            CssPropertyType::Opacity => CssProperty::Opacity(StyleOpacityValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(StyleTransformVecValue::$content_type),
            CssPropertyType::PerspectiveOrigin => CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::$content_type),
            CssPropertyType::TransformOrigin => CssProperty::TransformOrigin(StyleTransformOriginValue::$content_type),
            CssPropertyType::BackfaceVisibility => CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::$content_type),
            CssPropertyType::MixBlendMode => CssProperty::MixBlendMode(StyleMixBlendModeValue::$content_type),
            CssPropertyType::Filter => CssProperty::Filter(StyleFilterVecValue::$content_type),
            CssPropertyType::BackdropFilter => CssProperty::BackdropFilter(StyleFilterVecValue::$content_type),
            CssPropertyType::TextShadow => CssProperty::TextShadow(StyleBoxShadowValue::$content_type),
            CssPropertyType::WhiteSpace => CssProperty::WhiteSpace(StyleWhiteSpaceValue::$content_type),
            CssPropertyType::Hyphens => CssProperty::Hyphens(StyleHyphensValue::$content_type),
            CssPropertyType::Direction => CssProperty::Direction(StyleDirectionValue::$content_type),
        }
    }};
}

impl CssProperty {
    pub fn is_initial(&self) -> bool {
        use self::CssProperty::*;
        match self {
            TextColor(c) => c.is_initial(), FontSize(c) => c.is_initial(), FontFamily(c) => c.is_initial(), TextAlign(c) => c.is_initial(), LetterSpacing(c) => c.is_initial(), LineHeight(c) => c.is_initial(), WordSpacing(c) => c.is_initial(), TabWidth(c) => c.is_initial(), Cursor(c) => c.is_initial(),
            Display(c) => c.is_initial(), Float(c) => c.is_initial(), BoxSizing(c) => c.is_initial(),
            Width(c) => c.is_initial(), Height(c) => c.is_initial(), MinWidth(c) => c.is_initial(), MinHeight(c) => c.is_initial(), MaxWidth(c) => c.is_initial(), MaxHeight(c) => c.is_initial(),
            Position(c) => c.is_initial(), Top(c) => c.is_initial(), Right(c) => c.is_initial(), Left(c) => c.is_initial(), Bottom(c) => c.is_initial(),
            FlexWrap(c) => c.is_initial(), FlexDirection(c) => c.is_initial(), FlexGrow(c) => c.is_initial(), FlexShrink(c) => c.is_initial(), JustifyContent(c) => c.is_initial(), AlignItems(c) => c.is_initial(), AlignContent(c) => c.is_initial(),
            BackgroundContent(c) => c.is_initial(), BackgroundPosition(c) => c.is_initial(), BackgroundSize(c) => c.is_initial(), BackgroundRepeat(c) => c.is_initial(),
            OverflowX(c) => c.is_initial(), OverflowY(c) => c.is_initial(),
            PaddingTop(c) => c.is_initial(), PaddingLeft(c) => c.is_initial(), PaddingRight(c) => c.is_initial(), PaddingBottom(c) => c.is_initial(),
            MarginTop(c) => c.is_initial(), MarginLeft(c) => c.is_initial(), MarginRight(c) => c.is_initial(), MarginBottom(c) => c.is_initial(),
            BorderTopLeftRadius(c) => c.is_initial(), BorderTopRightRadius(c) => c.is_initial(), BorderBottomLeftRadius(c) => c.is_initial(), BorderBottomRightRadius(c) => c.is_initial(),
            BorderTopColor(c) => c.is_initial(), BorderRightColor(c) => c.is_initial(), BorderLeftColor(c) => c.is_initial(), BorderBottomColor(c) => c.is_initial(),
            BorderTopStyle(c) => c.is_initial(), BorderRightStyle(c) => c.is_initial(), BorderLeftStyle(c) => c.is_initial(), BorderBottomStyle(c) => c.is_initial(),
            BorderTopWidth(c) => c.is_initial(), BorderRightWidth(c) => c.is_initial(), BorderLeftWidth(c) => c.is_initial(), BorderBottomWidth(c) => c.is_initial(),
            BoxShadowLeft(c) => c.is_initial(), BoxShadowRight(c) => c.is_initial(), BoxShadowTop(c) => c.is_initial(), BoxShadowBottom(c) => c.is_initial(),
            ScrollbarStyle(c) => c.is_initial(), Opacity(c) => c.is_initial(), Transform(c) => c.is_initial(), TransformOrigin(c) => c.is_initial(), PerspectiveOrigin(c) => c.is_initial(), BackfaceVisibility(c) => c.is_initial(),
            MixBlendMode(c) => c.is_initial(), Filter(c) => c.is_initial(), BackdropFilter(c) => c.is_initial(), TextShadow(c) => c.is_initial(),
            WhiteSpace(c) => c.is_initial(), Direction(c) => c.is_initial(), Hyphens(c) => c.is_initial(),
        }
    }

    pub const fn const_none(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, None) }
    pub const fn const_auto(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Auto) }
    pub const fn const_initial(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Initial) }
    pub const fn const_inherit(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Inherit) }

    pub const fn const_text_color(input: StyleTextColor) -> Self { CssProperty::TextColor(CssPropertyValue::Exact(input)) }
    pub const fn const_font_size(input: StyleFontSize) -> Self { CssProperty::FontSize(CssPropertyValue::Exact(input)) }
    pub const fn const_font_family(input: StyleFontFamilyVec) -> Self { CssProperty::FontFamily(CssPropertyValue::Exact(input)) }
    pub const fn const_text_align(input: StyleTextAlign) -> Self { CssProperty::TextAlign(CssPropertyValue::Exact(input)) }
    pub const fn const_letter_spacing(input: StyleLetterSpacing) -> Self { CssProperty::LetterSpacing(CssPropertyValue::Exact(input)) }
    pub const fn const_line_height(input: StyleLineHeight) -> Self { CssProperty::LineHeight(CssPropertyValue::Exact(input)) }
    pub const fn const_word_spacing(input: StyleWordSpacing) -> Self { CssProperty::WordSpacing(CssPropertyValue::Exact(input)) }
    pub const fn const_tab_width(input: StyleTabWidth) -> Self { CssProperty::TabWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_cursor(input: StyleCursor) -> Self { CssProperty::Cursor(CssPropertyValue::Exact(input)) }
    pub const fn const_display(input: crate::properties::display::LayoutDisplay) -> Self { CssProperty::Display(CssPropertyValue::Exact(input)) }
    pub const fn const_float(input: crate::properties::float::LayoutFloat) -> Self { CssProperty::Float(CssPropertyValue::Exact(input)) }
    pub const fn const_box_sizing(input: crate::properties::box_sizing::LayoutBoxSizing) -> Self { CssProperty::BoxSizing(CssPropertyValue::Exact(input)) }
    pub const fn const_width(input: crate::properties::width::LayoutWidth) -> Self { CssProperty::Width(CssPropertyValue::Exact(input)) }
    pub const fn const_height(input: crate::properties::height::LayoutHeight) -> Self { CssProperty::Height(CssPropertyValue::Exact(input)) }
    pub const fn const_min_width(input: crate::properties::min_width::LayoutMinWidth) -> Self { CssProperty::MinWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_min_height(input: crate::properties::min_height::LayoutMinHeight) -> Self { CssProperty::MinHeight(CssPropertyValue::Exact(input)) }
    pub const fn const_max_width(input: crate::properties::max_width::LayoutMaxWidth) -> Self { CssProperty::MaxWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_max_height(input: crate::properties::max_height::LayoutMaxHeight) -> Self { CssProperty::MaxHeight(CssPropertyValue::Exact(input)) }
    pub const fn const_position(input: crate::properties::position::LayoutPosition) -> Self { CssProperty::Position(CssPropertyValue::Exact(input)) }
    pub const fn const_top(input: crate::properties::top::LayoutTop) -> Self { CssProperty::Top(CssPropertyValue::Exact(input)) }
    pub const fn const_right(input: crate::properties::right::LayoutRight) -> Self { CssProperty::Right(CssPropertyValue::Exact(input)) }
    pub const fn const_left(input: crate::properties::left::LayoutLeft) -> Self { CssProperty::Left(CssPropertyValue::Exact(input)) }
    pub const fn const_bottom(input: crate::properties::bottom::LayoutBottom) -> Self { CssProperty::Bottom(CssPropertyValue::Exact(input)) }
    pub const fn const_flex_wrap(input: crate::properties::flex_wrap::LayoutFlexWrap) -> Self { CssProperty::FlexWrap(CssPropertyValue::Exact(input)) }
    pub const fn const_flex_direction(input: crate::properties::flex_direction::LayoutFlexDirection) -> Self { CssProperty::FlexDirection(CssPropertyValue::Exact(input)) }
    pub const fn const_flex_grow(input: crate::properties::flex_grow::LayoutFlexGrow) -> Self { CssProperty::FlexGrow(CssPropertyValue::Exact(input)) }
    pub const fn const_flex_shrink(input: crate::properties::flex_shrink::LayoutFlexShrink) -> Self { CssProperty::FlexShrink(CssPropertyValue::Exact(input)) }
    pub const fn const_justify_content(input: crate::properties::justify_content::LayoutJustifyContent) -> Self { CssProperty::JustifyContent(CssPropertyValue::Exact(input)) }
    pub const fn const_align_items(input: crate::properties::align_items::LayoutAlignItems) -> Self { CssProperty::AlignItems(CssPropertyValue::Exact(input)) }
    pub const fn const_align_content(input: crate::properties::align_content::LayoutAlignContent) -> Self { CssProperty::AlignContent(CssPropertyValue::Exact(input)) }
    pub const fn const_background_content(input: StyleBackgroundContentVec) -> Self { CssProperty::BackgroundContent(CssPropertyValue::Exact(input)) }
    pub const fn const_background_position(input: StyleBackgroundPositionVec) -> Self { CssProperty::BackgroundPosition(CssPropertyValue::Exact(input)) }
    pub const fn const_background_size(input: StyleBackgroundSizeVec) -> Self { CssProperty::BackgroundSize(CssPropertyValue::Exact(input)) }
    pub const fn const_background_repeat(input: StyleBackgroundRepeatVec) -> Self { CssProperty::BackgroundRepeat(CssPropertyValue::Exact(input)) }
    pub const fn const_overflow_x(input: LayoutOverflow) -> Self { CssProperty::OverflowX(CssPropertyValue::Exact(input)) }
    pub const fn const_overflow_y(input: LayoutOverflow) -> Self { CssProperty::OverflowY(CssPropertyValue::Exact(input)) }
    pub const fn const_padding_top(input: LayoutPaddingTop) -> Self { CssProperty::PaddingTop(CssPropertyValue::Exact(input)) }
    pub const fn const_padding_left(input: LayoutPaddingLeft) -> Self { CssProperty::PaddingLeft(CssPropertyValue::Exact(input)) }
    pub const fn const_padding_right(input: LayoutPaddingRight) -> Self { CssProperty::PaddingRight(CssPropertyValue::Exact(input)) }
    pub const fn const_padding_bottom(input: LayoutPaddingBottom) -> Self { CssProperty::PaddingBottom(CssPropertyValue::Exact(input)) }
    pub const fn const_margin_top(input: LayoutMarginTop) -> Self { CssProperty::MarginTop(CssPropertyValue::Exact(input)) }
    pub const fn const_margin_left(input: LayoutMarginLeft) -> Self { CssProperty::MarginLeft(CssPropertyValue::Exact(input)) }
    pub const fn const_margin_right(input: LayoutMarginRight) -> Self { CssProperty::MarginRight(CssPropertyValue::Exact(input)) }
    pub const fn const_margin_bottom(input: LayoutMarginBottom) -> Self { CssProperty::MarginBottom(CssPropertyValue::Exact(input)) }
    pub const fn const_border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self { CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(input)) }
    pub const fn const_border_top_right_radius(input: StyleBorderTopRightRadius) -> Self { CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(input)) }
    pub const fn const_border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self { CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(input)) }
    pub const fn const_border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self { CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(input)) }
    pub const fn const_border_top_color(input: StyleBorderTopColor) -> Self { CssProperty::BorderTopColor(CssPropertyValue::Exact(input)) }
    pub const fn const_border_right_color(input: StyleBorderRightColor) -> Self { CssProperty::BorderRightColor(CssPropertyValue::Exact(input)) }
    pub const fn const_border_left_color(input: StyleBorderLeftColor) -> Self { CssProperty::BorderLeftColor(CssPropertyValue::Exact(input)) }
    pub const fn const_border_bottom_color(input: StyleBorderBottomColor) -> Self { CssProperty::BorderBottomColor(CssPropertyValue::Exact(input)) }
    pub const fn const_border_top_style(input: StyleBorderTopStyle) -> Self { CssProperty::BorderTopStyle(CssPropertyValue::Exact(input)) }
    pub const fn const_border_right_style(input: StyleBorderRightStyle) -> Self { CssProperty::BorderRightStyle(CssPropertyValue::Exact(input)) }
    pub const fn const_border_left_style(input: StyleBorderLeftStyle) -> Self { CssProperty::BorderLeftStyle(CssPropertyValue::Exact(input)) }
    pub const fn const_border_bottom_style(input: StyleBorderBottomStyle) -> Self { CssProperty::BorderBottomStyle(CssPropertyValue::Exact(input)) }
    pub const fn const_border_top_width(input: LayoutBorderTopWidth) -> Self { CssProperty::BorderTopWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_border_right_width(input: LayoutBorderRightWidth) -> Self { CssProperty::BorderRightWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_border_left_width(input: LayoutBorderLeftWidth) -> Self { CssProperty::BorderLeftWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_border_bottom_width(input: LayoutBorderBottomWidth) -> Self { CssProperty::BorderBottomWidth(CssPropertyValue::Exact(input)) }
    pub const fn const_box_shadow_left(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowLeft(CssPropertyValue::Exact(input)) }
    pub const fn const_box_shadow_right(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowRight(CssPropertyValue::Exact(input)) }
    pub const fn const_box_shadow_top(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowTop(CssPropertyValue::Exact(input)) }
    pub const fn const_box_shadow_bottom(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowBottom(CssPropertyValue::Exact(input)) }
    pub const fn const_opacity(input: StyleOpacity) -> Self { CssProperty::Opacity(CssPropertyValue::Exact(input)) }
    pub const fn const_transform(input: StyleTransformVec) -> Self { CssProperty::Transform(CssPropertyValue::Exact(input)) }
    pub const fn const_transform_origin(input: StyleTransformOrigin) -> Self { CssProperty::TransformOrigin(CssPropertyValue::Exact(input)) }
    pub const fn const_perspective_origin(input: StylePerspectiveOrigin) -> Self { CssProperty::PerspectiveOrigin(CssPropertyValue::Exact(input)) }
    pub const fn const_backface_visiblity(input: StyleBackfaceVisibility) -> Self { CssProperty::BackfaceVisibility(CssPropertyValue::Exact(input)) }

    pub const fn as_background_content(&self) -> Option<&StyleBackgroundContentVecValue> { match self { CssProperty::BackgroundContent(f) => Some(f), _ => None } }
    pub const fn as_background_position(&self) -> Option<&StyleBackgroundPositionVecValue> { match self { CssProperty::BackgroundPosition(f) => Some(f), _ => None } }
    pub const fn as_background_size(&self) -> Option<&StyleBackgroundSizeVecValue> { match self { CssProperty::BackgroundSize(f) => Some(f), _ => None } }
    pub const fn as_background_repeat(&self) -> Option<&StyleBackgroundRepeatVecValue> { match self { CssProperty::BackgroundRepeat(f) => Some(f), _ => None } }
    pub const fn as_font_size(&self) -> Option<&StyleFontSizeValue> { match self { CssProperty::FontSize(f) => Some(f), _ => None } }
    pub const fn as_font_family(&self) -> Option<&StyleFontFamilyVecValue> { match self { CssProperty::FontFamily(f) => Some(f), _ => None } }
    pub const fn as_text_color(&self) -> Option<&StyleTextColorValue> { match self { CssProperty::TextColor(f) => Some(f), _ => None } }
    pub const fn as_text_align(&self) -> Option<&StyleTextAlignValue> { match self { CssProperty::TextAlign(f) => Some(f), _ => None } }
    pub const fn as_line_height(&self) -> Option<&StyleLineHeightValue> { match self { CssProperty::LineHeight(f) => Some(f), _ => None } }
    pub const fn as_letter_spacing(&self) -> Option<&StyleLetterSpacingValue> { match self { CssProperty::LetterSpacing(f) => Some(f), _ => None } }
    pub const fn as_word_spacing(&self) -> Option<&StyleWordSpacingValue> { match self { CssProperty::WordSpacing(f) => Some(f), _ => None } }
    pub const fn as_tab_width(&self) -> Option<&StyleTabWidthValue> { match self { CssProperty::TabWidth(f) => Some(f), _ => None } }
    pub const fn as_cursor(&self) -> Option<&StyleCursorValue> { match self { CssProperty::Cursor(f) => Some(f), _ => None } }
    pub const fn as_box_shadow_left(&self) -> Option<&StyleBoxShadowValue> { match self { CssProperty::BoxShadowLeft(f) => Some(f), _ => None } }
    pub const fn as_box_shadow_right(&self) -> Option<&StyleBoxShadowValue> { match self { CssProperty::BoxShadowRight(f) => Some(f), _ => None } }
    pub const fn as_box_shadow_top(&self) -> Option<&StyleBoxShadowValue> { match self { CssProperty::BoxShadowTop(f) => Some(f), _ => None } }
    pub const fn as_box_shadow_bottom(&self) -> Option<&StyleBoxShadowValue> { match self { CssProperty::BoxShadowBottom(f) => Some(f), _ => None } }
    pub const fn as_border_top_color(&self) -> Option<&StyleBorderTopColorValue> { match self { CssProperty::BorderTopColor(f) => Some(f), _ => None } }
    pub const fn as_border_left_color(&self) -> Option<&StyleBorderLeftColorValue> { match self { CssProperty::BorderLeftColor(f) => Some(f), _ => None } }
    pub const fn as_border_right_color(&self) -> Option<&StyleBorderRightColorValue> { match self { CssProperty::BorderRightColor(f) => Some(f), _ => None } }
    pub const fn as_border_bottom_color(&self) -> Option<&StyleBorderBottomColorValue> { match self { CssProperty::BorderBottomColor(f) => Some(f), _ => None } }
    pub const fn as_border_top_style(&self) -> Option<&StyleBorderTopStyleValue> { match self { CssProperty::BorderTopStyle(f) => Some(f), _ => None } }
    pub const fn as_border_left_style(&self) -> Option<&StyleBorderLeftStyleValue> { match self { CssProperty::BorderLeftStyle(f) => Some(f), _ => None } }
    pub const fn as_border_right_style(&self) -> Option<&StyleBorderRightStyleValue> { match self { CssProperty::BorderRightStyle(f) => Some(f), _ => None } }
    pub const fn as_border_bottom_style(&self) -> Option<&StyleBorderBottomStyleValue> { match self { CssProperty::BorderBottomStyle(f) => Some(f), _ => None } }
    pub const fn as_border_top_left_radius(&self) -> Option<&StyleBorderTopLeftRadiusValue> { match self { CssProperty::BorderTopLeftRadius(f) => Some(f), _ => None } }
    pub const fn as_border_top_right_radius(&self) -> Option<&StyleBorderTopRightRadiusValue> { match self { CssProperty::BorderTopRightRadius(f) => Some(f), _ => None } }
    pub const fn as_border_bottom_left_radius(&self) -> Option<&StyleBorderBottomLeftRadiusValue> { match self { CssProperty::BorderBottomLeftRadius(f) => Some(f), _ => None } }
    pub const fn as_border_bottom_right_radius(&self) -> Option<&StyleBorderBottomRightRadiusValue> { match self { CssProperty::BorderBottomRightRadius(f) => Some(f), _ => None } }
    pub const fn as_opacity(&self) -> Option<&StyleOpacityValue> { match self { CssProperty::Opacity(f) => Some(f), _ => None } }
    pub const fn as_transform(&self) -> Option<&StyleTransformVecValue> { match self { CssProperty::Transform(f) => Some(f), _ => None } }
    pub const fn as_transform_origin(&self) -> Option<&StyleTransformOriginValue> { match self { CssProperty::TransformOrigin(f) => Some(f), _ => None } }
    pub const fn as_perspective_origin(&self) -> Option<&StylePerspectiveOriginValue> { match self { CssProperty::PerspectiveOrigin(f) => Some(f), _ => None } }
    pub const fn as_backface_visibility(&self) -> Option<&StyleBackfaceVisibilityValue> { match self { CssProperty::BackfaceVisibility(f) => Some(f), _ => None } }
    pub const fn as_mix_blend_mode(&self) -> Option<&StyleMixBlendModeValue> { match self { CssProperty::MixBlendMode(f) => Some(f), _ => None } }
    pub const fn as_filter(&self) -> Option<&StyleFilterVecValue> { match self { CssProperty::Filter(f) => Some(f), _ => None } }
    pub const fn as_backdrop_filter(&self) -> Option<&StyleFilterVecValue> { match self { CssProperty::BackdropFilter(f) => Some(f), _ => None } }
    pub const fn as_text_shadow(&self) -> Option<&StyleBoxShadowValue> { match self { CssProperty::TextShadow(f) => Some(f), _ => None } }
    pub const fn as_display(&self) -> Option<&crate::properties::display::LayoutDisplayValue> { match self { CssProperty::Display(f) => Some(f), _ => None } }
    pub const fn as_float(&self) -> Option<&crate::properties::float::LayoutFloatValue> { match self { CssProperty::Float(f) => Some(f), _ => None } }
    pub const fn as_box_sizing(&self) -> Option<&crate::properties::box_sizing::LayoutBoxSizingValue> { match self { CssProperty::BoxSizing(f) => Some(f), _ => None } }
    pub const fn as_width(&self) -> Option<&crate::properties::width::LayoutWidthValue> { match self { CssProperty::Width(f) => Some(f), _ => None } }
    pub const fn as_height(&self) -> Option<&crate::properties::height::LayoutHeightValue> { match self { CssProperty::Height(f) => Some(f), _ => None } }
    pub const fn as_min_width(&self) -> Option<&crate::properties::min_width::LayoutMinWidthValue> { match self { CssProperty::MinWidth(f) => Some(f), _ => None } }
    pub const fn as_min_height(&self) -> Option<&crate::properties::min_height::LayoutMinHeightValue> { match self { CssProperty::MinHeight(f) => Some(f), _ => None } }
    pub const fn as_max_width(&self) -> Option<&crate::properties::max_width::LayoutMaxWidthValue> { match self { CssProperty::MaxWidth(f) => Some(f), _ => None } }
    pub const fn as_max_height(&self) -> Option<&crate::properties::max_height::LayoutMaxHeightValue> { match self { CssProperty::MaxHeight(f) => Some(f), _ => None } }
    pub const fn as_position(&self) -> Option<&crate::properties::position::LayoutPositionValue> { match self { CssProperty::Position(f) => Some(f), _ => None } }
    pub const fn as_top(&self) -> Option<&crate::properties::top::LayoutTopValue> { match self { CssProperty::Top(f) => Some(f), _ => None } }
    pub const fn as_bottom(&self) -> Option<&crate::properties::bottom::LayoutBottomValue> { match self { CssProperty::Bottom(f) => Some(f), _ => None } }
    pub const fn as_right(&self) -> Option<&crate::properties::right::LayoutRightValue> { match self { CssProperty::Right(f) => Some(f), _ => None } }
    pub const fn as_left(&self) -> Option<&crate::properties::left::LayoutLeftValue> { match self { CssProperty::Left(f) => Some(f), _ => None } }
    pub const fn as_padding_top(&self) -> Option<&LayoutPaddingTopValue> { match self { CssProperty::PaddingTop(f) => Some(f), _ => None } }
    pub const fn as_padding_bottom(&self) -> Option<&LayoutPaddingBottomValue> { match self { CssProperty::PaddingBottom(f) => Some(f), _ => None } }
    pub const fn as_padding_left(&self) -> Option<&LayoutPaddingLeftValue> { match self { CssProperty::PaddingLeft(f) => Some(f), _ => None } }
    pub const fn as_padding_right(&self) -> Option<&LayoutPaddingRightValue> { match self { CssProperty::PaddingRight(f) => Some(f), _ => None } }
    pub const fn as_margin_top(&self) -> Option<&LayoutMarginTopValue> { match self { CssProperty::MarginTop(f) => Some(f), _ => None } }
    pub const fn as_margin_bottom(&self) -> Option<&LayoutMarginBottomValue> { match self { CssProperty::MarginBottom(f) => Some(f), _ => None } }
    pub const fn as_margin_left(&self) -> Option<&LayoutMarginLeftValue> { match self { CssProperty::MarginLeft(f) => Some(f), _ => None } }
    pub const fn as_margin_right(&self) -> Option<&LayoutMarginRightValue> { match self { CssProperty::MarginRight(f) => Some(f), _ => None } }
    pub const fn as_border_top_width(&self) -> Option<&LayoutBorderTopWidthValue> { match self { CssProperty::BorderTopWidth(f) => Some(f), _ => None } }
    pub const fn as_border_left_width(&self) -> Option<&LayoutBorderLeftWidthValue> { match self { CssProperty::BorderLeftWidth(f) => Some(f), _ => None } }
    pub const fn as_border_right_width(&self) -> Option<&LayoutBorderRightWidthValue> { match self { CssProperty::BorderRightWidth(f) => Some(f), _ => None } }
    pub const fn as_border_bottom_width(&self) -> Option<&LayoutBorderBottomWidthValue> { match self { CssProperty::BorderBottomWidth(f) => Some(f), _ => None } }
    pub const fn as_overflow_x(&self) -> Option<&LayoutOverflowValue> { match self { CssProperty::OverflowX(f) => Some(f), _ => None } }
    pub const fn as_overflow_y(&self) -> Option<&LayoutOverflowValue> { match self { CssProperty::OverflowY(f) => Some(f), _ => None } }
    pub const fn as_flex_direction(&self) -> Option<&crate::properties::flex_direction::LayoutFlexDirectionValue> { match self { CssProperty::FlexDirection(f) => Some(f), _ => None } }
    pub const fn as_direction(&self) -> Option<&StyleDirectionValue> { match self { CssProperty::Direction(f) => Some(f), _ => None } }
    pub const fn as_hyphens(&self) -> Option<&StyleHyphensValue> { match self { CssProperty::Hyphens(f) => Some(f), _ => None } }
    pub const fn as_white_space(&self) -> Option<&StyleWhiteSpaceValue> { match self { CssProperty::WhiteSpace(f) => Some(f), _ => None } }
    pub const fn as_flex_wrap(&self) -> Option<&crate::properties::flex_wrap::LayoutFlexWrapValue> { match self { CssProperty::FlexWrap(f) => Some(f), _ => None } }
    pub const fn as_flex_grow(&self) -> Option<&crate::properties::flex_grow::LayoutFlexGrowValue> { match self { CssProperty::FlexGrow(f) => Some(f), _ => None } }
    pub const fn as_flex_shrink(&self) -> Option<&crate::properties::flex_shrink::LayoutFlexShrinkValue> { match self { CssProperty::FlexShrink(f) => Some(f), _ => None } }
    pub const fn as_justify_content(&self) -> Option<&crate::properties::justify_content::LayoutJustifyContentValue> { match self { CssProperty::JustifyContent(f) => Some(f), _ => None } }
    pub const fn as_align_items(&self) -> Option<&crate::properties::align_items::LayoutAlignItemsValue> { match self { CssProperty::AlignItems(f) => Some(f), _ => None } }
    pub const fn as_align_content(&self) -> Option<&crate::properties::align_content::LayoutAlignContentValue> { match self { CssProperty::AlignContent(f) => Some(f), _ => None } }
}

macro_rules! impl_from_css_prop {
    ($a:ident, $b:ident::$enum_type:ident) => {
        impl From<$a> for $b {
            fn from(e: $a) -> Self {
                $b::$enum_type(CssPropertyValue::from(e))
            }
        }
    };
    (crate::properties::$mod_name:ident::$a:ident, $b:ident::$enum_type:ident) => {
        impl From<crate::properties::$mod_name::$a> for $b {
            fn from(e: crate::properties::$mod_name::$a) -> Self {
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
impl_from_css_prop!(crate::properties::display::LayoutDisplay, CssProperty::Display);
impl_from_css_prop!(crate::properties::float::LayoutFloat, CssProperty::Float);
impl_from_css_prop!(crate::properties::box_sizing::LayoutBoxSizing, CssProperty::BoxSizing);
impl_from_css_prop!(crate::properties::width::LayoutWidth, CssProperty::Width);
impl_from_css_prop!(crate::properties::height::LayoutHeight, CssProperty::Height);
impl_from_css_prop!(crate::properties::min_width::LayoutMinWidth, CssProperty::MinWidth);
impl_from_css_prop!(crate::properties::min_height::LayoutMinHeight, CssProperty::MinHeight);
impl_from_css_prop!(crate::properties::max_width::LayoutMaxWidth, CssProperty::MaxWidth);
impl_from_css_prop!(crate::properties::max_height::LayoutMaxHeight, CssProperty::MaxHeight);
impl_from_css_prop!(crate::properties::position::LayoutPosition, CssProperty::Position);
impl_from_css_prop!(crate::properties::top::LayoutTop, CssProperty::Top);
impl_from_css_prop!(crate::properties::left::LayoutLeft, CssProperty::Left);
impl_from_css_prop!(crate::properties::right::LayoutRight, CssProperty::Right);
impl_from_css_prop!(crate::properties::bottom::LayoutBottom, CssProperty::Bottom);
impl_from_css_prop!(crate::properties::flex_wrap::LayoutFlexWrap, CssProperty::FlexWrap);
impl_from_css_prop!(crate::properties::flex_direction::LayoutFlexDirection, CssProperty::FlexDirection);
impl_from_css_prop!(crate::properties::flex_grow::LayoutFlexGrow, CssProperty::FlexGrow);
impl_from_css_prop!(crate::properties::flex_shrink::LayoutFlexShrink, CssProperty::FlexShrink);
impl_from_css_prop!(crate::properties::justify_content::LayoutJustifyContent, CssProperty::JustifyContent);
impl_from_css_prop!(crate::properties::align_items::LayoutAlignItems, CssProperty::AlignItems);
impl_from_css_prop!(crate::properties::align_content::LayoutAlignContent, CssProperty::AlignContent);
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
impl_from_css_prop!(StyleBorderBottomLeftRadius, CssProperty::BorderBottomLeftRadius);
impl_from_css_prop!(StyleBorderBottomRightRadius, CssProperty::BorderBottomRightRadius);
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

const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValueNoPercent {
    pub inner: PixelValue,
}

impl PixelValueNoPercent {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl_option!(
    PixelValueNoPercent,
    OptionPixelValueNoPercent,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for PixelValueNoPercent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ::core::fmt::Debug for PixelValueNoPercent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl PixelValueNoPercent {
    pub fn to_pixels(&self) -> f32 {
        self.inner.to_pixels(0.0)
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AngleValue {
    pub metric: AngleMetric,
    pub number: FloatValue,
}

impl_option!(
    AngleValue,
    OptionAngleValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Debug for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for AngleValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AngleMetric {
    Degree,
    Radians,
    Grad,
    Turn,
    Percent,
}

impl Default for AngleMetric {
    fn default() -> AngleMetric {
        AngleMetric::Degree
    }
}

impl fmt::Display for AngleMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AngleMetric::*;
        match self {
            Degree => write!(f, "deg"),
            Radians => write!(f, "rad"),
            Grad => write!(f, "grad"),
            Turn => write!(f, "turn"),
            Percent => write!(f, "%"),
        }
    }
}

impl AngleValue {
    #[inline] pub const fn zero() -> Self { const ZERO_DEG: AngleValue = AngleValue::const_deg(0); ZERO_DEG }
    #[inline] pub const fn const_deg(value: isize) -> Self { Self::const_from_metric(AngleMetric::Degree, value) }
    #[inline] pub const fn const_rad(value: isize) -> Self { Self::const_from_metric(AngleMetric::Radians, value) }
    #[inline] pub const fn const_grad(value: isize) -> Self { Self::const_from_metric(AngleMetric::Grad, value) }
    #[inline] pub const fn const_turn(value: isize) -> Self { Self::const_from_metric(AngleMetric::Turn, value) }
    #[inline] pub fn const_percent(value: isize) -> Self { Self::const_from_metric(AngleMetric::Percent, value) }
    #[inline] pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self { Self { metric, number: FloatValue::const_new(value) } }
    #[inline] pub fn deg(value: f32) -> Self { Self::from_metric(AngleMetric::Degree, value) }
    #[inline] pub fn rad(value: f32) -> Self { Self::from_metric(AngleMetric::Radians, value) }
    #[inline] pub fn grad(value: f32) -> Self { Self::from_metric(AngleMetric::Grad, value) }
    #[inline] pub fn turn(value: f32) -> Self { Self::from_metric(AngleMetric::Turn, value) }
    #[inline] pub fn percent(value: f32) -> Self { Self::from_metric(AngleMetric::Percent, value) }
    #[inline] pub fn from_metric(metric: AngleMetric, value: f32) -> Self { Self { metric, number: FloatValue::new(value) } }
    #[inline] pub fn to_degrees(&self) -> f32 { let val = match self.metric { AngleMetric::Degree => self.number.get(), AngleMetric::Radians => self.number.get() / 400.0 * 360.0, AngleMetric::Grad => self.number.get() / (2.0 * core::f32::consts::PI) * 360.0, AngleMetric::Turn => self.number.get() * 360.0, AngleMetric::Percent => self.number.get() / 100.0 * 360.0 }; let mut val = val % 360.0; if val < 0.0 { val = 360.0 + val; } val }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

impl PixelValue {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.number = FloatValue::new(self.number.get() * scale_factor);
    }
}

impl fmt::Debug for PixelValue { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}{}", self.number, self.metric) } }
impl fmt::Display for PixelValue { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}{}", self.number, self.metric) } }

impl fmt::Display for SizeMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SizeMetric::*;
        match self { Px => write!(f, "px"), Pt => write!(f, "pt"), Em => write!(f, "em"), In => write!(f, "in"), Cm => write!(f, "cm"), Mm => write!(f, "mm"), Percent => write!(f, "%") }
    }
}

impl PixelValue {
    #[inline] pub const fn zero() -> Self { const ZERO_PX: PixelValue = PixelValue::const_px(0); ZERO_PX }
    #[inline] pub const fn const_px(value: isize) -> Self { Self::const_from_metric(SizeMetric::Px, value) }
    #[inline] pub const fn const_em(value: isize) -> Self { Self::const_from_metric(SizeMetric::Em, value) }
    #[inline] pub const fn const_pt(value: isize) -> Self { Self::const_from_metric(SizeMetric::Pt, value) }
    #[inline] pub const fn const_percent(value: isize) -> Self { Self::const_from_metric(SizeMetric::Percent, value) }
    #[inline] pub const fn const_in(value: isize) -> Self { Self::const_from_metric(SizeMetric::In, value) }
    #[inline] pub const fn const_cm(value: isize) -> Self { Self::const_from_metric(SizeMetric::Cm, value) }
    #[inline] pub const fn const_mm(value: isize) -> Self { Self::const_from_metric(SizeMetric::Mm, value) }
    #[inline] pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self { Self { metric, number: FloatValue::const_new(value) } }
    #[inline] pub fn px(value: f32) -> Self { Self::from_metric(SizeMetric::Px, value) }
    #[inline] pub fn em(value: f32) -> Self { Self::from_metric(SizeMetric::Em, value) }
    #[inline] pub fn inch(value: f32) -> Self { Self::from_metric(SizeMetric::In, value) }
    #[inline] pub fn cm(value: f32) -> Self { Self::from_metric(SizeMetric::Cm, value) }
    #[inline] pub fn mm(value: f32) -> Self { Self::from_metric(SizeMetric::Mm, value) }
    #[inline] pub fn pt(value: f32) -> Self { Self::from_metric(SizeMetric::Pt, value) }
    #[inline] pub fn percent(value: f32) -> Self { Self::from_metric(SizeMetric::Percent, value) }
    #[inline] pub fn from_metric(metric: SizeMetric, value: f32) -> Self { Self { metric, number: FloatValue::new(value) } }
    #[inline] pub fn interpolate(&self, other: &Self, t: f32) -> Self { if self.metric == other.metric { Self { metric: self.metric, number: self.number.interpolate(&other.number, t) } } else { let self_px_interp = self.to_pixels(0.0); let other_px_interp = other.to_pixels(0.0); Self::from_metric(SizeMetric::Px, self_px_interp + (other_px_interp - self_px_interp) * t) } }
    #[inline] pub fn to_pixels_no_percent(&self) -> Option<f32> { match self.metric { SizeMetric::Px => Some(self.number.get()), SizeMetric::Pt => Some(self.number.get() * PT_TO_PX), SizeMetric::Em => Some(self.number.get() * EM_HEIGHT), SizeMetric::In => Some(self.number.get() * 96.0), SizeMetric::Cm => Some(self.number.get() * 96.0 / 2.54), SizeMetric::Mm => Some(self.number.get() * 96.0 / 25.4), SizeMetric::Percent => None } }
    #[inline] pub fn to_pixels(&self, percent_resolve: f32) -> f32 { match self.metric { SizeMetric::Percent => self.number.get() / 100.0 * percent_resolve, _ => self.to_pixels_no_percent().unwrap_or(0.0) } }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PercentageValue {
    number: FloatValue,
}

impl_option!(PercentageValue, OptionPercentageValue, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl fmt::Display for PercentageValue { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}%", self.normalized() * 100.0) } }
impl PercentageValue {
    #[inline] pub const fn const_new(value: isize) -> Self { Self { number: FloatValue::const_new(value) } }
    #[inline] pub fn new(value: f32) -> Self { Self { number: value.into() } }
    #[inline] pub fn normalized(&self) -> f32 { self.number.get() / 100.0 }
    #[inline] pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { number: self.number.interpolate(&other.number, t) } }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FloatValue {
    pub number: isize,
}

impl fmt::Display for FloatValue { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.get()) } }
impl ::core::fmt::Debug for FloatValue { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{}", self) } }
impl Default for FloatValue { fn default() -> Self { const DEFAULT_FLV: FloatValue = FloatValue::const_new(0); DEFAULT_FLV } }

impl FloatValue {
    #[inline] pub const fn const_new(value: isize) -> Self { Self { number: value * FP_PRECISION_MULTIPLIER_CONST } }
    #[inline] pub fn new(value: f32) -> Self { Self { number: (value * FP_PRECISION_MULTIPLIER) as isize } }
    #[inline] pub fn get(&self) -> f32 { self.number as f32 / FP_PRECISION_MULTIPLIER }
    #[inline] pub fn interpolate(&self, other: &Self, t: f32) -> Self { let self_val_f32 = self.get(); let other_val_f32 = other.get(); let interpolated = self_val_f32 + ((other_val_f32 - self_val_f32) * t); Self::new(interpolated) }
}

impl From<f32> for FloatValue { #[inline] fn from(val: f32) -> Self { Self::new(val) } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SizeMetric { Px, Pt, Em, In, Cm, Mm, Percent }
impl Default for SizeMetric { fn default() -> Self { SizeMetric::Px } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum StyleBackgroundSize { ExactSize([PixelValue; 2]), Contain, Cover }
impl StyleBackgroundSize { pub fn scale_for_dpi(&mut self, scale_factor: f32) { match self { StyleBackgroundSize::ExactSize(a) => { for q in a.iter_mut() { q.scale_for_dpi(scale_factor); } } _ => {} } } }
impl Default for StyleBackgroundSize { fn default() -> Self { StyleBackgroundSize::Contain } }
impl_vec!(StyleBackgroundSize, StyleBackgroundSizeVec, StyleBackgroundSizeVecDestructor);
impl_vec_debug!(StyleBackgroundSize, StyleBackgroundSizeVec); impl_vec_partialord!(StyleBackgroundSize, StyleBackgroundSizeVec); impl_vec_ord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_clone!(StyleBackgroundSize, StyleBackgroundSizeVec, StyleBackgroundSizeVecDestructor);
impl_vec_partialeq!(StyleBackgroundSize, StyleBackgroundSizeVec); impl_vec_eq!(StyleBackgroundSize, StyleBackgroundSizeVec); impl_vec_hash!(StyleBackgroundSize, StyleBackgroundSizeVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBackgroundPosition { pub horizontal: BackgroundPositionHorizontal, pub vertical: BackgroundPositionVertical }
impl StyleBackgroundPosition { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.horizontal.scale_for_dpi(scale_factor); self.vertical.scale_for_dpi(scale_factor); } }
impl_vec!(StyleBackgroundPosition, StyleBackgroundPositionVec, StyleBackgroundPositionVecDestructor);
impl_vec_debug!(StyleBackgroundPosition, StyleBackgroundPositionVec); impl_vec_partialord!(StyleBackgroundPosition, StyleBackgroundPositionVec); impl_vec_ord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_clone!(StyleBackgroundPosition, StyleBackgroundPositionVec, StyleBackgroundPositionVecDestructor);
impl_vec_partialeq!(StyleBackgroundPosition, StyleBackgroundPositionVec); impl_vec_eq!(StyleBackgroundPosition, StyleBackgroundPositionVec); impl_vec_hash!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl Default for StyleBackgroundPosition { fn default() -> Self { StyleBackgroundPosition { horizontal: BackgroundPositionHorizontal::Left, vertical: BackgroundPositionVertical::Top } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum BackgroundPositionHorizontal { Left, Center, Right, Exact(PixelValue) }
impl BackgroundPositionHorizontal { pub fn scale_for_dpi(&mut self, scale_factor: f32) { match self { BackgroundPositionHorizontal::Exact(s) => { s.scale_for_dpi(scale_factor); } _ => {} } } }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum BackgroundPositionVertical { Top, Center, Bottom, Exact(PixelValue) }
impl BackgroundPositionVertical { pub fn scale_for_dpi(&mut self, scale_factor: f32) { match self { BackgroundPositionVertical::Exact(s) => { s.scale_for_dpi(scale_factor); } _ => {} } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleBackgroundRepeat { NoRepeat, Repeat, RepeatX, RepeatY }
impl_vec!(StyleBackgroundRepeat, StyleBackgroundRepeatVec, StyleBackgroundRepeatVecDestructor);
impl_vec_debug!(StyleBackgroundRepeat, StyleBackgroundRepeatVec); impl_vec_partialord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec); impl_vec_ord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_clone!(StyleBackgroundRepeat, StyleBackgroundRepeatVec, StyleBackgroundRepeatVecDestructor);
impl_vec_partialeq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec); impl_vec_eq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec); impl_vec_hash!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl Default for StyleBackgroundRepeat { fn default() -> Self { StyleBackgroundRepeat::Repeat } }

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTextColor { pub inner: ColorU }
derive_debug_zero!(StyleTextColor); derive_display_zero!(StyleTextColor);
impl StyleTextColor { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { inner: self.inner.interpolate(&other.inner, t) } } }

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderTopLeftRadius { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderBottomLeftRadius { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderTopRightRadius { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderBottomRightRadius { pub inner: PixelValue }
impl_pixel_value!(StyleBorderTopLeftRadius); impl_pixel_value!(StyleBorderBottomLeftRadius); impl_pixel_value!(StyleBorderTopRightRadius); impl_pixel_value!(StyleBorderBottomRightRadius);

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct LayoutBorderTopWidth { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct LayoutBorderLeftWidth { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct LayoutBorderRightWidth { pub inner: PixelValue }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct LayoutBorderBottomWidth { pub inner: PixelValue }
impl_pixel_value!(LayoutBorderTopWidth); impl_pixel_value!(LayoutBorderLeftWidth); impl_pixel_value!(LayoutBorderRightWidth); impl_pixel_value!(LayoutBorderBottomWidth);

impl CssPropertyValue<StyleBorderTopLeftRadius> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<StyleBorderTopRightRadius> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<StyleBorderBottomLeftRadius> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<StyleBorderBottomRightRadius> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<LayoutBorderTopWidth> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<LayoutBorderRightWidth> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<LayoutBorderBottomWidth> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }
impl CssPropertyValue<LayoutBorderLeftWidth> { pub fn scale_for_dpi(&mut self, scale_factor: f32) { if let CssPropertyValue::Exact(s) = self { s.scale_for_dpi(scale_factor); } } }

impl StyleBorderTopLeftRadius { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl StyleBorderTopRightRadius { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl StyleBorderBottomLeftRadius { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl StyleBorderBottomRightRadius { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl LayoutBorderTopWidth { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl LayoutBorderRightWidth { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl LayoutBorderBottomWidth { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }
impl LayoutBorderLeftWidth { pub fn scale_for_dpi(&mut self, scale_factor: f32) { self.inner.scale_for_dpi(scale_factor); } }

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderTopStyle { pub inner: BorderStyle, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderLeftStyle { pub inner: BorderStyle, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderRightStyle { pub inner: BorderStyle, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderBottomStyle { pub inner: BorderStyle, }
derive_debug_zero!(StyleBorderTopStyle); derive_debug_zero!(StyleBorderLeftStyle); derive_debug_zero!(StyleBorderBottomStyle); derive_debug_zero!(StyleBorderRightStyle);
derive_display_zero!(StyleBorderTopStyle); derive_display_zero!(StyleBorderLeftStyle); derive_display_zero!(StyleBorderBottomStyle); derive_display_zero!(StyleBorderRightStyle);

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderTopColor { pub inner: ColorU, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderLeftColor { pub inner: ColorU, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderRightColor { pub inner: ColorU, }
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBorderBottomColor { pub inner: ColorU, }
impl StyleBorderTopColor { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { inner: self.inner.interpolate(&other.inner, t) } } }
impl StyleBorderLeftColor { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { inner: self.inner.interpolate(&other.inner, t) } } }
impl StyleBorderRightColor { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { inner: self.inner.interpolate(&other.inner, t) } } }
impl StyleBorderBottomColor { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { inner: self.inner.interpolate(&other.inner, t) } } }
derive_debug_zero!(StyleBorderTopColor); derive_debug_zero!(StyleBorderLeftColor); derive_debug_zero!(StyleBorderRightColor); derive_debug_zero!(StyleBorderBottomColor);
derive_display_zero!(StyleBorderTopColor); derive_display_zero!(StyleBorderLeftColor); derive_display_zero!(StyleBorderRightColor); derive_display_zero!(StyleBorderBottomColor);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct StyleBorderSide { pub border_width: PixelValue, pub border_style: BorderStyle, pub border_color: ColorU }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleBoxShadow { pub offset: [PixelValueNoPercent; 2], pub color: ColorU, pub blur_radius: PixelValueNoPercent, pub spread_radius: PixelValueNoPercent, pub clip_mode: BoxShadowClipMode }
impl StyleBoxShadow { pub fn scale_for_dpi(&mut self, scale_factor: f32) { for s in self.offset.iter_mut() { s.scale_for_dpi(scale_factor); } self.blur_radius.scale_for_dpi(scale_factor); self.spread_radius.scale_for_dpi(scale_factor); } }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum StyleBackgroundContent { LinearGradient(LinearGradient), RadialGradient(RadialGradient), ConicGradient(ConicGradient), Image(AzString), Color(ColorU) }
impl_vec!(StyleBackgroundContent, StyleBackgroundContentVec, StyleBackgroundContentVecDestructor);
impl_vec_debug!(StyleBackgroundContent, StyleBackgroundContentVec); impl_vec_partialord!(StyleBackgroundContent, StyleBackgroundContentVec); impl_vec_ord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_clone!(StyleBackgroundContent, StyleBackgroundContentVec, StyleBackgroundContentVecDestructor);
impl_vec_partialeq!(StyleBackgroundContent, StyleBackgroundContentVec); impl_vec_eq!(StyleBackgroundContent, StyleBackgroundContentVec); impl_vec_hash!(StyleBackgroundContent, StyleBackgroundContentVec);
impl Default for StyleBackgroundContent { fn default() -> StyleBackgroundContent { StyleBackgroundContent::Color(ColorU::TRANSPARENT) } }
impl<'a> From<AzString> for StyleBackgroundContent { fn from(id: AzString) -> Self { StyleBackgroundContent::Image(id) } }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct LinearGradient { pub direction: Direction, pub extend_mode: ExtendMode, pub stops: NormalizedLinearColorStopVec }
impl Default for LinearGradient { fn default() -> Self { Self { direction: Direction::default(), extend_mode: ExtendMode::default(), stops: Vec::new().into() } } }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct ConicGradient { pub extend_mode: ExtendMode, pub center: StyleBackgroundPosition, pub angle: AngleValue, pub stops: NormalizedRadialColorStopVec }
impl Default for ConicGradient { fn default() -> Self { Self { extend_mode: ExtendMode::default(), center: StyleBackgroundPosition { horizontal: BackgroundPositionHorizontal::Center, vertical: BackgroundPositionVertical::Center }, angle: AngleValue::default(), stops: Vec::new().into() } } }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct NormalizedLinearColorStop { pub offset: PercentageValue, pub color: ColorU }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct NormalizedRadialColorStop { pub angle: AngleValue, pub color: ColorU }
impl LinearColorStop { pub fn get_normalized_linear_stops(stops_in: &[LinearColorStop]) -> Vec<NormalizedLinearColorStop> { const MIN_STOP_DEGREE:f32=0.0; const MAX_STOP_DEGREE:f32=100.0; if stops_in.is_empty(){return Vec::new()} let self_stops=stops_in; let mut stops=self_stops.iter().map(|s|NormalizedLinearColorStop{offset:s.offset.as_ref().copied().unwrap_or(PercentageValue::new(MIN_STOP_DEGREE)),color:s.color}).collect::<Vec<_>>(); let mut stops_to_distribute=0; let mut last_stop=None; let stops_len=stops.len(); for(stop_id,stop)in self_stops.iter().enumerate(){if let Some(s)=stop.offset.into_option(){let current_stop_val=s.normalized()*100.0; if stops_to_distribute!=0{let last_stop_val=stops[(stop_id-stops_to_distribute)].offset.normalized()*100.0; let value_to_add_per_stop=(current_stop_val.max(last_stop_val)-last_stop_val)/(stops_to_distribute-1)as f32; for(s_id,s_val)in stops[(stop_id-stops_to_distribute)..stop_id].iter_mut().enumerate(){s_val.offset=PercentageValue::new(last_stop_val+(s_id as f32*value_to_add_per_stop))}} stops_to_distribute=0; last_stop=Some(s)}else{stops_to_distribute+=1}} if stops_to_distribute!=0{let last_stop_val=last_stop.unwrap_or(PercentageValue::new(MIN_STOP_DEGREE)).normalized()*100.0; let value_to_add_per_stop=(MAX_STOP_DEGREE.max(last_stop_val)-last_stop_val)/(stops_to_distribute-1)as f32; for(s_id,s_val)in stops[(stops_len-stops_to_distribute)..].iter_mut().enumerate(){s_val.offset=PercentageValue::new(last_stop_val+(s_id as f32*value_to_add_per_stop))}} stops } }
impl RadialColorStop { pub fn get_normalized_radial_stops(stops_in: &[RadialColorStop]) -> Vec<NormalizedRadialColorStop> { const MIN_STOP_DEGREE:f32=0.0; const MAX_STOP_DEGREE:f32=360.0; if stops_in.is_empty(){return Vec::new()} let self_stops=stops_in; let mut stops=self_stops.iter().map(|s|NormalizedRadialColorStop{angle:s.offset.as_ref().copied().unwrap_or(AngleValue::deg(MIN_STOP_DEGREE)),color:s.color}).collect::<Vec<_>>(); let mut stops_to_distribute=0; let mut last_stop=None; let stops_len=stops.len(); for(stop_id,stop)in self_stops.iter().enumerate(){if let Some(s)=stop.offset.into_option(){let current_stop_val=s.to_degrees(); if stops_to_distribute!=0{let last_stop_val=stops[(stop_id-stops_to_distribute)].angle.to_degrees(); let value_to_add_per_stop=(current_stop_val.max(last_stop_val)-last_stop_val)/(stops_to_distribute-1)as f32; for(s_id,s_val)in stops[(stop_id-stops_to_distribute)..stop_id].iter_mut().enumerate(){s_val.angle=AngleValue::deg(last_stop_val+(s_id as f32*value_to_add_per_stop))}} stops_to_distribute=0; last_stop=Some(s)}else{stops_to_distribute+=1}} if stops_to_distribute!=0{let last_stop_val=last_stop.unwrap_or(AngleValue::deg(MIN_STOP_DEGREE)).to_degrees(); let value_to_add_per_stop=(MAX_STOP_DEGREE.max(last_stop_val)-last_stop_val)/(stops_to_distribute-1)as f32; for(s_id,s_val)in stops[(stops_len-stops_to_distribute)..].iter_mut().enumerate(){s_val.angle=AngleValue::deg(last_stop_val+(s_id as f32*value_to_add_per_stop))}} stops } }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct RadialGradient { pub shape: Shape, pub size: RadialGradientSize, pub position: StyleBackgroundPosition, pub extend_mode: ExtendMode, pub stops: NormalizedLinearColorStopVec }
impl Default for RadialGradient { fn default() -> Self { Self { shape: Shape::default(), size: RadialGradientSize::default(), position: StyleBackgroundPosition::default(), extend_mode: ExtendMode::default(), stops: Vec::new().into() } } }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum RadialGradientSize { ClosestSide, ClosestCorner, FarthestSide, FarthestCorner }
impl Default for RadialGradientSize { fn default() -> Self { RadialGradientSize::FarthestCorner } }
impl RadialGradientSize { pub fn get_size(&self, parent_rect: LayoutRect, _gradient_center: crate::properties::position::LayoutPosition) -> LayoutSize { parent_rect.size } } // Adjusted type for gradient_center

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct DirectionCorners { pub from: DirectionCorner, pub to: DirectionCorner }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum Direction { Angle(AngleValue), FromTo(DirectionCorners) }
impl Default for Direction { fn default() -> Self { Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }) } }
impl Direction { pub fn to_points(&self, rect: &LayoutRect) -> (LayoutPoint, LayoutPoint) { match self { Direction::Angle(angle_value) => { let deg = -angle_value.to_degrees(); let width_half = rect.size.width as f32 / 2.0; let height_half = rect.size.height as f32 / 2.0; let hypotenuse_len = libm::hypotf(width_half, height_half); let angle_to_top_left = libm::atanf(height_half / width_half).to_degrees(); let ending_point_degrees = if deg < 90.0 { 90.0 - angle_to_top_left } else if deg < 180.0 { 90.0 + angle_to_top_left } else if deg < 270.0 { 270.0 - angle_to_top_left } else { 270.0 + angle_to_top_left }; let degree_diff_to_corner = ending_point_degrees as f32 - deg; let searched_len = libm::fabsf(libm::cosf(hypotenuse_len * degree_diff_to_corner.to_radians() as f32)); let dx = libm::sinf(deg.to_radians() as f32) * searched_len; let dy = libm::cosf(deg.to_radians() as f32) * searched_len; (LayoutPoint { x: libm::roundf(width_half + dx) as isize, y: libm::roundf(height_half + dy) as isize }, LayoutPoint { x: libm::roundf(width_half - dx) as isize, y: libm::roundf(height_half - dy) as isize }) }, Direction::FromTo(ft) => (ft.from.to_point(rect), ft.to.to_point(rect)) } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum Shape { Ellipse, Circle }
impl Default for Shape { fn default() -> Self { Shape::Ellipse } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleCursor { Alias, AllScroll, Cell, ColResize, ContextMenu, Copy, Crosshair, Default, EResize, EwResize, Grab, Grabbing, Help, Move, NResize, NsResize, NeswResize, NwseResize, Pointer, Progress, RowResize, SResize, SeResize, Text, Unset, VerticalText, WResize, Wait, ZoomIn, ZoomOut }
impl Default for StyleCursor { fn default() -> StyleCursor { StyleCursor::Default } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum DirectionCorner { Right, Left, Top, Bottom, TopRight, TopLeft, BottomRight, BottomLeft }
impl fmt::Display for DirectionCorner { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", match self { DirectionCorner::Right => "right", DirectionCorner::Left => "left", DirectionCorner::Top => "top", DirectionCorner::Bottom => "bottom", DirectionCorner::TopRight => "top right", DirectionCorner::TopLeft => "top left", DirectionCorner::BottomRight => "bottom right", DirectionCorner::BottomLeft => "bottom left" }) } }
impl DirectionCorner { pub const fn opposite(&self) -> Self { use self::DirectionCorner::*; match *self { Right => Left, Left => Right, Top => Bottom, Bottom => Top, TopRight => BottomLeft, BottomLeft => TopRight, TopLeft => BottomRight, BottomRight => TopLeft } } pub const fn combine(&self, other: &Self) -> Option<Self> { use self::DirectionCorner::*; match (*self, *other) { (Right, Top) | (Top, Right) => Some(TopRight), (Left, Top) | (Top, Left) => Some(TopLeft), (Right, Bottom) | (Bottom, Right) => Some(BottomRight), (Left, Bottom) | (Bottom, Left) => Some(BottomLeft), _ => None } } pub const fn to_point(&self, rect: &LayoutRect) -> LayoutPoint { use self::DirectionCorner::*; match *self { Right => LayoutPoint { x: rect.size.width, y: rect.size.height / 2 }, Left => LayoutPoint { x: 0, y: rect.size.height / 2 }, Top => LayoutPoint { x: rect.size.width / 2, y: 0 }, Bottom => LayoutPoint { x: rect.size.width / 2, y: rect.size.height }, TopRight => LayoutPoint { x: rect.size.width, y: 0 }, TopLeft => LayoutPoint { x: 0, y: 0 }, BottomRight => LayoutPoint { x: rect.size.width, y: rect.size.height }, BottomLeft => LayoutPoint { x: 0, y: rect.size.height } } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct RadialColorStop { pub offset: OptionAngleValue, pub color: ColorU }
impl_vec!(NormalizedRadialColorStop, NormalizedRadialColorStopVec, NormalizedRadialColorStopVecDestructor);
impl_vec_debug!(NormalizedRadialColorStop, NormalizedRadialColorStopVec); impl_vec_partialord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec); impl_vec_ord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_clone!(NormalizedRadialColorStop, NormalizedRadialColorStopVec, NormalizedRadialColorStopVecDestructor);
impl_vec_partialeq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec); impl_vec_eq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec); impl_vec_hash!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct LinearColorStop { pub offset: OptionPercentageValue, pub color: ColorU }
impl_vec!(NormalizedLinearColorStop, NormalizedLinearColorStopVec, NormalizedLinearColorStopVecDestructor);
impl_vec_debug!(NormalizedLinearColorStop, NormalizedLinearColorStopVec); impl_vec_partialord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec); impl_vec_ord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_clone!(NormalizedLinearColorStop, NormalizedLinearColorStopVec, NormalizedLinearColorStopVecDestructor);
impl_vec_partialeq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec); impl_vec_eq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec); impl_vec_hash!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);

// Definitions for LayoutTop, LayoutLeft, LayoutRight, LayoutBottom are now in their respective modules.
// Definitions for LayoutPaddingTop, ...Left, ...Right, ...Bottom are now in their respective modules.
// Definitions for LayoutMarginTop, ...Left, ...Right, ...Bottom are now in their respective modules.

// Definitions for LayoutFlexGrow, LayoutFlexShrink, LayoutFlexDirection, LayoutFlexWrap,
// LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent are now in their respective modules.

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleLineHeight { pub inner: PercentageValue }
impl_percentage_value!(StyleLineHeight);
impl Default for StyleLineHeight { fn default() -> Self { Self { inner: PercentageValue::const_new(100) } } }

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTabWidth { pub inner: PercentageValue }
impl_percentage_value!(StyleTabWidth);
impl Default for StyleTabWidth { fn default() -> Self { Self { inner: PercentageValue::const_new(100) } } }

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleLetterSpacing { pub inner: PixelValue }
impl Default for StyleLetterSpacing { fn default() -> Self { Self { inner: PixelValue::const_px(0) } } }
impl_pixel_value!(StyleLetterSpacing);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleWordSpacing { pub inner: PixelValue }
impl_pixel_value!(StyleWordSpacing);
impl Default for StyleWordSpacing { fn default() -> Self { Self { inner: PixelValue::const_px(0) } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum LayoutAxis { Horizontal, Vertical }

// LayoutPosition is now in crate::properties::position
// LayoutOverflow is still here as it's used by OverflowX and OverflowY directly in CssProperty enum
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum LayoutOverflow { Scroll, Auto, Hidden, Visible }
impl Default for LayoutOverflow { fn default() -> Self { LayoutOverflow::Auto } }
impl LayoutOverflow { pub fn needs_scrollbar(&self, currently_overflowing: bool) -> bool { use self::LayoutOverflow::*; match self { Scroll => true, Auto => currently_overflowing, Hidden | Visible => false } } pub fn is_overflow_visible(&self) -> bool { *self == LayoutOverflow::Visible } pub fn is_overflow_hidden(&self) -> bool { *self == LayoutOverflow::Hidden } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleTextAlign { Left, Center, Right, Justify }
impl Default for StyleTextAlign { fn default() -> Self { StyleTextAlign::Left } }
impl_option!(StyleTextAlign, OptionStyleTextAlign, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleDirection { Ltr, Rtl }
impl Default for StyleDirection { fn default() -> Self { StyleDirection::Ltr } }
impl_option!(StyleDirection, OptionStyleDirection, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleHyphens { Auto, None }
impl Default for StyleHyphens { fn default() -> Self { StyleHyphens::Auto } }
impl_option!(StyleHyphens, OptionStyleHyphens, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleWhiteSpace { Normal, Pre, Nowrap }
impl Default for StyleWhiteSpace { fn default() -> Self { StyleWhiteSpace::Normal } }
impl_option!(StyleWhiteSpace, OptionStyleWhiteSpace, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleVerticalAlign { Top, Center, Bottom }
impl Default for StyleVerticalAlign { fn default() -> Self { StyleVerticalAlign::Top } }

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleOpacity { pub inner: PercentageValue }
impl Default for StyleOpacity { fn default() -> Self { StyleOpacity { inner: PercentageValue::const_new(0) } } } // Note: CSS default is 1 (100%), but 0 is often a better practical default if not specified.
impl_percentage_value!(StyleOpacity);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StylePerspectiveOrigin { pub x: PixelValue, pub y: PixelValue }
impl StylePerspectiveOrigin { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { x: self.x.interpolate(&other.x, t), y: self.y.interpolate(&other.y, t) } } }
impl Default for StylePerspectiveOrigin { fn default() -> Self { StylePerspectiveOrigin { x: PixelValue::const_px(0), y: PixelValue::const_px(0) } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformOrigin { pub x: PixelValue, pub y: PixelValue }
impl StyleTransformOrigin { pub fn interpolate(&self, other: &Self, t: f32) -> Self { Self { x: self.x.interpolate(&other.x, t), y: self.y.interpolate(&other.y, t) } } }
impl Default for StyleTransformOrigin { fn default() -> Self { StyleTransformOrigin { x: PixelValue::const_percent(50), y: PixelValue::const_percent(50) } } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub enum StyleBackfaceVisibility { Hidden, Visible }
impl Default for StyleBackfaceVisibility { fn default() -> Self { StyleBackfaceVisibility::Visible } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C, u8)] pub enum StyleTransform { Matrix(StyleTransformMatrix2D), Matrix3D(StyleTransformMatrix3D), Translate(StyleTransformTranslate2D), Translate3D(StyleTransformTranslate3D), TranslateX(PixelValue), TranslateY(PixelValue), TranslateZ(PixelValue), Rotate(AngleValue), Rotate3D(StyleTransformRotate3D), RotateX(AngleValue), RotateY(AngleValue), RotateZ(AngleValue), Scale(StyleTransformScale2D), Scale3D(StyleTransformScale3D), ScaleX(PercentageValue), ScaleY(PercentageValue), ScaleZ(PercentageValue), Skew(StyleTransformSkew2D), SkewX(PercentageValue), SkewY(PercentageValue), Perspective(PixelValue) }
impl_vec!(StyleTransform, StyleTransformVec, StyleTransformVecDestructor);
impl_vec_debug!(StyleTransform, StyleTransformVec); impl_vec_partialord!(StyleTransform, StyleTransformVec); impl_vec_ord!(StyleTransform, StyleTransformVec);
impl_vec_clone!(StyleTransform, StyleTransformVec, StyleTransformVecDestructor);
impl_vec_partialeq!(StyleTransform, StyleTransformVec); impl_vec_eq!(StyleTransform, StyleTransformVec); impl_vec_hash!(StyleTransform, StyleTransformVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformMatrix2D { pub a: PixelValue, pub b: PixelValue, pub c: PixelValue, pub d: PixelValue, pub tx: PixelValue, pub ty: PixelValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformMatrix3D { pub m11: PixelValue, pub m12: PixelValue, pub m13: PixelValue, pub m14: PixelValue, pub m21: PixelValue, pub m22: PixelValue, pub m23: PixelValue, pub m24: PixelValue, pub m31: PixelValue, pub m32: PixelValue, pub m33: PixelValue, pub m34: PixelValue, pub m41: PixelValue, pub m42: PixelValue, pub m43: PixelValue, pub m44: PixelValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformTranslate2D { pub x: PixelValue, pub y: PixelValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformTranslate3D { pub x: PixelValue, pub y: PixelValue, pub z: PixelValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformRotate3D { pub x: PercentageValue, pub y: PercentageValue, pub z: PercentageValue, pub angle: AngleValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformScale2D { pub x: PercentageValue, pub y: PercentageValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformScale3D { pub x: PercentageValue, pub y: PercentageValue, pub z: PercentageValue }
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(C)] pub struct StyleTransformSkew2D { pub x: PercentageValue, pub y: PercentageValue }

// Type aliases for CssPropertyValue wrappers
pub type StyleTextColorValue = CssPropertyValue<StyleTextColor>;
pub type StyleFontSizeValue = CssPropertyValue<StyleFontSize>;
pub type StyleFontFamilyVecValue = CssPropertyValue<StyleFontFamilyVec>;
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
pub type StyleHyphensValue = CssPropertyValue<StyleHyphens>;
pub type StyleDirectionValue = CssPropertyValue<StyleDirection>;
pub type StyleWhiteSpaceValue = CssPropertyValue<StyleWhiteSpace>;

// Value and OptionValue types for properties moved to their own modules are now defined in those modules.
// e.g. pub type LayoutDisplayValue = CssPropertyValue<crate::properties::display::LayoutDisplay>;
//      impl_option!(LayoutDisplayValue, OptionLayoutDisplayValue, ...);

pub type LayoutOverflowValue = CssPropertyValue<LayoutOverflow>;
impl_option!(LayoutOverflowValue, OptionLayoutOverflowValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutPaddingTopValue = CssPropertyValue<LayoutPaddingTop>;
impl_option!(LayoutPaddingTopValue, OptionLayoutPaddingTopValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutPaddingBottomValue = CssPropertyValue<LayoutPaddingBottom>;
impl_option!(LayoutPaddingBottomValue, OptionLayoutPaddingBottomValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutPaddingLeftValue = CssPropertyValue<LayoutPaddingLeft>;
impl_option!(LayoutPaddingLeftValue, OptionLayoutPaddingLeftValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutPaddingRightValue = CssPropertyValue<LayoutPaddingRight>;
impl_option!(LayoutPaddingRightValue, OptionLayoutPaddingRightValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutMarginTopValue = CssPropertyValue<LayoutMarginTop>;
impl_option!(LayoutMarginTopValue, OptionLayoutMarginTopValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutMarginBottomValue = CssPropertyValue<LayoutMarginBottom>;
impl_option!(LayoutMarginBottomValue, OptionLayoutMarginBottomValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutMarginLeftValue = CssPropertyValue<LayoutMarginLeft>;
impl_option!(LayoutMarginLeftValue, OptionLayoutMarginLeftValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
pub type LayoutMarginRightValue = CssPropertyValue<LayoutMarginRight>;
impl_option!(LayoutMarginRightValue, OptionLayoutMarginRightValue, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
pub type LayoutBorderLeftWidthValue = CssPropertyValue<LayoutBorderLeftWidth>;
pub type LayoutBorderRightWidthValue = CssPropertyValue<LayoutBorderRightWidth>;
pub type LayoutBorderBottomWidthValue = CssPropertyValue<LayoutBorderBottomWidth>;

// Definitions for LayoutFlexWrap, LayoutFlexGrow, LayoutFlexShrink,
// LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent, LayoutFlexDirection
// and their associated Value and impl_option! calls have been moved to their respective files
// in the `properties` directory.
// The `impl_float_value!` calls for LayoutFlexGrow and LayoutFlexShrink have also been moved.
// `impl_from_css_prop` has been updated to point to the new locations for these types.
// The `CssProperty` enum and both `css_property_from_type!` macros, as well as
// `const_*` and `as_*` methods in `CssProperty` have been updated to use the new paths.
