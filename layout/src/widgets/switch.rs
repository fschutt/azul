//! Switch (toggle) widget — a boolean on/off control rendered as a rounded,
//! pill-shaped "track" with a sliding circular "knob". A near-clone of
//! [`crate::widgets::check_box::CheckBox`] (boolean state + an `on_toggle`
//! callback) restyled as a switch: toggling flips the knob's horizontal
//! position (via `margin-left`) and the track's background colour.
//!
//! Key types: [`Switch`], [`SwitchState`], [`SwitchOnToggle`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    impl_option_inner,
    props::{
        basic::{color::ColorU, *},
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutHeight, LayoutMarginLeft, LayoutPaddingBottom, LayoutPaddingLeft,
            LayoutPaddingRight, LayoutPaddingTop, LayoutWidth,
        },
        property::{CssProperty, *},
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleCursor,
        },
    },
    AzString, OptionString,
};

use crate::callbacks::{Callback, CallbackInfo};

static SWITCH_TRACK_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-switch"))];
static SWITCH_KNOB_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-switch-knob"))];

/// Callback function type invoked when the switch is toggled.
pub type SwitchOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, SwitchState) -> Update;
impl_widget_callback!(
    SwitchOnToggle,
    OptionSwitchOnToggle,
    SwitchOnToggleCallback,
    SwitchOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SwitchOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SWITCH_ON_TOGGLE_INVOKER,
    invoker_ty:     AzSwitchOnToggleCallbackInvoker,
    thunk_fn:       az_switch_on_toggle_callback_thunk,
    setter_fn:      AzApp_setSwitchOnToggleCallbackInvoker,
    from_handle_fn: AzSwitchOnToggleCallback_createFromHostHandle,
    extra_args:     [ state: SwitchState ],
}

/// A toggleable on/off switch widget with a sliding knob and toggle callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Switch {
    pub switch_state: SwitchStateWrapper,
    /// Style for the switch track (the pill-shaped container)
    pub track_style: CssPropertyWithConditionsVec,
    /// Style for the sliding knob
    pub knob_style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET rather than patched onto the finished `Dom`: that
    /// is what lets the widget know at build time whether it was named, so its
    /// warning fires only when nobody supplied one. Forwarded into the
    /// accessibility declaration the widget builds anyway, beside its role and
    /// state.
    pub accessibility_name: OptionString,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchStateWrapper {
    /// On/off state of this Switch
    pub inner: SwitchState,
    /// Optional: function to call when the Switch is toggled
    pub on_toggle: OptionSwitchOnToggle,
}

/// The on/off state of a [`Switch`].
#[derive(Copy, Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchState {
    /// `true` = on (knob slid right), `false` = off (knob at left)
    pub checked: bool,
}

// ---- dimensions ----
const TRACK_WIDTH: isize = 36;
const TRACK_HEIGHT: isize = 20;
const TRACK_PADDING: isize = 2;
const TRACK_RADIUS: isize = 10;
const KNOB_SIZE: isize = 16;
const KNOB_RADIUS: isize = 8;
/// Horizontal travel of the knob = `track_width` − 2·padding − `knob_size`.
const KNOB_TRAVEL: isize = TRACK_WIDTH - (2 * TRACK_PADDING) - KNOB_SIZE;

// ---- colours ----
const TRACK_OFF_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
}; // #cccccc
const TRACK_ON_COLOR: ColorU = ColorU {
    r: 76,
    g: 217,
    b: 100,
    a: 255,
}; // #4cd964
const KNOB_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white

const TRACK_OFF_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRACK_OFF_COLOR)];
const TRACK_OFF_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRACK_OFF_BG_ITEMS);
const TRACK_ON_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRACK_ON_COLOR)];
const TRACK_ON_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRACK_ON_BG_ITEMS);
const KNOB_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(KNOB_COLOR)];
const KNOB_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(KNOB_BG_ITEMS);

/// Build the track (pill container) style. Background colour is the only
/// state-dependent property, so the style is built at runtime per the recipe's
/// "runtime vec if param-dependent" path.
fn build_track_style(checked: bool) -> CssPropertyWithConditionsVec {
    let bg = if checked { TRACK_ON_BG } else { TRACK_OFF_BG };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            TRACK_WIDTH,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            TRACK_HEIGHT,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg)),
    ])
}

/// Build the knob style. The knob's `margin-left` is the state-dependent
/// property that slides it between the off (left) and on (right) positions.
fn build_knob_style(checked: bool) -> CssPropertyWithConditionsVec {
    let margin = if checked { KNOB_TRAVEL } else { 0 };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            KNOB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            KNOB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(KNOB_BG)),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(margin),
        )),
    ])
}

impl Switch {
    /// Creates a new switch in the given on/off state with default styling.
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use]
    pub fn create(checked: bool) -> Self {
        Self {
            switch_state: SwitchStateWrapper {
                inner: SwitchState { checked },
                ..Default::default()
            },
            track_style: build_track_style(checked),
            knob_style: build_knob_style(checked),
            accessibility_name: OptionString::None,
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
    pub fn set_on_toggle<C: Into<SwitchOnToggleCallback>>(&mut self, data: RefAny, on_toggle: C) {
        self.switch_state.on_toggle = Some(SwitchOnToggle {
            callback: on_toggle.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_toggle<C: Into<SwitchOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        // Read before the widget's fields are moved into the DOM below.
        let sw_name = self.accessibility_name.clone();
        crate::widgets::warn_widget_needs_a_name("switch", sw_name.is_some());

        // Read before the wrapper is moved into the callback below.
        let switch_checked = self.switch_state.inner.checked;

        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{Dom, EventFilter, HoverEventFilter},
        };

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from(SWITCH_TRACK_CLASS))
            .with_css_props(self.track_style)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: input::default_on_switch_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(self.switch_state),
                }]
                .into(),
            )
            .with_tab_index(TabIndex::Auto)
            // A switch announces as a checkbox with a state. Publishing it on
            // every build (not once at construction) is what keeps the spoken
            // state in step with the rendered one.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::CheckButton,
                accessibility_name: sw_name,
                states: azul_core::a11y::AccessibilityStateVec::from_vec(vec![
                    if switch_checked {
                        azul_core::a11y::AccessibilityState::CheckedTrue
                    } else {
                        azul_core::a11y::AccessibilityState::CheckedFalse
                    },
                ]),
                ..Default::default()
            })
            .with_children(
                vec![Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from(SWITCH_KNOB_CLASS))
                    .with_css_props(self.knob_style)]
                .into(),
            )
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::create(false)
    }
}

// handle input events for the switch
mod input {

    use azul_core::{callbacks::Update, refany::RefAny};
    use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty};

    use super::{SwitchOnToggle, SwitchStateWrapper, KNOB_TRAVEL, TRACK_OFF_BG, TRACK_ON_BG};
    use crate::callbacks::CallbackInfo;

    pub(super) extern "C" fn default_on_switch_clicked(
        mut switch: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        let Some(mut switch) = switch.downcast_mut::<SwitchStateWrapper>() else {
            return Update::DoNothing;
        };

        let track_id = info.get_hit_node();
        let Some(knob_id) = info.get_first_child(track_id) else {
            return Update::DoNothing;
        };

        switch.inner.checked = !switch.inner.checked;

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let switch = &mut *switch;
            let on_toggle = &mut switch.on_toggle;
            let inner = switch.inner;

            match on_toggle.as_mut() {
                Some(SwitchOnToggle {
                    callback,
                    refany: data,
                }) => (callback.cb)(data.clone(), info, inner),
                None => Update::DoNothing,
            }
        };

        // The ANNOUNCED state must follow the rendered one. This handler flips
        // css properties and returns Update::DoNothing — no rebuild — so the
        // CheckedTrue/False published at build time would freeze at whatever it
        // was then, and a screen reader would keep reporting the old position.
        info.set_accessibility_state(
            info.get_hit_node(),
            azul_core::a11y::AccessibilityStateVec::from_vec(vec![if switch.inner.checked {
                azul_core::a11y::AccessibilityState::CheckedTrue
            } else {
                azul_core::a11y::AccessibilityState::CheckedFalse
            }]),
        );

        // CallbackInfo is Copy, so `info` is still usable after the call above.
        if switch.inner.checked {
            info.set_css_property(track_id, CssProperty::const_background_content(TRACK_ON_BG));
            info.set_css_property(
                knob_id,
                CssProperty::const_margin_left(LayoutMarginLeft::const_px(KNOB_TRAVEL)),
            );
        } else {
            info.set_css_property(
                track_id,
                CssProperty::const_background_content(TRACK_OFF_BG),
            );
            info.set_css_property(
                knob_id,
                CssProperty::const_margin_left(LayoutMarginLeft::const_px(0)),
            );
        }

        result
    }
}

impl From<Switch> for Dom {
    fn from(s: Switch) -> Self {
        s.dom()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
// every float here is an exact, integral px constant
// `assertions_on_constants`: these are deliberate invariant guards over sibling
// `const`s in this module. They are const-foldable *today*, which is exactly the
// point — they must go red the moment someone edits one of those constants into an
// inconsistent value. Deleting them (clippy's suggestion) would delete the check.
#[allow(clippy::assertions_on_constants)]
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
    // Geometry, spelled out independently of the module's own constants
    // ------------------------------------------------------------------
    //
    // These literals are deliberately *not* derived from `TRACK_WIDTH` & friends:
    // they are the numbers a designer signed off on. If a constant upstream drifts,
    // the relation tests below fail instead of silently re-deriving themselves.

    const TRACK_W: f32 = 36.0;
    const TRACK_H: f32 = 20.0;
    const PAD: f32 = 2.0;
    const TRACK_R: f32 = 10.0;
    const KNOB: f32 = 16.0;
    const KNOB_R: f32 = 8.0;
    /// `36 - 2*2 - 16`
    const TRAVEL: f32 = 16.0;

    /// Flattened node ids of `Switch::dom()` (pre-order).
    const TRACK: usize = 0;
    const KNOB_NODE: usize = 1;

    // ------------------------------------------------------------------
    // Callback harness
    // ------------------------------------------------------------------

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

    /// A `DomLayoutResult` carrying only a `styled_dom`: the switch handler reaches
    /// exactly two `CallbackInfo` queries (`get_hit_node`, `get_first_child`), and both
    /// read the node hierarchy only — no real layout (and no font) is needed.
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

    /// Renders `switch`, then hands back both the laid-out DOM *and* the very `RefAny`
    /// the widget registered on its own mouse-up callback. Driving the handler with
    /// these two is the real wiring — nothing is re-created by hand, so a mismatch
    /// between what `dom()` stores and what the handler expects cannot hide behind the
    /// fixture.
    fn laid_out(switch: Switch) -> (StyledDom, RefAny) {
        let dom = switch.dom();
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
            input::default_on_switch_clicked(state.clone(), *info)
        })
    }

    fn is_checked(state: &RefAny) -> bool {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<SwitchStateWrapper>()
            .expect("the widget state changed type");
        wrapper.inner.checked
    }

    /// Every `(node, property)` pair the handler wrote, flattened and in push order.
    fn pushed_pairs(changes: &[CallbackChange]) -> Vec<(NodeId, CssProperty)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => Some((*node_id, properties.as_ref().to_vec())),
                _ => None,
            })
            .flat_map(|(n, ps)| ps.into_iter().map(move |p| (n, p)))
            .collect()
    }

    fn pushed_backgrounds(changes: &[CallbackChange]) -> Vec<(NodeId, StyleBackgroundContentVec)> {
        pushed_pairs(changes)
            .into_iter()
            .filter_map(|(n, p)| match p {
                CssProperty::BackgroundContent(b) => b.get_property().cloned().map(|b| (n, b)),
                _ => None,
            })
            .collect()
    }

    fn pushed_margins(changes: &[CallbackChange]) -> Vec<(NodeId, f32)> {
        pushed_pairs(changes)
            .into_iter()
            .filter_map(|(n, p)| match p {
                CssProperty::MarginLeft(m) => m.get_property().map(|m| (n, px(&m.inner))),
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

    fn find<T>(
        v: &CssPropertyWithConditionsVec,
        f: impl Fn(&CssProperty) -> Option<T>,
    ) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An `em`
    /// or `%` slipping into the switch geometry would resolve against the parent
    /// font/box, so a 36px track could render at any size at all — and the knob's
    /// travel would no longer line up with it.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "switch geometry must be absolute px, got {:?}",
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

    /// The knob's `margin-left` — the single property that encodes "which side is the
    /// knob on". This is the only thing distinguishing the two knob styles.
    fn margin_left_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::MarginLeft(m) => m.get_property().map(|m| px(&m.inner)),
            _ => None,
        })
    }

    /// `(top, right, bottom, left)` padding, each as an absolute px.
    fn paddings_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let get = |f: fn(&CssProperty) -> Option<f32>| find(v, f);
        (
            get(|p| match p {
                CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            get(|p| match p {
                CssProperty::PaddingRight(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            get(|p| match p {
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            get(|p| match p {
                CssProperty::PaddingLeft(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    /// The four corner radii, in declaration order. (Each corner is its own newtype, so
    /// the arms cannot be collapsed into an or-pattern.)
    fn radii_px(v: &CssPropertyWithConditionsVec) -> Vec<f32> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            })
            .collect()
    }

    fn flex_grow(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::FlexGrow(g) => g.get_property().map(|g| g.inner.get()),
            _ => None,
        })
    }

    fn background(v: &CssPropertyWithConditionsVec) -> Option<StyleBackgroundContentVec> {
        find(v, |p| match p {
            CssProperty::BackgroundContent(b) => b.get_property().cloned(),
            _ => None,
        })
    }

    /// The solid-colour layers of a background, in order. A gradient/image layer is
    /// dropped, so `solid_colors(bg).len() != bg.len()` means something non-solid crept in.
    fn solid_colors(bg: &StyleBackgroundContentVec) -> Vec<ColorU> {
        bg.as_ref()
            .iter()
            .filter_map(|c| match c {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Every absolute-px number declared by a style vec.
    fn px_values(v: &CssPropertyWithConditionsVec) -> Vec<f32> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Width(x) => match x.get_property() {
                    Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                    _ => None,
                },
                CssProperty::Height(x) => match x.get_property() {
                    Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                    _ => None,
                },
                CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::PaddingRight(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::PaddingLeft(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::MarginLeft(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::BorderTopLeftRadius(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::BorderTopRightRadius(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::BorderBottomLeftRadius(x) => x.get_property().map(|x| px(&x.inner)),
                CssProperty::BorderBottomRightRadius(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            })
            .collect()
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
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
    /// argument — a *shared* clone of what the test still holds — so the test can read
    /// back exactly what the widget passed, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ToggleLog {
        seen: Vec<bool>,
        payload: u32,
    }

    extern "C" fn record_toggle(
        mut data: RefAny,
        _info: CallbackInfo,
        state: SwitchState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.seen.push(state.checked);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: SwitchState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn toggle_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: SwitchState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in, which the
    /// `From<Callback>` arm *transmutes* into the 3-arg switch slot. Never called.
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
    // Geometry constants — numeric limits / relations
    // ==================================================================

    #[test]
    fn knob_travel_matches_its_documented_formula_and_is_positive() {
        // The doc comment on KNOB_TRAVEL *is* the spec. A negative travel (knob wider
        // than the track's content box) would slide the knob left, off the widget.
        assert_eq!(
            KNOB_TRAVEL,
            TRACK_WIDTH - (2 * TRACK_PADDING) - KNOB_SIZE,
            "KNOB_TRAVEL no longer matches its own documented formula",
        );
        assert!(
            KNOB_TRAVEL > 0,
            "the knob has no room to travel: KNOB_TRAVEL = {KNOB_TRAVEL}",
        );
        assert_eq!(KNOB_TRAVEL as f32, TRAVEL);
    }

    #[test]
    fn the_knob_exactly_fills_the_track_from_edge_to_edge_when_on() {
        // padding + travel + knob + padding == track width. One px of drift either way
        // and the "on" knob either overhangs the pill or leaves a visible gap.
        assert_eq!(
            TRACK_PADDING + KNOB_TRAVEL + KNOB_SIZE + TRACK_PADDING,
            TRACK_WIDTH,
            "the switched-on knob does not sit flush against the track's right padding edge",
        );
    }

    #[test]
    fn the_knob_exactly_fills_the_tracks_vertical_padding_box() {
        // A knob taller than `track_height - 2*padding` overflows the pill vertically;
        // a shorter one floats. 16 == 20 - 2*2.
        assert_eq!(
            KNOB_SIZE,
            TRACK_HEIGHT - (2 * TRACK_PADDING),
            "the knob no longer fits the track's vertical padding box",
        );
    }

    #[test]
    fn both_shapes_are_fully_round_not_merely_rounded() {
        // radius == half the cross-axis extent is what makes a pill a pill and a knob a
        // circle. Anything less renders a rounded rectangle.
        assert_eq!(TRACK_RADIUS * 2, TRACK_HEIGHT, "the track is not a pill");
        assert_eq!(KNOB_RADIUS * 2, KNOB_SIZE, "the knob is not a circle");
    }

    #[test]
    fn every_geometry_constant_survives_floatvalues_fixed_point_encoding() {
        // `FloatValue::const_new` multiplies by 1000 in *isize* arithmetic with no
        // checked path — a constant near isize::MAX/1000 would wrap silently and produce
        // a nonsense length. Assert every constant is comfortably representable, and
        // that the encode/decode round-trips exactly.
        const MULT: isize = 1000;
        for (name, v) in [
            ("TRACK_WIDTH", TRACK_WIDTH),
            ("TRACK_HEIGHT", TRACK_HEIGHT),
            ("TRACK_PADDING", TRACK_PADDING),
            ("TRACK_RADIUS", TRACK_RADIUS),
            ("KNOB_SIZE", KNOB_SIZE),
            ("KNOB_RADIUS", KNOB_RADIUS),
            ("KNOB_TRAVEL", KNOB_TRAVEL),
        ] {
            assert!(
                v.checked_mul(MULT).is_some(),
                "{name} = {v} overflows FloatValue's fixed-point encoding",
            );
            assert!(v >= 0, "{name} = {v} is negative");

            // encode -> decode must be lossless for these integral px values.
            let encoded = LayoutMarginLeft::const_px(v);
            assert_eq!(
                px(&encoded.inner),
                v as f32,
                "{name} = {v} does not round-trip through PixelValue",
            );
        }
    }

    #[test]
    fn no_declared_px_value_is_nan_infinite_or_negative() {
        for checked in [false, true] {
            for (name, v) in [
                ("track", build_track_style(checked)),
                ("knob", build_knob_style(checked)),
            ] {
                let values = px_values(&v);
                assert!(
                    !values.is_empty(),
                    "checked={checked}: the {name} style declares no px lengths at all",
                );
                for x in values {
                    assert!(
                        x.is_finite(),
                        "checked={checked}: the {name} style declares a non-finite length {x}",
                    );
                    assert!(
                        x >= 0.0,
                        "checked={checked}: the {name} style declares a negative length {x}",
                    );
                }
            }
        }
    }

    // ==================================================================
    // build_track_style
    // ==================================================================

    #[test]
    fn build_track_style_is_pure() {
        for checked in [false, true] {
            assert_eq!(
                properties(&build_track_style(checked)),
                properties(&build_track_style(checked)),
                "build_track_style({checked}) is not deterministic",
            );
        }
    }

    #[test]
    fn build_track_style_differs_between_the_two_states_only_in_the_background() {
        // Everything but the colour must be byte-for-byte identical: a track that
        // changed size or radius when flipped would reflow its neighbours mid-animation.
        let on = properties(&build_track_style(true));
        let off = properties(&build_track_style(false));
        assert_eq!(
            on.len(),
            off.len(),
            "the two track styles declare a different number of properties",
        );

        let differing: Vec<_> = on
            .iter()
            .zip(off.iter())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| discriminant(a))
            .collect();
        assert_eq!(
            differing,
            vec![discriminant(&CssProperty::const_background_content(
                TRACK_ON_BG
            ))],
            "the on/off track styles differ in something other than the background",
        );
    }

    #[test]
    fn build_track_style_maps_on_to_green_and_off_to_grey() {
        // A swapped branch here yields a switch that reads as "on" when it is off —
        // which still type-checks and still animates.
        let on = background(&build_track_style(true)).expect("the on track has no background");
        let off = background(&build_track_style(false)).expect("the off track has no background");

        assert_eq!(solid_colors(&on), vec![TRACK_ON_COLOR]);
        assert_eq!(solid_colors(&off), vec![TRACK_OFF_COLOR]);
        assert_ne!(
            solid_colors(&on),
            solid_colors(&off),
            "the on and off tracks are the same colour — the switch has no visible state",
        );
    }

    #[test]
    fn the_track_background_is_exactly_one_fully_opaque_solid_layer() {
        // A translucent (or multi-layer, or gradient) track would let whatever is behind
        // it show through, so "off grey" would not actually be grey.
        for checked in [false, true] {
            let bg = background(&build_track_style(checked)).expect("no background");
            assert_eq!(
                bg.as_ref().len(),
                1,
                "checked={checked}: the track stacks {} background layers",
                bg.as_ref().len(),
            );
            let colors = solid_colors(&bg);
            assert_eq!(
                colors.len(),
                1,
                "checked={checked}: the track background is not a plain solid colour",
            );
            assert_eq!(
                colors[0].a, 255,
                "checked={checked}: the track background is translucent (a = {})",
                colors[0].a,
            );
        }
    }

    #[test]
    fn build_track_style_geometry_is_absolute_px_in_both_states() {
        for checked in [false, true] {
            let v = build_track_style(checked);
            // `px()` asserts SizeMetric::Px — an em/% here would scale with the parent
            // and desynchronise the knob's (absolute px) travel from the track.
            assert_eq!(
                width_px(&v),
                Some(TRACK_W),
                "checked={checked}: track width"
            );
            assert_eq!(
                height_px(&v),
                Some(TRACK_H),
                "checked={checked}: track height"
            );
            assert_eq!(
                paddings_px(&v),
                (Some(PAD), Some(PAD), Some(PAD), Some(PAD)),
                "checked={checked}: the track padding is not uniform",
            );
            assert_eq!(
                radii_px(&v),
                vec![TRACK_R; 4],
                "checked={checked}: the track's four corners are not all {TRACK_R}px",
            );
        }
    }

    #[test]
    fn build_track_style_declares_no_property_twice() {
        // A duplicate declaration means the later one silently wins — a latent
        // "why is my override ignored" bug that never surfaces as an error.
        for checked in [false, true] {
            let props = properties(&build_track_style(checked));
            let mut seen = Vec::new();
            for p in &props {
                let d = discriminant(p);
                assert!(
                    !seen.contains(&d),
                    "checked={checked}: the track style declares {p:?} twice",
                );
                seen.push(d);
            }
        }
    }

    #[test]
    fn build_track_style_marks_the_track_as_clickable_and_non_growing() {
        for checked in [false, true] {
            let v = build_track_style(checked);
            let cursor = find(&v, |p| match p {
                CssProperty::Cursor(c) => c.get_property().copied(),
                _ => None,
            });
            assert_eq!(
                cursor,
                Some(StyleCursor::Pointer),
                "checked={checked}: the switch does not present as clickable",
            );
            // flex-grow > 0 would let the track stretch past 36px inside a flex row,
            // and the knob's fixed 16px travel would no longer reach the right edge.
            assert_eq!(
                flex_grow(&v),
                Some(0.0),
                "checked={checked}: the track is allowed to grow",
            );
        }
    }

    #[test]
    fn build_track_style_lays_the_knob_out_as_a_centred_row() {
        // The knob is positioned by `margin-left` alone, which only behaves as a
        // left-anchored offset inside a row flex container.
        for checked in [false, true] {
            let v = build_track_style(checked);
            assert_eq!(
                find(&v, |p| match p {
                    CssProperty::Display(d) => d.get_property().copied(),
                    _ => None,
                }),
                Some(LayoutDisplay::Flex),
            );
            assert_eq!(
                find(&v, |p| match p {
                    CssProperty::FlexDirection(d) => d.get_property().copied(),
                    _ => None,
                }),
                Some(LayoutFlexDirection::Row),
                "checked={checked}: margin-left only slides the knob in a row container",
            );
            assert_eq!(
                find(&v, |p| match p {
                    CssProperty::AlignItems(a) => a.get_property().copied(),
                    _ => None,
                }),
                Some(LayoutAlignItems::Center),
            );
        }
    }

    // ==================================================================
    // build_knob_style
    // ==================================================================

    #[test]
    fn build_knob_style_is_pure() {
        for checked in [false, true] {
            assert_eq!(
                properties(&build_knob_style(checked)),
                properties(&build_knob_style(checked)),
                "build_knob_style({checked}) is not deterministic",
            );
        }
    }

    #[test]
    fn build_knob_style_differs_between_the_two_states_only_in_margin_left() {
        let on = properties(&build_knob_style(true));
        let off = properties(&build_knob_style(false));
        assert_eq!(
            on.len(),
            off.len(),
            "the two knob styles declare a different number of properties",
        );

        let differing: Vec<_> = on
            .iter()
            .zip(off.iter())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| discriminant(a))
            .collect();
        assert_eq!(
            differing,
            vec![discriminant(&CssProperty::const_margin_left(
                LayoutMarginLeft::const_px(0)
            ))],
            "the on/off knob styles differ in something other than margin-left",
        );
    }

    #[test]
    fn build_knob_style_parks_the_knob_left_when_off_and_right_when_on() {
        assert_eq!(
            margin_left_px(&build_knob_style(false)),
            Some(0.0),
            "the off knob is not flush against the track's left padding edge",
        );
        assert_eq!(
            margin_left_px(&build_knob_style(true)),
            Some(TRAVEL),
            "the on knob does not travel the full width of the track",
        );
    }

    #[test]
    fn build_knob_style_geometry_is_a_circle_in_absolute_px() {
        for checked in [false, true] {
            let v = build_knob_style(checked);
            assert_eq!(width_px(&v), Some(KNOB), "checked={checked}: knob width");
            assert_eq!(height_px(&v), Some(KNOB), "checked={checked}: knob height");
            assert_eq!(
                width_px(&v),
                height_px(&v),
                "checked={checked}: the knob is not square, so it cannot be a circle",
            );
            assert_eq!(
                radii_px(&v),
                vec![KNOB_R; 4],
                "checked={checked}: the knob's four corners are not all {KNOB_R}px",
            );
            assert_eq!(
                flex_grow(&v),
                Some(0.0),
                "checked={checked}: the knob is allowed to grow and would fill the track",
            );
        }
    }

    #[test]
    fn build_knob_style_declares_no_property_twice() {
        // Two `margin-left` declarations would make the knob's position depend on
        // declaration order rather than on `checked`.
        for checked in [false, true] {
            let props = properties(&build_knob_style(checked));
            let mut seen = Vec::new();
            for p in &props {
                let d = discriminant(p);
                assert!(
                    !seen.contains(&d),
                    "checked={checked}: the knob style declares {p:?} twice",
                );
                seen.push(d);
            }
        }
    }

    #[test]
    fn the_knob_is_opaque_white_in_both_states() {
        // The knob must never inherit or blend with the track colour, or the "off" knob
        // would disappear into the grey.
        for checked in [false, true] {
            let bg = background(&build_knob_style(checked)).expect("the knob has no background");
            assert_eq!(
                bg.as_ref().len(),
                1,
                "checked={checked}: the knob stacks layers"
            );
            assert_eq!(solid_colors(&bg), vec![KNOB_COLOR]);
            assert_eq!(
                KNOB_COLOR.a, 255,
                "the knob is translucent and would tint with the track",
            );
            assert_ne!(
                solid_colors(&bg),
                vec![TRACK_OFF_COLOR],
                "checked={checked}: the knob is the same colour as the off track",
            );
            assert_ne!(
                solid_colors(&bg),
                vec![TRACK_ON_COLOR],
                "checked={checked}: the knob is the same colour as the on track",
            );
        }
    }

    #[test]
    fn the_knob_never_leaves_the_track_in_either_state() {
        // The real safety property, read back out of the *built styles* rather than the
        // constants: left edge >= 0 and right edge <= the track's content width.
        let content_w = TRACK_W - 2.0 * PAD;
        for checked in [false, true] {
            let margin = margin_left_px(&build_knob_style(checked)).expect("no margin-left");
            let size = width_px(&build_knob_style(checked)).expect("no width");

            assert!(
                margin >= 0.0,
                "checked={checked}: the knob is pushed off the left edge"
            );
            assert!(
                margin + size <= content_w,
                "checked={checked}: the knob overhangs the track ({margin} + {size} > {content_w})",
            );
            assert!(
                height_px(&build_knob_style(checked)).expect("no height") <= TRACK_H - 2.0 * PAD,
                "checked={checked}: the knob overflows the track vertically",
            );
        }
    }

    // ==================================================================
    // Switch::create / Default
    // ==================================================================

    #[test]
    fn create_stores_the_flag_and_installs_no_callback() {
        for checked in [false, true] {
            let s = Switch::create(checked);
            assert_eq!(
                s.switch_state.inner.checked, checked,
                "create({checked}) did not store the flag it was given",
            );
            assert!(
                s.switch_state.on_toggle.as_ref().is_none(),
                "create({checked}) invented a toggle callback out of nowhere",
            );
        }
    }

    #[test]
    fn create_is_pure_and_its_two_states_are_distinguishable() {
        assert_eq!(Switch::create(true), Switch::create(true));
        assert_eq!(Switch::create(false), Switch::create(false));
        assert_ne!(
            Switch::create(true),
            Switch::create(false),
            "an on and an off switch are indistinguishable",
        );
    }

    #[test]
    fn create_wires_the_flag_through_to_both_style_builders() {
        // The one way `create` can be wrong without any test noticing: passing the flag
        // to one builder and a literal (or the negation) to the other, so the track says
        // "on" while the knob sits left.
        for checked in [false, true] {
            let s = Switch::create(checked);
            assert_eq!(
                properties(&s.track_style),
                properties(&build_track_style(checked)),
                "create({checked}) did not build the track for state {checked}",
            );
            assert_eq!(
                properties(&s.knob_style),
                properties(&build_knob_style(checked)),
                "create({checked}) did not build the knob for state {checked}",
            );
        }
    }

    #[test]
    fn the_rendered_colour_and_the_knob_position_always_agree_with_the_stored_flag() {
        for checked in [false, true] {
            let s = Switch::create(checked);
            let bg = background(&s.track_style).expect("no track background");
            let margin = margin_left_px(&s.knob_style).expect("no knob margin");

            let (expected_color, expected_margin) = if s.switch_state.inner.checked {
                (TRACK_ON_COLOR, TRAVEL)
            } else {
                (TRACK_OFF_COLOR, 0.0)
            };
            assert_eq!(solid_colors(&bg), vec![expected_color]);
            assert_eq!(
                margin, expected_margin,
                "checked={checked}: the knob position contradicts the track colour",
            );
        }
    }

    #[test]
    fn default_is_an_off_switch() {
        assert_eq!(Switch::default(), Switch::create(false));
        assert!(!Switch::default().switch_state.inner.checked);
        assert!(
            !SwitchState::default().checked,
            "the default SwitchState is not off"
        );
        assert!(!SwitchStateWrapper::default().inner.checked);
        assert!(SwitchStateWrapper::default().on_toggle.as_ref().is_none());
    }

    // ==================================================================
    // Switch::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_an_off_switch_behind() {
        let mut s = Switch::create(true);
        let old = s.swap_with_default();

        assert_eq!(
            old,
            Switch::create(true),
            "the old widget was not returned intact"
        );
        assert_eq!(
            s,
            Switch::create(false),
            "what was left behind is not a fresh off switch"
        );
    }

    #[test]
    fn swap_with_default_on_an_already_default_widget_is_a_no_op() {
        let mut s = Switch::create(false);
        let old = s.swap_with_default();
        assert_eq!(
            old, s,
            "swapping a default with a default produced two different widgets"
        );
        assert_eq!(old, Switch::create(false));
    }

    #[test]
    fn swapping_twice_round_trips_the_original_widget() {
        let mut a = Switch::create(true);
        let mut b = a.swap_with_default(); // a = default, b = on
        let c = b.swap_with_default(); // b = default, c = on

        assert_eq!(c, Switch::create(true));
        assert_eq!(a, Switch::create(false));
        assert_eq!(b, Switch::create(false));
    }

    #[test]
    fn swap_with_default_moves_the_toggle_callback_out_rather_than_copying_or_dropping_it() {
        let probe = log_refany();
        let mut s = Switch::create(true)
            .with_on_toggle(probe.clone(), record_toggle as SwitchOnToggleCallbackType);

        let old = s.swap_with_default();

        // The callback (and its payload) left with the returned value ...
        let moved = old
            .switch_state
            .on_toggle
            .as_ref()
            .expect("the toggle callback vanished during the swap");
        assert_eq!(
            moved.callback.cb as *const () as usize,
            record_toggle as SwitchOnToggleCallbackType as *const () as usize,
            "the fn pointer was mangled by the swap",
        );

        // ... and did NOT stay behind: a duplicated callback would fire twice, and a
        // duplicated RefAny would double-free its payload.
        assert!(
            s.switch_state.on_toggle.as_ref().is_none(),
            "the toggle callback was copied instead of moved",
        );

        // The payload is still alive and unchanged after the move.
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);
    }

    // ==================================================================
    // Switch::set_on_toggle / with_on_toggle
    // ==================================================================

    #[test]
    fn set_on_toggle_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut s = Switch::create(false);
        s.set_on_toggle(
            RefAny::new(0xDEAD_BEEF_u32),
            toggle_do_nothing as SwitchOnToggleCallbackType,
        );

        let t = s
            .switch_state
            .on_toggle
            .as_ref()
            .expect("set_on_toggle did not store anything");
        assert_eq!(
            t.callback.cb as *const () as usize,
            toggle_do_nothing as SwitchOnToggleCallbackType as *const () as usize,
            "the fn pointer was corrupted on the way in",
        );

        let mut data = t.refany.clone();
        assert_eq!(
            *data
                .downcast_ref::<u32>()
                .expect("the payload changed type"),
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
        // `OptionSwitchOnToggle` is a single slot; setting twice must leave the *second*
        // callback installed (and must not leak or free the first one's RefAny).
        let first = log_refany();
        let mut s = Switch::create(false);
        s.set_on_toggle(
            first.clone(),
            toggle_do_nothing as SwitchOnToggleCallbackType,
        );
        s.set_on_toggle(
            RefAny::new(1u8),
            toggle_refresh_all as SwitchOnToggleCallbackType,
        );

        let t = s
            .switch_state
            .on_toggle
            .as_ref()
            .expect("the callback vanished");
        assert_eq!(
            t.callback.cb as *const () as usize,
            toggle_refresh_all as SwitchOnToggleCallbackType as *const () as usize,
            "the second set_on_toggle did not win",
        );
        // The displaced payload is still a valid, readable RefAny (not freed twice).
        assert_eq!(read_log(&first).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn set_on_toggle_does_not_disturb_the_state_or_the_styles() {
        for checked in [false, true] {
            let pristine = Switch::create(checked);
            let mut s = Switch::create(checked);
            s.set_on_toggle(
                RefAny::new(0u8),
                toggle_do_nothing as SwitchOnToggleCallbackType,
            );

            assert_eq!(
                s.switch_state.inner.checked, checked,
                "installing a callback flipped the switch",
            );
            assert_eq!(
                properties(&s.track_style),
                properties(&pristine.track_style),
                "installing a callback rewrote the track style",
            );
            assert_eq!(
                properties(&s.knob_style),
                properties(&pristine.knob_style),
                "installing a callback rewrote the knob style",
            );
        }
    }

    #[test]
    fn with_on_toggle_is_exactly_set_on_toggle_in_builder_form() {
        let by_builder = Switch::create(true).with_on_toggle(
            RefAny::new(7u32),
            toggle_do_nothing as SwitchOnToggleCallbackType,
        );

        let mut by_setter = Switch::create(true);
        by_setter.set_on_toggle(
            RefAny::new(7u32),
            toggle_do_nothing as SwitchOnToggleCallbackType,
        );

        assert_eq!(by_builder.switch_state.inner, by_setter.switch_state.inner);
        assert_eq!(
            properties(&by_builder.track_style),
            properties(&by_setter.track_style),
        );
        assert_eq!(
            properties(&by_builder.knob_style),
            properties(&by_setter.knob_style),
        );

        let a = by_builder
            .switch_state
            .on_toggle
            .as_ref()
            .expect("builder lost the callback");
        let b = by_setter
            .switch_state
            .on_toggle
            .as_ref()
            .expect("setter lost the callback");
        assert_eq!(
            a.callback.cb as *const () as usize,
            b.callback.cb as *const () as usize
        );

        let (mut a, mut b) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *a.downcast_ref::<u32>()
                .expect("builder payload changed type"),
            *b.downcast_ref::<u32>()
                .expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_toggle_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg switch
        // slot — this is the FFI (Python/C) path. The pointer must come out bit-identical;
        // a mangled one would be called as a wild jump on the first click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let s = Switch::create(false).with_on_toggle(RefAny::new(0u8), generic);
        let t = s
            .switch_state
            .on_toggle
            .as_ref()
            .expect("the generic callback was dropped");
        assert_eq!(
            t.callback.cb as *const () as usize, expected,
            "the Callback -> SwitchOnToggleCallback transmute mangled the pointer",
        );
    }

    // ==================================================================
    // Switch::dom
    // ==================================================================

    #[test]
    fn dom_builds_a_focusable_track_with_exactly_one_knob_child() {
        for checked in [false, true] {
            let dom = Switch::create(checked).dom();

            assert!(matches!(dom.root.get_node_type(), NodeType::Div));
            assert_eq!(
                dom.root.flags.get_tab_index(),
                Some(TabIndex::Auto),
                "checked={checked}: the switch is not keyboard-focusable",
            );
            assert_eq!(classes(&dom), vec!["__azul-native-switch".to_string()]);

            let children = dom.children.as_ref();
            assert_eq!(
                children.len(),
                1,
                "checked={checked}: the switch must have exactly one knob"
            );
            assert_eq!(
                classes(&children[0]),
                vec!["__azul-native-switch-knob".to_string()],
                "checked={checked}: the knob carries the wrong class (external CSS would miss it)",
            );
            assert!(
                children[0].children.as_ref().is_empty(),
                "checked={checked}: the knob grew children",
            );
            assert_eq!(
                children[0].root.flags.get_tab_index(),
                None,
                "checked={checked}: the knob is separately focusable, so Tab stops twice",
            );
        }
    }

    #[test]
    fn dom_puts_the_track_style_on_the_track_and_the_knob_style_on_the_knob() {
        // Swapping the two would style the 16px knob like a 36px pill (and vice versa) —
        // the widget would still render, just wrong.
        for checked in [false, true] {
            let s = Switch::create(checked);
            let track = properties(&s.track_style);
            let knob = properties(&s.knob_style);

            let dom = s.dom();
            assert_eq!(
                inline_properties(&dom),
                track,
                "checked={checked}: the track style did not land on the track",
            );
            assert_eq!(
                inline_properties(&dom.children.as_ref()[0]),
                knob,
                "checked={checked}: the knob style did not land on the knob",
            );
        }
    }

    #[test]
    fn dom_registers_exactly_one_mouse_up_handler_and_it_is_the_widgets_own() {
        for checked in [false, true] {
            let dom = Switch::create(checked).dom();
            let callbacks = dom.root.callbacks.as_ref();

            assert_eq!(
                callbacks.len(),
                1,
                "checked={checked}: expected exactly one callback"
            );
            assert_eq!(
                callbacks[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "checked={checked}: the switch must toggle on mouse-up",
            );
            assert_eq!(
                callbacks[0].callback.cb,
                input::default_on_switch_clicked as usize,
                "checked={checked}: the registered handler is not default_on_switch_clicked",
            );

            // The knob itself must stay inert — a second handler there would toggle twice
            // per click (the event bubbles).
            assert!(
                dom.children.as_ref()[0].root.callbacks.as_ref().is_empty(),
                "checked={checked}: the knob registered a handler of its own",
            );
        }
    }

    #[test]
    fn dom_hands_the_widget_state_to_the_handler_not_the_user_payload() {
        // `dom()` moves `switch_state` (state + on_toggle + user RefAny) into the
        // callback's RefAny. If it stored the *user's* payload instead, the handler's
        // `downcast_mut::<SwitchStateWrapper>()` would fail and every click would be a
        // silent no-op.
        for checked in [false, true] {
            let dom = Switch::create(checked)
                .with_on_toggle(
                    RefAny::new(9u32),
                    toggle_do_nothing as SwitchOnToggleCallbackType,
                )
                .dom();

            let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
            let wrapper = state
                .downcast_ref::<SwitchStateWrapper>()
                .expect("the handler's RefAny is not a SwitchStateWrapper");

            assert_eq!(
                wrapper.inner.checked, checked,
                "the on/off flag was lost on the way into the DOM",
            );
            assert!(
                wrapper.on_toggle.as_ref().is_some(),
                "the user's toggle callback was lost on the way into the DOM",
            );
        }
    }

    #[test]
    fn dom_of_a_callback_less_switch_still_registers_the_toggle_handler() {
        // The switch must always install its own handler: the knob has to slide even
        // with no user callback.
        let dom = Switch::create(false).dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);

        let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
        let wrapper = state
            .downcast_ref::<SwitchStateWrapper>()
            .expect("wrong RefAny type");
        assert!(wrapper.on_toggle.as_ref().is_none());
    }

    #[test]
    fn from_switch_for_dom_is_the_same_as_calling_dom() {
        for checked in [false, true] {
            let via_from = Dom::from(Switch::create(checked));
            let via_dom = Switch::create(checked).dom();

            assert_eq!(classes(&via_from), classes(&via_dom));
            assert_eq!(inline_properties(&via_from), inline_properties(&via_dom));
            assert_eq!(
                via_from.children.as_ref().len(),
                via_dom.children.as_ref().len()
            );
            assert_eq!(
                inline_properties(&via_from.children.as_ref()[0]),
                inline_properties(&via_dom.children.as_ref()[0]),
            );
            assert_eq!(
                via_from.root.callbacks.as_ref().len(),
                via_dom.root.callbacks.as_ref().len(),
            );
            assert_eq!(
                via_from.root.flags.get_tab_index(),
                via_dom.root.flags.get_tab_index()
            );
        }
    }

    #[test]
    fn the_rendered_dom_flattens_to_exactly_two_nodes() {
        // `Dom::estimated_total_children` is a *cached* count; if it under-reports, the
        // flatten under-allocates its arenas. Two nodes: track (0), knob (1).
        for checked in [false, true] {
            let styled = StyledDom::create_from_dom(Switch::create(checked).dom());
            assert_eq!(
                styled.node_data.as_ref().len(),
                2,
                "checked={checked}: the switch no longer flattens to a track + a knob",
            );
        }
    }

    // ==================================================================
    // input::default_on_switch_clicked
    // ==================================================================

    #[test]
    fn clicking_an_off_switch_turns_it_on_and_pushes_the_on_visuals() {
        let (styled, state) = laid_out(Switch::create(false));
        assert!(!is_checked(&state));

        let (update, changes) = click(styled, &state, node(TRACK));

        assert!(is_checked(&state), "the click did not turn the switch on");
        assert!(
            matches!(update, Update::DoNothing),
            "with no user callback installed, the handler must report DoNothing",
        );
        assert_eq!(
            pushed_backgrounds(&changes)
                .iter()
                .map(|(n, b)| (*n, solid_colors(b)))
                .collect::<Vec<_>>(),
            vec![(NodeId::new(TRACK), vec![TRACK_ON_COLOR])],
            "turning the switch on did not repaint the *track* green",
        );
        assert_eq!(
            pushed_margins(&changes),
            vec![(NodeId::new(KNOB_NODE), TRAVEL)],
            "turning the switch on did not slide the *knob* right",
        );
    }

    #[test]
    fn clicking_an_on_switch_turns_it_off_and_pushes_the_off_visuals() {
        let (styled, state) = laid_out(Switch::create(true));
        let (_, changes) = click(styled, &state, node(TRACK));

        assert!(!is_checked(&state), "the click did not turn the switch off");
        assert_eq!(
            pushed_backgrounds(&changes)
                .iter()
                .map(|(n, b)| (*n, solid_colors(b)))
                .collect::<Vec<_>>(),
            vec![(NodeId::new(TRACK), vec![TRACK_OFF_COLOR])],
        );
        assert_eq!(
            pushed_margins(&changes),
            vec![(NodeId::new(KNOB_NODE), 0.0)]
        );
    }

    #[test]
    fn a_click_pushes_exactly_what_a_freshly_created_switch_would_render() {
        // The handler re-derives the visuals by hand instead of calling
        // `build_track_style`/`build_knob_style`. That duplication is the bug surface:
        // a colour or a travel distance can drift in one place and not the other, and
        // the switch then renders differently after a click than it did on mount.
        for start in [false, true] {
            let (styled, state) = laid_out(Switch::create(start));
            let (_, changes) = click(styled, &state, node(TRACK));

            let expected = Switch::create(!start);
            assert_eq!(
                pushed_backgrounds(&changes)
                    .into_iter()
                    .map(|(_, b)| b)
                    .collect::<Vec<_>>(),
                vec![background(&expected.track_style).expect("no background")],
                "start={start}: the clicked track colour differs from a freshly built one",
            );
            assert_eq!(
                pushed_margins(&changes)
                    .into_iter()
                    .map(|(_, m)| m)
                    .collect::<Vec<_>>(),
                vec![margin_left_px(&expected.knob_style).expect("no margin")],
                "start={start}: the clicked knob offset differs from a freshly built one",
            );
        }
    }

    #[test]
    fn clicking_twice_returns_to_the_original_state() {
        let (styled, state) = laid_out(Switch::create(false));

        // Two independent deliveries against the same widget state — the styled DOM is
        // rebuilt each time because the harness consumes it, but the RefAny is shared,
        // which is exactly how the real event loop drives it.
        let (styled2, _) = laid_out(Switch::create(false));
        let (_, first) = click(styled, &state, node(TRACK));
        let (_, second) = click(styled2, &state, node(TRACK));

        assert!(
            !is_checked(&state),
            "two clicks did not return the switch to its original state"
        );
        assert_eq!(
            pushed_margins(&first),
            vec![(NodeId::new(KNOB_NODE), TRAVEL)]
        );
        assert_eq!(pushed_margins(&second), vec![(NodeId::new(KNOB_NODE), 0.0)]);
    }

    #[test]
    fn clicking_with_a_refany_of_the_wrong_type_is_a_silent_no_op() {
        // The handler downcasts blind; a foreign RefAny must bail out, not reinterpret
        // the bytes as a SwitchStateWrapper.
        let (styled, _) = laid_out(Switch::create(false));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = click(styled, &foreign, node(TRACK));

        assert!(matches!(update, Update::DoNothing));
        assert!(
            changes.is_empty(),
            "the handler wrote to the DOM through a foreign RefAny"
        );

        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand",
        );
    }

    #[test]
    fn clicking_the_knob_itself_does_not_half_apply_the_toggle() {
        // The knob is the child that sits under the cursor for most of the track's area,
        // and it has no first child of its own. The handler needs that child to slide the
        // knob, so it must leave the *flag* alone too — a flipped flag with no visual
        // update is a switch that renders the opposite of what it reports.
        let (styled, state) = laid_out(Switch::create(false));

        let (update, changes) = click(styled, &state, node(KNOB_NODE));

        assert!(matches!(update, Update::DoNothing));
        assert!(
            changes.is_empty(),
            "a change was pushed for a node the handler could not resolve"
        );
        assert!(
            !is_checked(&state),
            "the flag was flipped even though the knob could not be moved",
        );
    }

    #[test]
    fn stale_or_missing_hit_ids_do_not_panic_or_toggle() {
        // Stale hit ids reach callbacks after a DOM mutation. `set_css_property` *panics*
        // on a None node id, so the handler has to bail out before that point.
        // usize::MAX is unencodable by NodeId's 1-based scheme and would overflow while
        // building this fixture, before `click()` is even called; usize::MAX - 1 is the
        // repo's MAX_ENCODABLE_NODE and still absent from the layout.
        for hit in [node(2), node(99), node(usize::MAX - 1), node_none()] {
            let (styled, state) = laid_out(Switch::create(true));
            let (update, changes) = click(styled, &state, hit);

            assert!(
                matches!(update, Update::DoNothing),
                "{hit:?}: a stale hit was acted on"
            );
            assert!(
                changes.is_empty(),
                "{hit:?}: a stale hit pushed a DOM change"
            );
            assert!(
                is_checked(&state),
                "{hit:?}: a stale hit toggled the switch"
            );
        }
    }

    #[test]
    fn the_toggle_callback_sees_the_new_state_and_its_verdict_is_forwarded() {
        // Order matters: the flag is flipped *before* the user callback runs, so the
        // callback observes the state the user just asked for — not the stale one.
        let probe = log_refany();
        let (styled, state) = laid_out(
            Switch::create(false)
                .with_on_toggle(probe.clone(), record_toggle as SwitchOnToggleCallbackType),
        );

        let (update, changes) = click(styled, &state, node(TRACK));

        assert_eq!(
            read_log(&probe).seen,
            vec![true],
            "the toggle callback was not called exactly once with the NEW state",
        );
        assert!(
            matches!(update, Update::RefreshDom),
            "the user callback's Update was swallowed instead of forwarded",
        );
        // ... and the visual sync still happens *after* the user callback returns.
        assert_eq!(
            pushed_margins(&changes),
            vec![(NodeId::new(KNOB_NODE), TRAVEL)]
        );
    }

    #[test]
    fn the_toggle_callback_receives_the_user_payload_not_the_widget_state() {
        let probe = log_refany();
        let (styled, state) = laid_out(
            Switch::create(true)
                .with_on_toggle(probe.clone(), record_toggle as SwitchOnToggleCallbackType),
        );

        click(styled, &state, node(TRACK));

        let log = read_log(&probe);
        assert_eq!(
            log.payload, 0xDEAD_BEEF,
            "the callback was handed something other than the user's own RefAny",
        );
        // create(true) -> clicked once -> the callback must have seen `false`.
        assert_eq!(log.seen, vec![false]);
    }

    #[test]
    fn a_toggle_callback_that_declines_the_update_still_gets_the_visuals_synced() {
        // A user callback returning DoNothing must not suppress the widget's own visual
        // bookkeeping — otherwise the flag says "on" and the knob stays parked left.
        let (styled, state) = laid_out(Switch::create(false).with_on_toggle(
            RefAny::new(0u8),
            toggle_do_nothing as SwitchOnToggleCallbackType,
        ));

        let (update, changes) = click(styled, &state, node(TRACK));

        assert!(matches!(update, Update::DoNothing));
        assert!(is_checked(&state));
        assert_eq!(
            pushed_margins(&changes),
            vec![(NodeId::new(KNOB_NODE), TRAVEL)],
            "a DoNothing user callback suppressed the knob slide",
        );
    }

    #[test]
    fn a_toggle_callback_on_an_unresolvable_node_is_never_called_at_all() {
        // The bail-out happens before the flip *and* before the user callback: a click
        // that cannot be rendered must not be reported to the app either.
        let probe = log_refany();
        let (styled, state) = laid_out(
            Switch::create(false)
                .with_on_toggle(probe.clone(), record_toggle as SwitchOnToggleCallbackType),
        );

        let (update, changes) = click(styled, &state, node(KNOB_NODE));

        assert!(matches!(update, Update::DoNothing));
        assert!(changes.is_empty());
        assert!(!is_checked(&state));
        assert!(
            read_log(&probe).seen.is_empty(),
            "the user was notified of a toggle that never happened",
        );
    }

    #[test]
    fn many_clicks_leave_the_flag_the_colour_and_the_knob_in_agreement() {
        // 51 clicks starting off -> on. Every push must agree with the flag it
        // accompanies; a drift between the three is exactly the class of bug that makes
        // a switch render inverted after a while.
        let mut expected_on = false;
        let (_, state) = laid_out(Switch::create(false));

        for i in 0..51u32 {
            let (styled, _) = laid_out(Switch::create(false));
            let (_, changes) = click(styled, &state, node(TRACK));
            expected_on = !expected_on;

            let (color, margin) = if expected_on {
                (TRACK_ON_COLOR, TRAVEL)
            } else {
                (TRACK_OFF_COLOR, 0.0)
            };
            assert_eq!(
                pushed_backgrounds(&changes)
                    .iter()
                    .map(|(n, b)| (*n, solid_colors(b)))
                    .collect::<Vec<_>>(),
                vec![(NodeId::new(TRACK), vec![color])],
                "click #{i}: the pushed track colour disagrees with the flag",
            );
            assert_eq!(
                pushed_margins(&changes),
                vec![(NodeId::new(KNOB_NODE), margin)],
                "click #{i}: the pushed knob offset disagrees with the flag",
            );
            assert_eq!(
                is_checked(&state),
                expected_on,
                "click #{i}: the flag drifted"
            );
        }

        assert!(
            is_checked(&state),
            "an odd number of clicks left the switch off"
        );
    }

    #[test]
    fn a_click_writes_to_two_distinct_nodes_and_never_to_the_wrong_one() {
        // The track gets the colour, the knob gets the offset — never the other way
        // round, and never both onto the same node.
        let (styled, state) = laid_out(Switch::create(false));
        let (_, changes) = click(styled, &state, node(TRACK));

        let bg_nodes: Vec<_> = pushed_backgrounds(&changes)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        let margin_nodes: Vec<_> = pushed_margins(&changes)
            .into_iter()
            .map(|(n, _)| n)
            .collect();

        assert_eq!(bg_nodes, vec![NodeId::new(TRACK)]);
        assert_eq!(margin_nodes, vec![NodeId::new(KNOB_NODE)]);
        assert_ne!(
            bg_nodes, margin_nodes,
            "the colour and the knob offset landed on the same node",
        );
    }
}
