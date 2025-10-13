//! Scroll state management for layout
//!
//! Handles scroll positions and will eventually handle scrollbar IDs
//! and mapping hit tests back to scrolling commands.

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    hit_test::ScrollPosition,
};

/// Scroll state manager for a window
#[derive(Debug, Clone, Default)]
pub struct ScrollStates {
    /// Maps (DomId, NodeId) to their scroll positions
    states: BTreeMap<(DomId, NodeId), ScrollPosition>,
}

impl ScrollStates {
    /// Create a new empty scroll states manager
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
        }
    }

    /// Get the scroll states for a specific DOM
    pub fn get_scroll_states_for_dom(&self, dom_id: DomId) -> BTreeMap<NodeId, ScrollPosition> {
        self.states
            .iter()
            .filter_map(|((d, node_id), scroll_pos)| {
                if *d == dom_id {
                    Some((*node_id, scroll_pos.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Set the scroll position for a specific node
    pub fn set(&mut self, dom_id: DomId, node_id: NodeId, position: ScrollPosition) {
        self.states.insert((dom_id, node_id), position);
    }

    /// Remove the scroll position for a specific node
    pub fn remove(&mut self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        self.states.remove(&(dom_id, node_id))
    }

    /// Get the scroll position for a specific node (taking two args to match ScrollStates API)
    pub fn get(&self, dom_id: DomId, node_id: NodeId) -> Option<&ScrollPosition> {
        self.states.get(&(dom_id, node_id))
    }

    /// Insert a scroll position (compatibility method)
    pub fn insert(&mut self, key: (DomId, NodeId), value: ScrollPosition) {
        self.states.insert(key, value);
    }

    /// Clear all scroll states
    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Get all scroll states
    pub fn all(&self) -> &BTreeMap<(DomId, NodeId), ScrollPosition> {
        &self.states
    }

    /// Get mutable reference to all scroll states
    pub fn all_mut(&mut self) -> &mut BTreeMap<(DomId, NodeId), ScrollPosition> {
        &mut self.states
    }

    /// Merge scroll states from another source
    pub fn merge(&mut self, other: BTreeMap<(DomId, NodeId), ScrollPosition>) {
        for (key, value) in other {
            self.states.insert(key, value);
        }
    }
}

// TODO: Add scrollbar ID management
// TODO: Add hit test to scroll command mapping
// TODO: Add scrollbar rendering state
