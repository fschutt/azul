//! Toast / snackbar widget — a transient notification banner. A near-clone of
//! [`crate::widgets::alert::Alert`] (a coloured message box with a "x" dismiss
//! affordance and a `visible` state) that, instead of sitting inline, floats as
//! an overlay pinned to a corner of its positioned parent
//! (`position: absolute; bottom; right`).
//!
//! Like [`crate::widgets::alert::Alert`] / [`crate::widgets::check_box::CheckBox`]
//! it is stateful: it carries a [`ToastStateWrapper`] (`{ visible } + on_dismiss`)
//! in a [`RefAny`] attached to the "x" close button. Clicking "x" flips `visible`
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
//! dismissable toast (the "x"); a host that wants auto-timeout must register a
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
    impl_option_inner,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutInsetBottom, LayoutMarginLeft, LayoutMaxWidth, LayoutPaddingBottom,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutRight,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
            StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static TOAST_CONTAINER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-toast"))];
static TOAST_MESSAGE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-toast-message",
))];
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

/// Callback function type invoked when a toast's "x" close button is clicked.
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
                ColorU {
                    r: 207,
                    g: 244,
                    b: 252,
                    a: 255,
                }, // #cff4fc
                ColorU {
                    r: 182,
                    g: 239,
                    b: 251,
                    a: 255,
                }, // #b6effb
                ColorU {
                    r: 5,
                    g: 81,
                    b: 96,
                    a: 255,
                }, // #055160
            ),
            Self::Success => (
                ColorU {
                    r: 209,
                    g: 231,
                    b: 221,
                    a: 255,
                }, // #d1e7dd
                ColorU {
                    r: 186,
                    g: 219,
                    b: 204,
                    a: 255,
                }, // #badbcc
                ColorU {
                    r: 15,
                    g: 81,
                    b: 50,
                    a: 255,
                }, // #0f5132
            ),
            Self::Warning => (
                ColorU {
                    r: 255,
                    g: 243,
                    b: 205,
                    a: 255,
                }, // #fff3cd
                ColorU {
                    r: 255,
                    g: 236,
                    b: 181,
                    a: 255,
                }, // #ffecb5
                ColorU {
                    r: 102,
                    g: 77,
                    b: 3,
                    a: 255,
                }, // #664d03
            ),
            Self::Danger => (
                ColorU {
                    r: 248,
                    g: 215,
                    b: 218,
                    a: 255,
                }, // #f8d7da
                ColorU {
                    r: 245,
                    g: 194,
                    b: 199,
                    a: 255,
                }, // #f5c2c7
                ColorU {
                    r: 132,
                    g: 32,
                    b: 41,
                    a: 255,
                }, // #842029
            ),
        }
    }

    /// CSS class name for this toast kind (mirrors `AlertKind::class_name`).
    #[must_use]
    pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Info => "__azul-toast-info",
            Self::Success => "__azul-toast-success",
            Self::Warning => "__azul-toast-warning",
            Self::Danger => "__azul-toast-danger",
        }
    }
}

/// A transient, floating notification banner with a "x" dismiss button.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Toast {
    /// Runtime state (`visible`) plus the optional dismiss callback.
    pub toast_state: ToastStateWrapper,
    /// The message text shown inside the toast.
    pub message: AzString,
    /// The colour variant.
    pub kind: ToastKind,
    /// Whether to render the "x" close button (default `true` — the only way to
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
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(12,)
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: border }
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14
        ))),
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

/// Close-button ("x") style: a small pointer-cursor box on the right.
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
    /// with a "x" close button).
    #[inline]
    #[must_use]
    pub fn create(message: AzString) -> Self {
        Self::with_kind(message, ToastKind::Info)
    }

    /// Creates a new toast with the given message and colour variant.
    #[inline]
    #[must_use]
    pub fn with_kind(message: AzString, kind: ToastKind) -> Self {
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
    #[must_use]
    pub fn with_toast_kind(mut self, kind: ToastKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Sets whether the toast shows a "x" close button.
    #[inline]
    pub const fn set_dismissible(&mut self, dismissible: bool) {
        self.dismissible = dismissible;
    }

    /// Builder-style setter for the dismissible flag.
    #[inline]
    #[must_use]
    pub const fn with_dismissible(mut self, dismissible: bool) -> Self {
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
    #[must_use]
    pub fn with_on_dismiss<C: Into<ToastOnDismissCallback>>(
        mut self,
        data: RefAny,
        on_dismiss: C,
    ) -> Self {
        self.set_on_dismiss(data, on_dismiss);
        self
    }

    /// Replaces `self` with a default (empty info) toast and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this toast into a DOM subtree with the `__azul-native-toast` class.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let message = crate::widgets::widget_p_with_text(self.message)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOAST_MESSAGE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                TOAST_MESSAGE_STYLE,
            ));

        let mut children = alloc::vec![message];

        if self.dismissible {
            let close = crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TOAST_CLOSE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TOAST_CLOSE_STYLE))
                .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // a transient announcement. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            // This is the CLOSE BUTTON, not the container — the tab stop is on
            // the dismiss affordance. Its label is a multiplication sign: a
            // picture of an X, not a name. Hence an explicit name, and
            // PushButton rather than the container's role.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::PushButton,
                accessibility_name: Some(AzString::from_const_str("Close")).into(),
                ..Default::default()
            })
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

#[cfg(test)]
// `assertions_on_constants`: these are deliberate invariant guards over sibling
// `const`s in this module. They are const-foldable *today*, which is exactly the
// point — they must go red the moment someone edits one of those constants into an
// inconsistent value. Deleting them (clippy's suggestion) would delete the check.
#[allow(clippy::assertions_on_constants)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{
        props::basic::{length::SizeMetric, pixel::PixelValue},
        system::SystemStyle,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    const ALL_KINDS: [ToastKind; 4] = [
        ToastKind::Info,
        ToastKind::Success,
        ToastKind::Warning,
        ToastKind::Danger,
    ];

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The inline (static) CSS properties actually attached to a DOM node.
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The `background-color` of a style vec (first background layer only).
    fn background_color(style: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(v) => match v.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// Every `border-*-color` in a style vec, in declaration order.
    fn border_colors(style: &CssPropertyWithConditionsVec) -> Vec<ColorU> {
        style
            .as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::BorderTopColor(v) => v.get_property().map(|c| c.inner),
                CssProperty::BorderBottomColor(v) => v.get_property().map(|c| c.inner),
                CssProperty::BorderLeftColor(v) => v.get_property().map(|c| c.inner),
                CssProperty::BorderRightColor(v) => v.get_property().map(|c| c.inner),
                _ => None,
            })
            .collect()
    }

    /// The `color` (text colour) of a style vec.
    fn text_color(style: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        })
    }

    /// The declared `position` of a style vec.
    fn position_of(style: &CssPropertyWithConditionsVec) -> Option<LayoutPosition> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Position(v) => v.get_property().copied(),
            _ => None,
        })
    }

    /// The `bottom` offset of a style vec, as a raw `PixelValue`.
    fn bottom_px(style: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Bottom(v) => v.get_property().map(|b| b.inner),
            _ => None,
        })
    }

    /// The `right` offset of a style vec, as a raw `PixelValue`.
    fn right_px(style: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Right(v) => v.get_property().map(|r| r.inner),
            _ => None,
        })
    }

    /// The `max-width` of a style vec, as a raw `PixelValue`.
    fn max_width_px(style: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::MaxWidth(v) => v.get_property().map(|w| w.inner),
            _ => None,
        })
    }

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_types(
        style: &CssPropertyWithConditionsVec,
    ) -> Vec<core::mem::Discriminant<CssProperty>> {
        style
            .as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    /// A `RefAny` payload recording every `ToastState` a user `on_dismiss` sees.
    struct DismissLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_dismiss(mut data: RefAny, _: CallbackInfo, state: ToastState) -> Update {
        if let Some(mut log) = data.downcast_mut::<DismissLog>() {
            log.calls.push(state.visible);
        }
        Update::RefreshDom
    }

    extern "C" fn dismiss_do_nothing(_: RefAny, _: CallbackInfo, _: ToastState) -> Update {
        Update::DoNothing
    }

    fn dismiss_cb(f: ToastOnDismissCallbackType) -> ToastOnDismissCallback {
        f.into()
    }

    /// `visible` of a `ToastStateWrapper` payload.
    fn wrapper_visible(data: &mut RefAny) -> bool {
        data.downcast_ref::<ToastStateWrapper>()
            .expect("payload must still be a ToastStateWrapper")
            .inner
            .visible
    }

    /// The `visible` flags recorded by a `DismissLog` payload.
    fn log_calls(data: &mut RefAny) -> Vec<bool> {
        data.downcast_ref::<DismissLog>()
            .expect("payload must still be a DismissLog")
            .calls
            .clone()
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the dismiss handler only
    /// walks `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// The flattened DOM of a default toast: `container(0)`, `message(1)`,
    /// `close(2)` — i.e. exactly the hierarchy `default_on_toast_dismiss` walks
    /// (hit node -> parent).
    fn dismissible_styled_dom() -> StyledDom {
        let toast = Toast::create(AzString::from("msg"));
        assert!(
            toast.dismissible,
            "a fresh toast must already carry a close button"
        );
        let styled = StyledDom::create_from_dom(toast.dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            5,
            "fixture must flatten to container / message <p> + text / close <p> + text"
        );
        styled
    }

    /// Invokes `default_on_toast_dismiss` against a `LayoutWindow` holding
    /// `styled` (or nothing at all, when `styled` is `None`), with `hit` as the
    /// hit node. Returns the `Update` plus every recorded `CallbackChange`.
    /// Flat indices in `dismissible_styled_dom`, depth-first pre-order:
    /// `0 container / 1 message <p> / 2 message text / 3 close <p> / 4 close text`.
    /// Both callbacks and styles sit on the `<p>`s, never on the text nodes.
    const MESSAGE_NODE: usize = 1;
    const CLOSE_NODE: usize = 3;

    fn run_dismiss(
        styled: Option<StyledDom>,
        hit: usize,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(hit))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = default_on_toast_dismiss(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every `display` write recorded in the change log, as `(node index, display)`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutDisplay)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id,
                properties,
                ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Display(v) = p {
                        if let Some(d) = v.get_property() {
                            out.push((node_id.index(), *d));
                        }
                    }
                }
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // ToastKind::colors  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn kind_colors_are_the_documented_bootstrap_palette() {
        let expect = |(r, g, b): (u8, u8, u8)| ColorU { r, g, b, a: 255 };

        assert_eq!(
            ToastKind::Info.colors(),
            (
                expect((207, 244, 252)), // #cff4fc
                expect((182, 239, 251)), // #b6effb
                expect((5, 81, 96)),     // #055160
            )
        );
        assert_eq!(
            ToastKind::Success.colors(),
            (
                expect((209, 231, 221)), // #d1e7dd
                expect((186, 219, 204)), // #badbcc
                expect((15, 81, 50)),    // #0f5132
            )
        );
        assert_eq!(
            ToastKind::Warning.colors(),
            (
                expect((255, 243, 205)), // #fff3cd
                expect((255, 236, 181)), // #ffecb5
                expect((102, 77, 3)),    // #664d03
            )
        );
        assert_eq!(
            ToastKind::Danger.colors(),
            (
                expect((248, 215, 218)), // #f8d7da
                expect((245, 194, 199)), // #f5c2c7
                expect((132, 32, 41)),   // #842029
            )
        );
    }

    #[test]
    fn kind_colors_are_fully_opaque_and_pairwise_distinct() {
        for kind in ALL_KINDS {
            let (bg, border, text) = kind.colors();
            for (name, c) in [("bg", bg), ("border", border), ("text", text)] {
                assert_eq!(c.a, 255, "{kind:?}.{name} must be fully opaque");
            }
            // a floating card is only legible if bg != text
            assert_ne!(bg, text, "{kind:?}: background must differ from text");
            // ... and only visible against the page if bg != border
            assert_ne!(
                bg, border,
                "{kind:?}: the border must be visible on the card"
            );
        }

        for (i, a) in ALL_KINDS.iter().enumerate() {
            for b in &ALL_KINDS[i + 1..] {
                assert_ne!(
                    a.colors(),
                    b.colors(),
                    "{a:?} and {b:?} must be visually distinguishable"
                );
            }
        }
    }

    #[test]
    fn kind_colors_default_is_info_and_the_call_is_pure() {
        assert_eq!(ToastKind::default(), ToastKind::Info);
        assert_eq!(ToastKind::default().colors(), ToastKind::Info.colors());

        // repeated calls on the same (Copy) receiver must be stable
        let k = ToastKind::Danger;
        assert_eq!(k.colors(), k.colors());
        assert_eq!(k.colors(), k.colors());
    }

    #[test]
    fn kind_colors_is_const_evaluable() {
        const INFO: (ColorU, ColorU, ColorU) = ToastKind::Info.colors();
        assert_eq!(
            INFO.0,
            ColorU {
                r: 207,
                g: 244,
                b: 252,
                a: 255
            }
        );
    }

    // ------------------------------------------------------------------
    // ToastKind::class_name  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn class_name_exact_values_and_shape() {
        assert_eq!(ToastKind::Info.class_name(), "__azul-toast-info");
        assert_eq!(ToastKind::Success.class_name(), "__azul-toast-success");
        assert_eq!(ToastKind::Warning.class_name(), "__azul-toast-warning");
        assert_eq!(ToastKind::Danger.class_name(), "__azul-toast-danger");

        for kind in ALL_KINDS {
            let name = kind.class_name();
            assert!(
                name.starts_with("__azul-toast-"),
                "{kind:?} -> {name:?} must keep the widget prefix"
            );
            assert!(
                !name.contains(char::is_whitespace),
                "{name:?} must be a single CSS class token"
            );
            assert!(name.is_ascii(), "{name:?} must stay ASCII");
            // stable across calls, and equal for equal kinds
            assert_eq!(name, kind.class_name());
        }
    }

    #[test]
    fn class_name_is_unique_per_kind() {
        let mut names: Vec<&str> = ALL_KINDS.iter().map(|k| k.class_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 4, "every kind needs its own class name");
    }

    #[test]
    fn class_name_never_collides_with_the_structural_classes() {
        // the kind classes live in a different namespace than the three
        // `__azul-native-toast*` structural classes emitted by `dom()`
        let structural = [
            "__azul-native-toast",
            "__azul-native-toast-message",
            "__azul-native-toast-close",
        ];
        for kind in ALL_KINDS {
            for s in structural {
                assert_ne!(kind.class_name(), s, "{kind:?} must not shadow {s:?}");
            }
        }
    }

    #[test]
    fn class_name_is_const_evaluable() {
        const DANGER: &str = ToastKind::Danger.class_name();
        assert_eq!(DANGER, "__azul-toast-danger");
    }

    // ------------------------------------------------------------------
    // build_toast_style
    // ------------------------------------------------------------------

    #[test]
    fn build_toast_style_declares_the_same_properties_for_every_kind() {
        let info = property_types(&build_toast_style(ToastKind::Info));
        assert_eq!(
            info.len(),
            32,
            "the container style declares 32 properties (pin: adding/removing one is a \
             deliberate change)"
        );

        for kind in ALL_KINDS {
            let style = build_toast_style(kind);
            assert_eq!(
                property_types(&style),
                info,
                "{kind:?} must declare the same properties, in the same order, as Info"
            );
            // the style is unconditional: nothing is gated behind :hover/@media/...
            for p in style.as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{kind:?}: {:?} must be unconditional",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_toast_style_declares_no_property_twice() {
        // a duplicated property would silently shadow the earlier declaration
        for kind in ALL_KINDS {
            let types = property_types(&build_toast_style(kind));
            for (i, a) in types.iter().enumerate() {
                for b in &types[i + 1..] {
                    assert_ne!(
                        a, b,
                        "{kind:?}: the container style declares the same property twice"
                    );
                }
            }
        }
    }

    #[test]
    fn build_toast_style_colors_track_the_kind_palette() {
        for kind in ALL_KINDS {
            let style = build_toast_style(kind);
            let (bg, border, text) = kind.colors();

            assert_eq!(background_color(&style), Some(bg), "{kind:?}: background");
            assert_eq!(text_color(&style), Some(text), "{kind:?}: text colour");

            let borders = border_colors(&style);
            assert_eq!(
                borders.len(),
                4,
                "{kind:?}: all four edges must be coloured"
            );
            assert!(
                borders.iter().all(|c| *c == border),
                "{kind:?}: every edge must use the kind's border colour, got {borders:?}"
            );
        }
    }

    #[test]
    fn build_toast_style_pins_the_card_to_the_bottom_right_corner() {
        // This is what makes a toast a toast (rather than an inline alert):
        // position:absolute + bottom/right insets + a width cap.
        for kind in ALL_KINDS {
            let style = build_toast_style(kind);

            assert_eq!(
                position_of(&style),
                Some(LayoutPosition::Absolute),
                "{kind:?}: a toast must float out of flow"
            );
            assert_eq!(
                bottom_px(&style),
                Some(LayoutInsetBottom::const_px(TOAST_INSET).inner),
                "{kind:?}: bottom inset"
            );
            assert_eq!(
                right_px(&style),
                Some(LayoutRight::const_px(TOAST_INSET).inner),
                "{kind:?}: right inset"
            );
            assert_eq!(
                max_width_px(&style),
                Some(LayoutMaxWidth::const_px(TOAST_MAX_WIDTH).inner),
                "{kind:?}: width cap"
            );
            // an absolutely-positioned card must not also try to grow in a flex row
            assert!(
                style.as_ref().contains(&CssPropertyWithConditions::simple(
                    CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))
                )),
                "{kind:?}: the container must not flex-grow"
            );
        }
    }

    #[test]
    fn toast_inset_and_max_width_survive_the_fixed_point_encoding_exactly() {
        // The `isize`-backed `FloatValue` encoding must reproduce the constants
        // bit-exactly — a drifting inset silently mis-places every toast.
        assert_eq!(TOAST_INSET, 24);
        assert_eq!(TOAST_MAX_WIDTH, 360);
        assert!(
            TOAST_INSET > 0 && TOAST_MAX_WIDTH > TOAST_INSET,
            "an inset must push the card inward, and the cap must exceed the inset"
        );

        let style = build_toast_style(ToastKind::Info);
        for (name, got, want) in [
            ("bottom", bottom_px(&style), TOAST_INSET),
            ("right", right_px(&style), TOAST_INSET),
            ("max-width", max_width_px(&style), TOAST_MAX_WIDTH),
        ] {
            let pv = got.unwrap_or_else(|| panic!("{name} must be declared"));
            assert_eq!(
                pv.metric,
                SizeMetric::Px,
                "{name} must be an absolute px length, not a %/em"
            );
            assert!(
                (pv.number.get() - want as f32).abs() < f32::EPSILON,
                "{name}: {} px decoded back as {}",
                want,
                pv.number.get()
            );
            assert!(
                pv.number.get().is_finite(),
                "{name} must never decode to NaN/inf"
            );
        }
    }

    #[test]
    fn build_toast_style_geometry_is_kind_independent() {
        // Everything that is *not* a colour must be identical for all kinds.
        let expected = [
            CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
            CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
                LayoutFlexDirection::Row,
            )),
            CssPropertyWithConditions::simple(CssProperty::const_align_items(
                LayoutAlignItems::Start,
            )),
            CssPropertyWithConditions::simple(CssProperty::const_position(
                LayoutPosition::Absolute,
            )),
            CssPropertyWithConditions::simple(CssProperty::const_padding_top(
                LayoutPaddingTop::const_px(12),
            )),
            CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
                LayoutBorderTopWidth::const_px(1),
            )),
            CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
                StyleBorderTopLeftRadius::const_px(6),
            )),
            CssPropertyWithConditions::simple(CssProperty::const_font_size(
                StyleFontSize::const_px(14),
            )),
        ];

        for kind in ALL_KINDS {
            let style = build_toast_style(kind);
            for want in &expected {
                assert!(
                    style.as_ref().contains(want),
                    "{kind:?}: missing {:?}",
                    want.property
                );
            }
        }
    }

    #[test]
    fn build_toast_style_differs_only_in_the_colours() {
        let info = build_toast_style(ToastKind::Info);
        for kind in [ToastKind::Success, ToastKind::Warning, ToastKind::Danger] {
            let other = build_toast_style(kind);
            let differing: Vec<_> = info
                .as_ref()
                .iter()
                .zip(other.as_ref().iter())
                .filter(|(a, b)| a != b)
                .map(|(a, _)| core::mem::discriminant(&a.property))
                .collect();

            // background + 4 border colours + text colour = 6 kind-dependent props
            assert_eq!(
                differing.len(),
                6,
                "{kind:?}: only bg + 4 border colours + text colour may depend on the kind"
            );
        }
    }

    #[test]
    fn build_toast_style_is_pure_and_repeatable() {
        for kind in ALL_KINDS {
            assert_eq!(
                build_toast_style(kind),
                build_toast_style(kind),
                "{kind:?}: the builder must be deterministic"
            );
        }
    }

    // ------------------------------------------------------------------
    // Toast::create / with_kind / Default
    // ------------------------------------------------------------------

    #[test]
    fn create_is_an_info_toast_that_is_dismissible_by_default() {
        let toast = Toast::create(AzString::from("hello"));

        assert_eq!(toast.message.as_str(), "hello");
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(
            toast.dismissible,
            "unlike Alert, a fresh Toast ships the close button (the only way to dismiss it)"
        );
        assert!(toast.toast_state.inner.visible, "a fresh toast is visible");
        assert!(toast.toast_state.on_dismiss.is_none());
        assert_eq!(toast.container_style, build_toast_style(ToastKind::Info));
    }

    #[test]
    fn toast_state_defaults_to_visible_not_to_the_bool_default() {
        // `bool::default()` is false — `ToastState` must *override* that, else
        // every default-constructed toast would start out already dismissed.
        assert!(ToastState::default().visible);
        assert!(ToastStateWrapper::default().inner.visible);
        assert!(ToastStateWrapper::default().on_dismiss.is_none());
        assert!(Toast::default().toast_state.inner.visible);
    }

    #[test]
    fn create_with_empty_message_equals_default_and_is_value_comparable() {
        assert_eq!(Toast::create(AzString::from("")), Toast::default());
        // equality is structural, not pointer-based
        assert_eq!(
            Toast::create(AzString::from("a")),
            Toast::create(AzString::from("a"))
        );
        assert_ne!(
            Toast::create(AzString::from("a")),
            Toast::create(AzString::from("b"))
        );
        assert_ne!(
            Toast::create(AzString::from("a")),
            Toast::with_kind(AzString::from("a"), ToastKind::Danger)
        );
    }

    #[test]
    fn create_survives_extreme_messages_and_round_trips_them_into_the_dom() {
        let long = "ab".repeat(50_000);
        let cases: Vec<AzString> = alloc::vec![
            AzString::from(""),
            AzString::from(" "),
            AzString::from("a\0b"),             // interior NUL
            AzString::from("line\nbreak\ttab"), // control chars
            AzString::from("👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪"), // ZWJ + combining + RTL
            AzString::from("\u{feff}\u{202e}rtl-override"), // BOM + bidi override
            AzString::from("×"),                // same glyph as the close button
            AzString::from("\u{00D7}\u{00D7}\u{00D7}"), // three close glyphs
            AzString::from(long.as_str()),      // 100k chars
        ];

        for message in cases {
            let toast = Toast::create(message.clone());
            assert_eq!(toast.message.as_str(), message.as_str());

            // the message must survive the trip through the DOM byte-for-byte
            let dom = toast.dom();
            let children = dom.children.as_ref();
            assert_eq!(
                children.len(),
                2,
                "message content must never change the child count"
            );
            assert_eq!(text_of(&children[0]), Some(message.as_str()));
            // a "×" in the *message* must not be mistaken for the close button
            assert!(
                children[0].root.has_class("__azul-native-toast-message"),
                "the first child is always the message"
            );
            assert!(
                children[1].root.has_class("__azul-native-toast-close"),
                "the close button is always last"
            );
        }
    }

    #[test]
    fn with_kind_stores_both_args_for_every_kind() {
        for kind in ALL_KINDS {
            let toast = Toast::with_kind(AzString::from("m"), kind);

            assert_eq!(toast.kind, kind);
            assert_eq!(toast.message.as_str(), "m");
            assert!(toast.dismissible);
            assert!(toast.toast_state.on_dismiss.is_none());
            assert!(toast.toast_state.inner.visible);
            assert_eq!(
                toast.container_style,
                build_toast_style(kind),
                "{kind:?}: the container style must match the kind it was built with"
            );
        }
    }

    #[test]
    fn create_is_with_kind_info() {
        assert_eq!(
            Toast::create(AzString::from("m")),
            Toast::with_kind(AzString::from("m"), ToastKind::Info)
        );
        assert_eq!(
            Toast::create(AzString::from("m")),
            Toast::with_kind(AzString::from("m"), ToastKind::default())
        );
    }

    // ------------------------------------------------------------------
    // set_kind / with_toast_kind
    // ------------------------------------------------------------------

    #[test]
    fn set_kind_recomputes_the_style_and_is_idempotent() {
        let mut toast = Toast::create(AzString::from("m"));

        for kind in ALL_KINDS {
            toast.set_kind(kind);
            assert_eq!(toast.kind, kind);
            assert_eq!(toast.container_style, build_toast_style(kind));

            // applying the same kind twice must not append/duplicate anything
            let before = toast.container_style.clone();
            toast.set_kind(kind);
            assert_eq!(
                toast.container_style, before,
                "{kind:?}: set_kind must be idempotent"
            );
            assert_eq!(
                toast.container_style.len(),
                32,
                "{kind:?}: restyling must not grow the property vec"
            );
        }

        // a full cycle back to the original kind restores the original toast
        let original = Toast::create(AzString::from("m"));
        let mut cycled = original.clone();
        for kind in ALL_KINDS {
            cycled.set_kind(kind);
        }
        cycled.set_kind(ToastKind::Info);
        assert_eq!(cycled, original, "kind cycling must not accumulate state");
    }

    #[test]
    fn set_kind_leaves_message_dismissible_and_callback_alone() {
        let log = RefAny::new(DismissLog { calls: Vec::new() });
        let mut toast = Toast::create(AzString::from("keep me"));
        toast.set_on_dismiss(log, dismiss_cb(dismiss_do_nothing));
        toast.toast_state.inner.visible = false;

        toast.set_kind(ToastKind::Warning);

        assert_eq!(toast.message.as_str(), "keep me");
        assert!(
            toast.dismissible,
            "set_kind must not clear the close button"
        );
        assert!(
            toast.toast_state.on_dismiss.is_some(),
            "set_kind must not drop the callback"
        );
        assert!(
            !toast.toast_state.inner.visible,
            "set_kind must not resurrect a dismissed toast"
        );
    }

    #[test]
    fn with_toast_kind_matches_set_kind_and_last_write_wins() {
        for kind in ALL_KINDS {
            let built = Toast::create(AzString::from("m")).with_toast_kind(kind);
            let mut mutated = Toast::create(AzString::from("m"));
            mutated.set_kind(kind);
            assert_eq!(built, mutated, "{kind:?}: builder and setter must agree");
        }

        let toast = Toast::create(AzString::from("m"))
            .with_toast_kind(ToastKind::Danger)
            .with_toast_kind(ToastKind::Success);
        assert_eq!(toast.kind, ToastKind::Success);
        assert_eq!(toast.container_style, build_toast_style(ToastKind::Success));
    }

    // ------------------------------------------------------------------
    // set_dismissible / with_dismissible
    // ------------------------------------------------------------------

    #[test]
    fn set_dismissible_last_write_wins_and_touches_nothing_else() {
        let mut toast = Toast::with_kind(AzString::from("m"), ToastKind::Warning);
        let style_before = toast.container_style.clone();

        for flag in [true, true, false, true, false, false] {
            toast.set_dismissible(flag);
            assert_eq!(toast.dismissible, flag);
        }

        assert_eq!(toast.kind, ToastKind::Warning);
        assert_eq!(toast.message.as_str(), "m");
        assert_eq!(
            toast.container_style, style_before,
            "toggling must not restyle"
        );
        assert!(
            toast.toast_state.on_dismiss.is_none(),
            "toggling must not invent a callback"
        );
        assert!(
            toast.toast_state.inner.visible,
            "toggling the close button must not hide the toast"
        );
    }

    #[test]
    fn with_dismissible_toggle_sequence_ends_on_the_last_value() {
        assert!(Toast::default().with_dismissible(true).dismissible);
        assert!(!Toast::default().with_dismissible(false).dismissible);
        assert!(
            !Toast::default()
                .with_dismissible(true)
                .with_dismissible(false)
                .dismissible
        );
        assert!(
            Toast::default()
                .with_dismissible(false)
                .with_dismissible(true)
                .dismissible
        );
        // builder == setter
        let mut mutated = Toast::default();
        mutated.set_dismissible(false);
        assert_eq!(Toast::default().with_dismissible(false), mutated);
        // and re-enabling restores the exact default value
        assert_eq!(
            Toast::default()
                .with_dismissible(false)
                .with_dismissible(true),
            Toast::default()
        );
    }

    // ------------------------------------------------------------------
    // set_on_dismiss / with_on_dismiss
    // ------------------------------------------------------------------

    #[test]
    fn set_on_dismiss_forces_dismissible_back_on() {
        let mut toast = Toast::create(AzString::from("m")).with_dismissible(false);
        assert!(!toast.dismissible);

        toast.set_on_dismiss(RefAny::new(1u8), dismiss_cb(dismiss_do_nothing));

        assert!(
            toast.dismissible,
            "a dismiss callback must re-render the close button"
        );
        assert!(toast.toast_state.on_dismiss.is_some());
        assert!(
            toast.toast_state.inner.visible,
            "wiring a callback must not hide the toast"
        );
    }

    #[test]
    fn set_on_dismiss_replaces_rather_than_appends() {
        let mut toast = Toast::create(AzString::from("m"));

        toast.set_on_dismiss(RefAny::new(1u8), dismiss_cb(dismiss_do_nothing));
        let first = toast
            .toast_state
            .on_dismiss
            .as_ref()
            .expect("first callback")
            .refany
            .get_type_id();
        assert_eq!(first, RefAny::new(1u8).get_type_id());

        // a second call must *replace* the payload + function, not stack another one
        toast.set_on_dismiss(RefAny::new(9i64), dismiss_cb(record_dismiss));
        let second = toast
            .toast_state
            .on_dismiss
            .as_ref()
            .expect("second callback");
        assert_eq!(second.refany.get_type_id(), RefAny::new(9i64).get_type_id());
        assert_eq!(second.callback, dismiss_cb(record_dismiss));
        assert_ne!(second.callback, dismiss_cb(dismiss_do_nothing));
    }

    #[test]
    fn with_on_dismiss_keeps_message_and_kind() {
        let toast = Toast::with_kind(AzString::from("boom"), ToastKind::Danger)
            .with_on_dismiss(RefAny::new(0u8), dismiss_cb(dismiss_do_nothing));

        assert_eq!(toast.message.as_str(), "boom");
        assert_eq!(toast.kind, ToastKind::Danger);
        assert_eq!(toast.container_style, build_toast_style(ToastKind::Danger));
        assert!(toast.dismissible);
        assert!(toast.toast_state.on_dismiss.is_some());
    }

    #[test]
    fn set_dismissible_false_after_set_on_dismiss_silently_drops_the_close_button() {
        // Footgun, pinned as the *current* behaviour: `set_on_dismiss` forces
        // `dismissible = true`, but a later `set_dismissible(false)` wins and the
        // wired-up callback becomes unreachable. For a toast this is worse than
        // for an alert: the "x" is the *only* dismissal path (no auto-timeout),
        // so the toast can never be dismissed at all.
        let mut toast = Toast::create(AzString::from("m"));
        toast.set_on_dismiss(RefAny::new(0u8), dismiss_cb(record_dismiss));
        toast.set_dismissible(false);

        assert!(
            toast.toast_state.on_dismiss.is_some(),
            "the callback is still stored"
        );
        let dom = toast.dom();
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "no close button is rendered, so the toast is undismissable"
        );
        assert!(
            dom.children.as_ref()[0]
                .root
                .get_callbacks()
                .as_ref()
                .is_empty(),
            "and no handler is attached anywhere else either"
        );
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_resets_self() {
        let mut toast =
            Toast::with_kind(AzString::from("payload"), ToastKind::Danger).with_dismissible(false);
        let snapshot = toast.clone();

        let returned = toast.swap_with_default();

        assert_eq!(returned, snapshot, "the original must come back untouched");
        assert_eq!(
            toast,
            Toast::default(),
            "self must be reset to a default toast"
        );
        assert_eq!(toast.message.as_str(), "");
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(
            toast.dismissible,
            "the reset toast is a *default* toast, so it is dismissible again"
        );
        assert!(toast.toast_state.on_dismiss.is_none());
        assert!(toast.toast_state.inner.visible);
    }

    #[test]
    fn swap_with_default_is_stable_when_repeated() {
        let mut toast = Toast::default();
        for _ in 0..3 {
            let returned = toast.swap_with_default();
            assert_eq!(returned, Toast::default());
            assert_eq!(toast, Toast::default());
        }
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_of_self() {
        let mut toast = Toast::create(AzString::from("m"))
            .with_on_dismiss(RefAny::new(7u32), dismiss_cb(record_dismiss));

        let returned = toast.swap_with_default();

        assert!(
            returned.toast_state.on_dismiss.is_some(),
            "the callback moves out"
        );
        assert!(
            toast.toast_state.on_dismiss.is_none(),
            "the reset toast must not keep a reference to the old callback"
        );
    }

    #[test]
    fn swap_with_default_round_trips_a_dismissed_toast() {
        // a toast that was already dismissed must hand its `visible == false`
        // state to the caller, not silently reset it in the returned value
        let mut toast = Toast::create(AzString::from("m"));
        toast.toast_state.inner.visible = false;

        let returned = toast.swap_with_default();

        assert!(!returned.toast_state.inner.visible);
        assert!(
            toast.toast_state.inner.visible,
            "the fresh toast is visible"
        );
    }

    // ------------------------------------------------------------------
    // Toast::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_a_default_toast_is_a_container_with_message_and_close() {
        let toast = Toast::create(AzString::from("hi"));
        let style = toast.container_style.clone();
        let dom = toast.dom();

        assert!(dom.root.has_class("__azul-native-toast"));
        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "the container itself must carry no live callback"
        );
        assert_eq!(
            dom.root.style.iter_inline_properties().count(),
            style.len(),
            "every container property must reach the node's inline style"
        );

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "[message, close]");
        assert!(children[0].root.has_class("__azul-native-toast-message"));
        assert_eq!(text_of(&children[0]), Some("hi"));
        assert!(children[0].root.get_callbacks().as_ref().is_empty());
        assert!(children[0].root.get_tab_index().is_none());
    }

    #[test]
    fn dom_children_carry_exactly_the_static_child_styles() {
        let dom = Toast::create(AzString::from("hi")).dom();
        let children = dom.children.as_ref();

        let want_message: Vec<CssProperty> = TOAST_MESSAGE_STYLE
            .iter()
            .map(|p| p.property.clone())
            .collect();
        let want_close: Vec<CssProperty> = TOAST_CLOSE_STYLE
            .iter()
            .map(|p| p.property.clone())
            .collect();

        assert_eq!(inline_props(&children[0]), want_message);
        assert_eq!(inline_props(&children[1]), want_close);

        // the message takes the free space, the close button never does
        assert!(want_message.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))));
        assert!(want_close.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
        // ... and the "x" must not be text-selectable / must show a pointer
        assert!(want_close.contains(&CssProperty::const_cursor(StyleCursor::Pointer)));
        assert!(want_close.contains(&CssProperty::user_select(StyleUserSelect::None)));
    }

    #[test]
    fn dom_close_button_is_focusable_and_wired_to_the_dismiss_handler() {
        let dom = Toast::create(AzString::from("hi")).dom();

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2);

        let close = &children[1];
        assert!(close.root.has_class("__azul-native-toast-close"));
        assert_eq!(
            text_of(close),
            Some("\u{00D7}"),
            "the close glyph is U+00D7 MULTIPLICATION SIGN"
        );
        assert!(
            matches!(close.root.get_tab_index(), Some(TabIndex::Auto)),
            "the close button must be keyboard-reachable"
        );

        let callbacks = close.root.get_callbacks();
        assert_eq!(callbacks.as_ref().len(), 1, "exactly one dismiss handler");
        let cb = &callbacks.as_ref()[0];
        assert!(matches!(
            &cb.event,
            EventFilter::Hover(HoverEventFilter::MouseUp)
        ));
        assert_eq!(cb.callback.cb, default_on_toast_dismiss as usize);
        assert!(matches!(&cb.callback.ctx, OptionRefAny::None));
    }

    #[test]
    fn dom_hands_the_toast_state_to_the_close_button() {
        let toast = Toast::create(AzString::from("hi"))
            .with_on_dismiss(RefAny::new(0u8), dismiss_cb(record_dismiss));
        let dom = toast.dom();

        let close = &dom.children.as_ref()[1];
        let mut payload = close.root.get_callbacks().as_ref()[0].refany.clone();

        assert!(
            wrapper_visible(&mut payload),
            "the close button must receive a live, visible ToastStateWrapper"
        );
        assert!(
            payload
                .downcast_ref::<ToastStateWrapper>()
                .expect("ToastStateWrapper")
                .on_dismiss
                .is_some(),
            "the user callback must travel with the state"
        );
    }

    #[test]
    fn dom_of_a_non_dismissible_toast_has_no_callbacks_at_all() {
        let dom = Toast::create(AzString::from("m"))
            .with_dismissible(false)
            .dom();

        assert!(dom.root.has_class("__azul-native-toast"));
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 1, "no close button without `dismissible`");
        assert!(children[0].root.has_class("__azul-native-toast-message"));
        assert!(children[0].root.get_callbacks().as_ref().is_empty());
        assert!(dom.root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_renders_even_when_the_state_says_hidden() {
        // Pinned current behaviour: `visible` is *only* consulted by the dismiss
        // handler (which restyles the live node); `dom()` ignores it, so a
        // pre-dismissed toast is still emitted at full size.  A host that
        // rebuilds its DOM must filter dismissed toasts out itself.
        let mut toast = Toast::create(AzString::from("gone"));
        toast.toast_state.inner.visible = false;

        let dom = toast.dom();
        assert_eq!(dom.children.as_ref().len(), 2);
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some("gone"));

        let close = &dom.children.as_ref()[1];
        let mut payload = close.root.get_callbacks().as_ref()[0].refany.clone();
        assert!(
            !wrapper_visible(&mut payload),
            "the hidden state travels into the DOM verbatim"
        );
    }

    #[test]
    fn dom_is_stable_across_kinds_and_the_kind_class_is_not_emitted() {
        for kind in ALL_KINDS {
            let dom = Toast::with_kind(AzString::from("m"), kind).dom();
            assert!(dom.root.has_class("__azul-native-toast"));
            assert_eq!(dom.children.as_ref().len(), 2);

            // NOTE: `ToastKind::class_name()` is *not* applied to the DOM - the
            // container only ever carries the generic container class.
            assert!(
                !dom.root.has_class(kind.class_name()),
                "current behaviour: the kind class is not emitted"
            );
        }
    }

    #[test]
    fn from_toast_for_dom_is_exactly_dom() {
        // non-dismissible, so no RefAny identity is involved in the comparison
        let toast =
            Toast::with_kind(AzString::from("m"), ToastKind::Success).with_dismissible(false);
        let via_from = Dom::from(toast.clone());
        let via_method = toast.dom();
        assert!(
            via_from == via_method,
            "`impl From<Toast> for Dom` must delegate to `Toast::dom`"
        );
    }

    // ------------------------------------------------------------------
    // default_on_toast_dismiss
    // ------------------------------------------------------------------

    #[test]
    fn dismiss_hides_the_container_and_flips_visible() {
        let mut data = RefAny::new(ToastStateWrapper::default());

        // node 2 == the close button, its parent (node 0) is the container
        let (update, changes) =
            run_dismiss(Some(dismissible_styled_dom()), CLOSE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the *container* (not the close button) must be hidden"
        );
        assert!(!wrapper_visible(&mut data), "state must flip to hidden");
    }

    #[test]
    fn dismiss_invokes_the_user_callback_with_the_already_flipped_state() {
        let mut log = RefAny::new(DismissLog { calls: Vec::new() });
        let mut data = RefAny::new(ToastStateWrapper {
            inner: ToastState { visible: true },
            on_dismiss: Some(ToastOnDismiss {
                callback: dismiss_cb(record_dismiss),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) =
            run_dismiss(Some(dismissible_styled_dom()), CLOSE_NODE, data.clone());

        assert_eq!(
            update,
            Update::RefreshDom,
            "the user callback's Update is returned"
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false],
            "the callback must see `visible == false` (already dismissed)"
        );
        assert!(!wrapper_visible(&mut data));
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the container is hidden even after a user callback ran"
        );
    }

    #[test]
    fn dismiss_twice_is_idempotent() {
        let mut log = RefAny::new(DismissLog { calls: Vec::new() });
        let mut data = RefAny::new(ToastStateWrapper {
            inner: ToastState { visible: true },
            on_dismiss: Some(ToastOnDismiss {
                callback: dismiss_cb(record_dismiss),
                refany: log.clone(),
            })
            .into(),
        });

        for _ in 0..2 {
            let (update, changes) =
                run_dismiss(Some(dismissible_styled_dom()), CLOSE_NODE, data.clone());
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(
                display_writes(&changes),
                alloc::vec![(0usize, LayoutDisplay::None)]
            );
        }

        assert!(
            !wrapper_visible(&mut data),
            "a second dismiss must not un-hide"
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false, false],
            "each click fires the callback exactly once, always with visible == false"
        );
    }

    #[test]
    fn dismiss_from_the_message_node_also_hides_the_container() {
        // Pinned: the handler hides `parent(hit)`, whatever the hit node is.
        // For the toast the message <p>'s parent is the container too, so a
        // mis-wired handler would still "work" - which is why the close button
        // must stay the only node carrying it (see the wiring test above).
        let mut data = RefAny::new(ToastStateWrapper::default());

        let (update, changes) =
            run_dismiss(Some(dismissible_styled_dom()), MESSAGE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert!(!wrapper_visible(&mut data));
    }

    #[test]
    fn dismiss_on_a_root_hit_node_is_a_noop() {
        // node 0 has no parent -> there is no container to hide
        let mut data = RefAny::new(ToastStateWrapper::default());

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "nothing may be restyled without a parent"
        );
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn dismiss_with_a_stale_hit_node_is_a_noop() {
        // node 999 does not exist in the 3-node fixture
        let mut data = RefAny::new(ToastStateWrapper::default());

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data));
    }

    #[test]
    fn dismiss_with_an_absurd_hit_node_index_does_not_panic() {
        // usize::MAX / 2 is far past any allocated NodeId
        let mut data = RefAny::new(ToastStateWrapper::default());

        let (update, changes) =
            run_dismiss(Some(dismissible_styled_dom()), usize::MAX / 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data));
    }

    #[test]
    fn dismiss_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(ToastStateWrapper::default());

        let (update, changes) = run_dismiss(None, CLOSE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn dismiss_with_a_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) =
            run_dismiss(Some(dismissible_styled_dom()), CLOSE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not hide the container"
        );
    }

    #[test]
    fn dismiss_end_to_end_through_the_real_dom_payload() {
        // Take the *actual* RefAny the widget wired into its close button and
        // drive the *actual* handler the widget registered against it.
        let toast = Toast::create(AzString::from("bye"));
        let dom = toast.dom();
        let close = &dom.children.as_ref()[1];
        let entry = &close.root.get_callbacks().as_ref()[0];
        assert_eq!(entry.callback.cb, default_on_toast_dismiss as usize);
        let mut payload = entry.refany.clone();

        let styled = StyledDom::create_from_dom(dom);
        let (update, changes) = run_dismiss(Some(styled), CLOSE_NODE, payload.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert!(
            !wrapper_visible(&mut payload),
            "the state living in the DOM must be flipped to hidden"
        );
    }

    #[test]
    fn dismiss_end_to_end_reaches_a_user_callback_wired_through_the_builder() {
        let mut log = RefAny::new(DismissLog { calls: Vec::new() });
        let dom = Toast::with_kind(AzString::from("bye"), ToastKind::Danger)
            .with_on_dismiss(log.clone(), dismiss_cb(record_dismiss))
            .dom();
        let payload = dom.children.as_ref()[1].root.get_callbacks().as_ref()[0]
            .refany
            .clone();

        let styled = StyledDom::create_from_dom(dom);
        let (update, changes) = run_dismiss(Some(styled), CLOSE_NODE, payload);

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false],
            "the builder-wired callback must fire exactly once, with visible == false"
        );
    }
}
