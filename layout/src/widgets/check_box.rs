//! Checkbox widget with toggle callback support and default native-like styling.
//!
//! Key types: [`CheckBox`], [`CheckBoxState`], [`CheckBoxOnToggle`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

static CHECKBOX_CONTAINER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-checkbox-container",
))];
static CHECKBOX_CONTENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-checkbox-content",
))];

/// Callback function type invoked when the checkbox is toggled.
pub type CheckBoxOnToggleCallbackType =
    extern "C" fn(RefAny, CallbackInfo, CheckBoxState) -> Update;
impl_widget_callback!(
    CheckBoxOnToggle,
    OptionCheckBoxOnToggle,
    CheckBoxOnToggleCallback,
    CheckBoxOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CheckBoxOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CHECK_BOX_ON_TOGGLE_INVOKER,
    invoker_ty:     AzCheckBoxOnToggleCallbackInvoker,
    thunk_fn:       az_check_box_on_toggle_callback_thunk,
    setter_fn:      AzApp_setCheckBoxOnToggleCallbackInvoker,
    from_handle_fn: AzCheckBoxOnToggleCallback_createFromHostHandle,
    extra_args:     [ state: CheckBoxState ],
}

/// A toggleable checkbox widget with customizable styling and toggle callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CheckBox {
    pub check_box_state: CheckBoxStateWrapper,
    /// Style for the checkbox container
    pub container_style: CssPropertyWithConditionsVec,
    /// Style for the checkbox content
    pub content_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CheckBoxStateWrapper {
    /// Content (image or text) of this `CheckBox`, centered by default
    pub inner: CheckBoxState,
    /// Optional: Function to call when the `CheckBox` is toggled
    pub on_toggle: OptionCheckBoxOnToggle,
}

/// The checked/unchecked state of a [`CheckBox`].
#[derive(Copy, Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CheckBoxState {
    pub checked: bool,
}

const BACKGROUND_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white
const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);
const COLOR_9B9B9B: ColorU = ColorU {
    r: 155,
    g: 155,
    b: 155,
    a: 255,
}; // #9b9b9b

const FILL_THEME: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(COLOR_9B9B9B)];
const FILL_COLOR_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(FILL_THEME);

static DEFAULT_CHECKBOX_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        BACKGROUND_COLOR_LIGHT,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(14))),
    // padding: 2px
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        2,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(2),
    )),
    // border: 1px solid #484c52;
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
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

static DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(FILL_COLOR_BACKGROUND)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

static DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(FILL_COLOR_BACKGROUND)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

impl CheckBox {
    #[must_use] pub fn create(checked: bool) -> Self {
        Self {
            check_box_state: CheckBoxStateWrapper {
                inner: CheckBoxState { checked },
                ..Default::default()
            },
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                DEFAULT_CHECKBOX_CONTAINER_STYLE,
            ),
            content_style: if checked {
                CssPropertyWithConditionsVec::from_const_slice(
                    DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED,
                )
            } else {
                CssPropertyWithConditionsVec::from_const_slice(
                    DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED,
                )
            },
        }
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(false);
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_toggle<C: Into<CheckBoxOnToggleCallback>>(&mut self, data: RefAny, on_toggle: C) {
        self.check_box_state.on_toggle = Some(CheckBoxOnToggle {
            callback: on_toggle.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_toggle<C: Into<CheckBoxOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{Dom, EventFilter, HoverEventFilter},
        };

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from(CHECKBOX_CONTAINER_CLASS))
            .with_css_props(self.container_style)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: input::default_on_checkbox_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(self.check_box_state),
                }]
                .into(),
            )
            .with_tab_index(TabIndex::Auto)
            .with_children(
                vec![Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from(CHECKBOX_CONTENT_CLASS))
                    .with_css_props(self.content_style)]
                .into(),
            )
    }
}

// handle input events for the checkbox
mod input {

    use azul_core::{callbacks::Update, refany::RefAny};
    use azul_css::props::{property::CssProperty, style::effects::StyleOpacity};

    use super::{CheckBoxOnToggle, CheckBoxStateWrapper};
    use crate::callbacks::CallbackInfo;

    pub(super) extern "C" fn default_on_checkbox_clicked(
        mut check_box: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        let Some(mut check_box) = check_box.downcast_mut::<CheckBoxStateWrapper>() else {
            return Update::DoNothing;
        };

        let Some(checkbox_content_id) = info.get_first_child(info.get_hit_node()) else {
            return Update::DoNothing;
        };

        check_box.inner.checked = !check_box.inner.checked;

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let check_box = &mut *check_box;
            let ontoggle = &mut check_box.on_toggle;
            let inner = check_box.inner;

            match ontoggle.as_mut() {
                Some(CheckBoxOnToggle {
                    callback,
                    refany: data,
                }) => (callback.cb)(data.clone(), info, inner),
                None => Update::DoNothing,
            }
        };

        if check_box.inner.checked {
            info.set_css_property(
                checkbox_content_id,
                CssProperty::const_opacity(StyleOpacity::const_new(100)),
            );
        } else {
            info.set_css_property(
                checkbox_content_id,
                CssProperty::const_opacity(StyleOpacity::const_new(0)),
            );
        }

        result
    }
}

impl From<CheckBox> for Dom {
    fn from(b: CheckBox) -> Self {
        b.dom()
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        mem::discriminant,
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
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};
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
    // Harness
    // ------------------------------------------------------------------

    /// The container is `14px` wide with `2px` padding and a `1px` border on each
    /// side; the checkmark is `8px`. These four numbers are the entire geometry of
    /// the widget, and `8 == 14 - 2*2 - 2*1` is the relation that makes the mark
    /// fill the padding box exactly (see `content_exactly_fills_the_containers_padding_box`).
    const CONTAINER_SIDE: f32 = 14.0;
    const PADDING: f32 = 2.0;
    const BORDER: f32 = 1.0;
    const CONTENT_SIDE: f32 = 8.0;

    /// Flattened node ids of `CheckBox::dom()` (pre-order).
    const CONTAINER: usize = 0;
    const CONTENT: usize = 1;

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was hit"
    /// case. `CallbackInfo::set_css_property` *panics* on such an id, so the handler
    /// must bail out before ever reaching it.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`: the checkbox handler reaches
    /// exactly two `CallbackInfo` queries (`get_hit_node`, `get_first_child`), and
    /// both read the node hierarchy only — no real layout (and no font) is needed.
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

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root DOM
    /// and whose hit node is `hit`. Returns `f`'s value plus every change the callback
    /// pushed onto the transaction log.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

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
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// Renders `check_box`, then hands back both the laid-out DOM *and* the very
    /// `RefAny` the widget registered on its own mouse-up callback. Driving the
    /// handler with these two is the real wiring — nothing is re-created by hand,
    /// so a mismatch between what `dom()` stores and what the handler expects
    /// cannot hide behind the fixture.
    fn laid_out(check_box: CheckBox) -> (StyledDom, RefAny) {
        let dom = check_box.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    /// One "mouse-up on `hit`" delivered to the widget's own registered handler.
    fn click(
        styled_dom: StyledDom,
        state: &RefAny,
        hit: DomNodeId,
    ) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| {
            input::default_on_checkbox_clicked(state.clone(), *info)
        })
    }

    fn is_checked(state: &RefAny) -> bool {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<CheckBoxStateWrapper>()
            .expect("the widget state changed type");
        wrapper.inner.checked
    }

    /// The opacity overrides pushed onto the content node, in push order.
    fn pushed_opacities(changes: &[CallbackChange]) -> Vec<(NodeId, f32)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id, properties, ..
                } => {
                    let o = properties.as_ref().iter().find_map(|p| match p {
                        CssProperty::Opacity(o) => o.get_property().map(|o| o.inner.normalized()),
                        _ => None,
                    })?;
                    Some((*node_id, o))
                }
                _ => None,
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Style-vec probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    fn find<T>(v: &CssPropertyWithConditionsVec, f: impl Fn(&CssProperty) -> Option<T>) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An `em`
    /// or `%` slipping into the checkbox geometry would resolve against the parent
    /// font/box, so a 14px box could render at any size at all.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "checkbox geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    /// The declared opacity, normalized to 0.0..=1.0. `StyleOpacity::const_new` takes
    /// a *percentage*, so a `const_new(1)` typo yields 1% — a checkmark that is there
    /// but invisible. This is the only property that distinguishes the two states.
    fn opacity(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Opacity(o) => o.get_property().map(|o| o.inner.normalized()),
            _ => None,
        })
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // Toggle callbacks
    // ------------------------------------------------------------------

    /// A payload the toggle callback writes into. It arrives as the `data: RefAny`
    /// argument — a *shared* clone of what the test still holds — so the test can
    /// read back exactly what the widget passed, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ToggleLog {
        seen: Vec<bool>,
        payload: u32,
    }

    extern "C" fn record_toggle(
        mut data: RefAny,
        _info: CallbackInfo,
        state: CheckBoxState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.seen.push(state.checked);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: CheckBoxState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn toggle_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: CheckBoxState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in, which the
    /// `From<Callback>` arm *transmutes* into the 3-arg checkbox slot. Never called.
    extern "C" fn generic_shaped(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn log_refany() -> RefAny {
        RefAny::new(ToggleLog {
            seen: Vec::new(),
            payload: 0xDEAD_BEEF,
        })
    }

    fn read_log(probe: &RefAny) -> ToggleLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<ToggleLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    // ==================================================================
    // CheckBox::create
    // ==================================================================

    #[test]
    fn create_stores_the_checked_flag_and_installs_no_callback() {
        for checked in [false, true] {
            let c = CheckBox::create(checked);
            assert_eq!(
                c.check_box_state.inner.checked, checked,
                "create({checked}) did not store the flag it was given",
            );
            assert!(
                c.check_box_state.on_toggle.as_ref().is_none(),
                "create({checked}) invented a toggle callback out of nowhere",
            );
        }
    }

    #[test]
    fn create_is_pure_and_its_two_states_are_distinguishable() {
        // Same input -> same widget (the const style tables are shared, not rebuilt),
        // and the checked/unchecked widgets must not compare equal — if they did, the
        // two states would render identically.
        assert_eq!(CheckBox::create(true), CheckBox::create(true));
        assert_eq!(CheckBox::create(false), CheckBox::create(false));
        assert_ne!(
            CheckBox::create(true),
            CheckBox::create(false),
            "a checked and an unchecked checkbox are indistinguishable",
        );
    }

    #[test]
    fn create_only_the_checkmark_opacity_depends_on_the_checked_flag() {
        // The container chrome must be byte-for-byte identical in both states: a box
        // that changed size/colour when ticked would reflow its neighbours.
        let checked = CheckBox::create(true);
        let unchecked = CheckBox::create(false);
        assert_eq!(
            properties(&checked.container_style),
            properties(&unchecked.container_style),
            "the container style differs between the checked and unchecked state",
        );

        // ... and the *content* styles differ in opacity and nothing else.
        let a = properties(&checked.content_style);
        let b = properties(&unchecked.content_style);
        assert_eq!(a.len(), b.len(), "the two content styles declare a different number of properties");
        let differing: Vec<_> = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x != y)
            .map(|(x, _)| discriminant(x))
            .collect();
        assert_eq!(
            differing,
            vec![discriminant(&CssProperty::const_opacity(StyleOpacity::const_new(0)))],
            "the checked/unchecked content styles differ in something other than opacity",
        );
    }

    #[test]
    fn create_maps_checked_to_a_fully_opaque_mark_and_unchecked_to_a_fully_transparent_one() {
        // `StyleOpacity::const_new` takes a percentage: 100 -> 1.0, 0 -> 0.0. A swapped
        // branch (or a `const_new(1)` typo) yields a checkbox that is permanently ticked
        // or permanently blank — both of which still *type*-check.
        assert_eq!(
            opacity(&CheckBox::create(true).content_style),
            Some(1.0),
            "a checked checkbox does not show its checkmark",
        );
        assert_eq!(
            opacity(&CheckBox::create(false).content_style),
            Some(0.0),
            "an unchecked checkbox still shows its checkmark",
        );
    }

    #[test]
    fn create_geometry_is_absolute_px_in_both_states() {
        for checked in [false, true] {
            let c = CheckBox::create(checked);
            // `px()` asserts SizeMetric::Px — an em/% here would scale with the parent.
            assert_eq!(width_px(&c.container_style), Some(CONTAINER_SIDE));
            assert_eq!(height_px(&c.container_style), Some(CONTAINER_SIDE));
            assert_eq!(width_px(&c.content_style), Some(CONTENT_SIDE));
            assert_eq!(height_px(&c.content_style), Some(CONTENT_SIDE));
        }
    }

    #[test]
    fn content_exactly_fills_the_containers_padding_box() {
        // 8 == 14 - 2*2 - 2*1. The mark is sized to fill the box's padding box exactly
        // under border-box sizing; if any of the four constants drifts, the checkmark
        // either overflows its box or leaves a gap, and nothing else in the file would
        // notice.
        assert_eq!(
            CONTENT_SIDE,
            CONTAINER_SIDE - 2.0 * PADDING - 2.0 * BORDER,
            "the checkbox geometry constants no longer add up",
        );

        let c = CheckBox::create(true);
        let padding = |f: fn(&CssProperty) -> Option<f32>| find(&c.container_style, f);
        let pad_l = padding(|p| match p {
            CssProperty::PaddingLeft(v) => v.get_property().map(|v| px(&v.inner)),
            _ => None,
        });
        let pad_r = padding(|p| match p {
            CssProperty::PaddingRight(v) => v.get_property().map(|v| px(&v.inner)),
            _ => None,
        });
        let bor_l = padding(|p| match p {
            CssProperty::BorderLeftWidth(v) => v.get_property().map(|v| px(&v.inner)),
            _ => None,
        });
        let bor_r = padding(|p| match p {
            CssProperty::BorderRightWidth(v) => v.get_property().map(|v| px(&v.inner)),
            _ => None,
        });

        assert_eq!(pad_l, Some(PADDING));
        assert_eq!(pad_r, Some(PADDING));
        assert_eq!(bor_l, Some(BORDER));
        assert_eq!(bor_r, Some(BORDER));

        let inner = CONTAINER_SIDE
            - pad_l.unwrap()
            - pad_r.unwrap()
            - bor_l.unwrap()
            - bor_r.unwrap();
        assert_eq!(
            width_px(&c.content_style),
            Some(inner),
            "the checkmark no longer fills the container's padding box",
        );
    }

    #[test]
    fn create_declares_no_property_twice() {
        // A duplicate declaration means the later one silently wins — a latent
        // "why is my override ignored" bug that never surfaces as an error.
        for checked in [false, true] {
            let c = CheckBox::create(checked);
            for (name, v) in [
                ("container", &c.container_style),
                ("content", &c.content_style),
            ] {
                let props = properties(v);
                let mut seen = Vec::new();
                for p in &props {
                    let d = discriminant(p);
                    assert!(
                        !seen.contains(&d),
                        "checked={checked}: the {name} style declares {p:?} twice",
                    );
                    seen.push(d);
                }
            }
        }
    }

    #[test]
    fn create_marks_the_container_as_clickable() {
        // Without `cursor: pointer` the checkbox looks inert even though it is the
        // node that carries the mouse-up handler.
        for checked in [false, true] {
            let cursor = find(&CheckBox::create(checked).container_style, |p| match p {
                CssProperty::Cursor(c) => c.get_property().copied(),
                _ => None,
            });
            assert_eq!(
                cursor,
                Some(StyleCursor::Pointer),
                "checked={checked}: the checkbox does not present as clickable",
            );
        }
    }

    // ==================================================================
    // CheckBox::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_an_unchecked_default_behind() {
        let mut c = CheckBox::create(true);
        let old = c.swap_with_default();

        assert_eq!(old, CheckBox::create(true), "the old widget was not returned intact");
        assert_eq!(c, CheckBox::create(false), "what was left behind is not a fresh unchecked checkbox");
    }

    #[test]
    fn swap_with_default_on_an_already_default_widget_is_a_no_op() {
        let mut c = CheckBox::create(false);
        let old = c.swap_with_default();
        assert_eq!(old, c, "swapping a default with a default produced two different widgets");
        assert_eq!(old, CheckBox::create(false));
    }

    #[test]
    fn swap_with_default_moves_the_toggle_callback_out_rather_than_copying_or_dropping_it() {
        let probe = log_refany();
        let mut c = CheckBox::create(true)
            .with_on_toggle(probe.clone(), record_toggle as CheckBoxOnToggleCallbackType);

        let old = c.swap_with_default();

        // The callback (and its payload) left with the returned value ...
        let moved = old
            .check_box_state
            .on_toggle
            .as_ref()
            .expect("the toggle callback vanished during the swap");
        assert_eq!(
            moved.callback.cb as *const () as usize,
            record_toggle as CheckBoxOnToggleCallbackType as *const () as usize,
            "the fn pointer was mangled by the swap",
        );

        // ... and did NOT stay behind: a duplicated callback would fire twice, and a
        // duplicated RefAny would double-free its payload.
        assert!(
            c.check_box_state.on_toggle.as_ref().is_none(),
            "the toggle callback was copied instead of moved",
        );

        // The payload is still alive and unchanged after the move.
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn swapping_twice_round_trips_the_original_widget() {
        let mut a = CheckBox::create(true);
        let mut b = a.swap_with_default(); // a = default, b = checked
        let c = b.swap_with_default(); // b = default, c = checked

        assert_eq!(c, CheckBox::create(true));
        assert_eq!(a, CheckBox::create(false));
        assert_eq!(b, CheckBox::create(false));
    }

    // ==================================================================
    // CheckBox::set_on_toggle / with_on_toggle
    // ==================================================================

    #[test]
    fn set_on_toggle_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut c = CheckBox::create(false);
        c.set_on_toggle(RefAny::new(0xDEAD_BEEF_u32), toggle_do_nothing as CheckBoxOnToggleCallbackType);

        let t = c
            .check_box_state
            .on_toggle
            .as_ref()
            .expect("set_on_toggle did not store anything");
        assert_eq!(
            t.callback.cb as *const () as usize,
            toggle_do_nothing as CheckBoxOnToggleCallbackType as *const () as usize,
            "the fn pointer was corrupted on the way in",
        );

        let mut data = t.refany.clone();
        assert_eq!(
            *data.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
            "the payload was corrupted",
        );
        assert!(
            data.downcast_ref::<u64>().is_none(),
            "downcasting to the wrong type must fail, not reinterpret the bytes",
        );
    }

    #[test]
    fn set_on_toggle_replaces_rather_than_accumulates() {
        // `OptionCheckBoxOnToggle` is a single slot; setting twice must leave the
        // *second* callback installed (and must not leak the first one's RefAny).
        let first = log_refany();
        let mut c = CheckBox::create(false);
        c.set_on_toggle(first.clone(), toggle_do_nothing as CheckBoxOnToggleCallbackType);
        c.set_on_toggle(RefAny::new(1u8), toggle_refresh_all as CheckBoxOnToggleCallbackType);

        let t = c.check_box_state.on_toggle.as_ref().expect("the callback vanished");
        assert_eq!(
            t.callback.cb as *const () as usize,
            toggle_refresh_all as CheckBoxOnToggleCallbackType as *const () as usize,
            "the second set_on_toggle did not win",
        );
        // The displaced payload is still a valid, readable RefAny (not freed twice).
        assert_eq!(read_log(&first).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn set_on_toggle_does_not_disturb_the_state_or_the_styles() {
        for checked in [false, true] {
            let pristine = CheckBox::create(checked);
            let mut c = CheckBox::create(checked);
            c.set_on_toggle(RefAny::new(0u8), toggle_do_nothing as CheckBoxOnToggleCallbackType);

            assert_eq!(
                c.check_box_state.inner.checked, checked,
                "installing a callback flipped the checked flag",
            );
            assert_eq!(
                properties(&c.container_style),
                properties(&pristine.container_style),
                "installing a callback rewrote the container style",
            );
            assert_eq!(
                properties(&c.content_style),
                properties(&pristine.content_style),
                "installing a callback rewrote the content style",
            );
        }
    }

    #[test]
    fn with_on_toggle_is_exactly_set_on_toggle_in_builder_form() {
        let by_builder = CheckBox::create(true)
            .with_on_toggle(RefAny::new(7u32), toggle_do_nothing as CheckBoxOnToggleCallbackType);

        let mut by_setter = CheckBox::create(true);
        by_setter.set_on_toggle(RefAny::new(7u32), toggle_do_nothing as CheckBoxOnToggleCallbackType);

        assert_eq!(
            by_builder.check_box_state.inner,
            by_setter.check_box_state.inner,
        );
        assert_eq!(
            properties(&by_builder.container_style),
            properties(&by_setter.container_style),
        );
        assert_eq!(
            properties(&by_builder.content_style),
            properties(&by_setter.content_style),
        );

        let a = by_builder.check_box_state.on_toggle.as_ref().expect("builder lost the callback");
        let b = by_setter.check_box_state.on_toggle.as_ref().expect("setter lost the callback");
        assert_eq!(a.callback.cb as *const () as usize, b.callback.cb as *const () as usize);

        let (mut a, mut b) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *a.downcast_ref::<u32>().expect("builder payload changed type"),
            *b.downcast_ref::<u32>().expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_toggle_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // checkbox slot — this is the FFI (Python/C) path. The pointer must come out
        // bit-identical; a mangled one would be called as a wild jump on the first click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: azul_core::refany::OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let c = CheckBox::create(false).with_on_toggle(RefAny::new(0u8), generic);
        let t = c.check_box_state.on_toggle.as_ref().expect("the generic callback was dropped");
        assert_eq!(
            t.callback.cb as *const () as usize,
            expected,
            "the Callback -> CheckBoxOnToggleCallback transmute mangled the pointer",
        );
    }

    // ==================================================================
    // CheckBox::dom
    // ==================================================================

    #[test]
    fn dom_builds_a_focusable_container_with_exactly_one_content_child() {
        for checked in [false, true] {
            let dom = CheckBox::create(checked).dom();

            assert!(matches!(dom.root.get_node_type(), NodeType::Div));
            assert_eq!(
                dom.root.flags.get_tab_index(),
                Some(TabIndex::Auto),
                "checked={checked}: the checkbox is not keyboard-focusable",
            );
            assert_eq!(classes(&dom), vec!["__azul-native-checkbox-container".to_string()]);

            let children = dom.children.as_ref();
            assert_eq!(children.len(), 1, "checked={checked}: the checkbox must have exactly one child");
            assert_eq!(
                classes(&children[0]),
                vec!["__azul-native-checkbox-content".to_string()],
            );
            assert!(
                children[0].children.as_ref().is_empty(),
                "checked={checked}: the checkmark grew children",
            );
        }
    }

    #[test]
    fn dom_puts_the_container_style_on_the_box_and_the_content_style_on_the_mark() {
        // Swapping the two would style the 8px mark like a 14px bordered box (and vice
        // versa) — the widget would still render, just wrong.
        for checked in [false, true] {
            let c = CheckBox::create(checked);
            let container = properties(&c.container_style);
            let content = properties(&c.content_style);

            let dom = c.dom();
            assert_eq!(
                inline_properties(&dom),
                container,
                "checked={checked}: the container style did not land on the container",
            );
            assert_eq!(
                inline_properties(&dom.children.as_ref()[0]),
                content,
                "checked={checked}: the content style did not land on the checkmark",
            );
        }
    }

    #[test]
    fn dom_registers_exactly_one_mouse_up_handler_and_it_is_the_widgets_own() {
        for checked in [false, true] {
            let dom = CheckBox::create(checked).dom();
            let callbacks = dom.root.callbacks.as_ref();

            assert_eq!(callbacks.len(), 1, "checked={checked}: expected exactly one callback");
            assert_eq!(
                callbacks[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "checked={checked}: the checkbox must toggle on mouse-up",
            );
            assert_eq!(
                callbacks[0].callback.cb,
                input::default_on_checkbox_clicked as usize,
                "checked={checked}: the registered handler is not default_on_checkbox_clicked",
            );

            // The mark itself must stay inert — a second handler there would toggle twice
            // per click (the event bubbles).
            assert!(
                dom.children.as_ref()[0].root.callbacks.as_ref().is_empty(),
                "checked={checked}: the checkmark registered a handler of its own",
            );
        }
    }

    #[test]
    fn dom_hands_the_widget_state_to_the_handler_not_the_user_payload() {
        // `dom()` moves `check_box_state` (state + on_toggle + user RefAny) into the
        // callback's RefAny. If it stored the *user's* payload instead, the handler's
        // `downcast_mut::<CheckBoxStateWrapper>()` would fail and every click would be
        // a silent no-op.
        for checked in [false, true] {
            let dom = CheckBox::create(checked)
                .with_on_toggle(RefAny::new(9u32), toggle_do_nothing as CheckBoxOnToggleCallbackType)
                .dom();

            let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
            let wrapper = state
                .downcast_ref::<CheckBoxStateWrapper>()
                .expect("the handler's RefAny is not a CheckBoxStateWrapper");

            assert_eq!(wrapper.inner.checked, checked, "the checked flag was lost on the way into the DOM");
            assert!(
                wrapper.on_toggle.as_ref().is_some(),
                "the user's toggle callback was lost on the way into the DOM",
            );
        }
    }

    #[test]
    fn dom_of_a_callback_less_checkbox_still_registers_the_toggle_handler() {
        // Unlike Button (which registers nothing without an on_click), the checkbox must
        // always install its own handler: the box has to tick even with no user callback.
        let dom = CheckBox::create(false).dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);

        let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
        let wrapper = state.downcast_ref::<CheckBoxStateWrapper>().expect("wrong RefAny type");
        assert!(wrapper.on_toggle.as_ref().is_none());
    }

    #[test]
    fn from_checkbox_for_dom_is_the_same_as_calling_dom() {
        for checked in [false, true] {
            let via_from = Dom::from(CheckBox::create(checked));
            let via_dom = CheckBox::create(checked).dom();

            assert_eq!(classes(&via_from), classes(&via_dom));
            assert_eq!(inline_properties(&via_from), inline_properties(&via_dom));
            assert_eq!(via_from.children.as_ref().len(), via_dom.children.as_ref().len());
            assert_eq!(
                via_from.root.callbacks.as_ref().len(),
                via_dom.root.callbacks.as_ref().len(),
            );
            assert_eq!(via_from.root.flags.get_tab_index(), via_dom.root.flags.get_tab_index());
        }
    }

    #[test]
    fn the_rendered_dom_flattens_to_exactly_two_nodes() {
        // `Dom::estimated_total_children` is a *cached* count; if it under-reports, the
        // flatten under-allocates its arenas. Two nodes: container (0), content (1).
        let styled = StyledDom::create_from_dom(CheckBox::create(true).dom());
        assert_eq!(
            styled.node_data.as_ref().len(),
            2,
            "the checkbox no longer flattens to a container + a checkmark",
        );
    }

    // ==================================================================
    // input::default_on_checkbox_clicked
    // ==================================================================

    #[test]
    fn clicking_toggles_the_flag_and_pushes_the_matching_opacity_onto_the_checkmark() {
        let (styled, state) = laid_out(CheckBox::create(false));
        assert!(!is_checked(&state));

        let (update, changes) = click(styled, &state, node(CONTAINER));

        assert!(is_checked(&state), "the click did not tick the checkbox");
        assert!(
            matches!(update, Update::DoNothing),
            "with no user callback installed, the handler must report DoNothing",
        );
        assert_eq!(
            pushed_opacities(&changes),
            vec![(NodeId::new(CONTENT), 1.0)],
            "ticking the box did not make the checkmark opaque (on the *content* node)",
        );
    }

    #[test]
    fn clicking_a_checked_box_unticks_it_and_hides_the_checkmark() {
        let (styled, state) = laid_out(CheckBox::create(true));
        let (_, changes) = click(styled, &state, node(CONTAINER));

        assert!(!is_checked(&state), "the click did not untick the checkbox");
        assert_eq!(
            pushed_opacities(&changes),
            vec![(NodeId::new(CONTENT), 0.0)],
            "unticking the box left the checkmark visible",
        );
    }

    #[test]
    fn clicking_twice_returns_to_the_original_state() {
        let (styled, state) = laid_out(CheckBox::create(false));

        // Two independent deliveries against the same widget state — the styled DOM is
        // rebuilt each time because the harness consumes it, but the RefAny is shared,
        // which is exactly how the real event loop drives it.
        let (styled2, _) = laid_out(CheckBox::create(false));
        let (_, first) = click(styled, &state, node(CONTAINER));
        let (_, second) = click(styled2, &state, node(CONTAINER));

        assert!(!is_checked(&state), "two clicks did not return the checkbox to its original state");
        assert_eq!(pushed_opacities(&first), vec![(NodeId::new(CONTENT), 1.0)]);
        assert_eq!(pushed_opacities(&second), vec![(NodeId::new(CONTENT), 0.0)]);
    }

    #[test]
    fn clicking_with_a_refany_of_the_wrong_type_is_a_silent_no_op() {
        // The handler downcasts blind; a foreign RefAny must bail out, not reinterpret
        // the bytes as a CheckBoxStateWrapper.
        let (styled, _) = laid_out(CheckBox::create(false));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = click(styled, &foreign, node(CONTAINER));

        assert!(matches!(update, Update::DoNothing));
        assert!(changes.is_empty(), "the handler wrote to the DOM through a foreign RefAny");

        let mut foreign = foreign;
        assert_eq!(
            *foreign.downcast_ref::<u32>().expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand",
        );
    }

    #[test]
    fn clicking_a_childless_node_does_not_half_apply_the_toggle() {
        // The handler needs the content node to sync the checkmark's opacity. If it is
        // not there, it must leave the *state* alone too — a flipped flag with no visual
        // update is a checkbox that renders the opposite of what it reports.
        let (styled, state) = laid_out(CheckBox::create(false));

        // The checkmark (node 1) is a leaf: it has no first child.
        let (update, changes) = click(styled, &state, node(CONTENT));

        assert!(matches!(update, Update::DoNothing));
        assert!(changes.is_empty(), "a change was pushed for a node the handler could not find");
        assert!(
            !is_checked(&state),
            "the checked flag was flipped even though the checkmark could not be updated",
        );
    }

    #[test]
    fn clicking_a_node_that_is_not_in_the_layout_does_not_panic_or_toggle() {
        // Stale hit ids reach callbacks after a DOM mutation. `set_css_property` *panics*
        // on a None node id, so the handler has to bail out before that point.
        // usize::MAX is unencodable by NodeId's 1-based scheme and would overflow while
        // building this fixture, before `click()` is even called. usize::MAX - 1 is the
        // repo's MAX_ENCODABLE_NODE and still absent from the layout.
        for hit in [node(99), node(usize::MAX - 1), node_none()] {
            let (styled, state) = laid_out(CheckBox::create(true));
            let (update, changes) = click(styled, &state, hit);

            assert!(matches!(update, Update::DoNothing), "{hit:?}: a stale hit was acted on");
            assert!(changes.is_empty(), "{hit:?}: a stale hit pushed a DOM change");
            assert!(is_checked(&state), "{hit:?}: a stale hit toggled the checkbox");
        }
    }

    #[test]
    fn the_toggle_callback_sees_the_new_state_and_its_verdict_is_forwarded() {
        // Order matters: the flag is flipped *before* the user callback runs, so the
        // callback observes the state the user just asked for — not the stale one.
        let probe = log_refany();
        let (styled, state) = laid_out(
            CheckBox::create(false)
                .with_on_toggle(probe.clone(), record_toggle as CheckBoxOnToggleCallbackType),
        );

        let (update, changes) = click(styled, &state, node(CONTAINER));

        assert_eq!(
            read_log(&probe).seen,
            vec![true],
            "the toggle callback was not called exactly once with the NEW state",
        );
        assert!(
            matches!(update, Update::RefreshDom),
            "the user callback's Update was swallowed instead of forwarded",
        );
        // ... and the opacity sync still happens *after* the user callback returns.
        assert_eq!(pushed_opacities(&changes), vec![(NodeId::new(CONTENT), 1.0)]);
    }

    #[test]
    fn the_toggle_callback_receives_the_user_payload_not_the_widget_state() {
        let probe = log_refany();
        let (styled, state) = laid_out(
            CheckBox::create(true)
                .with_on_toggle(probe.clone(), record_toggle as CheckBoxOnToggleCallbackType),
        );

        click(styled, &state, node(CONTAINER));

        let log = read_log(&probe);
        assert_eq!(
            log.payload, 0xDEAD_BEEF,
            "the callback was handed something other than the user's own RefAny",
        );
        // create(true) -> clicked once -> the callback must have seen `false`.
        assert_eq!(log.seen, vec![false]);
    }

    #[test]
    fn a_toggle_callback_that_declines_the_update_still_gets_the_checkmark_synced() {
        // A user callback returning DoNothing must not suppress the widget's own visual
        // bookkeeping — otherwise the flag says "checked" and the mark stays invisible.
        let (styled, state) = laid_out(
            CheckBox::create(false)
                .with_on_toggle(RefAny::new(0u8), toggle_do_nothing as CheckBoxOnToggleCallbackType),
        );

        let (update, changes) = click(styled, &state, node(CONTAINER));

        assert!(matches!(update, Update::DoNothing));
        assert!(is_checked(&state));
        assert_eq!(
            pushed_opacities(&changes),
            vec![(NodeId::new(CONTENT), 1.0)],
            "a DoNothing user callback suppressed the checkmark update",
        );
    }

    #[test]
    fn a_toggle_callback_on_a_childless_node_is_never_called_at_all() {
        // The bail-out happens before the flip *and* before the user callback: a click
        // that cannot be rendered must not be reported to the app either.
        let probe = log_refany();
        let (styled, state) = laid_out(
            CheckBox::create(false)
                .with_on_toggle(probe.clone(), record_toggle as CheckBoxOnToggleCallbackType),
        );

        let (update, changes) = click(styled, &state, node(CONTENT));

        assert!(matches!(update, Update::DoNothing));
        assert!(changes.is_empty());
        assert!(!is_checked(&state));
        assert!(
            read_log(&probe).seen.is_empty(),
            "the user was notified of a toggle that never happened",
        );
    }

    #[test]
    fn many_clicks_leave_the_state_and_the_pushed_opacity_in_agreement() {
        // 101 clicks starting unchecked -> checked. Every push must agree with the flag
        // it accompanies; a drift between the two is exactly the class of bug that makes
        // a checkbox render inverted after a while.
        let mut state_after = false;
        let start = CheckBox::create(false);
        let (_, state) = laid_out(start);

        for i in 0..101u32 {
            let (styled, _) = laid_out(CheckBox::create(false));
            let (_, changes) = click(styled, &state, node(CONTAINER));
            state_after = !state_after;

            let expected = if state_after { 1.0 } else { 0.0 };
            assert_eq!(
                pushed_opacities(&changes),
                vec![(NodeId::new(CONTENT), expected)],
                "click #{i}: the pushed opacity disagrees with the checked flag",
            );
            assert_eq!(is_checked(&state), state_after, "click #{i}: the flag drifted");
        }

        assert!(is_checked(&state), "an odd number of clicks left the checkbox unchecked");
    }
}
