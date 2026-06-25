//! Card widget — an elevated, bordered content container with rounded corners,
//! a soft drop shadow and padding, holding arbitrary child content. A near-clone
//! of [`crate::widgets::frame::Frame`] (a container) but without the fieldset
//! title/header — just a single styled box wrapping the body content.
//!
//! Key types: [`Card`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::css::BoxOrStatic;
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{ColorU, PixelValueNoPercent, PixelValue, FloatValue},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutFlexGrow},
        property::{CssProperty, StyleBoxShadowValue, LayoutFlexGrowValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBoxShadow, BoxShadowClipMode, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius},
    },
    AzString,
};

/// Card border colour (#dee2e6).
const CARD_BORDER_COLOR: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Card background colour (white).
const CARD_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Soft drop-shadow colour (black @ ~15% alpha).
const CARD_SHADOW_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 38,
};

const CARD_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(CARD_BG_COLOR)];
const CARD_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CARD_BG_ITEMS);

/// Shared drop-shadow descriptor referenced by all four edge box-shadows.
static CARD_SHADOW: StyleBoxShadow = StyleBoxShadow {
    offset_x: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    offset_y: PixelValueNoPercent {
        inner: PixelValue::const_px(2),
    },
    blur_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(6),
    },
    spread_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    clip_mode: BoxShadowClipMode::Outset,
    color: CARD_SHADOW_COLOR,
};

const CARD_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(CARD_BG)),
    // padding: 12px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
    // border: 1px solid #dee2e6
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: CARD_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: CARD_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: CARD_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: CARD_BORDER_COLOR,
        },
    )),
    // border-radius: 8px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(8),
    )),
    // soft drop shadow on all four edges
    CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
];

/// An elevated, bordered content container with rounded corners, a soft drop
/// shadow and padding. Holds arbitrary child content (`content`).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Card {
    /// The body content rendered inside the card.
    pub content: Dom,
    /// `flex-grow` factor applied to the card container.
    pub flex_grow: f32,
}

impl Card {
    /// Creates a new `Card` wrapping the given content DOM.
    #[must_use] pub const fn create(content: Dom) -> Self {
        Self {
            content,
            flex_grow: 0.0,
        }
    }

    /// Replaces `self` with an empty default card and returns the original.
    #[must_use] pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// Sets the body content.
    pub fn set_content(&mut self, content: Dom) {
        self.content = content;
    }

    /// Builder-style setter for the body content.
    #[must_use] pub fn with_content(mut self, content: Dom) -> Self {
        self.set_content(content);
        self
    }

    /// Sets the flex-grow factor for the card container.
    pub const fn set_flex_grow(&mut self, flex_grow: f32) {
        self.flex_grow = flex_grow;
    }

    /// Builder-style setter for the flex-grow factor.
    #[must_use] pub const fn with_flex_grow(mut self, flex_grow: f32) -> Self {
        self.set_flex_grow(flex_grow);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        static CARD_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-card"))];

        // Prepend the (param-dependent) flex-grow, then the static card style.
        let mut props = vec![CssPropertyWithConditions::simple(CssProperty::FlexGrow(
            LayoutFlexGrowValue::Exact(LayoutFlexGrow {
                inner: FloatValue::new(self.flex_grow),
            }),
        ))];
        props.extend_from_slice(CARD_STYLE);

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CARD_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(props))
            .with_children(vec![self.content].into())
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::create(Dom::create_div())
    }
}

impl From<Card> for Dom {
    fn from(c: Card) -> Self {
        c.dom()
    }
}
