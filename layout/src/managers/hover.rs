//! Hover state management for tracking mouse and touch hover history
//!
//! The `HoverManager` records hit test results for multiple input points
//! (mouse, touch, pen) over multiple frames to enable gesture detection
//! (like `DragStart`) that requires analyzing hover patterns over time
//! rather than just the current frame.

use std::collections::{BTreeMap, VecDeque};

use crate::hit_test::FullHitTest;

/// Maximum number of frames to keep in hover history
const MAX_HOVER_HISTORY: usize = 5;

/// Pick the front-most deepest hovered node across all hit DOMs.
///
/// Iterates DOMs from highest `DomId` (most-nested child, composited on top)
/// to lowest and returns the deepest node (last in `NodeId` order) of the first
/// DOM that actually has a regular hit. See [`HoverManager::current_hover_node_full`].
fn deepest_node_across_doms(ht: &FullHitTest) -> Option<azul_core::dom::DomNodeId> {
    for (dom_id, hit) in ht.hovered_nodes.iter().rev() {
        if let Some(node_id) = hit.regular_hit_test_nodes.keys().last().copied() {
            return Some(azul_core::dom::DomNodeId {
                dom: *dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    node_id,
                )),
            });
        }
    }
    None
}

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
/// - `DragStart` detection (requires movement threshold over multiple frames)
/// - Hover-over event detection
/// - Multi-touch gesture detection
/// - Input path analysis
///
/// The manager maintains a separate history for each active input point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverManager {
    /// Hit test history for each input point
    /// Each point has its own ring buffer of the last N frames
    hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>,
}

impl HoverManager {
    /// Create a new empty `HoverManager`
    #[must_use] pub const fn new() -> Self {
        Self {
            hover_histories: BTreeMap::new(),
        }
    }

    /// (input points, total history entries across all points). Used by
    /// `AZ_E2E_TEST` to watch for unbounded growth.
    #[must_use] pub fn debug_counts(&self) -> (usize, usize) {
        let points = self.hover_histories.len();
        let total: usize = self.hover_histories.values().map(VecDeque::len).sum();
        (points, total)
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
    #[must_use] pub fn get_current(&self, input_id: &InputPointId) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.front())
    }

    /// Get the most recent mouse cursor hit test (convenience method)
    #[must_use] pub fn get_current_mouse(&self) -> Option<&FullHitTest> {
        self.get_current(&InputPointId::Mouse)
    }

    /// Get the hit test result from N frames ago for an input point
    /// (0 = current frame)
    ///
    /// Returns None if the requested frame is not in history.
    #[must_use] pub fn get_frame(&self, input_id: &InputPointId, frames_ago: usize) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.get(frames_ago))
    }

    /// Get the entire hover history for an input point (most recent first)
    #[must_use] pub fn get_history(&self, input_id: &InputPointId) -> Option<&VecDeque<FullHitTest>> {
        self.hover_histories.get(input_id)
    }

    /// Get all currently tracked input points
    #[must_use] pub fn get_active_input_points(&self) -> Vec<InputPointId> {
        self.hover_histories.keys().copied().collect()
    }

    /// Get the number of frames in history for an input point
    #[must_use] pub fn frame_count(&self, input_id: &InputPointId) -> usize {
        self.hover_histories
            .get(input_id)
            .map_or(0, VecDeque::len)
    }

    /// Purge every recorded hit-test entry for `dom_id` across all input
    /// points and all history frames.
    ///
    /// Called when a `VirtualView` child DOM is rebuilt IN PLACE (fresh `NodeIds`,
    /// no reconcile mapping — e.g. a `MapWidget` pan rebuilding the tile grid):
    /// the recorded hits for that DOM reference the OLD generation's `NodeIds`,
    /// and consumers that resolve them against the NEW styled DOM read out of
    /// bounds (the `hit_test.rs` cursor panic: "len is 25 but the index is 27")
    /// or target the wrong node. Unlike incremental reconciles there is no
    /// `NodeId` map to `remap` with, so the only safe option is to forget that
    /// DOM's hits; the next pointer move re-populates them from a fresh
    /// hit test.
    pub fn purge_dom(&mut self, dom_id: &azul_core::dom::DomId) {
        for history in self.hover_histories.values_mut() {
            for frame in history.iter_mut() {
                frame.hovered_nodes.remove(dom_id);
            }
        }
    }

    /// Clear all hover history for all input points
    pub fn clear(&mut self) {
        self.hover_histories.clear();
    }

    /// Clear history for a specific input point
    pub(crate) fn clear_input_point(&mut self, input_id: &InputPointId) {
        if let Some(history) = self.hover_histories.get_mut(input_id) {
            history.clear();
        }
    }

    /// Check if we have enough frames for gesture detection on an input point
    ///
    /// `DragStart` detection requires analyzing movement over multiple frames.
    /// This returns true if we have at least 2 frames of history.
    #[must_use] pub fn has_sufficient_history_for_gestures(&self, input_id: &InputPointId) -> bool {
        self.frame_count(input_id) >= 2
    }

    /// Check if any input point has enough history for gesture detection
    #[must_use] pub fn any_has_sufficient_history_for_gestures(&self) -> bool {
        self.hover_histories
            .iter()
            .any(|(_, history)| history.len() >= 2)
    }

    /// Get the deepest hovered node from the current mouse hit test.
    ///
    /// Returns the `NodeId` of the most specific (deepest in DOM tree) node
    /// that the mouse cursor is currently over, or None if not hovering anything.
    ///
    /// NOTE: Assumes single-DOM architecture (uses `DomId { inner: 0 }`).
    #[must_use] pub fn current_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let current = self.get_current_mouse()?;
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = current.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Get the deepest hovered node from the previous frame's mouse hit test.
    ///
    /// Returns the `NodeId` from one frame ago, or None if not hovering anything
    /// or no previous frame exists.
    ///
    /// NOTE: Assumes single-DOM architecture (uses `DomId { inner: 0 }`).
    #[must_use] pub fn previous_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let history = self.hover_histories.get(&InputPointId::Mouse)?;
        let previous = history.get(1)?; // index 1 = one frame ago
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = previous.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Multi-DOM aware: the deepest hovered node across ALL hit DOMs (current
    /// frame). Returns a full `DomNodeId` so events can target `VirtualView` /
    /// iframe child DOMs, not just the root.
    ///
    /// Selection rule: prefer the most-nested DOM that was hit. Child DOMs
    /// (`VirtualView` / iframe content) always have higher `DomId`s than their
    /// host and are composited on top of it, so the highest hit `DomId` is the
    /// front-most surface. Within that DOM the deepest node (last in `NodeId`
    /// order) is the W3C event target; bubbling then reaches ancestor handlers.
    ///
    /// For single-DOM apps only `DomId 0` is ever hit, so this is equivalent to
    /// [`current_hover_node`] wrapped in `DomId { inner: 0 }`.
    #[must_use] pub fn current_hover_node_full(&self) -> Option<azul_core::dom::DomNodeId> {
        deepest_node_across_doms(self.get_current_mouse()?)
    }

    /// Multi-DOM aware counterpart of [`previous_hover_node`] (one frame ago).
    #[must_use] pub fn previous_hover_node_full(&self) -> Option<azul_core::dom::DomNodeId> {
        let history = self.hover_histories.get(&InputPointId::Mouse)?;
        deepest_node_across_doms(history.get(1)?)
    }

}

impl crate::managers::NodeIdRemap for HoverManager {
    /// Remap `NodeIds` in all hover histories after DOM reconciliation.
    ///
    /// Hits on unmounted nodes are dropped (they cannot be hovered any more) —
    /// keeping them would make the hover history describe a node that no longer
    /// exists at that index.
    fn remap_node_ids(&mut self, dom_id: azul_core::dom::DomId, map: &crate::managers::NodeIdMap) {
        let node_id_map = map.as_btree_map();
        for history in self.hover_histories.values_mut() {
            for hit_test in history.iter_mut() {
                if let Some(ht) = hit_test.hovered_nodes.get_mut(&dom_id) {
                    crate::managers::remap_keys(&mut ht.regular_hit_test_nodes, map);
                    crate::managers::remap_keys(&mut ht.scroll_hit_test_nodes, map);
                    crate::managers::remap_keys(&mut ht.cursor_hit_test_nodes, map);

                    // Remap scrollbar_hit_test_nodes (ScrollbarHitId contains NodeId)
                    let old_sb: Vec<_> = ht.scrollbar_hit_test_nodes.keys().copied().collect();
                    let mut new_sb = BTreeMap::new();
                    for old_key in old_sb {
                        let Some(new_key) = remap_scrollbar_hit_id(&old_key, dom_id, node_id_map)
                        else {
                            // node unmounted — drop the scrollbar hit
                            ht.scrollbar_hit_test_nodes.remove(&old_key);
                            continue;
                        };
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

/// Remap a `ScrollbarHitId`'s `NodeId` using the reconciliation map.
/// `None` = the node was unmounted, so the hit must be dropped.
/// A `ScrollbarHitId` for a different `DomId` is returned unchanged.
fn remap_scrollbar_hit_id(
    id: &azul_core::hit_test::ScrollbarHitId,
    dom_id: azul_core::dom::DomId,
    node_id_map: &BTreeMap<azul_core::id::NodeId, azul_core::id::NodeId>,
) -> Option<azul_core::hit_test::ScrollbarHitId> {
    use azul_core::hit_test::ScrollbarHitId;
    Some(match id {
        ScrollbarHitId::VerticalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalTrack(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::VerticalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalThumb(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::HorizontalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalTrack(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::HorizontalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalThumb(*d, *node_id_map.get(n)?)
        }
        other => *other,
    })
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{DomId, DomNodeId, ScrollbarOrientation},
        geom::LogicalPosition,
        hit_test::{
            CursorHitTestItem, CursorType, HitTest, HitTestItem, OverflowingScrollNode,
            ScrollHitTestItem, ScrollbarHitId, ScrollbarHitTestItem,
        },
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
    };

    use super::*;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    // ---------------------------------------------------------------- fixtures

    fn hit_item(depth: u32) -> HitTestItem {
        HitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: LogicalPosition::zero(),
            is_focusable: false,
            is_virtual_view_hit: None,
            hit_depth: depth,
        }
    }

    fn scroll_item() -> ScrollHitTestItem {
        ScrollHitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: LogicalPosition::zero(),
            scroll_node: OverflowingScrollNode::default(),
        }
    }

    fn cursor_item() -> CursorHitTestItem {
        CursorHitTestItem {
            cursor_type: CursorType::Text,
            hit_depth: 0,
            point_in_viewport: LogicalPosition::zero(),
        }
    }

    fn scrollbar_item() -> ScrollbarHitTestItem {
        ScrollbarHitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: LogicalPosition::zero(),
            orientation: ScrollbarOrientation::Vertical,
        }
    }

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    /// A `FullHitTest` where every `(dom, &[node..])` entry is a set of regular hits.
    /// Node ids are inserted in the given (deliberately unsorted) order.
    fn hits(entries: &[(usize, &[usize])]) -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        for (dom_inner, nodes) in entries {
            let ht = full
                .hovered_nodes
                .entry(dom(*dom_inner))
                .or_insert_with(HitTest::empty);
            for n in *nodes {
                ht.regular_hit_test_nodes
                    .insert(NodeId::new(*n), hit_item(0));
            }
        }
        full
    }

    /// `DomNodeId` for `(dom, node)`, matching what the hover getters return.
    fn dom_node(dom_inner: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: dom(dom_inner),
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    /// A manager whose mouse history is `frames` (pushed oldest-first, so the
    /// LAST element ends up at index 0 = current).
    fn mouse_history(frames: Vec<FullHitTest>) -> HoverManager {
        let mut hm = HoverManager::new();
        for f in frames {
            hm.push_hit_test(InputPointId::Mouse, f);
        }
        hm
    }

    // ------------------------------------------- deepest_node_across_doms (other)

    #[test]
    fn deepest_node_across_doms_empty_returns_none() {
        assert_eq!(deepest_node_across_doms(&FullHitTest::empty(None)), None);
    }

    #[test]
    fn deepest_node_across_doms_uses_nodeid_order_not_insertion_order() {
        // Inserted 2, 9, 7 — BTreeMap key order makes 9 the deepest regardless.
        let ht = hits(&[(0, &[2, 9, 7])]);
        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(0, 9)));
    }

    #[test]
    fn deepest_node_across_doms_prefers_highest_dom_even_if_its_node_is_shallower() {
        // dom 0 has the deeper NodeId (99) but dom 3 is composited on top.
        let ht = hits(&[(0, &[99]), (3, &[1])]);
        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(3, 1)));
    }

    #[test]
    fn deepest_node_across_doms_skips_dom_with_no_regular_hits() {
        // dom 5 is "hit" but only in the scroll/cursor/scrollbar maps — the
        // front-most DOM with a REGULAR hit (dom 0) must win instead.
        let mut ht = hits(&[(0, &[4])]);
        let mut empty_regular = HitTest::empty();
        empty_regular
            .scroll_hit_test_nodes
            .insert(NodeId::new(1), scroll_item());
        empty_regular
            .cursor_hit_test_nodes
            .insert(NodeId::new(1), cursor_item());
        empty_regular.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalThumb(dom(5), NodeId::new(1)),
            scrollbar_item(),
        );
        ht.hovered_nodes.insert(dom(5), empty_regular);

        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(0, 4)));
    }

    #[test]
    fn deepest_node_across_doms_all_doms_empty_returns_none() {
        let mut ht = FullHitTest::empty(None);
        ht.hovered_nodes.insert(dom(0), HitTest::empty());
        ht.hovered_nodes.insert(dom(usize::MAX), HitTest::empty());
        assert_eq!(deepest_node_across_doms(&ht), None);
    }

    #[test]
    fn deepest_node_across_doms_extreme_ids_survive_the_nodeid_encoding() {
        // usize::MAX - 1 is the largest NodeId that survives the 1-based
        // `NodeHierarchyItemId` encode (n + 1) without wrapping.
        let max_node = usize::MAX - 1;
        let ht = hits(&[(usize::MAX, &[0, max_node])]);
        let got = deepest_node_across_doms(&ht).expect("a hit exists");
        assert_eq!(got.dom, dom(usize::MAX));
        // The DomNodeId must decode back to exactly the NodeId that was hit.
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(max_node)));
    }

    // ------------------------------------------------- new / Default (constructor)

    #[test]
    fn new_manager_is_empty_and_every_getter_is_none_or_zero() {
        let hm = HoverManager::new();
        let mouse = InputPointId::Mouse;

        assert_eq!(hm.debug_counts(), (0, 0));
        assert!(hm.get_active_input_points().is_empty());
        assert!(hm.get_current(&mouse).is_none());
        assert!(hm.get_current_mouse().is_none());
        assert!(hm.get_history(&mouse).is_none());
        assert_eq!(hm.frame_count(&mouse), 0);
        assert!(!hm.has_sufficient_history_for_gestures(&mouse));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
        // Frame lookups on an unknown point must not panic at any index.
        assert!(hm.get_frame(&mouse, 0).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX).is_none());
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(HoverManager::default(), HoverManager::new());
    }

    // ------------------------------------------------- push_hit_test (numeric)

    #[test]
    fn push_hit_test_index_zero_is_the_newest_frame() {
        let hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);

        assert_eq!(hm.frame_count(&InputPointId::Mouse), 2);
        assert_eq!(hm.get_frame(&InputPointId::Mouse, 0), Some(&hits(&[(0, &[2])])));
        assert_eq!(hm.get_frame(&InputPointId::Mouse, 1), Some(&hits(&[(0, &[1])])));
        assert_eq!(hm.get_current_mouse(), Some(&hits(&[(0, &[2])])));
    }

    #[test]
    fn push_hit_test_ring_buffer_never_exceeds_max_hover_history() {
        let mut hm = HoverManager::new();
        for i in 0..1000_usize {
            hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[i])]));
        }

        assert_eq!(hm.frame_count(&InputPointId::Mouse), MAX_HOVER_HISTORY);
        assert_eq!(hm.debug_counts(), (1, MAX_HOVER_HISTORY));
        // The retained window is the LAST MAX_HOVER_HISTORY pushes, newest first.
        for ago in 0..MAX_HOVER_HISTORY {
            assert_eq!(
                hm.get_frame(&InputPointId::Mouse, ago),
                Some(&hits(&[(0, &[999 - ago])])),
                "frame {ago} frames ago"
            );
        }
        // Anything older was dropped.
        assert!(hm.get_frame(&InputPointId::Mouse, MAX_HOVER_HISTORY).is_none());
    }

    #[test]
    fn get_frame_out_of_range_index_returns_none_without_overflow() {
        let hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let mouse = InputPointId::Mouse;

        assert!(hm.get_frame(&mouse, 0).is_some());
        assert!(hm.get_frame(&mouse, 1).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX / 2).is_none());
        // Unknown input point at a huge index is still just None.
        assert!(hm.get_frame(&InputPointId::Touch(u64::MAX), usize::MAX).is_none());
    }

    #[test]
    fn touch_ids_at_u64_boundaries_are_distinct_histories() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Touch(u64::MIN), hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(u64::MAX), hits(&[(0, &[2])]));
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[3])]));

        assert_eq!(hm.debug_counts(), (3, 3));
        assert_eq!(
            hm.get_current(&InputPointId::Touch(u64::MIN)),
            Some(&hits(&[(0, &[1])]))
        );
        assert_eq!(
            hm.get_current(&InputPointId::Touch(u64::MAX)),
            Some(&hits(&[(0, &[2])]))
        );
        assert_eq!(hm.get_current_mouse(), Some(&hits(&[(0, &[3])])));
        // Ord derive: Mouse sorts before every Touch, touches sort by id.
        assert_eq!(
            hm.get_active_input_points(),
            vec![
                InputPointId::Mouse,
                InputPointId::Touch(0),
                InputPointId::Touch(u64::MAX),
            ]
        );
    }

    #[test]
    fn debug_counts_stays_bounded_under_a_flood_of_points_and_frames() {
        let mut hm = HoverManager::new();
        for point in 0..100_u64 {
            for frame in 0..50_usize {
                hm.push_hit_test(InputPointId::Touch(point), hits(&[(0, &[frame])]));
            }
        }
        // 100 points, each capped at MAX_HOVER_HISTORY frames — no unbounded growth.
        assert_eq!(hm.debug_counts(), (100, 100 * MAX_HOVER_HISTORY));
    }

    #[test]
    fn push_hit_test_stores_the_value_verbatim_including_focused_node() {
        let focused = dom_node(0, 7);
        let mut ht = FullHitTest::empty(Some(focused));
        ht.hovered_nodes.insert(dom(0), HitTest::empty());

        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, ht.clone());

        assert_eq!(hm.get_current_mouse(), Some(&ht));
        assert_eq!(
            hm.get_current_mouse().map(|h| h.focused_node),
            Some(Some(focused).into())
        );
        // A hovered DOM with zero hits is still "no hovered node".
        assert!(hm.current_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
    }

    #[test]
    fn get_history_returns_all_frames_newest_first() {
        let hm = mouse_history(vec![
            hits(&[(0, &[1])]),
            hits(&[(0, &[2])]),
            hits(&[(0, &[3])]),
        ]);
        let history = hm.get_history(&InputPointId::Mouse).expect("history exists");

        assert_eq!(history.len(), 3);
        assert_eq!(history[0], hits(&[(0, &[3])]));
        assert_eq!(history[2], hits(&[(0, &[1])]));
        assert!(hm.get_history(&InputPointId::Touch(0)).is_none());
    }

    // --------------------------------------- remove / clear / clear_input_point

    #[test]
    fn remove_absent_input_point_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.remove_input_point(&InputPointId::Touch(0));
        hm.remove_input_point(&InputPointId::Touch(u64::MAX));

        assert_eq!(hm, before);
    }

    #[test]
    fn remove_input_point_only_drops_the_target_point() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(3), hits(&[(0, &[2])]));

        hm.remove_input_point(&InputPointId::Touch(3));

        assert_eq!(hm.debug_counts(), (1, 1));
        assert_eq!(hm.get_active_input_points(), vec![InputPointId::Mouse]);
        assert!(hm.get_current(&InputPointId::Touch(3)).is_none());
        assert_eq!(hm.frame_count(&InputPointId::Touch(3)), 0);
        assert!(hm.get_current_mouse().is_some());
    }

    #[test]
    fn remove_then_push_restarts_the_history_from_scratch() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);
        assert!(hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));

        hm.remove_input_point(&InputPointId::Mouse);
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[3])]));

        assert_eq!(hm.frame_count(&InputPointId::Mouse), 1);
        assert!(!hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));
        assert!(hm.previous_hover_node().is_none());
    }

    #[test]
    fn clear_drops_every_point() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(9), hits(&[(0, &[2])]));

        hm.clear();

        assert_eq!(hm, HoverManager::new());
        assert_eq!(hm.debug_counts(), (0, 0));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        // Clearing twice is still fine.
        hm.clear();
        assert_eq!(hm.debug_counts(), (0, 0));
    }

    #[test]
    fn clear_input_point_empties_history_but_keeps_the_point_registered() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);

        hm.clear_input_point(&InputPointId::Mouse);

        // The point remains a key with an EMPTY deque (unlike remove_input_point).
        assert_eq!(hm.debug_counts(), (1, 0));
        assert_eq!(hm.get_active_input_points(), vec![InputPointId::Mouse]);
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 0);
        assert!(hm.get_current_mouse().is_none());
        assert!(hm.get_history(&InputPointId::Mouse).is_some());
        assert!(!hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
    }

    #[test]
    fn clear_input_point_on_an_absent_point_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.clear_input_point(&InputPointId::Touch(u64::MAX));

        assert_eq!(hm, before);
    }

    // ------------------------------------------------------------- predicates

    #[test]
    fn has_sufficient_history_needs_at_least_two_frames() {
        let mouse = InputPointId::Mouse;
        let mut hm = HoverManager::new();
        assert!(!hm.has_sufficient_history_for_gestures(&mouse));

        hm.push_hit_test(mouse, hits(&[(0, &[1])]));
        assert!(!hm.has_sufficient_history_for_gestures(&mouse), "1 frame is not enough");

        hm.push_hit_test(mouse, hits(&[(0, &[2])]));
        assert!(hm.has_sufficient_history_for_gestures(&mouse), "2 frames is the threshold");

        for i in 0..10 {
            hm.push_hit_test(mouse, hits(&[(0, &[i])]));
        }
        assert!(hm.has_sufficient_history_for_gestures(&mouse), "stays true when saturated");
    }

    #[test]
    fn any_has_sufficient_history_is_an_or_across_points() {
        let mut hm = HoverManager::new();
        // Three points with one frame each => still false.
        for id in [
            InputPointId::Mouse,
            InputPointId::Touch(0),
            InputPointId::Touch(u64::MAX),
        ] {
            hm.push_hit_test(id, hits(&[(0, &[1])]));
        }
        assert!(!hm.any_has_sufficient_history_for_gestures());

        // A single point reaching 2 frames flips it.
        hm.push_hit_test(InputPointId::Touch(u64::MAX), hits(&[(0, &[2])]));
        assert!(hm.any_has_sufficient_history_for_gestures());

        // Emptying that point's history flips it back.
        hm.clear_input_point(&InputPointId::Touch(u64::MAX));
        assert!(!hm.any_has_sufficient_history_for_gestures());
    }

    // ---------------------------------------------------- hover node getters

    #[test]
    fn current_hover_node_returns_the_deepest_node_of_dom_zero() {
        let hm = mouse_history(vec![hits(&[(0, &[3, 8, 5])])]);

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(8)));
        // Single-DOM: the _full variant is the same node wrapped in DomId 0.
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 8)));
    }

    #[test]
    fn current_hover_node_ignores_non_zero_doms_but_full_does_not() {
        // Only a child DOM was hit — the single-DOM getter is blind to it.
        let hm = mouse_history(vec![hits(&[(2, &[4])])]);

        assert_eq!(hm.current_hover_node(), None);
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(2, 4)));
    }

    #[test]
    fn current_hover_node_full_prefers_the_front_most_child_dom() {
        let hm = mouse_history(vec![hits(&[(0, &[9]), (1, &[2])])]);

        // The root getter still reports the root's deepest node...
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(9)));
        // ...while the multi-DOM getter targets the composited-on-top child.
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(1, 2)));
    }

    #[test]
    fn previous_hover_node_is_none_until_a_second_frame_exists() {
        let hm = mouse_history(vec![hits(&[(0, &[1])])]);

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(1)));
        assert_eq!(hm.previous_hover_node(), None);
        assert_eq!(hm.previous_hover_node_full(), None);
    }

    #[test]
    fn previous_hover_node_reads_frame_one_not_the_oldest_frame() {
        // 6 pushes => the oldest (node 0) is evicted; frame 1 is node 4.
        let hm = mouse_history((0..6).map(|i| hits(&[(0, &[i])])).collect());

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(5)));
        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(4)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(0, 4)));
    }

    #[test]
    fn previous_hover_node_full_sees_child_doms_of_the_previous_frame() {
        let hm = mouse_history(vec![hits(&[(0, &[1]), (7, &[3])]), hits(&[(0, &[2])])]);

        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(1)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(7, 3)));
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 2)));
    }

    #[test]
    fn hover_node_getters_are_none_when_the_frame_hit_nothing() {
        let hm = mouse_history(vec![FullHitTest::empty(None), FullHitTest::empty(None)]);

        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
    }

    #[test]
    fn hover_node_getters_ignore_touch_history_entirely() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Touch(1), hits(&[(0, &[5])]));
        hm.push_hit_test(InputPointId::Touch(1), hits(&[(0, &[6])]));

        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
        assert!(hm.any_has_sufficient_history_for_gestures());
    }

    // ------------------------------------------------------ purge_dom (other)

    #[test]
    fn purge_dom_removes_that_dom_from_every_frame_of_every_point() {
        let mut hm = HoverManager::new();
        for id in [InputPointId::Mouse, InputPointId::Touch(2)] {
            hm.push_hit_test(id, hits(&[(0, &[1]), (1, &[2])]));
            hm.push_hit_test(id, hits(&[(0, &[3]), (1, &[4])]));
        }

        hm.purge_dom(&dom(1));

        // Frames themselves are kept — only DOM 1's hits are forgotten.
        assert_eq!(hm.debug_counts(), (2, 4));
        for id in [InputPointId::Mouse, InputPointId::Touch(2)] {
            let history = hm.get_history(&id).expect("history exists");
            for frame in history {
                assert!(!frame.hovered_nodes.contains_key(&dom(1)));
                assert!(frame.hovered_nodes.contains_key(&dom(0)));
            }
        }
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 3)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(0, 1)));
    }

    #[test]
    fn purge_dom_zero_leaves_child_dom_hits_intact() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (4, &[2])])]);

        hm.purge_dom(&dom(0));

        // The single-DOM getter now finds nothing, the multi-DOM one falls back.
        assert_eq!(hm.current_hover_node(), None);
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(4, 2)));
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 1);
    }

    #[test]
    fn purge_absent_or_extreme_dom_id_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.purge_dom(&dom(9));
        hm.purge_dom(&dom(usize::MAX));
        assert_eq!(hm, before);

        // Purging on an empty manager must not panic either.
        let mut empty = HoverManager::new();
        empty.purge_dom(&dom(0));
        assert_eq!(empty, HoverManager::new());
    }

    #[test]
    fn purge_dom_twice_is_idempotent() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (1, &[2])])]);

        hm.purge_dom(&dom(1));
        let once = hm.clone();
        hm.purge_dom(&dom(1));

        assert_eq!(hm, once);
    }

    // -------------------------------------------- remap_scrollbar_hit_id (other)

    fn sb_map(pairs: &[(usize, usize)]) -> BTreeMap<NodeId, NodeId> {
        pairs
            .iter()
            .map(|(o, n)| (NodeId::new(*o), NodeId::new(*n)))
            .collect()
    }

    #[test]
    fn remap_scrollbar_hit_id_rewrites_every_variant_of_the_target_dom() {
        let map = sb_map(&[(1, 42)]);
        let d = dom(0);
        let old = NodeId::new(1);
        let new = NodeId::new(42);

        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalTrack(d, old), d, &map),
            Some(ScrollbarHitId::VerticalTrack(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalThumb(d, old), d, &map),
            Some(ScrollbarHitId::VerticalThumb(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::HorizontalTrack(d, old), d, &map),
            Some(ScrollbarHitId::HorizontalTrack(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::HorizontalThumb(d, old), d, &map),
            Some(ScrollbarHitId::HorizontalThumb(d, new))
        );
    }

    #[test]
    fn remap_scrollbar_hit_id_drops_unmounted_nodes() {
        let map = sb_map(&[(1, 42)]);
        let d = dom(0);
        // Node 2 is absent from the map => unmounted => the hit must be dropped.
        let unmounted = ScrollbarHitId::VerticalThumb(d, NodeId::new(2));

        assert_eq!(remap_scrollbar_hit_id(&unmounted, d, &map), None);
        // Empty map: everything on the target DOM is unmounted.
        let hit = ScrollbarHitId::VerticalThumb(d, NodeId::new(1));
        assert_eq!(remap_scrollbar_hit_id(&hit, d, &BTreeMap::new()), None);
    }

    #[test]
    fn remap_scrollbar_hit_id_passes_other_doms_through_untouched() {
        // The map applies to DOM 0 only; an id naming DOM 1 must NOT be rewritten
        // even though its NodeId happens to be a key in the map.
        let map = sb_map(&[(1, 42)]);
        let other = ScrollbarHitId::HorizontalTrack(dom(1), NodeId::new(1));

        assert_eq!(remap_scrollbar_hit_id(&other, dom(0), &map), Some(other));
        // ...and it survives an empty map too (no accidental drop).
        assert_eq!(
            remap_scrollbar_hit_id(&other, dom(0), &BTreeMap::new()),
            Some(other)
        );
    }

    #[test]
    fn remap_scrollbar_hit_id_handles_extreme_ids() {
        let big = usize::MAX - 1;
        let map = sb_map(&[(big, 0)]);
        let d = dom(usize::MAX);

        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalTrack(d, NodeId::new(big)), d, &map),
            Some(ScrollbarHitId::VerticalTrack(d, NodeId::ZERO))
        );
    }

    // ------------------------------------------------ NodeIdRemap::remap_node_ids

    /// A hit test with one regular + scroll + cursor + scrollbar hit on `node`.
    fn all_maps_hit(dom_inner: usize, node: usize) -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes
            .insert(NodeId::new(node), hit_item(0));
        ht.scroll_hit_test_nodes
            .insert(NodeId::new(node), scroll_item());
        ht.cursor_hit_test_nodes
            .insert(NodeId::new(node), cursor_item());
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalThumb(dom(dom_inner), NodeId::new(node)),
            scrollbar_item(),
        );
        full.hovered_nodes.insert(dom(dom_inner), ht);
        full
    }

    #[test]
    fn remap_node_ids_rewrites_all_four_hit_maps() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);

        hm.remap_node_ids(dom(0), &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(11))]));

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(
            ht.regular_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.scroll_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.cursor_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.scrollbar_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![ScrollbarHitId::VerticalThumb(dom(0), NodeId::new(11))]
        );
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(11)));
    }

    #[test]
    fn remap_node_ids_with_an_empty_map_drops_every_hit_of_that_dom() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);

        // Empty map = nothing matched = every node was unmounted.
        hm.remap_node_ids(dom(0), &NodeIdMap::default());

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert!(ht.regular_hit_test_nodes.is_empty());
        assert!(ht.scroll_hit_test_nodes.is_empty());
        assert!(ht.cursor_hit_test_nodes.is_empty());
        assert!(ht.scrollbar_hit_test_nodes.is_empty());
        assert_eq!(hm.current_hover_node(), None);
        // The (now empty) DOM entry itself is kept — only purge_dom removes it.
        assert!(hm
            .get_current_mouse()
            .expect("frame exists")
            .hovered_nodes
            .contains_key(&dom(0)));
    }

    #[test]
    fn remap_node_ids_drops_unmounted_but_keeps_survivors() {
        // Nodes 1 and 4 hit; only 4 survives the rebuild (as node 0).
        let mut hm = mouse_history(vec![hits(&[(0, &[1, 4])])]);

        hm.remap_node_ids(dom(0), &NodeIdMap::from_pairs([(NodeId::new(4), NodeId::ZERO)]));

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(
            ht.regular_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::ZERO]
        );
        assert_eq!(hm.current_hover_node(), Some(NodeId::ZERO));
    }

    #[test]
    fn remap_node_ids_swap_does_not_lose_or_alias_entries() {
        // 1 -> 2 and 2 -> 1 in the same pass: the naive in-place rewrite would
        // clobber one of them. Both must survive with their items swapped.
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes
            .insert(NodeId::new(1), hit_item(10));
        ht.regular_hit_test_nodes
            .insert(NodeId::new(2), hit_item(20));
        full.hovered_nodes.insert(dom(0), ht);
        let mut hm = mouse_history(vec![full]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(1), NodeId::new(2)),
                (NodeId::new(2), NodeId::new(1)),
            ]),
        );

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(ht.regular_hit_test_nodes.len(), 2);
        assert_eq!(ht.regular_hit_test_nodes[&NodeId::new(2)].hit_depth, 10);
        assert_eq!(ht.regular_hit_test_nodes[&NodeId::new(1)].hit_depth, 20);
    }

    #[test]
    fn remap_node_ids_can_change_which_node_is_deepest() {
        // Old order: 7 is deepest. The rebuild renumbers 3 -> 9 and 7 -> 2,
        // so the deepest hit must be recomputed (9), not carried over.
        let mut hm = mouse_history(vec![hits(&[(0, &[3, 7])])]);
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(7)));

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(3), NodeId::new(9)),
                (NodeId::new(7), NodeId::new(2)),
            ]),
        );

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(9)));
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 9)));
    }

    #[test]
    fn remap_node_ids_leaves_other_doms_alone() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (1, &[1])])]);

        hm.remap_node_ids(dom(0), &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(5))]));

        let frame = hm.get_current_mouse().expect("frame exists");
        assert_eq!(
            frame.hovered_nodes[&dom(0)]
                .regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::new(5)],
            "DOM 0 is remapped"
        );
        assert_eq!(
            frame.hovered_nodes[&dom(1)]
                .regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::new(1)],
            "DOM 1 must be untouched by DOM 0's reconciliation"
        );
    }

    #[test]
    fn remap_node_ids_keeps_foreign_dom_scrollbar_ids_stored_under_the_target_dom() {
        // A scrollbar hit recorded under DOM 0's HitTest but whose ScrollbarHitId
        // names DOM 1: remap_scrollbar_hit_id must pass it through, not drop it.
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalTrack(dom(1), NodeId::new(1)),
            scrollbar_item(),
        );
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalTrack(dom(0), NodeId::new(1)),
            scrollbar_item(),
        );
        full.hovered_nodes.insert(dom(0), ht);
        let mut hm = mouse_history(vec![full]);

        hm.remap_node_ids(dom(0), &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(8))]));

        let keys: Vec<_> = hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)]
            .scrollbar_hit_test_nodes
            .keys()
            .copied()
            .collect();
        assert!(
            keys.contains(&ScrollbarHitId::VerticalTrack(dom(1), NodeId::new(1))),
            "foreign-DOM scrollbar id must survive unchanged, got {keys:?}"
        );
        assert!(
            keys.contains(&ScrollbarHitId::VerticalTrack(dom(0), NodeId::new(8))),
            "target-DOM scrollbar id must be rewritten, got {keys:?}"
        );
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn remap_node_ids_applies_to_every_frame_and_every_input_point() {
        let mut hm = HoverManager::new();
        for id in [InputPointId::Mouse, InputPointId::Touch(1)] {
            hm.push_hit_test(id, hits(&[(0, &[2])]));
            hm.push_hit_test(id, hits(&[(0, &[2])]));
        }

        hm.remap_node_ids(dom(0), &NodeIdMap::from_pairs([(NodeId::new(2), NodeId::new(6))]));

        for id in [InputPointId::Mouse, InputPointId::Touch(1)] {
            for frame in hm.get_history(&id).expect("history exists") {
                assert_eq!(
                    frame.hovered_nodes[&dom(0)]
                        .regular_hit_test_nodes
                        .keys()
                        .copied()
                        .collect::<Vec<_>>(),
                    vec![NodeId::new(6)]
                );
            }
        }
        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(6)));
    }

    #[test]
    fn remap_node_ids_on_an_empty_manager_or_unknown_dom_does_not_panic() {
        let mut empty = HoverManager::new();
        empty.remap_node_ids(dom(usize::MAX), &NodeIdMap::default());
        assert_eq!(empty, HoverManager::new());

        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();
        // Reconciliation for a DOM that was never hit changes nothing.
        hm.remap_node_ids(dom(3), &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(2))]));
        assert_eq!(hm, before);
    }

    #[test]
    fn remap_node_ids_identity_map_is_idempotent() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);
        let before = hm.clone();
        let identity = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(3))]);

        hm.remap_node_ids(dom(0), &identity);
        assert_eq!(hm, before, "identity remap must not change anything");

        hm.remap_node_ids(dom(0), &identity);
        assert_eq!(hm, before, "and applying it twice must not either");
    }

    // ------------------------------------------------------------- misc invariants

    #[test]
    fn clone_is_equal_and_independent_of_the_original() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let snapshot = hm.clone();
        assert_eq!(hm, snapshot);

        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[2])]));

        assert_ne!(hm, snapshot, "the clone must not observe later pushes");
        assert_eq!(snapshot.frame_count(&InputPointId::Mouse), 1);
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 2);
    }

    #[test]
    fn debug_counts_agrees_with_frame_count_and_active_points() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(7), hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(7), hits(&[(0, &[2])]));

        let (points, total) = hm.debug_counts();
        let active = hm.get_active_input_points();
        assert_eq!(points, active.len());
        assert_eq!(
            total,
            active.iter().map(|id| hm.frame_count(id)).sum::<usize>()
        );
        assert_eq!((points, total), (2, 3));
    }
}
