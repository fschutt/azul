//! Toast / snackbar widget — a transient notification banner. A near-clone of
//! [`crate::widgets::alert::Alert`] (a coloured message box with a "×" dismiss
//! affordance and a `visible` state) that, instead of sitting inline, floats as
//! an overlay pinned to a corner of its positioned parent
//! (`position: absolute; bottom; right`).
//!
//! Like [`crate::widgets::alert::Alert`] / [`crate::widgets::check_box::CheckBox`]
//! it is stateful: it carries a [`ToastStateWrapper`] (`{ visible } + on_dismiss`)
//! in a [`RefAny`] attached to the "×" close button. Clicking "×" flips `visible`
//! to `false`, invokes the optional user `on_dismiss`, and hides the whole toast
//! by setting `display: none` on the container via `set_css_property` (mirroring
//! alert's / check_box's live restyle).
//!
//! TODO2 — **auto-dismiss is intentionally NOT implemented (be honest, don't fake
//! it).** A real toast disappears on its own after N seconds. That requires a
//! host-driven `Timer`/`Update` loop that re-enters the event loop on a clock
//! tick and flips `visible` to `false` — a widget handler cannot *start* such a
//! timer (it only runs in response to an input event, with no access to schedule
//! a future wakeup). This is the same limitation the spinner hit with CSS
//! animation: there is no widget-local timer. So this widget ships a **manually**
//! dismissable toast (the "×"); a host that wants auto-timeout must register a
//! `Timer` itself and call `set_css_property(display: none)` (or rebuild without
//! the toast) when it fires.
//!
//! TODO2 — covering sibling widgets relies on paint order (being a later sibling)
//! because there is no real stacking-context / z-index, and a drop `box-shadow`
//! elevation is omitted (it needs a runtime-heap shadow value — see
//! `progressbar.rs`); the border + radius over the page convey the floating card.
//! The `display:none` relayout itself is not GUI-verified in this build.
//!
//! Key types: [`Toast`], [`ToastKind`], [`ToastState`], [`ToastOnDismiss`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutFlexGrow, LayoutPosition, LayoutInsetBottom, LayoutRight, LayoutMaxWidth, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleTextAlign, StyleCursor, StyleUserSelect},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static TOAST_CONTAINER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-toast"))];
static TOAST_MESSAGE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-toast-message"))];
static TOAST_CLOSE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-toast-close"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Distance (logical px) of the toast from the bottom / right edges of its parent.
const TOAST_INSET: isize = 24;
/// Maximum width (logical px) of the toast card.
const TOAST_MAX_WIDTH: isize = 360;

/// Callback function type invoked when a toast's "×" close button is clicked.
pub type ToastOnDismissCallbackType = extern "C" fn(RefAny, CallbackInfo, ToastState) -> Update;
impl_widget_callback!(
    ToastOnDismiss,
    OptionToastOnDismiss,
    ToastOnDismissCallback,
    ToastOnDismissCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ToastOnDismissCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TOAST_ON_DISMISS_INVOKER,
    invoker_ty:     AzToastOnDismissCallbackInvoker,
    thunk_fn:       az_toast_on_dismiss_callback_thunk,
    setter_fn:      AzApp_setToastOnDismissCallbackInvoker,
    from_handle_fn: AzToastOnDismissCallback_createFromHostHandle,
    extra_args:     [ state: ToastState ],
}

/// The semantic colour variant of a [`Toast`] (Bootstrap alert palette, mirroring
/// [`crate::widgets::alert::AlertKind`]).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum ToastKind {
    /// Blue informational toast — the default.
    #[default]
    Info,
    /// Green success toast.
    Success,
    /// Yellow warning toast.
    Warning,
    /// Red danger/error toast.
    Danger,
}

impl ToastKind {
    /// Returns the `(background, border, text)` colours for this toast kind.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn colors(&self) -> (ColorU, ColorU, ColorU) {
        match self {
            Self::Info => (
                ColorU { r: 207, g: 244, b: 252, a: 255 }, // #cff4fc
                ColorU { r: 182, g: 239, b: 251, a: 255 }, // #b6effb
                ColorU { r: 5, g: 81, b: 96, a: 255 },     // #055160
            ),
            Self::Success => (
                ColorU { r: 209, g: 231, b: 221, a: 255 }, // #d1e7dd
                ColorU { r: 186, g: 219, b: 204, a: 255 }, // #badbcc
                ColorU { r: 15, g: 81, b: 50, a: 255 },    // #0f5132
            ),
            Self::Warning => (
                ColorU { r: 255, g: 243, b: 205, a: 255 }, // #fff3cd
                ColorU { r: 255, g: 236, b: 181, a: 255 }, // #ffecb5
                ColorU { r: 102, g: 77, b: 3, a: 255 },    // #664d03
            ),
            Self::Danger => (
                ColorU { r: 248, g: 215, b: 218, a: 255 }, // #f8d7da
                ColorU { r: 245, g: 194, b: 199, a: 255 }, // #f5c2c7
                ColorU { r: 132, g: 32, b: 41, a: 255 },   // #842029
            ),
        }
    }

    /// CSS class name for this toast kind (mirrors `AlertKind::class_name`).
    #[must_use] pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Info => "__azul-toast-info",
            Self::Success => "__azul-toast-success",
            Self::Warning => "__azul-toast-warning",
            Self::Danger => "__azul-toast-danger",
        }
    }
}

/// A transient, floating notification banner with a "×" dismiss button.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Toast {
    /// Runtime state (`visible`) plus the optional dismiss callback.
    pub toast_state: ToastStateWrapper,
    /// The message text shown inside the toast.
    pub message: AzString,
    /// The colour variant.
    pub kind: ToastKind,
    /// Whether to render the "×" close button (default `true` — the only way to
    /// dismiss; see the module-level auto-dismiss TODO2).
    pub dismissible: bool,
    /// The computed inline style for the (absolutely-positioned) container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ToastStateWrapper {
    /// Whether the toast is currently visible.
    pub inner: ToastState,
    /// Optional: function to call when the toast is dismissed.
    pub on_dismiss: OptionToastOnDismiss,
}

/// The visible/hidden state of a [`Toast`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ToastState {
    /// `true` (default) = shown, `false` = dismissed/hidden.
    pub visible: bool,
}

impl Default for ToastState {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Builds the container style for a given [`ToastKind`]. Mirrors
/// `alert::build_alert_style` but pins the box to the bottom-right corner of its
/// positioned parent (`position: absolute`) and caps its width instead of
/// stretching to fill a flex column.
fn build_toast_style(kind: ToastKind) -> CssPropertyWithConditionsVec {
    let (bg, border, text) = kind.colors();
    let bg_vec =
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(bg)]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        // Float pinned to the bottom-right corner of the positioned parent.
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(
            TOAST_INSET,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_right(LayoutRight::const_px(
            TOAST_INSET,
        ))),
        // Cap the width so the toast hugs its content rather than spanning the page.
        CssPropertyWithConditions::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(
            TOAST_MAX_WIDTH,
        ))),
        // padding: 12px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            12,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(12),
        )),
        // border: 1px solid <border>
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
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: border,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: border },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: border },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: border },
        )),
        // border-radius: 6px
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        // Text colour is inherited by the message + close children.
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

/// Message-text style: takes the remaining horizontal space, left-aligned.
static TOAST_MESSAGE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
];

/// Close-button ("×") style: a small pointer-cursor box on the right.
static TOAST_CLOSE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        12,
    ))),
];

impl Toast {
    /// Creates a new informational (blue) toast with the given message (visible,
    /// with a "×" close button).
    #[inline]
    #[must_use] pub fn create(message: AzString) -> Self {
        Self::with_kind(message, ToastKind::Info)
    }

    /// Creates a new toast with the given message and colour variant.
    #[inline]
    #[must_use] pub fn with_kind(message: AzString, kind: ToastKind) -> Self {
        Self {
            toast_state: ToastStateWrapper::default(),
            message,
            kind,
            dismissible: true,
            container_style: build_toast_style(kind),
        }
    }

    /// Sets the colour variant, recomputing the container style.
    #[inline]
    pub fn set_kind(&mut self, kind: ToastKind) {
        self.kind = kind;
        self.container_style = build_toast_style(kind);
    }

    /// Builder-style setter for the colour variant.
    #[inline]
    #[must_use] pub fn with_toast_kind(mut self, kind: ToastKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Sets whether the toast shows a "×" close button.
    #[inline]
    pub const fn set_dismissible(&mut self, dismissible: bool) {
        self.dismissible = dismissible;
    }

    /// Builder-style setter for the dismissible flag.
    #[inline]
    #[must_use] pub const fn with_dismissible(mut self, dismissible: bool) -> Self {
        self.set_dismissible(dismissible);
        self
    }

    /// Sets the dismiss callback. Implies `dismissible = true` so the close
    /// button is rendered.
    #[inline]
    pub fn set_on_dismiss<C: Into<ToastOnDismissCallback>>(&mut self, data: RefAny, on_dismiss: C) {
        self.dismissible = true;
        self.toast_state.on_dismiss = Some(ToastOnDismiss {
            callback: on_dismiss.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the dismiss callback (implies dismissible).
    #[inline]
    #[must_use] pub fn with_on_dismiss<C: Into<ToastOnDismissCallback>>(
        mut self,
        data: RefAny,
        on_dismiss: C,
    ) -> Self {
        self.set_on_dismiss(data, on_dismiss);
        self
    }

    /// Replaces `self` with a default (empty info) toast and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this toast into a DOM subtree with the `__azul-native-toast` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let message = Dom::create_text(self.message)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOAST_MESSAGE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TOAST_MESSAGE_STYLE));

        let mut children = alloc::vec![message];

        if self.dismissible {
            let close = Dom::create_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TOAST_CLOSE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TOAST_CLOSE_STYLE))
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: default_on_toast_dismiss as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(self.toast_state),
                    }]
                    .into(),
                );
            children.push(close);
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOAST_CONTAINER_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Toast {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

/// Close-button click handler. The hit node is the close button (the
/// callback-bearing node, per `currentTarget` semantics — see `alert`); its
/// parent is the toast container. Flips `visible` to `false`, invokes the
/// optional user callback, then hides the whole toast via `display: none`.
extern "C" fn default_on_toast_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let close_node = info.get_hit_node();
    let Some(container) = info.get_parent(close_node) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut toast) = data.downcast_mut::<ToastStateWrapper>() else {
            return Update::DoNothing;
        };
        toast.inner.visible = false;
        let inner = toast.inner;
        let toast = &mut *toast;
        match toast.on_dismiss.as_mut() {
            Some(ToastOnDismiss { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        }
    };

    // TODO2: hides the toast by toggling `display: none` via set_css_property.
    // This follows the proven live-restyle pattern of alert/check_box (which
    // toggle display/opacity/background); the display:none relayout itself is not
    // GUI-verified in this build. (Auto-timeout dismissal is a host-driven Timer —
    // see the module-level TODO2 — and is intentionally not attempted here.)
    info.set_css_property(container, CssProperty::const_display(LayoutDisplay::None));

    result
}

impl From<Toast> for Dom {
    fn from(t: Toast) -> Self {
        t.dom()
    }
}
