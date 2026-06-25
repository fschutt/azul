//! Badge widget — a small rounded "pill" showing a short count or status string
//! (e.g. a notification count or a status label). A stateless, single styled
//! text node with no callback — a near-clone of [`crate::widgets::label::Label`]
//! restyled as a coloured pill, with an optional [`BadgeKind`] colour variant
//! (mirroring `button::ButtonType`).
//!
//! Key types: [`Badge`], [`BadgeKind`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutJustifyContent, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleTextColor},
    },
    AzString,
};

/// The semantic colour variant of a [`Badge`] (mirrors `button::ButtonType`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum BadgeKind {
    /// Neutral grey badge — the default.
    #[default]
    Default,
    /// Blue "primary" badge.
    Primary,
    /// Green "success" badge.
    Success,
    /// Red "danger" badge.
    Danger,
    /// Yellow "warning" badge (uses dark text).
    Warning,
    /// Cyan "info" badge (uses dark text).
    Info,
}

impl BadgeKind {
    /// Returns the `(background, text)` colours for this badge kind.
    const fn colors(&self) -> (ColorU, ColorU) {
        const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
        const DARK: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
        match self {
            Self::Default => (ColorU { r: 108, g: 117, b: 125, a: 255 }, WHITE),
            Self::Primary => (ColorU { r: 13, g: 110, b: 253, a: 255 }, WHITE),
            Self::Success => (ColorU { r: 25, g: 135, b: 84, a: 255 }, WHITE),
            Self::Danger => (ColorU { r: 220, g: 53, b: 69, a: 255 }, WHITE),
            Self::Warning => (ColorU { r: 255, g: 193, b: 7, a: 255 }, DARK),
            Self::Info => (ColorU { r: 13, g: 202, b: 240, a: 255 }, DARK),
        }
    }

    /// CSS class name for this badge kind (mirrors `ButtonType::class_name`).
    #[must_use] pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Default => "__azul-badge-default",
            Self::Primary => "__azul-badge-primary",
            Self::Success => "__azul-badge-success",
            Self::Danger => "__azul-badge-danger",
            Self::Warning => "__azul-badge-warning",
            Self::Info => "__azul-badge-info",
        }
    }
}

/// A small rounded pill showing a short status/count string. Stateless;
/// renders a single styled text node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Badge {
    /// The text shown inside the pill.
    pub string: AzString,
    /// The colour variant.
    pub kind: BadgeKind,
    /// The computed inline style for the pill.
    pub badge_style: CssPropertyWithConditionsVec,
}

/// Builds the pill style for a given [`BadgeKind`]. The colours are the only
/// kind-dependent properties, so the style is built at runtime per the recipe's
/// "runtime vec when param-dependent" path (see `switch::build_track_style`).
fn build_badge_style(kind: BadgeKind) -> CssPropertyWithConditionsVec {
    let (bg, text) = kind.colors();
    let bg_vec =
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(bg)]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        // Hug the content rather than stretch across a flex parent's cross axis.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        // padding: 2px 8px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            2,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(2),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(8),
        )),
        // border-radius: 10px (pill)
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

impl Badge {
    /// Creates a new badge with the given text and the default (grey) kind.
    #[inline]
    #[must_use] pub fn create(string: AzString) -> Self {
        Self::with_kind(string, BadgeKind::Default)
    }

    /// Creates a new badge with the given text and colour variant.
    #[inline]
    #[must_use] pub fn with_kind(string: AzString, kind: BadgeKind) -> Self {
        Self {
            string,
            kind,
            badge_style: build_badge_style(kind),
        }
    }

    /// Sets the colour variant, recomputing the style.
    #[inline]
    pub fn set_kind(&mut self, kind: BadgeKind) {
        self.kind = kind;
        self.badge_style = build_badge_style(kind);
    }

    /// Builder-style setter for the colour variant.
    #[inline]
    #[must_use] pub fn with_badge_kind(mut self, kind: BadgeKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Replaces `self` with an empty default badge and returns the original.
    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this badge into a DOM text node with the `__azul-native-badge` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        static BADGE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-badge"))];

        Dom::create_text(self.string)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BADGE_CLASS))
            .with_css_props(self.badge_style)
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<Badge> for Dom {
    fn from(b: Badge) -> Self {
        b.dom()
    }
}
