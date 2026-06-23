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
    dom::NodeId,
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

/// Manager for undo/redo operations across all text nodes
#[derive(Debug, Clone, Default)]
pub struct UndoRedoManager {
    /// Per-node undo/redo stacks
    /// Using Vec instead of `HashMap` for `no_std` compatibility
    pub node_stacks: Vec<NodeUndoRedoStack>,
}

impl UndoRedoManager {
    /// Create a new empty undo/redo manager
    #[must_use] pub const fn new() -> Self {
        Self {
            node_stacks: Vec::new(),
        }
    }

    /// Get or create a stack for a specific node
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

