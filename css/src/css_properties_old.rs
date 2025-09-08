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

// The following types are present in webrender, however, azul-css should not
// depend on webrender, just to have the same types, azul-css should be a standalone crate.

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

    /// Same as `contains()`, but returns the (x, y) offset of the hit point
    ///
    /// On a regular computer this function takes ~3.2ns to run
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

    /// Faster union for a Vec<LayoutRect>
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

    // Returns the scroll rect (not the union rect) of the parent / children
    #[inline]
    pub fn get_scroll_rect<I: Iterator<Item = Self>>(&self, children: I) -> Option<Self> {
        let children_union = Self::union(children)?;
        Self::union([*self, children_union].iter().map(|r| *r))
    }

    // Returns if b overlaps a
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

macro_rules! derive_debug_zero {
    ($struct:ident) => {
        impl fmt::Debug for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:?}", self.inner)
            }
        }
    };
}

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
    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_css::{CombinedCssPropertyType, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(
    ///     Some(CombinedCssPropertyType::Border),
    ///     CombinedCssPropertyType::from_str("border", &map)
    /// );
    /// ```
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.shorthands.get(input).map(|x| *x)
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
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
    // Contains all keys that have no shorthand
    pub non_shorthands: BTreeMap<&'static str, CssPropertyType>,
    // Contains all keys that act as a shorthand for other types
    pub shorthands: BTreeMap<&'static str, CombinedCssPropertyType>,
}

impl CssKeyMap {
    pub fn get() -> Self {
        get_css_key_map()
    }
}

/// Returns a map useful for parsing the keys of CSS stylesheets
pub fn get_css_key_map() -> CssKeyMap {
    CssKeyMap {
        non_shorthands: CSS_PROPERTY_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect(),
        shorthands: COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .map(|(v, k)| (*k, *v))
            .collect(),
    }
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
    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_css::{CssPropertyType, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(
    ///     Some(CssPropertyType::Width),
    ///     CssPropertyType::from_str("width", &map)
    /// );
    /// assert_eq!(
    ///     Some(CssPropertyType::JustifyContent),
    ///     CssPropertyType::from_str("justify-content", &map)
    /// );
    /// assert_eq!(None, CssPropertyType::from_str("asdfasdfasdf", &map));
    /// ```
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.non_shorthands.get(input).and_then(|x| Some(*x))
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
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

    /// Returns whether this property will be inherited during cascading
    pub fn is_inheritable(&self) -> bool {
        use self::CssPropertyType::*;
        match self {
            TextColor | FontFamily | FontSize | LineHeight | TextAlign => true,
            _ => false,
        }
    }

    /// Returns whether this property can trigger a re-layout (important for incremental layout and
    /// caching layouted DOMs).
    pub fn can_trigger_relayout(&self) -> bool {
        use self::CssPropertyType::*;

        // Since the border can be larger than the content,
        // in which case the content needs to be re-layouted, assume true for Border

        // FontFamily, FontSize, LetterSpacing and LineHeight can affect
        // the text layout and therefore the screen layout

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

    /// Returns whether the property is a GPU property (currently only opacity and transforms)
    pub fn is_gpu_only_property(&self) -> bool {
        match self {
            CssPropertyType::Opacity |
            CssPropertyType::Transform /* | CssPropertyType::Color */ => true,
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
            CssPropertyType::TextColor => {
                CssProperty::TextColor(StyleTextColorValue::$content_type)
            }
            CssPropertyType::FontSize => CssProperty::FontSize(StyleFontSizeValue::$content_type),
            CssPropertyType::FontFamily => {
                CssProperty::FontFamily(StyleFontFamilyVecValue::$content_type)
            }
            CssPropertyType::TextAlign => {
                CssProperty::TextAlign(StyleTextAlignValue::$content_type)
            }
            CssPropertyType::LetterSpacing => {
                CssProperty::LetterSpacing(StyleLetterSpacingValue::$content_type)
            }
            CssPropertyType::LineHeight => {
                CssProperty::LineHeight(StyleLineHeightValue::$content_type)
            }
            CssPropertyType::WordSpacing => {
                CssProperty::WordSpacing(StyleWordSpacingValue::$content_type)
            }
            CssPropertyType::TabWidth => CssProperty::TabWidth(StyleTabWidthValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(StyleCursorValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(LayoutDisplayValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(LayoutFloatValue::$content_type),
            CssPropertyType::BoxSizing => {
                CssProperty::BoxSizing(LayoutBoxSizingValue::$content_type)
            }
            CssPropertyType::Width => CssProperty::Width(LayoutWidthValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(LayoutHeightValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(LayoutMinWidthValue::$content_type),
            CssPropertyType::MinHeight => {
                CssProperty::MinHeight(LayoutMinHeightValue::$content_type)
            }
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(LayoutMaxWidthValue::$content_type),
            CssPropertyType::MaxHeight => {
                CssProperty::MaxHeight(LayoutMaxHeightValue::$content_type)
            }
            CssPropertyType::Position => CssProperty::Position(LayoutPositionValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(LayoutTopValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(LayoutRightValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(LayoutLeftValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(LayoutBottomValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(LayoutFlexWrapValue::$content_type),
            CssPropertyType::FlexDirection => {
                CssProperty::FlexDirection(LayoutFlexDirectionValue::$content_type)
            }
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(LayoutFlexGrowValue::$content_type),
            CssPropertyType::FlexShrink => {
                CssProperty::FlexShrink(LayoutFlexShrinkValue::$content_type)
            }
            CssPropertyType::JustifyContent => {
                CssProperty::JustifyContent(LayoutJustifyContentValue::$content_type)
            }
            CssPropertyType::AlignItems => {
                CssProperty::AlignItems(LayoutAlignItemsValue::$content_type)
            }
            CssPropertyType::AlignContent => {
                CssProperty::AlignContent(LayoutAlignContentValue::$content_type)
            }
            CssPropertyType::BackgroundContent => {
                CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type)
            }
            CssPropertyType::BackgroundPosition => {
                CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::$content_type)
            }
            CssPropertyType::BackgroundSize => {
                CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::$content_type)
            }
            CssPropertyType::BackgroundRepeat => {
                CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::$content_type)
            }
            CssPropertyType::OverflowX => {
                CssProperty::OverflowX(LayoutOverflowValue::$content_type)
            }
            CssPropertyType::OverflowY => {
                CssProperty::OverflowY(LayoutOverflowValue::$content_type)
            }
            CssPropertyType::PaddingTop => {
                CssProperty::PaddingTop(LayoutPaddingTopValue::$content_type)
            }
            CssPropertyType::PaddingLeft => {
                CssProperty::PaddingLeft(LayoutPaddingLeftValue::$content_type)
            }
            CssPropertyType::PaddingRight => {
                CssProperty::PaddingRight(LayoutPaddingRightValue::$content_type)
            }
            CssPropertyType::PaddingBottom => {
                CssProperty::PaddingBottom(LayoutPaddingBottomValue::$content_type)
            }
            CssPropertyType::MarginTop => {
                CssProperty::MarginTop(LayoutMarginTopValue::$content_type)
            }
            CssPropertyType::MarginLeft => {
                CssProperty::MarginLeft(LayoutMarginLeftValue::$content_type)
            }
            CssPropertyType::MarginRight => {
                CssProperty::MarginRight(LayoutMarginRightValue::$content_type)
            }
            CssPropertyType::MarginBottom => {
                CssProperty::MarginBottom(LayoutMarginBottomValue::$content_type)
            }
            CssPropertyType::BorderTopLeftRadius => {
                CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::$content_type)
            }
            CssPropertyType::BorderTopRightRadius => {
                CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::$content_type)
            }
            CssPropertyType::BorderBottomLeftRadius => {
                CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::$content_type)
            }
            CssPropertyType::BorderBottomRightRadius => CssProperty::BorderBottomRightRadius(
                StyleBorderBottomRightRadiusValue::$content_type,
            ),
            CssPropertyType::BorderTopColor => {
                CssProperty::BorderTopColor(StyleBorderTopColorValue::$content_type)
            }
            CssPropertyType::BorderRightColor => {
                CssProperty::BorderRightColor(StyleBorderRightColorValue::$content_type)
            }
            CssPropertyType::BorderLeftColor => {
                CssProperty::BorderLeftColor(StyleBorderLeftColorValue::$content_type)
            }
            CssPropertyType::BorderBottomColor => {
                CssProperty::BorderBottomColor(StyleBorderBottomColorValue::$content_type)
            }
            CssPropertyType::BorderTopStyle => {
                CssProperty::BorderTopStyle(StyleBorderTopStyleValue::$content_type)
            }
            CssPropertyType::BorderRightStyle => {
                CssProperty::BorderRightStyle(StyleBorderRightStyleValue::$content_type)
            }
            CssPropertyType::BorderLeftStyle => {
                CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::$content_type)
            }
            CssPropertyType::BorderBottomStyle => {
                CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::$content_type)
            }
            CssPropertyType::BorderTopWidth => {
                CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::$content_type)
            }
            CssPropertyType::BorderRightWidth => {
                CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::$content_type)
            }
            CssPropertyType::BorderLeftWidth => {
                CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::$content_type)
            }
            CssPropertyType::BorderBottomWidth => {
                CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::$content_type)
            }
            CssPropertyType::BoxShadowLeft => {
                CssProperty::BoxShadowLeft(StyleBoxShadowValue::$content_type)
            }
            CssPropertyType::BoxShadowRight => {
                CssProperty::BoxShadowRight(StyleBoxShadowValue::$content_type)
            }
            CssPropertyType::BoxShadowTop => {
                CssProperty::BoxShadowTop(StyleBoxShadowValue::$content_type)
            }
            CssPropertyType::BoxShadowBottom => {
                CssProperty::BoxShadowBottom(StyleBoxShadowValue::$content_type)
            }
            CssPropertyType::ScrollbarStyle => {
                CssProperty::ScrollbarStyle(ScrollbarStyleValue::$content_type)
            }
            CssPropertyType::Opacity => CssProperty::Opacity(StyleOpacityValue::$content_type),
            CssPropertyType::Transform => {
                CssProperty::Transform(StyleTransformVecValue::$content_type)
            }
            CssPropertyType::PerspectiveOrigin => {
                CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::$content_type)
            }
            CssPropertyType::TransformOrigin => {
                CssProperty::TransformOrigin(StyleTransformOriginValue::$content_type)
            }
            CssPropertyType::BackfaceVisibility => {
                CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::$content_type)
            }
            CssPropertyType::MixBlendMode => {
                CssProperty::MixBlendMode(StyleMixBlendModeValue::$content_type)
            }
            CssPropertyType::Filter => CssProperty::Filter(StyleFilterVecValue::$content_type),
            CssPropertyType::BackdropFilter => {
                CssProperty::BackdropFilter(StyleFilterVecValue::$content_type)
            }
            CssPropertyType::TextShadow => {
                CssProperty::TextShadow(StyleBoxShadowValue::$content_type)
            }
            CssPropertyType::WhiteSpace => {
                CssProperty::WhiteSpace(StyleWhiteSpaceValue::$content_type)
            }
            CssPropertyType::Hyphens => CssProperty::Hyphens(StyleHyphensValue::$content_type),
            CssPropertyType::Direction => {
                CssProperty::Direction(StyleDirectionValue::$content_type)
            }
        }
    }};
}

impl CssProperty {
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
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum AnimationInterpolationFunction {
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(SvgCubicCurve),
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPoint {
    pub x: f32,
    pub y: f32,
}

impl_option!(
    SvgPoint,
    OptionSvgPoint,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl SvgPoint {
    #[inline]
    pub fn distance(&self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        libm::hypotf(dx, dy) as f64
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub radius_top_left: f32,
    pub radius_top_right: f32,
    pub radius_bottom_left: f32,
    pub radius_bottom_right: f32,
}

impl SvgRect {
    pub fn union_with(&mut self, other: &Self) {
        let self_max_x = self.x + self.width;
        let self_max_y = self.y + self.height;
        let self_min_x = self.x;
        let self_min_y = self.y;

        let other_max_x = other.x + other.width;
        let other_max_y = other.y + other.height;
        let other_min_x = other.x;
        let other_min_y = other.y;

        let max_x = self_max_x.max(other_max_x);
        let max_y = self_max_y.max(other_max_y);
        let min_x = self_min_x.min(other_min_x);
        let min_y = self_min_y.min(other_min_y);

        self.x = min_x;
        self.y = min_y;
        self.width = max_x - min_x;
        self.height = max_y - min_y;
    }

    /// Note: does not incorporate rounded edges!
    /// Origin of x and y is assumed to be the top left corner
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x > self.x && x < self.x + self.width && y > self.y && y < self.y + self.height
    }

    /// Expands the rect with a certain amount of padding
    pub fn expand(
        &self,
        padding_top: f32,
        padding_bottom: f32,
        padding_left: f32,
        padding_right: f32,
    ) -> SvgRect {
        SvgRect {
            width: self.width + padding_left + padding_right,
            height: self.height + padding_top + padding_bottom,
            x: self.x - padding_left,
            y: self.y - padding_top,
            ..*self
        }
    }

    pub fn get_center(&self) -> SvgPoint {
        SvgPoint {
            x: self.x + (self.width / 2.0),
            y: self.y + (self.height / 2.0),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgCubicCurve {
    pub start: SvgPoint,
    pub ctrl_1: SvgPoint,
    pub ctrl_2: SvgPoint,
    pub end: SvgPoint,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgVector {
    pub x: f64,
    pub y: f64,
}

impl SvgVector {
    /// Returns the angle of the vector in degrees
    #[inline]
    pub fn angle_degrees(&self) -> f64 {
        //  y
        //  |  /
        //  | /
        //   / a)
        //   ___ x

        (-self.x).atan2(self.y).to_degrees()
    }

    #[inline]
    #[must_use = "returns a new vector"]
    pub fn normalize(&self) -> Self {
        let tangent_length = libm::hypotf(self.x as f32, self.y as f32) as f64;

        Self {
            x: self.x / tangent_length,
            y: self.y / tangent_length,
        }
    }

    /// Rotate the vector 90 degrees counter-clockwise
    #[must_use = "returns a new vector"]
    #[inline]
    pub fn rotate_90deg_ccw(&self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

const STEP_SIZE: usize = 20;
const STEP_SIZE_F64: f64 = 0.05;

impl SvgCubicCurve {
    pub fn reverse(&mut self) {
        let mut temp = self.start;
        self.start = self.end;
        self.end = temp;
        temp = self.ctrl_1;
        self.ctrl_1 = self.ctrl_2;
        self.ctrl_2 = temp;
    }

    pub fn get_start(&self) -> SvgPoint {
        self.start
    }
    pub fn get_end(&self) -> SvgPoint {
        self.end
    }

    // evaluate the curve at t
    pub fn get_x_at_t(&self, t: f64) -> f64 {
        let c_x = 3.0 * (self.ctrl_1.x as f64 - self.start.x as f64);
        let b_x = 3.0 * (self.ctrl_2.x as f64 - self.ctrl_1.x as f64) - c_x;
        let a_x = self.end.x as f64 - self.start.x as f64 - c_x - b_x;

        (a_x * t * t * t) + (b_x * t * t) + (c_x * t) + self.start.x as f64
    }

    pub fn get_y_at_t(&self, t: f64) -> f64 {
        let c_x = 3.0 * (self.ctrl_1.y as f64 - self.start.y as f64);
        let b_x = 3.0 * (self.ctrl_2.y as f64 - self.ctrl_1.y as f64) - c_x;
        let a_x = self.end.y as f64 - self.start.y as f64 - c_x - b_x;

        (a_x * t * t * t) + (b_x * t * t) + (c_x * t) + self.start.y as f64
    }

    pub fn get_length(&self) -> f64 {
        // NOTE: this arc length parametrization is not very precise, but fast
        let mut arc_length = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = (i + 1) as f64 * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: self.get_x_at_t(t_next) as f32,
                y: self.get_y_at_t(t_next) as f32,
            };
            arc_length += prev_point.distance(next_point);
            prev_point = next_point;
        }

        arc_length
    }

    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        // step through the line until the offset is reached,
        // then interpolate linearly between the
        // current at the last sampled point
        let mut arc_length = 0.0;
        let mut t_current = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = (i + 1) as f64 * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: self.get_x_at_t(t_next) as f32,
                y: self.get_y_at_t(t_next) as f32,
            };

            let distance = prev_point.distance(next_point);

            arc_length += distance;

            // linearly interpolate between last t and current t
            if arc_length > offset {
                let remaining = arc_length - offset;
                return t_current + (remaining / distance) * STEP_SIZE_F64;
            }

            prev_point = next_point;
            t_current = t_next;
        }

        t_current
    }

    pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        // 1. Calculate the derivative of the bezier curve.
        //
        // This means that we go from 4 points to 3 points and redistribute
        // the weights of the control points according to the formula:
        //
        // w'0 = 3 * (w1-w0)
        // w'1 = 3 * (w2-w1)
        // w'2 = 3 * (w3-w2)

        let w0 = SvgPoint {
            x: self.ctrl_1.x - self.start.x,
            y: self.ctrl_1.y - self.start.y,
        };

        let w1 = SvgPoint {
            x: self.ctrl_2.x - self.ctrl_1.x,
            y: self.ctrl_2.y - self.ctrl_1.y,
        };

        let w2 = SvgPoint {
            x: self.end.x - self.ctrl_2.x,
            y: self.end.y - self.ctrl_2.y,
        };

        let quadratic_curve = SvgQuadraticCurve {
            start: w0,
            ctrl: w1,
            end: w2,
        };

        // The first derivative of a cubic bezier curve is a quadratic
        // bezier curve. Luckily, the first derivative is also the tangent
        // vector (slope) of the curve. So all we need to do is to sample the
        // quadratic curve at t
        let tangent_vector = SvgVector {
            x: quadratic_curve.get_x_at_t(t),
            y: quadratic_curve.get_y_at_t(t),
        };

        tangent_vector.normalize()
    }

    pub fn get_bounds(&self) -> SvgRect {
        let min_x = self
            .start
            .x
            .min(self.end.x)
            .min(self.ctrl_1.x)
            .min(self.ctrl_2.x);
        let max_x = self
            .start
            .x
            .max(self.end.x)
            .max(self.ctrl_1.x)
            .max(self.ctrl_2.x);

        let min_y = self
            .start
            .y
            .min(self.end.y)
            .min(self.ctrl_1.y)
            .min(self.ctrl_2.y);
        let max_y = self
            .start
            .y
            .max(self.end.y)
            .max(self.ctrl_1.y)
            .max(self.ctrl_2.y);

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        SvgRect {
            width,
            height,
            x: min_x,
            y: min_y,
            ..SvgRect::default()
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgQuadraticCurve {
    pub start: SvgPoint,
    pub ctrl: SvgPoint,
    pub end: SvgPoint,
}

impl SvgQuadraticCurve {
    pub fn reverse(&mut self) {
        let mut temp = self.start;
        self.start = self.end;
        self.end = temp;
    }
    pub fn get_start(&self) -> SvgPoint {
        self.start
    }
    pub fn get_end(&self) -> SvgPoint {
        self.end
    }
    pub fn get_bounds(&self) -> SvgRect {
        let min_x = self.start.x.min(self.end.x).min(self.ctrl.x);
        let max_x = self.start.x.max(self.end.x).max(self.ctrl.x);

        let min_y = self.start.y.min(self.end.y).min(self.ctrl.y);
        let max_y = self.start.y.max(self.end.y).max(self.ctrl.y);

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        SvgRect {
            width,
            height,
            x: min_x,
            y: min_y,
            ..SvgRect::default()
        }
    }

    pub fn get_x_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        let one_minus_squared = one_minus * one_minus;
        let t_squared = t * t;

        1.0 * one_minus_squared * 1.0 * self.start.x as f64
            + 2.0 * one_minus * t * self.ctrl.x as f64
            + 3.0 * 1.0 * t_squared * self.end.x as f64
    }

    pub fn get_y_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        let one_minus_squared = one_minus * one_minus;
        let t_squared = t * t;

        1.0 * one_minus_squared * 1.0 * self.start.y as f64
            + 2.0 * one_minus * t * self.ctrl.y as f64
            + 3.0 * 1.0 * t_squared * self.end.y as f64
    }

    pub fn get_length(&self) -> f64 {
        self.to_cubic().get_length()
    }

    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        self.to_cubic().get_t_at_offset(offset)
    }

    pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        self.to_cubic().get_tangent_vector_at_t(t)
    }

    fn to_cubic(&self) -> SvgCubicCurve {
        SvgCubicCurve {
            start: self.start,
            ctrl_1: SvgPoint {
                x: self.start.x + (0.75 * self.ctrl.x - self.start.x),
                y: self.start.y + (0.75 * self.ctrl.y - self.start.y),
            },
            ctrl_2: SvgPoint {
                x: self.start.x + (0.75 * self.end.x - self.ctrl.x),
                y: self.start.y + (0.75 * self.end.y - self.ctrl.y),
            },
            end: self.end,
        }
    }
}

impl AnimationInterpolationFunction {
    pub const fn get_curve(self) -> SvgCubicCurve {
        match self {
            AnimationInterpolationFunction::Ease => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.25, y: 0.1 },
                ctrl_2: SvgPoint { x: 0.25, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::Linear => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseIn => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseInOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::CubicBezier(c) => c,
        }
    }

    pub fn evaluate(self, t: f64) -> f32 {
        self.get_curve().get_y_at_t(t) as f32
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InterpolateResolver {
    pub interpolate_func: AnimationInterpolationFunction,
    pub parent_rect_width: f32,
    pub parent_rect_height: f32,
    pub current_rect_width: f32,
    pub current_rect_height: f32,
}

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
}

impl_vec!(CssProperty, CssPropertyVec, CssPropertyVecDestructor);
impl_vec_debug!(CssProperty, CssPropertyVec);
impl_vec_partialord!(CssProperty, CssPropertyVec);
impl_vec_ord!(CssProperty, CssPropertyVec);
impl_vec_clone!(CssProperty, CssPropertyVec, CssPropertyVecDestructor);
impl_vec_partialeq!(CssProperty, CssPropertyVec);
impl_vec_eq!(CssProperty, CssPropertyVec);
impl_vec_hash!(CssProperty, CssPropertyVec);

macro_rules! css_property_from_type {
    ($prop_type:expr, $content_type:ident) => {{
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(CssPropertyValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(CssPropertyValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(CssPropertyValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(CssPropertyValue::$content_type),
            CssPropertyType::LetterSpacing => {
                CssProperty::LetterSpacing(CssPropertyValue::$content_type)
            }
            CssPropertyType::LineHeight => CssProperty::LineHeight(CssPropertyValue::$content_type),
            CssPropertyType::WordSpacing => {
                CssProperty::WordSpacing(CssPropertyValue::$content_type)
            }
            CssPropertyType::TabWidth => CssProperty::TabWidth(CssPropertyValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(CssPropertyValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(CssPropertyValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(CssPropertyValue::$content_type),
            CssPropertyType::BoxSizing => CssProperty::BoxSizing(CssPropertyValue::$content_type),
            CssPropertyType::Width => CssProperty::Width(CssPropertyValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(CssPropertyValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(CssPropertyValue::$content_type),
            CssPropertyType::MinHeight => CssProperty::MinHeight(CssPropertyValue::$content_type),
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(CssPropertyValue::$content_type),
            CssPropertyType::MaxHeight => CssProperty::MaxHeight(CssPropertyValue::$content_type),
            CssPropertyType::Position => CssProperty::Position(CssPropertyValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(CssPropertyValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(CssPropertyValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(CssPropertyValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(CssPropertyValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(CssPropertyValue::$content_type),
            CssPropertyType::FlexDirection => {
                CssProperty::FlexDirection(CssPropertyValue::$content_type)
            }
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(CssPropertyValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(CssPropertyValue::$content_type),
            CssPropertyType::JustifyContent => {
                CssProperty::JustifyContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::AlignItems => CssProperty::AlignItems(CssPropertyValue::$content_type),
            CssPropertyType::AlignContent => {
                CssProperty::AlignContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::OverflowX => CssProperty::OverflowX(CssPropertyValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(CssPropertyValue::$content_type),
            CssPropertyType::PaddingTop => CssProperty::PaddingTop(CssPropertyValue::$content_type),
            CssPropertyType::PaddingLeft => {
                CssProperty::PaddingLeft(CssPropertyValue::$content_type)
            }
            CssPropertyType::PaddingRight => {
                CssProperty::PaddingRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::PaddingBottom => {
                CssProperty::PaddingBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::MarginTop => CssProperty::MarginTop(CssPropertyValue::$content_type),
            CssPropertyType::MarginLeft => CssProperty::MarginLeft(CssPropertyValue::$content_type),
            CssPropertyType::MarginRight => {
                CssProperty::MarginRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::MarginBottom => {
                CssProperty::MarginBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundContent => {
                CssProperty::BackgroundContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundPosition => {
                CssProperty::BackgroundPosition(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundSize => {
                CssProperty::BackgroundSize(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundRepeat => {
                CssProperty::BackgroundRepeat(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopLeftRadius => {
                CssProperty::BorderTopLeftRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopRightRadius => {
                CssProperty::BorderTopRightRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomLeftRadius => {
                CssProperty::BorderBottomLeftRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomRightRadius => {
                CssProperty::BorderBottomRightRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopColor => {
                CssProperty::BorderTopColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightColor => {
                CssProperty::BorderRightColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftColor => {
                CssProperty::BorderLeftColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomColor => {
                CssProperty::BorderBottomColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopStyle => {
                CssProperty::BorderTopStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightStyle => {
                CssProperty::BorderRightStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftStyle => {
                CssProperty::BorderLeftStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomStyle => {
                CssProperty::BorderBottomStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopWidth => {
                CssProperty::BorderTopWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightWidth => {
                CssProperty::BorderRightWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftWidth => {
                CssProperty::BorderLeftWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomWidth => {
                CssProperty::BorderBottomWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowLeft => {
                CssProperty::BoxShadowLeft(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowRight => {
                CssProperty::BoxShadowRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowTop => {
                CssProperty::BoxShadowTop(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowBottom => {
                CssProperty::BoxShadowBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::ScrollbarStyle => {
                CssProperty::ScrollbarStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::Opacity => CssProperty::Opacity(CssPropertyValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(CssPropertyValue::$content_type),
            CssPropertyType::PerspectiveOrigin => {
                CssProperty::PerspectiveOrigin(CssPropertyValue::$content_type)
            }
            CssPropertyType::TransformOrigin => {
                CssProperty::TransformOrigin(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackfaceVisibility => {
                CssProperty::BackfaceVisibility(CssPropertyValue::$content_type)
            }
            CssPropertyType::MixBlendMode => {
                CssProperty::MixBlendMode(CssPropertyValue::$content_type)
            }
            CssPropertyType::Filter => CssProperty::Filter(CssPropertyValue::$content_type),
            CssPropertyType::BackdropFilter => {
                CssProperty::BackdropFilter(CssPropertyValue::$content_type)
            }
            CssPropertyType::TextShadow => CssProperty::TextShadow(CssPropertyValue::$content_type),
            CssPropertyType::Direction => CssProperty::Direction(CssPropertyValue::$content_type),
            CssPropertyType::Hyphens => CssProperty::Hyphens(CssPropertyValue::$content_type),
            CssPropertyType::WhiteSpace => CssProperty::WhiteSpace(CssPropertyValue::$content_type),
        }
    }};
}

impl CssProperty {
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
}

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

/// Multiplier for floating point accuracy. Elements such as px or %
/// are only accurate until a certain number of decimal points, therefore
/// they have to be casted to isizes in order to make the f32 values
/// hash-able: Css has a relatively low precision here, roughly 5 digits, i.e
/// `1.00001 == 1.0`
const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

/// Same as PixelValue, but doesn't allow a "%" sign
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

/// FloatValue, but associated with a certain metric (i.e. px, em, etc.)
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

// Manual Debug implementation, because the auto-generated one is nearly unreadable
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
    #[inline]
    pub const fn zero() -> Self {
        const ZERO_DEG: AngleValue = AngleValue::const_deg(0);
        ZERO_DEG
    }

    /// Same as `PixelValue::px()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_deg(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Degree, value)
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_rad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Radians, value)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_grad(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Grad, value)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_turn(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Turn, value)
    }

    #[inline]
    pub fn const_percent(value: isize) -> Self {
        Self::const_from_metric(AngleMetric::Percent, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    #[inline]
    pub fn deg(value: f32) -> Self {
        Self::from_metric(AngleMetric::Degree, value)
    }

    #[inline]
    pub fn rad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Radians, value)
    }

    #[inline]
    pub fn grad(value: f32) -> Self {
        Self::from_metric(AngleMetric::Grad, value)
    }

    #[inline]
    pub fn turn(value: f32) -> Self {
        Self::from_metric(AngleMetric::Turn, value)
    }

    #[inline]
    pub fn percent(value: f32) -> Self {
        Self::from_metric(AngleMetric::Percent, value)
    }

    #[inline]
    pub fn from_metric(metric: AngleMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    /// Returns the value of the AngleMetric in degrees
    #[inline]
    pub fn to_degrees(&self) -> f32 {
        let val = match self.metric {
            AngleMetric::Degree => self.number.get(),
            AngleMetric::Radians => self.number.get() / 400.0 * 360.0,
            AngleMetric::Grad => self.number.get() / (2.0 * core::f32::consts::PI) * 360.0,
            AngleMetric::Turn => self.number.get() * 360.0,
            AngleMetric::Percent => self.number.get() / 100.0 * 360.0,
        };

        // clamp the degree to a positive value from 0 to 360 (so 410deg = 50deg)
        let mut val = val % 360.0;
        if val < 0.0 {
            val = 360.0 + val;
        }
        val
    }
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

impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

// Manual Debug implementation, because the auto-generated one is nearly unreadable
impl fmt::Display for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl fmt::Display for SizeMetric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SizeMetric::*;
        match self {
            Px => write!(f, "px"),
            Pt => write!(f, "pt"),
            Em => write!(f, "pt"),
            In => write!(f, "in"),
            Cm => write!(f, "cm"),
            Mm => write!(f, "mm"),
            Percent => write!(f, "%"),
        }
    }
}

impl PixelValue {
    #[inline]
    pub const fn zero() -> Self {
        const ZERO_PX: PixelValue = PixelValue::const_px(0);
        ZERO_PX
    }

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

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_percent(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Percent, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_in(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::In, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_cm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Cm, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_mm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric,
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
    pub fn inch(value: f32) -> Self {
        Self::from_metric(SizeMetric::In, value)
    }

    #[inline]
    pub fn cm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Cm, value)
    }

    #[inline]
    pub fn mm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub fn percent(value: f32) -> Self {
        Self::from_metric(SizeMetric::Percent, value)
    }

    #[inline]
    pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.metric == other.metric {
            Self {
                metric: self.metric,
                number: self.number.interpolate(&other.number, t),
            }
        } else {
            // TODO: how to interpolate between different metrics
            // (interpolate between % and em? - currently impossible)
            let self_px_interp = self.to_pixels(0.0);
            let other_px_interp = other.to_pixels(0.0);
            Self::from_metric(
                SizeMetric::Px,
                self_px_interp + (other_px_interp - self_px_interp) * t,
            )
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels_no_percent(&self) -> Option<f32> {
        // to_pixels always assumes 96 DPI
        match self.metric {
            SizeMetric::Px => Some(self.number.get()),
            SizeMetric::Pt => Some(self.number.get() * PT_TO_PX),
            SizeMetric::Em => Some(self.number.get() * EM_HEIGHT),
            SizeMetric::In => Some(self.number.get() * 96.0),
            SizeMetric::Cm => Some(self.number.get() * 96.0 / 2.54),
            SizeMetric::Mm => Some(self.number.get() * 96.0 / 25.4),
            SizeMetric::Percent => None,
        }
    }

    /// Returns the value of the SizeMetric in pixels
    #[inline]
    pub fn to_pixels(&self, percent_resolve: f32) -> f32 {
        // to_pixels always assumes 96 DPI
        match self.metric {
            SizeMetric::Percent => self.number.get() / 100.0 * percent_resolve,
            _ => self.to_pixels_no_percent().unwrap_or(0.0),
        }
    }
}

/// Wrapper around FloatValue, represents a percentage instead
/// of just being a regular floating-point value, i.e `5` = `5%`
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PercentageValue {
    number: FloatValue,
}

impl_option!(
    PercentageValue,
    OptionPercentageValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for PercentageValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}%", self.normalized() * 100.0)
    }
}

impl PercentageValue {
    /// Same as `PercentageValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_new(value: isize) -> Self {
        Self {
            number: FloatValue::const_new(value),
        }
    }

    #[inline]
    pub fn new(value: f32) -> Self {
        Self {
            number: value.into(),
        }
    }

    // NOTE: no get() function, to avoid confusion with "150%"

    #[inline]
    pub fn normalized(&self) -> f32 {
        self.number.get() / 100.0
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            number: self.number.interpolate(&other.number, t),
        }
    }
}

/// Wrapper around an f32 value that is internally casted to an isize,
/// in order to provide hash-ability (to avoid numerical instability).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FloatValue {
    pub number: isize,
}

impl fmt::Display for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl ::core::fmt::Debug for FloatValue {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Default for FloatValue {
    fn default() -> Self {
        const DEFAULT_FLV: FloatValue = FloatValue::const_new(0);
        DEFAULT_FLV
    }
}

impl FloatValue {
    /// Same as `FloatValue::new()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_new(value: isize) -> Self {
        Self {
            number: value * FP_PRECISION_MULTIPLIER_CONST,
        }
    }

    #[inline]
    pub fn new(value: f32) -> Self {
        Self {
            number: (value * FP_PRECISION_MULTIPLIER) as isize,
        }
    }

    #[inline]
    pub fn get(&self) -> f32 {
        self.number as f32 / FP_PRECISION_MULTIPLIER
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        let self_val_f32 = self.get();
        let other_val_f32 = other.get();
        let interpolated = self_val_f32 + ((other_val_f32 - self_val_f32) * t);
        Self::new(interpolated)
    }
}

impl From<f32> for FloatValue {
    #[inline]
    fn from(val: f32) -> Self {
        Self::new(val)
    }
}

/// Enum representing the metric associated with a number (px, pt, em, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SizeMetric {
    Px,
    Pt,
    Em,
    In,
    Cm,
    Mm,
    Percent,
}

impl Default for SizeMetric {
    fn default() -> Self {
        SizeMetric::Px
    }
}

/// Represents a `background-size` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundSize {
    ExactSize([PixelValue; 2]),
    Contain,
    Cover,
}

impl StyleBackgroundSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            StyleBackgroundSize::ExactSize(a) => {
                for q in a.iter_mut() {
                    q.scale_for_dpi(scale_factor);
                }
            }
            _ => {}
        }
    }
}

impl Default for StyleBackgroundSize {
    fn default() -> Self {
        StyleBackgroundSize::Contain
    }
}

impl_vec!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_debug!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_partialord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_ord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_clone!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_partialeq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_eq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_hash!(StyleBackgroundSize, StyleBackgroundSizeVec);

/// Represents a `background-position` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBackgroundPosition {
    pub horizontal: BackgroundPositionHorizontal,
    pub vertical: BackgroundPositionVertical,
}

impl StyleBackgroundPosition {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.horizontal.scale_for_dpi(scale_factor);
        self.vertical.scale_for_dpi(scale_factor);
    }
}

impl_vec!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_debug!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_partialord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_ord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_clone!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_partialeq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_eq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_hash!(StyleBackgroundPosition, StyleBackgroundPositionVec);

impl Default for StyleBackgroundPosition {
    fn default() -> Self {
        StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Left,
            vertical: BackgroundPositionVertical::Top,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionHorizontal {
    Left,
    Center,
    Right,
    Exact(PixelValue),
}

impl BackgroundPositionHorizontal {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            BackgroundPositionHorizontal::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionVertical {
    Top,
    Center,
    Bottom,
    Exact(PixelValue),
}

impl BackgroundPositionVertical {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            BackgroundPositionVertical::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

/// Represents a `background-repeat` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackgroundRepeat {
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY,
}

impl_vec!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
impl_vec_debug!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_partialord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_ord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_clone!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
impl_vec_partialeq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_eq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_hash!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);

impl Default for StyleBackgroundRepeat {
    fn default() -> Self {
        StyleBackgroundRepeat::Repeat
    }
}

/// Represents a `color` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTextColor {
    pub inner: ColorU,
}

derive_debug_zero!(StyleTextColor);
derive_display_zero!(StyleTextColor);

impl StyleTextColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

// -- TODO: Technically, border-radius can take two values for each corner!

/// Represents a `border-top-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopLeftRadius {
    pub inner: PixelValue,
}
/// Represents a `border-left-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomLeftRadius {
    pub inner: PixelValue,
}
/// Represents a `border-right-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopRightRadius {
    pub inner: PixelValue,
}
/// Represents a `border-bottom-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomRightRadius {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleBorderTopLeftRadius);
impl_pixel_value!(StyleBorderBottomLeftRadius);
impl_pixel_value!(StyleBorderTopRightRadius);
impl_pixel_value!(StyleBorderBottomRightRadius);

/// Represents a `border-top-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderTopWidth {
    pub inner: PixelValue,
}
/// Represents a `border-left-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderLeftWidth {
    pub inner: PixelValue,
}
/// Represents a `border-right-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderRightWidth {
    pub inner: PixelValue,
}
/// Represents a `border-bottom-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderBottomWidth {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutBorderTopWidth);
impl_pixel_value!(LayoutBorderLeftWidth);
impl_pixel_value!(LayoutBorderRightWidth);
impl_pixel_value!(LayoutBorderBottomWidth);

impl CssPropertyValue<StyleBorderTopLeftRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<StyleBorderTopRightRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<StyleBorderBottomLeftRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<StyleBorderBottomRightRadius> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<LayoutBorderTopWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<LayoutBorderRightWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<LayoutBorderBottomWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl CssPropertyValue<LayoutBorderLeftWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            CssPropertyValue::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

impl StyleBorderTopLeftRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl StyleBorderTopRightRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl StyleBorderBottomLeftRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl StyleBorderBottomRightRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl LayoutBorderTopWidth {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl LayoutBorderRightWidth {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl LayoutBorderBottomWidth {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl LayoutBorderLeftWidth {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

/// Represents a `border-top-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopStyle {
    pub inner: BorderStyle,
}
/// Represents a `border-left-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftStyle {
    pub inner: BorderStyle,
}
/// Represents a `border-right-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightStyle {
    pub inner: BorderStyle,
}
/// Represents a `border-bottom-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderTopStyle);
derive_debug_zero!(StyleBorderLeftStyle);
derive_debug_zero!(StyleBorderBottomStyle);
derive_debug_zero!(StyleBorderRightStyle);

derive_display_zero!(StyleBorderTopStyle);
derive_display_zero!(StyleBorderLeftStyle);
derive_display_zero!(StyleBorderBottomStyle);
derive_display_zero!(StyleBorderRightStyle);

/// Represents a `border-top-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopColor {
    pub inner: ColorU,
}
/// Represents a `border-left-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftColor {
    pub inner: ColorU,
}
/// Represents a `border-right-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightColor {
    pub inner: ColorU,
}
/// Represents a `border-bottom-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomColor {
    pub inner: ColorU,
}

impl StyleBorderTopColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}
impl StyleBorderLeftColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}
impl StyleBorderRightColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}
impl StyleBorderBottomColor {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}
derive_debug_zero!(StyleBorderTopColor);
derive_debug_zero!(StyleBorderLeftColor);
derive_debug_zero!(StyleBorderRightColor);
derive_debug_zero!(StyleBorderBottomColor);

derive_display_zero!(StyleBorderTopColor);
derive_display_zero!(StyleBorderLeftColor);
derive_display_zero!(StyleBorderRightColor);
derive_display_zero!(StyleBorderBottomColor);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

// missing StyleBorderRadius & LayoutRect
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBoxShadow {
    pub offset: [PixelValueNoPercent; 2],
    pub color: ColorU,
    pub blur_radius: PixelValueNoPercent,
    pub spread_radius: PixelValueNoPercent,
    pub clip_mode: BoxShadowClipMode,
}

impl StyleBoxShadow {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        for s in self.offset.iter_mut() {
            s.scale_for_dpi(scale_factor);
        }
        self.blur_radius.scale_for_dpi(scale_factor);
        self.spread_radius.scale_for_dpi(scale_factor);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundContent {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    Image(AzString),
    Color(ColorU),
}

impl_vec!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_debug!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_partialord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_ord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_clone!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_partialeq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_eq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_hash!(StyleBackgroundContent, StyleBackgroundContentVec);

impl Default for StyleBackgroundContent {
    fn default() -> StyleBackgroundContent {
        StyleBackgroundContent::Color(ColorU::TRANSPARENT)
    }
}

impl<'a> From<AzString> for StyleBackgroundContent {
    fn from(id: AzString) -> Self {
        StyleBackgroundContent::Image(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LinearGradient {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            direction: Direction::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ConicGradient {
    pub extend_mode: ExtendMode,             // default = clamp (no-repeat)
    pub center: StyleBackgroundPosition,     // default = center center
    pub angle: AngleValue,                   // default = 0deg
    pub stops: NormalizedRadialColorStopVec, // default = []
}

impl Default for ConicGradient {
    fn default() -> Self {
        Self {
            extend_mode: ExtendMode::default(),
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::default(),
            stops: Vec::new().into(),
        }
    }
}

// normalized linear color stop
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedLinearColorStop {
    pub offset: PercentageValue, // 0 to 100% // -- todo: theoretically this should be PixelValue
    pub color: ColorU,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub angle: AngleValue, // 0 to 360 degrees
    pub color: ColorU,
}

impl LinearColorStop {
    pub fn get_normalized_linear_stops(
        stops: &[LinearColorStop],
    ) -> Vec<NormalizedLinearColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 100.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedLinearColorStop {
                offset: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.normalized() * 100.0;
                if stops_to_distribute != 0 {
                    let last_stop_val =
                        stops[(stop_id - stops_to_distribute)].offset.normalized() * 100.0;
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.offset = PercentageValue::new(
                            last_stop_val + (s_id as f32 * value_to_add_per_stop),
                        );
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE))
                .normalized()
                * 100.0;
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.offset =
                    PercentageValue::new(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
    }
}

impl RadialColorStop {
    pub fn get_normalized_radial_stops(
        stops: &[RadialColorStop],
    ) -> Vec<NormalizedRadialColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 360.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedRadialColorStop {
                angle: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.to_degrees();
                if stops_to_distribute != 0 {
                    let last_stop_val = stops[(stop_id - stops_to_distribute)].angle.to_degrees();
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.angle =
                            AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE))
                .to_degrees();
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.angle = AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RadialGradient {
    pub shape: Shape,
    pub size: RadialGradientSize,
    pub position: StyleBackgroundPosition,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}

impl Default for RadialGradient {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            size: RadialGradientSize::default(),
            position: StyleBackgroundPosition::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum RadialGradientSize {
    // The gradient's ending shape meets the side of the box closest to its center
    // (for circles) or meets both the vertical and horizontal sides closest to the
    // center (for ellipses).
    ClosestSide,
    // The gradient's ending shape is sized so that it exactly meets the closest
    // corner of the box from its center
    ClosestCorner,
    // Similar to closest-side, except the ending shape is sized to meet the side
    // of the box farthest from its center (or vertical and horizontal sides)
    FarthestSide,
    // The default value, the gradient's ending shape is sized so that it exactly
    // meets the farthest corner of the box from its center
    FarthestCorner,
}

impl Default for RadialGradientSize {
    fn default() -> Self {
        RadialGradientSize::FarthestCorner
    }
}

impl RadialGradientSize {
    pub fn get_size(&self, parent_rect: LayoutRect, gradient_center: LayoutPosition) -> LayoutSize {
        // TODO!
        parent_rect.size
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DirectionCorners {
    pub from: DirectionCorner,
    pub to: DirectionCorner,
}

/// CSS direction (necessary for gradients). Can either be a fixed angle or
/// a direction ("to right" / "to left", etc.).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Direction {
    Angle(AngleValue),
    FromTo(DirectionCorners),
}

impl Default for Direction {
    fn default() -> Self {
        Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
        })
    }
}

impl Direction {
    /// Calculates the points of the gradient stops for angled linear gradients
    pub fn to_points(&self, rect: &LayoutRect) -> (LayoutPoint, LayoutPoint) {
        match self {
            Direction::Angle(angle_value) => {
                // note: assumes that the LayoutRect has positive sides

                // see: https://hugogiraudel.com/2013/02/04/css-gradients/

                let deg = angle_value.to_degrees(); // FloatValue -> f32

                let deg = -deg; // negate winding direction

                let width_half = rect.size.width as f32 / 2.0;
                let height_half = rect.size.height as f32 / 2.0;

                // hypotenuse_len is the length of the center of the rect to the corners
                let hypotenuse_len = libm::hypotf(width_half, height_half);

                // The corner also serves to determine what quadrant we're in
                // Get the quadrant (corner) the angle is in and get the degree associated
                // with that corner.

                let angle_to_top_left = libm::atanf(height_half / width_half).to_degrees();

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
                let degree_diff_to_corner = ending_point_degrees as f32 - deg;

                // Searched_len is the distance between the center of the rect and the
                // ending point of the gradient
                let searched_len = libm::fabsf(libm::cosf(
                    hypotenuse_len * degree_diff_to_corner.to_radians() as f32,
                ));

                // TODO: This searched_len is incorrect...

                // Once we have the length, we can simply rotate the length by the angle,
                // then translate it to the center of the rect
                let dx = libm::sinf(deg.to_radians() as f32) * searched_len;
                let dy = libm::cosf(deg.to_radians() as f32) * searched_len;

                let start_point_location = LayoutPoint {
                    x: libm::roundf(width_half + dx) as isize,
                    y: libm::roundf(height_half + dy) as isize,
                };
                let end_point_location = LayoutPoint {
                    x: libm::roundf(width_half - dx) as isize,
                    y: libm::roundf(height_half - dy) as isize,
                };

                (start_point_location, end_point_location)
            }
            Direction::FromTo(ft) => (ft.from.to_point(rect), ft.to.to_point(rect)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Shape {
    Ellipse,
    Circle,
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Ellipse
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
#[repr(C)]
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

impl fmt::Display for DirectionCorner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DirectionCorner::Right => "right",
                DirectionCorner::Left => "left",
                DirectionCorner::Top => "top",
                DirectionCorner::Bottom => "bottom",
                DirectionCorner::TopRight => "top right",
                DirectionCorner::TopLeft => "top left",
                DirectionCorner::BottomRight => "bottom right",
                DirectionCorner::BottomLeft => "bottom left",
            }
        )
    }
}

impl DirectionCorner {
    pub const fn opposite(&self) -> Self {
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

    pub const fn combine(&self, other: &Self) -> Option<Self> {
        use self::DirectionCorner::*;
        match (*self, *other) {
            (Right, Top) | (Top, Right) => Some(TopRight),
            (Left, Top) | (Top, Left) => Some(TopLeft),
            (Right, Bottom) | (Bottom, Right) => Some(BottomRight),
            (Left, Bottom) | (Bottom, Left) => Some(BottomLeft),
            _ => None,
        }
    }

    pub const fn to_point(&self, rect: &LayoutRect) -> LayoutPoint {
        use self::DirectionCorner::*;
        match *self {
            Right => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height / 2,
            },
            Left => LayoutPoint {
                x: 0,
                y: rect.size.height / 2,
            },
            Top => LayoutPoint {
                x: rect.size.width / 2,
                y: 0,
            },
            Bottom => LayoutPoint {
                x: rect.size.width / 2,
                y: rect.size.height,
            },
            TopRight => LayoutPoint {
                x: rect.size.width,
                y: 0,
            },
            TopLeft => LayoutPoint { x: 0, y: 0 },
            BottomRight => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height,
            },
            BottomLeft => LayoutPoint {
                x: 0,
                y: rect.size.height,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadialColorStop {
    // this is set to None if there was no offset that could be parsed
    pub offset: OptionAngleValue,
    pub color: ColorU,
}

impl_vec!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
impl_vec_debug!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_partialord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_ord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_clone!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_eq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_hash!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinearColorStop {
    // this is set to None if there was no offset that could be parsed
    pub offset: OptionPercentageValue,
    pub color: ColorU,
}

impl_vec!(
    NormalizedLinearColorStop,
    NormalizedLinearColorStopVec,
    NormalizedLinearColorStopVecDestructor
);
impl_vec_debug!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_partialord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_ord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_clone!(
    NormalizedLinearColorStop,
    NormalizedLinearColorStopVec,
    NormalizedLinearColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_eq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_hash!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);

/// Represents a `width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutWidth {
    pub inner: PixelValue,
}
/// Represents a `min-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMinWidth {
    pub inner: PixelValue,
}
/// Represents a `max-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMaxWidth {
    pub inner: PixelValue,
}
/// Represents a `height` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutHeight {
    pub inner: PixelValue,
}
/// Represents a `min-height` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMinHeight {
    pub inner: PixelValue,
}
/// Represents a `max-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMaxHeight {
    pub inner: PixelValue,
}

impl Default for LayoutMaxHeight {
    fn default() -> Self {
        Self {
            inner: PixelValue::px(core::f32::MAX),
        }
    }
}
impl Default for LayoutMaxWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::px(core::f32::MAX),
        }
    }
}

impl_pixel_value!(LayoutWidth);
impl_pixel_value!(LayoutHeight);
impl_pixel_value!(LayoutMinHeight);
impl_pixel_value!(LayoutMinWidth);
impl_pixel_value!(LayoutMaxWidth);
impl_pixel_value!(LayoutMaxHeight);

/// Represents a `top` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutTop {
    pub inner: PixelValue,
}
/// Represents a `left` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutLeft {
    pub inner: PixelValue,
}
/// Represents a `right` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutRight {
    pub inner: PixelValue,
}
/// Represents a `bottom` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBottom {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutTop);
impl_pixel_value!(LayoutBottom);
impl_pixel_value!(LayoutRight);
impl_pixel_value!(LayoutLeft);

/// Represents a `padding-top` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingTop {
    pub inner: PixelValue,
}
/// Represents a `padding-left` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingLeft {
    pub inner: PixelValue,
}
/// Represents a `padding-right` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingRight {
    pub inner: PixelValue,
}
/// Represents a `padding-bottom` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingBottom {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutPaddingTop);
impl_pixel_value!(LayoutPaddingBottom);
impl_pixel_value!(LayoutPaddingRight);
impl_pixel_value!(LayoutPaddingLeft);

/// Represents a `padding-top` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginTop {
    pub inner: PixelValue,
}
/// Represents a `padding-left` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginLeft {
    pub inner: PixelValue,
}
/// Represents a `padding-right` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginRight {
    pub inner: PixelValue,
}
/// Represents a `padding-bottom` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginBottom {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutMarginTop);
impl_pixel_value!(LayoutMarginBottom);
impl_pixel_value!(LayoutMarginRight);
impl_pixel_value!(LayoutMarginLeft);

/// Represents a `flex-grow` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexGrow {
    pub inner: FloatValue,
}

impl Default for LayoutFlexGrow {
    fn default() -> Self {
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        }
    }
}

/// Represents a `flex-shrink` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexShrink {
    pub inner: FloatValue,
}

impl Default for LayoutFlexShrink {
    fn default() -> Self {
        LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        }
    }
}

impl_float_value!(LayoutFlexGrow);
impl_float_value!(LayoutFlexShrink);

/// Represents a `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for LayoutFlexDirection {
    fn default() -> Self {
        LayoutFlexDirection::Column
    }
}

impl LayoutFlexDirection {
    pub fn get_axis(&self) -> LayoutAxis {
        use self::{LayoutAxis::*, LayoutFlexDirection::*};
        match self {
            Row | RowReverse => Horizontal,
            Column | ColumnReverse => Vertical,
        }
    }

    /// Returns true, if this direction is a `column-reverse` or `row-reverse` direction
    pub fn is_reverse(&self) -> bool {
        *self == LayoutFlexDirection::RowReverse || *self == LayoutFlexDirection::ColumnReverse
    }
}

/// Represents a `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutBoxSizing {
    ContentBox,
    BorderBox,
}

impl Default for LayoutBoxSizing {
    fn default() -> Self {
        LayoutBoxSizing::ContentBox
    }
}

/// Represents a `line-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLineHeight {
    pub inner: PercentageValue,
}

impl_percentage_value!(StyleLineHeight);

impl Default for StyleLineHeight {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(100),
        }
    }
}

/// Represents a `tab-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTabWidth {
    pub inner: PercentageValue,
}

impl_percentage_value!(StyleTabWidth);

impl Default for StyleTabWidth {
    fn default() -> Self {
        Self {
            inner: PercentageValue::const_new(100),
        }
    }
}

/// Represents a `letter-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleLetterSpacing {
    pub inner: PixelValue,
}

impl Default for StyleLetterSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

impl_pixel_value!(StyleLetterSpacing);

/// Represents a `word-spacing` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleWordSpacing {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleWordSpacing);

impl Default for StyleWordSpacing {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(0),
        }
    }
}

/// Same as the `LayoutFlexDirection`, but without the `-reverse` properties, used in the layout
/// solver, makes decisions based on horizontal / vertical direction easier to write.
/// Use `LayoutFlexDirection::get_axis()` to get the axis for a given `LayoutFlexDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

/// Represents a `display` CSS property value
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    // Basic display types
    None,
    #[default]
    Block,
    Inline,
    InlineBlock,

    // Flex layout
    Flex,
    InlineFlex,

    // Table layout
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableColumnGroup,
    TableColumn,
    TableCell,
    TableCaption,

    // List layout
    ListItem,

    // Special displays
    RunIn,
    Marker,

    // CSS3 additions
    Grid,
    InlineGrid,

    // Initial/Inherit values
    Initial,
    Inherit,
}

impl LayoutDisplay {
    /// Returns whether this display type creates a block formatting context
    pub fn creates_block_context(&self) -> bool {
        matches!(
            self,
            LayoutDisplay::Block
                | LayoutDisplay::Flex
                | LayoutDisplay::Grid
                | LayoutDisplay::Table
                | LayoutDisplay::ListItem
        )
    }

    /// Returns whether this display type creates a flex formatting context
    pub fn creates_flex_context(&self) -> bool {
        matches!(self, LayoutDisplay::Flex | LayoutDisplay::InlineFlex)
    }

    /// Returns whether this display type creates a table formatting context
    pub fn creates_table_context(&self) -> bool {
        matches!(self, LayoutDisplay::Table | LayoutDisplay::InlineTable)
    }

    /// Returns whether this is an inline-level display type
    pub fn is_inline_level(&self) -> bool {
        matches!(
            self,
            LayoutDisplay::Inline
                | LayoutDisplay::InlineBlock
                | LayoutDisplay::InlineFlex
                | LayoutDisplay::InlineTable
                | LayoutDisplay::InlineGrid
        )
    }
}

/// Represents a `float` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    Left,
    Right,
    None,
}

impl Default for LayoutFloat {
    fn default() -> Self {
        LayoutFloat::Left
    }
}

/// Represents a `position` attribute - default: `Static`
///
/// NOTE: No inline positioning is supported.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
    Fixed,
}

impl LayoutPosition {
    pub fn is_positioned(&self) -> bool {
        *self != LayoutPosition::Static
    }
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

/// Represents a `flex-wrap` attribute - default: `Wrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexWrap {
    Wrap,
    NoWrap,
}

impl Default for LayoutFlexWrap {
    fn default() -> Self {
        LayoutFlexWrap::Wrap
    }
}

/// Represents a `justify-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
    /// Items are distributed so that the spacing between any two adjacent alignment subjects,
    /// before the first alignment subject, and after the last alignment subject is the same
    SpaceEvenly,
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        LayoutJustifyContent::Start
    }
}

/// Represents a `align-items` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignItems {
    /// Items are stretched to fit the container
    Stretch,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned at the beginning of the container
    FlexStart,
    /// Items are positioned at the end of the container
    FlexEnd,
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        LayoutAlignItems::FlexStart
    }
}

/// Represents a `align-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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

impl Default for LayoutAlignContent {
    fn default() -> Self {
        LayoutAlignContent::Stretch
    }
}

/// Represents a `overflow-x` or `overflow-y` property, see
/// [`TextOverflowBehaviour`](./struct.TextOverflowBehaviour.html) - default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutOverflow {
    /// Always shows a scroll bar, overflows on scroll
    Scroll,
    /// Does not show a scroll bar by default, only when text is overflowing
    Auto,
    /// Never shows a scroll bar, simply clips text
    Hidden,
    /// Doesn't show a scroll bar, simply overflows the text
    Visible,
}

impl Default for LayoutOverflow {
    fn default() -> Self {
        LayoutOverflow::Auto
    }
}

impl LayoutOverflow {
    /// Returns whether this overflow value needs to display the scrollbars.
    ///
    /// - `overflow:scroll` always shows the scrollbar
    /// - `overflow:auto` only shows the scrollbar when the content is currently overflowing
    /// - `overflow:hidden` and `overflow:visible` do not show any scrollbars
    pub fn needs_scrollbar(&self, currently_overflowing: bool) -> bool {
        use self::LayoutOverflow::*;
        match self {
            Scroll => true,
            Auto => currently_overflowing,
            Hidden | Visible => false,
        }
    }

    /// Returns whether this is an `overflow:visible` node
    /// (the only overflow type that doesn't clip its children)
    pub fn is_overflow_visible(&self) -> bool {
        *self == LayoutOverflow::Visible
    }

    pub fn is_overflow_hidden(&self) -> bool {
        *self == LayoutOverflow::Hidden
    }
}

/// Horizontal text alignment enum (left, center, right) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextAlign {
    Left,
    Center,
    Right,
    Justify,
}

impl Default for StyleTextAlign {
    fn default() -> Self {
        StyleTextAlign::Left
    }
}

impl_option!(
    StyleTextAlign,
    OptionStyleTextAlign,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Force text direction: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleDirection {
    Ltr,
    Rtl,
}

impl Default for StyleDirection {
    fn default() -> Self {
        StyleDirection::Ltr
    }
}

impl_option!(
    StyleDirection,
    OptionStyleDirection,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Force text hyphens: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleHyphens {
    Auto,
    None,
}

impl Default for StyleHyphens {
    fn default() -> Self {
        StyleHyphens::Auto
    }
}

impl_option!(
    StyleHyphens,
    OptionStyleHyphens,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Force text hyphens: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleWhiteSpace {
    Normal,
    Pre,
    Nowrap,
}

impl Default for StyleWhiteSpace {
    fn default() -> Self {
        StyleWhiteSpace::Normal
    }
}

impl_option!(
    StyleWhiteSpace,
    OptionStyleWhiteSpace,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Vertical text alignment enum (top, center, bottom) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVerticalAlign {
    Top,
    Center,
    Bottom,
}

impl Default for StyleVerticalAlign {
    fn default() -> Self {
        StyleVerticalAlign::Top
    }
}

/// Represents an `opacity` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleOpacity {
    pub inner: PercentageValue,
}

impl Default for StyleOpacity {
    fn default() -> Self {
        StyleOpacity {
            inner: PercentageValue::const_new(0),
        }
    }
}

impl_percentage_value!(StyleOpacity);

/// Represents a `perspective-origin` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StylePerspectiveOrigin {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl StylePerspectiveOrigin {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl Default for StylePerspectiveOrigin {
    fn default() -> Self {
        StylePerspectiveOrigin {
            x: PixelValue::const_px(0),
            y: PixelValue::const_px(0),
        }
    }
}

/// Represents a `transform-origin` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformOrigin {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl StyleTransformOrigin {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl Default for StyleTransformOrigin {
    fn default() -> Self {
        StyleTransformOrigin {
            x: PixelValue::const_percent(50),
            y: PixelValue::const_percent(50),
        }
    }
}

/// Represents a `backface-visibility` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackfaceVisibility {
    Hidden,
    Visible,
}

impl Default for StyleBackfaceVisibility {
    fn default() -> Self {
        StyleBackfaceVisibility::Visible
    }
}

/// Represents an `opacity` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleTransform {
    Matrix(StyleTransformMatrix2D),
    Matrix3D(StyleTransformMatrix3D),
    Translate(StyleTransformTranslate2D),
    Translate3D(StyleTransformTranslate3D),
    TranslateX(PixelValue),
    TranslateY(PixelValue),
    TranslateZ(PixelValue),
    Rotate(AngleValue),
    Rotate3D(StyleTransformRotate3D),
    RotateX(AngleValue),
    RotateY(AngleValue),
    RotateZ(AngleValue),
    Scale(StyleTransformScale2D),
    Scale3D(StyleTransformScale3D),
    ScaleX(PercentageValue),
    ScaleY(PercentageValue),
    ScaleZ(PercentageValue),
    Skew(StyleTransformSkew2D),
    SkewX(PercentageValue),
    SkewY(PercentageValue),
    Perspective(PixelValue),
}

impl_vec!(
    StyleTransform,
    StyleTransformVec,
    StyleTransformVecDestructor
);
impl_vec_debug!(StyleTransform, StyleTransformVec);
impl_vec_partialord!(StyleTransform, StyleTransformVec);
impl_vec_ord!(StyleTransform, StyleTransformVec);
impl_vec_clone!(
    StyleTransform,
    StyleTransformVec,
    StyleTransformVecDestructor
);
impl_vec_partialeq!(StyleTransform, StyleTransformVec);
impl_vec_eq!(StyleTransform, StyleTransformVec);
impl_vec_hash!(StyleTransform, StyleTransformVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix2D {
    pub a: PixelValue,
    pub b: PixelValue,
    pub c: PixelValue,
    pub d: PixelValue,
    pub tx: PixelValue,
    pub ty: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformMatrix3D {
    pub m11: PixelValue,
    pub m12: PixelValue,
    pub m13: PixelValue,
    pub m14: PixelValue,
    pub m21: PixelValue,
    pub m22: PixelValue,
    pub m23: PixelValue,
    pub m24: PixelValue,
    pub m31: PixelValue,
    pub m32: PixelValue,
    pub m33: PixelValue,
    pub m34: PixelValue,
    pub m41: PixelValue,
    pub m42: PixelValue,
    pub m43: PixelValue,
    pub m44: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate2D {
    pub x: PixelValue,
    pub y: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformTranslate3D {
    pub x: PixelValue,
    pub y: PixelValue,
    pub z: PixelValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformRotate3D {
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
    pub angle: AngleValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformScale3D {
    pub x: PercentageValue,
    pub y: PercentageValue,
    pub z: PercentageValue,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleTransformSkew2D {
    pub x: PercentageValue,
    pub y: PercentageValue,
}

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

impl_option!(
    LayoutDisplayValue,
    OptionLayoutDisplayValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutFloatValue = CssPropertyValue<LayoutFloat>;
impl_option!(
    LayoutFloatValue,
    OptionLayoutFloatValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutBoxSizingValue = CssPropertyValue<LayoutBoxSizing>;
impl_option!(
    LayoutBoxSizingValue,
    OptionLayoutBoxSizingValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutWidthValue = CssPropertyValue<LayoutWidth>;
impl_option!(
    LayoutWidthValue,
    OptionLayoutWidthValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutHeightValue = CssPropertyValue<LayoutHeight>;
impl_option!(
    LayoutHeightValue,
    OptionLayoutHeightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMinWidthValue = CssPropertyValue<LayoutMinWidth>;
impl_option!(
    LayoutMinWidthValue,
    OptionLayoutMinWidthValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMinHeightValue = CssPropertyValue<LayoutMinHeight>;
impl_option!(
    LayoutMinHeightValue,
    OptionLayoutMinHeightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMaxWidthValue = CssPropertyValue<LayoutMaxWidth>;
impl_option!(
    LayoutMaxWidthValue,
    OptionLayoutMaxWidthValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMaxHeightValue = CssPropertyValue<LayoutMaxHeight>;
impl_option!(
    LayoutMaxHeightValue,
    OptionLayoutMaxHeightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutPositionValue = CssPropertyValue<LayoutPosition>;
impl_option!(
    LayoutPositionValue,
    OptionLayoutPositionValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutTopValue = CssPropertyValue<LayoutTop>;
impl_option!(
    LayoutTopValue,
    OptionLayoutTopValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutBottomValue = CssPropertyValue<LayoutBottom>;
impl_option!(
    LayoutBottomValue,
    OptionLayoutBottomValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutRightValue = CssPropertyValue<LayoutRight>;
impl_option!(
    LayoutRightValue,
    OptionLayoutRightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutLeftValue = CssPropertyValue<LayoutLeft>;
impl_option!(
    LayoutLeftValue,
    OptionLayoutLeftValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutPaddingTopValue = CssPropertyValue<LayoutPaddingTop>;
impl_option!(
    LayoutPaddingTopValue,
    OptionLayoutPaddingTopValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutPaddingBottomValue = CssPropertyValue<LayoutPaddingBottom>;
impl_option!(
    LayoutPaddingBottomValue,
    OptionLayoutPaddingBottomValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutPaddingLeftValue = CssPropertyValue<LayoutPaddingLeft>;
impl_option!(
    LayoutPaddingLeftValue,
    OptionLayoutPaddingLeftValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutPaddingRightValue = CssPropertyValue<LayoutPaddingRight>;
impl_option!(
    LayoutPaddingRightValue,
    OptionLayoutPaddingRightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMarginTopValue = CssPropertyValue<LayoutMarginTop>;
impl_option!(
    LayoutMarginTopValue,
    OptionLayoutMarginTopValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMarginBottomValue = CssPropertyValue<LayoutMarginBottom>;
impl_option!(
    LayoutMarginBottomValue,
    OptionLayoutMarginBottomValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMarginLeftValue = CssPropertyValue<LayoutMarginLeft>;
impl_option!(
    LayoutMarginLeftValue,
    OptionLayoutMarginLeftValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutMarginRightValue = CssPropertyValue<LayoutMarginRight>;
impl_option!(
    LayoutMarginRightValue,
    OptionLayoutMarginRightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
pub type LayoutBorderLeftWidthValue = CssPropertyValue<LayoutBorderLeftWidth>;
pub type LayoutBorderRightWidthValue = CssPropertyValue<LayoutBorderRightWidth>;
pub type LayoutBorderBottomWidthValue = CssPropertyValue<LayoutBorderBottomWidth>;
pub type LayoutOverflowValue = CssPropertyValue<LayoutOverflow>;
impl_option!(
    LayoutOverflowValue,
    OptionLayoutOverflowValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutFlexDirectionValue = CssPropertyValue<LayoutFlexDirection>;
impl_option!(
    LayoutFlexDirectionValue,
    OptionLayoutFlexDirectionValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutFlexWrapValue = CssPropertyValue<LayoutFlexWrap>;
impl_option!(
    LayoutFlexWrapValue,
    OptionLayoutFlexWrapValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutFlexGrowValue = CssPropertyValue<LayoutFlexGrow>;
impl_option!(
    LayoutFlexGrowValue,
    OptionLayoutFlexGrowValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutFlexShrinkValue = CssPropertyValue<LayoutFlexShrink>;
impl_option!(
    LayoutFlexShrinkValue,
    OptionLayoutFlexShrinkValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutJustifyContentValue = CssPropertyValue<LayoutJustifyContent>;
impl_option!(
    LayoutJustifyContentValue,
    OptionLayoutJustifyContentValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutAlignItemsValue = CssPropertyValue<LayoutAlignItems>;
impl_option!(
    LayoutAlignItemsValue,
    OptionLayoutAlignItemsValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
pub type LayoutAlignContentValue = CssPropertyValue<LayoutAlignContent>;
impl_option!(
    LayoutAlignContentValue,
    OptionLayoutAlignContentValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Holds info necessary for layouting / styling scrollbars (-webkit-scrollbar)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarInfo {
    /// Total width (or height for vertical scrollbars) of the scrollbar in pixels
    pub width: LayoutWidth,
    /// Padding of the scrollbar tracker, in pixels. The inner bar is `width - padding` pixels
    /// wide.
    pub padding_left: LayoutPaddingLeft,
    /// Padding of the scrollbar (right)
    pub padding_right: LayoutPaddingRight,
    /// Style of the scrollbar background
    /// (`-webkit-scrollbar` / `-webkit-scrollbar-track` / `-webkit-scrollbar-track-piece`
    /// combined)
    pub track: StyleBackgroundContent,
    /// Style of the scrollbar thumbs (the "up" / "down" arrows), (`-webkit-scrollbar-thumb`)
    pub thumb: StyleBackgroundContent,
    /// Styles the directional buttons on the scrollbar (`-webkit-scrollbar-button`)
    pub button: StyleBackgroundContent,
    /// If two scrollbars are present, addresses the (usually) bottom corner
    /// of the scrollable element, where two scrollbars might meet (`-webkit-scrollbar-corner`)
    pub corner: StyleBackgroundContent,
    /// Addresses the draggable resizing handle that appears above the
    /// `corner` at the bottom corner of some elements (`-webkit-resizer`)
    pub resizer: StyleBackgroundContent,
}

impl Default for ScrollbarInfo {
    fn default() -> Self {
        ScrollbarInfo {
            width: LayoutWidth::px(17.0),
            padding_left: LayoutPaddingLeft::px(2.0),
            padding_right: LayoutPaddingRight::px(2.0),
            track: StyleBackgroundContent::Color(ColorU {
                r: 241,
                g: 241,
                b: 241,
                a: 255,
            }),
            thumb: StyleBackgroundContent::Color(ColorU {
                r: 193,
                g: 193,
                b: 193,
                a: 255,
            }),
            button: StyleBackgroundContent::Color(ColorU {
                r: 163,
                g: 163,
                b: 163,
                a: 255,
            }),
            corner: StyleBackgroundContent::default(),
            resizer: StyleBackgroundContent::default(),
        }
    }
}

/// Scrollbar style
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarStyle {
    /// Vertical scrollbar style, if any
    pub horizontal: ScrollbarInfo,
    /// Horizontal scrollbar style, if any
    pub vertical: ScrollbarInfo,
}

/// Represents a `font-size` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFontSize {
    pub inner: PixelValue,
}

impl Default for StyleFontSize {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_em(1),
        }
    }
}

impl_pixel_value!(StyleFontSize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FontMetrics {
    // head table
    pub units_per_em: u16,
    pub font_flags: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,

    // hhea table
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub num_h_metrics: u16,

    // os/2 table
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: [u8; 10],
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32,
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,

    // os/2 version 0 table
    pub s_typo_ascender: OptionI16,
    pub s_typo_descender: OptionI16,
    pub s_typo_line_gap: OptionI16,
    pub us_win_ascent: OptionU16,
    pub us_win_descent: OptionU16,

    // os/2 version 1 table
    pub ul_code_page_range1: OptionU32,
    pub ul_code_page_range2: OptionU32,

    // os/2 version 2 table
    pub sx_height: OptionI16,
    pub s_cap_height: OptionI16,
    pub us_default_char: OptionU16,
    pub us_break_char: OptionU16,
    pub us_max_context: OptionU16,

    // os/2 version 3 table
    pub us_lower_optical_point_size: OptionU16,
    pub us_upper_optical_point_size: OptionU16,
}

impl Default for FontMetrics {
    fn default() -> Self {
        FontMetrics::zero()
    }
}

impl FontMetrics {
    /// Only for testing, zero-sized font, will always return 0 for every metric (`units_per_em =
    /// 1000`)
    pub const fn zero() -> Self {
        FontMetrics {
            units_per_em: 1000,
            font_flags: 0,
            x_min: 0,
            y_min: 0,
            x_max: 0,
            y_max: 0,
            ascender: 0,
            descender: 0,
            line_gap: 0,
            advance_width_max: 0,
            min_left_side_bearing: 0,
            min_right_side_bearing: 0,
            x_max_extent: 0,
            caret_slope_rise: 0,
            caret_slope_run: 0,
            caret_offset: 0,
            num_h_metrics: 0,
            x_avg_char_width: 0,
            us_weight_class: 0,
            us_width_class: 0,
            fs_type: 0,
            y_subscript_x_size: 0,
            y_subscript_y_size: 0,
            y_subscript_x_offset: 0,
            y_subscript_y_offset: 0,
            y_superscript_x_size: 0,
            y_superscript_y_size: 0,
            y_superscript_x_offset: 0,
            y_superscript_y_offset: 0,
            y_strikeout_size: 0,
            y_strikeout_position: 0,
            s_family_class: 0,
            panose: [0; 10],
            ul_unicode_range1: 0,
            ul_unicode_range2: 0,
            ul_unicode_range3: 0,
            ul_unicode_range4: 0,
            ach_vend_id: 0,
            fs_selection: 0,
            us_first_char_index: 0,
            us_last_char_index: 0,
            s_typo_ascender: OptionI16::None,
            s_typo_descender: OptionI16::None,
            s_typo_line_gap: OptionI16::None,
            us_win_ascent: OptionU16::None,
            us_win_descent: OptionU16::None,
            ul_code_page_range1: OptionU32::None,
            ul_code_page_range2: OptionU32::None,
            sx_height: OptionI16::None,
            s_cap_height: OptionI16::None,
            us_default_char: OptionU16::None,
            us_break_char: OptionU16::None,
            us_max_context: OptionU16::None,
            us_lower_optical_point_size: OptionU16::None,
            us_upper_optical_point_size: OptionU16::None,
        }
    }

    /// If set, use `OS/2.sTypoAscender - OS/2.sTypoDescender + OS/2.sTypoLineGap` to calculate the
    /// height
    ///
    /// See [`USE_TYPO_METRICS`](https://docs.microsoft.com/en-us/typography/opentype/spec/os2#fss)
    pub fn use_typo_metrics(&self) -> bool {
        self.fs_selection & (1 << 7) != 0
    }

    pub fn get_ascender_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_ascender.into()
        };
        match use_typo {
            Some(s) => s,
            None => self.ascender,
        }
    }

    /// NOTE: descender is NEGATIVE
    pub fn get_descender_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_descender.into()
        };
        match use_typo {
            Some(s) => s,
            None => self.descender,
        }
    }

    pub fn get_line_gap_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_line_gap.into()
        };
        match use_typo {
            Some(s) => s,
            None => self.line_gap,
        }
    }

    pub fn get_ascender(&self, target_font_size: f32) -> f32 {
        self.get_ascender_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_descender(&self, target_font_size: f32) -> f32 {
        self.get_descender_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_line_gap(&self, target_font_size: f32) -> f32 {
        self.get_line_gap_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }

    pub fn get_x_min(&self, target_font_size: f32) -> f32 {
        self.x_min as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_min(&self, target_font_size: f32) -> f32 {
        self.y_min as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_max(&self, target_font_size: f32) -> f32 {
        self.x_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_max(&self, target_font_size: f32) -> f32 {
        self.y_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_advance_width_max(&self, target_font_size: f32) -> f32 {
        self.advance_width_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_min_left_side_bearing(&self, target_font_size: f32) -> f32 {
        self.min_left_side_bearing as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_min_right_side_bearing(&self, target_font_size: f32) -> f32 {
        self.min_right_side_bearing as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_max_extent(&self, target_font_size: f32) -> f32 {
        self.x_max_extent as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_avg_char_width(&self, target_font_size: f32) -> f32 {
        self.x_avg_char_width as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_x_size(&self, target_font_size: f32) -> f32 {
        self.y_subscript_x_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_y_size(&self, target_font_size: f32) -> f32 {
        self.y_subscript_y_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_x_offset(&self, target_font_size: f32) -> f32 {
        self.y_subscript_x_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_y_offset(&self, target_font_size: f32) -> f32 {
        self.y_subscript_y_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_x_size(&self, target_font_size: f32) -> f32 {
        self.y_superscript_x_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_y_size(&self, target_font_size: f32) -> f32 {
        self.y_superscript_y_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_x_offset(&self, target_font_size: f32) -> f32 {
        self.y_superscript_x_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_y_offset(&self, target_font_size: f32) -> f32 {
        self.y_superscript_y_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_strikeout_size(&self, target_font_size: f32) -> f32 {
        self.y_strikeout_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_strikeout_position(&self, target_font_size: f32) -> f32 {
        self.y_strikeout_position as f32 / self.units_per_em as f32 * target_font_size
    }

    pub fn get_s_typo_ascender(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_ascender
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_typo_descender(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_descender
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_typo_line_gap(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_line_gap
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_us_win_ascent(&self, target_font_size: f32) -> Option<f32> {
        self.us_win_ascent
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_us_win_descent(&self, target_font_size: f32) -> Option<f32> {
        self.us_win_descent
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_sx_height(&self, target_font_size: f32) -> Option<f32> {
        self.sx_height
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_cap_height(&self, target_font_size: f32) -> Option<f32> {
        self.s_cap_height
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
}

#[repr(C)]
pub struct FontRef {
    /// shared pointer to an opaque implementation of the parsed font
    pub data: *const FontData,
    /// How many copies does this font have (if 0, the font data will be deleted on drop)
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}

impl fmt::Debug for FontRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "printing FontRef 0x{:0x}", self.data as usize)?;
        if let Some(d) = unsafe { self.data.as_ref() } {
            d.fmt(f)?;
        }
        if let Some(c) = unsafe { self.copies.as_ref() } {
            c.fmt(f)?;
        }
        Ok(())
    }
}

impl FontRef {
    #[inline]
    pub fn get_data<'a>(&'a self) -> &'a FontData {
        unsafe { &*self.data }
    }
}

impl_option!(
    FontRef,
    OptionFontRef,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);

unsafe impl Send for FontRef {}
unsafe impl Sync for FontRef {}

impl PartialEq for FontRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.data as usize == rhs.data as usize
    }
}

impl PartialOrd for FontRef {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.data as usize).cmp(&(other.data as usize)))
    }
}

impl Ord for FontRef {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_data = self.data as usize;
        let other_data = other.data as usize;
        self_data.cmp(&other_data)
    }
}

impl Eq for FontRef {}

impl Hash for FontRef {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_data = self.data as usize;
        self_data.hash(state)
    }
}

impl FontRef {
    pub fn new(data: FontData) -> Self {
        Self {
            data: Box::into_raw(Box::new(data)),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }
    pub fn get_bytes(&self) -> U8Vec {
        self.get_data().bytes.clone()
    }
}

impl Clone for FontRef {
    fn clone(&self) -> Self {
        unsafe {
            self.copies
                .as_ref()
                .map(|f| f.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            data: self.data,     // copy the pointer
            copies: self.copies, // copy the pointer
            run_destructor: true,
        }
    }
}

impl Drop for FontRef {
    fn drop(&mut self) {
        self.run_destructor = false;
        unsafe {
            let copies = unsafe { (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst) };
            if copies == 1 {
                let _ = Box::from_raw(self.data as *mut FontData);
                let _ = Box::from_raw(self.copies as *mut AtomicUsize);
            }
        }
    }
}

pub struct FontData {
    // T = ParsedFont
    /// Bytes of the font file, either &'static (never changing bytes) or a Vec<u8>.
    pub bytes: U8Vec,
    /// Index of the font in the file (if not known, set to 0) -
    /// only relevant if the file is a font collection
    pub font_index: u32,
    // Since this type has to be defined in the
    pub parsed: *const c_void, // *const ParsedFont
    // destructor of the ParsedFont
    pub parsed_destructor: fn(*mut c_void),
}

impl fmt::Debug for FontData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FontData: {{");
        "    bytes: ".fmt(f)?;
        self.bytes.len().fmt(f)?;
        "    font_index: ".fmt(f)?;
        write!(f, "}}")?;
        Ok(())
    }
}

unsafe impl Send for FontData {}
unsafe impl Sync for FontData {}

impl Drop for FontData {
    fn drop(&mut self) {
        // destroy the ParsedFont
        (self.parsed_destructor)(self.parsed as *mut c_void)
    }
}

/// Represents a `font-family` attribute
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFontFamily {
    /// Native font, such as "Webly Sleeky UI", "monospace", etc.
    System(AzString),
    /// Font loaded from a file
    File(AzString),
    /// Reference-counted, already-decoded font,
    /// so that specific DOM nodes are required to use this font
    Ref(FontRef),
}

impl StyleFontFamily {
    pub(crate) fn as_string(&self) -> String {
        match &self {
            StyleFontFamily::System(s) => s.clone().into_library_owned_string(),
            StyleFontFamily::File(s) => s.clone().into_library_owned_string(),
            StyleFontFamily::Ref(s) => format!("{:0x}", s.data as usize),
        }
    }
}

impl_vec!(
    StyleFontFamily,
    StyleFontFamilyVec,
    StyleFontFamilyVecDestructor
);
impl_vec_clone!(
    StyleFontFamily,
    StyleFontFamilyVec,
    StyleFontFamilyVecDestructor
);
impl_vec_debug!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_eq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_ord!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_hash!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_partialeq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_partialord!(StyleFontFamily, StyleFontFamilyVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleMixBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for StyleMixBlendMode {
    fn default() -> StyleMixBlendMode {
        StyleMixBlendMode::Normal
    }
}

impl fmt::Display for StyleMixBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::StyleMixBlendMode::*;
        write!(
            f,
            "{}",
            match self {
                Normal => "normal",
                Multiply => "multiply",
                Screen => "screen",
                Overlay => "overlay",
                Darken => "darken",
                Lighten => "lighten",
                ColorDodge => "color-dodge",
                ColorBurn => "color-burn",
                HardLight => "hard-light",
                SoftLight => "soft-light",
                Difference => "difference",
                Exclusion => "exclusion",
                Hue => "hue",
                Saturation => "saturation",
                Color => "color",
                Luminosity => "luminosity",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFilter {
    Blend(StyleMixBlendMode),
    Flood(ColorU),
    Blur(StyleBlur),
    Opacity(PercentageValue),
    ColorMatrix(StyleColorMatrix),
    DropShadow(StyleBoxShadow),
    ComponentTransfer,
    Offset(StyleFilterOffset),
    Composite(StyleCompositeFilter),
}

impl_vec!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_clone!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_debug!(StyleFilter, StyleFilterVec);
impl_vec_eq!(StyleFilter, StyleFilterVec);
impl_vec_ord!(StyleFilter, StyleFilterVec);
impl_vec_hash!(StyleFilter, StyleFilterVec);
impl_vec_partialeq!(StyleFilter, StyleFilterVec);
impl_vec_partialord!(StyleFilter, StyleFilterVec);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBlur {
    pub width: PixelValue,
    pub height: PixelValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleColorMatrix {
    pub matrix: [FloatValue; 20],
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFilterOffset {
    pub x: PixelValue,
    pub y: PixelValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleCompositeFilter {
    Over,
    In,
    Atop,
    Out,
    Xor,
    Lighter,
    Arithmetic([FloatValue; 4]),
}
