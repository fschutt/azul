//! Undo/Redo Manager for text editing operations
//!
//! This module implements a per-node undo/redo stack that records text changesets
//! and the state before they were applied. This allows reverting changes with Ctrl+Z
//! and re-applying them with Ctrl+Y/Ctrl+Shift+Z.
//!
//! ## Architecture
//!
//! - **Per-Node Tracking**: Each text node has its own undo/redo stack
//! - **Changeset-Based**: Records `TextChangesets` from changeset.rs
//! - **State Snapshots**: Saves node state BEFORE changeset application (for revert)
//! - **Bounded History**: Keeps last 10 operations per node (configurable)
//! - **Callback Integration**: User can intercept via `preventDefault()`
//!
//! ## Usage Flow
//!
//! 1. User types text → `TextChangeset` created
//! 2. Pre-callback: Record current node state
//! 3. User callback: Can query/modify via `CallbackInfo`
//! 4. Apply changeset (if !preventDefault)
//! 5. Post-callback: Push changeset + pre-state to undo stack
//!
//! 6. User presses Ctrl+Z → Undo event detected
//! 7. Pre-callback: Pop undo stack, create revert changeset
//! 8. User callback: Can preventDefault or inspect
//! 9. Apply revert (if !preventDefault)
//! 10. Post-callback: Push original changeset to redo stack

use alloc::{collections::VecDeque, vec::Vec};

use azul_core::{
    dom::{DomId, NodeId},
    selection::{OptionSelectionRange, OptionTextCursor},
    task::Instant,
};
use azul_css::{impl_option, impl_option_inner, AzString};

use super::changeset::TextChangeset;

/// Maximum number of undo operations to keep per node
pub const MAX_UNDO_HISTORY: usize = 10;

/// Maximum number of redo operations to keep per node
pub const MAX_REDO_HISTORY: usize = 10;

/// Snapshot of a text node's state before a changeset was applied.
///
/// This contains enough information to fully revert a text operation.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeStateSnapshot {
    /// The node this snapshot belongs to
    pub node_id: NodeId,
    /// Full text content before changeset
    pub text_content: AzString,
    /// Cursor position before changeset (if applicable)
    /// For now, we store the logical position, not the `TextCursor`
    pub cursor_position: OptionTextCursor,
    /// Selection range before changeset (if applicable)
    pub selection_range: OptionSelectionRange,
    /// When this snapshot was taken
    pub timestamp: Instant,
}

/// A recorded operation that can be undone/redone.
///
/// Combines the changeset that was applied with the state before application.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct UndoableOperation {
    /// The changeset that was applied
    pub changeset: TextChangeset,
    /// Node state BEFORE the changeset was applied
    pub pre_state: NodeStateSnapshot,
}

impl_option!(
    UndoableOperation,
    OptionUndoableOperation,
    copy = false,
    [Debug, Clone]
);

/// Per-node undo/redo stack
#[derive(Debug, Clone)]
pub struct NodeUndoRedoStack {
    /// Node ID this stack belongs to
    pub node_id: NodeId,
    /// Undo stack (most recent at back)
    pub undo_stack: VecDeque<UndoableOperation>,
    /// Redo stack (most recent at back)
    pub redo_stack: VecDeque<UndoableOperation>,
}

impl NodeUndoRedoStack {
    #[must_use] pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            undo_stack: VecDeque::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: VecDeque::with_capacity(MAX_REDO_HISTORY),
        }
    }

    /// Push a new operation to the undo stack
    pub fn push_undo(&mut self, operation: UndoableOperation) {
        // Clear redo stack when new operation is performed
        self.redo_stack.clear();

        // Add to undo stack
        self.undo_stack.push_back(operation);

        // Limit stack size
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
    }

    /// MWA-C-undo_redo: move a REDONE operation back onto the undo stack
    /// WITHOUT clearing the redo stack — `push_undo` clears it (correct for
    /// fresh user edits), which made every redo destroy the remaining redo
    /// history.
    pub fn push_undo_preserving_redo(&mut self, operation: UndoableOperation) {
        self.undo_stack.push_back(operation);
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
    }

    /// Pop the most recent operation from undo stack
    pub fn pop_undo(&mut self) -> Option<UndoableOperation> {
        self.undo_stack.pop_back()
    }

    /// Push an operation to the redo stack (after undo)
    pub fn push_redo(&mut self, operation: UndoableOperation) {
        self.redo_stack.push_back(operation);

        // Limit stack size
        if self.redo_stack.len() > MAX_REDO_HISTORY {
            self.redo_stack.pop_front();
        }
    }

    /// Pop the most recent operation from redo stack
    pub fn pop_redo(&mut self) -> Option<UndoableOperation> {
        self.redo_stack.pop_back()
    }

    /// Check if undo is available
    #[must_use] pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    #[must_use] pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Peek at the most recent undo operation without removing it
    #[must_use] pub fn peek_undo(&self) -> Option<&UndoableOperation> {
        self.undo_stack.back()
    }

    /// Peek at the most recent redo operation without removing it
    #[must_use] pub fn peek_redo(&self) -> Option<&UndoableOperation> {
        self.redo_stack.back()
    }
}

/// MWA-C-undo_redo: styled-content snapshots for an operation.
///
/// Kept OUT of the FFI-exposed `UndoableOperation` (which crosses the C API
/// via `inspect_undo_operation`) and keyed by `TextChangeset.id`. Undo restores
/// `pre`, redo restores `post` — previously both rebuilt the text with
/// `StyleProperties::default()`, discarding all styling, and redo re-entered
/// the recording pipeline (double-recording + clearing the redo stack).
#[derive(Debug, Clone)]
pub struct ContentSnapshot {
    /// Full styled inline content BEFORE the operation
    pub pre: Vec<crate::text3::cache::InlineContent>,
    /// Full styled inline content AFTER the operation
    pub post: Vec<crate::text3::cache::InlineContent>,
}

/// Bound on the styled-snapshot side table (undo+redo stacks hold at most
/// 10+10 per node; 64 gives headroom across a handful of editable nodes —
/// lookup misses fall back to the plain-text restore).
const MAX_CONTENT_SNAPSHOTS: usize = 64;

/// Manager for undo/redo operations across all text nodes
#[derive(Debug, Clone, Default)]
pub struct UndoRedoManager {
    /// Per-node undo/redo stacks
    /// Using Vec instead of `HashMap` for `no_std` compatibility
    pub node_stacks: Vec<NodeUndoRedoStack>,
    /// Styled-content snapshots keyed by `TextChangeset.id` (see
    /// [`ContentSnapshot`]); FIFO-capped at [`MAX_CONTENT_SNAPSHOTS`].
    pub content_snapshots: Vec<(super::changeset::ChangesetId, ContentSnapshot)>,
}

impl UndoRedoManager {
    /// Create a new empty undo/redo manager
    #[must_use] pub const fn new() -> Self {
        Self {
            node_stacks: Vec::new(),
            content_snapshots: Vec::new(),
        }
    }

    /// Store the styled pre/post content for a changeset (see [`ContentSnapshot`]).
    pub fn store_content_snapshot(
        &mut self,
        id: super::changeset::ChangesetId,
        pre: Vec<crate::text3::cache::InlineContent>,
        post: Vec<crate::text3::cache::InlineContent>,
    ) {
        self.content_snapshots.retain(|(existing, _)| *existing != id);
        self.content_snapshots.push((id, ContentSnapshot { pre, post }));
        if self.content_snapshots.len() > MAX_CONTENT_SNAPSHOTS {
            self.content_snapshots.remove(0);
        }
    }

    /// Look up the styled snapshot for a changeset id.
    #[must_use] pub fn get_content_snapshot(
        &self,
        id: super::changeset::ChangesetId,
    ) -> Option<&ContentSnapshot> {
        self.content_snapshots
            .iter()
            .find(|(existing, _)| *existing == id)
            .map(|(_, snap)| snap)
    }

    /// MWA-C-undo_redo: put a redone operation back on the undo stack
    /// WITHOUT clearing the redo stack (see
    /// [`NodeUndoRedoStack::push_undo_preserving_redo`]).
    /// # Panics
    /// Panics if the operation's changeset target node is None.
    pub fn reinstate_undo(&mut self, operation: UndoableOperation) {
        let node_id = operation
            .changeset
            .target
            .node
            .into_crate_internal()
            .expect("TextChangeset target node should not be None");
        let stack = self.get_or_create_stack_mut(node_id);
        stack.push_undo_preserving_redo(operation);
    }

    /// Get or create a stack for a specific node
    /// # Panics
    ///
    /// Panics if the per-node stack list is unexpectedly empty after insertion.
    pub fn get_or_create_stack_mut(&mut self, node_id: NodeId) -> &mut NodeUndoRedoStack {
        if let Some(pos) = self.node_stacks.iter().position(|s| s.node_id == node_id) {
            &mut self.node_stacks[pos]
        } else {
            self.node_stacks.push(NodeUndoRedoStack::new(node_id));
            self.node_stacks.last_mut().unwrap()
        }
    }

    /// Get a stack for a specific node (immutable)
    #[must_use] pub fn get_stack(&self, node_id: NodeId) -> Option<&NodeUndoRedoStack> {
        self.node_stacks.iter().find(|s| s.node_id == node_id)
    }

    /// Get a stack for a specific node (mutable)
    fn get_stack_mut(&mut self, node_id: NodeId) -> Option<&mut NodeUndoRedoStack> {
        self.node_stacks.iter_mut().find(|s| s.node_id == node_id)
    }

    /// Record a text operation (push to undo stack)
    ///
    /// This should be called AFTER a changeset has been successfully applied.
    /// The `pre_state` should contain the node state BEFORE the changeset was applied.
    ///
    /// ## Arguments
    /// * `changeset` - The changeset that was applied
    /// * `pre_state` - Node state before the changeset
    /// # Panics
    ///
    /// Panics if the changeset's target node is None.
    pub fn record_operation(&mut self, changeset: TextChangeset, pre_state: NodeStateSnapshot) {
        // Convert DomNodeId to NodeId for indexing
        // NodeHierarchyItemId.into_crate_internal() decodes the 1-based encoding to Option<NodeId>
        let node_id = changeset
            .target
            .node
            .into_crate_internal()
            .expect("TextChangeset target node should not be None");
        let stack = self.get_or_create_stack_mut(node_id);

        let operation = UndoableOperation {
            changeset,
            pre_state,
        };

        stack.push_undo(operation);
    }

    /// Check if undo is available for a node
    #[must_use] pub fn can_undo(&self, node_id: NodeId) -> bool {
        self.get_stack(node_id)
            .is_some_and(NodeUndoRedoStack::can_undo)
    }

    /// Check if redo is available for a node
    #[must_use] pub fn can_redo(&self, node_id: NodeId) -> bool {
        self.get_stack(node_id)
            .is_some_and(NodeUndoRedoStack::can_redo)
    }

    /// Peek at the next undo operation for a node (without removing it)
    ///
    /// This allows user callbacks to inspect what would be undone.
    #[must_use] pub fn peek_undo(&self, node_id: NodeId) -> Option<&UndoableOperation> {
        self.get_stack(node_id).and_then(|s| s.peek_undo())
    }

    /// Peek at the next redo operation for a node (without removing it)
    ///
    /// This allows user callbacks to inspect what would be redone.
    #[must_use] pub fn peek_redo(&self, node_id: NodeId) -> Option<&UndoableOperation> {
        self.get_stack(node_id).and_then(|s| s.peek_redo())
    }

    /// Pop an operation from the undo stack
    ///
    /// This should be called during undo processing to get the operation to revert.
    /// After reverting, the operation should be pushed to the redo stack.
    ///
    /// ## Returns
    /// * `Some(operation)` - The operation to undo
    /// * `None` - No undo history available
    pub fn pop_undo(&mut self, node_id: NodeId) -> Option<UndoableOperation> {
        self.get_stack_mut(node_id)?.pop_undo()
    }

    /// Pop an operation from the redo stack
    ///
    /// This should be called during redo processing to get the operation to re-apply.
    /// After re-applying, the operation should be pushed to the undo stack.
    ///
    /// ## Returns
    /// * `Some(operation)` - The operation to redo
    /// * `None` - No redo history available
    pub fn pop_redo(&mut self, node_id: NodeId) -> Option<UndoableOperation> {
        self.get_stack_mut(node_id)?.pop_redo()
    }

    /// Push an operation to the redo stack (after successful undo)
    ///
    /// This should be called AFTER an undo operation has been successfully applied.
    /// # Panics
    ///
    /// Panics if the operation's changeset target node is None.
    pub fn push_redo(&mut self, operation: UndoableOperation) {
        let node_id = operation
            .changeset
            .target
            .node
            .into_crate_internal()
            .expect("TextChangeset target node should not be None");
        let stack = self.get_or_create_stack_mut(node_id);
        stack.push_redo(operation);
    }

    /// Push an operation to the undo stack (after successful redo)
    ///
    /// This should be called AFTER a redo operation has been successfully applied.
    /// # Panics
    ///
    /// Panics if the operation's changeset target node is None.
    pub fn push_undo(&mut self, operation: UndoableOperation) {
        let node_id = operation
            .changeset
            .target
            .node
            .into_crate_internal()
            .expect("TextChangeset target node should not be None");
        let stack = self.get_or_create_stack_mut(node_id);
        stack.push_undo(operation);
    }

}

impl crate::managers::NodeIdRemap for UndoRedoManager {
    /// Remap the per-node undo/redo stacks after a DOM rebuild.
    ///
    /// This is the worst offender of the "stale manager" family: the stacks are
    /// keyed by a bare `NodeId`, so deleting a PRECEDING SIBLING (which shifts
    /// every following index down by one) used to leave the whole undo history
    /// silently re-attached to a DIFFERENT, still-live element — undo would edit
    /// the wrong node, with no panic and no error.
    ///
    /// A stack is attributed to a DOM through the `DomNodeId` target of its
    /// operations (an empty stack carries no information and is treated as
    /// belonging to the DOM being reconciled). Stacks for unmounted nodes are
    /// dropped, and the content snapshots they referenced are GC'd with them.
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        let old_stacks = core::mem::take(&mut self.node_stacks);

        for mut stack in old_stacks {
            // Which DOM does this stack belong to? Derived from its operations.
            let stack_dom = stack
                .undo_stack
                .front()
                .or_else(|| stack.redo_stack.front())
                .map(|op| op.changeset.target.dom);

            if stack_dom.is_some_and(|d| d != dom) {
                // Belongs to a different DOM — this reconciliation says nothing about it.
                self.node_stacks.push(stack);
                continue;
            }

            let Some(new_node_id) = map.resolve(stack.node_id) else {
                // Node unmounted — drop the whole history (GC). Keeping it would
                // re-attach this history to whichever node inherits the index.
                continue;
            };

            stack.node_id = new_node_id;
            for op in stack.undo_stack.iter_mut().chain(stack.redo_stack.iter_mut()) {
                remap_operation(op, dom, map, new_node_id);
            }
            self.node_stacks.push(stack);
        }

        // GC content snapshots whose changesets no longer exist in any stack.
        let live: alloc::collections::BTreeSet<_> = self
            .node_stacks
            .iter()
            .flat_map(|s| s.undo_stack.iter().chain(s.redo_stack.iter()))
            .map(|op| op.changeset.id)
            .collect();
        self.content_snapshots.retain(|(id, _)| live.contains(id));
    }
}

/// Rewrite the `NodeIds` embedded inside a single undoable operation.
fn remap_operation(
    op: &mut UndoableOperation,
    dom: DomId,
    map: &crate::managers::NodeIdMap,
    new_node_id: NodeId,
) {
    use azul_core::styled_dom::NodeHierarchyItemId;
    if op.changeset.target.dom == dom {
        op.changeset.target.node = NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
    }
    if let Some(remapped) = map.resolve(op.pre_state.node_id) {
        op.pre_state.node_id = remapped;
    } else {
        // The snapshot's node vanished but the stack's node survived: keep the
        // stack coherent by pointing the snapshot at the (surviving) stack node.
        op.pre_state.node_id = new_node_id;
    }
}


#[cfg(test)]
mod undo_redo_tests {
    use super::*;
    use crate::managers::changeset::{TextChangeset, TextOpInsertText, TextOperation};
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::styled_dom::NodeHierarchyItemId;
    use azul_core::task::SystemTick;
    use azul_core::window::CursorPosition;

    fn ts() -> Instant {
        Instant::Tick(SystemTick { tick_counter: 0 })
    }

    fn op(id: usize, node: usize) -> UndoableOperation {
        UndoableOperation {
            changeset: TextChangeset {
                id,
                target: DomNodeId {
                    dom: DomId { inner: 0 },
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
                },
                operation: TextOperation::InsertText(TextOpInsertText {
                    text: "x".into(),
                    position: CursorPosition::Uninitialized,
                    new_cursor: CursorPosition::Uninitialized,
                }),
                timestamp: ts(),
            },
            pre_state: NodeStateSnapshot {
                node_id: NodeId::new(node),
                text_content: "".into(),
                cursor_position: None.into(),
                selection_range: None.into(),
                timestamp: ts(),
            },
        }
    }

    #[test]
    fn push_undo_clears_redo_but_reinstate_preserves_it() {
        let mut stack = NodeUndoRedoStack::new(NodeId::new(1));
        stack.push_redo(op(1, 1));
        stack.push_redo(op(2, 1));
        assert_eq!(stack.redo_stack.len(), 2);

        // Fresh user edit: redo history is invalidated.
        stack.push_undo(op(3, 1));
        assert_eq!(stack.redo_stack.len(), 0);

        // Redone operation moving back to undo: remaining redos survive.
        stack.push_redo(op(4, 1));
        stack.push_redo(op(5, 1));
        stack.push_undo_preserving_redo(op(6, 1));
        assert_eq!(stack.redo_stack.len(), 2);
        assert!(stack.can_undo());
    }

    #[test]
    fn content_snapshots_replace_lookup_and_evict() {
        let mut mgr = UndoRedoManager::new();
        mgr.store_content_snapshot(7, Vec::new(), Vec::new());
        assert!(mgr.get_content_snapshot(7).is_some());
        assert!(mgr.get_content_snapshot(8).is_none());

        // Same id replaces, no duplicate entries.
        mgr.store_content_snapshot(7, Vec::new(), Vec::new());
        assert_eq!(mgr.content_snapshots.len(), 1);

        // FIFO cap: oldest evicted once over MAX_CONTENT_SNAPSHOTS.
        for id in 100..(100 + MAX_CONTENT_SNAPSHOTS) {
            mgr.store_content_snapshot(id, Vec::new(), Vec::new());
        }
        assert!(mgr.content_snapshots.len() <= MAX_CONTENT_SNAPSHOTS);
        assert!(mgr.get_content_snapshot(7).is_none(), "oldest entry evicted");
        assert!(mgr
            .get_content_snapshot(100 + MAX_CONTENT_SNAPSHOTS - 1)
            .is_some());
    }

    #[test]
    fn reinstate_undo_keeps_manager_redo_stack() {
        let mut mgr = UndoRedoManager::new();
        mgr.record_operation(op(1, 3).changeset, op(1, 3).pre_state);
        let popped = mgr.pop_undo(NodeId::new(3)).unwrap();
        mgr.push_redo(popped);
        let redone = mgr.pop_redo(NodeId::new(3)).unwrap();
        // Second redo entry that must survive the reinstate:
        mgr.push_redo(op(2, 3));
        mgr.reinstate_undo(redone);
        assert!(mgr.can_undo(NodeId::new(3)));
        assert!(mgr.can_redo(NodeId::new(3)), "redo stack preserved");
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::DomNodeId,
        geom::LogicalPosition,
        selection::{CursorAffinity, GraphemeClusterId, TextCursor},
        styled_dom::NodeHierarchyItemId,
        task::SystemTick,
        window::CursorPosition,
    };

    use super::*;
    use crate::{
        managers::{
            changeset::{TextOpInsertText, TextOperation},
            NodeIdMap, NodeIdRemap,
        },
        text3::cache::{InlineContent, InlineSpace},
    };

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    fn tick(t: u64) -> Instant {
        Instant::Tick(SystemTick { tick_counter: t })
    }

    fn cursor(run: u32, byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    /// Full operation with explicitly-chosen dom / node / changeset id / text.
    fn op_full(id: usize, dom: usize, node: usize, text: &str) -> UndoableOperation {
        UndoableOperation {
            changeset: TextChangeset {
                id,
                target: DomNodeId {
                    dom: DomId { inner: dom },
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
                },
                operation: TextOperation::InsertText(TextOpInsertText {
                    text: text.into(),
                    position: CursorPosition::Uninitialized,
                    new_cursor: CursorPosition::Uninitialized,
                }),
                timestamp: tick(u64::from(id as u32)),
            },
            pre_state: NodeStateSnapshot {
                node_id: NodeId::new(node),
                text_content: text.into(),
                cursor_position: Some(cursor(0, 0)).into(),
                selection_range: None.into(),
                timestamp: tick(u64::from(id as u32)),
            },
        }
    }

    /// Operation on DOM 0 (the common case).
    fn op(id: usize, node: usize) -> UndoableOperation {
        op_full(id, 0, node, "x")
    }

    /// Operation whose changeset target node is `None` — the input every
    /// `expect()` in this module is documented to panic on.
    fn op_with_none_target(id: usize) -> UndoableOperation {
        let mut o = op(id, 0);
        o.changeset.target.node = NodeHierarchyItemId::from_crate_internal(None);
        o
    }

    fn text_of(o: &UndoableOperation) -> &str {
        match &o.changeset.operation {
            TextOperation::InsertText(i) => i.text.as_str(),
            _ => panic!("helper only builds InsertText operations"),
        }
    }

    fn target_node(o: &UndoableOperation) -> Option<NodeId> {
        o.changeset.target.node.into_crate_internal()
    }

    fn space(width: f32) -> InlineContent {
        InlineContent::Space(InlineSpace {
            width,
            is_breaking: false,
            is_stretchy: false,
        })
    }

    fn space_width(c: &InlineContent) -> f32 {
        match c {
            InlineContent::Space(s) => s.width,
            _ => panic!("expected an InlineContent::Space"),
        }
    }

    fn one(c: InlineContent) -> Vec<InlineContent> {
        vec![c]
    }

    // ---------------------------------------------------------------------
    // NodeUndoRedoStack — constructor + invariants
    // ---------------------------------------------------------------------

    #[test]
    fn stack_new_holds_invariants_for_extreme_node_ids() {
        // NodeId::ZERO, a normal id, and the largest id that still survives the
        // 1-based FFI encoding (`usize::MAX` would overflow `into_raw`).
        for node in [0usize, 7, usize::MAX - 1] {
            let s = NodeUndoRedoStack::new(NodeId::new(node));
            assert_eq!(s.node_id.index(), node);
            assert!(s.undo_stack.is_empty());
            assert!(s.redo_stack.is_empty());
            assert!(!s.can_undo());
            assert!(!s.can_redo());
            assert!(s.peek_undo().is_none());
            assert!(s.peek_redo().is_none());
            assert!(s.undo_stack.capacity() >= MAX_UNDO_HISTORY);
            assert!(s.redo_stack.capacity() >= MAX_REDO_HISTORY);
        }
    }

    #[test]
    fn stack_draining_an_empty_stack_never_panics() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(0));
        for _ in 0..100 {
            assert!(s.pop_undo().is_none());
            assert!(s.pop_redo().is_none());
            assert!(!s.can_undo());
            assert!(!s.can_redo());
        }
    }

    // ---------------------------------------------------------------------
    // NodeUndoRedoStack — bounded history (saturation, not growth)
    // ---------------------------------------------------------------------

    #[test]
    fn undo_stack_saturates_at_max_history_and_evicts_oldest() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(1));
        let total = MAX_UNDO_HISTORY * 3 + 7;
        for id in 0..total {
            s.push_undo(op(id, 1));
        }
        assert_eq!(s.undo_stack.len(), MAX_UNDO_HISTORY, "history is bounded");
        // FIFO eviction: the surviving window is the LAST MAX_UNDO_HISTORY pushes.
        assert_eq!(s.undo_stack.front().unwrap().changeset.id, total - MAX_UNDO_HISTORY);
        assert_eq!(s.peek_undo().unwrap().changeset.id, total - 1);
    }

    #[test]
    fn redo_stack_saturates_at_max_history_and_evicts_oldest() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(1));
        let total = MAX_REDO_HISTORY * 2 + 3;
        for id in 0..total {
            s.push_redo(op(id, 1));
        }
        assert_eq!(s.redo_stack.len(), MAX_REDO_HISTORY);
        assert_eq!(s.redo_stack.front().unwrap().changeset.id, total - MAX_REDO_HISTORY);
        assert_eq!(s.peek_redo().unwrap().changeset.id, total - 1);
    }

    #[test]
    fn push_undo_preserving_redo_is_bounded_too_and_leaves_redo_alone() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(1));
        for id in 0..MAX_REDO_HISTORY {
            s.push_redo(op(id, 1));
        }
        for id in 1000..(1000 + MAX_UNDO_HISTORY * 4) {
            s.push_undo_preserving_redo(op(id, 1));
        }
        assert_eq!(s.undo_stack.len(), MAX_UNDO_HISTORY);
        assert_eq!(
            s.redo_stack.len(),
            MAX_REDO_HISTORY,
            "preserving variant must not touch the redo stack"
        );
        assert_eq!(s.peek_redo().unwrap().changeset.id, MAX_REDO_HISTORY - 1);
    }

    #[test]
    fn push_undo_clears_a_full_redo_stack() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(1));
        for id in 0..MAX_REDO_HISTORY {
            s.push_redo(op(id, 1));
        }
        assert!(s.can_redo());
        s.push_undo(op(999, 1));
        assert!(!s.can_redo(), "a fresh edit invalidates the whole redo branch");
        assert!(s.redo_stack.is_empty());
        assert!(s.peek_redo().is_none());
        assert!(s.can_undo());
    }

    // ---------------------------------------------------------------------
    // NodeUndoRedoStack — LIFO ordering + peek is non-destructive
    // ---------------------------------------------------------------------

    #[test]
    fn pop_undo_and_pop_redo_are_lifo_then_drain_to_none() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(2));
        for id in 0..5 {
            s.push_undo(op(id, 2));
        }
        for expected in (0..5).rev() {
            assert_eq!(s.pop_undo().unwrap().changeset.id, expected);
        }
        assert!(s.pop_undo().is_none());

        for id in 0..5 {
            s.push_redo(op(id, 2));
        }
        for expected in (0..5).rev() {
            assert_eq!(s.pop_redo().unwrap().changeset.id, expected);
        }
        assert!(s.pop_redo().is_none());
    }

    #[test]
    fn peek_is_non_destructive_and_agrees_with_pop() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(2));
        s.push_undo(op(1, 2));
        s.push_undo(op(2, 2));
        s.push_redo(op(3, 2));

        for _ in 0..10 {
            assert_eq!(s.peek_undo().unwrap().changeset.id, 2);
            assert_eq!(s.peek_redo().unwrap().changeset.id, 3);
        }
        assert_eq!(s.undo_stack.len(), 2, "peek must not consume");
        assert_eq!(s.redo_stack.len(), 1);
        assert_eq!(s.pop_undo().unwrap().changeset.id, 2, "peek == next pop");
        assert_eq!(s.pop_redo().unwrap().changeset.id, 3);
    }

    #[test]
    fn can_undo_can_redo_track_emptiness_exactly() {
        let mut s = NodeUndoRedoStack::new(NodeId::new(2));
        assert!(!s.can_undo() && !s.can_redo());

        s.push_undo(op(1, 2));
        assert!(s.can_undo() && !s.can_redo());
        assert_eq!(s.can_undo(), !s.undo_stack.is_empty());

        s.push_redo(op(2, 2));
        assert!(s.can_undo() && s.can_redo());

        assert!(s.pop_undo().is_some());
        assert!(!s.can_undo() && s.can_redo());
        assert!(s.pop_redo().is_some());
        assert!(!s.can_undo() && !s.can_redo());
    }

    // ---------------------------------------------------------------------
    // Round-trip: what goes onto a stack comes back off byte-identical
    // ---------------------------------------------------------------------

    #[test]
    fn undo_redo_round_trip_preserves_unicode_payload_exactly() {
        // Combining mark, ZWJ emoji sequence, RTL override, astral plane, and a
        // NUL byte in the middle — none of which the stacks may normalize.
        let nasty = "a\u{0301}👩\u{200D}👩\u{200D}👧\u{202E}rtl\u{0000}end\u{FFFD}";
        let mut s = NodeUndoRedoStack::new(NodeId::new(4));
        s.push_undo(op_full(usize::MAX, 0, 4, nasty));

        let undone = s.pop_undo().unwrap();
        assert_eq!(text_of(&undone), nasty);
        assert_eq!(undone.pre_state.text_content.as_str(), nasty);
        assert_eq!(undone.changeset.id, usize::MAX, "ChangesetId::MAX survives");

        s.push_redo(undone);
        let redone = s.pop_redo().unwrap();
        assert_eq!(text_of(&redone), nasty, "encode == decode across undo→redo");
        assert_eq!(redone.pre_state.text_content.as_str(), nasty);
        assert_eq!(target_node(&redone), Some(NodeId::new(4)));
        assert_eq!(redone.pre_state.node_id, NodeId::new(4));
    }

    #[test]
    fn round_trip_preserves_huge_text_and_extreme_timestamps() {
        let huge: AzString = "é".repeat(50_000).into();
        assert_eq!(huge.as_str().len(), 100_000, "2 bytes per 'é'");

        let mut o = op(1, 6);
        o.pre_state.text_content = huge.clone();
        o.pre_state.timestamp = tick(u64::MAX);
        o.changeset.timestamp = tick(u64::MAX);

        let mut s = NodeUndoRedoStack::new(NodeId::new(6));
        s.push_undo(o);
        let back = s.pop_undo().unwrap();
        assert_eq!(back.pre_state.text_content.as_str().len(), 100_000);
        assert_eq!(back.pre_state.text_content.as_str(), huge.as_str());
        match back.pre_state.timestamp {
            Instant::Tick(t) => assert_eq!(t.tick_counter, u64::MAX, "u64::MAX tick unclamped"),
            Instant::System(_) => panic!("helper builds Tick instants"),
        }
    }

    #[test]
    fn round_trip_preserves_nan_and_infinite_cursor_coordinates() {
        let mut o = op(1, 6);
        o.changeset.operation = TextOperation::InsertText(TextOpInsertText {
            text: "q".into(),
            position: CursorPosition::InWindow(LogicalPosition {
                x: f32::NAN,
                y: f32::NEG_INFINITY,
            }),
            new_cursor: CursorPosition::OutOfWindow(LogicalPosition {
                x: f32::INFINITY,
                y: -0.0,
            }),
        });

        let mut s = NodeUndoRedoStack::new(NodeId::new(6));
        s.push_undo(o);
        let back = s.pop_undo().unwrap();
        match back.changeset.operation {
            TextOperation::InsertText(i) => {
                match i.position {
                    CursorPosition::InWindow(p) => {
                        assert!(p.x.is_nan(), "NaN must not be normalized away");
                        assert_eq!(p.y, f32::NEG_INFINITY);
                    }
                    _ => panic!("position variant changed across the stack"),
                }
                match i.new_cursor {
                    CursorPosition::OutOfWindow(p) => {
                        assert_eq!(p.x, f32::INFINITY);
                        assert!(
                            p.y.is_sign_negative(),
                            "-0.0 must keep its sign bit through the stack"
                        );
                    }
                    _ => panic!("new_cursor variant changed across the stack"),
                }
            }
            _ => panic!("operation variant changed across the stack"),
        }
    }

    // ---------------------------------------------------------------------
    // UndoRedoManager — constructor + queries on unknown nodes
    // ---------------------------------------------------------------------

    #[test]
    fn manager_new_and_default_are_empty_and_agree() {
        let a = UndoRedoManager::new();
        let b = UndoRedoManager::default();
        for mgr in [&a, &b] {
            assert!(mgr.node_stacks.is_empty());
            assert!(mgr.content_snapshots.is_empty());
        }
        for node in [0usize, 1, usize::MAX - 1] {
            assert!(!a.can_undo(NodeId::new(node)));
            assert!(!a.can_redo(NodeId::new(node)));
            assert!(a.get_stack(NodeId::new(node)).is_none());
            assert!(a.peek_undo(NodeId::new(node)).is_none());
            assert!(a.peek_redo(NodeId::new(node)).is_none());
        }
        assert!(a.get_content_snapshot(0).is_none());
        assert!(a.get_content_snapshot(usize::MAX).is_none());
    }

    #[test]
    fn manager_queries_on_unknown_node_return_none_without_allocating_a_stack() {
        let mut mgr = UndoRedoManager::new();
        for node in [0usize, 42, usize::MAX - 1] {
            assert!(mgr.pop_undo(NodeId::new(node)).is_none());
            assert!(mgr.pop_redo(NodeId::new(node)).is_none());
        }
        assert!(
            mgr.node_stacks.is_empty(),
            "read/pop paths must not silently create per-node stacks"
        );
    }

    #[test]
    fn get_or_create_stack_mut_is_idempotent_and_mutations_persist() {
        let mut mgr = UndoRedoManager::new();
        let boundary = NodeId::new(usize::MAX - 1);

        mgr.get_or_create_stack_mut(NodeId::new(3)).push_undo(op(1, 3));
        // Second call for the same node must reuse, not duplicate.
        mgr.get_or_create_stack_mut(NodeId::new(3)).push_undo(op(2, 3));
        assert_eq!(mgr.node_stacks.len(), 1);
        assert_eq!(mgr.get_stack(NodeId::new(3)).unwrap().undo_stack.len(), 2);

        mgr.get_or_create_stack_mut(boundary).push_undo(op(3, 0));
        assert_eq!(mgr.node_stacks.len(), 2);
        assert_eq!(mgr.get_or_create_stack_mut(boundary).node_id, boundary);
        assert_eq!(mgr.node_stacks.len(), 2, "no duplicate for the boundary id");
        assert!(mgr.can_undo(boundary));
    }

    // ---------------------------------------------------------------------
    // UndoRedoManager — recording, per-node isolation, bounds
    // ---------------------------------------------------------------------

    #[test]
    fn record_operation_is_bounded_and_isolated_per_node() {
        let mut mgr = UndoRedoManager::new();
        for id in 0..(MAX_UNDO_HISTORY * 2 + 5) {
            let o = op(id, 1);
            mgr.record_operation(o.changeset, o.pre_state);
        }
        for id in 500..503 {
            let o = op(id, 2);
            mgr.record_operation(o.changeset, o.pre_state);
        }

        assert_eq!(mgr.node_stacks.len(), 2);
        assert_eq!(
            mgr.get_stack(NodeId::new(1)).unwrap().undo_stack.len(),
            MAX_UNDO_HISTORY
        );
        assert_eq!(mgr.get_stack(NodeId::new(2)).unwrap().undo_stack.len(), 3);

        // Draining node 2 must leave node 1 untouched.
        while mgr.pop_undo(NodeId::new(2)).is_some() {}
        assert!(!mgr.can_undo(NodeId::new(2)));
        assert!(mgr.can_undo(NodeId::new(1)));
        assert_eq!(mgr.peek_undo(NodeId::new(1)).unwrap().changeset.id, MAX_UNDO_HISTORY * 2 + 4);
    }

    #[test]
    fn manager_push_undo_clears_only_the_target_nodes_redo_stack() {
        let mut mgr = UndoRedoManager::new();
        mgr.push_redo(op(1, 1));
        mgr.push_redo(op(2, 2));
        assert!(mgr.can_redo(NodeId::new(1)) && mgr.can_redo(NodeId::new(2)));

        mgr.push_undo(op(3, 1));
        assert!(!mgr.can_redo(NodeId::new(1)), "fresh edit drops node 1's redo branch");
        assert!(mgr.can_redo(NodeId::new(2)), "node 2 is an independent history");
    }

    #[test]
    fn full_undo_then_redo_cycle_returns_the_same_operation() {
        let mut mgr = UndoRedoManager::new();
        let original = op_full(11, 0, 5, "hello");
        mgr.record_operation(original.changeset.clone(), original.pre_state.clone());

        // Undo: pop from undo, push to redo.
        let undone = mgr.pop_undo(NodeId::new(5)).unwrap();
        assert_eq!(undone.changeset.id, 11);
        mgr.push_redo(undone);
        assert!(!mgr.can_undo(NodeId::new(5)));
        assert!(mgr.can_redo(NodeId::new(5)));

        // Redo: pop from redo, reinstate onto undo.
        let redone = mgr.pop_redo(NodeId::new(5)).unwrap();
        assert_eq!(text_of(&redone), "hello", "payload survives undo→redo");
        mgr.reinstate_undo(redone);
        assert!(mgr.can_undo(NodeId::new(5)));
        assert!(!mgr.can_redo(NodeId::new(5)));
        assert_eq!(mgr.peek_undo(NodeId::new(5)).unwrap().changeset.id, 11);
    }

    #[test]
    fn reinstate_undo_preserves_a_full_redo_stack() {
        let mut mgr = UndoRedoManager::new();
        for id in 0..MAX_REDO_HISTORY {
            mgr.push_redo(op(id, 9));
        }
        let redone = mgr.pop_redo(NodeId::new(9)).unwrap();
        mgr.reinstate_undo(redone);
        assert_eq!(
            mgr.get_stack(NodeId::new(9)).unwrap().redo_stack.len(),
            MAX_REDO_HISTORY - 1,
            "reinstating one redo must not wipe the remaining redo branch"
        );
        assert!(mgr.can_undo(NodeId::new(9)));
    }

    // ---------------------------------------------------------------------
    // UndoRedoManager — documented panics on a None changeset target
    // ---------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "TextChangeset target node should not be None")]
    fn record_operation_panics_on_none_target() {
        let mut mgr = UndoRedoManager::new();
        let o = op_with_none_target(1);
        mgr.record_operation(o.changeset, o.pre_state);
    }

    #[test]
    #[should_panic(expected = "TextChangeset target node should not be None")]
    fn manager_push_undo_panics_on_none_target() {
        UndoRedoManager::new().push_undo(op_with_none_target(1));
    }

    #[test]
    #[should_panic(expected = "TextChangeset target node should not be None")]
    fn manager_push_redo_panics_on_none_target() {
        UndoRedoManager::new().push_redo(op_with_none_target(1));
    }

    #[test]
    #[should_panic(expected = "TextChangeset target node should not be None")]
    fn reinstate_undo_panics_on_none_target() {
        UndoRedoManager::new().reinstate_undo(op_with_none_target(1));
    }

    // ---------------------------------------------------------------------
    // Content snapshots — lookup, replacement, FIFO cap, float payloads
    // ---------------------------------------------------------------------

    #[test]
    fn content_snapshot_does_not_swap_pre_and_post() {
        let mut mgr = UndoRedoManager::new();
        mgr.store_content_snapshot(3, one(space(1.0)), one(space(2.0)));
        let snap = mgr.get_content_snapshot(3).expect("stored id must be found");
        assert_eq!(space_width(&snap.pre[0]), 1.0, "pre must stay pre");
        assert_eq!(space_width(&snap.post[0]), 2.0, "post must stay post");
    }

    #[test]
    fn content_snapshot_preserves_nan_and_infinite_widths() {
        let mut mgr = UndoRedoManager::new();
        mgr.store_content_snapshot(1, one(space(f32::NAN)), one(space(f32::INFINITY)));
        let snap = mgr.get_content_snapshot(1).unwrap();
        assert!(space_width(&snap.pre[0]).is_nan(), "NaN width round-trips");
        assert_eq!(space_width(&snap.post[0]), f32::INFINITY);
    }

    #[test]
    fn content_snapshot_boundary_ids_and_misses() {
        let mut mgr = UndoRedoManager::new();
        mgr.store_content_snapshot(0, Vec::new(), Vec::new());
        mgr.store_content_snapshot(usize::MAX, one(space(0.0)), Vec::new());

        assert!(mgr.get_content_snapshot(0).is_some(), "id 0 is a real key, not a sentinel");
        assert!(mgr.get_content_snapshot(usize::MAX).is_some());
        assert!(mgr.get_content_snapshot(1).is_none());
        assert!(mgr.get_content_snapshot(usize::MAX - 1).is_none());
        assert_eq!(mgr.content_snapshots.len(), 2);
    }

    #[test]
    fn storing_the_same_id_replaces_the_entry_and_refreshes_its_recency() {
        let mut mgr = UndoRedoManager::new();
        for id in 0..MAX_CONTENT_SNAPSHOTS {
            mgr.store_content_snapshot(id, one(space(0.0)), Vec::new());
        }
        assert_eq!(mgr.content_snapshots.len(), MAX_CONTENT_SNAPSHOTS);

        // Re-store the OLDEST id with a new payload: it is replaced (no duplicate)
        // and moves to the back of the FIFO.
        mgr.store_content_snapshot(0, one(space(42.0)), Vec::new());
        assert_eq!(mgr.content_snapshots.len(), MAX_CONTENT_SNAPSHOTS, "no duplicate key");
        assert_eq!(space_width(&mgr.get_content_snapshot(0).unwrap().pre[0]), 42.0);

        // One more insert evicts id 1 (now the oldest), NOT the refreshed id 0.
        mgr.store_content_snapshot(9_999, Vec::new(), Vec::new());
        assert_eq!(mgr.content_snapshots.len(), MAX_CONTENT_SNAPSHOTS, "cap holds");
        assert!(mgr.get_content_snapshot(0).is_some(), "refreshed entry survived");
        assert!(mgr.get_content_snapshot(1).is_none(), "second-oldest evicted");
        assert!(mgr.get_content_snapshot(9_999).is_some());
    }

    #[test]
    fn content_snapshot_table_never_exceeds_its_cap_under_heavy_churn() {
        let mut mgr = UndoRedoManager::new();
        for id in 0..(MAX_CONTENT_SNAPSHOTS * 10) {
            mgr.store_content_snapshot(id, one(space(id as f32)), one(space(0.0)));
            assert!(mgr.content_snapshots.len() <= MAX_CONTENT_SNAPSHOTS);
        }
        assert_eq!(mgr.content_snapshots.len(), MAX_CONTENT_SNAPSHOTS);
        // The surviving window is the newest MAX_CONTENT_SNAPSHOTS ids.
        let newest = MAX_CONTENT_SNAPSHOTS * 10 - 1;
        assert!(mgr.get_content_snapshot(newest).is_some());
        assert!(mgr.get_content_snapshot(newest - MAX_CONTENT_SNAPSHOTS).is_none());
    }

    // ---------------------------------------------------------------------
    // remap_node_ids — the stale-manager failure mode
    // ---------------------------------------------------------------------

    #[test]
    fn remap_shifts_stack_and_every_embedded_node_id() {
        let mut mgr = UndoRedoManager::new();
        let o = op_full(1, 0, 3, "a");
        mgr.record_operation(o.changeset, o.pre_state);
        mgr.push_redo(op_full(2, 0, 3, "b"));
        mgr.store_content_snapshot(1, one(space(1.0)), Vec::new());

        // Preceding sibling deleted: node 3 becomes node 2.
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2))]);
        mgr.remap_node_ids(DomId { inner: 0 }, &map);

        assert!(mgr.get_stack(NodeId::new(3)).is_none(), "old key must be gone");
        let stack = mgr.get_stack(NodeId::new(2)).expect("stack re-keyed to the new id");
        assert_eq!(stack.node_id, NodeId::new(2));

        let undo = stack.peek_undo().unwrap();
        assert_eq!(target_node(undo), Some(NodeId::new(2)), "changeset target remapped");
        assert_eq!(undo.pre_state.node_id, NodeId::new(2), "snapshot node remapped");
        let redo = stack.peek_redo().unwrap();
        assert_eq!(target_node(redo), Some(NodeId::new(2)), "redo stack remapped too");
        assert_eq!(redo.pre_state.node_id, NodeId::new(2));

        assert!(
            mgr.get_content_snapshot(1).is_some(),
            "snapshot of a still-live changeset must survive the GC"
        );
    }

    #[test]
    fn remap_drops_unmounted_history_and_gcs_its_snapshots() {
        let mut mgr = UndoRedoManager::new();
        let o = op_full(1, 0, 3, "a");
        mgr.record_operation(o.changeset, o.pre_state);
        mgr.store_content_snapshot(1, one(space(1.0)), Vec::new());

        // Node 3 is not in the map → it was unmounted.
        let map = NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(0))]);
        mgr.remap_node_ids(DomId { inner: 0 }, &map);

        assert!(mgr.node_stacks.is_empty(), "history of an unmounted node is dropped");
        assert!(!mgr.can_undo(NodeId::new(3)));
        assert!(!mgr.can_undo(NodeId::new(2)), "history must NOT re-attach to a neighbour");
        assert!(
            mgr.content_snapshots.is_empty(),
            "snapshots of dropped changesets are GC'd"
        );
    }

    #[test]
    fn remap_leaves_stacks_belonging_to_another_dom_untouched() {
        let mut mgr = UndoRedoManager::new();
        // Stack on node 3 of DOM 1 (a *different* DOM from the one being rebuilt).
        let o = op_full(1, 1, 3, "a");
        mgr.record_operation(o.changeset, o.pre_state);
        mgr.store_content_snapshot(1, one(space(1.0)), Vec::new());

        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2))]);
        mgr.remap_node_ids(DomId { inner: 0 }, &map);

        let stack = mgr
            .get_stack(NodeId::new(3))
            .expect("a foreign DOM's stack keeps its key");
        assert_eq!(stack.node_id, NodeId::new(3));
        let undo = stack.peek_undo().unwrap();
        assert_eq!(target_node(undo), Some(NodeId::new(3)), "foreign target untouched");
        assert_eq!(undo.changeset.target.dom, DomId { inner: 1 });
        assert_eq!(undo.pre_state.node_id, NodeId::new(3));
        assert!(mgr.get_content_snapshot(1).is_some());
    }

    #[test]
    fn remap_of_an_empty_stack_follows_the_map_or_drops_it() {
        // An empty stack carries no DOM evidence, so it is treated as belonging to
        // the DOM being reconciled: mapped → re-keyed, unmapped → dropped.
        let mut mapped = UndoRedoManager::new();
        mapped.get_or_create_stack_mut(NodeId::new(7));
        mapped.remap_node_ids(
            DomId { inner: 0 },
            &NodeIdMap::from_pairs([(NodeId::new(7), NodeId::new(9))]),
        );
        assert!(mapped.get_stack(NodeId::new(9)).is_some());
        assert!(mapped.get_stack(NodeId::new(7)).is_none());

        let mut dropped = UndoRedoManager::new();
        dropped.get_or_create_stack_mut(NodeId::new(7));
        dropped.remap_node_ids(DomId { inner: 0 }, &NodeIdMap::from_pairs([]));
        assert!(dropped.node_stacks.is_empty());
    }

    #[test]
    fn remap_with_an_empty_map_drops_every_stack_of_that_dom() {
        let mut mgr = UndoRedoManager::new();
        for node in 0..5 {
            let o = op_full(node, 0, node, "a");
            mgr.record_operation(o.changeset, o.pre_state);
        }
        assert_eq!(mgr.node_stacks.len(), 5);

        mgr.remap_node_ids(DomId { inner: 0 }, &NodeIdMap::from_pairs([]));
        assert!(mgr.node_stacks.is_empty(), "nothing survived the rebuild");
        assert!(mgr.content_snapshots.is_empty());
    }

    #[test]
    fn remap_is_idempotent_when_the_map_is_the_identity() {
        let mut mgr = UndoRedoManager::new();
        let o = op_full(1, 0, 3, "a");
        mgr.record_operation(o.changeset, o.pre_state);

        let identity = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(3))]);
        for _ in 0..3 {
            mgr.remap_node_ids(DomId { inner: 0 }, &identity);
        }
        assert_eq!(mgr.node_stacks.len(), 1);
        let stack = mgr.get_stack(NodeId::new(3)).unwrap();
        assert_eq!(stack.undo_stack.len(), 1, "repeated remaps must not duplicate ops");
        assert_eq!(target_node(stack.peek_undo().unwrap()), Some(NodeId::new(3)));
    }

    #[test]
    fn remap_gcs_a_snapshot_whose_changeset_was_never_recorded() {
        // A snapshot stored for an operation that never made it onto a stack is
        // collected by the very next remap — documented GC behaviour, pinned here
        // so a future change to the GC predicate is a visible test failure.
        let mut mgr = UndoRedoManager::new();
        mgr.store_content_snapshot(77, one(space(1.0)), Vec::new());
        assert!(mgr.get_content_snapshot(77).is_some());

        mgr.remap_node_ids(DomId { inner: 0 }, &NodeIdMap::from_pairs([]));
        assert!(mgr.get_content_snapshot(77).is_none());
    }

    // ---------------------------------------------------------------------
    // remap_operation (private) — direct unit tests
    // ---------------------------------------------------------------------

    #[test]
    fn remap_operation_rewrites_target_and_snapshot_for_the_matching_dom() {
        let mut o = op_full(1, 0, 3, "a");
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2))]);
        remap_operation(&mut o, DomId { inner: 0 }, &map, NodeId::new(2));

        assert_eq!(target_node(&o), Some(NodeId::new(2)));
        assert_eq!(o.pre_state.node_id, NodeId::new(2));
    }

    #[test]
    fn remap_operation_leaves_a_foreign_doms_target_alone() {
        let mut o = op_full(1, 1, 3, "a");
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2))]);
        remap_operation(&mut o, DomId { inner: 0 }, &map, NodeId::new(2));

        assert_eq!(
            target_node(&o),
            Some(NodeId::new(3)),
            "target in DOM 1 must not be rewritten by a DOM 0 reconciliation"
        );
        assert_eq!(o.changeset.target.dom, DomId { inner: 1 });
        // The bare pre_state NodeId has no DOM tag, so it still follows the map.
        assert_eq!(o.pre_state.node_id, NodeId::new(2));
    }

    #[test]
    fn remap_operation_falls_back_to_the_stack_node_for_a_vanished_snapshot() {
        let mut o = op_full(1, 0, 3, "a");
        o.pre_state.node_id = NodeId::new(88); // snapshot node not in the map
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2))]);
        remap_operation(&mut o, DomId { inner: 0 }, &map, NodeId::new(2));

        assert_eq!(target_node(&o), Some(NodeId::new(2)));
        assert_eq!(
            o.pre_state.node_id,
            NodeId::new(2),
            "unresolvable snapshot node is pinned to the surviving stack node"
        );
    }

    #[test]
    fn remap_operation_handles_the_largest_encodable_node_id() {
        // NodeId(usize::MAX - 1) is the largest id the 1-based FFI encoding can
        // hold (`into_raw` computes n + 1); it must survive a remap round-trip.
        let boundary = NodeId::new(usize::MAX - 1);
        let mut o = op_full(1, 0, 3, "a");
        let map = NodeIdMap::from_pairs([(NodeId::new(3), boundary)]);
        remap_operation(&mut o, DomId { inner: 0 }, &map, boundary);

        assert_eq!(target_node(&o), Some(boundary), "encode/decode is lossless at the boundary");
        assert_eq!(o.pre_state.node_id, boundary);
        assert_eq!(target_node(&o).unwrap().index(), usize::MAX - 1);
    }

    #[test]
    fn remap_operation_is_idempotent() {
        let mut o = op_full(1, 0, 3, "a");
        let map = NodeIdMap::from_pairs([
            (NodeId::new(3), NodeId::new(2)),
            (NodeId::new(2), NodeId::new(2)),
        ]);
        remap_operation(&mut o, DomId { inner: 0 }, &map, NodeId::new(2));
        remap_operation(&mut o, DomId { inner: 0 }, &map, NodeId::new(2));

        assert_eq!(target_node(&o), Some(NodeId::new(2)), "no double-shift");
        assert_eq!(o.pre_state.node_id, NodeId::new(2));
    }
}
