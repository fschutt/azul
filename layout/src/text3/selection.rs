//! Text selection helper functions
//!
//! Provides word and paragraph selection algorithms.

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};

use crate::text3::cache::{
    is_word_char, PositionedItem, ShapedCluster, ShapedItem, UnifiedLayout,
};

/// Select the word at the given cursor position
///
/// Uses a simple word character heuristic (alphanumeric and underscore)
/// to determine word start/end. Returns a `SelectionRange` covering the entire word.
#[must_use] pub fn select_word_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, _cluster) = find_cluster_at_cursor(cursor, layout)?;

    // Get text and cluster mapping for this line
    let (line_text, cluster_map) = extract_line_text_and_clusters(item_idx, layout);

    // Compute byte offset within concatenated line text
    let cursor_byte_offset = cluster_map
        .iter()
        .take_while(|(id, _)| *id != cursor.cluster_id)
        .map(|(_, len)| len)
        .sum::<usize>();

    // Find word boundaries in the concatenated text
    let (word_start, word_end) = find_word_boundaries(&line_text, cursor_byte_offset);

    // Map byte offsets back to cluster IDs
    let start_cluster_id = byte_offset_to_cluster_id(&cluster_map, word_start)?;
    let end_cluster_id = byte_offset_to_cluster_id(&cluster_map, word_end.saturating_sub(1))
        .unwrap_or(start_cluster_id);

    Some(SelectionRange {
        start: TextCursor {
            cluster_id: start_cluster_id,
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: end_cluster_id,
            affinity: CursorAffinity::Trailing,
        },
    })
}

/// Select the paragraph/line at the given cursor position
///
/// Returns a `SelectionRange` covering the entire line from the first
/// to the last cluster on that line.
#[must_use] pub fn select_paragraph_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, _) = find_cluster_at_cursor(cursor, layout)?;
    let item = &layout.items[item_idx];
    let line_index = item.line_index;

    // Find all items on this line
    let line_items: Vec<(usize, &PositionedItem)> = layout
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

// Helper Functions

/// Find the cluster containing the given cursor
fn find_cluster_at_cursor<'a>(
    cursor: &TextCursor,
    layout: &'a UnifiedLayout,
) -> Option<(usize, &'a ShapedCluster)> {
    layout.items.iter().enumerate().find_map(|(idx, item)| {
        if let ShapedItem::Cluster(cluster) = &item.item {
            if cluster.source_cluster_id == cursor.cluster_id {
                return Some((idx, cluster));
            }
        }
        None
    })
}

/// Extract text and cluster ID mapping for the cursor's logical run.
///
/// Returns concatenated text and a vec of (`cluster_id`, `byte_length`) pairs
/// so byte offsets can be mapped back to cluster IDs.
///
/// Clusters are gathered by their logical run (`source_run`) and concatenated in
/// LOGICAL byte order — NOT visual (`layout.items`) order, and NOT restricted to a
/// single visual line. This makes word segmentation correct in two cases the old
/// per-visual-line code broke:
///   * bidi text, where visual order differs from logical order, so word boundaries
///     computed on the visual concatenation mapped back to the wrong clusters, and
///   * a word split across a soft wrap, where filtering to one `line_index` only
///     selected the fragment on the clicked line.
fn extract_line_text_and_clusters(
    item_idx: usize,
    layout: &UnifiedLayout,
) -> (String, Vec<(GraphemeClusterId, usize)>) {
    let Some(source_run) = layout.items[item_idx]
        .item
        .as_cluster()
        .map(|c| c.source_cluster_id.source_run)
    else {
        return (String::new(), Vec::new());
    };

    // Gather every cluster of this logical run across all visual lines, then sort
    // into logical order so segmentation runs on the real character sequence.
    let mut clusters: Vec<&ShapedCluster> = layout
        .items
        .iter()
        .filter_map(|item| item.item.as_cluster())
        .filter(|c| c.source_cluster_id.source_run == source_run)
        .collect();
    clusters.sort_by_key(|c| c.source_cluster_id.start_byte_in_run);

    let mut text = String::new();
    let mut cluster_map = Vec::new();
    for c in clusters {
        let s = c.text.as_str();
        cluster_map.push((c.source_cluster_id, s.len()));
        text.push_str(s);
    }

    (text, cluster_map)
}

/// Map a byte offset in concatenated line text back to a cluster ID.
fn byte_offset_to_cluster_id(
    cluster_map: &[(GraphemeClusterId, usize)],
    byte_offset: usize,
) -> Option<GraphemeClusterId> {
    let mut cumulative = 0;
    for (id, len) in cluster_map {
        if byte_offset < cumulative + len {
            return Some(*id);
        }
        cumulative += len;
    }
    cluster_map.last().map(|(id, _)| *id)
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
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();

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
    for (byte_idx, ch) in &char_indices {
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
                .map_or(0, |(idx, c)| idx + c.len_utf8());

            let end = char_indices
                .iter()
                .find(|(idx, c)| *idx > cursor_offset && is_word_char(*c))
                .map_or(text.len(), |(idx, _)| *idx);

            return (start, end);
        }
    }

    (word_start, word_end)
}

// Word-character classification is shared with cursor word-motion via
// `cache::is_word_char` (imported above) so selection and Ctrl/Alt+Arrow agree
// on punctuation. Kept distinct from `cache::is_word_separator`, which is for
// word-spacing justification, not segmentation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_boundaries_simple() {
        let text = "Hello World";
        let (start, end) = find_word_boundaries(text, 2);
        assert_eq!(&text[start..end], "Hello");

        let (start, end) = find_word_boundaries(text, 7);
        assert_eq!(&text[start..end], "World");

        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], " ");
    }

    #[test]
    fn test_word_boundaries_start_end() {
        let text = "Hello";
        let (start, end) = find_word_boundaries(text, 0);
        assert_eq!(&text[start..end], "Hello");

        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], "Hello");
    }

    #[test]
    fn test_word_boundaries_punctuation() {
        let text = "Hello, World!";
        let (start, end) = find_word_boundaries(text, 2);
        assert_eq!(&text[start..end], "Hello");

        let (start, end) = find_word_boundaries(text, 5);
        assert_eq!(&text[start..end], ", ");

        let (start, end) = find_word_boundaries(text, 8);
        assert_eq!(&text[start..end], "World");
    }

    #[test]
    fn test_word_boundaries_underscore() {
        let text = "hello_world";
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

    #[test]
    fn test_word_boundaries_empty() {
        let (start, end) = find_word_boundaries("", 0);
        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    #[test]
    fn test_byte_offset_to_cluster_id_basic() {
        let id0 = GraphemeClusterId { source_run: 0, start_byte_in_run: 0 };
        let id1 = GraphemeClusterId { source_run: 0, start_byte_in_run: 5 };
        let id2 = GraphemeClusterId { source_run: 0, start_byte_in_run: 6 };
        let map = vec![(id0, 5), (id1, 1), (id2, 5)];

        assert_eq!(byte_offset_to_cluster_id(&map, 0), Some(id0));
        assert_eq!(byte_offset_to_cluster_id(&map, 4), Some(id0));
        assert_eq!(byte_offset_to_cluster_id(&map, 5), Some(id1));
        assert_eq!(byte_offset_to_cluster_id(&map, 6), Some(id2));
        assert_eq!(byte_offset_to_cluster_id(&map, 100), Some(id2));
    }
}
