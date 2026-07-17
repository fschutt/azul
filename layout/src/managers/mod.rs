//! Manager types responsible for stateful input and UI concerns.
//!
//! This module collects managers for accessibility, clipboard, drag-and-drop,
//! focus/cursor, gestures, hover, scroll state, selection, text editing,
//! text input, undo/redo, and virtual views. These managers are consumed
//! primarily by `layout/src/window.rs` and `layout/src/event_determination.rs`.
//!
//! # `NodeId` staleness — read this before adding a manager
//!
//! A `NodeId` is an INDEX into the current DOM arena, not a stable identity.
//! Every DOM rebuild (virtual-view re-invocation, window resize, route switch,
//! any `regenerate_layout`) renumbers them. Reconciliation
//! (`azul_core::diff::reconcile_dom`) tells us how: `node_moves` maps every
//! MATCHED old `NodeId` to its new one. An old `NodeId` that is absent from
//! that map was UNMOUNTED.
//!
//! Any manager that keys state by `NodeId` and does not participate in that
//! remap ends up pointing at a *live but wrong* node — deleting a preceding
//! sibling shifts every following index down by one, so the state silently
//! re-attaches to a different element (no dangling id, no panic, no error).
//! Unmapped keys also leak forever.
//!
//! The fix is structural: every node-keyed manager implements
//! [`NodeIdRemap`], and [`crate::window::LayoutWindow::remap_node_ids`]
//! (exhaustively destructured, so a NEW FIELD IS A COMPILE ERROR until it is
//! classified) drives all of them from one place.

pub mod a11y;
pub mod biometric;
pub mod changeset;
pub mod clipboard;
pub mod drag_drop;
pub mod file_drop;
pub mod focus_cursor;
pub mod gamepad;
pub mod geolocation;
pub mod gesture;
pub mod gpu_state;
pub mod hover;
pub mod keyring;
pub mod permission;
pub mod virtual_view;
pub mod scroll_into_view;
pub mod scroll_state;
pub mod selection;
pub mod sensors;
pub mod text_edit;
pub mod text_input;
pub mod undo_redo;

use alloc::collections::BTreeMap;

use azul_core::dom::{DomId, DomNodeId, NodeId};
use azul_core::styled_dom::NodeHierarchyItemId;

/// The result of a DOM reconciliation, from the point of view of anyone holding
/// `NodeId`-keyed state for a single DOM.
///
/// Built from `azul_core::diff::DiffResult::node_moves`, which contains an entry
/// for EVERY matched node (including nodes that kept their index). The absence
/// of an old `NodeId` from the map therefore has a precise meaning: that node was
/// **unmounted**. This is what makes GC possible without a second "alive" set.
///
/// The contract for consumers is a single rule:
///
/// * [`NodeIdMap::resolve`] returns `Some(new_id)` — the node survived, rewrite the key.
/// * [`NodeIdMap::resolve`] returns `None` — the node is GONE, **drop the state**.
///
/// Never "keep it, just in case": a kept key is a key that now denotes a
/// different node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeIdMap {
    moves: BTreeMap<NodeId, NodeId>,
}

impl NodeIdMap {
    /// Build from reconciliation output (`DiffResult::node_moves`).
    #[must_use]
    pub fn from_node_moves(node_moves: &[azul_core::diff::NodeMove]) -> Self {
        Self {
            moves: node_moves
                .iter()
                .map(|m| (m.old_node_id, m.new_node_id))
                .collect(),
        }
    }

    /// Build from raw `(old, new)` pairs — used by tests and by callers that
    /// already computed a migration map.
    #[must_use]
    pub fn from_pairs<I: IntoIterator<Item = (NodeId, NodeId)>>(pairs: I) -> Self {
        Self {
            moves: pairs.into_iter().collect(),
        }
    }

    /// `Some(new_id)` if the node survived the rebuild, `None` if it was unmounted.
    #[must_use]
    pub fn resolve(&self, old: NodeId) -> Option<NodeId> {
        self.moves.get(&old).copied()
    }

    /// `true` if `old` no longer exists in the new DOM.
    #[must_use]
    pub fn is_unmounted(&self, old: NodeId) -> bool {
        !self.moves.contains_key(&old)
    }

    /// Resolve a full `DomNodeId`. Ids belonging to a *different* DOM are passed
    /// through untouched (this reconciliation says nothing about them).
    #[must_use]
    pub fn resolve_dom_node_id(&self, dom: DomId, id: DomNodeId) -> Option<DomNodeId> {
        if id.dom != dom {
            return Some(id);
        }
        let old = id.node.into_crate_internal()?;
        let new = self.resolve(old)?;
        Some(DomNodeId {
            dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(new)),
        })
    }

    /// The raw old→new map, for `azul_core` APIs that take a `BTreeMap`
    /// (`DragContext::remap_node_ids`, `MultiCursorState::remap_node_ids`).
    #[must_use]
    pub const fn as_btree_map(&self) -> &BTreeMap<NodeId, NodeId> {
        &self.moves
    }

    /// No matched nodes at all (everything was unmounted / the DOM is brand new).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }
}

/// Implemented by EVERY manager (or cache) that keys state by `NodeId`.
///
/// One method on purpose: remapping and GC are the same pass, so it is
/// impossible to do one and forget the other. Implementors MUST, for state
/// belonging to `dom`:
///
/// 1. rewrite each key/field `old` to `map.resolve(old)`, and
/// 2. **drop** the state whenever `resolve` returns `None` (unmounted node).
///
/// State belonging to any *other* `DomId` must be left alone.
pub trait NodeIdRemap {
    /// Rewrite all `NodeId`s for `dom` and drop state for unmounted nodes.
    fn remap_node_ids(&mut self, dom: DomId, map: &NodeIdMap);
}

/// Remap the keys of a `BTreeMap<NodeId, V>` in place, dropping unmounted nodes.
pub(crate) fn remap_keys<V>(map: &mut BTreeMap<NodeId, V>, node_map: &NodeIdMap) {
    let old = core::mem::take(map);
    for (old_id, v) in old {
        if let Some(new_id) = node_map.resolve(old_id) {
            map.insert(new_id, v);
        }
    }
}

/// Remap the keys of a `BTreeMap<(DomId, NodeId), V>` in place: entries for
/// `dom` are rewritten (or dropped if unmounted), entries for other DOMs are
/// left untouched.
pub(crate) fn remap_dom_keys<V>(
    map: &mut BTreeMap<(DomId, NodeId), V>,
    dom: DomId,
    node_map: &NodeIdMap,
) {
    let old = core::mem::take(map);
    for ((d, old_id), v) in old {
        if d != dom {
            map.insert((d, old_id), v);
        } else if let Some(new_id) = node_map.resolve(old_id) {
            map.insert((d, new_id), v);
        }
    }
}

// ============================================================================
// THE PRECEDING-SIBLING TEST
// ============================================================================
//
// These tests encode the failure mode that motivated `NodeIdRemap`. They do NOT
// assert "no panic" — an unremapped manager never panics, that is exactly what
// made this bug survive. They assert LOGICAL IDENTITY: after the rebuild, every
// manager's state must still describe the SAME ELEMENT it described before.
//
// Scenario (one DOM, four nodes):
//
//     before:  0=root  1=A   2=B   3=C
//     delete A
//     after:   0=root        1=B   2=C          map = {0→0, 2→1, 3→2}
//
// State is seeded on B(2) and C(3) with DISTINGUISHABLE payloads, and on the
// doomed A(1). A manager that skips the remap keeps C's state at key 3 and
// leaves B's state at key 2 — but index 2 now denotes C. So the state is not
// dangling, it is MISATTACHED: "give me C's state" silently answers with B's.
// Asserting `state_at(2) == C_payload` is what catches that; a null/panic check
// does not.
#[cfg(all(test, feature = "std"))]
mod preceding_sibling_remap_tests {
    use alloc::collections::BTreeMap;

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId},
        drag::{DragContext, DragData},
        geom::LogicalPosition,
        hit_test::{FullHitTest, HitTest, HitTestItem},
        selection::{CursorAffinity, GraphemeClusterId, MultiCursorState, TextCursor},
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    use super::{
        changeset::{TextChangeset, TextOpInsertText, TextOperation},
        focus_cursor::FocusManager,
        gesture::GestureAndDragManager,
        gpu_state::GpuStateManager,
        hover::{HoverManager, InputPointId},
        scroll_state::ScrollManager,
        text_edit::TextEditManager,
        text_input::{TextInputManager, TextInputSource},
        undo_redo::{NodeStateSnapshot, UndoRedoManager},
        virtual_view::VirtualViewManager,
        NodeIdMap, NodeIdRemap,
    };

    const ROOT: DomId = DomId { inner: 0 };
    /// The node that gets deleted.
    const A: NodeId = NodeId::new(1);
    /// Surviving sibling, index 2 → 1.
    const B_OLD: NodeId = NodeId::new(2);
    const B_NEW: NodeId = NodeId::new(1);
    /// Surviving sibling, index 3 → 2.
    const C_OLD: NodeId = NodeId::new(3);
    const C_NEW: NodeId = NodeId::new(2);

    /// Exactly what `reconcile_dom` produces when the preceding sibling A is
    /// deleted: every MATCHED node, with A absent (= unmounted).
    fn delete_a() -> NodeIdMap {
        NodeIdMap::from_pairs([
            (NodeId::new(0), NodeId::new(0)),
            (B_OLD, B_NEW),
            (C_OLD, C_NEW),
        ])
    }

    fn now() -> Instant {
        Instant::Tick(SystemTick { tick_counter: 0 })
    }

    fn dom_node(node: NodeId) -> DomNodeId {
        DomNodeId {
            dom: ROOT,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        }
    }

    // ---------------------------------------------------------------- scroll

    #[test]
    fn scroll_offsets_follow_their_node_across_a_preceding_sibling_delete() {
        let mut m = ScrollManager::new();
        // Distinguishable payloads: y = 10 for A, 20 for B, 30 for C.
        m.set_scroll_position_unclamped(ROOT, A, LogicalPosition::new(0.0, 10.0), now());
        m.set_scroll_position_unclamped(ROOT, B_OLD, LogicalPosition::new(0.0, 20.0), now());
        m.set_scroll_position_unclamped(ROOT, C_OLD, LogicalPosition::new(0.0, 30.0), now());

        m.remap_node_ids(ROOT, &delete_a());

        // C is now node 2 and MUST still have C's offset (30) — not B's (20),
        // which is what an unremapped manager would answer here.
        assert_eq!(
            m.get_scroll_state(ROOT, C_NEW).map(|s| s.current_offset.y),
            Some(30.0),
            "C's scroll offset must follow C to its new NodeId"
        );
        assert_eq!(
            m.get_scroll_state(ROOT, B_NEW).map(|s| s.current_offset.y),
            Some(20.0),
            "B's scroll offset must follow B to its new NodeId"
        );
        // GC: the deleted node's state must not linger. NodeId(3) no longer exists.
        assert!(
            m.get_scroll_state(ROOT, NodeId::new(3)).is_none(),
            "no state may remain at a NodeId that no longer exists"
        );
        assert_eq!(m.get_scroll_states_for_dom(ROOT).len(), 2, "A's state must be GC'd");
    }

    // ------------------------------------------------------------- undo/redo

    fn undo_op(changeset_id: usize, node: NodeId, text: &str) -> super::undo_redo::UndoableOperation {
        super::undo_redo::UndoableOperation {
            changeset: TextChangeset {
                id: changeset_id,
                target: dom_node(node),
                operation: TextOperation::InsertText(TextOpInsertText {
                    text: text.into(),
                    position: azul_core::window::CursorPosition::Uninitialized,
                    new_cursor: azul_core::window::CursorPosition::Uninitialized,
                }),
                timestamp: now(),
            },
            pre_state: NodeStateSnapshot {
                node_id: node,
                text_content: text.into(),
                cursor_position: None.into(),
                selection_range: None.into(),
                timestamp: now(),
            },
        }
    }

    #[test]
    fn undo_history_stays_attached_to_the_same_element() {
        let mut m = UndoRedoManager::new();
        let a = undo_op(1, A, "typed-into-A");
        let b = undo_op(2, B_OLD, "typed-into-B");
        let c = undo_op(3, C_OLD, "typed-into-C");
        m.record_operation(a.changeset.clone(), a.pre_state.clone());
        m.record_operation(b.changeset.clone(), b.pre_state.clone());
        m.record_operation(c.changeset.clone(), c.pre_state.clone());

        m.remap_node_ids(ROOT, &delete_a());

        // THE bug: undoing "on C" must revert C's edit, not B's.
        let undo_on_c = m.peek_undo(C_NEW).expect("C must still have undo history");
        assert_eq!(
            undo_on_c.pre_state.text_content.as_str(),
            "typed-into-C",
            "undo on C must revert C's edit — an unremapped Vec re-attaches B's history here"
        );
        let undo_on_b = m.peek_undo(B_NEW).expect("B must still have undo history");
        assert_eq!(undo_on_b.pre_state.text_content.as_str(), "typed-into-B");

        // The embedded NodeIds must be rewritten too, or the *replay* targets the wrong node.
        assert_eq!(
            undo_on_c.changeset.target.node.into_crate_internal(),
            Some(C_NEW)
        );
        assert_eq!(undo_on_c.pre_state.node_id, C_NEW);

        // GC: A is gone, its history must be gone.
        assert_eq!(m.node_stacks.len(), 2, "the deleted node's undo stack must be GC'd");
        assert!(!m.can_undo(NodeId::new(3)), "no history at a NodeId that no longer exists");
    }

    // ----------------------------------------------------------- virtual view

    #[test]
    fn virtual_view_nested_doms_stay_with_their_host_node() {
        let mut m = VirtualViewManager::new();
        let dom_a = m.get_or_create_nested_dom_id(ROOT, A);
        let dom_b = m.get_or_create_nested_dom_id(ROOT, B_OLD);
        let dom_c = m.get_or_create_nested_dom_id(ROOT, C_OLD);
        assert_ne!(dom_b, dom_c);

        m.remap_node_ids(ROOT, &delete_a());

        assert_eq!(
            m.get_nested_dom_id(ROOT, C_NEW),
            Some(dom_c),
            "C's nested DOM must follow C — otherwise C renders B's virtual view"
        );
        assert_eq!(m.get_nested_dom_id(ROOT, B_NEW), Some(dom_b));
        assert_eq!(m.debug_counts(), 2, "the deleted view's state must be GC'd");
        assert!(!m.all_view_keys().iter().any(|(_, n)| *n == C_OLD));
        assert_ne!(m.get_nested_dom_id(ROOT, B_NEW), Some(dom_a));
    }

    // -------------------------------------------------------------- gpu state

    #[test]
    fn gpu_transform_keys_stay_with_their_node() {
        use azul_core::resources::{OpacityKey, TransformKey};
        let mut m = GpuStateManager::default();
        {
            let cache = m.get_or_create_cache(ROOT);
            cache.opacity_keys.insert(A, OpacityKey::unique());
            cache.current_opacity_values.insert(A, 0.1);
            cache.current_opacity_values.insert(B_OLD, 0.2);
            cache.current_opacity_values.insert(C_OLD, 0.3);
            cache.css_transform_keys.insert(C_OLD, TransformKey::unique());
        }
        let c_key = m.get_cache(ROOT).unwrap().css_transform_keys[&C_OLD];

        m.remap_node_ids(ROOT, &delete_a());

        let cache = m.get_cache(ROOT).unwrap();
        assert_eq!(
            cache.current_opacity_values.get(&C_NEW).copied(),
            Some(0.3),
            "C's opacity must follow C, not be inherited from B"
        );
        assert_eq!(cache.current_opacity_values.get(&B_NEW).copied(), Some(0.2));
        assert_eq!(
            cache.css_transform_keys.get(&C_NEW).copied(),
            Some(c_key),
            "C's GPU transform key must follow C"
        );
        assert!(cache.opacity_keys.is_empty(), "the deleted node's GPU keys must be GC'd");
        assert_eq!(cache.current_opacity_values.len(), 2);
    }

    // ------------------------------------------------------------------ focus

    #[test]
    fn focus_follows_its_node_and_is_cleared_when_the_node_dies() {
        let mut m = FocusManager::new();
        m.set_focused_node(Some(dom_node(C_OLD)));
        m.remap_node_ids(ROOT, &delete_a());
        assert_eq!(
            m.get_focused_node().and_then(|f| f.node.into_crate_internal()),
            Some(C_NEW),
            "focus must follow the focused element, not stay on a recycled index"
        );

        let mut m = FocusManager::new();
        m.set_focused_node(Some(dom_node(A)));
        m.remap_node_ids(ROOT, &delete_a());
        assert!(
            m.get_focused_node().is_none(),
            "focus on an unmounted node must be cleared, never retargeted"
        );
    }

    // -------------------------------------------------------------- text edit

    #[test]
    fn a_live_selection_stays_on_the_edited_element() {
        let cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        };
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(MultiCursorState::new_with_cursor(cursor, dom_node(C_OLD), 0));

        m.remap_node_ids(ROOT, &delete_a());

        let mc = m.multi_cursor.as_ref().expect("the editing session survives");
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(C_NEW),
            "the caret must stay in the element the user is editing"
        );
        assert_eq!(mc.selections.len(), 1, "surviving node keeps its selections");

        // Editing a node that gets deleted ends the session (no retarget).
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(MultiCursorState::new_with_cursor(cursor, dom_node(A), 0));
        m.remap_node_ids(ROOT, &delete_a());
        assert!(m.multi_cursor.is_none(), "editing an unmounted node must end the session");
    }

    // ----------------------------------------------------------------- drag

    fn node_drag(node: NodeId) -> DragContext {
        DragContext::node_drag(ROOT, node, LogicalPosition::zero(), DragData::default(), 1)
    }

    #[test]
    fn a_live_drag_keeps_dragging_the_same_element() {
        let mut m = GestureAndDragManager::new();
        m.active_drag = Some(node_drag(C_OLD));

        m.remap_node_ids(ROOT, &delete_a());

        assert!(
            m.is_node_dragging(ROOT, C_NEW),
            "the dragged element must still be the dragged element after the rebuild"
        );
        assert!(
            !m.is_node_dragging(ROOT, B_NEW),
            "the drag must NOT jump onto the sibling that inherited the old index"
        );

        // Dragging a node that gets deleted cancels the drag.
        let mut m = GestureAndDragManager::new();
        m.active_drag = Some(node_drag(A));
        m.remap_node_ids(ROOT, &delete_a());
        assert!(m.get_drag_context().is_none(), "a drag whose source vanished is cancelled");
    }

    // ----------------------------------------------------------- text input

    #[test]
    fn a_pending_text_edit_is_not_applied_to_the_wrong_node() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(C_OLD), "x".into(), String::new(), TextInputSource::Keyboard);
        m.remap_node_ids(ROOT, &delete_a());
        assert_eq!(
            m.get_pending_changeset()
                .and_then(|p| p.node.node.into_crate_internal()),
            Some(C_NEW),
            "the recorded edit must apply to the node it was recorded on"
        );

        let mut m = TextInputManager::new();
        m.record_input(dom_node(A), "x".into(), String::new(), TextInputSource::Keyboard);
        m.remap_node_ids(ROOT, &delete_a());
        assert!(
            m.get_pending_changeset().is_none(),
            "an edit recorded on an unmounted node must be dropped, not applied elsewhere"
        );
    }

    // ---------------------------------------------------------------- hover

    #[test]
    fn hover_history_hits_follow_their_nodes() {
        fn hit(depth: u32) -> HitTestItem {
            HitTestItem {
                point_in_viewport: LogicalPosition::zero(),
                point_relative_to_item: LogicalPosition::zero(),
                is_focusable: false,
                is_virtual_view_hit: None,
                hit_depth: depth,
            }
        }
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes.insert(A, hit(1));
        ht.regular_hit_test_nodes.insert(B_OLD, hit(2));
        ht.regular_hit_test_nodes.insert(C_OLD, hit(3));
        let mut full = FullHitTest::empty(None);
        full.hovered_nodes.insert(ROOT, ht);

        let mut m = HoverManager::new();
        m.push_hit_test(InputPointId::Mouse, full);

        m.remap_node_ids(ROOT, &delete_a());

        let nodes = &m
            .get_current(&InputPointId::Mouse)
            .unwrap()
            .hovered_nodes[&ROOT]
            .regular_hit_test_nodes;
        assert_eq!(
            nodes.get(&C_NEW).map(|h| h.hit_depth),
            Some(3),
            "C's hit must follow C (an unremapped history hands B's hit back for C)"
        );
        assert_eq!(nodes.get(&B_NEW).map(|h| h.hit_depth), Some(2));
        assert_eq!(nodes.len(), 2, "the deleted node's hit must be GC'd");
    }

    // ------------------------------------------------------ cross-DOM safety

    #[test]
    fn state_belonging_to_another_dom_is_never_touched() {
        let other = DomId { inner: 7 };
        let mut m = ScrollManager::new();
        m.set_scroll_position_unclamped(other, C_OLD, LogicalPosition::new(0.0, 99.0), now());
        m.remap_node_ids(ROOT, &delete_a());
        assert_eq!(
            m.get_scroll_state(other, C_OLD).map(|s| s.current_offset.y),
            Some(99.0),
            "a reconciliation of DOM 0 says nothing about DOM 7"
        );

        let mut vv = VirtualViewManager::new();
        let nested = vv.get_or_create_nested_dom_id(other, C_OLD);
        vv.remap_node_ids(ROOT, &delete_a());
        assert_eq!(vv.get_nested_dom_id(other, C_OLD), Some(nested));
    }

    /// The map itself is the GC oracle: `node_moves` lists EVERY matched node, so
    /// an id missing from it is unmounted (not merely "unmoved").
    #[test]
    fn node_id_map_semantics() {
        let map = delete_a();
        assert_eq!(map.resolve(NodeId::new(0)), Some(NodeId::new(0)));
        assert_eq!(map.resolve(C_OLD), Some(C_NEW));
        assert!(map.is_unmounted(A));
        assert!(map.resolve(A).is_none());
        let _unused: &BTreeMap<NodeId, NodeId> = map.as_btree_map();
    }
}

// ============================================================================
// AUTOTEST: adversarial tests for `NodeIdMap`, `remap_keys`, `remap_dom_keys`
// ============================================================================
#[cfg(test)]
mod autotest_generated {
    use azul_core::diff::NodeMove;

    use super::*;

    const DOM0: DomId = DomId { inner: 0 };
    const DOM1: DomId = DomId { inner: 1 };
    /// A `DomId` at the top of the `usize` range — must be handled like any other.
    const DOM_MAX: DomId = DomId { inner: usize::MAX };

    fn nid(i: usize) -> NodeId {
        NodeId::new(i)
    }

    fn mv(old: usize, new: usize) -> NodeMove {
        NodeMove {
            old_node_id: nid(old),
            new_node_id: nid(new),
        }
    }

    /// `NodeHierarchyItemId` uses a 1-based encoding, so the largest node index
    /// that can round-trip through a `DomNodeId` is `usize::MAX - 1`
    /// (`from_crate_internal` computes `inner + 1`). Anything above that is not
    /// representable and is deliberately not exercised through `DomNodeId`.
    const MAX_ENCODABLE: usize = usize::MAX - 1;

    fn dom_node_at(dom: DomId, node: NodeId) -> DomNodeId {
        DomNodeId {
            dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        }
    }

    // ------------------------------------------------------ constructors

    #[test]
    fn from_node_moves_on_an_empty_slice_yields_an_empty_map() {
        let map = NodeIdMap::from_node_moves(&[]);
        assert!(map.is_empty());
        assert!(map.as_btree_map().is_empty());
        assert_eq!(map, NodeIdMap::default());
    }

    #[test]
    fn from_node_moves_agrees_with_from_pairs_on_the_same_data() {
        let moves = [mv(0, 0), mv(5, 3), mv(9, 9)];
        let from_moves = NodeIdMap::from_node_moves(&moves);
        let from_pairs =
            NodeIdMap::from_pairs([(nid(0), nid(0)), (nid(5), nid(3)), (nid(9), nid(9))]);
        assert_eq!(
            from_moves, from_pairs,
            "the two constructors must produce identical maps for identical data"
        );
    }

    /// A malformed `node_moves` slice (the same old id listed twice) must not
    /// panic and must resolve deterministically — `BTreeMap::collect` keeps the
    /// LAST entry.
    #[test]
    fn from_node_moves_with_a_duplicated_old_id_keeps_the_last_entry() {
        let map = NodeIdMap::from_node_moves(&[mv(1, 10), mv(1, 20), mv(1, 30)]);
        assert_eq!(map.as_btree_map().len(), 1, "duplicates collapse to one key");
        assert_eq!(
            map.resolve(nid(1)),
            Some(nid(30)),
            "the last NodeMove for an old id wins"
        );
    }

    /// Two old nodes mapped onto the SAME new id is nonsense input, but it must
    /// still build a well-formed (if lossy) map rather than panic.
    #[test]
    fn from_node_moves_with_a_non_injective_mapping_does_not_panic() {
        let map = NodeIdMap::from_node_moves(&[mv(1, 7), mv(2, 7)]);
        assert_eq!(map.as_btree_map().len(), 2, "both old ids are retained as keys");
        assert_eq!(map.resolve(nid(1)), Some(nid(7)));
        assert_eq!(map.resolve(nid(2)), Some(nid(7)));
    }

    /// `NodeId` wraps a `usize`; ids at the very top of the range are just
    /// indices as far as the map is concerned — no arithmetic, no overflow.
    #[test]
    fn from_node_moves_handles_extreme_node_ids() {
        let map = NodeIdMap::from_node_moves(&[
            mv(usize::MAX, usize::MAX),
            mv(usize::MAX - 1, 0),
            mv(0, usize::MAX),
        ]);
        assert_eq!(map.resolve(nid(usize::MAX)), Some(nid(usize::MAX)));
        assert_eq!(map.resolve(nid(usize::MAX - 1)), Some(nid(0)));
        assert_eq!(map.resolve(nid(0)), Some(nid(usize::MAX)));
        assert!(!map.is_empty());
    }

    #[test]
    fn from_pairs_on_an_empty_iterator_yields_an_empty_map() {
        let map = NodeIdMap::from_pairs(Vec::new());
        assert!(map.is_empty());
        assert!(map.resolve(NodeId::ZERO).is_none());
        assert!(map.is_unmounted(NodeId::ZERO));
    }

    #[test]
    fn from_pairs_with_a_duplicated_old_id_keeps_the_last_entry() {
        let map = NodeIdMap::from_pairs([(nid(4), nid(1)), (nid(4), nid(2))]);
        assert_eq!(map.as_btree_map().len(), 1);
        assert_eq!(map.resolve(nid(4)), Some(nid(2)));
    }

    /// Post-construction invariants at volume: every pair fed in resolves back
    /// out, the length matches the number of distinct old ids, and nothing that
    /// was never inserted resolves.
    #[test]
    fn from_pairs_invariants_hold_at_volume() {
        let n = 2048usize;
        let pairs: Vec<(NodeId, NodeId)> = (0..n).map(|i| (nid(i), nid(n - 1 - i))).collect();
        let map = NodeIdMap::from_pairs(pairs);

        assert_eq!(map.as_btree_map().len(), n);
        assert!(!map.is_empty());
        for i in 0..n {
            assert_eq!(map.resolve(nid(i)), Some(nid(n - 1 - i)));
            assert!(!map.is_unmounted(nid(i)));
        }
        assert!(map.resolve(nid(n)).is_none(), "an id never inserted is unmounted");
        assert!(map.is_unmounted(nid(n)));
    }

    /// Round-trip: `as_btree_map` is a faithful encoding of what went in, and
    /// feeding it back through `from_pairs` reproduces the map exactly.
    #[test]
    fn as_btree_map_round_trips_through_from_pairs() {
        let original = NodeIdMap::from_pairs([
            (nid(0), nid(0)),
            (nid(2), nid(1)),
            (nid(3), nid(2)),
            (nid(usize::MAX), nid(4)),
        ]);
        let decoded = NodeIdMap::from_pairs(
            original
                .as_btree_map()
                .iter()
                .map(|(old, new)| (*old, *new))
                .collect::<Vec<_>>(),
        );
        assert_eq!(decoded, original, "encode == decode");
        assert_eq!(decoded.as_btree_map(), original.as_btree_map());
    }

    #[test]
    fn a_default_map_unmounts_everything() {
        let map = NodeIdMap::default();
        assert!(map.is_empty());
        assert!(map.as_btree_map().is_empty());
        for i in [0usize, 1, 2, 1024, usize::MAX - 1, usize::MAX] {
            assert!(map.resolve(nid(i)).is_none());
            assert!(
                map.is_unmounted(nid(i)),
                "an empty reconciliation means every old node was unmounted"
            );
        }
    }

    // --------------------------------------------- resolve / is_unmounted

    /// The two accessors are two views of the same fact — they must never
    /// disagree, for any id, on any map.
    #[test]
    fn resolve_and_is_unmounted_never_disagree() {
        let map = NodeIdMap::from_pairs([(nid(0), nid(0)), (nid(2), nid(1)), (nid(3), nid(2))]);
        for i in [0usize, 1, 2, 3, 4, 100, usize::MAX - 1, usize::MAX] {
            assert_eq!(
                map.resolve(nid(i)).is_none(),
                map.is_unmounted(nid(i)),
                "is_unmounted({i}) must be exactly !resolve({i}).is_some()"
            );
        }
    }

    #[test]
    fn resolve_is_pure_repeated_calls_return_the_same_answer() {
        let map = NodeIdMap::from_pairs([(nid(3), nid(2))]);
        let first = map.resolve(nid(3));
        assert_eq!(first, map.resolve(nid(3)));
        assert_eq!(first, map.resolve(nid(3)));
        assert_eq!(first, Some(nid(2)));
    }

    /// `resolve` is a single lookup, NOT a transitive closure. If it chased
    /// chains, `1 → 2 → 3` would collapse and every remap would be wrong for
    /// any map whose new ids overlap its old ids (which is the normal case).
    #[test]
    fn resolve_does_not_chase_chains() {
        let map = NodeIdMap::from_pairs([(nid(1), nid(2)), (nid(2), nid(3))]);
        assert_eq!(
            map.resolve(nid(1)),
            Some(nid(2)),
            "resolve must apply exactly one hop"
        );
        assert_eq!(map.resolve(nid(2)), Some(nid(3)));
    }

    /// A node that kept its index is MATCHED, not unmounted — the whole GC rule
    /// depends on identity entries being present and meaningful.
    #[test]
    fn an_identity_entry_means_matched_not_unmounted() {
        let map = NodeIdMap::from_pairs([(nid(7), nid(7))]);
        assert!(!map.is_unmounted(nid(7)));
        assert_eq!(map.resolve(nid(7)), Some(nid(7)));
        assert!(map.is_unmounted(nid(6)));
        assert!(map.is_unmounted(nid(8)));
    }

    #[test]
    fn is_empty_is_exactly_the_btree_maps_emptiness() {
        let empty = NodeIdMap::from_pairs(Vec::new());
        assert_eq!(empty.is_empty(), empty.as_btree_map().is_empty());
        assert!(empty.is_empty());

        let full = NodeIdMap::from_pairs([(nid(0), nid(0))]);
        assert_eq!(full.is_empty(), full.as_btree_map().is_empty());
        assert!(!full.is_empty());
    }

    // ------------------------------------------------- resolve_dom_node_id

    /// The documented pass-through rule: a reconciliation of DOM 0 says NOTHING
    /// about DOM 1, so an id from another DOM must come back byte-identical —
    /// even when its node index happens to be unmounted in *this* map.
    #[test]
    fn a_foreign_dom_node_id_is_passed_through_untouched() {
        let map = NodeIdMap::from_pairs([(nid(2), nid(1))]);
        // Node 5 is "unmounted" as far as this map is concerned...
        let foreign = dom_node_at(DOM1, nid(5));
        assert_eq!(
            map.resolve_dom_node_id(DOM0, foreign),
            Some(foreign),
            "an id in another DOM must never be dropped by this DOM's reconciliation"
        );
        // ...and a foreign id whose index IS in the map must not be rewritten either.
        let foreign_colliding = dom_node_at(DOM1, nid(2));
        assert_eq!(
            map.resolve_dom_node_id(DOM0, foreign_colliding),
            Some(foreign_colliding),
            "a foreign id must not be remapped just because its index appears in the map"
        );
    }

    #[test]
    fn resolve_dom_node_id_rewrites_matched_nodes_and_drops_unmounted_ones() {
        let map = NodeIdMap::from_pairs([(nid(2), nid(1))]);
        assert_eq!(
            map.resolve_dom_node_id(DOM0, dom_node_at(DOM0, nid(2))),
            Some(dom_node_at(DOM0, nid(1)))
        );
        assert_eq!(
            map.resolve_dom_node_id(DOM0, dom_node_at(DOM0, nid(1))),
            None,
            "a node absent from the map is unmounted, so the id must be dropped"
        );
    }

    /// `NodeHierarchyItemId::NONE` decodes to `None`. For the reconciled DOM
    /// that means "no node" → `None`; for a foreign DOM the pass-through branch
    /// fires first, so it survives unchanged. Both are deterministic.
    #[test]
    fn a_none_node_id_is_handled_without_panicking() {
        let map = NodeIdMap::from_pairs([(nid(0), nid(0))]);
        let none_here = DomNodeId {
            dom: DOM0,
            node: NodeHierarchyItemId::NONE,
        };
        assert_eq!(map.resolve_dom_node_id(DOM0, none_here), None);

        let none_elsewhere = DomNodeId {
            dom: DOM1,
            node: NodeHierarchyItemId::NONE,
        };
        assert_eq!(
            map.resolve_dom_node_id(DOM0, none_elsewhere),
            Some(none_elsewhere),
            "the foreign-DOM pass-through happens before the node is decoded"
        );
    }

    /// Boundary ids on both axes: the largest encodable node index and the
    /// largest `DomId`. `MAX_ENCODABLE` maps to raw `usize::MAX` in the 1-based
    /// encoding, i.e. the last value that fits.
    #[test]
    fn resolve_dom_node_id_survives_boundary_ids() {
        let map = NodeIdMap::from_pairs([
            (nid(MAX_ENCODABLE), nid(0)),
            (nid(0), nid(MAX_ENCODABLE)),
        ]);

        // Largest encodable index as the OLD id.
        let big_old = dom_node_at(DOM0, nid(MAX_ENCODABLE));
        assert_eq!(big_old.node.into_raw(), usize::MAX, "1-based encoding is saturated");
        assert_eq!(
            map.resolve_dom_node_id(DOM0, big_old),
            Some(dom_node_at(DOM0, nid(0)))
        );

        // Largest encodable index as the NEW id (re-encoding must not overflow).
        assert_eq!(
            map.resolve_dom_node_id(DOM0, dom_node_at(DOM0, nid(0))),
            Some(dom_node_at(DOM0, nid(MAX_ENCODABLE)))
        );

        // An extreme DomId is still just a DomId.
        let far_dom = dom_node_at(DOM_MAX, nid(0));
        assert_eq!(
            map.resolve_dom_node_id(DOM0, far_dom),
            Some(far_dom),
            "DomId::MAX is foreign to DOM 0 and passes through"
        );
        assert_eq!(
            map.resolve_dom_node_id(DOM_MAX, far_dom),
            Some(dom_node_at(DOM_MAX, nid(MAX_ENCODABLE))),
            "when DomId::MAX *is* the reconciled DOM, its nodes are remapped"
        );
    }

    #[test]
    fn resolve_dom_node_id_on_an_empty_map_drops_own_dom_and_keeps_foreign() {
        let map = NodeIdMap::default();
        assert_eq!(map.resolve_dom_node_id(DOM0, dom_node_at(DOM0, nid(0))), None);
        let foreign = dom_node_at(DOM1, nid(0));
        assert_eq!(map.resolve_dom_node_id(DOM0, foreign), Some(foreign));
    }

    // ------------------------------------------------------- remap_keys

    /// The reason `remap_keys` takes the map out before rebuilding it: a SWAP
    /// (`1 → 2`, `2 → 1`) is a legal reconciliation, and an in-place rewrite
    /// would overwrite one payload with the other. Both payloads must survive,
    /// on the correct keys.
    #[test]
    fn remap_keys_handles_a_swap_without_clobbering_payloads() {
        let mut map: BTreeMap<NodeId, &str> = BTreeMap::new();
        map.insert(nid(1), "one");
        map.insert(nid(2), "two");
        let node_map = NodeIdMap::from_pairs([(nid(1), nid(2)), (nid(2), nid(1))]);

        remap_keys(&mut map, &node_map);

        assert_eq!(map.len(), 2, "a swap must not lose an entry");
        assert_eq!(map.get(&nid(1)).copied(), Some("two"));
        assert_eq!(map.get(&nid(2)).copied(), Some("one"));
    }

    /// The GC half of the contract: keys absent from the map are unmounted and
    /// must be dropped, never kept "just in case".
    #[test]
    fn remap_keys_drops_state_for_unmounted_nodes() {
        let mut map: BTreeMap<NodeId, u32> = BTreeMap::new();
        map.insert(nid(1), 10);
        map.insert(nid(2), 20);
        map.insert(nid(3), 30);
        let node_map = NodeIdMap::from_pairs([(nid(2), nid(1)), (nid(3), nid(2))]);

        remap_keys(&mut map, &node_map);

        assert_eq!(map.len(), 2, "node 1's state must be GC'd");
        assert_eq!(map.get(&nid(1)).copied(), Some(20));
        assert_eq!(map.get(&nid(2)).copied(), Some(30));
        assert!(!map.contains_key(&nid(3)), "no state may remain at a dead index");
    }

    #[test]
    fn remap_keys_with_an_empty_node_map_clears_all_state() {
        let mut map: BTreeMap<NodeId, u32> = BTreeMap::new();
        map.insert(nid(0), 1);
        map.insert(nid(9), 2);

        remap_keys(&mut map, &NodeIdMap::default());

        assert!(
            map.is_empty(),
            "an empty reconciliation unmounts everything, so all state is dropped"
        );
    }

    #[test]
    fn remap_keys_with_an_identity_map_is_a_no_op() {
        let mut map: BTreeMap<NodeId, u32> = (0..64).map(|i| (nid(i), i as u32)).collect();
        let before = map.clone();
        let node_map = NodeIdMap::from_pairs((0..64).map(|i| (nid(i), nid(i))));

        remap_keys(&mut map, &node_map);

        assert_eq!(map, before);
    }

    #[test]
    fn remap_keys_on_an_empty_map_does_not_panic() {
        let mut map: BTreeMap<NodeId, u32> = BTreeMap::new();
        remap_keys(&mut map, &NodeIdMap::from_pairs([(nid(1), nid(0))]));
        assert!(map.is_empty());
    }

    /// A non-injective reconciliation (`1 → 5` and `2 → 5`) cannot be
    /// represented by a map keyed on `NodeId` — one entry must win. Assert the
    /// outcome is deterministic (source keys are visited in ascending order, so
    /// the HIGHEST old id lands last) rather than a panic or a silent duplicate.
    #[test]
    fn remap_keys_collision_is_lossy_but_deterministic() {
        let mut map: BTreeMap<NodeId, &str> = BTreeMap::new();
        map.insert(nid(1), "from-1");
        map.insert(nid(2), "from-2");
        let node_map = NodeIdMap::from_pairs([(nid(1), nid(5)), (nid(2), nid(5))]);

        remap_keys(&mut map, &node_map);

        assert_eq!(map.len(), 1, "two old keys collapsing onto one new key lose one entry");
        assert_eq!(
            map.get(&nid(5)).copied(),
            Some("from-2"),
            "the last-visited (highest) old id wins — deterministic, not arbitrary"
        );
    }

    /// The preceding-sibling delete at scale: deleting node 1 of 256 shifts every
    /// later index down by one. Every payload must land on exactly the key that
    /// now denotes its element — an off-by-one here is the misattachment bug the
    /// module doc-comment describes.
    #[test]
    fn remap_keys_preserves_payload_identity_under_a_shift_down() {
        let n = 256usize;
        let mut map: BTreeMap<NodeId, usize> = (0..n).map(|i| (nid(i), i * 1000)).collect();
        // 0 stays, 1 is deleted, 2..n shift down by one.
        let node_map = NodeIdMap::from_pairs(
            core::iter::once((nid(0), nid(0))).chain((2..n).map(|i| (nid(i), nid(i - 1)))),
        );

        remap_keys(&mut map, &node_map);

        assert_eq!(map.len(), n - 1, "exactly the deleted node's state is GC'd");
        assert_eq!(map.get(&nid(0)).copied(), Some(0));
        for i in 2..n {
            assert_eq!(
                map.get(&nid(i - 1)).copied(),
                Some(i * 1000),
                "node {i}'s payload must follow it to index {}",
                i - 1
            );
        }
        assert!(!map.contains_key(&nid(n - 1)), "the vacated tail index is empty");
    }

    /// Ordering trap: an entry is rewritten ONTO an index that another (about to
    /// be dropped) entry currently occupies. Because the source map is taken out
    /// first, the survivor must not be eaten by the corpse.
    #[test]
    fn remap_keys_survivor_moving_onto_a_dead_index_is_not_dropped() {
        let mut map: BTreeMap<NodeId, &str> = BTreeMap::new();
        map.insert(nid(1), "doomed");
        map.insert(nid(2), "survivor");
        // Node 1 is unmounted; node 2 moves into its slot.
        let node_map = NodeIdMap::from_pairs([(nid(2), nid(1))]);

        remap_keys(&mut map, &node_map);

        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&nid(1)).copied(),
            Some("survivor"),
            "the surviving payload occupies the recycled index — the dead one is gone"
        );
    }

    // --------------------------------------------------- remap_dom_keys

    #[test]
    fn remap_dom_keys_never_touches_another_dom() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        map.insert((DOM0, nid(2)), 20);
        // Same node index, different DOM — must survive verbatim.
        map.insert((DOM1, nid(2)), 99);
        // A foreign entry at an index that is unmounted in DOM 0.
        map.insert((DOM1, nid(1)), 98);
        let node_map = NodeIdMap::from_pairs([(nid(2), nid(1))]);

        remap_dom_keys(&mut map, DOM0, &node_map);

        assert_eq!(map.get(&(DOM0, nid(1))).copied(), Some(20), "DOM 0's entry is remapped");
        assert!(!map.contains_key(&(DOM0, nid(2))), "the old DOM 0 key is gone");
        assert_eq!(
            map.get(&(DOM1, nid(2))).copied(),
            Some(99),
            "DOM 1 is untouched by a DOM 0 reconciliation"
        );
        assert_eq!(map.get(&(DOM1, nid(1))).copied(), Some(98));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn remap_dom_keys_drops_unmounted_entries_of_the_target_dom_only() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        map.insert((DOM0, nid(1)), 10); // unmounted in DOM 0 -> dropped
        map.insert((DOM1, nid(1)), 11); // same index, other DOM -> kept
        let node_map = NodeIdMap::from_pairs([(nid(0), nid(0))]);

        remap_dom_keys(&mut map, DOM0, &node_map);

        assert!(!map.contains_key(&(DOM0, nid(1))));
        assert_eq!(map.get(&(DOM1, nid(1))).copied(), Some(11));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn remap_dom_keys_handles_a_swap_within_the_target_dom() {
        let mut map: BTreeMap<(DomId, NodeId), &str> = BTreeMap::new();
        map.insert((DOM0, nid(1)), "one");
        map.insert((DOM0, nid(2)), "two");
        map.insert((DOM1, nid(1)), "other-dom");
        let node_map = NodeIdMap::from_pairs([(nid(1), nid(2)), (nid(2), nid(1))]);

        remap_dom_keys(&mut map, DOM0, &node_map);

        assert_eq!(map.get(&(DOM0, nid(1))).copied(), Some("two"));
        assert_eq!(map.get(&(DOM0, nid(2))).copied(), Some("one"));
        assert_eq!(map.get(&(DOM1, nid(1))).copied(), Some("other-dom"));
        assert_eq!(map.len(), 3, "a swap plus a bystander DOM loses nothing");
    }

    #[test]
    fn remap_dom_keys_with_an_empty_node_map_clears_only_the_target_dom() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        map.insert((DOM0, nid(0)), 1);
        map.insert((DOM0, nid(5)), 2);
        map.insert((DOM1, nid(0)), 3);

        remap_dom_keys(&mut map, DOM0, &NodeIdMap::default());

        assert_eq!(map.len(), 1, "every DOM 0 node was unmounted");
        assert_eq!(map.get(&(DOM1, nid(0))).copied(), Some(3));
    }

    #[test]
    fn remap_dom_keys_on_an_empty_map_does_not_panic() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        remap_dom_keys(&mut map, DOM0, &NodeIdMap::from_pairs([(nid(1), nid(0))]));
        assert!(map.is_empty());
    }

    #[test]
    fn remap_dom_keys_with_a_target_dom_that_has_no_entries_is_a_no_op() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        map.insert((DOM1, nid(1)), 1);
        map.insert((DOM_MAX, nid(2)), 2);
        let before = map.clone();

        remap_dom_keys(&mut map, DOM0, &NodeIdMap::from_pairs([(nid(1), nid(9))]));

        assert_eq!(map, before);
    }

    #[test]
    fn remap_dom_keys_handles_boundary_dom_and_node_ids() {
        let mut map: BTreeMap<(DomId, NodeId), u32> = BTreeMap::new();
        map.insert((DOM_MAX, nid(usize::MAX)), 1);
        map.insert((DOM_MAX, nid(usize::MAX - 1)), 2);
        map.insert((DOM0, nid(usize::MAX)), 3);
        let node_map = NodeIdMap::from_pairs([(nid(usize::MAX), nid(0))]);

        remap_dom_keys(&mut map, DOM_MAX, &node_map);

        assert_eq!(
            map.get(&(DOM_MAX, nid(0))).copied(),
            Some(1),
            "usize::MAX remaps like any other index"
        );
        assert!(
            !map.contains_key(&(DOM_MAX, nid(usize::MAX - 1))),
            "unmounted in DOM_MAX -> dropped"
        );
        assert_eq!(
            map.get(&(DOM0, nid(usize::MAX))).copied(),
            Some(3),
            "DOM 0 is a bystander here"
        );
        assert_eq!(map.len(), 2);
    }

    /// Applying the SAME reconciliation twice is not idempotent in general (the
    /// second pass re-reads already-new ids as if they were old), which is why
    /// callers must run it exactly once per rebuild. Pin the one case that IS
    /// safe — the identity map — so the no-op guarantee cannot regress.
    #[test]
    fn remap_dom_keys_with_an_identity_map_is_a_no_op_even_when_repeated() {
        let mut map: BTreeMap<(DomId, NodeId), u32> =
            (0..32).map(|i| ((DOM0, nid(i)), i as u32)).collect();
        let before = map.clone();
        let node_map = NodeIdMap::from_pairs((0..32).map(|i| (nid(i), nid(i))));

        remap_dom_keys(&mut map, DOM0, &node_map);
        remap_dom_keys(&mut map, DOM0, &node_map);

        assert_eq!(map, before);
    }
}
