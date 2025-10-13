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
    callbacks::{DocumentId, DomNodeId, HitTestItem, ScrollPosition, Update},
    dom::{EventFilter, FocusEventFilter, HoverEventFilter, NotEventFilter, WindowEventFilter},
    gl::OptionGlContextPtr,
    gpu::GpuEventChanges,
    id::NodeId,
    resources::{ImageCache, RendererResources},
    styled_dom::{ChangedCssProperty, DomId, NodeHierarchyItemId},
    window::{FullHitTest, RawWindowHandle, ScrollStates},
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

#[derive(Debug, Clone, PartialEq)]
pub struct FocusChange {
    pub old: Option<DomNodeId>,
    pub new: Option<DomNodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallbackToCall {
    pub node_id: NodeId,
    pub hit_test_item: Option<HitTestItem>,
    pub event_filter: EventFilter,
}

pub fn get_hover_events(input: &[WindowEventFilter]) -> Vec<HoverEventFilter> {
    input
        .iter()
        .filter_map(|window_event| window_event.to_hover_event_filter())
        .collect()
}

pub fn get_focus_events(input: &[HoverEventFilter]) -> Vec<FocusEventFilter> {
    input
        .iter()
        .filter_map(|hover_event| hover_event.to_focus_event_filter())
        .collect()
}
