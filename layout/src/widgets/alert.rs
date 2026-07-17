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

#[cfg(test)]
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
    use azul_css::system::SystemStyle;
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

    const ALL_KINDS: [AlertKind; 4] = [
        AlertKind::Info,
        AlertKind::Success,
        AlertKind::Warning,
        AlertKind::Danger,
    ];

    /// The text of a `NodeType::Text` node (`None` for any other node type).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
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

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_types(style: &CssPropertyWithConditionsVec) -> Vec<core::mem::Discriminant<CssProperty>> {
        style
            .as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    /// A `RefAny` payload recording every `AlertState` a user `on_dismiss` sees.
    struct DismissLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_dismiss(mut data: RefAny, _: CallbackInfo, state: AlertState) -> Update {
        if let Some(mut log) = data.downcast_mut::<DismissLog>() {
            log.calls.push(state.visible);
        }
        Update::RefreshDom
    }

    extern "C" fn dismiss_do_nothing(_: RefAny, _: CallbackInfo, _: AlertState) -> Update {
        Update::DoNothing
    }

    fn dismiss_cb(f: AlertOnDismissCallbackType) -> AlertOnDismissCallback {
        f.into()
    }

    /// `visible` of an `AlertStateWrapper` payload.
    fn wrapper_visible(data: &mut RefAny) -> bool {
        data.downcast_ref::<AlertStateWrapper>()
            .expect("payload must still be an AlertStateWrapper")
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
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// The flattened DOM of a dismissible alert: `container(0)`, `message(1)`,
    /// `close(2)` — i.e. exactly the hierarchy `default_on_alert_dismiss` walks
    /// (hit node -> parent).
    fn dismissible_styled_dom() -> StyledDom {
        let alert = Alert::create(AzString::from("msg")).with_dismissible(true);
        let styled = StyledDom::create_from_dom(alert.dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly container/message/close"
        );
        styled
    }

    /// Invokes `default_on_alert_dismiss` against a `LayoutWindow` holding
    /// `styled` (or nothing at all, when `styled` is `None`), with `hit` as the
    /// hit node. Returns the `Update` plus every recorded `CallbackChange`.
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

        let update = default_on_alert_dismiss(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every `display` write recorded in the change log, as `(node index, display)`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutDisplay)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id, properties, ..
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
    // AlertKind::colors  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn kind_colors_are_the_documented_bootstrap_palette() {
        let expect = |(r, g, b): (u8, u8, u8)| ColorU { r, g, b, a: 255 };

        assert_eq!(
            AlertKind::Info.colors(),
            (
                expect((207, 244, 252)), // #cff4fc
                expect((182, 239, 251)), // #b6effb
                expect((5, 81, 96)),     // #055160
            )
        );
        assert_eq!(
            AlertKind::Success.colors(),
            (
                expect((209, 231, 221)), // #d1e7dd
                expect((186, 219, 204)), // #badbcc
                expect((15, 81, 50)),    // #0f5132
            )
        );
        assert_eq!(
            AlertKind::Warning.colors(),
            (
                expect((255, 243, 205)), // #fff3cd
                expect((255, 236, 181)), // #ffecb5
                expect((102, 77, 3)),    // #664d03
            )
        );
        assert_eq!(
            AlertKind::Danger.colors(),
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
            // a coloured banner is only legible if bg != text
            assert_ne!(bg, text, "{kind:?}: background must differ from text");
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
    fn kind_colors_default_is_info_and_call_is_pure() {
        assert_eq!(AlertKind::default(), AlertKind::Info);
        assert_eq!(AlertKind::default().colors(), AlertKind::Info.colors());

        // repeated calls on the same (Copy) receiver must be stable
        let k = AlertKind::Danger;
        assert_eq!(k.colors(), k.colors());
        assert_eq!(k.colors(), k.colors());
    }

    #[test]
    fn kind_colors_is_const_evaluable() {
        const INFO: (ColorU, ColorU, ColorU) = AlertKind::Info.colors();
        assert_eq!(INFO.0, ColorU { r: 207, g: 244, b: 252, a: 255 });
    }

    // ------------------------------------------------------------------
    // AlertKind::class_name  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn class_name_exact_values_and_shape() {
        assert_eq!(AlertKind::Info.class_name(), "__azul-alert-info");
        assert_eq!(AlertKind::Success.class_name(), "__azul-alert-success");
        assert_eq!(AlertKind::Warning.class_name(), "__azul-alert-warning");
        assert_eq!(AlertKind::Danger.class_name(), "__azul-alert-danger");

        for kind in ALL_KINDS {
            let name = kind.class_name();
            assert!(
                name.starts_with("__azul-alert-"),
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
    fn class_name_is_const_evaluable() {
        const DANGER: &str = AlertKind::Danger.class_name();
        assert_eq!(DANGER, "__azul-alert-danger");
    }

    // ------------------------------------------------------------------
    // build_alert_style
    // ------------------------------------------------------------------

    #[test]
    fn build_alert_style_declares_the_same_properties_for_every_kind() {
        let info = property_types(&build_alert_style(AlertKind::Info));
        assert!(!info.is_empty(), "the container style must not be empty");

        for kind in ALL_KINDS {
            let style = build_alert_style(kind);
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
    fn build_alert_style_declares_no_property_twice() {
        // a duplicated property would silently shadow the earlier declaration
        for kind in ALL_KINDS {
            let types = property_types(&build_alert_style(kind));
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
    fn build_alert_style_colors_track_the_kind_palette() {
        for kind in ALL_KINDS {
            let style = build_alert_style(kind);
            let (bg, border, text) = kind.colors();

            assert_eq!(background_color(&style), Some(bg), "{kind:?}: background");
            assert_eq!(text_color(&style), Some(text), "{kind:?}: text colour");

            let borders = border_colors(&style);
            assert_eq!(borders.len(), 4, "{kind:?}: all four edges must be coloured");
            assert!(
                borders.iter().all(|c| *c == border),
                "{kind:?}: every edge must use the kind's border colour, got {borders:?}"
            );
        }
    }

    #[test]
    fn build_alert_style_geometry_is_kind_independent() {
        // Everything that is *not* a colour must be identical for all kinds.
        let expected = [
            CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
            CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
                LayoutFlexDirection::Row,
            )),
            CssPropertyWithConditions::simple(CssProperty::const_align_items(
                LayoutAlignItems::Start,
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
            let style = build_alert_style(kind);
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
    fn build_alert_style_differs_only_in_the_colours() {
        let info = build_alert_style(AlertKind::Info);
        for kind in [AlertKind::Success, AlertKind::Warning, AlertKind::Danger] {
            let other = build_alert_style(kind);
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

    // ------------------------------------------------------------------
    // Alert::create / with_kind / Default
    // ------------------------------------------------------------------

    #[test]
    fn create_is_an_info_alert_with_no_close_button() {
        let alert = Alert::create(AzString::from("hello"));

        assert_eq!(alert.message.as_str(), "hello");
        assert_eq!(alert.kind, AlertKind::Info);
        assert!(!alert.dismissible, "a fresh alert has no close button");
        assert!(alert.alert_state.inner.visible, "a fresh alert is visible");
        assert!(alert.alert_state.on_dismiss.is_none());
        assert_eq!(alert.container_style, build_alert_style(AlertKind::Info));
    }

    #[test]
    fn create_with_empty_message_equals_default_and_is_value_comparable() {
        assert_eq!(Alert::create(AzString::from("")), Alert::default());
        // equality is structural, not pointer-based
        assert_eq!(Alert::create(AzString::from("a")), Alert::create(AzString::from("a")));
        assert_ne!(Alert::create(AzString::from("a")), Alert::create(AzString::from("b")));
        assert_ne!(
            Alert::create(AzString::from("a")),
            Alert::with_kind(AzString::from("a"), AlertKind::Danger)
        );
    }

    #[test]
    fn create_survives_extreme_messages_and_round_trips_them_into_the_dom() {
        let long = "ab".repeat(50_000);
        let cases: Vec<AzString> = alloc::vec![
            AzString::from(""),
            AzString::from(" "),
            AzString::from("a\0b"),                                  // interior NUL
            AzString::from("line\nbreak\ttab"),                      // control chars
            AzString::from("👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪"), // ZWJ + combining + RTL
            AzString::from("\u{feff}\u{202e}rtl-override"),          // BOM + bidi override
            AzString::from("×"),                                     // same glyph as the close button
            AzString::from(long.as_str()),                           // 100k chars
        ];

        for message in cases {
            let alert = Alert::create(message.clone());
            assert_eq!(alert.message.as_str(), message.as_str());

            // the message must survive the trip through the DOM byte-for-byte
            let dom = alert.dom();
            let msg_node = &dom.children.as_ref()[0];
            assert_eq!(text_of(msg_node), Some(message.as_str()));
        }
    }

    #[test]
    fn with_kind_stores_both_args_for_every_kind() {
        for kind in ALL_KINDS {
            let alert = Alert::with_kind(AzString::from("m"), kind);

            assert_eq!(alert.kind, kind);
            assert_eq!(alert.message.as_str(), "m");
            assert!(!alert.dismissible);
            assert!(alert.alert_state.on_dismiss.is_none());
            assert!(alert.alert_state.inner.visible);
            assert_eq!(
                alert.container_style,
                build_alert_style(kind),
                "{kind:?}: the container style must match the kind it was built with"
            );
        }
    }

    // ------------------------------------------------------------------
    // set_kind / with_alert_kind
    // ------------------------------------------------------------------

    #[test]
    fn set_kind_recomputes_the_style_and_is_idempotent() {
        let mut alert = Alert::create(AzString::from("m"));

        for kind in ALL_KINDS {
            alert.set_kind(kind);
            assert_eq!(alert.kind, kind);
            assert_eq!(alert.container_style, build_alert_style(kind));

            // applying the same kind twice must not append/duplicate anything
            let before = alert.container_style.clone();
            alert.set_kind(kind);
            assert_eq!(alert.container_style, before, "{kind:?}: set_kind must be idempotent");
        }

        // a full cycle back to the original kind restores the original alert
        let original = Alert::create(AzString::from("m"));
        let mut cycled = original.clone();
        for kind in ALL_KINDS {
            cycled.set_kind(kind);
        }
        cycled.set_kind(AlertKind::Info);
        assert_eq!(cycled, original, "kind cycling must not accumulate state");
    }

    #[test]
    fn set_kind_leaves_message_dismissible_and_callback_alone() {
        let log = RefAny::new(DismissLog { calls: Vec::new() });
        let mut alert = Alert::create(AzString::from("keep me"));
        alert.set_on_dismiss(log, dismiss_cb(dismiss_do_nothing));
        alert.alert_state.inner.visible = false;

        alert.set_kind(AlertKind::Warning);

        assert_eq!(alert.message.as_str(), "keep me");
        assert!(alert.dismissible, "set_kind must not clear the close button");
        assert!(alert.alert_state.on_dismiss.is_some(), "set_kind must not drop the callback");
        assert!(!alert.alert_state.inner.visible, "set_kind must not resurrect a dismissed alert");
    }

    #[test]
    fn with_alert_kind_matches_set_kind_and_last_write_wins() {
        for kind in ALL_KINDS {
            let built = Alert::create(AzString::from("m")).with_alert_kind(kind);
            let mut mutated = Alert::create(AzString::from("m"));
            mutated.set_kind(kind);
            assert_eq!(built, mutated, "{kind:?}: builder and setter must agree");
        }

        let alert = Alert::create(AzString::from("m"))
            .with_alert_kind(AlertKind::Danger)
            .with_alert_kind(AlertKind::Success);
        assert_eq!(alert.kind, AlertKind::Success);
        assert_eq!(alert.container_style, build_alert_style(AlertKind::Success));
    }

    // ------------------------------------------------------------------
    // set_dismissible / with_dismissible
    // ------------------------------------------------------------------

    #[test]
    fn set_dismissible_last_write_wins_and_touches_nothing_else() {
        let mut alert = Alert::with_kind(AzString::from("m"), AlertKind::Warning);
        let style_before = alert.container_style.clone();

        for flag in [true, true, false, true, false, false] {
            alert.set_dismissible(flag);
            assert_eq!(alert.dismissible, flag);
        }

        assert_eq!(alert.kind, AlertKind::Warning);
        assert_eq!(alert.message.as_str(), "m");
        assert_eq!(alert.container_style, style_before, "toggling must not restyle");
        assert!(alert.alert_state.on_dismiss.is_none(), "toggling must not invent a callback");
    }

    #[test]
    fn with_dismissible_toggle_sequence_ends_on_the_last_value() {
        assert!(Alert::default().with_dismissible(true).dismissible);
        assert!(!Alert::default().with_dismissible(false).dismissible);
        assert!(
            !Alert::default()
                .with_dismissible(true)
                .with_dismissible(false)
                .dismissible
        );
        assert!(
            Alert::default()
                .with_dismissible(false)
                .with_dismissible(true)
                .dismissible
        );
        // builder == setter
        let mut mutated = Alert::default();
        mutated.set_dismissible(true);
        assert_eq!(Alert::default().with_dismissible(true), mutated);
    }

    // ------------------------------------------------------------------
    // set_on_dismiss / with_on_dismiss
    // ------------------------------------------------------------------

    #[test]
    fn set_on_dismiss_implies_dismissible() {
        let mut alert = Alert::create(AzString::from("m"));
        assert!(!alert.dismissible);

        alert.set_on_dismiss(RefAny::new(1u8), dismiss_cb(dismiss_do_nothing));

        assert!(alert.dismissible, "a dismiss callback must render a close button");
        assert!(alert.alert_state.on_dismiss.is_some());
        assert!(alert.alert_state.inner.visible, "wiring a callback must not hide the alert");
    }

    #[test]
    fn set_on_dismiss_replaces_rather_than_appends() {
        let mut alert = Alert::create(AzString::from("m"));

        alert.set_on_dismiss(RefAny::new(1u8), dismiss_cb(dismiss_do_nothing));
        let first = alert
            .alert_state
            .on_dismiss
            .as_ref()
            .expect("first callback")
            .refany
            .get_type_id();
        assert_eq!(first, RefAny::new(1u8).get_type_id());

        // a second call must *replace* the payload + function, not stack another one
        alert.set_on_dismiss(RefAny::new(9i64), dismiss_cb(record_dismiss));
        let second = alert.alert_state.on_dismiss.as_ref().expect("second callback");
        assert_eq!(second.refany.get_type_id(), RefAny::new(9i64).get_type_id());
        assert_eq!(second.callback, dismiss_cb(record_dismiss));
        assert_ne!(second.callback, dismiss_cb(dismiss_do_nothing));
    }

    #[test]
    fn with_on_dismiss_keeps_message_and_kind() {
        let alert = Alert::with_kind(AzString::from("boom"), AlertKind::Danger)
            .with_on_dismiss(RefAny::new(0u8), dismiss_cb(dismiss_do_nothing));

        assert_eq!(alert.message.as_str(), "boom");
        assert_eq!(alert.kind, AlertKind::Danger);
        assert_eq!(alert.container_style, build_alert_style(AlertKind::Danger));
        assert!(alert.dismissible);
        assert!(alert.alert_state.on_dismiss.is_some());
    }

    #[test]
    fn set_dismissible_false_after_set_on_dismiss_silently_drops_the_close_button() {
        // Footgun, pinned as the *current* behaviour: `set_on_dismiss` implies
        // `dismissible = true`, but a later `set_dismissible(false)` wins and the
        // wired-up callback becomes unreachable (no close button is rendered).
        let mut alert = Alert::create(AzString::from("m"));
        alert.set_on_dismiss(RefAny::new(0u8), dismiss_cb(record_dismiss));
        alert.set_dismissible(false);

        assert!(alert.alert_state.on_dismiss.is_some(), "the callback is still stored");
        let dom = alert.dom();
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "no close button is rendered, so the callback can never fire"
        );
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_resets_self() {
        let mut alert = Alert::with_kind(AzString::from("payload"), AlertKind::Danger)
            .with_dismissible(true);
        let snapshot = alert.clone();

        let returned = alert.swap_with_default();

        assert_eq!(returned, snapshot, "the original must come back untouched");
        assert_eq!(alert, Alert::default(), "self must be reset to a default alert");
        assert_eq!(alert.message.as_str(), "");
        assert_eq!(alert.kind, AlertKind::Info);
        assert!(!alert.dismissible);
        assert!(alert.alert_state.on_dismiss.is_none());
        assert!(alert.alert_state.inner.visible);
    }

    #[test]
    fn swap_with_default_is_stable_when_repeated() {
        let mut alert = Alert::default();
        for _ in 0..3 {
            let returned = alert.swap_with_default();
            assert_eq!(returned, Alert::default());
            assert_eq!(alert, Alert::default());
        }
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_of_self() {
        let mut alert = Alert::create(AzString::from("m"))
            .with_on_dismiss(RefAny::new(7u32), dismiss_cb(record_dismiss));

        let returned = alert.swap_with_default();

        assert!(returned.alert_state.on_dismiss.is_some(), "the callback moves out");
        assert!(
            alert.alert_state.on_dismiss.is_none(),
            "the reset alert must not keep a reference to the old callback"
        );
        assert!(!alert.dismissible);
    }

    // ------------------------------------------------------------------
    // Alert::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_a_plain_alert_is_a_container_with_one_message_child() {
        let alert = Alert::create(AzString::from("hi"));
        let style = alert.container_style.clone();
        let dom = alert.dom();

        assert!(dom.root.has_class("__azul-native-alert"));
        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "a non-dismissible alert must carry no live callback"
        );
        assert_eq!(
            dom.root.style.iter_inline_properties().count(),
            style.len(),
            "every container property must reach the node's inline style"
        );

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 1, "no close button without `dismissible`");
        assert!(children[0].root.has_class("__azul-native-alert-message"));
        assert_eq!(text_of(&children[0]), Some("hi"));
        assert!(children[0].root.get_callbacks().as_ref().is_empty());
        assert!(children[0].root.get_tab_index().is_none());
    }

    #[test]
    fn dom_of_a_dismissible_alert_appends_a_focusable_close_button() {
        let dom = Alert::create(AzString::from("hi")).with_dismissible(true).dom();

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "[message, close]");

        let close = &children[1];
        assert!(close.root.has_class("__azul-native-alert-close"));
        assert_eq!(text_of(close), Some("\u{00D7}"), "the close glyph is U+00D7 MULTIPLICATION SIGN");
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
        assert_eq!(cb.callback.cb, default_on_alert_dismiss as usize);
        assert!(matches!(&cb.callback.ctx, OptionRefAny::None));
    }

    #[test]
    fn dom_hands_the_alert_state_to_the_close_button() {
        let alert = Alert::create(AzString::from("hi"))
            .with_on_dismiss(RefAny::new(0u8), dismiss_cb(record_dismiss));
        let dom = alert.dom();

        let close = &dom.children.as_ref()[1];
        let mut payload = close.root.get_callbacks().as_ref()[0].refany.clone();

        assert!(
            wrapper_visible(&mut payload),
            "the close button must receive a live, visible AlertStateWrapper"
        );
        assert!(
            payload
                .downcast_ref::<AlertStateWrapper>()
                .expect("AlertStateWrapper")
                .on_dismiss
                .is_some(),
            "the user callback must travel with the state"
        );
    }

    #[test]
    fn dom_is_stable_across_kinds_and_only_the_container_style_changes() {
        for kind in ALL_KINDS {
            let dom = Alert::with_kind(AzString::from("m"), kind)
                .with_dismissible(true)
                .dom();
            assert!(dom.root.has_class("__azul-native-alert"));
            assert_eq!(dom.children.as_ref().len(), 2);

            // NOTE: `AlertKind::class_name()` is *not* applied to the DOM - the
            // container only ever carries the generic container class.
            assert!(
                !dom.root.has_class(kind.class_name()),
                "current behaviour: the kind class is not emitted"
            );
        }
    }

    // ------------------------------------------------------------------
    // default_on_alert_dismiss
    // ------------------------------------------------------------------

    #[test]
    fn dismiss_hides_the_container_and_flips_visible() {
        let mut data = RefAny::new(AlertStateWrapper::default());

        // node 2 == the close button, its parent (node 0) is the container
        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 2, data.clone());

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
        let mut data = RefAny::new(AlertStateWrapper {
            inner: AlertState { visible: true },
            on_dismiss: Some(AlertOnDismiss {
                callback: dismiss_cb(record_dismiss),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 2, data.clone());

        assert_eq!(update, Update::RefreshDom, "the user callback's Update is returned");
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
        let mut data = RefAny::new(AlertStateWrapper {
            inner: AlertState { visible: true },
            on_dismiss: Some(AlertOnDismiss {
                callback: dismiss_cb(record_dismiss),
                refany: log.clone(),
            })
            .into(),
        });

        for _ in 0..2 {
            let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 2, data.clone());
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(display_writes(&changes), alloc::vec![(0usize, LayoutDisplay::None)]);
        }

        assert!(!wrapper_visible(&mut data), "a second dismiss must not un-hide");
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false, false],
            "each click fires the callback exactly once, always with visible == false"
        );
    }

    #[test]
    fn dismiss_on_a_root_hit_node_is_a_noop() {
        // node 0 has no parent -> there is no container to hide
        let mut data = RefAny::new(AlertStateWrapper::default());

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "nothing may be restyled without a parent");
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn dismiss_with_a_stale_hit_node_is_a_noop() {
        // node 999 does not exist in the 3-node fixture
        let mut data = RefAny::new(AlertStateWrapper::default());

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data));
    }

    #[test]
    fn dismiss_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(AlertStateWrapper::default());

        let (update, changes) = run_dismiss(None, 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn dismiss_with_a_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_dismiss(Some(dismissible_styled_dom()), 2, data.clone());

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
        let alert = Alert::create(AzString::from("bye")).with_dismissible(true);
        let dom = alert.dom();
        let close = &dom.children.as_ref()[1];
        let entry = &close.root.get_callbacks().as_ref()[0];
        assert_eq!(entry.callback.cb, default_on_alert_dismiss as usize);
        let mut payload = entry.refany.clone();

        let styled = StyledDom::create_from_dom(dom);
        let (update, changes) = run_dismiss(Some(styled), 2, payload.clone());

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
}
