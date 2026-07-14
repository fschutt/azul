//! Pure functions for editing a `Vec<InlineContent>` based on selections.
//!
//! Entry points: [`edit_text`] (single edit, multiple cursors),
//! [`edit_text_multi`] (per-cursor text), and [`inspect_delete`]
//! (preview what a delete would remove).

use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor,
};

use crate::text3::cache::{InlineContent, StyledRun};

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

const fn selection_start_run(selection: &Selection) -> u32 {
    match selection {
        Selection::Cursor(c) => c.cluster_id.source_run,
        Selection::Range(r) => r.start.cluster_id.source_run,
    }
}

const fn selection_start_byte(selection: &Selection) -> u32 {
    match selection {
        Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
        Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
    }
}

/// Sorts selections from the end of the document to the beginning so that
/// applying an edit at one selection does not invalidate the byte offsets of
/// selections still to be processed.
fn sort_selections_back_to_front(selections: &[Selection]) -> Vec<Selection> {
    let mut sorted = selections.to_vec();
    sorted.sort_by(|a, b| {
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
    sorted
}

/// Shifts every already-processed cursor sitting at or after `edit_byte` in
/// `edit_run` by `byte_offset_change`, clamping to zero.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn adjust_cursors(
    selections: &mut [Selection],
    edit_run: u32,
    edit_byte: u32,
    byte_offset_change: i32,
) {
    for sel in selections.iter_mut() {
        if let Selection::Cursor(cursor) = sel {
            if cursor.cluster_id.source_run == edit_run
                && cursor.cluster_id.start_byte_in_run >= edit_byte
            {
                cursor.cluster_id.start_byte_in_run =
                    (cursor.cluster_id.start_byte_in_run as i32 + byte_offset_change).max(0) as u32;
            }
        }
    }
}

/// Shifts the `source_run` index of every already-processed cursor that sits in a
/// run AFTER `boundary_run`, by `run_count_change` (negative when runs were removed
/// or merged), clamping so it never drops to or below the surviving boundary run.
///
/// Needed because edits do not only change byte offsets within a run — a multi-run
/// delete (or a cross-run backspace/forward-delete, or removing an inline image)
/// changes the NUMBER of runs. Since cursors are processed back-to-front, a
/// previously-processed (later-in-document) cursor whose run comes after the edit
/// would otherwise keep a stale `source_run` pointing one-or-more runs too high —
/// landing on the wrong run or going out of bounds entirely.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn adjust_cursor_runs(selections: &mut [Selection], boundary_run: u32, run_count_change: i32) {
    if run_count_change == 0 {
        return;
    }
    for sel in selections.iter_mut() {
        if let Selection::Cursor(cursor) = sel {
            if cursor.cluster_id.source_run > boundary_run {
                let shifted = (cursor.cluster_id.source_run as i32 + run_count_change)
                    .max(boundary_run as i32);
                cursor.cluster_id.source_run = shifted as u32;
            }
        }
    }
}

/// Byte length of the text in the run at `run_idx`, or 0 for non-text / missing runs.
fn run_text_len(content: &[InlineContent], run_idx: u32) -> usize {
    match content.get(run_idx as usize) {
        Some(InlineContent::Text(run)) => run.text.len(),
        _ => 0,
    }
}

/// The primary entry point for text modification. Takes the current content and selections,
/// applies an edit, and returns the new content and the resulting cursor positions.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded layout/render numeric cast
#[must_use] pub fn edit_text(
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
    let sorted_selections = sort_selections_back_to_front(selections);

    for selection in sorted_selections {
        let edit_run = selection_start_run(&selection);
        let edit_byte = selection_start_byte(&selection);

        // Measure the affected run before and after the edit so we can shift
        // previously-processed cursors by the ACTUAL byte delta. The old code
        // hardcoded -1 for any delete, which mis-tracked multi-byte graphemes.
        let old_run_len = run_text_len(&new_content, edit_run);
        let old_run_count = new_content.len();
        let (temp_content, new_cursor) =
            apply_edit_to_selection(&new_content, &selection, edit);
        let new_run_len = run_text_len(&temp_content, edit_run);
        let byte_offset_change = new_run_len as i32 - old_run_len as i32;
        let run_count_change = temp_content.len() as i32 - old_run_count as i32;

        // Adjust all previously-processed cursors in the same run that come after this position
        adjust_cursors(&mut new_selections, edit_run, edit_byte, byte_offset_change);
        // If the edit changed the run COUNT (multi-run delete / cross-run delete /
        // image removal), reindex later cursors whose run sits after this edit.
        adjust_cursor_runs(&mut new_selections, edit_run, run_count_change);

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
#[must_use] pub fn apply_edit_to_selection(
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
                    insert_text(&c, &cursor_pos, text_to_insert)
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
                    insert_text(&new_content, cursor, text_to_insert)
                }
                TextEdit::DeleteBackward => delete_backward(&new_content, cursor),
                TextEdit::DeleteForward => delete_forward(&new_content, cursor),
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
pub(crate) fn cursor_byte_offset_in_run(text: &str, cursor: &TextCursor) -> usize {
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
                    .map_or(text.len(), |(_, g)| csb + g.len())
            }
        }
    }
}

/// Deletes the content within a given range.
///
/// Handles:
/// - Deletions within a single text run.
/// - Deletions spanning multiple runs: the start/end runs are truncated, the
///   runs strictly between them are dropped, and the two truncated runs are
///   merged when they share the same style.
///
/// Non-text items (images, etc.) at the boundaries are left intact (their text
/// offset resolves to 0), while intermediate non-text items are dropped along
/// with the rest of the spanned content.
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[must_use] pub fn delete_range(
    content: &[InlineContent],
    range: &SelectionRange,
) -> (Vec<InlineContent>, TextCursor) {
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
        } else if start_run_idx < new_content.len() && range.start != range.end {
            // The selection covers a single NON-text run (inline image / object /
            // shape). A byte-offset drain can't remove it; delete the whole item and
            // collapse the caret to its former index. `range.start != range.end`
            // guards against a zero-width (collapsed) selection deleting the item.
            new_content.remove(start_run_idx);
            cursor_after = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: start_run_idx as u32,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            };
        }
    } else {
        // Multi-run deletion.
        //
        // Normalize direction so `lo` precedes `hi` in document order (the range
        // may be backward if the user selected right-to-left across runs). Then:
        //   1. truncate the start (lo) run to the text BEFORE the selection,
        //   2. truncate the end (hi) run to the text AFTER the selection,
        //   3. drop every run strictly between them,
        //   4. merge the two truncated runs when they share the same style.
        let (lo_run, lo_cursor, hi_run, hi_cursor) = if start_run_idx <= end_run_idx {
            (start_run_idx, range.start, end_run_idx, range.end)
        } else {
            (end_run_idx, range.end, start_run_idx, range.start)
        };

        // Affinity-aware byte offsets within the two boundary runs. Non-text
        // boundary runs resolve to 0 (nothing to truncate there).
        let lo_byte = match new_content.get(lo_run) {
            Some(InlineContent::Text(run)) => cursor_byte_offset_in_run(&run.text, &lo_cursor),
            _ => 0,
        };
        let hi_byte = match new_content.get(hi_run) {
            Some(InlineContent::Text(run)) => cursor_byte_offset_in_run(&run.text, &hi_cursor),
            _ => 0,
        };

        // 1. Keep only text[..lo_byte] in the start run; remember the head length
        //    (the collapse point for the caret).
        let head_len = if let Some(InlineContent::Text(run)) = new_content.get_mut(lo_run) {
            let cut = lo_byte.min(run.text.len());
            run.text.truncate(cut);
            cut
        } else {
            0
        };

        // 2. Keep only text[hi_byte..] in the end run.
        if let Some(InlineContent::Text(run)) = new_content.get_mut(hi_run) {
            let cut = hi_byte.min(run.text.len());
            run.text.drain(..cut);
        }

        // 3. Drop the intermediate runs. After draining, the end run sits at
        //    `lo_run + 1`. Clamp the end so a bogus out-of-range `hi_run` can
        //    never panic the drain.
        let drain_end = hi_run.min(new_content.len());
        if drain_end > lo_run + 1 {
            new_content.drain((lo_run + 1)..drain_end);
        }
        let tail_idx = lo_run + 1;

        // 4. Merge head and tail when both are text with matching style. Compared
        //    by value (`StyleProperties: PartialEq`) so runs that were split from
        //    one DOM element — or otherwise carry identical styling — re-join into
        //    a single run, while genuinely different styles stay separate.
        let mergeable = matches!(
            (new_content.get(lo_run), new_content.get(tail_idx)),
            (Some(InlineContent::Text(a)), Some(InlineContent::Text(b)))
                if a.style == b.style
        );
        if mergeable {
            if let InlineContent::Text(tail) = new_content.remove(tail_idx) {
                if let Some(InlineContent::Text(head)) = new_content.get_mut(lo_run) {
                    head.text.push_str(&tail.text);
                }
            }
        }

        // Collapse the caret to the join point (start of the deleted region).
        cursor_after = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: lo_run as u32,
                start_byte_in_run: head_len as u32,
            },
            affinity: CursorAffinity::Leading,
        };
    }

    (new_content, cursor_after) // caret at the start of the deleted range
}

/// Inserts text at a cursor position.
/// 
/// The cursor's affinity determines the exact insertion point:
/// - `Leading`: Insert at the start of the referenced cluster (`start_byte_in_run`)
/// - `Trailing`: Insert at the end of the referenced cluster (after the grapheme)
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[must_use]
pub fn insert_text(
    content: &[InlineContent],
    cursor: &TextCursor,
    text_to_insert: &str,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    
    let mut new_content = content.to_vec();
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
                        .map_or(run.text.len(), |(_, grapheme)| cluster_start_byte + grapheme.len())
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[allow(clippy::too_many_lines)] // cohesive grapheme-deletion routine: one branch per cursor affinity
#[must_use]
pub fn delete_backward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.to_vec();
    let run_idx = cursor.cluster_id.source_run as usize;
    let cluster_start_byte = cursor.cluster_id.start_byte_in_run as usize;

    // Non-text run (inline image / object / shape) under the cursor. A grapheme
    // drain can't act on it, so handle it explicitly instead of silently no-op'ing.
    if new_content.get(run_idx).is_some()
        && !matches!(new_content.get(run_idx), Some(InlineContent::Text(_)))
    {
        return match cursor.affinity {
            // Caret sits AFTER the item — Backspace removes the item itself.
            CursorAffinity::Trailing => {
                new_content.remove(run_idx);
                (
                    new_content,
                    TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: run_idx as u32,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                )
            }
            // Caret sits BEFORE the item — Backspace acts on the previous run.
            CursorAffinity::Leading if run_idx > 0 => {
                let prev_byte = match content.get(run_idx - 1) {
                    Some(InlineContent::Text(r)) => r.text.len() as u32,
                    _ => 0,
                };
                delete_backward(
                    content,
                    &TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx - 1) as u32,
                            start_byte_in_run: prev_byte,
                        },
                        affinity: CursorAffinity::Trailing,
                    },
                )
            }
            CursorAffinity::Leading => (content.to_vec(), *cursor),
        };
    }

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
                        .map_or(run.text.len(), |(_, grapheme)| cluster_start_byte + grapheme.len())
                }
            },
        };

        if byte_offset > 0 {
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .next_back()
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
            // Handle deleting across run boundaries.
            match content.get(run_idx - 1).cloned() {
                // Previous run is text — merge the two runs.
                Some(InlineContent::Text(prev_run)) => {
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
                // Previous run is a non-text item — Backspace removes it.
                Some(_) => {
                    new_content.remove(run_idx - 1);
                    let new_cursor = TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx - 1) as u32,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    };
                    return (new_content, new_cursor);
                }
                None => {}
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[must_use]
pub fn delete_forward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.to_vec();
    let run_idx = cursor.cluster_id.source_run as usize;
    let cluster_start_byte = cursor.cluster_id.start_byte_in_run as usize;

    // Non-text run (inline image / object / shape) under the cursor.
    if new_content.get(run_idx).is_some()
        && !matches!(new_content.get(run_idx), Some(InlineContent::Text(_)))
    {
        return match cursor.affinity {
            // Caret sits BEFORE the item — Delete removes the item itself.
            CursorAffinity::Leading => {
                new_content.remove(run_idx);
                (
                    new_content,
                    TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: run_idx as u32,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                )
            }
            // Caret sits AFTER the item — Delete acts on the next run.
            CursorAffinity::Trailing if run_idx + 1 < content.len() => delete_forward(
                content,
                &TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: (run_idx + 1) as u32,
                        start_byte_in_run: 0,
                    },
                    affinity: CursorAffinity::Leading,
                },
            ),
            CursorAffinity::Trailing => (content.to_vec(), *cursor),
        };
    }

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
                        .map_or(run.text.len(), |(_, grapheme)| cluster_start_byte + grapheme.len())
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
            // Handle deleting across run boundaries.
            match content.get(run_idx + 1).cloned() {
                // Next run is text — merge the two runs.
                Some(InlineContent::Text(next_run)) => {
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
                // Next run is a non-text item — Delete removes it.
                Some(_) => {
                    new_content.remove(run_idx + 1);
                    return (new_content, *cursor);
                }
                None => {}
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
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded layout/render numeric cast
#[must_use] pub fn edit_text_multi(
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
        let edit = TextEdit::Insert((*text).to_string());

        let edit_run = selection_start_run(selection);
        let edit_byte = selection_start_byte(selection);

        let old_run_len = run_text_len(&new_content, edit_run);
        let old_run_count = new_content.len();
        let (temp_content, new_cursor) =
            apply_edit_to_selection(&new_content, selection, &edit);
        let new_run_len = run_text_len(&temp_content, edit_run);
        let byte_offset_change = new_run_len as i32 - old_run_len as i32;
        let run_count_change = temp_content.len() as i32 - old_run_count as i32;

        adjust_cursors(&mut new_selections, edit_run, edit_byte, byte_offset_change);
        adjust_cursor_runs(&mut new_selections, edit_run, run_count_change);

        new_content = temp_content;
        new_selections.push(Selection::Cursor(new_cursor));
    }

    new_selections.reverse();
    (new_content, new_selections)
}

/// Returns the range and text that a delete operation would remove, without
/// actually modifying the content.
///
/// Useful for callbacks that need to inspect
/// pending deletes. Returns `None` if nothing would be deleted.
#[must_use] pub fn inspect_delete(
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
fn inspect_delete_forward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        // Honor cursor affinity, mirroring delete_forward — a Trailing cursor
        // sits after its grapheme, so the raw start_byte_in_run is wrong here.
        let byte_offset = cursor_byte_offset_in_run(&run.text, cursor);
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
fn inspect_delete_backward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        // Honor cursor affinity, mirroring delete_backward — a Trailing cursor
        // sits after its grapheme, so the raw start_byte_in_run is wrong here.
        let byte_offset = cursor_byte_offset_in_run(&run.text, cursor);
        if byte_offset > 0 {
            // Delete within same run
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .next_back()
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
                let deleted_text = prev_run.text.graphemes(true).next_back()?.to_string();

                let prev_grapheme_start = prev_run.text[..]
                    .grapheme_indices(true)
                    .next_back()
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::sync::Arc;

    use unicode_segmentation::UnicodeSegmentation;

    use super::*;
    use crate::text3::cache::StyleProperties;

    // ---------------------------------------------------------------- helpers

    fn style_a() -> Arc<StyleProperties> {
        Arc::new(StyleProperties::default())
    }

    /// A style that compares unequal to [`style_a`] (`StyleProperties: PartialEq`),
    /// so `delete_range`'s style-based run merge can be exercised both ways.
    fn style_b() -> Arc<StyleProperties> {
        Arc::new(StyleProperties {
            font_size_px: 99.0,
            ..StyleProperties::default()
        })
    }

    fn text(s: &str) -> InlineContent {
        InlineContent::Text(StyledRun {
            text: s.to_string(),
            style: style_a(),
            logical_start_byte: 0,
            source_node_id: None,
        })
    }

    fn text_styled(s: &str, style: Arc<StyleProperties>) -> InlineContent {
        InlineContent::Text(StyledRun {
            text: s.to_string(),
            style,
            logical_start_byte: 0,
            source_node_id: None,
        })
    }

    /// A non-text inline item (stands in for an inline image / object / shape).
    /// `Tab` is the cheapest such variant to build — it carries only a style.
    fn obj() -> InlineContent {
        InlineContent::Tab { style: style_a() }
    }

    /// `InlineContent` has no `PartialEq`, so compare a printable projection:
    /// text runs render as their text, everything else as `<obj>`.
    fn dump(content: &[InlineContent]) -> Vec<String> {
        content
            .iter()
            .map(|c| match c {
                InlineContent::Text(r) => r.text.clone(),
                _ => "<obj>".to_string(),
            })
            .collect()
    }

    fn lead(run: u32, byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn trail(run: u32, byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Trailing,
        }
    }

    fn range_sel(start: TextCursor, end: TextCursor) -> Selection {
        Selection::Range(SelectionRange { start, end })
    }

    fn cursor_of(sel: &Selection) -> TextCursor {
        match sel {
            Selection::Cursor(c) => *c,
            Selection::Range(r) => r.start,
        }
    }

    /// A ZWJ emoji family: 👨(4) + ZWJ(3) + 👩(4) + ZWJ(3) + 👧(4) = 18 bytes,
    /// but exactly ONE extended grapheme cluster.
    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

    #[test]
    fn family_constant_is_one_grapheme_of_18_bytes() {
        // Guards the fixture the grapheme tests below rely on.
        assert_eq!(FAMILY.len(), 18);
        assert_eq!(FAMILY.graphemes(true).count(), 1);
    }

    // ------------------------------------------- selection_start_run / _byte

    #[test]
    fn selection_start_run_and_byte_read_the_cursor() {
        let sel = Selection::Cursor(lead(3, 7));
        assert_eq!(selection_start_run(&sel), 3);
        assert_eq!(selection_start_byte(&sel), 7);
    }

    #[test]
    fn selection_start_run_and_byte_read_range_start_not_end() {
        // Even for a BACKWARD range (start after end) the raw `start` is reported.
        let sel = range_sel(lead(9, 40), lead(1, 2));
        assert_eq!(selection_start_run(&sel), 9);
        assert_eq!(selection_start_byte(&sel), 40);
    }

    #[test]
    fn selection_start_accessors_survive_u32_max() {
        let sel = Selection::Cursor(trail(u32::MAX, u32::MAX));
        assert_eq!(selection_start_run(&sel), u32::MAX);
        assert_eq!(selection_start_byte(&sel), u32::MAX);

        let sel = range_sel(lead(u32::MAX, u32::MAX), lead(0, 0));
        assert_eq!(selection_start_run(&sel), u32::MAX);
        assert_eq!(selection_start_byte(&sel), u32::MAX);
    }

    // ------------------------------------------ sort_selections_back_to_front

    #[test]
    fn sort_back_to_front_empty_and_single() {
        assert!(sort_selections_back_to_front(&[]).is_empty());
        let one = [Selection::Cursor(lead(0, 0))];
        assert_eq!(sort_selections_back_to_front(&one).len(), 1);
    }

    #[test]
    fn sort_back_to_front_is_descending_by_cluster_id() {
        let sels = [
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(2, 5)),
            Selection::Cursor(lead(1, 3)),
            Selection::Cursor(lead(2, 1)),
        ];
        let sorted = sort_selections_back_to_front(&sels);
        let keys: Vec<(u32, u32)> = sorted
            .iter()
            .map(|s| {
                let c = cursor_of(s).cluster_id;
                (c.source_run, c.start_byte_in_run)
            })
            .collect();
        assert_eq!(keys, vec![(2, 5), (2, 1), (1, 3), (0, 0)]);
        // Monotonically non-increasing — the invariant the multi-cursor edit loop
        // depends on for its byte offsets to stay valid.
        assert!(keys.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn sort_back_to_front_is_a_permutation_with_duplicates() {
        let sels = [
            Selection::Cursor(lead(1, 1)),
            Selection::Cursor(lead(1, 1)),
            Selection::Cursor(lead(0, 0)),
        ];
        let sorted = sort_selections_back_to_front(&sels);
        assert_eq!(sorted.len(), 3);
        let mut got: Vec<Selection> = sorted;
        let mut want: Vec<Selection> = sels.to_vec();
        got.sort();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn sort_back_to_front_keys_ranges_on_their_start() {
        // Range starts at run 5; the plain cursor is at run 0 -> range sorts first.
        let sels = [
            Selection::Cursor(lead(0, 0)),
            range_sel(lead(5, 0), lead(0, 0)),
        ];
        let sorted = sort_selections_back_to_front(&sels);
        assert!(matches!(sorted[0], Selection::Range(_)));
        assert!(matches!(sorted[1], Selection::Cursor(_)));
    }

    #[test]
    fn sort_back_to_front_handles_u32_max_keys() {
        let sels = [
            Selection::Cursor(lead(u32::MAX, u32::MAX)),
            Selection::Cursor(lead(0, 0)),
        ];
        let sorted = sort_selections_back_to_front(&sels);
        assert_eq!(cursor_of(&sorted[0]).cluster_id.source_run, u32::MAX);
    }

    // ------------------------------------------------------- adjust_cursors

    fn byte_at(sels: &[Selection], i: usize) -> u32 {
        cursor_of(&sels[i]).cluster_id.start_byte_in_run
    }

    #[test]
    fn adjust_cursors_empty_slice_is_a_noop() {
        let mut sels: Vec<Selection> = Vec::new();
        adjust_cursors(&mut sels, 0, 0, i32::MIN);
        assert!(sels.is_empty());
    }

    #[test]
    fn adjust_cursors_zero_change_leaves_everything_alone() {
        let mut sels = vec![
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(0, 10)),
        ];
        adjust_cursors(&mut sels, 0, 0, 0);
        assert_eq!(byte_at(&sels, 0), 0);
        assert_eq!(byte_at(&sels, 1), 10);
    }

    #[test]
    fn adjust_cursors_only_shifts_at_or_after_edit_byte_in_the_edit_run() {
        let mut sels = vec![
            Selection::Cursor(lead(0, 2)),  // before edit_byte -> untouched
            Selection::Cursor(lead(0, 5)),  // AT edit_byte     -> shifted
            Selection::Cursor(lead(0, 9)),  // after edit_byte  -> shifted
            Selection::Cursor(lead(1, 0)),  // other run        -> untouched
        ];
        adjust_cursors(&mut sels, 0, 5, 3);
        assert_eq!(byte_at(&sels, 0), 2);
        assert_eq!(byte_at(&sels, 1), 8);
        assert_eq!(byte_at(&sels, 2), 12);
        assert_eq!(byte_at(&sels, 3), 0);
    }

    #[test]
    fn adjust_cursors_negative_change_clamps_at_zero() {
        let mut sels = vec![Selection::Cursor(lead(0, 3))];
        adjust_cursors(&mut sels, 0, 0, -100);
        assert_eq!(byte_at(&sels, 0), 0, "documented clamp-to-zero");
    }

    #[test]
    fn adjust_cursors_i32_extremes_do_not_panic() {
        // i32::MAX applied to byte 0 saturates the offset, not the process.
        let mut sels = vec![Selection::Cursor(lead(0, 0))];
        adjust_cursors(&mut sels, 0, 0, i32::MAX);
        assert_eq!(byte_at(&sels, 0), i32::MAX as u32);

        // i32::MIN applied to byte 0 clamps to zero rather than wrapping.
        let mut sels = vec![Selection::Cursor(lead(0, 0))];
        adjust_cursors(&mut sels, 0, 0, i32::MIN);
        assert_eq!(byte_at(&sels, 0), 0);
    }

    #[test]
    fn adjust_cursors_u32_max_byte_collapses_to_zero() {
        // NOTE (reported): `start_byte_in_run as i32` WRAPS — u32::MAX becomes -1,
        // so a no-op (+0) adjustment silently relocates the cursor to byte 0.
        // Not reachable from real 2 GiB-run content, but it is the current behavior.
        let mut sels = vec![Selection::Cursor(lead(0, u32::MAX))];
        adjust_cursors(&mut sels, 0, 0, 0);
        assert_eq!(byte_at(&sels, 0), 0);
    }

    #[test]
    fn adjust_cursors_never_touches_range_selections() {
        let mut sels = vec![range_sel(lead(0, 4), lead(0, 8))];
        adjust_cursors(&mut sels, 0, 0, 100);
        match &sels[0] {
            Selection::Range(r) => {
                assert_eq!(r.start.cluster_id.start_byte_in_run, 4);
                assert_eq!(r.end.cluster_id.start_byte_in_run, 8);
            }
            Selection::Cursor(_) => panic!("range must stay a range"),
        }
    }

    // --------------------------------------------------- adjust_cursor_runs

    fn run_at(sels: &[Selection], i: usize) -> u32 {
        cursor_of(&sels[i]).cluster_id.source_run
    }

    #[test]
    fn adjust_cursor_runs_zero_change_returns_early() {
        let mut sels = vec![Selection::Cursor(lead(u32::MAX, 0))];
        adjust_cursor_runs(&mut sels, 0, 0);
        assert_eq!(run_at(&sels, 0), u32::MAX, "zero change must not touch runs");
    }

    #[test]
    fn adjust_cursor_runs_shifts_only_runs_strictly_after_the_boundary() {
        let mut sels = vec![
            Selection::Cursor(lead(0, 0)), // before boundary -> untouched
            Selection::Cursor(lead(1, 0)), // AT boundary     -> untouched
            Selection::Cursor(lead(2, 0)), // after           -> -1
            Selection::Cursor(lead(3, 0)), // after           -> -1
        ];
        adjust_cursor_runs(&mut sels, 1, -1);
        assert_eq!(run_at(&sels, 0), 0);
        assert_eq!(run_at(&sels, 1), 1);
        assert_eq!(run_at(&sels, 2), 1);
        assert_eq!(run_at(&sels, 3), 2);
    }

    #[test]
    fn adjust_cursor_runs_positive_change_shifts_up() {
        let mut sels = vec![Selection::Cursor(lead(2, 0))];
        adjust_cursor_runs(&mut sels, 0, 3);
        assert_eq!(run_at(&sels, 0), 5);
    }

    #[test]
    fn adjust_cursor_runs_negative_overshoot_clamps_to_the_boundary_run() {
        let mut sels = vec![Selection::Cursor(lead(3, 0))];
        adjust_cursor_runs(&mut sels, 1, -100);
        assert_eq!(run_at(&sels, 0), 1, "never drops below the surviving run");
    }

    #[test]
    fn adjust_cursor_runs_i32_min_clamps_instead_of_wrapping() {
        // 3 + i32::MIN stays inside i32 (no overflow) and the clamp catches it.
        let mut sels = vec![Selection::Cursor(lead(3, 0))];
        adjust_cursor_runs(&mut sels, 2, i32::MIN);
        assert_eq!(run_at(&sels, 0), 2);

        let mut sels = vec![Selection::Cursor(lead(1, 0))];
        adjust_cursor_runs(&mut sels, 0, i32::MIN);
        assert_eq!(run_at(&sels, 0), 0);
    }

    #[test]
    fn adjust_cursor_runs_i32_max_is_inert_when_no_cursor_qualifies() {
        // Every cursor is at/below the boundary, so the huge delta is never applied.
        let mut sels = vec![
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(5, 0)),
        ];
        adjust_cursor_runs(&mut sels, 5, i32::MAX);
        assert_eq!(run_at(&sels, 0), 0);
        assert_eq!(run_at(&sels, 1), 5);
    }

    #[test]
    fn adjust_cursor_runs_never_touches_range_selections() {
        let mut sels = vec![range_sel(lead(9, 0), lead(9, 1))];
        adjust_cursor_runs(&mut sels, 0, -5);
        match &sels[0] {
            Selection::Range(r) => assert_eq!(r.start.cluster_id.source_run, 9),
            Selection::Cursor(_) => panic!("range must stay a range"),
        }
    }

    // ---------------------------------------------------------- run_text_len

    #[test]
    fn run_text_len_counts_bytes_not_chars() {
        let content = vec![text("héllo")]; // é is 2 bytes -> 6 bytes, 5 chars
        assert_eq!(run_text_len(&content, 0), 6);
        assert_eq!(content_text_chars(&content), 5);
    }

    fn content_text_chars(content: &[InlineContent]) -> usize {
        content
            .iter()
            .map(|c| match c {
                InlineContent::Text(r) => r.text.chars().count(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn run_text_len_zero_for_empty_missing_and_non_text_runs() {
        let content = vec![text(""), obj()];
        assert_eq!(run_text_len(&content, 0), 0, "empty text run");
        assert_eq!(run_text_len(&content, 1), 0, "non-text run");
        assert_eq!(run_text_len(&content, 2), 0, "one past the end");
        assert_eq!(run_text_len(&content, u32::MAX), 0, "u32::MAX index");
        assert_eq!(run_text_len(&[], 0), 0, "empty content");
    }

    // ------------------------------------------------------------- edit_text

    #[test]
    fn edit_text_empty_selections_returns_content_unchanged() {
        let content = vec![text("hello")];
        let (new_content, sels) = edit_text(&content, &[], &TextEdit::Insert("x".into()));
        assert_eq!(dump(&new_content), vec!["hello"]);
        assert!(sels.is_empty());
    }

    #[test]
    fn edit_text_on_empty_content_does_not_panic() {
        let (new_content, sels) = edit_text(
            &[],
            &[Selection::Cursor(lead(0, 0))],
            &TextEdit::Insert("x".into()),
        );
        assert!(new_content.is_empty());
        assert_eq!(sels.len(), 1, "the cursor survives, unmoved");
        assert_eq!(cursor_of(&sels[0]), lead(0, 0));
    }

    #[test]
    fn edit_text_out_of_range_cursor_is_a_noop_not_a_panic() {
        let content = vec![text("hi")];
        let (new_content, sels) = edit_text(
            &content,
            &[Selection::Cursor(lead(u32::MAX, 0))],
            &TextEdit::DeleteBackward,
        );
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(sels.len(), 1);
    }

    #[test]
    fn edit_text_multi_cursor_insert_keeps_both_cursors_correct() {
        // Two cursors in the same run: the earlier edit must shift the later cursor
        // by the ACTUAL byte delta (this is what adjust_cursors exists for).
        let content = vec![text("hello")];
        let sels = [
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(0, 3)),
        ];
        let (new_content, new_sels) = edit_text(&content, &sels, &TextEdit::Insert("X".into()));
        assert_eq!(dump(&new_content), vec!["XhelXlo"]);
        assert_eq!(cursor_of(&new_sels[0]), lead(0, 1));
        assert_eq!(cursor_of(&new_sels[1]), lead(0, 5));
    }

    #[test]
    fn edit_text_multi_cursor_insert_shifts_by_multibyte_length_not_one() {
        // Inserting a 4-byte emoji must move the trailing cursor by 4 bytes.
        let content = vec![text("ab")];
        let sels = [
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(0, 2)),
        ];
        let (new_content, new_sels) = edit_text(&content, &sels, &TextEdit::Insert("👍".into()));
        assert_eq!(dump(&new_content), vec!["👍ab👍"]);
        assert_eq!(cursor_of(&new_sels[0]), lead(0, 4));
        assert_eq!(cursor_of(&new_sels[1]), lead(0, 10)); // 4 + "ab" + 4
    }

    #[test]
    fn edit_text_backspace_with_a_range_deletes_the_range_only() {
        // Regression guard for the documented rule: Backspace on a selection removes
        // the selection, NOT the selection plus one more grapheme.
        let content = vec![text("hello")];
        let sel = [range_sel(lead(0, 1), lead(0, 3))];
        let (new_content, _) = edit_text(&content, &sel, &TextEdit::DeleteBackward);
        assert_eq!(dump(&new_content), vec!["hlo"]);
    }

    // ------------------------------------------------ apply_edit_to_selection

    #[test]
    fn apply_edit_range_insert_replaces_the_range() {
        let content = vec![text("hello")];
        let sel = range_sel(lead(0, 1), lead(0, 4));
        let (new_content, cursor) =
            apply_edit_to_selection(&content, &sel, &TextEdit::Insert("EY".into()));
        assert_eq!(dump(&new_content), vec!["hEYo"]);
        assert_eq!(cursor, lead(0, 3));
    }

    #[test]
    fn apply_edit_range_delete_forward_deletes_range_only() {
        let content = vec![text("hello")];
        let sel = range_sel(lead(0, 1), lead(0, 3));
        let (new_content, cursor) =
            apply_edit_to_selection(&content, &sel, &TextEdit::DeleteForward);
        assert_eq!(dump(&new_content), vec!["hlo"]);
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn apply_edit_cursor_insert_of_empty_string_only_moves_the_caret() {
        let content = vec![text("hello")];
        let sel = Selection::Cursor(lead(0, 2));
        let (new_content, cursor) =
            apply_edit_to_selection(&content, &sel, &TextEdit::Insert(String::new()));
        assert_eq!(dump(&new_content), vec!["hello"]);
        assert_eq!(cursor, lead(0, 2));
    }

    // ------------------------------------------------ cursor_byte_offset_in_run

    #[test]
    fn cursor_byte_offset_leading_clamps_past_the_end() {
        assert_eq!(cursor_byte_offset_in_run("hi", &lead(0, 999)), 2);
        assert_eq!(cursor_byte_offset_in_run("hi", &lead(0, u32::MAX)), 2);
        assert_eq!(cursor_byte_offset_in_run("", &lead(0, 5)), 0);
    }

    #[test]
    fn cursor_byte_offset_trailing_clamps_past_the_end() {
        assert_eq!(cursor_byte_offset_in_run("hi", &trail(0, 2)), 2);
        assert_eq!(cursor_byte_offset_in_run("hi", &trail(0, u32::MAX)), 2);
        assert_eq!(cursor_byte_offset_in_run("", &trail(0, 0)), 0);
    }

    #[test]
    fn cursor_byte_offset_trailing_lands_after_the_whole_grapheme() {
        // Combining sequence: "e" + U+0301 is one cluster of 3 bytes.
        assert_eq!(cursor_byte_offset_in_run("e\u{0301}x", &trail(0, 0)), 3);
        // A 4-byte astral char.
        assert_eq!(cursor_byte_offset_in_run("👍x", &trail(0, 0)), 4);
        // A ZWJ emoji family is ONE cluster — a char-wise implementation would
        // return 4 here instead of 18.
        assert_eq!(cursor_byte_offset_in_run(FAMILY, &trail(0, 0)), 18);
    }

    #[test]
    fn cursor_byte_offset_leading_is_the_raw_offset() {
        assert_eq!(cursor_byte_offset_in_run(FAMILY, &lead(0, 0)), 0);
        assert_eq!(cursor_byte_offset_in_run("abc", &lead(0, 1)), 1);
    }

    // ---------------------------------------------------------- delete_range

    #[test]
    fn delete_range_within_one_run() {
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(0, 3),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["hlo"]);
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn delete_range_backward_range_is_normalized() {
        // Right-to-left selection (Shift+Left / Shift+Home): start is AFTER end.
        // It must delete the same bytes as the forward range, not silently no-op.
        let content = vec![text("hello")];
        let backward = SelectionRange {
            start: lead(0, 3),
            end: lead(0, 1),
        };
        let (new_content, cursor) = delete_range(&content, &backward);
        assert_eq!(dump(&new_content), vec!["hlo"]);
        assert_eq!(cursor, lead(0, 1), "caret collapses to the LOW end");
    }

    #[test]
    fn delete_range_collapsed_range_deletes_nothing() {
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 2),
            end: lead(0, 2),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["hello"]);
        assert_eq!(cursor, lead(0, 2));
    }

    #[test]
    fn delete_range_select_all_with_trailing_end_covers_the_last_cluster() {
        // The end cursor of a select-all sits Trailing on the last cluster; the
        // affinity-aware offset is what makes the final grapheme part of the range.
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 0),
            end: trail(0, 4),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec![""]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_range_spanning_runs_merges_matching_styles() {
        let content = vec![text("abc"), text("def")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(1, 2),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["af"], "same style -> one run");
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn delete_range_spanning_runs_keeps_differing_styles_apart() {
        let content = vec![text_styled("abc", style_a()), text_styled("def", style_b())];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(1, 2),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["a", "f"], "styles differ -> no merge");
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn delete_range_drops_the_runs_strictly_between_the_boundaries() {
        let content = vec![text("abc"), text("XYZ"), obj(), text("def")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(3, 2),
        };
        let (new_content, _) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["af"], "middle text AND obj dropped");
    }

    #[test]
    fn delete_range_over_a_single_non_text_item_removes_it() {
        let content = vec![text("ab"), obj(), text("cd")];
        // start != end (affinity differs) so the guard against a zero-width delete passes.
        let r = SelectionRange {
            start: lead(1, 0),
            end: trail(1, 0),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["ab", "cd"]);
        assert_eq!(cursor, lead(1, 0));
    }

    #[test]
    fn delete_range_collapsed_on_a_non_text_item_keeps_it() {
        let content = vec![text("ab"), obj()];
        let r = SelectionRange {
            start: lead(1, 0),
            end: lead(1, 0),
        };
        let (new_content, _) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["ab", "<obj>"]);
    }

    #[test]
    fn delete_range_out_of_bounds_runs_do_not_panic() {
        let content = vec![text("ab")];

        // Both ends past the end (same-run path).
        let r = SelectionRange {
            start: lead(99, 0),
            end: lead(99, 5),
        };
        let (new_content, _) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec!["ab"]);

        // Multi-run path with a bogus `hi_run` — the drain must be clamped.
        let r = SelectionRange {
            start: lead(0, 0),
            end: lead(u32::MAX, 0),
        };
        let (new_content, cursor) = delete_range(&content, &r);
        assert_eq!(dump(&new_content), vec![""]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_range_backward_across_runs_is_normalized() {
        let content = vec![text("abc"), text("def")];
        let backward = SelectionRange {
            start: lead(1, 2),
            end: lead(0, 1),
        };
        let (new_content, cursor) = delete_range(&content, &backward);
        assert_eq!(dump(&new_content), vec!["af"]);
        assert_eq!(cursor, lead(0, 1));
    }

    // ---------------------------------------------------------- insert_text

    #[test]
    fn insert_text_leading_inserts_before_the_cluster() {
        let content = vec![text("hello")];
        let (new_content, cursor) = insert_text(&content, &lead(0, 2), "XY");
        assert_eq!(dump(&new_content), vec!["heXYllo"]);
        assert_eq!(cursor, lead(0, 4));
    }

    #[test]
    fn insert_text_trailing_inserts_after_the_whole_grapheme() {
        // Trailing on the 4-byte emoji must land at byte 4, not byte 1.
        let content = vec![text("👍z")];
        let (new_content, cursor) = insert_text(&content, &trail(0, 0), "X");
        assert_eq!(dump(&new_content), vec!["👍Xz"]);
        assert_eq!(cursor, lead(0, 5));
    }

    #[test]
    fn insert_text_trailing_past_the_end_appends() {
        let content = vec![text("hi")];
        let (new_content, cursor) = insert_text(&content, &trail(0, 999), "!");
        assert_eq!(dump(&new_content), vec!["hi!"]);
        assert_eq!(cursor, lead(0, 3));
    }

    #[test]
    fn insert_text_leading_past_the_end_is_a_noop() {
        // Asymmetry with the Trailing case above: a Leading offset beyond the run
        // is NOT clamped, the insert is dropped and the caret is returned as-is.
        let content = vec![text("hi")];
        let (new_content, cursor) = insert_text(&content, &lead(0, 999), "!");
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(0, 999));
    }

    #[test]
    fn insert_text_into_missing_or_non_text_run_is_a_noop() {
        let content = vec![obj()];
        let (new_content, cursor) = insert_text(&content, &lead(0, 0), "x");
        assert_eq!(dump(&new_content), vec!["<obj>"]);
        assert_eq!(cursor, lead(0, 0));

        let (new_content, cursor) = insert_text(&content, &lead(u32::MAX, 0), "x");
        assert_eq!(dump(&new_content), vec!["<obj>"]);
        assert_eq!(cursor, lead(u32::MAX, 0));

        let (new_content, _) = insert_text(&[], &lead(0, 0), "x");
        assert!(new_content.is_empty());
    }

    #[test]
    fn insert_text_empty_string_leaves_the_text_alone() {
        let content = vec![text("hi")];
        let (new_content, cursor) = insert_text(&content, &lead(0, 1), "");
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn insert_text_cursor_advances_by_bytes_not_chars() {
        let content = vec![text("")];
        let (new_content, cursor) = insert_text(&content, &lead(0, 0), FAMILY);
        assert_eq!(dump(&new_content), vec![FAMILY]);
        assert_eq!(cursor, lead(0, 18));
    }

    #[test]
    fn insert_text_of_a_huge_string_does_not_panic() {
        let big = "a".repeat(200_000);
        let content = vec![text("hi")];
        let (new_content, cursor) = insert_text(&content, &lead(0, 1), &big);
        assert_eq!(run_text_len(&new_content, 0), 200_002);
        assert_eq!(cursor, lead(0, 200_001));
    }

    // ------------------------------------------------------- delete_backward

    #[test]
    fn delete_backward_on_empty_content_is_a_noop() {
        let (new_content, cursor) = delete_backward(&[], &lead(0, 0));
        assert!(new_content.is_empty());
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_backward_at_the_start_of_the_document_is_a_noop() {
        let content = vec![text("hi")];
        let (new_content, cursor) = delete_backward(&content, &lead(0, 0));
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_backward_removes_a_whole_grapheme_cluster() {
        let content = vec![text(&format!("a{FAMILY}"))];
        let (new_content, cursor) = delete_backward(&content, &lead(0, 19));
        assert_eq!(dump(&new_content), vec!["a"], "all 18 bytes go at once");
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn delete_backward_trailing_affinity_removes_the_current_cluster() {
        let content = vec![text("ab")];
        let (new_content, cursor) = delete_backward(&content, &trail(0, 0));
        assert_eq!(dump(&new_content), vec!["b"]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_backward_merges_across_a_run_boundary() {
        let content = vec![text("ab"), text("cd")];
        let (new_content, cursor) = delete_backward(&content, &lead(1, 0));
        assert_eq!(dump(&new_content), vec!["abcd"]);
        assert_eq!(cursor, lead(0, 2), "caret sits at the join point");
    }

    #[test]
    fn delete_backward_removes_a_non_text_item_sitting_before_the_caret() {
        let content = vec![text("ab"), obj(), text("cd")];
        let (new_content, cursor) = delete_backward(&content, &lead(2, 0));
        assert_eq!(dump(&new_content), vec!["ab", "cd"]);
        assert_eq!(cursor, lead(1, 0));
    }

    #[test]
    fn delete_backward_with_the_caret_after_a_non_text_item_removes_the_item() {
        let content = vec![text("ab"), obj()];
        let (new_content, _) = delete_backward(&content, &trail(1, 0));
        assert_eq!(dump(&new_content), vec!["ab"]);
    }

    #[test]
    fn delete_backward_with_the_caret_before_a_non_text_item_acts_on_the_previous_run() {
        let content = vec![text("ab"), obj()];
        let (new_content, cursor) = delete_backward(&content, &lead(1, 0));
        assert_eq!(dump(&new_content), vec!["a", "<obj>"], "the item survives");
        assert_eq!(cursor, lead(0, 1));
    }

    #[test]
    fn delete_backward_before_a_leading_non_text_item_at_run_zero_is_a_noop() {
        let content = vec![obj()];
        let (new_content, cursor) = delete_backward(&content, &lead(0, 0));
        assert_eq!(dump(&new_content), vec!["<obj>"]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_backward_out_of_range_run_is_a_noop() {
        let content = vec![text("hi")];
        let (new_content, cursor) = delete_backward(&content, &lead(u32::MAX, u32::MAX));
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(u32::MAX, u32::MAX));
    }

    // -------------------------------------------------------- delete_forward

    #[test]
    fn delete_forward_on_empty_content_is_a_noop() {
        let (new_content, cursor) = delete_forward(&[], &lead(0, 0));
        assert!(new_content.is_empty());
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_forward_at_the_end_of_the_document_is_a_noop() {
        let content = vec![text("hi")];
        let (new_content, cursor) = delete_forward(&content, &lead(0, 2));
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(0, 2));
    }

    #[test]
    fn delete_forward_removes_a_whole_grapheme_cluster() {
        let content = vec![text(&format!("{FAMILY}z"))];
        let (new_content, cursor) = delete_forward(&content, &lead(0, 0));
        assert_eq!(dump(&new_content), vec!["z"]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_forward_merges_across_a_run_boundary() {
        let content = vec![text("ab"), text("cd")];
        let (new_content, cursor) = delete_forward(&content, &lead(0, 2));
        assert_eq!(dump(&new_content), vec!["abcd"]);
        assert_eq!(cursor, lead(0, 2));
    }

    #[test]
    fn delete_forward_removes_a_non_text_item_sitting_after_the_caret() {
        let content = vec![text("ab"), obj()];
        let (new_content, _) = delete_forward(&content, &lead(0, 2));
        assert_eq!(dump(&new_content), vec!["ab"]);
    }

    #[test]
    fn delete_forward_with_the_caret_before_a_non_text_item_removes_the_item() {
        let content = vec![obj(), text("ab")];
        let (new_content, cursor) = delete_forward(&content, &lead(0, 0));
        assert_eq!(dump(&new_content), vec!["ab"]);
        assert_eq!(cursor, lead(0, 0));
    }

    #[test]
    fn delete_forward_with_the_caret_after_a_non_text_item_acts_on_the_next_run() {
        let content = vec![obj(), text("ab")];
        let (new_content, cursor) = delete_forward(&content, &trail(0, 0));
        assert_eq!(dump(&new_content), vec!["<obj>", "b"], "the item survives");
        assert_eq!(cursor, lead(1, 0));
    }

    #[test]
    fn delete_forward_after_a_trailing_non_text_item_at_the_last_run_is_a_noop() {
        let content = vec![text("ab"), obj()];
        let (new_content, cursor) = delete_forward(&content, &trail(1, 0));
        assert_eq!(dump(&new_content), vec!["ab", "<obj>"]);
        assert_eq!(cursor, trail(1, 0));
    }

    #[test]
    fn delete_forward_out_of_range_run_is_a_noop() {
        let content = vec![text("hi")];
        let (new_content, cursor) = delete_forward(&content, &lead(u32::MAX, u32::MAX));
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert_eq!(cursor, lead(u32::MAX, u32::MAX));
    }

    // ------------------------------------------------------- edit_text_multi

    #[test]
    #[should_panic(expected = "same length")]
    fn edit_text_multi_panics_on_a_length_mismatch() {
        // Documented in the function's `# Panics` section.
        let content = vec![text("hi")];
        let sels = [Selection::Cursor(lead(0, 0))];
        let _ = edit_text_multi(&content, &sels, &["a", "b"]);
    }

    #[test]
    fn edit_text_multi_with_no_selections_returns_content_unchanged() {
        let content = vec![text("hi")];
        let (new_content, sels) = edit_text_multi(&content, &[], &[]);
        assert_eq!(dump(&new_content), vec!["hi"]);
        assert!(sels.is_empty());
    }

    #[test]
    fn edit_text_multi_gives_each_cursor_its_own_text() {
        let content = vec![text("ab")];
        let sels = [
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(0, 2)),
        ];
        let (new_content, new_sels) = edit_text_multi(&content, &sels, &["X", "Y"]);
        assert_eq!(dump(&new_content), vec!["XabY"]);
        assert_eq!(cursor_of(&new_sels[0]), lead(0, 1));
        assert_eq!(cursor_of(&new_sels[1]), lead(0, 4));
    }

    #[test]
    fn edit_text_multi_with_empty_texts_only_moves_the_carets() {
        let content = vec![text("ab")];
        let sels = [
            Selection::Cursor(lead(0, 0)),
            Selection::Cursor(lead(0, 1)),
        ];
        let (new_content, new_sels) = edit_text_multi(&content, &sels, &["", ""]);
        assert_eq!(dump(&new_content), vec!["ab"]);
        assert_eq!(new_sels.len(), 2);
    }

    // ---------------------------------------------------------- inspect_delete

    #[test]
    fn inspect_delete_forward_at_the_end_of_the_document_is_none() {
        let content = vec![text("hi")];
        assert!(inspect_delete(&content, &Selection::Cursor(lead(0, 2)), true).is_none());
        assert!(inspect_delete(&[], &Selection::Cursor(lead(0, 0)), true).is_none());
    }

    #[test]
    fn inspect_delete_backward_at_the_start_of_the_document_is_none() {
        let content = vec![text("hi")];
        assert!(inspect_delete(&content, &Selection::Cursor(lead(0, 0)), false).is_none());
        assert!(inspect_delete(&[], &Selection::Cursor(lead(0, 0)), false).is_none());
    }

    #[test]
    fn inspect_delete_forward_reports_exactly_what_delete_forward_removes() {
        let content = vec![text("héllo")]; // é starts at byte 1, 2 bytes long
        let cursor = lead(0, 1);
        let (_, reported) = inspect_delete(&content, &Selection::Cursor(cursor), true).unwrap();
        let (after, _) = delete_forward(&content, &cursor);
        assert_eq!(reported, "é");
        assert_eq!(dump(&after), vec!["hllo"], "inspect and delete agree");
    }

    #[test]
    fn inspect_delete_backward_reports_exactly_what_delete_backward_removes() {
        let content = vec![text(&format!("a{FAMILY}"))];
        let cursor = lead(0, 19);
        let (range, reported) =
            inspect_delete(&content, &Selection::Cursor(cursor), false).unwrap();
        let (after, _) = delete_backward(&content, &cursor);
        assert_eq!(reported, FAMILY, "the whole ZWJ cluster, not one codepoint");
        assert_eq!(range.start, lead(0, 1));
        assert_eq!(dump(&after), vec!["a"]);
    }

    #[test]
    fn inspect_delete_forward_honors_trailing_affinity() {
        // A Trailing cursor sits AFTER its grapheme, so Delete removes the NEXT one.
        let content = vec![text("abc")];
        let (_, reported) =
            inspect_delete(&content, &Selection::Cursor(trail(0, 0)), true).unwrap();
        assert_eq!(reported, "b");
    }

    #[test]
    fn inspect_delete_across_a_run_boundary_reports_the_neighbouring_grapheme() {
        let content = vec![text("ab"), text("cd")];
        let (_, fwd) = inspect_delete(&content, &Selection::Cursor(lead(0, 2)), true).unwrap();
        assert_eq!(fwd, "c");
        let (_, back) = inspect_delete(&content, &Selection::Cursor(lead(1, 0)), false).unwrap();
        assert_eq!(back, "b");
    }

    #[test]
    fn inspect_delete_reports_none_for_a_non_text_neighbour_that_delete_would_remove() {
        // BUG (reported): inspect_delete_forward/backward only match on a TEXT
        // neighbour, so they answer "nothing would be deleted" while
        // delete_forward/delete_backward actually remove the inline item. A callback
        // relying on inspect_delete to veto or log the edit sees nothing coming.
        let content = vec![text("ab"), obj()];
        assert!(inspect_delete(&content, &Selection::Cursor(lead(0, 2)), true).is_none());
        let (after, _) = delete_forward(&content, &lead(0, 2));
        assert_eq!(dump(&after), vec!["ab"], "...but the item IS removed");

        let content = vec![obj(), text("ab")];
        assert!(inspect_delete(&content, &Selection::Cursor(lead(1, 0)), false).is_none());
        let (after, _) = delete_backward(&content, &lead(1, 0));
        assert_eq!(dump(&after), vec!["ab"], "...but the item IS removed");
    }

    #[test]
    fn inspect_delete_on_a_range_returns_the_range_and_its_text() {
        let content = vec![text("hello")];
        let sel = range_sel(lead(0, 1), lead(0, 3));
        let (range, reported) = inspect_delete(&content, &sel, false).unwrap();
        assert_eq!(range.start, lead(0, 1));
        assert_eq!(range.end, lead(0, 3));
        assert_eq!(reported, "el");
        // ...and that is exactly what the delete removes.
        let (after, _) = apply_edit_to_selection(&content, &sel, &TextEdit::DeleteBackward);
        assert_eq!(dump(&after), vec!["hlo"]);
    }

    #[test]
    fn inspect_delete_out_of_range_cursor_is_none_not_a_panic() {
        let content = vec![text("hi")];
        assert!(inspect_delete(&content, &Selection::Cursor(lead(u32::MAX, 0)), true).is_none());
        assert!(inspect_delete(&content, &Selection::Cursor(lead(u32::MAX, 0)), false).is_none());
    }

    // ---------------------------------------------------- extract_text_in_range

    #[test]
    fn extract_text_in_range_single_run() {
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(0, 4),
        };
        assert_eq!(extract_text_in_range(&content, &r), "ell");
    }

    #[test]
    fn extract_text_in_range_multi_run_concatenates_the_span() {
        let content = vec![text("abc"), text("MID"), text("def")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(2, 2),
        };
        assert_eq!(extract_text_in_range(&content, &r), "bcMIDde");
    }

    #[test]
    fn extract_text_in_range_skips_non_text_items_in_the_span() {
        let content = vec![text("abc"), obj(), text("def")];
        let r = SelectionRange {
            start: lead(0, 1),
            end: lead(2, 2),
        };
        assert_eq!(extract_text_in_range(&content, &r), "bcde");
    }

    #[test]
    fn extract_text_in_range_out_of_bounds_yields_empty_string() {
        let content = vec![text("hi")];

        // end_byte past the run length.
        let r = SelectionRange {
            start: lead(0, 0),
            end: lead(0, 99),
        };
        assert_eq!(extract_text_in_range(&content, &r), "");

        // Both runs past the end.
        let r = SelectionRange {
            start: lead(9, 0),
            end: lead(9, 1),
        };
        assert_eq!(extract_text_in_range(&content, &r), "");

        // Empty content.
        let r = SelectionRange {
            start: lead(0, 0),
            end: lead(0, 1),
        };
        assert_eq!(extract_text_in_range(&[], &r), "");
    }

    #[test]
    fn extract_text_in_range_backward_single_run_yields_empty_string() {
        // Unlike delete_range, extract does NOT normalize direction — a
        // right-to-left selection inside one run reports no text at all.
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 3),
            end: lead(0, 1),
        };
        assert_eq!(extract_text_in_range(&content, &r), "");
    }

    #[test]
    fn extract_text_in_range_ignores_affinity_and_drops_the_last_cluster() {
        // BUG (reported): extract_text_in_range reads the RAW `start_byte_in_run`
        // while delete_range goes through cursor_byte_offset_in_run. On a select-all
        // (end cursor Trailing on the last cluster) inspect_delete therefore reports
        // one grapheme LESS than the delete actually removes.
        let content = vec![text("hello")];
        let r = SelectionRange {
            start: lead(0, 0),
            end: trail(0, 4),
        };
        assert_eq!(extract_text_in_range(&content, &r), "hell", "the 'o' is missing");

        let (after, _) = delete_range(&content, &r);
        assert_eq!(dump(&after), vec![""], "...yet delete_range removes all of it");
    }
}
