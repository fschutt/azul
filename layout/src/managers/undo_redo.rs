//! Undo/Redo Manager for text editing operations
//!
//! This module implements a per-node undo/redo stack that records text changesets
//! and the state before they were applied. This allows reverting changes with Ctrl+Z
//! and re-applying them with Ctrl+Y/Ctrl+Shift+Z.
//!
//! ## Architecture
//!
//! - **Per-Node Tracking**: Each text node has its own undo/redo stack
//! - **Changeset-Based**: Records TextChangesets from changeset.rs
//! - **State Snapshots**: Saves node state BEFORE changeset application (for revert)
//! - **Bounded History**: Keeps last 10 operations per node (configurable)
//! - **Callback Integration**: User can intercept via preventDefault()
//!
//! ## Usage Flow
//!
//! 1. User types text → TextChangeset created
//! 2. Pre-callback: Record current node state
//! 3. User callback: Can query/modify via CallbackInfo
//! 4. Apply changeset (if !preventDefault)
//! 5. Post-callback: Push changeset + pre-state to undo stack
//!
//! 6. User presses Ctrl+Z → Undo event detected
//! 7. Pre-callback: Pop undo stack, create revert changeset
//! 8. User callback: Can preventDefault or inspect
//! 9. Apply revert (if !preventDefault)
//! 10. Post-callback: Push original changeset to redo stack

use alloc::{collections::VecDeque, vec::Vec};

use azul_css::{impl_option, impl_option_inner, AzString};
use azul_core::{
    dom::NodeId,
    geom::LogicalPosition,
    selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor, OptionTextCursor, OptionSelectionRange},
    task::Instant,
    window::CursorPosition,
};

use super::changeset::{TextChangeset, TextOperation};

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
    /// For now, we store the logical position, not the TextCursor
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
    pub fn new(node_id: NodeId) -> Self {
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
    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Peek at the most recent undo operation without removing it
    pub fn peek_undo(&self) -> Option<&UndoableOperation> {
        self.undo_stack.back()
    }

    /// Peek at the most recent redo operation without removing it
    pub fn peek_redo(&self) -> Option<&UndoableOperation> {
        self.redo_stack.back()
    }
}

/// Manager for undo/redo operations across all text nodes
#[derive(Debug, Clone, Default)]
pub struct UndoRedoManager {
    /// Per-node undo/redo stacks
    /// Using Vec instead of HashMap for no_std compatibility
    pub node_stacks: Vec<NodeUndoRedoStack>,
}

impl UndoRedoManager {
    /// Create a new empty undo/redo manager
    pub fn new() -> Self {
        Self {
            node_stacks: Vec::new(),
        }
    }

    /// Get or create a stack for a specific node
    pub fn get_or_create_stack_mut(&mut self, node_id: NodeId) -> &mut NodeUndoRedoStack {
        // Check if stack exists
        let stack_exists = self.node_stacks.iter().any(|s| s.node_id == node_id);

        if !stack_exists {
            // Create new stack
            let stack = NodeUndoRedoStack::new(node_id);
            self.node_stacks.push(stack);
        }

        // Now find and return the stack (guaranteed to exist)
        self.node_stacks
            .iter_mut()
            .find(|s| s.node_id == node_id)
            .unwrap()
    }

    /// Get a stack for a specific node (immutable)
    pub fn get_stack(&self, node_id: NodeId) -> Option<&NodeUndoRedoStack> {
        self.node_stacks.iter().find(|s| s.node_id == node_id)
    }

    /// Get a stack for a specific node (mutable)
    fn get_stack_mut(&mut self, node_id: NodeId) -> Option<&mut NodeUndoRedoStack> {
        self.node_stacks.iter_mut().find(|s| s.node_id == node_id)
    }

    /// Record a text operation (push to undo stack)
    ///
    /// This should be called AFTER a changeset has been successfully applied.
    /// The pre_state should contain the node state BEFORE the changeset was applied.
    ///
    /// ## Arguments
    /// * `changeset` - The changeset that was applied
    /// * `pre_state` - Node state before the changeset
    pub fn record_operation(&mut self, changeset: TextChangeset, pre_state: NodeStateSnapshot) {
        // Convert DomNodeId to NodeId for indexing
        // DomNodeId contains both DomId and NodeId, we only need the NodeId
        // NodeHierarchyItemId can be converted to NodeId
        let node_id = NodeId::new(changeset.target.node.inner as usize);
        let stack = self.get_or_create_stack_mut(node_id);

        let operation = UndoableOperation {
            changeset,
            pre_state,
        };

        stack.push_undo(operation);
    }

    /// Check if undo is available for a node
    pub fn can_undo(&self, node_id: NodeId) -> bool {
        self.get_stack(node_id)
            .map(|s| s.can_undo())
            .unwrap_or(false)
    }

    /// Check if redo is available for a node
    pub fn can_redo(&self, node_id: NodeId) -> bool {
        self.get_stack(node_id)
            .map(|s| s.can_redo())
            .unwrap_or(false)
    }

    /// Peek at the next undo operation for a node (without removing it)
    ///
    /// This allows user callbacks to inspect what would be undone.
    pub fn peek_undo(&self, node_id: NodeId) -> Option<&UndoableOperation> {
        self.get_stack(node_id).and_then(|s| s.peek_undo())
    }

    /// Peek at the next redo operation for a node (without removing it)
    ///
    /// This allows user callbacks to inspect what would be redone.
    pub fn peek_redo(&self, node_id: NodeId) -> Option<&UndoableOperation> {
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
        let node_id = NodeId::new(operation.changeset.target.node.inner as usize);
        let stack = self.get_or_create_stack_mut(node_id);
        stack.push_redo(operation);
    }

    /// Push an operation to the undo stack (after successful redo)
    ///
    /// This should be called AFTER a redo operation has been successfully applied.
    pub fn push_undo(&mut self, operation: UndoableOperation) {
        let node_id = NodeId::new(operation.changeset.target.node.inner as usize);
        let stack = self.get_or_create_stack_mut(node_id);
        stack.push_undo(operation);
    }

    /// Clear all undo/redo history for a specific node
    pub fn clear_node(&mut self, node_id: NodeId) {
        if let Some(stack) = self.get_stack_mut(node_id) {
            stack.undo_stack.clear();
            stack.redo_stack.clear();
        }
    }

    /// Clear all undo/redo history for all nodes
    pub fn clear_all(&mut self) {
        self.node_stacks.clear();
    }

    /// Get the total number of operations in undo stack for a node
    pub fn undo_depth(&self, node_id: NodeId) -> usize {
        self.get_stack(node_id)
            .map(|s| s.undo_stack.len())
            .unwrap_or(0)
    }

    /// Get the total number of operations in redo stack for a node
    pub fn redo_depth(&self, node_id: NodeId) -> usize {
        self.get_stack(node_id)
            .map(|s| s.redo_stack.len())
            .unwrap_or(0)
    }
}

/// Helper function to create a revert changeset from an undoable operation.
///
/// This analyzes the changeset and creates the inverse operation that will
/// restore the pre_state.
///
/// ## Arguments
///
/// * `operation` - The operation to create a revert for
/// * `timestamp` - Current time for the revert changeset
///
/// Returns: `TextChangeset` - The changeset that reverts the operation
pub fn create_revert_changeset(operation: &UndoableOperation, timestamp: Instant) -> TextChangeset {
    use crate::managers::changeset::{
        TextOpClearSelection, TextOpCopy, TextOpCut, TextOpDeleteText, TextOpExtendSelection,
        TextOpInsertText, TextOpMoveCursor, TextOpPaste, TextOpReplaceText, TextOpSelectAll,
        TextOpSetSelection,
    };

    // Create the inverse operation based on what was done
    let revert_operation = match &operation.changeset.operation {
        // InsertText → DeleteText (remove what was inserted)
        TextOperation::InsertText(op) => {
            // To revert an insert, we need to delete the inserted text
            // The range is from old position to new position
            // For now, we use a simplified approach - restore the old text completely
            let dummy_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            };
            let end_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: operation.pre_state.text_content.len() as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            TextOperation::ReplaceText(TextOpReplaceText {
                range: SelectionRange {
                    start: dummy_cursor,
                    end: end_cursor,
                },
                old_text: op.text.clone(), // What's currently there (will be removed)
                new_text: operation.pre_state.text_content.clone(), // What to restore
                new_cursor: operation
                    .pre_state
                    .cursor_position
                    .as_ref()
                    .map(|_| {
                        CursorPosition::InWindow(azul_core::geom::LogicalPosition::new(0.0, 0.0))
                    })
                    .unwrap_or(CursorPosition::Uninitialized),
            })
        }

        // DeleteText → InsertText (re-insert what was deleted)
        TextOperation::DeleteText(op) => {
            let dummy_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            };
            TextOperation::ReplaceText(TextOpReplaceText {
                range: SelectionRange {
                    start: dummy_cursor,
                    // Empty current content
                    end: dummy_cursor,
                },
                // What's currently there (nothing)
                old_text: AzString::from(""),
                // Restore full text
                new_text: operation.pre_state.text_content.clone(),
                new_cursor: operation
                    .pre_state
                    .cursor_position
                    .as_ref()
                    .map(|_| {
                        CursorPosition::InWindow(azul_core::geom::LogicalPosition::new(0.0, 0.0))
                    })
                    .unwrap_or(CursorPosition::Uninitialized),
            })
        }

        // ReplaceText → ReplaceText (swap old and new)
        TextOperation::ReplaceText(op) => {
            let end_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: op.new_text.len() as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            TextOperation::ReplaceText(TextOpReplaceText {
                range: SelectionRange {
                    start: TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: 0,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                    end: end_cursor,
                },
                // What's currently there
                old_text: op.new_text.clone(),
                // Restore to pre-state
                new_text: operation.pre_state.text_content.clone(),
                new_cursor: operation
                    .pre_state
                    .cursor_position
                    .as_ref()
                    .map(|_| CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0)))
                    .unwrap_or(CursorPosition::Uninitialized),
            })
        }

        // For non-text-mutating operations, return the inverse
        TextOperation::SetSelection(op) => TextOperation::SetSelection(TextOpSetSelection {
            old_range: OptionSelectionRange::Some(op.new_range),
            new_range: op.old_range.into_option().unwrap_or(op.new_range),
        }),

        TextOperation::ExtendSelection(op) => TextOperation::SetSelection(TextOpSetSelection {
            old_range: OptionSelectionRange::Some(op.new_range),
            new_range: op.old_range,
        }),

        TextOperation::ClearSelection(op) => TextOperation::SetSelection(TextOpSetSelection {
            old_range: OptionSelectionRange::None,
            new_range: op.old_range,
        }),

        TextOperation::MoveCursor(op) => {
            TextOperation::MoveCursor(TextOpMoveCursor {
                old_position: op.new_position,
                new_position: op.old_position,
                movement: op.movement, // Keep same movement type
            })
        }

        // SelectAll → restore old selection
        TextOperation::SelectAll(op) => {
            if let OptionSelectionRange::Some(old_sel) = op.old_range {
                TextOperation::SetSelection(TextOpSetSelection {
                    old_range: OptionSelectionRange::Some(op.new_range),
                    new_range: old_sel,
                })
            } else {
                // If there was no selection, clear it
                // We use a zero-width selection at start
                let dummy_cursor = TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: 0,
                        start_byte_in_run: 0,
                    },
                    affinity: CursorAffinity::Leading,
                };
                TextOperation::SetSelection(TextOpSetSelection {
                    old_range: OptionSelectionRange::Some(op.new_range),
                    new_range: SelectionRange {
                        start: dummy_cursor,
                        end: dummy_cursor,
                    },
                })
            }
        }

        // Clipboard operations - these don't change text, so no revert needed
        TextOperation::Copy(_) | TextOperation::Cut(_) | TextOperation::Paste(_) => {
            // For clipboard operations, we treat them as no-op for revert
            // The actual text changes are tracked separately
            operation.changeset.operation.clone()
        }
    };

    TextChangeset::new(operation.changeset.target, revert_operation, timestamp)
}
