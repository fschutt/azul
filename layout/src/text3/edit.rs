//! Pure functions for editing a `Vec<InlineContent>` based on selections.

use std::sync::Arc;

use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor,
};

use crate::text3::cache::{ContentIndex, InlineContent, StyledRun};

/// An enum representing a single text editing action.
#[derive(Debug, Clone)]
pub enum TextEdit {
    Insert(String),
    DeleteBackward,
    DeleteForward,
}

/// The primary entry point for text modification. Takes the current content and selections,
/// applies an edit, and returns the new content and the resulting cursor positions.
pub fn edit_text(
    content: &[InlineContent],
    selections: &[Selection],
    edit: &TextEdit,
) -> (Vec<InlineContent>, Vec<Selection>) {
    if selections.is_empty() {
        return (content.to_vec(), Vec::new());
    }

    let mut new_content = content.to_vec();
    let mut new_selections = Vec::new();

    // Key insight: To handle multiple cursors correctly, we must process edits
    // from the end of the document to the beginning. This ensures that earlier edits
    // do not invalidate the indices of later edits.
    let mut sorted_selections = selections.to_vec();
    sorted_selections.sort_by(|a, b| {
        let cursor_a = match a {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        let cursor_b = match b {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        cursor_b.cluster_id.cmp(&cursor_a.cluster_id) // Reverse sort
    });

    for selection in sorted_selections {
        let (mut temp_content, new_cursor) =
            apply_edit_to_selection(&new_content, &selection, edit);
        
        // When we insert/delete text, we need to adjust all previously-processed cursors
        // that come after this edit position in the same run
        let edit_run = match selection {
            Selection::Cursor(c) => c.cluster_id.source_run,
            Selection::Range(r) => r.start.cluster_id.source_run,
        };
        let edit_byte = match selection {
            Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
            Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
        };
        
        // Calculate the byte offset change
        let byte_offset_change: i32 = match edit {
            TextEdit::Insert(text) => text.len() as i32,
            TextEdit::DeleteBackward | TextEdit::DeleteForward => {
                // For simplicity, assume 1 grapheme deleted = some bytes
                // A full implementation would track actual bytes deleted
                -1
            }
        };
        
        // Adjust all previously-processed cursors in the same run that come after this position
        for prev_selection in new_selections.iter_mut() {
            if let Selection::Cursor(cursor) = prev_selection {
                if cursor.cluster_id.source_run == edit_run && cursor.cluster_id.start_byte_in_run >= edit_byte {
                    cursor.cluster_id.start_byte_in_run = 
                        (cursor.cluster_id.start_byte_in_run as i32 + byte_offset_change).max(0) as u32;
                }
            }
        }
        
        new_content = temp_content;
        new_selections.push(Selection::Cursor(new_cursor));
    }

    // The new selections were added in reverse order, so we reverse them back.
    new_selections.reverse();

    (new_content, new_selections)
}

/// Applies a single edit to a single selection.
pub fn apply_edit_to_selection(
    content: &[InlineContent],
    selection: &Selection,
    edit: &TextEdit,
) -> (Vec<InlineContent>, TextCursor) {
    let mut new_content = content.to_vec();

    // First, if the selection is a range, we perform a deletion.
    // The result of a deletion is always a single cursor.
    let cursor_after_delete = match selection {
        Selection::Range(range) => {
            let (content_after_delete, cursor_pos) = delete_range(&new_content, range);
            new_content = content_after_delete;
            cursor_pos
        }
        Selection::Cursor(cursor) => *cursor,
    };

    // Now, apply the edit at the collapsed cursor position.
    match edit {
        TextEdit::Insert(text_to_insert) => {
            insert_text(&mut new_content, &cursor_after_delete, text_to_insert)
        }
        TextEdit::DeleteBackward => delete_backward(&mut new_content, &cursor_after_delete),
        TextEdit::DeleteForward => delete_forward(&mut new_content, &cursor_after_delete),
    }
}

/// Deletes the content within a given range.
pub fn delete_range(
    content: &[InlineContent],
    range: &SelectionRange,
) -> (Vec<InlineContent>, TextCursor) {
    // This is a highly complex function. A full implementation needs to handle:
    // - Deletions within a single text run.
    // - Deletions that span across multiple text runs.
    // - Deletions that include non-text items like images.
    // For now, we provide a simplified version that handles deletion within a single run.

    let mut new_content = content.to_vec();
    let start_run_idx = range.start.cluster_id.source_run as usize;
    let end_run_idx = range.end.cluster_id.source_run as usize;

    if start_run_idx == end_run_idx {
        if let Some(InlineContent::Text(run)) = new_content.get_mut(start_run_idx) {
            let start_byte = range.start.cluster_id.start_byte_in_run as usize;
            let end_byte = range.end.cluster_id.start_byte_in_run as usize;
            if start_byte <= end_byte && end_byte <= run.text.len() {
                run.text.drain(start_byte..end_byte);
            }
        }
    } else {
        // TODO: Handle multi-run deletion
    }

    (new_content, range.start) // Return cursor at the start of the deleted range
}

/// Inserts text at a cursor position.
pub fn insert_text(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
    text_to_insert: &str,
) -> (Vec<InlineContent>, TextCursor) {
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset <= run.text.len() {
            run.text.insert_str(byte_offset, text_to_insert);

            let new_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run_idx as u32,
                    start_byte_in_run: (byte_offset + text_to_insert.len()) as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            return (new_content, new_cursor);
        }
    }

    // If insertion failed, return original state
    (content.to_vec(), *cursor)
}

/// Deletes one grapheme cluster backward from the cursor.
pub fn delete_backward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset > 0 {
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .last()
                .map_or(0, |(i, _)| i);
            run.text.drain(prev_grapheme_start..byte_offset);

            let new_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run_idx as u32,
                    start_byte_in_run: prev_grapheme_start as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            return (new_content, new_cursor);
        } else if run_idx > 0 {
            // Handle deleting across run boundaries (merge with previous run)
            if let Some(InlineContent::Text(prev_run)) = content.get(run_idx - 1).cloned() {
                let mut merged_text = prev_run.text;
                let new_cursor_byte_offset = merged_text.len();
                merged_text.push_str(&run.text);

                new_content[run_idx - 1] = InlineContent::Text(StyledRun {
                    text: merged_text,
                    style: prev_run.style,
                    logical_start_byte: prev_run.logical_start_byte,
                });
                new_content.remove(run_idx);

                let new_cursor = TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: (run_idx - 1) as u32,
                        start_byte_in_run: new_cursor_byte_offset as u32,
                    },
                    affinity: CursorAffinity::Leading,
                };
                return (new_content, new_cursor);
            }
        }
    }

    (content.to_vec(), *cursor)
}

/// Deletes one grapheme cluster forward from the cursor.
pub fn delete_forward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset < run.text.len() {
            let next_grapheme_end = run.text[byte_offset..]
                .grapheme_indices(true)
                .nth(1)
                .map_or(run.text.len(), |(i, _)| byte_offset + i);
            run.text.drain(byte_offset..next_grapheme_end);

            // Cursor position doesn't change
            return (new_content, *cursor);
        } else if run_idx < content.len() - 1 {
            // Handle deleting across run boundaries (merge with next run)
            if let Some(InlineContent::Text(next_run)) = content.get(run_idx + 1).cloned() {
                let mut merged_text = run.text.clone();
                merged_text.push_str(&next_run.text);

                new_content[run_idx] = InlineContent::Text(StyledRun {
                    text: merged_text,
                    style: run.style.clone(),
                    logical_start_byte: run.logical_start_byte,
                });
                new_content.remove(run_idx + 1);

                return (new_content, *cursor);
            }
        }
    }

    (content.to_vec(), *cursor)
}
