//! Pure functions for editing a `Vec<InlineContent>` based on selections.
//!
//! Entry points: [`edit_text`] (single edit, multiple cursors),
//! [`edit_text_multi`] (per-cursor text), and [`inspect_delete`]
//! (preview what a delete would remove).

use std::sync::Arc;

use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor,
};

use crate::text3::cache::{ContentIndex, InlineContent, StyledRun};

/// An enum representing a single text editing action.
#[derive(Debug, Clone)]
pub enum TextEdit {
    /// Insert the given string at the cursor position.
    Insert(String),
    /// Delete one grapheme cluster before the cursor (Backspace).
    DeleteBackward,
    /// Delete one grapheme cluster after the cursor (Delete key).
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

    // To handle multiple cursors correctly, we must process edits
    // from the end of the document to the beginning. This ensures that
    // earlier edits do not invalidate the indices of later edits.
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
                if cursor.cluster_id.source_run == edit_run
                    && cursor.cluster_id.start_byte_in_run >= edit_byte
                {
                    cursor.cluster_id.start_byte_in_run =
                        (cursor.cluster_id.start_byte_in_run as i32 + byte_offset_change).max(0)
                            as u32;
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
///
/// When the selection is a Range:
/// - `Insert`: deletes the range, then inserts text at the collapsed cursor
/// - `DeleteBackward`/`DeleteForward`: deletes the range ONLY (the range
///   deletion replaces the character-level delete — pressing Backspace with
///   a selection should remove the selection, not the selection + 1 char)
pub fn apply_edit_to_selection(
    content: &[InlineContent],
    selection: &Selection,
    edit: &TextEdit,
) -> (Vec<InlineContent>, TextCursor) {
    let mut new_content = content.to_vec();

    match selection {
        Selection::Range(range) => {
            // Delete the range first
            let (content_after_delete, cursor_pos) = delete_range(&new_content, range);
            match edit {
                // Insert: replace the deleted range with new text
                TextEdit::Insert(text_to_insert) => {
                    let mut c = content_after_delete;
                    insert_text(&mut c, &cursor_pos, text_to_insert)
                }
                // Delete: range deletion is sufficient — don't delete again
                TextEdit::DeleteBackward | TextEdit::DeleteForward => {
                    (content_after_delete, cursor_pos)
                }
            }
        }
        Selection::Cursor(cursor) => {
            match edit {
                TextEdit::Insert(text_to_insert) => {
                    insert_text(&mut new_content, cursor, text_to_insert)
                }
                TextEdit::DeleteBackward => delete_backward(&mut new_content, cursor),
                TextEdit::DeleteForward => delete_forward(&mut new_content, cursor),
            }
        }
    }
}

/// Absolute byte offset of a cursor within its run's text, honoring affinity.
///
/// `Leading` = at the start of the referenced grapheme cluster; `Trailing` =
/// after it. This mirrors the affinity handling in `insert_text` /
/// `delete_backward` / `delete_forward`, and is what lets a select-all range
/// (whose end cursor is `Trailing` on the last cluster) cover the whole text.
fn cursor_byte_offset_in_run(text: &str, cursor: &TextCursor) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let csb = cursor.cluster_id.start_byte_in_run as usize;
    match cursor.affinity {
        CursorAffinity::Leading => csb.min(text.len()),
        CursorAffinity::Trailing => {
            if csb >= text.len() {
                text.len()
            } else {
                text[csb..]
                    .grapheme_indices(true)
                    .next()
                    .map(|(_, g)| csb + g.len())
                    .unwrap_or(text.len())
            }
        }
    }
}

/// Deletes the content within a given range.
pub fn delete_range(
    content: &[InlineContent],
    range: &SelectionRange,
) -> (Vec<InlineContent>, TextCursor) {
    // This is a highly complex function. A full implementation needs to handle:
    //
    // - Deletions within a single text run.
    // - Deletions that span across multiple text runs.
    // - Deletions that include non-text items like images.
    //
    // For now, we provide a simplified version that handles deletion within a
    // single run.

    let mut new_content = content.to_vec();
    let start_run_idx = range.start.cluster_id.source_run as usize;
    let end_run_idx = range.end.cluster_id.source_run as usize;

    // The range may be "backward" (start after end) when the user selected
    // right-to-left, e.g. Shift+Home or Shift+Left. Normalize to [lo, hi] so the
    // deletion is direction-agnostic. The old `start_byte <= end_byte` guard
    // skipped the drain for backward ranges, so Delete/Backspace (and type-to-
    // replace) silently did nothing on such selections.
    let mut cursor_after = range.start;
    if start_run_idx == end_run_idx {
        if let Some(InlineContent::Text(run)) = new_content.get_mut(start_run_idx) {
            let a = cursor_byte_offset_in_run(&run.text, &range.start);
            let b = cursor_byte_offset_in_run(&run.text, &range.end);
            let lo = a.min(b);
            let hi = a.max(b);
            if hi <= run.text.len() && lo < hi {
                run.text.drain(lo..hi);
                // Collapse the caret to the start of the deleted region (the low
                // end), regardless of the original selection direction.
                cursor_after = TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: start_run_idx as u32,
                        start_byte_in_run: lo as u32,
                    },
                    affinity: CursorAffinity::Leading,
                };
            }
        }
    } else {
        // TODO: Handle multi-run deletion
    }

    (new_content, cursor_after) // caret at the start of the deleted range
}

/// Inserts text at a cursor position.
/// 
/// The cursor's affinity determines the exact insertion point:
/// - `Leading`: Insert at the start of the referenced cluster (start_byte_in_run)
/// - `Trailing`: Insert at the end of the referenced cluster (after the grapheme)
pub fn insert_text(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
    text_to_insert: &str,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let cluster_start_byte = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        // Calculate the actual insertion byte offset based on affinity
        let byte_offset = match cursor.affinity {
            CursorAffinity::Leading => {
                // Insert at the start of the cluster
                cluster_start_byte
            },
            CursorAffinity::Trailing => {
                // Insert at the end of the cluster - find the next grapheme boundary
                // We need to find where this grapheme cluster ends
                if cluster_start_byte >= run.text.len() {
                    // Cursor is at/past end of run - insert at end
                    run.text.len()
                } else {
                    // Find the grapheme that starts at cluster_start_byte and get its end
                    run.text[cluster_start_byte..]
                        .grapheme_indices(true)
                        .next()
                        .map(|(_, grapheme)| cluster_start_byte + grapheme.len())
                        .unwrap_or(run.text.len())
                }
            },
        };
        
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
/// 
/// The cursor's affinity determines the actual cursor position:
/// - `Leading`: Cursor is at start of cluster, delete the previous grapheme
/// - `Trailing`: Cursor is at end of cluster, delete the current grapheme
pub fn delete_backward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let cluster_start_byte = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        // Calculate the actual cursor byte offset based on affinity
        let byte_offset = match cursor.affinity {
            CursorAffinity::Leading => cluster_start_byte,
            CursorAffinity::Trailing => {
                // Cursor is at end of cluster - find the next grapheme boundary
                if cluster_start_byte >= run.text.len() {
                    run.text.len()
                } else {
                    run.text[cluster_start_byte..]
                        .grapheme_indices(true)
                        .next()
                        .map(|(_, grapheme)| cluster_start_byte + grapheme.len())
                        .unwrap_or(run.text.len())
                }
            },
        };
        
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
                    source_node_id: prev_run.source_node_id,
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
/// 
/// The cursor's affinity determines the actual cursor position:
/// - `Leading`: Cursor is at start of cluster, delete the current grapheme
/// - `Trailing`: Cursor is at end of cluster, delete the next grapheme
pub fn delete_forward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let cluster_start_byte = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        // Calculate the actual cursor byte offset based on affinity
        let byte_offset = match cursor.affinity {
            CursorAffinity::Leading => cluster_start_byte,
            CursorAffinity::Trailing => {
                // Cursor is at end of cluster - find the next grapheme boundary
                if cluster_start_byte >= run.text.len() {
                    run.text.len()
                } else {
                    run.text[cluster_start_byte..]
                        .grapheme_indices(true)
                        .next()
                        .map(|(_, grapheme)| cluster_start_byte + grapheme.len())
                        .unwrap_or(run.text.len())
                }
            },
        };
        
        if byte_offset < run.text.len() {
            let next_grapheme_end = run.text[byte_offset..]
                .grapheme_indices(true)
                .nth(1)
                .map_or(run.text.len(), |(i, _)| byte_offset + i);
            run.text.drain(byte_offset..next_grapheme_end);

            // Cursor position stays at the same byte offset but with Leading affinity
            let new_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run_idx as u32,
                    start_byte_in_run: byte_offset as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            return (new_content, new_cursor);
        } else if run_idx < content.len() - 1 {
            // Handle deleting across run boundaries (merge with next run)
            if let Some(InlineContent::Text(next_run)) = content.get(run_idx + 1).cloned() {
                let mut merged_text = run.text.clone();
                merged_text.push_str(&next_run.text);

                new_content[run_idx] = InlineContent::Text(StyledRun {
                    text: merged_text,
                    style: run.style.clone(),
                    logical_start_byte: run.logical_start_byte,
                    source_node_id: run.source_node_id,
                });
                new_content.remove(run_idx + 1);

                return (new_content, *cursor);
            }
        }
    }

    (content.to_vec(), *cursor)
}

/// Edit text with different text per selection (for N-lines-to-N-cursors paste).
///
/// Each selection gets its own text inserted. Selections are processed back-to-front
/// to avoid index invalidation. Returns the new content and updated cursors.
///
/// # Panics
///
/// Panics if `texts.len() != selections.len()`.
pub fn edit_text_multi(
    content: &[InlineContent],
    selections: &[Selection],
    texts: &[&str],
) -> (Vec<InlineContent>, Vec<Selection>) {
    assert_eq!(
        selections.len(),
        texts.len(),
        "edit_text_multi: selections and texts must have the same length"
    );

    if selections.is_empty() {
        return (content.to_vec(), Vec::new());
    }

    let mut new_content = content.to_vec();
    let mut new_selections = Vec::new();

    // Pair selections with their text, sort back-to-front
    let mut pairs: Vec<(Selection, &str)> = selections
        .iter()
        .copied()
        .zip(texts.iter().copied())
        .collect();
    pairs.sort_by(|a, b| {
        let cursor_a = match &a.0 {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        let cursor_b = match &b.0 {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        cursor_b.cluster_id.cmp(&cursor_a.cluster_id) // Reverse sort
    });

    for (selection, text) in &pairs {
        let edit = TextEdit::Insert(text.to_string());
        let (temp_content, new_cursor) =
            apply_edit_to_selection(&new_content, selection, &edit);

        let edit_run = match selection {
            Selection::Cursor(c) => c.cluster_id.source_run,
            Selection::Range(r) => r.start.cluster_id.source_run,
        };
        let edit_byte = match selection {
            Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
            Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
        };

        let byte_offset_change = text.len() as i32;

        for prev_selection in new_selections.iter_mut() {
            if let Selection::Cursor(cursor) = prev_selection {
                if cursor.cluster_id.source_run == edit_run
                    && cursor.cluster_id.start_byte_in_run >= edit_byte
                {
                    cursor.cluster_id.start_byte_in_run =
                        (cursor.cluster_id.start_byte_in_run as i32 + byte_offset_change).max(0)
                            as u32;
                }
            }
        }

        new_content = temp_content;
        new_selections.push(Selection::Cursor(new_cursor));
    }

    new_selections.reverse();
    (new_content, new_selections)
}

/// Returns the range and text that a delete operation would remove, without
/// actually modifying the content. Useful for callbacks that need to inspect
/// pending deletes. Returns `None` if nothing would be deleted.
pub fn inspect_delete(
    content: &[InlineContent],
    selection: &Selection,
    forward: bool,
) -> Option<(SelectionRange, String)> {
    match selection {
        Selection::Range(range) => {
            // If there's already a selection, that's what would be deleted
            let deleted_text = extract_text_in_range(content, range);
            Some((*range, deleted_text))
        }
        Selection::Cursor(cursor) => {
            // No selection - would delete one grapheme cluster
            if forward {
                inspect_delete_forward(content, cursor)
            } else {
                inspect_delete_backward(content, cursor)
            }
        }
    }
}

/// Inspect what would be deleted by delete-forward (Delete key)
fn inspect_delete_forward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        if byte_offset < run.text.len() {
            // Delete within same run
            let next_grapheme_end = run.text[byte_offset..]
                .grapheme_indices(true)
                .nth(1)
                .map_or(run.text.len(), |(i, _)| byte_offset + i);

            let deleted_text = run.text[byte_offset..next_grapheme_end].to_string();

            let range = SelectionRange {
                start: *cursor,
                end: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: run_idx as u32,
                        start_byte_in_run: next_grapheme_end as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
            };

            return Some((range, deleted_text));
        } else if run_idx < content.len() - 1 {
            // Would delete across run boundary
            if let Some(InlineContent::Text(next_run)) = content.get(run_idx + 1) {
                let deleted_text = next_run.text.graphemes(true).next()?.to_string();

                let next_grapheme_end = next_run
                    .text
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(next_run.text.len(), |(i, _)| i);

                let range = SelectionRange {
                    start: *cursor,
                    end: TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx + 1) as u32,
                            start_byte_in_run: next_grapheme_end as u32,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                };

                return Some((range, deleted_text));
            }
        }
    }

    None // At end of document, nothing to delete
}

/// Inspect what would be deleted by delete-backward (Backspace key)
fn inspect_delete_backward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        if byte_offset > 0 {
            // Delete within same run
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .last()
                .map_or(0, |(i, _)| i);

            let deleted_text = run.text[prev_grapheme_start..byte_offset].to_string();

            let range = SelectionRange {
                start: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: run_idx as u32,
                        start_byte_in_run: prev_grapheme_start as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
                end: *cursor,
            };

            return Some((range, deleted_text));
        } else if run_idx > 0 {
            // Would delete across run boundary
            if let Some(InlineContent::Text(prev_run)) = content.get(run_idx - 1) {
                let deleted_text = prev_run.text.graphemes(true).last()?.to_string();

                let prev_grapheme_start = prev_run.text[..]
                    .grapheme_indices(true)
                    .last()
                    .map_or(0, |(i, _)| i);

                let range = SelectionRange {
                    start: TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx - 1) as u32,
                            start_byte_in_run: prev_grapheme_start as u32,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                    end: *cursor,
                };

                return Some((range, deleted_text));
            }
        }
    }

    None // At start of document, nothing to delete
}

/// Extract the text within a selection range
fn extract_text_in_range(content: &[InlineContent], range: &SelectionRange) -> String {
    let start_run = range.start.cluster_id.source_run as usize;
    let end_run = range.end.cluster_id.source_run as usize;
    let start_byte = range.start.cluster_id.start_byte_in_run as usize;
    let end_byte = range.end.cluster_id.start_byte_in_run as usize;

    if start_run == end_run {
        // Single run
        if let Some(InlineContent::Text(run)) = content.get(start_run) {
            if start_byte <= end_byte && end_byte <= run.text.len() {
                return run.text[start_byte..end_byte].to_string();
            }
        }
    } else {
        // Multi-run selection (simplified - full implementation would handle images, etc.)
        let mut result = String::new();

        for (idx, item) in content.iter().enumerate() {
            if let InlineContent::Text(run) = item {
                if idx == start_run {
                    // First run - from start_byte to end
                    if start_byte < run.text.len() {
                        result.push_str(&run.text[start_byte..]);
                    }
                } else if idx > start_run && idx < end_run {
                    // Middle runs - entire text
                    result.push_str(&run.text);
                } else if idx == end_run {
                    // Last run - from 0 to end_byte
                    if end_byte <= run.text.len() {
                        result.push_str(&run.text[..end_byte]);
                    }
                    break;
                }
            }
        }

        return result;
    }

    String::new()
}
