//! Card widget — an elevated, bordered content container with rounded corners,
//! a soft drop shadow and padding, holding arbitrary child content. A near-clone
//! of [`crate::widgets::frame::Frame`] (a container) but without the fieldset
//! title/header — just a single styled box wrapping the body content.
//!
//! Key types: [`Card`], [`CardOnClick`].

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::css::BoxOrStatic;
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{ColorU, PixelValueNoPercent, PixelValue, FloatValue},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutFlexGrow},
        property::{CssProperty, StyleBoxShadowValue, LayoutFlexGrowValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBoxShadow, BoxShadowClipMode, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::CallbackInfo;

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
    /// Optional: Function to call when the card is clicked
    pub on_click: OptionCardOnClick,
}

/// Callback function type invoked when the card container is clicked.
pub type CardOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    CardOnClick,
    OptionCardOnClick,
    CardOnClickCallback,
    CardOnClickCallbackType
);

// Host-invoker plumbing for managed-FFI bindings — see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        CardOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CARD_ON_CLICK_INVOKER,
    invoker_ty:     AzCardOnClickCallbackInvoker,
    thunk_fn:       az_card_on_click_callback_thunk,
    setter_fn:      AzApp_setCardOnClickCallbackInvoker,
    from_handle_fn: AzCardOnClickCallback_createFromHostHandle,
}

impl Card {
    /// Creates a new `Card` wrapping the given content DOM.
    #[must_use] pub const fn create(content: Dom) -> Self {
        Self {
            content,
            flex_grow: 0.0,
            on_click: OptionCardOnClick::None,
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

    /// Sets the click callback, invoked when the card container is clicked.
    pub fn set_on_click<C: Into<CardOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(CardOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// Builder-style setter for the click callback.
    #[must_use] pub fn with_on_click<C: Into<CardOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        static CARD_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-card"))];

        // Optional click callback on the card's root container (same wiring
        // as button's on_click).
        let callbacks = match self.on_click.into_option() {
            Some(CardOnClick {
                refany: data,
                callback,
            }) => vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callback.cb as *const () as usize,
                    ctx: callback.ctx,
                },
                refany: data,
            }],
            None => Vec::new(),
        };

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
            .with_callbacks(callbacks.into())
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
