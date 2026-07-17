//! Tooltip widget — wraps an arbitrary anchor [`Dom`] and shows a small text
//! popup near it while the pointer hovers, hiding it again on leave.
//!
//! ## Implementation note (CSS-based, see `TODO2` below)
//!
//! The drop-down popup path (`open_menu_for_hit_node` / `MenuPopupPosition`) is
//! built for *menus* — a list of clickable `MenuItem`s — not arbitrary text
//! shown next to an anchor, and it would also require a live window/hit-test to
//! verify. This widget therefore takes the simpler, fully-compilable and
//! self-contained CSS route the recipe allows: the tip is an absolutely-
//! positioned child of a `position: relative` wrapper, hidden by default
//! (`opacity: 0`) and revealed on `MouseEnter` / hidden on `MouseLeave` via
//! `set_css_property`. No user callbacks are needed — the show/hide handlers are
//! internal.
//!
//! TODO2: this is a CSS simplification of a "real" floating popover. The tip is
//! placed at a fixed offset below the anchor (it does not measure the anchor's
//! height, flip when near a screen edge, or escape an `overflow: hidden`
//! ancestor). A future revision could route through the window-popup / menu
//! popup path for true screen-anchored positioning once that is runtime-
//! verifiable.
//!
//! Key types: [`Tooltip`].

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutPosition, LayoutFlexGrow, LayoutTop, LayoutLeft, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPaddingBottom},
        property::{CssProperty, StyleWhiteSpaceValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleWhiteSpace, StyleOpacity},
    },
    AzString,
};

use crate::callbacks::CallbackInfo;

static TOOLTIP_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip"))];
static TOOLTIP_TIP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip-tip"))];

// ---- layout (logical px) ----
/// Fixed vertical offset of the tip below the wrapper's top edge. A
/// simplification — see the module-level `TODO2`.
const TIP_OFFSET_Y: isize = 22;
const TIP_RADIUS: isize = 4;

// ---- colours ----
/// Tip background (#333333, dark).
const TIP_BG_COLOR: ColorU = ColorU {
    r: 51,
    g: 51,
    b: 51,
    a: 240,
};
/// Tip text colour (white).
const TIP_TEXT_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

const TIP_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(TIP_BG_COLOR)];
const TIP_BG: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(TIP_BG_ITEMS);

/// Wrapper around the anchor: an inline-block positioning context so the
/// absolutely-positioned tip is placed relative to it.
static TOOLTIP_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The tip itself: absolutely positioned, hidden by default (`opacity: 0`).
static TOOLTIP_TIP_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(TIP_OFFSET_Y))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(TIP_BG)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TIP_TEXT_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
    // Preserve the tip on one line so it does not wrap into the anchor's width.
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    ))),
    // Hidden until hovered.
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

/// A tooltip: an anchor [`Dom`] plus the text shown on hover.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Tooltip {
    /// The element the tooltip is attached to.
    pub anchor: Dom,
    /// The text shown in the tip popup.
    pub text: AzString,
    /// Style of the positioning wrapper around the anchor.
    pub wrapper_style: CssPropertyWithConditionsVec,
    /// Style of the tip popup.
    pub tip_style: CssPropertyWithConditionsVec,
}

impl Default for Tooltip {
    fn default() -> Self {
        Self::new(Dom::default(), AzString::from_const_str(""))
    }
}

impl Tooltip {
    /// Creates a tooltip wrapping `anchor` that shows `text` on hover.
    #[must_use] pub fn new(anchor: Dom, text: AzString) -> Self {
        Self {
            anchor,
            text,
            wrapper_style: CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE),
            tip_style: CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_TIP_STYLE),
        }
    }

    /// Sets the tip text.
    #[inline]
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// Builder-style setter for the tip text.
    #[inline]
    #[must_use] pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// Overrides the tip popup style.
    #[inline]
    pub fn set_tip_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.tip_style = style;
    }

    /// Builder-style setter for the tip popup style.
    #[inline]
    #[must_use] pub fn with_tip_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_tip_style(style);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    #[must_use] pub fn dom(self) -> Dom {
        // The hover handlers only navigate the DOM (the tip is found relative to
        // the hovered wrapper), so no per-tooltip state is needed.
        let marker = RefAny::new(());

        let tip = Dom::create_text(self.text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLTIP_TIP_CLASS))
            .with_css_props(self.tip_style);

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLTIP_WRAPPER_CLASS))
            .with_css_props(self.wrapper_style)
            .with_callbacks(
                vec![
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseEnter),
                        callback: CoreCallback {
                            cb: on_tooltip_enter as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: marker.clone(),
                    },
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseLeave),
                        callback: CoreCallback {
                            cb: on_tooltip_leave as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: marker,
                    },
                ]
                .into(),
            )
            // children: [anchor, tip] — the tip is the anchor's next sibling.
            .with_children(vec![self.anchor, tip].into())
    }
}

/// Returns the tip node (the second child) of the hovered wrapper.
fn tip_of_wrapper(info: &CallbackInfo) -> Option<azul_core::dom::DomNodeId> {
    let wrapper = info.get_hit_node();
    let anchor = info.get_first_child(wrapper)?;
    info.get_next_sibling(anchor)
}

/// Pointer entered the wrapper → reveal the tip.
extern "C" fn on_tooltip_enter(_data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = tip_of_wrapper(&info) {
        info.set_css_property(tip, CssProperty::const_opacity(StyleOpacity::const_new(100)));
    }
    Update::DoNothing
}

/// Pointer left the wrapper → hide the tip.
extern "C" fn on_tooltip_leave(_data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = tip_of_wrapper(&info) {
        info.set_css_property(tip, CssProperty::const_opacity(StyleOpacity::const_new(0)));
    }
    Update::DoNothing
}

impl From<Tooltip> for Dom {
    fn from(t: Tooltip) -> Self {
        t.dom()
    }
}
