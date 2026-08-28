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
    impl_option_inner,
    props::{
        basic::{color::ColorU, *},
        layout::{
            LayoutDisplay, LayoutFlexGrow, LayoutLeft, LayoutMinWidth, LayoutPaddingBottom,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutTop,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
        },
    },
    AzString,
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
const CONTENT_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Panel border (#cccccc).
const CONTENT_BORDER_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
};

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
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(8,)
        )),
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
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(
            StyleBorderTopColor {
                inner: CONTENT_BORDER_COLOR,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: CONTENT_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(
            StyleBorderLeftColor {
                inner: CONTENT_BORDER_COLOR,
            }
        )),
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
    #[must_use]
    pub fn new(anchor: Dom, content: Dom) -> Self {
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
    #[must_use]
    pub fn with_open(mut self, open: bool) -> Self {
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
    #[must_use]
    pub fn with_on_toggle<C: Into<PopoverOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    /// Replaces `self` with a default (empty) popover and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(Dom::default(), Dom::default());
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the popover into a [`Dom`] subtree with the `__azul-native-popover`
    /// class.
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        // The trigger carries the click handler + the shared state. Clicking the
        // anchor (a descendant of the trigger) bubbles up to it (currentTarget
        // semantics — see `radio_group`), so `get_hit_node()` resolves to the
        // trigger regardless of what inside the anchor was clicked. Clicking the
        // panel does NOT toggle, since the panel is a sibling, not a child.
        let trigger = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(POPOVER_TRIGGER_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(POPOVER_TRIGGER_STYLE))
            .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // supplementary content attached to its anchor. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Tooltip,
                ..Default::default()
            })
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
            Some(PopoverOnToggle { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
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
    use azul_css::{props::property::CssPropertyType, system::SystemStyle};
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

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The text of a `NodeType::Text` node (`None` for any other node type).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// The `display` value in a node's *inline* style, if it sets one.
    fn inline_display(node: &Dom) -> Option<LayoutDisplay> {
        node.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
    }

    /// The property *types* of a style vec, in declaration order.
    fn prop_types(style: &CssPropertyWithConditionsVec) -> Vec<CssPropertyType> {
        style
            .as_ref()
            .iter()
            .map(|p| p.property.get_type())
            .collect()
    }

    /// *Every* `display` value declared in a style vec (order preserved) — a
    /// second entry would silently shadow the first.
    fn displays_in(style: &CssPropertyWithConditionsVec) -> Vec<LayoutDisplay> {
        style
            .as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
            .collect()
    }

    /// The `CssPropertyType` of `display`, without hard-coding the enum variant.
    fn display_ty() -> CssPropertyType {
        CssProperty::const_display(LayoutDisplay::None).get_type()
    }

    /// A three-node styled DOM — `root(0)` with children `trigger(1)` and
    /// `panel(2)` — i.e. the exact hierarchy `on_popover_toggle` walks
    /// (`hit node` -> `next sibling`).
    fn trigger_panel_dom() -> StyledDom {
        let styled = StyledDom::create_from_dom(
            Dom::create_div()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div()),
        );
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly wrapper/trigger/panel"
        );
        styled
    }

    /// Index of the first node carrying `class` in a flattened `StyledDom`.
    fn index_of_class(styled: &StyledDom, class: &str) -> usize {
        styled
            .node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .unwrap_or_else(|| panic!("no node with class {class} in the flattened DOM"))
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the toggle handler only
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

    /// Invokes `on_popover_toggle` against a `LayoutWindow` holding `styled` (or
    /// nothing at all, when `styled` is `None`), with `hit` as the hit node.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_toggle(
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

        let update = on_popover_toggle(data, info);
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

    /// `open` of a `PopoverStateWrapper` payload.
    fn payload_open(data: &mut RefAny) -> bool {
        data.downcast_ref::<PopoverStateWrapper>()
            .expect("payload must still be a PopoverStateWrapper")
            .inner
            .open
    }

    /// Records the states it is invoked with; used as a user `on_toggle`.
    struct ToggleLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_toggle(mut data: RefAny, _: CallbackInfo, state: PopoverState) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.calls.push(state.open);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(_: RefAny, _: CallbackInfo, _: PopoverState) -> Update {
        Update::DoNothing
    }

    fn toggle_cb(f: PopoverOnToggleCallbackType) -> PopoverOnToggleCallback {
        f.into()
    }

    // ------------------------------------------------------------------
    // build_content_style
    // ------------------------------------------------------------------

    #[test]
    fn content_style_open_and_closed_differ_only_in_display() {
        let closed = build_content_style(false);
        let open = build_content_style(true);

        assert_eq!(
            closed.len(),
            open.len(),
            "both states must declare the same props so the runtime display toggle \
             has everything it needs"
        );
        assert_eq!(prop_types(&closed), prop_types(&open));

        let differing: Vec<usize> = closed
            .as_ref()
            .iter()
            .zip(open.as_ref().iter())
            .enumerate()
            .filter_map(|(i, (c, o))| (c != o).then_some(i))
            .collect();

        assert_eq!(
            differing.len(),
            1,
            "exactly one declaration may differ between open and closed"
        );
        assert_eq!(
            closed.as_ref()[differing[0]].property.get_type(),
            display_ty(),
            "the only difference must be `display`"
        );
    }

    #[test]
    fn content_style_declares_display_exactly_once_and_correctly() {
        // A second `display` declaration would shadow the first and make the
        // open/closed state unobservable.
        assert_eq!(
            displays_in(&build_content_style(false)),
            alloc::vec![LayoutDisplay::None]
        );
        assert_eq!(
            displays_in(&build_content_style(true)),
            alloc::vec![LayoutDisplay::Block]
        );
    }

    #[test]
    fn content_style_has_no_duplicate_property_types() {
        for open in [false, true] {
            let mut types = prop_types(&build_content_style(open));
            let declared = types.len();
            assert!(declared > 0, "the panel style must not be empty");
            types.sort_unstable();
            types.dedup();
            assert_eq!(
                types.len(),
                declared,
                "a duplicated property type would make the later declaration silently win \
                 (open = {open})"
            );
        }
    }

    #[test]
    fn content_style_is_pure_and_unconditional() {
        for open in [false, true] {
            let a = build_content_style(open);
            let b = build_content_style(open);
            assert_eq!(
                a, b,
                "build_content_style must be a pure function of `open`"
            );
            assert!(
                a.as_ref().iter().all(|p| p.apply_if.as_ref().is_empty()),
                "the panel style must apply unconditionally — a stray condition would \
                 leave the panel unstyled (open = {open})"
            );
        }
    }

    #[test]
    fn content_style_carries_the_documented_geometry_in_both_states() {
        // The positioning props must be present whether the panel is open or
        // closed, otherwise the runtime `set_css_property(display)` toggle would
        // reveal an unpositioned panel.
        let expected = alloc::vec![
            CssPropertyWithConditions::simple(CssProperty::const_position(
                LayoutPosition::Absolute
            )),
            CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(
                CONTENT_OFFSET_Y
            ))),
            CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
            CssPropertyWithConditions::simple(CssProperty::const_min_width(
                LayoutMinWidth::const_px(CONTENT_MIN_WIDTH)
            )),
        ];

        for open in [false, true] {
            let style = build_content_style(open);
            for e in &expected {
                assert!(
                    style.as_ref().contains(e),
                    "{:?} missing from the {} panel style",
                    e.property.get_type(),
                    if open { "open" } else { "closed" }
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Popover::new / Default
    // ------------------------------------------------------------------

    #[test]
    fn new_stores_both_doms_and_starts_closed() {
        let anchor = Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("anchor"),
        );
        let content = Dom::create_text_do_not_use_without_block_level_wrapper("panel");
        let pop = Popover::new(anchor.clone(), content.clone());

        assert_eq!(pop.anchor, anchor, "the anchor must be stored verbatim");
        assert_eq!(pop.content, content, "the content must be stored verbatim");
        assert!(
            !pop.popover_state.inner.open,
            "a fresh popover must start closed"
        );
        assert!(
            pop.popover_state.on_toggle.is_none(),
            "Popover::new sets no callback"
        );
        assert_eq!(
            pop.content_style,
            build_content_style(false),
            "content_style must match the closed state it was constructed with"
        );
        assert_eq!(
            pop.wrapper_style,
            CssPropertyWithConditionsVec::from_const_slice(POPOVER_WRAPPER_STYLE)
        );
    }

    #[test]
    fn default_equals_new_with_empty_doms() {
        assert_eq!(
            Popover::default(),
            Popover::new(Dom::default(), Dom::default())
        );
        assert!(!Popover::default().popover_state.inner.open);
    }

    #[test]
    fn new_survives_extreme_doms() {
        // a 128-deep anchor and a 2000-sibling panel: nothing may be truncated,
        // reordered or recursed into during construction.
        let mut deep = Dom::create_text_do_not_use_without_block_level_wrapper("leaf");
        for _ in 0..128 {
            deep = Dom::create_div().with_child(deep);
        }
        let wide_children: Vec<Dom> = (0..2000)
            .map(|i| Dom::create_text_do_not_use_without_block_level_wrapper(alloc::format!("{i}")))
            .collect();
        let wide = Dom::create_div().with_children(wide_children.clone().into());

        let pop = Popover::new(deep.clone(), wide.clone());

        assert_eq!(pop.anchor, deep);
        assert_eq!(pop.content, wide);
        assert_eq!(pop.content.children.as_ref().len(), 2000);
        assert!(!pop.popover_state.inner.open);
    }

    #[test]
    fn new_accepts_the_same_dom_as_anchor_and_content() {
        // aliasing the two arguments must produce two independent subtrees, not
        // one shared (and later doubly-mounted) node.
        let shared = Dom::create_div()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x"));
        let pop = Popover::new(shared.clone(), shared.clone());

        assert_eq!(pop.anchor, shared);
        assert_eq!(pop.content, shared);

        let dom = pop.dom();
        let children = dom.children.as_ref();
        assert_eq!(
            text_of(&children[0].children.as_ref()[0].children.as_ref()[0]),
            Some("x")
        );
        assert_eq!(
            text_of(&children[1].children.as_ref()[0].children.as_ref()[0]),
            Some("x")
        );
    }

    // ------------------------------------------------------------------
    // set_open / with_open
    // ------------------------------------------------------------------

    #[test]
    fn set_open_round_trips_state_and_style() {
        let mut pop = Popover::new(
            Dom::create_text_do_not_use_without_block_level_wrapper("a"),
            Dom::create_text_do_not_use_without_block_level_wrapper("c"),
        );

        // repeats and flips: the style must follow the flag on every write,
        // including redundant ones.
        for open in [true, false, false, true, true, false, true] {
            pop.set_open(open);
            assert_eq!(pop.popover_state.inner.open, open);
            assert_eq!(
                pop.content_style,
                build_content_style(open),
                "content_style desynced from the open flag"
            );
            assert_eq!(
                displays_in(&pop.content_style),
                alloc::vec![if open {
                    LayoutDisplay::Block
                } else {
                    LayoutDisplay::None
                }]
            );
        }

        // the restyle must not touch the payload doms
        assert_eq!(
            pop.anchor,
            Dom::create_text_do_not_use_without_block_level_wrapper("a")
        );
        assert_eq!(
            pop.content,
            Dom::create_text_do_not_use_without_block_level_wrapper("c")
        );
    }

    #[test]
    fn with_open_matches_set_open() {
        for open in [false, true] {
            let mut mutated = Popover::new(
                Dom::create_text_do_not_use_without_block_level_wrapper("a"),
                Dom::create_text_do_not_use_without_block_level_wrapper("c"),
            );
            mutated.set_open(open);
            let built = Popover::new(
                Dom::create_text_do_not_use_without_block_level_wrapper("a"),
                Dom::create_text_do_not_use_without_block_level_wrapper("c"),
            )
            .with_open(open);
            assert_eq!(
                built, mutated,
                "builder and setter must agree (open = {open})"
            );
        }
    }

    #[test]
    fn with_open_last_write_wins() {
        let base = Popover::new(Dom::create_div(), Dom::create_div());

        assert!(
            !base
                .clone()
                .with_open(true)
                .with_open(false)
                .popover_state
                .inner
                .open
        );
        assert!(
            base.clone()
                .with_open(false)
                .with_open(true)
                .popover_state
                .inner
                .open
        );
        assert!(
            base.clone()
                .with_open(true)
                .with_open(true)
                .popover_state
                .inner
                .open,
            "applying the same value twice must be idempotent"
        );

        // the *style* must follow the last write too, not the first
        assert_eq!(
            base.with_open(true).with_open(false).content_style,
            build_content_style(false)
        );
    }

    // ------------------------------------------------------------------
    // set_on_toggle / with_on_toggle
    // ------------------------------------------------------------------

    #[test]
    fn set_on_toggle_last_call_wins() {
        let mut pop = Popover::default();

        pop.set_on_toggle(RefAny::new(1u8), toggle_cb(toggle_do_nothing));
        assert!(pop.popover_state.on_toggle.is_some());

        // a second call must *replace* (not append / leak / panic)
        pop.set_on_toggle(RefAny::new(9i64), toggle_cb(record_toggle));
        let set = pop.popover_state.on_toggle.as_ref().expect("still Some");
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());
        assert_eq!(set.callback, toggle_cb(record_toggle));
        assert_ne!(set.callback, toggle_cb(toggle_do_nothing));
    }

    #[test]
    fn set_on_toggle_does_not_disturb_state_style_or_doms() {
        let mut pop = Popover::new(
            Dom::create_text_do_not_use_without_block_level_wrapper("a"),
            Dom::create_text_do_not_use_without_block_level_wrapper("c"),
        )
        .with_open(true);
        let style_before = pop.content_style.clone();

        pop.set_on_toggle(RefAny::new(0u8), toggle_cb(toggle_do_nothing));

        assert!(pop.popover_state.inner.open, "open flag must survive");
        assert_eq!(pop.content_style, style_before, "style must survive");
        assert_eq!(
            pop.anchor,
            Dom::create_text_do_not_use_without_block_level_wrapper("a")
        );
        assert_eq!(
            pop.content,
            Dom::create_text_do_not_use_without_block_level_wrapper("c")
        );
    }

    #[test]
    fn with_on_toggle_matches_set_on_toggle() {
        let built = Popover::default().with_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        let mut mutated = Popover::default();
        mutated.set_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        assert!(built.popover_state.on_toggle.is_some());
        assert_eq!(
            built.popover_state.on_toggle.as_ref().unwrap().callback,
            mutated.popover_state.on_toggle.as_ref().unwrap().callback
        );
        // the builder form must not disturb the rest of the widget
        assert_eq!(built.anchor, Dom::default());
        assert!(!built.popover_state.inner.open);
        assert_eq!(built.content_style, build_content_style(false));
    }

    #[test]
    fn on_toggle_refany_is_shared_not_copied() {
        let mut shared = RefAny::new(ToggleLog { calls: Vec::new() });
        let pop = Popover::default().with_on_toggle(shared.clone(), toggle_cb(record_toggle));

        // a write through the widget's handle is visible through the caller's
        {
            let stored = pop.popover_state.on_toggle.as_ref().unwrap();
            let mut handle = stored.refany.clone();
            handle
                .downcast_mut::<ToggleLog>()
                .expect("payload type preserved")
                .calls
                .push(true);
        }

        assert_eq!(
            shared.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[true]
        );
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_moves_all_state_out() {
        let mut pop = Popover::new(
            Dom::create_text_do_not_use_without_block_level_wrapper("a"),
            Dom::create_text_do_not_use_without_block_level_wrapper("c"),
        )
        .with_open(true)
        .with_on_toggle(RefAny::new(5u8), toggle_cb(record_toggle));

        let original = pop.swap_with_default();

        assert_eq!(
            original.anchor,
            Dom::create_text_do_not_use_without_block_level_wrapper("a")
        );
        assert_eq!(
            original.content,
            Dom::create_text_do_not_use_without_block_level_wrapper("c")
        );
        assert!(original.popover_state.inner.open);
        assert!(original.popover_state.on_toggle.is_some());
        assert_eq!(original.content_style, build_content_style(true));

        assert_eq!(
            pop,
            Popover::default(),
            "self must be left as a default popover"
        );
        assert!(
            pop.popover_state.on_toggle.is_none(),
            "self must lose the callback"
        );
        assert!(!pop.popover_state.inner.open, "self must be re-closed");
        assert_eq!(pop.content_style, build_content_style(false));
    }

    #[test]
    fn swap_with_default_twice_is_a_noop() {
        let mut pop = Popover::default();
        let first = pop.swap_with_default();
        assert_eq!(first, Popover::default());

        let second = pop.swap_with_default();
        assert_eq!(second, Popover::default());
        assert_eq!(pop, Popover::default());
    }

    // ------------------------------------------------------------------
    // Popover::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_structure_classes_and_callback() {
        let dom = Popover::new(
            Dom::create_text_do_not_use_without_block_level_wrapper("anchor"),
            Dom::create_text_do_not_use_without_block_level_wrapper("panel"),
        )
        .dom();

        assert!(has_class(&dom, "__azul-native-popover"));
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "the wrapper is exactly [trigger, panel]");

        let (trigger, panel) = (&children[0], &children[1]);
        assert!(has_class(trigger, "__azul-native-popover-trigger"));
        assert!(has_class(panel, "__azul-native-popover-content"));

        // the caller's doms are wrapped, not rewritten
        assert_eq!(text_of(&trigger.children.as_ref()[0]), Some("anchor"));
        assert_eq!(text_of(&panel.children.as_ref()[0]), Some("panel"));

        // the trigger is focusable and carries exactly one MouseUp handler
        assert!(matches!(trigger.root.get_tab_index(), Some(TabIndex::Auto)));
        let cbs = trigger.root.get_callbacks();
        assert_eq!(cbs.len(), 1);
        assert_eq!(
            cbs.as_ref()[0].event,
            EventFilter::Hover(HoverEventFilter::MouseUp)
        );
        assert_eq!(cbs.as_ref()[0].callback.cb, on_popover_toggle as usize);

        // the panel must NOT be clickable — the documented behaviour is that
        // clicking *inside* the panel does not close it.
        assert!(
            panel.root.get_callbacks().as_ref().is_empty(),
            "the panel must carry no callbacks"
        );
    }

    #[test]
    fn dom_panel_display_follows_open_state() {
        for open in [false, true] {
            let dom = Popover::new(Dom::create_div(), Dom::create_div())
                .with_open(open)
                .dom();
            let panel = &dom.children.as_ref()[1];
            assert_eq!(
                inline_display(panel),
                Some(if open {
                    LayoutDisplay::Block
                } else {
                    LayoutDisplay::None
                }),
                "the rendered panel's display must match the open flag"
            );
        }
    }

    #[test]
    fn dom_payload_is_the_popover_state() {
        for open in [false, true] {
            let dom = Popover::new(Dom::create_div(), Dom::create_div())
                .with_open(open)
                .dom();
            let mut payload = dom.children.as_ref()[0].root.get_callbacks().as_ref()[0]
                .refany
                .clone();
            let state = payload
                .downcast_ref::<PopoverStateWrapper>()
                .expect("the trigger payload must be a PopoverStateWrapper");

            assert_eq!(
                state.inner.open, open,
                "the trigger's payload must agree with the panel's display"
            );
            assert!(state.on_toggle.is_none(), "no user callback was set");
        }
    }

    #[test]
    fn dom_keeps_the_user_callback_payload_alive() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let mut kept = log.clone();

        let dom = Popover::new(Dom::create_div(), Dom::create_div())
            .with_on_toggle(log, toggle_cb(record_toggle))
            .dom();

        let mut payload = dom.children.as_ref()[0].root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        assert!(
            payload
                .downcast_ref::<PopoverStateWrapper>()
                .unwrap()
                .on_toggle
                .is_some(),
            "the user callback must survive the move into the trigger payload"
        );

        // ...and the caller's handle to the shared payload is still valid (no free)
        assert!(kept.downcast_ref::<ToggleLog>().unwrap().calls.is_empty());
    }

    #[test]
    fn each_dom_gets_its_own_state_refany() {
        let a = Popover::default().dom();
        let b = Popover::default().dom();

        let ra = a.children.as_ref()[0].root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        let rb = b.children.as_ref()[0].root.get_callbacks().as_ref()[0]
            .refany
            .clone();

        assert_ne!(ra, rb, "two popovers must not share toggle state");
    }

    #[test]
    fn dom_child_count_cache_stays_consistent() {
        // deep + wide payloads: `estimated_total_children` must still equal the
        // real descendant count, otherwise the compact-DOM arena under-allocates
        // and panics later.
        let mut deep = Dom::create_text_do_not_use_without_block_level_wrapper("leaf");
        for _ in 0..64 {
            deep = Dom::create_div().with_child(deep);
        }
        let wide_children: Vec<Dom> = (0..256).map(|_| Dom::create_div()).collect();
        let wide = Dom::create_div().with_children(wide_children.into());

        let dom = Popover::new(deep, wide).dom();

        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children(),
            "cached descendant count desynced from the real tree"
        );
    }

    #[test]
    fn dom_of_default_popover_is_well_formed() {
        let dom = Popover::default().dom();
        assert!(has_class(&dom, "__azul-native-popover"));
        assert_eq!(dom.children.as_ref().len(), 2);
        assert_eq!(
            inline_display(&dom.children.as_ref()[1]),
            Some(LayoutDisplay::None),
            "a default popover renders a hidden panel"
        );
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
    }

    #[test]
    fn from_popover_for_dom_matches_dom_structurally() {
        // `Dom::from(p) == p.dom()` cannot be asserted directly: every `dom()`
        // call mints a fresh `RefAny` for the trigger payload, and two distinct
        // `RefAny`s never compare equal. Compare the observable structure.
        let make = || {
            Popover::new(
                Dom::create_text_do_not_use_without_block_level_wrapper("a"),
                Dom::create_text_do_not_use_without_block_level_wrapper("c"),
            )
            .with_open(true)
        };
        let via_from = Dom::from(make());
        let via_dom = make().dom();

        assert!(has_class(&via_from, "__azul-native-popover"));
        assert_eq!(via_from.children.as_ref().len(), 2);
        assert!(has_class(
            &via_from.children.as_ref()[0],
            "__azul-native-popover-trigger"
        ));
        assert!(has_class(
            &via_from.children.as_ref()[1],
            "__azul-native-popover-content"
        ));
        assert_eq!(
            inline_display(&via_from.children.as_ref()[1]),
            inline_display(&via_dom.children.as_ref()[1])
        );
        assert_eq!(
            via_from.children.as_ref()[1].children,
            via_dom.children.as_ref()[1].children
        );
        assert_eq!(
            via_from.estimated_total_children,
            via_dom.estimated_total_children
        );
    }

    // ------------------------------------------------------------------
    // on_popover_toggle
    // ------------------------------------------------------------------

    #[test]
    fn toggle_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(PopoverStateWrapper::default());

        let (update, changes) = run_toggle(None, 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "nothing may be restyled without a panel"
        );
        assert!(!payload_open(&mut data), "state must not flip");
    }

    #[test]
    fn toggle_without_next_sibling_does_not_flip_state() {
        // node 2 is the *last* child -> no next sibling -> early return, and
        // crucially `open` must NOT have been toggled.
        let mut data = RefAny::new(PopoverStateWrapper {
            inner: PopoverState { open: true },
            on_toggle: None.into(),
        });

        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(payload_open(&mut data), "state must be untouched");
    }

    #[test]
    fn toggle_with_stale_hit_node_is_a_noop() {
        let mut data = RefAny::new(PopoverStateWrapper::default());

        // node 999 does not exist in the 3-node fixture
        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!payload_open(&mut data));
    }

    #[test]
    fn toggle_with_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 1, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not restyle the panel"
        );
    }

    #[test]
    fn toggle_flips_state_and_panel_display() {
        let mut data = RefAny::new(PopoverStateWrapper::default());

        // closed -> open
        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::Block)]
        );
        assert!(payload_open(&mut data));

        // open -> closed (same payload, so the flip must be stateful)
        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::None)]
        );
        assert!(!payload_open(&mut data));
    }

    #[test]
    fn toggle_is_an_involution_over_many_clicks() {
        let mut data = RefAny::new(PopoverStateWrapper::default());

        for i in 0..8u32 {
            let (_, changes) = run_toggle(Some(trigger_panel_dom()), 1, data.clone());
            let expected_open = i % 2 == 0;
            assert_eq!(
                display_writes(&changes),
                alloc::vec![(
                    2usize,
                    if expected_open {
                        LayoutDisplay::Block
                    } else {
                        LayoutDisplay::None
                    }
                )],
                "click {i} wrote the wrong display"
            );
            assert_eq!(payload_open(&mut data), expected_open);
        }

        // an even number of clicks returns to the initial state
        assert!(!payload_open(&mut data));
    }

    #[test]
    fn toggle_display_agrees_with_build_content_style() {
        // the runtime override and the static style must not disagree, otherwise
        // a rebuild would flip the panel back.
        let data = RefAny::new(PopoverStateWrapper::default());
        let (_, changes) = run_toggle(Some(trigger_panel_dom()), 1, data);

        assert_eq!(
            display_writes(&changes)
                .into_iter()
                .map(|(_, d)| d)
                .collect::<Vec<_>>(),
            displays_in(&build_content_style(true))
        );
    }

    #[test]
    fn toggle_invokes_user_callback_with_the_new_state() {
        let mut log = RefAny::new(ToggleLog { calls: Vec::new() });
        let data = RefAny::new(PopoverStateWrapper {
            inner: PopoverState { open: false },
            on_toggle: Some(PopoverOnToggle {
                callback: toggle_cb(record_toggle),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 1, data.clone());

        // the user's return value wins over the internal DoNothing
        assert_eq!(update, Update::RefreshDom);
        // ...and the panel is still restyled, even though the user callback ran
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::Block)]
        );
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[true],
            "the callback must receive the *new* (post-flip) state"
        );

        // a second click reports the closed state
        let (_, _) = run_toggle(Some(trigger_panel_dom()), 1, data);
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[true, false]
        );
    }

    #[test]
    fn toggle_still_restyles_when_the_user_callback_does_nothing() {
        let data = RefAny::new(PopoverStateWrapper {
            inner: PopoverState { open: false },
            on_toggle: Some(PopoverOnToggle {
                callback: toggle_cb(toggle_do_nothing),
                refany: RefAny::new(0u8),
            })
            .into(),
        });

        let (update, changes) = run_toggle(Some(trigger_panel_dom()), 1, data);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::Block)],
            "the panel must be shown regardless of what the user callback returns"
        );
    }

    #[test]
    fn toggle_targets_the_panel_in_a_really_rendered_popover() {
        // End-to-end: the handler assumes "the hit trigger's next sibling is the
        // panel". Verify that against the DOM `Popover::dom()` actually builds,
        // rather than against a hand-made fixture.
        let styled =
            StyledDom::create_from_dom(Popover::new(Dom::create_div(), Dom::create_div()).dom());
        let trigger_idx = index_of_class(&styled, "__azul-native-popover-trigger");
        let panel_idx = index_of_class(&styled, "__azul-native-popover-content");

        let data = RefAny::new(PopoverStateWrapper::default());
        let (_, changes) = run_toggle(Some(styled), trigger_idx, data);

        assert_eq!(
            display_writes(&changes),
            alloc::vec![(panel_idx, LayoutDisplay::Block)],
            "the toggle must restyle the popover's own panel, not a stray sibling"
        );
    }

    #[test]
    fn toggle_on_the_wrapper_node_does_not_touch_the_panel() {
        // Clicking the *wrapper* (node 0, the root) must not flip anything: the
        // root has no next sibling.
        let styled =
            StyledDom::create_from_dom(Popover::new(Dom::create_div(), Dom::create_div()).dom());
        let wrapper_idx = index_of_class(&styled, "__azul-native-popover");

        let mut data = RefAny::new(PopoverStateWrapper::default());
        let (update, changes) = run_toggle(Some(styled), wrapper_idx, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!payload_open(&mut data));
    }
}
