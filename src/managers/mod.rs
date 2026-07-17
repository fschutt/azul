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
