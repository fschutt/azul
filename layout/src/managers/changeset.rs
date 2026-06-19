//! Text editing changeset system
//!
//! **STATUS:** The core types (`TextChangeset`, `TextOperation`, `TextOp*` structs) are
//! actively used by `window.rs`, `undo_redo.rs`, `event.rs`, and platform code.
//!
//! The live copy/cut/select-all/delete paths run through `common/event.rs`
//! (`SystemChange::CopyToClipboard`/`CutToClipboard`, `CallbackChange::SetSelectAllRange`,
//! `LayoutWindow::delete_selection`), not through changeset constructors. The earlier
//! `create_*_changeset` helpers were a never-wired parallel implementation (with
//! placeholder `deleted_text`, `CursorPosition::Uninitialized` cursors, and byte±1
//! UTF-8 deletion) and have been removed.
//!
//! ## Architecture
//!
//! This module implements a two-phase changeset system for all text editing operations:
//! 1. **Create changesets** (pre-callback): Analyze what would change, don't mutate yet
//! 2. **Apply changesets** (post-callback): Actually mutate state if !preventDefault
//!
//! This pattern enables:
//! - preventDefault support for ALL operations (not just text input)
//! - Undo/redo stack (record changesets before applying)
//! - Validation (check bounds, permissions before mutation)
//! - Inspection (user callbacks can see planned changes)

use azul_core::{
    dom::DomNodeId,
    selection::{OptionSelectionRange, SelectionRange},
    task::Instant,
    window::CursorPosition,
};
use azul_css::AzString;

use crate::managers::selection::ClipboardContent;

/// Unique identifier for a changeset (for undo/redo)
pub type ChangesetId = usize;

/// A text editing changeset that can be inspected before application
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextChangeset {
    /// Unique ID for undo/redo tracking
    pub id: ChangesetId,
    /// Target DOM node
    pub target: DomNodeId,
    /// The operation to perform
    pub operation: TextOperation,
    /// When this changeset was created
    pub timestamp: Instant,
}

/// Insert text at cursor position
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpInsertText {
    pub text: AzString,
    pub position: CursorPosition,
    pub new_cursor: CursorPosition,
}

/// Delete text in range
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpDeleteText {
    pub range: SelectionRange,
    pub deleted_text: AzString,
    pub new_cursor: CursorPosition,
}

/// Replace text in range with new text
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpReplaceText {
    pub range: SelectionRange,
    pub old_text: AzString,
    pub new_text: AzString,
    pub new_cursor: CursorPosition,
}

/// Set selection to new range
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpSetSelection {
    pub old_range: OptionSelectionRange,
    pub new_range: SelectionRange,
}

/// Extend selection in a direction
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpExtendSelection {
    pub old_range: SelectionRange,
    pub new_range: SelectionRange,
    pub direction: SelectionDirection,
}

/// Clear all selections
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpClearSelection {
    pub old_range: SelectionRange,
}

/// Move cursor to new position
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpMoveCursor {
    pub old_position: CursorPosition,
    pub new_position: CursorPosition,
    pub movement: CursorMovement,
}

/// Copy selection to clipboard (no text change)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpCopy {
    pub range: SelectionRange,
    pub content: ClipboardContent,
}

/// Cut selection to clipboard (deletes text)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpCut {
    pub range: SelectionRange,
    pub content: ClipboardContent,
    pub new_cursor: CursorPosition,
}

/// Paste from clipboard (inserts text)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpPaste {
    pub content: ClipboardContent,
    pub position: CursorPosition,
    pub new_cursor: CursorPosition,
}

/// Select all text in node
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpSelectAll {
    pub old_range: OptionSelectionRange,
    pub new_range: SelectionRange,
}

/// Text editing operation (what will change)
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum TextOperation {
    /// Insert text at cursor position
    InsertText(TextOpInsertText),
    /// Delete text in range
    DeleteText(TextOpDeleteText),
    /// Replace text in range with new text
    ReplaceText(TextOpReplaceText),
    /// Set selection to new range
    SetSelection(TextOpSetSelection),
    /// Extend selection in a direction
    ExtendSelection(TextOpExtendSelection),
    /// Clear all selections
    ClearSelection(TextOpClearSelection),
    /// Move cursor to new position
    MoveCursor(TextOpMoveCursor),
    /// Copy selection to clipboard (no text change)
    Copy(TextOpCopy),
    /// Cut selection to clipboard (deletes text)
    Cut(TextOpCut),
    /// Paste from clipboard (inserts text)
    Paste(TextOpPaste),
    /// Select all text in node
    SelectAll(TextOpSelectAll),
}

/// Re-export from events module
pub use azul_core::events::SelectionDirection;

/// Type of cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum CursorMovement {
    /// Move left one character
    Left,
    /// Move right one character
    Right,
    /// Move up one line
    Up,
    /// Move down one line
    Down,
    /// Jump to previous word boundary
    WordLeft,
    /// Jump to next word boundary
    WordRight,
    /// Jump to start of line
    LineStart,
    /// Jump to end of line
    LineEnd,
    /// Jump to start of document
    DocumentStart,
    /// Jump to end of document
    DocumentEnd,
    /// Absolute position (not relative)
    Absolute,
}

impl TextChangeset {
    /// Create a new changeset with unique ID
    pub fn new(target: DomNodeId, operation: TextOperation, timestamp: Instant) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CHANGESET_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: CHANGESET_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            target,
            operation,
            timestamp,
        }
    }

    /// Check if this changeset actually mutates text (vs just selection/cursor)
    pub fn mutates_text(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::InsertText { .. }
                | TextOperation::DeleteText { .. }
                | TextOperation::ReplaceText { .. }
                | TextOperation::Cut { .. }
                | TextOperation::Paste { .. }
        )
    }

    /// Check if this changeset changes selection (including cursor moves)
    pub fn changes_selection(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::SetSelection { .. }
                | TextOperation::ExtendSelection { .. }
                | TextOperation::ClearSelection { .. }
                | TextOperation::MoveCursor { .. }
                | TextOperation::SelectAll { .. }
        )
    }

    /// Check if this changeset involves clipboard
    pub fn uses_clipboard(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::Copy { .. } | TextOperation::Cut { .. } | TextOperation::Paste { .. }
        )
    }

    /// Get the target cursor position after this changeset is applied
    pub fn resulting_cursor_position(&self) -> Option<CursorPosition> {
        match &self.operation {
            TextOperation::InsertText(op) => Some(op.new_cursor),
            TextOperation::DeleteText(op) => Some(op.new_cursor),
            TextOperation::ReplaceText(op) => Some(op.new_cursor),
            TextOperation::Cut(op) => Some(op.new_cursor),
            TextOperation::Paste(op) => Some(op.new_cursor),
            TextOperation::MoveCursor(op) => Some(op.new_position),
            _ => None,
        }
    }

    /// Get the target selection range after this changeset is applied
    pub fn resulting_selection_range(&self) -> Option<SelectionRange> {
        match &self.operation {
            TextOperation::SetSelection(op) => Some(op.new_range),
            TextOperation::ExtendSelection(op) => Some(op.new_range),
            TextOperation::SelectAll(op) => Some(op.new_range),
            _ => None,
        }
    }
}

// The `create_copy_changeset` / `create_cut_changeset` / `create_select_all_changeset`
// / `create_delete_selection_changeset` constructors lived here. They had zero callers
// (the live editing paths run through `common/event.rs`, see the module docs) and
// contained unfinished stubs, so they were removed rather than wired up. The
// `TextOperation` payload types above are retained — they are FFI-exported (api.json)
// and used by `undo_redo.rs` / `window.rs` for the changeset/undo records.
