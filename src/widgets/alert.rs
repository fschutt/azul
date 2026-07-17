//! Alert / banner widget — a coloured inline message box conveying an
//! informational, success, warning or danger status. A container (a near-clone
//! of [`crate::widgets::card::Card`] / [`crate::widgets::frame::Frame`]) holding
//! a message string, with an optional dismissible "x" close affordance.
//!
//! When made dismissible (`with_dismissible(true)` or `set_on_dismiss`), the
//! alert mirrors the stateful pattern of [`crate::widgets::check_box::CheckBox`]:
//! it carries an [`AlertStateWrapper`] (`{ visible } + on_dismiss`) in a
//! [`RefAny`] attached to the close button. Clicking the close button flips
//! `visible` to `false`, invokes the optional user `on_dismiss`, and hides the
//! whole alert by setting `display: none` on the container via
//! `set_css_property` (mirroring check_box's live restyle). A non-dismissible
//! alert renders no close button and carries no live callback — it is then just
//! a stateless styled container.
//!
//! Key types: [`Alert`], [`AlertKind`], [`AlertState`], [`AlertOnDismiss`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleTextAlign, StyleCursor, StyleUserSelect},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static ALERT_CONTAINER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-alert"))];
static ALERT_MESSAGE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-alert-message"))];
static ALERT_CLOSE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-alert-close"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Callback function type invoked when a dismissible alert's close button is clicked.
pub type AlertOnDismissCallbackType = extern "C" fn(RefAny, CallbackInfo, AlertState) -> Update;
impl_widget_callback!(
    AlertOnDismiss,
    OptionAlertOnDismiss,
    AlertOnDismissCallback,
    AlertOnDismissCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        AlertOnDismissCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ALERT_ON_DISMISS_INVOKER,
    invoker_ty:     AzAlertOnDismissCallbackInvoker,
    thunk_fn:       az_alert_on_dismiss_callback_thunk,
    setter_fn:      AzApp_setAlertOnDismissCallbackInvoker,
    from_handle_fn: AzAlertOnDismissCallback_createFromHostHandle,
    extra_args:     [ state: AlertState ],
}

/// The semantic colour variant of an [`Alert`] (Bootstrap alert palette).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum AlertKind {
    /// Blue informational alert — the default.
    #[default]
    Info,
    /// Green success alert.
    Success,
    /// Yellow warning alert.
    Warning,
    /// Red danger/error alert.
    Danger,
}

impl AlertKind {
    /// Returns the `(background, border, text)` colours for this alert kind.
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

    /// CSS class name for this alert kind (mirrors `ButtonType::class_name`).
    #[must_use] pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Info => "__azul-alert-info",
            Self::Success => "__azul-alert-success",
            Self::Warning => "__azul-alert-warning",
            Self::Danger => "__azul-alert-danger",
        }
    }
}

/// A coloured inline message box with an optional dismissible close button.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Alert {
    /// Runtime state (`visible`) plus the optional dismiss callback.
    pub alert_state: AlertStateWrapper,
    /// The message text shown inside the alert.
    pub message: AzString,
    /// The colour variant.
    pub kind: AlertKind,
    /// Whether to render the "x" close button (hides the alert on click).
    pub dismissible: bool,
    /// The computed inline style for the container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct AlertStateWrapper {
    /// Whether the alert is currently visible.
    pub inner: AlertState,
    /// Optional: function to call when the alert is dismissed.
    pub on_dismiss: OptionAlertOnDismiss,
}

/// The visible/hidden state of an [`Alert`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct AlertState {
    /// `true` (default) = shown, `false` = dismissed/hidden.
    pub visible: bool,
}

impl Default for AlertState {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Builds the container style for a given [`AlertKind`]. The colours are the
/// only kind-dependent properties, so the style is built at runtime per the
/// recipe's "runtime vec when param-dependent" path (see `badge::build_badge_style`).
fn build_alert_style(kind: AlertKind) -> CssPropertyWithConditionsVec {
    let (bg, border, text) = kind.colors();
    let bg_vec =
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(bg)]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Start)),
        // Span the full width of a flex-column parent.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
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
static ALERT_MESSAGE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
];

/// Close-button ("x") style: a small pointer-cursor box on the right.
static ALERT_CLOSE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        12,
    ))),
];

impl Alert {
    /// Creates a new informational (blue) alert with the given message.
    #[inline]
    #[must_use] pub fn create(message: AzString) -> Self {
        Self::with_kind(message, AlertKind::Info)
    }

    /// Creates a new alert with the given message and colour variant.
    #[inline]
    #[must_use] pub fn with_kind(message: AzString, kind: AlertKind) -> Self {
        Self {
            alert_state: AlertStateWrapper::default(),
            message,
            kind,
            dismissible: false,
            container_style: build_alert_style(kind),
        }
    }

    /// Sets the colour variant, recomputing the container style.
    #[inline]
    pub fn set_kind(&mut self, kind: AlertKind) {
        self.kind = kind;
        self.container_style = build_alert_style(kind);
    }

    /// Builder-style setter for the colour variant.
    #[inline]
    #[must_use] pub fn with_alert_kind(mut self, kind: AlertKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Sets whether the alert shows a "x" close button.
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
    pub fn set_on_dismiss<C: Into<AlertOnDismissCallback>>(&mut self, data: RefAny, on_dismiss: C) {
        self.dismissible = true;
        self.alert_state.on_dismiss = Some(AlertOnDismiss {
            callback: on_dismiss.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the dismiss callback (implies dismissible).
    #[inline]
    #[must_use] pub fn with_on_dismiss<C: Into<AlertOnDismissCallback>>(
        mut self,
        data: RefAny,
        on_dismiss: C,
    ) -> Self {
        self.set_on_dismiss(data, on_dismiss);
        self
    }

    /// Replaces `self` with a default (empty info) alert and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this alert into a DOM subtree with the `__azul-native-alert` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let message = Dom::create_text(self.message)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ALERT_MESSAGE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ALERT_MESSAGE_STYLE));

        let mut children = alloc::vec![message];

        if self.dismissible {
            let close = Dom::create_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ALERT_CLOSE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ALERT_CLOSE_STYLE))
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: default_on_alert_dismiss as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(self.alert_state),
                    }]
                    .into(),
                );
            children.push(close);
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ALERT_CONTAINER_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Alert {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

/// Close-button click handler. The hit node is the close button (the
/// callback-bearing node, per `currentTarget` semantics — see `radio_group`);
/// its parent is the alert container. Flips `visible` to `false`, invokes the
/// optional user callback, then hides the whole alert via `display: none`.
extern "C" fn default_on_alert_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let close_node = info.get_hit_node();
    let Some(container) = info.get_parent(close_node) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut alert) = data.downcast_mut::<AlertStateWrapper>() else {
            return Update::DoNothing;
        };
        alert.inner.visible = false;
        let inner = alert.inner;
        let alert = &mut *alert;
        match alert.on_dismiss.as_mut() {
            Some(AlertOnDismiss { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        }
    };

    // TODO2: hides the alert by toggling `display: none` via set_css_property.
    // This follows the proven live-restyle pattern of switch/check_box/radio_group
    // (which toggle opacity/margin/background); the display:none relayout itself is
    // not GUI-verified in this build.
    info.set_css_property(container, CssProperty::const_display(LayoutDisplay::None));

    result
}

impl From<Alert> for Dom {
    fn from(a: Alert) -> Self {
        a.dom()
    }
}
