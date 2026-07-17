//! Popover widget — wraps an arbitrary anchor [`Dom`] and shows an
//! absolutely-positioned floating panel holding arbitrary `content: Dom` when
//! the anchor is **clicked** (toggling open/closed). A click-triggered sibling
//! of [`crate::widgets::tooltip::Tooltip`] (which is hover-triggered and
//! text-only): the CSS show/hide popup mechanism is identical, but the panel
//! holds a whole [`Dom`] and is toggled by an internal click handler that flips
//! a [`PopoverState`].
//!
//! Structure: a `position: relative` wrapper containing a clickable *trigger*
//! (which holds the anchor) followed by the absolutely-positioned *content*
//! panel, hidden by default (`display: none`). Clicking the trigger flips
//! `open`, invokes the optional user `on_toggle(state)`, and shows/hides the
//! panel via `set_css_property(display)` (mirroring the live-restyle pattern of
//! check_box / accordion).
//!
//! TODO2: like [`Tooltip`], this is a CSS simplification of a "real" floating
//! popover. The panel is placed at a fixed offset below the trigger (it does not
//! measure the trigger's height, flip when near a screen edge, escape an
//! `overflow: hidden` ancestor, or raise its z-order — it relies on being the
//! later sibling to paint on top). There is also no "click-outside to dismiss"
//! and no `Escape` handling — clicking the trigger again is the only way to
//! close it (clicking *inside* the panel does not close it, since the handler is
//! on the trigger, not the wrapper). A future revision could route through the
//! window-popup / menu popup path for true screen-anchored positioning and
//! outside-click dismissal once that is runtime-verifiable.
//!
//! Key types: [`Popover`], [`PopoverState`], [`PopoverOnToggle`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::{LayoutDisplay, LayoutPosition, LayoutFlexGrow, LayoutTop, LayoutLeft, LayoutMinWidth, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleCursor, StyleBackgroundContentVec, StyleBackgroundContent, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static POPOVER_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-popover"))];
static POPOVER_TRIGGER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-popover-trigger",
))];
static POPOVER_CONTENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-popover-content",
))];

// ---- layout (logical px) ----
/// Fixed vertical offset of the panel below the wrapper's top edge. A
/// simplification — see the module-level `TODO2`.
const CONTENT_OFFSET_Y: isize = 32;
/// Minimum width of the floating panel.
const CONTENT_MIN_WIDTH: isize = 160;
const CONTENT_RADIUS: isize = 6;

// ---- colours ----
/// Panel background (white).
const CONTENT_BG_COLOR: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
/// Panel border (#cccccc).
const CONTENT_BORDER_COLOR: ColorU = ColorU { r: 204, g: 204, b: 204, a: 255 };

/// Callback function type invoked when a popover is toggled. The [`PopoverState`]
/// carries the *new* open/closed value.
pub type PopoverOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, PopoverState) -> Update;
impl_widget_callback!(
    PopoverOnToggle,
    OptionPopoverOnToggle,
    PopoverOnToggleCallback,
    PopoverOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        PopoverOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: POPOVER_ON_TOGGLE_INVOKER,
    invoker_ty:     AzPopoverOnToggleCallbackInvoker,
    thunk_fn:       az_popover_on_toggle_callback_thunk,
    setter_fn:      AzApp_setPopoverOnToggleCallbackInvoker,
    from_handle_fn: AzPopoverOnToggleCallback_createFromHostHandle,
    extra_args:     [ state: PopoverState ],
}

/// A click-triggered floating panel anchored to an arbitrary [`Dom`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Popover {
    /// Runtime state (`open`) plus the optional toggle callback.
    pub popover_state: PopoverStateWrapper,
    /// The element that, when clicked, toggles the panel.
    pub anchor: Dom,
    /// The content shown inside the floating panel.
    pub content: Dom,
    /// Style of the positioning wrapper around the trigger + panel.
    pub wrapper_style: CssPropertyWithConditionsVec,
    /// Style of the floating content panel (includes its current `display`).
    pub content_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PopoverStateWrapper {
    /// Whether the panel is currently open.
    pub inner: PopoverState,
    /// Optional: function to call when the popover is toggled.
    pub on_toggle: OptionPopoverOnToggle,
}

/// The open/closed state of a [`Popover`].
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PopoverState {
    /// `true` = panel shown, `false` (default) = panel hidden.
    pub open: bool,
}

/// Wrapper around the trigger + panel: an inline-block positioning context so
/// the absolutely-positioned panel is placed relative to it.
static POPOVER_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The clickable trigger holding the anchor.
static POPOVER_TRIGGER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// Builds the floating-panel style. Only the `display` (open vs closed) differs
/// between states; all the positioning/visual props are present in both so the
/// runtime `set_css_property(display)` toggle has everything it needs (mirroring
/// the accordion body-style approach).
fn build_content_style(open: bool) -> CssPropertyWithConditionsVec {
    let display = if open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    let bg_vec = StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(
        CONTENT_BG_COLOR
    )]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(display)),
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(
            CONTENT_OFFSET_Y,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            CONTENT_MIN_WIDTH,
        ))),
        // padding: 8px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            8,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(8),
        )),
        // border: 1px solid #cccccc
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
            inner: CONTENT_BORDER_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: CONTENT_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: CONTENT_BORDER_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor {
                inner: CONTENT_BORDER_COLOR,
            },
        )),
        // border-radius: 6px
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

impl Popover {
    /// Creates a popover whose `anchor`, when clicked, toggles a panel holding
    /// `content`. The panel starts closed.
    #[must_use] pub fn new(anchor: Dom, content: Dom) -> Self {
        Self {
            popover_state: PopoverStateWrapper::default(),
            anchor,
            content,
            wrapper_style: CssPropertyWithConditionsVec::from_const_slice(POPOVER_WRAPPER_STYLE),
            content_style: build_content_style(false),
        }
    }

    /// Sets whether the panel starts open, recomputing the panel style.
    #[inline]
    pub fn set_open(&mut self, open: bool) {
        self.popover_state.inner.open = open;
        self.content_style = build_content_style(open);
    }

    /// Builder-style setter for the initial open state.
    #[inline]
    #[must_use] pub fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Sets the toggle callback (invoked with the new state on every toggle).
    #[inline]
    pub fn set_on_toggle<C: Into<PopoverOnToggleCallback>>(&mut self, data: RefAny, on_toggle: C) {
        self.popover_state.on_toggle = Some(PopoverOnToggle {
            callback: on_toggle.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the toggle callback.
    #[inline]
    #[must_use] pub fn with_on_toggle<C: Into<PopoverOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    /// Replaces `self` with a default (empty) popover and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(Dom::default(), Dom::default());
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the popover into a [`Dom`] subtree with the `__azul-native-popover`
    /// class.
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{callbacks::CoreCallback, dom::{EventFilter, HoverEventFilter}, refany::OptionRefAny};

        // The trigger carries the click handler + the shared state. Clicking the
        // anchor (a descendant of the trigger) bubbles up to it (currentTarget
        // semantics — see `radio_group`), so `get_hit_node()` resolves to the
        // trigger regardless of what inside the anchor was clicked. Clicking the
        // panel does NOT toggle, since the panel is a sibling, not a child.
        let trigger = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(POPOVER_TRIGGER_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(POPOVER_TRIGGER_STYLE))
            .with_tab_index(TabIndex::Auto)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: on_popover_toggle as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: RefAny::new(self.popover_state),
                }]
                .into(),
            )
            .with_children(vec![self.anchor].into());

        let content = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(POPOVER_CONTENT_CLASS))
            .with_css_props(self.content_style)
            .with_children(vec![self.content].into());

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(POPOVER_WRAPPER_CLASS))
            .with_css_props(self.wrapper_style)
            // children: [trigger, content] — the panel is the trigger's next sibling.
            .with_children(vec![trigger, content].into())
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new(Dom::default(), Dom::default())
    }
}

/// Trigger click handler. The hit node is the trigger (the callback-bearing
/// node, per `currentTarget` semantics — see `radio_group`); its next sibling is
/// the content panel. Flips `open`, invokes the optional user callback with the
/// new state, then shows/hides the panel via `display`.
extern "C" fn on_popover_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let trigger = info.get_hit_node();
    let Some(content) = info.get_next_sibling(trigger) else {
        return Update::DoNothing;
    };

    let (now_open, result) = {
        let Some(mut pop) = data.downcast_mut::<PopoverStateWrapper>() else {
            return Update::DoNothing;
        };
        pop.inner.open = !pop.inner.open;
        let now_open = pop.inner.open;
        let inner = pop.inner;
        let pop = &mut *pop;
        let result = match pop.on_toggle.as_mut() {
            Some(PopoverOnToggle { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        };
        (now_open, result)
    };

    // TODO2: shows/hides the panel by toggling `display` via set_css_property.
    // This follows the proven live-restyle pattern of accordion/check_box; the
    // display:none/block relayout itself is not GUI-verified in this build.
    let display = if now_open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(content, CssProperty::const_display(display));

    result
}

impl From<Popover> for Dom {
    fn from(p: Popover) -> Self {
        p.dom()
    }
}
