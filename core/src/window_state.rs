//! Event and callback filtering module

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};

use azul_css::{
    props::{
        basic::{LayoutPoint, LayoutRect, LayoutSize},
        property::CssProperty,
    },
    AzString, LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

use crate::{
    app_resources::{ImageCache, RendererResources},
    callbacks::{DocumentId, DomNodeId, HitTestItem, ScrollPosition, Update},
    dom::{EventFilter, FocusEventFilter, HoverEventFilter, NotEventFilter, WindowEventFilter},
    gl::OptionGlContextPtr,
    id_tree::NodeId,
    styled_dom::{ChangedCssProperty, DomId, NodeHierarchyItemId},
    task::ExternalSystemCallbacks,
    ui_solver::{GpuEventChanges, LayoutResult, RelayoutChanges},
    window::{CallCallbacksResult, FullHitTest, FullWindowState, RawWindowHandle, ScrollStates},
    FastBTreeSet, FastHashMap,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Events {
    pub window_events: Vec<WindowEventFilter>,
    pub hover_events: Vec<HoverEventFilter>,
    pub focus_events: Vec<FocusEventFilter>,
    pub old_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_focus_node: Option<DomNodeId>,
    pub current_window_state_mouse_is_down: bool,
    pub previous_window_state_mouse_is_down: bool,
    pub event_was_mouse_down: bool,
    pub event_was_mouse_leave: bool,
    pub event_was_mouse_release: bool,
}

impl Events {
    /// Warning: if the previous_window_state is none, this will return an empty Vec!
    pub fn new(
        current_window_state: &FullWindowState,
        previous_window_state: &Option<FullWindowState>,
    ) -> Self {
        let mut current_window_events =
            get_window_events(current_window_state, previous_window_state);
        let mut current_hover_events = get_hover_events(&current_window_events);
        let mut current_focus_events = get_focus_events(&current_hover_events);

        let event_was_mouse_down = current_window_events
            .iter()
            .any(|e| *e == WindowEventFilter::MouseDown);
        let event_was_mouse_release = current_window_events
            .iter()
            .any(|e| *e == WindowEventFilter::MouseUp);
        let event_was_mouse_leave = current_window_events
            .iter()
            .any(|e| *e == WindowEventFilter::MouseLeave);
        let current_window_state_mouse_is_down = current_window_state.mouse_state.mouse_down();
        let previous_window_state_mouse_is_down = previous_window_state
            .as_ref()
            .map(|f| f.mouse_state.mouse_down())
            .unwrap_or(false);

        let old_focus_node = previous_window_state
            .as_ref()
            .and_then(|f| f.focused_node.clone());
        let old_hit_node_ids = previous_window_state
            .as_ref()
            .map(|f| {
                if f.last_hit_test.hovered_nodes.is_empty() {
                    BTreeMap::new()
                } else {
                    f.last_hit_test
                        .hovered_nodes
                        .iter()
                        .map(|(dom_id, hit_test)| {
                            (*dom_id, hit_test.regular_hit_test_nodes.clone())
                        })
                        .collect()
                }
            })
            .unwrap_or_default();

        if let Some(prev_state) = previous_window_state.as_ref() {
            if prev_state.theme != current_window_state.theme {
                current_window_events.push(WindowEventFilter::ThemeChanged);
            }
            if current_window_state.last_hit_test.hovered_nodes
                != prev_state.last_hit_test.hovered_nodes.clone()
            {
                current_hover_events.push(HoverEventFilter::MouseLeave);
                current_hover_events.push(HoverEventFilter::MouseEnter);
            }
        }

        // even if there are no window events, the focus node can changed
        if current_window_state.focused_node != old_focus_node {
            current_focus_events.push(FocusEventFilter::FocusReceived);
            current_focus_events.push(FocusEventFilter::FocusLost);
        }

        Events {
            window_events: current_window_events,
            hover_events: current_hover_events,
            focus_events: current_focus_events,
            event_was_mouse_down,
            event_was_mouse_release,
            event_was_mouse_leave,
            current_window_state_mouse_is_down,
            previous_window_state_mouse_is_down,
            old_focus_node,
            old_hit_node_ids,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.window_events.is_empty()
            && self.hover_events.is_empty()
            && self.focus_events.is_empty()
    }

    /// Checks whether the event was a resize event
    pub fn contains_resize_event(&self) -> bool {
        self.window_events.contains(&WindowEventFilter::Resized)
    }

    pub fn event_was_mouse_scroll(&self) -> bool {
        // TODO: also need to look at TouchStart / TouchDrag
        self.window_events.contains(&WindowEventFilter::Scroll)
    }

    pub fn needs_hit_test(&self) -> bool {
        !(self.hover_events.is_empty() && self.focus_events.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodesToCheck {
    pub new_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub onmouseenter_nodes: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub onmouseleave_nodes: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_focus_node: Option<DomNodeId>,
    pub new_focus_node: Option<DomNodeId>,
    pub current_window_state_mouse_is_down: bool,
}

impl NodesToCheck {
    // Usually we need to perform a hit-test when the DOM is re-generated,
    // this function simulates that behaviour
    pub fn simulated_mouse_move(
        hit_test: &FullHitTest,
        old_focus_node: Option<DomNodeId>,
        mouse_down: bool,
    ) -> Self {
        let new_hit_node_ids = hit_test
            .hovered_nodes
            .iter()
            .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
            .collect::<BTreeMap<_, _>>();

        Self {
            new_hit_node_ids: new_hit_node_ids.clone(),
            old_hit_node_ids: BTreeMap::new(),
            onmouseenter_nodes: new_hit_node_ids,
            onmouseleave_nodes: BTreeMap::new(),
            old_focus_node,
            new_focus_node: old_focus_node,
            current_window_state_mouse_is_down: mouse_down,
        }
    }

    /// Determine which nodes are even relevant for callbacks or restyling
    //
    // TODO: avoid iteration / allocation!
    pub fn new(hit_test: &FullHitTest, events: &Events) -> Self {
        // TODO: If the current mouse is down, but the event wasn't a click, that means it was a
        // drag

        // Figure out what the hovered NodeIds are
        let new_hit_node_ids = if events.event_was_mouse_leave {
            BTreeMap::new()
        } else {
            hit_test
                .hovered_nodes
                .iter()
                .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
                .collect()
        };

        // Figure out what the current focused NodeId is
        let new_focus_node = if events.event_was_mouse_release {
            hit_test.focused_node.clone().map(|o| DomNodeId {
                dom: o.0,
                node: NodeHierarchyItemId::from_crate_internal(Some(o.1)),
            })
        } else {
            events.old_focus_node.clone()
        };

        // Collect all On::MouseEnter nodes (for both hover and focus events)
        let default_map = BTreeMap::new();
        let onmouseenter_nodes = new_hit_node_ids
            .iter()
            .filter_map(|(dom_id, nhnid)| {
                let old_hit_node_ids = events.old_hit_node_ids.get(dom_id).unwrap_or(&default_map);
                let new = nhnid
                    .iter()
                    .filter(|(current_node_id, _)| old_hit_node_ids.get(current_node_id).is_none())
                    .map(|(x, y)| (*x, y.clone()))
                    .collect::<BTreeMap<_, _>>();
                if new.is_empty() {
                    None
                } else {
                    Some((*dom_id, new))
                }
            })
            .collect::<BTreeMap<_, _>>();

        // Collect all On::MouseLeave nodes (for both hover and focus events)
        let onmouseleave_nodes = events
            .old_hit_node_ids
            .iter()
            .filter_map(|(dom_id, ohnid)| {
                let old = ohnid
                    .iter()
                    .filter(|(prev_node_id, _)| {
                        new_hit_node_ids
                            .get(dom_id)
                            .and_then(|d| d.get(prev_node_id))
                            .is_none()
                    })
                    .map(|(x, y)| (*x, y.clone()))
                    .collect::<BTreeMap<_, _>>();
                if old.is_empty() {
                    None
                } else {
                    Some((*dom_id, old))
                }
            })
            .collect::<BTreeMap<_, _>>();

        NodesToCheck {
            new_hit_node_ids,
            old_hit_node_ids: events.old_hit_node_ids.clone(),
            onmouseenter_nodes,
            onmouseleave_nodes,
            old_focus_node: events.old_focus_node.clone(),
            new_focus_node,
            current_window_state_mouse_is_down: events.current_window_state_mouse_is_down,
        }
    }

    pub fn empty(mouse_down: bool, old_focus_node: Option<DomNodeId>) -> Self {
        Self {
            new_hit_node_ids: BTreeMap::new(),
            old_hit_node_ids: BTreeMap::new(),
            onmouseenter_nodes: BTreeMap::new(),
            onmouseleave_nodes: BTreeMap::new(),
            old_focus_node,
            new_focus_node: old_focus_node,
            current_window_state_mouse_is_down: mouse_down,
        }
    }

    pub fn needs_hover_active_restyle(&self) -> bool {
        !(self.onmouseenter_nodes.is_empty() && self.onmouseleave_nodes.is_empty())
    }

    pub fn needs_focus_result(&self) -> bool {
        self.old_focus_node != self.new_focus_node
    }
}

pub type RestyleNodes = BTreeMap<NodeId, Vec<ChangedCssProperty>>;
pub type RelayoutNodes = BTreeMap<NodeId, Vec<ChangedCssProperty>>;
pub type RelayoutWords = BTreeMap<NodeId, AzString>;

/// Style and layout changes
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAndLayoutChanges {
    /// Changes that were made to style properties of nodes
    pub style_changes: Option<BTreeMap<DomId, RestyleNodes>>,
    /// Changes that were made to layout properties of nodes
    pub layout_changes: Option<BTreeMap<DomId, RelayoutNodes>>,
    /// Whether the focus has actually changed
    pub focus_change: Option<FocusChange>,
    /// Used to call `On::Resize` handlers
    pub nodes_that_changed_size: Option<BTreeMap<DomId, Vec<NodeId>>>,
    /// Changes to the text content
    pub nodes_that_changed_text_content: Option<BTreeMap<DomId, Vec<NodeId>>>,
    /// Changes to GPU-cached opacity / transform values
    pub gpu_key_changes: Option<BTreeMap<DomId, GpuEventChanges>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FocusChange {
    pub old: Option<DomNodeId>,
    pub new: Option<DomNodeId>,
}

// azul_layout::do_the_relayout satifies this
pub type RelayoutFn = fn(
    DomId,
    LayoutRect,
    &mut LayoutResult,
    &ImageCache,
    &mut RendererResources,
    &DocumentId,
    Option<&RelayoutNodes>,
    Option<&RelayoutWords>,
    &mut Option<Vec<LayoutDebugMessage>>,
) -> RelayoutChanges;

impl StyleAndLayoutChanges {
    /// Determines and immediately applies the changes to the layout results
    pub fn new(
        nodes: &NodesToCheck,
        layout_results: &mut [LayoutResult],
        image_cache: &ImageCache,
        renderer_resources: &mut RendererResources,
        window_size: LayoutSize,
        document_id: &DocumentId,
        css_changes: Option<&BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>>,
        word_changes: Option<&BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
        callbacks_new_focus: &Option<Option<DomNodeId>>,
        relayout_cb: RelayoutFn,
    ) -> StyleAndLayoutChanges {
        // immediately restyle the DOM to reflect the new :hover, :active and :focus nodes
        // and determine if the DOM needs a redraw or a relayout
        let mut style_changes = None;
        let mut layout_changes = None;

        let is_mouse_down = nodes.current_window_state_mouse_is_down;
        let nodes_that_changed_text_content = word_changes.and_then(|word_changes| {
            if word_changes.is_empty() {
                None
            } else {
                Some(
                    word_changes
                        .iter()
                        .map(|(dom_id, m)| (*dom_id, m.keys().cloned().collect()))
                        .collect(),
                )
            }
        });

        macro_rules! insert_props {
            ($dom_id:expr, $prop_map:expr) => {{
                let dom_id: DomId = $dom_id;
                for (node_id, prop_map) in $prop_map.into_iter() {
                    for changed_prop in prop_map.into_iter() {
                        let prop_key = changed_prop.previous_prop.get_type();
                        if prop_key.can_trigger_relayout() {
                            layout_changes
                                .get_or_insert_with(|| BTreeMap::new())
                                .entry(dom_id)
                                .or_insert_with(|| BTreeMap::new())
                                .entry(node_id)
                                .or_insert_with(|| Vec::new())
                                .push(changed_prop);
                        } else {
                            style_changes
                                .get_or_insert_with(|| BTreeMap::new())
                                .entry(dom_id)
                                .or_insert_with(|| BTreeMap::new())
                                .entry(node_id)
                                .or_insert_with(|| Vec::new())
                                .push(changed_prop);
                        }
                    }
                }
            }};
        }

        for (dom_id, onmouseenter_nodes) in nodes.onmouseenter_nodes.iter() {
            let layout_result = &mut layout_results[dom_id.inner];

            let keys = onmouseenter_nodes.keys().copied().collect::<Vec<_>>();
            let onmouseenter_nodes_hover_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_hover(&keys, /* currently_hovered = */ true);
            let onmouseleave_nodes_active_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_active(&keys, /* currently_active = */ is_mouse_down);

            insert_props!(*dom_id, onmouseenter_nodes_hover_restyle_props);
            insert_props!(*dom_id, onmouseleave_nodes_active_restyle_props);
        }

        for (dom_id, onmouseleave_nodes) in nodes.onmouseleave_nodes.iter() {
            let layout_result = &mut layout_results[dom_id.inner];
            let keys = onmouseleave_nodes.keys().copied().collect::<Vec<_>>();
            let onmouseleave_nodes_hover_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_hover(&keys, /* currently_hovered = */ false);
            let onmouseleave_nodes_active_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_active(&keys, /* currently_active = */ false);

            insert_props!(*dom_id, onmouseleave_nodes_hover_restyle_props);
            insert_props!(*dom_id, onmouseleave_nodes_active_restyle_props);
        }

        let new_focus_node = if let Some(new) = callbacks_new_focus.as_ref() {
            new
        } else {
            &nodes.new_focus_node
        };

        let focus_change = if nodes.old_focus_node != *new_focus_node {
            if let Some(DomNodeId { dom, node }) = nodes.old_focus_node.as_ref() {
                if let Some(node_id) = node.into_crate_internal() {
                    let layout_result = &mut layout_results[dom.inner];
                    let onfocus_leave_restyle_props = layout_result
                        .styled_dom
                        .restyle_nodes_focus(&[node_id], /* currently_focused = */ false);
                    let dom_id: DomId = *dom;
                    insert_props!(dom_id, onfocus_leave_restyle_props);
                }
            }

            if let Some(DomNodeId { dom, node }) = new_focus_node.as_ref() {
                if let Some(node_id) = node.into_crate_internal() {
                    let layout_result = &mut layout_results[dom.inner];
                    let onfocus_enter_restyle_props = layout_result
                        .styled_dom
                        .restyle_nodes_focus(&[node_id], /* currently_focused = */ true);
                    let dom_id: DomId = *dom;
                    insert_props!(dom_id, onfocus_enter_restyle_props);
                }
            }

            Some(FocusChange {
                old: nodes.old_focus_node,
                new: *new_focus_node,
            })
        } else {
            None
        };

        // restyle all the nodes according to the existing_changed_styles
        if let Some(css_changes) = css_changes {
            for (dom_id, existing_changes_map) in css_changes.iter() {
                let layout_result = &mut layout_results[dom_id.inner];
                let dom_id: DomId = *dom_id;
                for (node_id, changed_css_property_vec) in existing_changes_map.iter() {
                    let current_prop_changes = layout_result
                        .styled_dom
                        .restyle_user_property(node_id, &changed_css_property_vec);
                    insert_props!(dom_id, current_prop_changes);
                }
            }
        }

        let mut nodes_that_changed_size = None;
        let mut gpu_key_change_events = None;

        // recursively relayout if there are layout_changes or the window size has changed
        let window_was_resized = window_size != layout_results[DomId::ROOT_ID.inner].root_size;
        let need_root_relayout = layout_changes.is_some()
            || window_was_resized
            || nodes_that_changed_text_content.is_some();

        let mut doms_to_relayout = Vec::new();
        if need_root_relayout {
            doms_to_relayout.push(DomId::ROOT_ID);
        } else {
            // if no nodes were resized or styles changed,
            // still update the GPU-only properties
            for (dom_id, layout_result) in layout_results.iter_mut().enumerate() {
                let gpu_key_changes = layout_result
                    .gpu_value_cache
                    .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

                if !gpu_key_changes.is_empty() {
                    gpu_key_change_events
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(DomId { inner: dom_id }, gpu_key_changes);
                }
            }
        }

        loop {
            let mut new_iframes_to_relayout = Vec::new();

            for dom_id in doms_to_relayout.drain(..) {
                let parent_rect = match layout_results[dom_id.inner].parent_dom_id.as_ref() {
                    None => LayoutRect::new(LayoutPoint::zero(), window_size),
                    Some(parent_dom_id) => {
                        let parent_layout_result = &layout_results[parent_dom_id.inner];
                        let parent_iframe_node_id = parent_layout_result
                            .iframe_mapping
                            .iter()
                            .find_map(|(k, v)| if *v == dom_id { Some(*k) } else { None })
                            .unwrap();
                        parent_layout_result.rects.as_ref()[parent_iframe_node_id]
                            .get_approximate_static_bounds()
                    }
                };

                let layout_changes = layout_changes.as_ref().and_then(|w| w.get(&dom_id));
                let word_changes = word_changes.and_then(|w| w.get(&dom_id));

                // TODO: avoid allocation
                let RelayoutChanges {
                    resized_nodes,
                    gpu_key_changes,
                } = (relayout_cb)(
                    dom_id,
                    parent_rect,
                    &mut layout_results[dom_id.inner],
                    image_cache,
                    renderer_resources,
                    document_id,
                    layout_changes,
                    word_changes,
                    &mut None, // no debug messages
                );

                if !gpu_key_changes.is_empty() {
                    gpu_key_change_events
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(dom_id, gpu_key_changes);
                }

                if !resized_nodes.is_empty() {
                    new_iframes_to_relayout.extend(
                        layout_results[dom_id.inner]
                            .iframe_mapping
                            .iter()
                            .filter_map(|(node_id, dom_id)| {
                                if resized_nodes.contains(node_id) {
                                    Some(dom_id)
                                } else {
                                    None
                                }
                            }),
                    );
                    nodes_that_changed_size
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(dom_id, resized_nodes);
                }
            }

            if new_iframes_to_relayout.is_empty() {
                break;
            } else {
                doms_to_relayout = new_iframes_to_relayout;
            }
        }

        StyleAndLayoutChanges {
            style_changes,
            layout_changes,
            nodes_that_changed_size,
            nodes_that_changed_text_content,
            focus_change,
            gpu_key_changes: gpu_key_change_events,
        }
    }

    pub fn did_resize_nodes(&self) -> bool {
        use azul_css::props::property::CssPropertyType;

        if let Some(l) = self.nodes_that_changed_size.as_ref() {
            if !l.is_empty() {
                return true;
            }
        }

        if let Some(l) = self.nodes_that_changed_text_content.as_ref() {
            if !l.is_empty() {
                return true;
            }
        }

        // check if any changed node is a CSS transform
        if let Some(s) = self.style_changes.as_ref() {
            for restyle_nodes in s.values() {
                for changed in restyle_nodes.values() {
                    for changed in changed.iter() {
                        if changed.current_prop.get_type() == CssPropertyType::Transform {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    // Note: this can be false in case that only opacity: / transform: properties changed!
    pub fn need_regenerate_display_list(&self) -> bool {
        if !self.nodes_that_changed_size.is_none() {
            return true;
        }
        if !self.nodes_that_changed_text_content.is_none() {
            return true;
        }
        if !self.need_redraw() {
            return false;
        }

        // is_gpu_only_property = is the changed CSS property an opacity /
        // transform / rotate property (which doesn't require to regenerate the display list)
        if let Some(style_changes) = self.style_changes.as_ref() {
            !(style_changes.iter().all(|(_, restyle_nodes)| {
                restyle_nodes.iter().all(|(_, changed_css_properties)| {
                    changed_css_properties.iter().all(|changed_prop| {
                        changed_prop.current_prop.get_type().is_gpu_only_property()
                    })
                })
            }))
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.style_changes.is_none()
            && self.layout_changes.is_none()
            && self.focus_change.is_none()
            && self.nodes_that_changed_size.is_none()
            && self.nodes_that_changed_text_content.is_none()
            && self.gpu_key_changes.is_none()
    }

    pub fn need_redraw(&self) -> bool {
        !(self.style_changes.is_none()
            && self.layout_changes.is_none()
            && self.nodes_that_changed_text_content.is_none()
            && self.nodes_that_changed_size.is_none())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallbackToCall {
    pub node_id: NodeId,
    pub hit_test_item: Option<HitTestItem>,
    pub event_filter: EventFilter,
}

#[derive(Debug, Clone)]
pub struct CallbacksOfHitTest {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<DomId, Vec<CallbackToCall>>,
}

impl CallbacksOfHitTest {
    /// Determine which event / which callback(s) should be called and in which order
    ///
    /// This function also updates / mutates the current window states `focused_node`
    /// as well as the `window_state.previous_state`
    pub fn new(
        nodes_to_check: &NodesToCheck,
        events: &Events,
        layout_results: &[LayoutResult],
    ) -> Self {
        let mut nodes_with_callbacks = BTreeMap::new();

        if events.is_empty() {
            return Self {
                nodes_with_callbacks,
            };
        }

        let default_map = BTreeMap::new();
        let mouseenter_filter = EventFilter::Hover(HoverEventFilter::MouseEnter);
        let mouseleave_filter = EventFilter::Hover(HoverEventFilter::MouseEnter);
        let focus_received_filter = EventFilter::Focus(FocusEventFilter::FocusReceived);
        let focus_lost_filter = EventFilter::Focus(FocusEventFilter::FocusLost);

        for (dom_id, layout_result) in layout_results.iter().enumerate() {
            let dom_id = DomId { inner: dom_id };

            // Insert Window:: event filters
            let mut window_callbacks_this_dom = layout_result
                .styled_dom
                .nodes_with_window_callbacks
                .iter()
                .flat_map(|nid| {
                    let node_id = match nid.into_crate_internal() {
                        Some(s) => s,
                        None => return Vec::new(),
                    };
                    layout_result.styled_dom.node_data.as_container()[node_id]
                        .get_callbacks()
                        .iter()
                        .filter_map(|cb| match cb.event {
                            EventFilter::Window(wev) => {
                                if events.window_events.contains(&wev) {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Window(wev),
                                        hit_test_item: None,
                                        node_id,
                                    })
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            // window_callbacks_this_dom now contains all WindowEvent filters

            // insert Hover::MouseEnter events
            window_callbacks_this_dom.extend(
                nodes_to_check
                    .onmouseenter_nodes
                    .get(&dom_id)
                    .unwrap_or(&default_map)
                    .iter()
                    .filter_map(|(node_id, ht)| {
                        if layout_result.styled_dom.node_data.as_container()[*node_id]
                            .get_callbacks()
                            .iter()
                            .any(|e| e.event == mouseenter_filter)
                        {
                            Some(CallbackToCall {
                                event_filter: mouseenter_filter.clone(),
                                hit_test_item: Some(*ht),
                                node_id: *node_id,
                            })
                        } else {
                            None
                        }
                    }),
            );

            // insert Hover::MouseLeave events
            window_callbacks_this_dom.extend(
                nodes_to_check
                    .onmouseleave_nodes
                    .get(&dom_id)
                    .unwrap_or(&default_map)
                    .iter()
                    .filter_map(|(node_id, ht)| {
                        if layout_result.styled_dom.node_data.as_container()[*node_id]
                            .get_callbacks()
                            .iter()
                            .any(|e| e.event == mouseleave_filter)
                        {
                            Some(CallbackToCall {
                                event_filter: mouseleave_filter.clone(),
                                hit_test_item: Some(*ht),
                                node_id: *node_id,
                            })
                        } else {
                            None
                        }
                    }),
            );

            // insert other Hover:: events
            for (nid, ht) in nodes_to_check
                .new_hit_node_ids
                .get(&dom_id)
                .unwrap_or(&default_map)
                .iter()
            {
                for hev in events.hover_events.iter() {
                    window_callbacks_this_dom.extend(
                        layout_result.styled_dom.node_data.as_container()[*nid]
                            .get_callbacks()
                            .iter()
                            .filter_map(|e| {
                                if e.event == EventFilter::Hover(*hev)
                                    && e.event != mouseenter_filter
                                    && e.event != mouseleave_filter
                                {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Hover(hev.clone()),
                                        hit_test_item: Some(*ht),
                                        node_id: *nid,
                                    })
                                } else {
                                    None
                                }
                            }),
                    );
                }
            }

            // insert Focus(FocusReceived / FocusLost) event
            if nodes_to_check.new_focus_node != nodes_to_check.old_focus_node {
                if let Some(DomNodeId {
                    dom,
                    node: az_node_id,
                }) = nodes_to_check.old_focus_node
                {
                    if dom == dom_id {
                        if let Some(nid) = az_node_id.into_crate_internal() {
                            if layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                                .any(|e| e.event == focus_lost_filter)
                            {
                                window_callbacks_this_dom.push(CallbackToCall {
                                    event_filter: focus_lost_filter.clone(),
                                    hit_test_item: events
                                        .old_hit_node_ids
                                        .get(&dom_id)
                                        .and_then(|map| map.get(&nid))
                                        .cloned(),
                                    node_id: nid,
                                })
                            }
                        }
                    }
                }

                if let Some(DomNodeId {
                    dom,
                    node: az_node_id,
                }) = nodes_to_check.new_focus_node
                {
                    if dom == dom_id {
                        if let Some(nid) = az_node_id.into_crate_internal() {
                            if layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                                .any(|e| e.event == focus_received_filter)
                            {
                                window_callbacks_this_dom.push(CallbackToCall {
                                    event_filter: focus_received_filter.clone(),
                                    hit_test_item: events
                                        .old_hit_node_ids
                                        .get(&dom_id)
                                        .and_then(|map| map.get(&nid))
                                        .cloned(),
                                    node_id: nid,
                                })
                            }
                        }
                    }
                }
            }

            // Insert other Focus: events
            if let Some(DomNodeId {
                dom,
                node: az_node_id,
            }) = nodes_to_check.new_focus_node
            {
                if dom == dom_id {
                    if let Some(nid) = az_node_id.into_crate_internal() {
                        for fev in events.focus_events.iter() {
                            for cb in layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                            {
                                if cb.event == EventFilter::Focus(*fev)
                                    && cb.event != focus_received_filter
                                    && cb.event != focus_lost_filter
                                {
                                    window_callbacks_this_dom.push(CallbackToCall {
                                        event_filter: EventFilter::Focus(fev.clone()),
                                        hit_test_item: events
                                            .old_hit_node_ids
                                            .get(&dom_id)
                                            .and_then(|map| map.get(&nid))
                                            .cloned(),
                                        node_id: nid,
                                    })
                                }
                            }
                        }
                    }
                }
            }

            if !window_callbacks_this_dom.is_empty() {
                nodes_with_callbacks.insert(dom_id, window_callbacks_this_dom);
            }
        }

        // Final: insert Not:: event filters
        for (dom_id, layout_result) in layout_results.iter().enumerate() {
            let dom_id = DomId { inner: dom_id };

            let not_event_filters = layout_result
                .styled_dom
                .nodes_with_not_callbacks
                .iter()
                .flat_map(|node_id| {
                    let node_id = match node_id.into_crate_internal() {
                        Some(s) => s,
                        None => return Vec::new(),
                    };
                    layout_result.styled_dom.node_data.as_container()[node_id]
                        .get_callbacks()
                        .iter()
                        .filter_map(|cb| match cb.event {
                            EventFilter::Not(nev) => {
                                if nodes_with_callbacks.get(&dom_id).map(|v| {
                                    v.iter().any(|cb| {
                                        cb.node_id == node_id
                                            && cb.event_filter == nev.as_event_filter()
                                    })
                                }) != Some(true)
                                {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Not(nev.clone()),
                                        hit_test_item: events
                                            .old_hit_node_ids
                                            .get(&dom_id)
                                            .and_then(|map| map.get(&node_id))
                                            .cloned(),
                                        node_id,
                                    })
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            for cb in not_event_filters {
                nodes_with_callbacks
                    .entry(dom_id)
                    .or_insert_with(|| Vec::new())
                    .push(cb);
            }
        }

        CallbacksOfHitTest {
            nodes_with_callbacks,
        }
    }

    /// The actual function that calls the callbacks in their proper hierarchy and order
    pub fn call(
        &mut self,
        previous_window_state: &Option<FullWindowState>,
        full_window_state: &FullWindowState,
        raw_window_handle: &RawWindowHandle,
        scroll_states: &BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
        gl_context: &OptionGlContextPtr,
        layout_results: &mut Vec<LayoutResult>,
        modifiable_scroll_states: &mut ScrollStates,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_callbacks: &ExternalSystemCallbacks,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use crate::{
            callbacks::CallbackInfo, styled_dom::ParentWithNodeDepth, window::WindowState,
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
        };
        let mut new_focus_target = None;

        let current_cursor = full_window_state.mouse_state.mouse_cursor_type.clone();

        if self.nodes_with_callbacks.is_empty() {
            // common case
            return ret;
        }

        let mut ret_modified_window_state: WindowState = full_window_state.clone().into();
        let mut ret_modified_window_state_unmodified = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();

        {
            for (dom_id, callbacks_filter_list) in self.nodes_with_callbacks.iter() {
                let mut callbacks = BTreeMap::new();
                for cbtc in callbacks_filter_list {
                    callbacks
                        .entry(cbtc.node_id)
                        .or_insert_with(|| Vec::new())
                        .push((cbtc.hit_test_item, cbtc.event_filter));
                }
                let callbacks = callbacks;
                let mut empty_vec = Vec::new();
                let lr = match layout_results.get(dom_id.inner) {
                    Some(s) => s,
                    None => continue,
                };

                let mut blacklisted_event_types = BTreeSet::new();

                // Run all callbacks (front to back)
                for ParentWithNodeDepth { depth: _, node_id } in
                    lr.styled_dom.non_leaf_nodes.as_ref().iter().rev()
                {
                    let parent_node_id = node_id;
                    for child_id in parent_node_id
                        .into_crate_internal()
                        .unwrap()
                        .az_children(&lr.styled_dom.node_hierarchy.as_container())
                    {
                        for (hit_test_item, event_filter) in
                            callbacks.get(&child_id).unwrap_or(&empty_vec)
                        {
                            if blacklisted_event_types.contains(&*event_filter) {
                                continue;
                            }

                            let mut new_focus = None;
                            let mut stop_propagation = false;

                            let mut callback_info = CallbackInfo::new(
                                /* layout_results: */ &layout_results,
                                /* renderer_resources: */ renderer_resources,
                                /* previous_window_state: */ &previous_window_state,
                                /* current_window_state: */ &full_window_state,
                                /* modifiable_window_state: */
                                &mut ret_modified_window_state,
                                /* gl_context, */ gl_context,
                                /* image_cache, */ image_cache,
                                /* system_fonts, */ system_fonts,
                                /* timers: */ &mut ret_timers,
                                /* threads: */ &mut ret_threads,
                                /* timers_removed: */ &mut ret_timers_removed,
                                /* threads_removed: */ &mut ret_threads_removed,
                                /* current_window_handle: */ raw_window_handle,
                                /* new_windows: */ &mut ret.windows_created,
                                /* system_callbacks */ system_callbacks,
                                /* stop_propagation: */ &mut stop_propagation,
                                /* focus_target: */ &mut new_focus,
                                /* words_changed_in_callbacks: */ &mut ret_words_changed,
                                /* images_changed_in_callbacks: */ &mut ret_images_changed,
                                /* image_masks_changed_in_callbacks: */
                                &mut ret_image_masks_changed,
                                /* css_properties_changed_in_callbacks: */
                                &mut ret_css_properties_changed,
                                /* current_scroll_states: */ scroll_states,
                                /* nodes_scrolled_in_callback: */
                                &mut ret_nodes_scrolled_in_callbacks,
                                /* hit_dom_node: */
                                DomNodeId {
                                    dom: *dom_id,
                                    node: NodeHierarchyItemId::from_crate_internal(Some(child_id)),
                                },
                                /* cursor_relative_to_item: */
                                hit_test_item
                                    .as_ref()
                                    .map(|hi| hi.point_relative_to_item)
                                    .into(),
                                /* cursor_in_viewport: */
                                hit_test_item.as_ref().map(|hi| hi.point_in_viewport).into(),
                            );

                            let callback_return = {
                                // get a MUTABLE reference to the RefAny inside of the DOM
                                let node_data_container = lr.styled_dom.node_data.as_container();
                                if let Some(callback_data) =
                                    node_data_container.get(child_id).and_then(|nd| {
                                        nd.callbacks
                                            .as_ref()
                                            .iter()
                                            .find(|i| i.event == *event_filter)
                                    })
                                {
                                    let mut callback_data_clone = callback_data.clone();
                                    // Invoke callback
                                    (callback_data_clone.callback.cb)(
                                        &mut callback_data_clone.data,
                                        &mut callback_info,
                                    )
                                } else {
                                    Update::DoNothing
                                }
                            };

                            ret.callbacks_update_screen.max_self(callback_return);

                            if let Some(new_focus) = new_focus.clone() {
                                new_focus_target = Some(new_focus);
                            }

                            if stop_propagation {
                                blacklisted_event_types.insert(event_filter.clone());
                            }
                        }
                    }
                }

                // run the callbacks for node ID 0
                loop {
                    for ((hit_test_item, event_filter), root_id) in lr
                        .styled_dom
                        .root
                        .into_crate_internal()
                        .map(|root_id| {
                            callbacks
                                .get(&root_id)
                                .unwrap_or(&empty_vec)
                                .iter()
                                .map(|item| (item, root_id))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                    {
                        if blacklisted_event_types.contains(&event_filter) {
                            break; // break out of loop
                        }

                        let mut new_focus = None;
                        let mut stop_propagation = false;

                        let mut callback_info = CallbackInfo::new(
                            /* layout_results: */ &layout_results,
                            /* renderer_resources: */ renderer_resources,
                            /* previous_window_state: */ &previous_window_state,
                            /* current_window_state: */ &full_window_state,
                            /* modifiable_window_state: */ &mut ret_modified_window_state,
                            /* gl_context, */ gl_context,
                            /* image_cache, */ image_cache,
                            /* system_fonts, */ system_fonts,
                            /* timers: */ &mut ret_timers,
                            /* threads: */ &mut ret_threads,
                            /* timers_removed: */ &mut ret_timers_removed,
                            /* threads_removed: */ &mut ret_threads_removed,
                            /* current_window_handle: */ raw_window_handle,
                            /* new_windows: */ &mut ret.windows_created,
                            /* system_callbacks */ system_callbacks,
                            /* stop_propagation: */ &mut stop_propagation,
                            /* focus_target: */ &mut new_focus,
                            /* words_changed_in_callbacks: */ &mut ret_words_changed,
                            /* images_changed_in_callbacks: */ &mut ret_images_changed,
                            /* image_masks_changed_in_callbacks: */
                            &mut ret_image_masks_changed,
                            /* css_properties_changed_in_callbacks: */
                            &mut ret_css_properties_changed,
                            /* current_scroll_states: */ scroll_states,
                            /* nodes_scrolled_in_callback: */
                            &mut ret_nodes_scrolled_in_callbacks,
                            /* hit_dom_node: */
                            DomNodeId {
                                dom: *dom_id,
                                node: NodeHierarchyItemId::from_crate_internal(Some(root_id)),
                            },
                            /* cursor_relative_to_item: */
                            hit_test_item
                                .as_ref()
                                .map(|hi| hi.point_relative_to_item)
                                .into(),
                            /* cursor_in_viewport: */
                            hit_test_item.as_ref().map(|hi| hi.point_in_viewport).into(),
                        );

                        let callback_return = {
                            // get a MUTABLE reference to the RefAny inside of the DOM
                            let node_data_container = lr.styled_dom.node_data.as_container();
                            if let Some(callback_data) =
                                node_data_container.get(root_id).and_then(|nd| {
                                    nd.callbacks
                                        .as_ref()
                                        .iter()
                                        .find(|i| i.event == *event_filter)
                                })
                            {
                                // Invoke callback
                                let mut callback_data_clone = callback_data.clone();
                                (callback_data_clone.callback.cb)(
                                    &mut callback_data_clone.data,
                                    &mut callback_info,
                                )
                            } else {
                                Update::DoNothing
                            }
                        };

                        ret.callbacks_update_screen.max_self(callback_return);

                        if let Some(new_focus) = new_focus.clone() {
                            new_focus_target = Some(new_focus);
                        }

                        if stop_propagation {
                            blacklisted_event_types.insert(event_filter.clone());
                        }
                    }

                    break;
                }
            }
        }

        // Scroll nodes from programmatic callbacks
        for (dom_id, callback_scrolled_nodes) in ret_nodes_scrolled_in_callbacks.iter() {
            let scrollable_nodes = &layout_results[dom_id.inner].scrollable_nodes;
            for (scroll_node_id, scroll_position) in callback_scrolled_nodes.iter() {
                let scroll_node = match scrollable_nodes.overflowing_nodes.get(&scroll_node_id) {
                    Some(s) => s,
                    None => continue,
                };

                modifiable_scroll_states.set_scroll_position(&scroll_node, *scroll_position);
                ret.should_scroll_render = true;
            }
        }

        // Resolve the new focus target
        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = ft.resolve(&layout_results, full_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
            }
        }

        if current_cursor != ret_modified_window_state.mouse_state.mouse_cursor_type {
            ret.cursor_changed = true;
        }

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_modified_window_state_unmodified {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        ret
    }
}

fn get_window_events(
    current_window_state: &FullWindowState,
    previous_window_state: &Option<FullWindowState>,
) -> Vec<WindowEventFilter> {
    use crate::window::{CursorPosition::*, WindowPosition};

    let mut events = Vec::new();

    let previous_window_state = match previous_window_state.as_ref() {
        Some(s) => s,
        None => return events,
    };

    // match mouse move events first since they are the most common

    match (
        previous_window_state.mouse_state.cursor_position,
        current_window_state.mouse_state.cursor_position,
    ) {
        (InWindow(_), OutOfWindow(_)) | (InWindow(_), Uninitialized) => {
            events.push(WindowEventFilter::MouseLeave);
        }
        (OutOfWindow(_), InWindow(_)) | (Uninitialized, InWindow(_)) => {
            events.push(WindowEventFilter::MouseEnter);
        }
        (InWindow(a), InWindow(b)) => {
            if a != b {
                events.push(WindowEventFilter::MouseOver);
            }
        }
        _ => {}
    }

    if current_window_state.mouse_state.mouse_down()
        && !previous_window_state.mouse_state.mouse_down()
    {
        events.push(WindowEventFilter::MouseDown);
    }

    if current_window_state.mouse_state.left_down && !previous_window_state.mouse_state.left_down {
        events.push(WindowEventFilter::LeftMouseDown);
    }

    if current_window_state.mouse_state.right_down && !previous_window_state.mouse_state.right_down
    {
        events.push(WindowEventFilter::RightMouseDown);
    }

    if current_window_state.mouse_state.middle_down
        && !previous_window_state.mouse_state.middle_down
    {
        events.push(WindowEventFilter::MiddleMouseDown);
    }

    if previous_window_state.mouse_state.mouse_down()
        && !current_window_state.mouse_state.mouse_down()
    {
        events.push(WindowEventFilter::MouseUp);
    }

    if previous_window_state.mouse_state.left_down && !current_window_state.mouse_state.left_down {
        events.push(WindowEventFilter::LeftMouseUp);
    }

    if previous_window_state.mouse_state.right_down && !current_window_state.mouse_state.right_down
    {
        events.push(WindowEventFilter::RightMouseUp);
    }

    if previous_window_state.mouse_state.middle_down
        && !current_window_state.mouse_state.middle_down
    {
        events.push(WindowEventFilter::MiddleMouseUp);
    }

    // resize, move, close events

    if current_window_state.flags.has_focus != previous_window_state.flags.has_focus {
        if current_window_state.flags.has_focus {
            events.push(WindowEventFilter::FocusReceived);
            events.push(WindowEventFilter::WindowFocusReceived);
        } else {
            events.push(WindowEventFilter::FocusLost);
            events.push(WindowEventFilter::WindowFocusLost);
        }
    }

    if current_window_state.size.dimensions != previous_window_state.size.dimensions
        || current_window_state.size.dpi != previous_window_state.size.dpi
    {
        events.push(WindowEventFilter::Resized);
    }

    match (
        current_window_state.position,
        previous_window_state.position,
    ) {
        (WindowPosition::Initialized(cur_pos), WindowPosition::Initialized(prev_pos)) => {
            if prev_pos != cur_pos {
                events.push(WindowEventFilter::Moved);
            }
        }
        (WindowPosition::Initialized(_), WindowPosition::Uninitialized) => {
            events.push(WindowEventFilter::Moved);
        }
        _ => {}
    }

    let about_to_close_equals = current_window_state.flags.is_about_to_close
        == previous_window_state.flags.is_about_to_close;
    if current_window_state.flags.is_about_to_close && !about_to_close_equals {
        events.push(WindowEventFilter::CloseRequested);
    }

    // scroll events

    let is_scroll_previous = previous_window_state.mouse_state.scroll_x.is_some()
        || previous_window_state.mouse_state.scroll_y.is_some();

    let is_scroll_now = current_window_state.mouse_state.scroll_x.is_some()
        || current_window_state.mouse_state.scroll_y.is_some();

    if !is_scroll_previous && is_scroll_now {
        events.push(WindowEventFilter::ScrollStart);
    }

    if is_scroll_now {
        events.push(WindowEventFilter::Scroll);
    }

    if is_scroll_previous && !is_scroll_now {
        events.push(WindowEventFilter::ScrollEnd);
    }

    // keyboard events
    let cur_vk_equal = current_window_state.keyboard_state.current_virtual_keycode
        == previous_window_state.keyboard_state.current_virtual_keycode;
    let cur_char_equal = current_window_state.keyboard_state.current_char
        == previous_window_state.keyboard_state.current_char;

    if !cur_vk_equal
        && previous_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_none()
        && current_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_some()
    {
        events.push(WindowEventFilter::VirtualKeyDown);
    }

    if !cur_char_equal && current_window_state.keyboard_state.current_char.is_some() {
        events.push(WindowEventFilter::TextInput);
    }

    if !cur_vk_equal
        && previous_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_some()
        && current_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_none()
    {
        events.push(WindowEventFilter::VirtualKeyUp);
    }

    // misc events

    let hovered_file_equals =
        previous_window_state.hovered_file == current_window_state.hovered_file;
    if previous_window_state.hovered_file.is_none()
        && current_window_state.hovered_file.is_some()
        && !hovered_file_equals
    {
        events.push(WindowEventFilter::HoveredFile);
    }

    if previous_window_state.hovered_file.is_some() && current_window_state.hovered_file.is_none() {
        if current_window_state.dropped_file.is_some() {
            events.push(WindowEventFilter::DroppedFile);
        } else {
            events.push(WindowEventFilter::HoveredFileCancelled);
        }
    }

    if current_window_state.theme != previous_window_state.theme {
        events.push(WindowEventFilter::ThemeChanged);
    }

    events
}

fn get_hover_events(input: &[WindowEventFilter]) -> Vec<HoverEventFilter> {
    input
        .iter()
        .filter_map(|window_event| window_event.to_hover_event_filter())
        .collect()
}

fn get_focus_events(input: &[HoverEventFilter]) -> Vec<FocusEventFilter> {
    input
        .iter()
        .filter_map(|hover_event| hover_event.to_focus_event_filter())
        .collect()
}
