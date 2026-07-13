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
