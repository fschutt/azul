//! Text editing changeset system (FUTURE ARCHITECTURE - NOT YET IMPLEMENTED)
//!
//! **STATUS:** This module defines the planned architecture for a unified text editing
//! changeset system, but is not yet implemented. Current text editing works through:
//! - `text3::edit` module for text manipulation
//! - `managers::text_input` for event recording
//! - `window.rs` for integration
//!
//! This module serves as a design document for post-1.0 refactoring.
//!
//! ## Planned Architecture (Future)
//!
//! This module will implement a two-phase changeset system for all text editing operations:
//! 1. **Create changesets** (pre-callback): Analyze what would change, don't mutate yet
//! 2. **Apply changesets** (post-callback): Actually mutate state if !preventDefault
//!
//! This pattern will enable:
//! - preventDefault support for ALL operations (not just text input)
//! - Undo/redo stack (record changesets before applying)
//! - Validation (check bounds, permissions before mutation)
//! - Inspection (user callbacks can see planned changes)

use azul_core::{
    dom::DomNodeId, geom::LogicalPosition, selection::SelectionRange, task::Instant,
    window::CursorPosition,
};

use crate::managers::selection::ClipboardContent;

/// Unique identifier for a changeset (for undo/redo)
pub type ChangesetId = usize;

/// A text editing changeset that can be inspected before application
#[derive(Debug, Clone)]
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

/// Text editing operation (what will change)
#[derive(Debug, Clone)]
pub enum TextOperation {
    // Text Mutations (actually modify text)
    /// Insert text at cursor position
    InsertText {
        position: CursorPosition,
        text: String,
        new_cursor: CursorPosition,
    },

    /// Delete text in range
    DeleteText {
        range: SelectionRange,
        deleted_text: String, // For undo
        new_cursor: CursorPosition,
    },

    /// Replace text in range with new text
    ReplaceText {
        range: SelectionRange,
        old_text: String, // For undo
        new_text: String,
        new_cursor: CursorPosition,
    },

    // Selection Mutations (no text change, only selection)
    /// Set selection to new range
    SetSelection {
        old_range: Option<SelectionRange>, // For undo
        new_range: SelectionRange,
    },

    /// Extend selection in a direction
    ExtendSelection {
        old_range: SelectionRange,
        new_range: SelectionRange,
        direction: SelectionDirection,
    },

    /// Clear all selections
    ClearSelection {
        old_range: SelectionRange, // For undo
    },

    // Cursor Mutations (no text change, only cursor position)
    /// Move cursor to new position
    MoveCursor {
        old_position: CursorPosition,
        new_position: CursorPosition,
        movement: CursorMovement,
    },

    // Clipboard Operations
    /// Copy selection to clipboard (no text change)
    Copy {
        range: SelectionRange,
        content: ClipboardContent,
    },

    /// Cut selection to clipboard (deletes text)
    Cut {
        range: SelectionRange,
        content: ClipboardContent,
        new_cursor: CursorPosition,
    },

    /// Paste from clipboard (inserts text)
    Paste {
        position: CursorPosition,
        content: ClipboardContent,
        new_cursor: CursorPosition,
    },

    // Compound Operations
    /// Select all text in node
    SelectAll {
        old_range: Option<SelectionRange>, // For undo
        new_range: SelectionRange,
    },
}

/// Direction of selection extension
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionDirection {
    /// Extending selection forward (to the right/down)
    Forward,
    /// Extending selection backward (to the left/up)
    Backward,
}

/// Type of cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            TextOperation::InsertText { new_cursor, .. }
            | TextOperation::DeleteText { new_cursor, .. }
            | TextOperation::ReplaceText { new_cursor, .. }
            | TextOperation::Cut { new_cursor, .. }
            | TextOperation::Paste { new_cursor, .. }
            | TextOperation::MoveCursor {
                new_position: new_cursor,
                ..
            } => Some(*new_cursor),
            _ => None,
        }
    }

    /// Get the target selection range after this changeset is applied
    pub fn resulting_selection_range(&self) -> Option<SelectionRange> {
        match &self.operation {
            TextOperation::SetSelection { new_range, .. }
            | TextOperation::ExtendSelection { new_range, .. }
            | TextOperation::SelectAll { new_range, .. } => Some(*new_range),
            _ => None,
        }
    }
}

/// Returns the current system time using external callbacks.
fn get_current_time() -> Instant {
    let external = crate::callbacks::ExternalSystemCallbacks::rust_internal();
    (external.get_system_time_fn.cb)().into()
}

/// Creates a copy changeset from the current selection.
///
/// Extracts the selected text content and creates a `TextChangeset` with a `Copy`
/// operation. Returns `None` if there is no selection or no content to copy.
pub fn create_copy_changeset(
    target: DomNodeId,
    timestamp: Instant,
    layout_window: &crate::window::LayoutWindow,
) -> Option<TextChangeset> {
    // Extract clipboard content from current selection
    let dom_id = &target.dom;
    let content = layout_window.get_selected_content_for_clipboard(dom_id)?;

    // Get selection range for changeset
    let ranges = layout_window.selection_manager.get_ranges(dom_id);
    let range = ranges.first()?;

    Some(TextChangeset::new(
        target,
        TextOperation::Copy {
            range: *range,
            content,
        },
        timestamp,
    ))
}

/// Creates a cut changeset from the current selection.
///
/// Extracts the selected text content and creates a `TextChangeset` with a `Cut`
/// operation that will delete the selected text after copying it to clipboard.
/// Returns `None` if there is no selection or no content to cut.
pub fn create_cut_changeset(
    target: DomNodeId,
    timestamp: Instant,
    layout_window: &crate::window::LayoutWindow,
) -> Option<TextChangeset> {
    // Extract clipboard content from current selection
    let dom_id = &target.dom;
    let content = layout_window.get_selected_content_for_clipboard(dom_id)?;

    // Get selection range for changeset
    let ranges = layout_window.selection_manager.get_ranges(dom_id);
    let range = ranges.first()?;

    // The logical cursor will be at the start of the deleted range
    // SelectionManager will map this to physical coordinates
    let new_cursor_position = azul_core::window::CursorPosition::Uninitialized;

    Some(TextChangeset::new(
        target,
        TextOperation::Cut {
            range: *range,
            content,
            new_cursor: new_cursor_position,
        },
        timestamp,
    ))
}

/// Creates a paste changeset at the current cursor position.
///
/// Note: The actual clipboard content must be provided by the caller (typically
/// `event_v2.rs`), as clipboard access is platform-specific and not available
/// in the layout engine. This function currently returns `None` and paste
/// operations are initiated from `event_v2.rs` with pre-read clipboard content.
pub fn create_paste_changeset(
    target: DomNodeId,
    timestamp: Instant,
    layout_window: &crate::window::LayoutWindow,
) -> Option<TextChangeset> {
    // Paste is handled by event_v2.rs with clipboard content parameter.
    // This stub exists for API consistency with other changeset creators.
    None
}

/// Creates a select-all changeset for the target node.set for the target node.
///
/// Selects all text content in the target node from the beginning to the end.
/// Returns `None` if the node has no text content.
pub fn create_select_all_changeset(
    target: DomNodeId,
    timestamp: Instant,
    layout_window: &crate::window::LayoutWindow,
) -> Option<TextChangeset> {
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

    let dom_id = &target.dom;
    let node_id = target.node.into_crate_internal()?;

    // Get current selection (if any) for undo
    let old_range = layout_window
        .selection_manager
        .get_ranges(dom_id)
        .first()
        .copied();

    // Get the text content to determine end position
    let content = layout_window.get_text_before_textinput(*dom_id, node_id);
    let text = layout_window.extract_text_from_inline_content(&content);

    // Create selection range from start to end of text
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };

    let end_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: text.len() as u32,
        },
        affinity: CursorAffinity::Leading,
    };

    let new_range = azul_core::selection::SelectionRange {
        start: start_cursor,
        end: end_cursor,
    };

    Some(TextChangeset::new(
        target,
        TextOperation::SelectAll {
            old_range,
            new_range,
        },
        timestamp,
    ))
}

/// Creates a delete changeset for the current selection or single character.
///
/// If there is an active selection, deletes the entire selection.
/// If there is only a cursor (no selection), deletes a single character:
/// - `forward = true` (Delete key): deletes the character after the cursor
/// - `forward = false` (Backspace): deletes the character before the cursor
///
/// Returns `None` if there is nothing to delete (e.g., cursor at document boundary).
pub fn create_delete_selection_changeset(
    target: DomNodeId,
    forward: bool,
    timestamp: Instant,
    layout_window: &crate::window::LayoutWindow,
) -> Option<TextChangeset> {
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

    let dom_id = &target.dom;
    let node_id = target.node.into_crate_internal()?;

    // Get current selection/cursor
    let ranges = layout_window.selection_manager.get_ranges(dom_id);
    let cursor = layout_window.cursor_manager.get_cursor();

    // Determine what to delete
    let (delete_range, deleted_text) = if let Some(range) = ranges.first() {
        // Selection exists - delete the selection
        let content = layout_window.get_text_before_textinput(*dom_id, node_id);
        let text = layout_window.extract_text_from_inline_content(&content);

        // Extract the text in the range
        // For now, simplified: delete entire selection
        // TODO: Actually extract text between range.start and range.end
        let deleted = String::new(); // Placeholder

        (*range, deleted)
    } else if let Some(cursor_pos) = cursor {
        // No selection - delete one character
        let content = layout_window.get_text_before_textinput(*dom_id, node_id);
        let text = layout_window.extract_text_from_inline_content(&content);

        let byte_pos = cursor_pos.cluster_id.start_byte_in_run as usize;

        let (range, deleted) = if forward {
            // Delete key - delete character after cursor
            if byte_pos >= text.len() {
                return None; // At end, nothing to delete
            }
            // Delete one character forward
            let end_pos = (byte_pos + 1).min(text.len());
            let deleted = text[byte_pos..end_pos].to_string();

            let range = azul_core::selection::SelectionRange {
                start: *cursor_pos,
                end: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: cursor_pos.cluster_id.source_run,
                        start_byte_in_run: end_pos as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
            };
            (range, deleted)
        } else {
            // Backspace - delete character before cursor
            if byte_pos == 0 {
                return None; // At start, nothing to delete
            }
            // Delete one character backward
            let start_pos = byte_pos.saturating_sub(1);
            let deleted = text[start_pos..byte_pos].to_string();

            let range = azul_core::selection::SelectionRange {
                start: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: cursor_pos.cluster_id.source_run,
                        start_byte_in_run: start_pos as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
                end: *cursor_pos,
            };
            (range, deleted)
        };

        (range, deleted)
    } else {
        return None; // No cursor or selection
    };

    // New cursor position after deletion (at start of deleted range)
    let new_cursor = azul_core::window::CursorPosition::Uninitialized;

    Some(TextChangeset::new(
        target,
        TextOperation::DeleteText {
            range: delete_range,
            deleted_text,
            new_cursor,
        },
        timestamp,
    ))
}
