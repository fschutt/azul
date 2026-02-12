//! Hover state management for tracking mouse and touch hover history
//!
//! The HoverManager records hit test results for multiple input points
//! (mouse, touch, pen) over multiple frames to enable gesture detection
//! (like DragStart) that requires analyzing hover patterns over time
//! rather than just the current frame.

use std::collections::{BTreeMap, VecDeque};

use crate::hit_test::FullHitTest;

/// Maximum number of frames to keep in hover history
const MAX_HOVER_HISTORY: usize = 5;

/// Identifier for an input point (mouse, touch, pen, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputPointId {
    /// Mouse cursor
    Mouse,
    /// Touch point with unique ID (from TouchEvent.id)
    Touch(u64),
}

/// Manages hover state history for all input points
///
/// Records hit test results for mouse and touch inputs over multiple frames:
/// - DragStart detection (requires movement threshold over multiple frames)
/// - Hover-over event detection
/// - Multi-touch gesture detection
/// - Input path analysis
///
/// The manager maintains a separate history for each active input point.
#[derive(Debug, Clone, PartialEq)]
pub struct HoverManager {
    /// Hit test history for each input point
    /// Each point has its own ring buffer of the last N frames
    hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>,
}

impl HoverManager {
    /// Create a new empty HoverManager
    pub fn new() -> Self {
        Self {
            hover_histories: BTreeMap::new(),
        }
    }

    /// Push a new hit test result for a specific input point
    ///
    /// The most recent result is always at index 0 for that input point.
    /// If the history is full, the oldest frame is dropped.
    pub fn push_hit_test(&mut self, input_id: InputPointId, hit_test: FullHitTest) {
        let history = self
            .hover_histories
            .entry(input_id)
            .or_insert_with(|| VecDeque::with_capacity(MAX_HOVER_HISTORY));

        // Add to front (most recent)
        history.push_front(hit_test);

        // Remove oldest if we exceed the limit
        if history.len() > MAX_HOVER_HISTORY {
            history.pop_back();
        }
    }

    /// Remove an input point's history (e.g., when touch ends)
    pub fn remove_input_point(&mut self, input_id: &InputPointId) {
        self.hover_histories.remove(input_id);
    }

    /// Get the most recent hit test result for an input point
    ///
    /// Returns None if no hit tests have been recorded for this input point.
    pub fn get_current(&self, input_id: &InputPointId) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.front())
    }

    /// Get the most recent mouse cursor hit test (convenience method)
    pub fn get_current_mouse(&self) -> Option<&FullHitTest> {
        self.get_current(&InputPointId::Mouse)
    }

    /// Get the hit test result from N frames ago for an input point
    // (0 = current frame)
    ///
    /// Returns None if the requested frame is not in history.
    pub fn get_frame(&self, input_id: &InputPointId, frames_ago: usize) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.get(frames_ago))
    }

    /// Get the entire hover history for an input point (most recent first)
    pub fn get_history(&self, input_id: &InputPointId) -> Option<&VecDeque<FullHitTest>> {
        self.hover_histories.get(input_id)
    }

    /// Get all currently tracked input points
    pub fn get_active_input_points(&self) -> Vec<InputPointId> {
        self.hover_histories.keys().copied().collect()
    }

    /// Get the number of frames in history for an input point
    pub fn frame_count(&self, input_id: &InputPointId) -> usize {
        self.hover_histories
            .get(input_id)
            .map(|h| h.len())
            .unwrap_or(0)
    }

    /// Clear all hover history for all input points
    pub fn clear(&mut self) {
        self.hover_histories.clear();
    }

    /// Clear history for a specific input point
    pub fn clear_input_point(&mut self, input_id: &InputPointId) {
        if let Some(history) = self.hover_histories.get_mut(input_id) {
            history.clear();
        }
    }

    /// Check if we have enough frames for gesture detection on an input point
    ///
    /// DragStart detection requires analyzing movement over multiple frames.
    /// This returns true if we have at least 2 frames of history.
    pub fn has_sufficient_history_for_gestures(&self, input_id: &InputPointId) -> bool {
        self.frame_count(input_id) >= 2
    }

    /// Check if any input point has enough history for gesture detection
    pub fn any_has_sufficient_history_for_gestures(&self) -> bool {
        self.hover_histories
            .iter()
            .any(|(_, history)| history.len() >= 2)
    }

    /// Get the deepest hovered node from the current mouse hit test.
    ///
    /// Returns the NodeId of the most specific (deepest in DOM tree) node
    /// that the mouse cursor is currently over, or None if not hovering anything.
    pub fn current_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let current = self.get_current_mouse()?;
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = current.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Get the deepest hovered node from the previous frame's mouse hit test.
    ///
    /// Returns the NodeId from one frame ago, or None if not hovering anything
    /// or no previous frame exists.
    pub fn previous_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let history = self.hover_histories.get(&InputPointId::Mouse)?;
        let previous = history.get(1)?; // index 1 = one frame ago
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = previous.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Remap NodeIds in all hover histories after DOM reconciliation.
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates
    /// all stored NodeIds in hover histories using the old→new mapping from
    /// reconciliation. Nodes not found in the map are removed from hit tests.
    pub fn remap_node_ids(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id_map: &std::collections::BTreeMap<azul_core::id::NodeId, azul_core::id::NodeId>,
    ) {
        for history in self.hover_histories.values_mut() {
            for hit_test in history.iter_mut() {
                if let Some(ht) = hit_test.hovered_nodes.get_mut(&dom_id) {
                    // Remap regular_hit_test_nodes
                    let old_regular: Vec<_> = ht.regular_hit_test_nodes.keys().cloned().collect();
                    let mut new_regular = std::collections::BTreeMap::new();
                    for old_nid in old_regular {
                        if let Some(&new_nid) = node_id_map.get(&old_nid) {
                            if let Some(item) = ht.regular_hit_test_nodes.remove(&old_nid) {
                                new_regular.insert(new_nid, item);
                            }
                        }
                    }
                    ht.regular_hit_test_nodes = new_regular;

                    // Remap scroll_hit_test_nodes
                    let old_scroll: Vec<_> = ht.scroll_hit_test_nodes.keys().cloned().collect();
                    let mut new_scroll = std::collections::BTreeMap::new();
                    for old_nid in old_scroll {
                        if let Some(&new_nid) = node_id_map.get(&old_nid) {
                            if let Some(item) = ht.scroll_hit_test_nodes.remove(&old_nid) {
                                new_scroll.insert(new_nid, item);
                            }
                        }
                    }
                    ht.scroll_hit_test_nodes = new_scroll;

                    // Remap cursor_hit_test_nodes
                    let old_cursor: Vec<_> = ht.cursor_hit_test_nodes.keys().cloned().collect();
                    let mut new_cursor = std::collections::BTreeMap::new();
                    for old_nid in old_cursor {
                        if let Some(&new_nid) = node_id_map.get(&old_nid) {
                            if let Some(item) = ht.cursor_hit_test_nodes.remove(&old_nid) {
                                new_cursor.insert(new_nid, item);
                            }
                        }
                    }
                    ht.cursor_hit_test_nodes = new_cursor;

                    // Remap scrollbar_hit_test_nodes (ScrollbarHitId contains NodeId)
                    let old_sb: Vec<_> = ht.scrollbar_hit_test_nodes.keys().cloned().collect();
                    let mut new_sb = std::collections::BTreeMap::new();
                    for old_key in old_sb {
                        let new_key = remap_scrollbar_hit_id(&old_key, dom_id, node_id_map);
                        if let Some(item) = ht.scrollbar_hit_test_nodes.remove(&old_key) {
                            new_sb.insert(new_key, item);
                        }
                    }
                    ht.scrollbar_hit_test_nodes = new_sb;
                }
            }
        }
    }
}

impl Default for HoverManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Remap a ScrollbarHitId's NodeId using the reconciliation map.
/// If the NodeId's DomId doesn't match, or the NodeId isn't in the map, returns unchanged.
fn remap_scrollbar_hit_id(
    id: &azul_core::hit_test::ScrollbarHitId,
    dom_id: azul_core::dom::DomId,
    node_id_map: &std::collections::BTreeMap<azul_core::id::NodeId, azul_core::id::NodeId>,
) -> azul_core::hit_test::ScrollbarHitId {
    use azul_core::hit_test::ScrollbarHitId;
    match id {
        ScrollbarHitId::VerticalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalTrack(*d, *node_id_map.get(n).unwrap_or(n))
        }
        ScrollbarHitId::VerticalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalThumb(*d, *node_id_map.get(n).unwrap_or(n))
        }
        ScrollbarHitId::HorizontalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalTrack(*d, *node_id_map.get(n).unwrap_or(n))
        }
        ScrollbarHitId::HorizontalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalThumb(*d, *node_id_map.get(n).unwrap_or(n))
        }
        other => other.clone(),
    }
}
