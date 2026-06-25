//! Divider (separator) widget — a thin rule line. A stateless single styled
//! node with no callback, a near-clone of [`crate::widgets::label::Label`].
//! Supports a horizontal (default) or vertical orientation.
//!
//! Key types: [`Divider`], [`DividerOrientation`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::ColorU,
        layout::{LayoutDisplay, LayoutHeight, LayoutAlignSelf, LayoutFlexGrow, LayoutMarginTop, LayoutMarginBottom, LayoutWidth, LayoutMarginLeft, LayoutMarginRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
    AzString,
};

/// Orientation of a [`Divider`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DividerOrientation {
    /// A full-width horizontal rule (1px tall) — the default.
    #[default]
    Horizontal,
    /// A full-height vertical rule (1px wide).
    Vertical,
}

/// A thin separator rule. Stateless; renders a single styled `div`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Divider {
    pub orientation: DividerOrientation,
    pub divider_style: CssPropertyWithConditionsVec,
}

/// Default rule colour (#dddddd), matching the frame widget's border colour.
const DIVIDER_COLOR: ColorU = ColorU {
    r: 221,
    g: 221,
    b: 221,
    a: 255,
};
const DIVIDER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(DIVIDER_COLOR)];
const DIVIDER_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(DIVIDER_BG_ITEMS);

static DIVIDER_STYLE_HORIZONTAL: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(1))),
    // Stretch across the parent's cross axis so the rule spans the full width.
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_top(LayoutMarginTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(
        LayoutMarginBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DIVIDER_BG)),
];

static DIVIDER_STYLE_VERTICAL: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(1))),
    // Stretch across the parent's cross axis so the rule spans the full height.
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_right(
        LayoutMarginRight::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DIVIDER_BG)),
];

impl Divider {
    /// Creates a new horizontal divider with default styling.
    #[inline]
    #[must_use] pub fn create() -> Self {
        Self::create_with_orientation(DividerOrientation::Horizontal)
    }

    /// Creates a new divider with the given orientation and default styling.
    #[inline]
    #[must_use] pub fn create_with_orientation(orientation: DividerOrientation) -> Self {
        let divider_style = match orientation {
            DividerOrientation::Horizontal => {
                CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_HORIZONTAL)
            }
            DividerOrientation::Vertical => {
                CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_VERTICAL)
            }
        };
        Self {
            orientation,
            divider_style,
        }
    }

    /// Sets the orientation, resetting the style to the matching default.
    #[inline]
    pub fn set_orientation(&mut self, orientation: DividerOrientation) {
        *self = Self::create_with_orientation(orientation);
    }

    /// Builder-style setter for the orientation.
    #[inline]
    #[must_use] pub fn with_orientation(mut self, orientation: DividerOrientation) -> Self {
        self.set_orientation(orientation);
        self
    }

    /// Replaces `self` with a default horizontal divider and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this divider into a DOM node with the `__azul-native-divider` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        static DIVIDER_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-divider"))];

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DIVIDER_CLASS))
            .with_css_props(self.divider_style)
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::create()
    }
}

impl From<Divider> for Dom {
    fn from(d: Divider) -> Self {
        d.dom()
    }
}
