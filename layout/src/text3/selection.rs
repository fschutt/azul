//! Text selection helper functions
//!
//! Provides word and paragraph selection algorithms.

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};

use crate::text3::cache::{
    ParsedFontTrait, PositionedItem, ShapedCluster, ShapedItem, UnifiedLayout,
};

/// Select the word at the given cursor position
///
/// Uses Unicode word boundaries to determine word start/end.
/// Returns a SelectionRange covering the entire word.
pub fn select_word_at_cursor<T: ParsedFontTrait>(
    cursor: &TextCursor,
    layout: &UnifiedLayout<T>,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, cluster) = find_cluster_at_cursor(cursor, layout)?;

    // Get the text from this cluster and surrounding clusters on the same line
    let line_text = extract_line_text_at_item(item_idx, layout);
    let cursor_byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    // Find word boundaries
    let (word_start, word_end) = find_word_boundaries(&line_text, cursor_byte_offset);

    // Convert byte offsets to cursors
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_start as u32,
        },
        affinity: CursorAffinity::Leading,
    };

    let end_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_end as u32,
        },
        affinity: CursorAffinity::Trailing,
    };

    Some(SelectionRange {
        start: start_cursor,
        end: end_cursor,
    })
}

/// Select the paragraph/line at the given cursor position
///
/// Returns a SelectionRange covering the entire line from the first
/// to the last cluster on that line.
pub fn select_paragraph_at_cursor<T: ParsedFontTrait>(
    cursor: &TextCursor,
    layout: &UnifiedLayout<T>,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, _) = find_cluster_at_cursor(cursor, layout)?;
    let item = &layout.items[item_idx];
    let line_index = item.line_index;

    // Find all items on this line
    let line_items: Vec<(usize, &PositionedItem<T>)> = layout
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.line_index == line_index)
        .collect();

    if line_items.is_empty() {
        return None;
    }

    // Get first and last cluster on line
    let first_cluster = line_items
        .iter()
        .find_map(|(_, item)| item.item.as_cluster())?;

    let last_cluster = line_items
        .iter()
        .rev()
        .find_map(|(_, item)| item.item.as_cluster())?;

    // Create selection spanning entire line
    Some(SelectionRange {
        start: TextCursor {
            cluster_id: first_cluster.source_cluster_id,
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: last_cluster.source_cluster_id,
            affinity: CursorAffinity::Trailing,
        },
    })
}

// === Helper Functions ===

/// Find the cluster containing the given cursor
fn find_cluster_at_cursor<'a, T: ParsedFontTrait>(
    cursor: &TextCursor,
    layout: &'a UnifiedLayout<T>,
) -> Option<(usize, &'a ShapedCluster<T>)> {
    layout.items.iter().enumerate().find_map(|(idx, item)| {
        if let ShapedItem::Cluster(cluster) = &item.item {
            if cluster.source_cluster_id == cursor.cluster_id {
                return Some((idx, cluster));
            }
        }
        None
    })
}

/// Extract text from all clusters on the same line as the given item
fn extract_line_text_at_item<T: ParsedFontTrait>(
    item_idx: usize,
    layout: &UnifiedLayout<T>,
) -> String {
    let line_index = layout.items[item_idx].line_index;

    let mut text = String::new();
    for item in &layout.items {
        if item.line_index != line_index {
            continue;
        }

        if let ShapedItem::Cluster(cluster) = &item.item {
            text.push_str(&cluster.text);
        }
    }

    text
}

/// Find word boundaries around the given byte offset
///
/// Uses a simple algorithm: word characters are alphanumeric or underscore,
/// everything else is a boundary.
fn find_word_boundaries(text: &str, cursor_offset: usize) -> (usize, usize) {
    // Clamp cursor offset to text length
    let cursor_offset = cursor_offset.min(text.len());

    // Find word start (scan backwards)
    let mut word_start = 0;
    let mut char_indices: Vec<(usize, char)> = text.char_indices().collect();

    for (i, (byte_idx, ch)) in char_indices.iter().enumerate().rev() {
        if *byte_idx >= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word starts after this char
            word_start = if i + 1 < char_indices.len() {
                char_indices[i + 1].0
            } else {
                text.len()
            };
            break;
        }
    }

    // Find word end (scan forwards)
    let mut word_end = text.len();
    for (byte_idx, ch) in char_indices.iter() {
        if *byte_idx <= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word ends before this char
            word_end = *byte_idx;
            break;
        }
    }

    // If cursor is on whitespace, select just that whitespace
    if let Some((_, ch)) = char_indices.iter().find(|(idx, _)| *idx == cursor_offset) {
        if !is_word_char(*ch) {
            // Find span of consecutive whitespace/punctuation
            let start = char_indices
                .iter()
                .rev()
                .find(|(idx, c)| *idx < cursor_offset && is_word_char(*c))
                .map(|(idx, c)| idx + c.len_utf8())
                .unwrap_or(0);

            let end = char_indices
                .iter()
                .find(|(idx, c)| *idx > cursor_offset && is_word_char(*c))
                .map(|(idx, _)| *idx)
                .unwrap_or(text.len());

            return (start, end);
        }
    }

    (word_start, word_end)
}

/// Check if a character is part of a word
#[inline]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_boundaries_simple() {
        let text = "Hello World";

        // Cursor in "Hello"
        let (start, end) = find_word_boundaries(text, 2);
        assert_eq!(&text[start..end], "Hello");

        // Cursor in "World"
        let (start, end) = find_word_boundaries(text, 7);
        assert_eq!(&text[start..end], "World");

        // Cursor on space
        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], " ");
    }

    #[test]
    fn test_word_boundaries_start_end() {
        let text = "Hello";

        // At start
        let (start, end) = find_word_boundaries(text, 0);
        assert_eq!(&text[start..end], "Hello");

        // At end
        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], "Hello");
    }

    #[test]
    fn test_word_boundaries_punctuation() {
        let text = "Hello, World!";

        // In "Hello"
        let (start, end) = find_word_boundaries(text, 2);
        assert_eq!(&text[start..end], "Hello");

        // On comma
        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], ", ");

        // In "World"
        let (start, end) = find_word_boundaries(text, 8);
        assert_eq!(&text[start..end], "World");
    }

    #[test]
    fn test_word_boundaries_underscore() {
        let text = "hello_world";

        // Should treat underscore as word char
        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], "hello_world");
    }

    #[test]
    fn test_is_word_char() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('0'));
        assert!(is_word_char('_'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char(','));
        assert!(!is_word_char('!'));
    }
}
