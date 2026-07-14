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

/// Adversarial unit tests generated for `layout/src/text3/selection.rs`.
///
/// These push the selection helpers at the boundaries the production callers never
/// reach: `usize::MAX` byte offsets, offsets that land *inside* a multi-byte char,
/// zero-length clusters, empty layouts, cluster ids that do not exist, visually
/// reordered (bidi) item vectors, words split across a soft wrap, and cluster
/// metadata that contradicts the cluster text. Where the current behaviour is
/// surprising but real (an empty range for a trailing boundary char; a combining
/// mark splitting a word; paragraph selection returning a *logically inverted*
/// range for reordered runs) the test PINS that behaviour and says so rather than
/// pretending it is correct.
#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::too_many_lines
)]
mod autotest_generated {
    use std::sync::Arc;

    use azul_core::selection::ContentIndex;

    use super::*;
    use crate::text3::cache::{
        BidiDirection, OverflowInfo, Point, Rect, ShapedGlyphVec, StyleProperties,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    const fn gid(run: u32, byte: u32) -> GraphemeClusterId {
        GraphemeClusterId {
            source_run: run,
            start_byte_in_run: byte,
        }
    }

    const fn ci(run: u32, item: u32) -> ContentIndex {
        ContentIndex {
            run_index: run,
            item_index: item,
        }
    }

    fn cluster(text: &str, id: GraphemeClusterId) -> ShapedCluster {
        ShapedCluster {
            text: text.to_string(),
            source_cluster_id: id,
            source_content_index: ci(id.source_run, id.start_byte_in_run),
            source_node_id: None,
            glyphs: ShapedGlyphVec::new(),
            advance: 10.0,
            direction: BidiDirection::Ltr,
            style: Arc::new(StyleProperties::default()),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        }
    }

    /// A cluster item on `line`.
    fn cl(text: &str, id: GraphemeClusterId, line: usize) -> PositionedItem {
        PositionedItem {
            item: ShapedItem::Cluster(cluster(text, id)),
            position: Point::default(),
            line_index: line,
        }
    }

    /// A non-cluster item (`as_cluster()` returns `None`) on `line`.
    fn tab(line: usize) -> PositionedItem {
        PositionedItem {
            item: ShapedItem::Tab {
                source: ci(0, 0),
                bounds: Rect::default(),
            },
            position: Point::default(),
            line_index: line,
        }
    }

    fn layout_of(items: Vec<PositionedItem>) -> UnifiedLayout {
        UnifiedLayout {
            items,
            overflow: OverflowInfo::default(),
        }
    }

    /// One cluster per `char` of `text`, all in `run`, all on line 0, with
    /// `start_byte_in_run` equal to the real logical byte offset of the char.
    fn layout_from_str(text: &str, run: u32) -> UnifiedLayout {
        layout_of(
            text.char_indices()
                .map(|(byte_idx, ch)| {
                    let mut buf = [0u8; 4];
                    cl(ch.encode_utf8(&mut buf), gid(run, byte_idx as u32), 0)
                })
                .collect(),
        )
    }

    const fn cursor_at(id: GraphemeClusterId) -> TextCursor {
        TextCursor {
            cluster_id: id,
            affinity: CursorAffinity::Leading,
        }
    }

    /// Strings chosen to break byte/char assumptions: ASCII, CJK (alphanumeric,
    /// 3 bytes), emoji (NOT alphanumeric, 4 bytes), Arabic (RTL), NBSP (a 2-byte
    /// *non*-word char), a combining mark, and pathological all-boundary input.
    const NASTY: &[&str] = &[
        "",
        " ",
        "_",
        "a",
        "!",
        "Hello World",
        "Hello, World!",
        "  ",
        "ab ",
        " ab",
        "héllo wörld",
        "日本語のテキスト",
        "👍👍",
        "a👍b",
        "مرحبا بالعالم",
        "a\u{00A0}b",
        "a\u{0301}b",
        "!!!???",
        "foo_bar42",
        "\n\t\r ",
    ];

    // ------------------------------------------------------------------
    // find_word_boundaries — numeric: zero / min_max / overflow / unicode
    // ------------------------------------------------------------------

    #[test]
    fn word_boundaries_empty_text_at_any_offset_is_zero_zero() {
        for off in [0, 1, 7, usize::MAX / 2, usize::MAX] {
            assert_eq!(
                find_word_boundaries("", off),
                (0, 0),
                "empty text must collapse to (0, 0) for offset {off}"
            );
        }
    }

    #[test]
    fn word_boundaries_usize_max_offset_is_clamped_to_text_len() {
        let text = "Hello World";
        let at_max = find_word_boundaries(text, usize::MAX);
        let at_len = find_word_boundaries(text, text.len());

        assert_eq!(at_max, at_len, "usize::MAX must clamp to text.len()");
        assert_eq!(&text[at_max.0..at_max.1], "World");
    }

    /// The load-bearing safety invariant: whatever offset it is handed — including
    /// offsets *inside* a multi-byte char and offsets past the end — the returned
    /// pair must be sliceable, ordered, and on char boundaries. Callers slice
    /// `&text[start..end]`, so a violation here is an immediate panic in the caller.
    #[test]
    fn word_boundaries_invariants_hold_for_every_offset_of_nasty_unicode() {
        for &text in NASTY {
            let probes = (0..=text.len() + 4)
                .chain([usize::MAX - 1, usize::MAX])
                .collect::<Vec<_>>();

            for off in probes {
                let (start, end) = find_word_boundaries(text, off);

                assert!(
                    start <= end,
                    "{text:?} @ {off}: start {start} > end {end} (inverted range)"
                );
                assert!(
                    end <= text.len(),
                    "{text:?} @ {off}: end {end} past len {}",
                    text.len()
                );
                assert!(
                    text.is_char_boundary(start),
                    "{text:?} @ {off}: start {start} splits a char"
                );
                assert!(
                    text.is_char_boundary(end),
                    "{text:?} @ {off}: end {end} splits a char"
                );
                // Must not panic — this is what every caller does with the result.
                let _slice = &text[start..end];
            }
        }
    }

    #[test]
    fn word_boundaries_offset_inside_multibyte_char_does_not_split_it() {
        // 'é' occupies bytes 1..3; offset 2 is *inside* it.
        let text = "héllo";
        let (start, end) = find_word_boundaries(text, 2);
        assert_eq!(&text[start..end], "héllo");

        // NBSP occupies bytes 1..3 and is NOT a word char; offset 2 is inside it.
        let text = "a\u{00A0}b";
        let (start, end) = find_word_boundaries(text, 2);
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert_eq!(&text[start..end], "b");
    }

    /// PINNED QUIRK: an offset that sits *after* a trailing boundary char (only
    /// reachable by a direct call, not through a cluster id) yields the empty
    /// range `(len, len)` rather than selecting the trailing whitespace.
    #[test]
    fn word_boundaries_offset_past_trailing_boundary_char_yields_empty_range() {
        let text = "ab ";
        assert_eq!(find_word_boundaries(text, 3), (3, 3));
        assert_eq!(&text[3..3], "");
    }

    /// PINNED QUIRK: `is_word_char` is `is_alphanumeric() || '_'`, and a combining
    /// mark (category Mn) is neither — so decomposed "á" is segmented as TWO words.
    /// NFC "á" (a single precomposed alphanumeric char) is not. Real Unicode
    /// weakness of the heuristic; recorded, not worked around.
    #[test]
    fn word_boundaries_combining_mark_splits_a_word() {
        let decomposed = "a\u{0301}b"; // a + COMBINING ACUTE + b
        let (start, end) = find_word_boundaries(decomposed, 0);
        assert_eq!(
            &decomposed[start..end],
            "a",
            "combining mark is treated as a word boundary"
        );

        let precomposed = "áb";
        let (start, end) = find_word_boundaries(precomposed, 0);
        assert_eq!(&precomposed[start..end], "áb");
    }

    #[test]
    fn word_boundaries_emoji_is_a_boundary_char_cjk_is_a_word_char() {
        // Emoji are not alphanumeric → boundary run selected whole.
        let emoji = "👍👍";
        assert_eq!(find_word_boundaries(emoji, 0), (0, emoji.len()));

        // Ideographs ARE alphanumeric → one word.
        let cjk = "日本語";
        let (start, end) = find_word_boundaries(cjk, 3);
        assert_eq!(&cjk[start..end], "日本語");

        // Emoji between words acts as a separator.
        let mixed = "a👍b";
        let (start, end) = find_word_boundaries(mixed, 0);
        assert_eq!(&mixed[start..end], "a");
    }

    #[test]
    fn word_boundaries_all_boundary_chars_selects_the_whole_run() {
        let text = "!!!???";
        assert_eq!(find_word_boundaries(text, 0), (0, 6));
        assert_eq!(find_word_boundaries(text, 3), (0, 6));
        assert_eq!(find_word_boundaries(text, 5), (0, 6));
    }

    #[test]
    fn word_boundaries_huge_text_with_max_offset_does_not_overflow() {
        let text = "a".repeat(64 * 1024);
        let (start, end) = find_word_boundaries(&text, usize::MAX);
        assert_eq!((start, end), (0, text.len()));

        // …and the same for a huge all-boundary text.
        let sep = " ".repeat(64 * 1024);
        let (start, end) = find_word_boundaries(&sep, usize::MAX);
        assert!(start <= end && end <= sep.len());
    }

    // ------------------------------------------------------------------
    // byte_offset_to_cluster_id — numeric: zero / min_max / overflow
    // ------------------------------------------------------------------

    #[test]
    fn byte_offset_to_cluster_id_empty_map_is_none_for_every_offset() {
        for off in [0, 1, usize::MAX / 2, usize::MAX] {
            assert_eq!(byte_offset_to_cluster_id(&[], off), None);
        }
    }

    /// Non-empty map ⇒ ALWAYS `Some` (offsets past the end fall back to the last
    /// cluster). Exhaustive over every offset in and beyond the mapped range.
    #[test]
    fn byte_offset_to_cluster_id_non_empty_map_is_always_some() {
        let map = [(gid(0, 0), 3), (gid(0, 3), 1), (gid(0, 4), 2)];
        let total: usize = map.iter().map(|(_, l)| l).sum();

        for off in (0..=total + 8).chain([usize::MAX - 1, usize::MAX]) {
            assert!(
                byte_offset_to_cluster_id(&map, off).is_some(),
                "offset {off} returned None for a non-empty map"
            );
        }
        assert_eq!(byte_offset_to_cluster_id(&map, 0), Some(gid(0, 0)));
        assert_eq!(byte_offset_to_cluster_id(&map, total - 1), Some(gid(0, 4)));
        assert_eq!(byte_offset_to_cluster_id(&map, usize::MAX), Some(gid(0, 4)));
    }

    /// PINNED QUIRK: a zero-length cluster is *unaddressable* — `offset < cum + 0`
    /// is never true — so it is silently skipped and the next cluster wins.
    #[test]
    fn byte_offset_to_cluster_id_zero_length_clusters_are_skipped() {
        let map = [(gid(0, 0), 0), (gid(0, 1), 2), (gid(0, 3), 0)];

        assert_eq!(
            byte_offset_to_cluster_id(&map, 0),
            Some(gid(0, 1)),
            "leading zero-length cluster must be skipped, not returned"
        );
        assert_eq!(byte_offset_to_cluster_id(&map, 1), Some(gid(0, 1)));
        // Past the end → last entry, even though the last entry is zero-length.
        assert_eq!(byte_offset_to_cluster_id(&map, 2), Some(gid(0, 3)));
    }

    #[test]
    fn byte_offset_to_cluster_id_all_zero_length_map_returns_last_never_none() {
        let map = [(gid(0, 0), 0), (gid(0, 1), 0), (gid(0, 2), 0)];
        for off in [0, 1, usize::MAX] {
            assert_eq!(byte_offset_to_cluster_id(&map, off), Some(gid(0, 2)));
        }
    }

    /// Cluster lengths at the top of the `usize` range: a single `usize::MAX`-long
    /// cluster, and two `usize::MAX / 2`-long ones whose running sum stays just
    /// below the overflow point. The internal `cumulative + len` must not wrap.
    #[test]
    fn byte_offset_to_cluster_id_huge_lengths_do_not_overflow() {
        let single = [(gid(0, 0), usize::MAX)];
        assert_eq!(byte_offset_to_cluster_id(&single, 0), Some(gid(0, 0)));
        assert_eq!(
            byte_offset_to_cluster_id(&single, usize::MAX - 1),
            Some(gid(0, 0))
        );
        // offset == len → falls through the loop, then to the `last()` fallback.
        assert_eq!(
            byte_offset_to_cluster_id(&single, usize::MAX),
            Some(gid(0, 0))
        );

        let half = usize::MAX / 2; // 2*half == usize::MAX - 1, no wrap.
        let pair = [(gid(0, 0), half), (gid(0, 1), half)];
        assert_eq!(byte_offset_to_cluster_id(&pair, 0), Some(gid(0, 0)));
        assert_eq!(byte_offset_to_cluster_id(&pair, half - 1), Some(gid(0, 0)));
        assert_eq!(byte_offset_to_cluster_id(&pair, half), Some(gid(0, 1)));
        assert_eq!(byte_offset_to_cluster_id(&pair, usize::MAX), Some(gid(0, 1)));
    }

    // ------------------------------------------------------------------
    // find_cluster_at_cursor — getters / predicates: invariants
    // ------------------------------------------------------------------

    #[test]
    fn find_cluster_at_cursor_empty_layout_is_none() {
        let layout = layout_of(vec![]);
        assert!(find_cluster_at_cursor(&cursor_at(gid(0, 0)), &layout).is_none());
        assert!(find_cluster_at_cursor(&cursor_at(gid(u32::MAX, u32::MAX)), &layout).is_none());
    }

    #[test]
    fn find_cluster_at_cursor_unknown_id_is_none() {
        let layout = layout_from_str("abc", 0);
        // Right run, byte offset past the end.
        assert!(find_cluster_at_cursor(&cursor_at(gid(0, 99)), &layout).is_none());
        // Right byte offset, wrong run.
        assert!(find_cluster_at_cursor(&cursor_at(gid(7, 0)), &layout).is_none());
        // Saturated id.
        assert!(find_cluster_at_cursor(&cursor_at(gid(u32::MAX, u32::MAX)), &layout).is_none());
    }

    #[test]
    fn find_cluster_at_cursor_skips_non_cluster_items_and_reports_visual_index() {
        let layout = layout_of(vec![tab(0), cl("a", gid(0, 0), 0), tab(0)]);
        let (idx, found) = find_cluster_at_cursor(&cursor_at(gid(0, 0)), &layout).unwrap();
        assert_eq!(idx, 1, "index must be into layout.items, skipping the tab");
        assert_eq!(found.text, "a");
    }

    /// Duplicate cluster ids are not supposed to happen; if they do, the FIRST
    /// visual match wins. Pinned so a change of iteration order is caught.
    #[test]
    fn find_cluster_at_cursor_duplicate_ids_return_the_first_match() {
        let layout = layout_of(vec![cl("x", gid(0, 0), 0), cl("y", gid(0, 0), 1)]);
        let (idx, found) = find_cluster_at_cursor(&cursor_at(gid(0, 0)), &layout).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(found.text, "x");
    }

    // ------------------------------------------------------------------
    // extract_line_text_and_clusters — numeric: index bounds / ordering
    // ------------------------------------------------------------------

    /// `layout.items[item_idx]` is an unchecked index: the function's contract is
    /// that `item_idx` comes from `find_cluster_at_cursor`. Pin the panic so that
    /// contract is not silently broadened.
    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn extract_line_text_out_of_bounds_index_panics_on_empty_layout() {
        let layout = layout_of(vec![]);
        let _ = extract_line_text_and_clusters(0, &layout);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn extract_line_text_usize_max_index_panics() {
        let layout = layout_from_str("abc", 0);
        let _ = extract_line_text_and_clusters(usize::MAX, &layout);
    }

    #[test]
    fn extract_line_text_non_cluster_item_yields_empty_text_and_map() {
        let layout = layout_of(vec![tab(0), cl("a", gid(0, 0), 0)]);
        let (text, map) = extract_line_text_and_clusters(0, &layout);
        assert!(text.is_empty());
        assert!(map.is_empty());
    }

    #[test]
    fn extract_line_text_zero_index_on_a_cluster_gathers_the_whole_run() {
        let layout = layout_from_str("hi there", 0);
        let (text, map) = extract_line_text_and_clusters(0, &layout);
        assert_eq!(text, "hi there");
        assert_eq!(map.len(), 8);
        assert_eq!(map[0], (gid(0, 0), 1));
    }

    /// Documented behaviour: clusters are gathered by LOGICAL run and sorted by
    /// `start_byte_in_run`, so a visually reordered (bidi) item vector must still
    /// concatenate in logical order.
    #[test]
    fn extract_line_text_restores_logical_order_from_reversed_visual_items() {
        let layout = layout_of(vec![
            cl("o", gid(0, 4), 0),
            cl("l", gid(0, 3), 0),
            cl("l", gid(0, 2), 0),
            cl("e", gid(0, 1), 0),
            cl("H", gid(0, 0), 0),
        ]);
        let (text, map) = extract_line_text_and_clusters(0, &layout);
        assert_eq!(text, "Hello", "visual order must not leak into the text");
        assert_eq!(
            map.iter().map(|(id, _)| id.start_byte_in_run).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
    }

    /// Documented behaviour: gathering is NOT restricted to one visual line, so a
    /// word split by a soft wrap is still reassembled.
    #[test]
    fn extract_line_text_crosses_visual_lines_and_excludes_other_runs() {
        let layout = layout_of(vec![
            cl("H", gid(0, 0), 0),
            cl("e", gid(0, 1), 0),
            cl("l", gid(0, 2), 1), // soft-wrapped onto line 1
            cl("X", gid(1, 0), 1), // a different logical run — must be excluded
            cl("o", gid(0, 3), 2), // and onto line 2
        ]);
        let (text, map) = extract_line_text_and_clusters(0, &layout);
        assert_eq!(text, "Helo");
        assert_eq!(map.len(), 4);
        assert!(map.iter().all(|(id, _)| id.source_run == 0));
    }

    #[test]
    fn extract_line_text_byte_lengths_are_utf8_lengths_not_char_counts() {
        let layout = layout_from_str("é日👍", 0);
        let (text, map) = extract_line_text_and_clusters(0, &layout);
        assert_eq!(text, "é日👍");
        assert_eq!(
            map.iter().map(|(_, len)| *len).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(map.iter().map(|(_, l)| l).sum::<usize>(), text.len());
    }

    // ------------------------------------------------------------------
    // select_word_at_cursor — round-trip + invariants
    // ------------------------------------------------------------------

    #[test]
    fn select_word_empty_layout_is_none() {
        let layout = layout_of(vec![]);
        assert!(select_word_at_cursor(&cursor_at(gid(0, 0)), &layout).is_none());
    }

    #[test]
    fn select_word_unknown_cursor_is_none() {
        let layout = layout_from_str("Hello", 0);
        assert!(select_word_at_cursor(&cursor_at(gid(3, 0)), &layout).is_none());
        assert!(select_word_at_cursor(&cursor_at(gid(0, u32::MAX)), &layout).is_none());
    }

    #[test]
    fn select_word_selects_the_word_under_the_cursor_with_correct_affinities() {
        let layout = layout_from_str("Hello World", 0);
        let range = select_word_at_cursor(&cursor_at(gid(0, 1)), &layout).unwrap();

        assert_eq!(range.start.cluster_id, gid(0, 0), "start of \"Hello\"");
        assert_eq!(range.end.cluster_id, gid(0, 4), "last cluster of \"Hello\"");
        assert_eq!(range.start.affinity, CursorAffinity::Leading);
        assert_eq!(range.end.affinity, CursorAffinity::Trailing);

        let range = select_word_at_cursor(&cursor_at(gid(0, 8)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 6));
        assert_eq!(range.end.cluster_id, gid(0, 10));
    }

    /// A word broken by a soft wrap must select whole, not just the clicked fragment.
    #[test]
    fn select_word_spans_a_soft_wrap() {
        let layout = layout_of(vec![
            cl("H", gid(0, 0), 0),
            cl("e", gid(0, 1), 0),
            cl("l", gid(0, 2), 0),
            cl("l", gid(0, 3), 1), // wrapped
            cl("o", gid(0, 4), 1),
        ]);
        let range = select_word_at_cursor(&cursor_at(gid(0, 1)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 0));
        assert_eq!(range.end.cluster_id, gid(0, 4), "must cross the line break");
    }

    /// Bidi: items in visual (reversed) order must still yield the logical word.
    #[test]
    fn select_word_uses_logical_not_visual_order() {
        let layout = layout_of(vec![
            cl("o", gid(0, 4), 0),
            cl("l", gid(0, 3), 0),
            cl("l", gid(0, 2), 0),
            cl("e", gid(0, 1), 0),
            cl("H", gid(0, 0), 0),
        ]);
        let range = select_word_at_cursor(&cursor_at(gid(0, 3)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 0));
        assert_eq!(range.end.cluster_id, gid(0, 4));
    }

    /// Round-trip / idempotence: re-selecting from the START of a returned range
    /// must reproduce the identical range. A fixpoint failure here would make
    /// double-click-then-drag jitter.
    #[test]
    fn select_word_is_idempotent_from_its_own_start_cursor() {
        for text in ["Hello, World! foo_bar 42", "a  b", "héllo wörld", "!!!a"] {
            let layout = layout_from_str(text, 0);

            for (byte_idx, _) in text.char_indices() {
                let cur = cursor_at(gid(0, byte_idx as u32));
                let first = select_word_at_cursor(&cur, &layout)
                    .unwrap_or_else(|| panic!("{text:?} @ {byte_idx}: no selection"));
                let again = select_word_at_cursor(&first.start, &layout)
                    .unwrap_or_else(|| panic!("{text:?} @ {byte_idx}: re-select failed"));

                assert_eq!(
                    first, again,
                    "{text:?} @ {byte_idx}: selection is not a fixpoint"
                );
            }
        }
    }

    /// Invariants over every reachable cursor of every nasty string: always `Some`,
    /// never inverted, both endpoints are real clusters of the layout, affinities fixed.
    #[test]
    fn select_word_invariants_hold_for_every_cursor_of_nasty_unicode() {
        for &text in NASTY {
            let layout = layout_from_str(text, 0);
            let ids: Vec<GraphemeClusterId> = text
                .char_indices()
                .map(|(b, _)| gid(0, b as u32))
                .collect();

            for id in &ids {
                let range = select_word_at_cursor(&cursor_at(*id), &layout)
                    .unwrap_or_else(|| panic!("{text:?} @ {id:?}: expected a selection"));

                assert!(
                    range.start.cluster_id <= range.end.cluster_id,
                    "{text:?} @ {id:?}: inverted range {range:?}"
                );
                assert!(
                    ids.contains(&range.start.cluster_id),
                    "{text:?} @ {id:?}: start is not a cluster of the layout"
                );
                assert!(
                    ids.contains(&range.end.cluster_id),
                    "{text:?} @ {id:?}: end is not a cluster of the layout"
                );
                assert_eq!(range.start.affinity, CursorAffinity::Leading);
                assert_eq!(range.end.affinity, CursorAffinity::Trailing);
            }
        }
    }

    /// A zero-length cluster (empty `text`) cannot be addressed by a byte offset,
    /// so selecting *on* it resolves to the neighbouring cluster instead of panicking.
    #[test]
    fn select_word_on_zero_length_cluster_resolves_to_a_neighbour() {
        let layout = layout_of(vec![cl("", gid(0, 0), 0), cl("x", gid(0, 1), 0)]);
        let range = select_word_at_cursor(&cursor_at(gid(0, 0)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 1));
        assert_eq!(range.end.cluster_id, gid(0, 1));
    }

    #[test]
    fn select_word_all_clusters_empty_does_not_panic() {
        let layout = layout_of(vec![cl("", gid(0, 0), 0), cl("", gid(0, 1), 0)]);
        let range = select_word_at_cursor(&cursor_at(gid(0, 0)), &layout).unwrap();
        // Empty concatenated text ⇒ boundaries (0, 0) ⇒ both ends fall back to last().
        assert_eq!(range.start.cluster_id, gid(0, 1));
        assert_eq!(range.end.cluster_id, gid(0, 1));
    }

    /// Cluster metadata that contradicts the cluster text (`start_byte_in_run`
    /// values that do not match the utf-8 lengths) must not panic or slice mid-char.
    #[test]
    fn select_word_with_inconsistent_cluster_metadata_does_not_panic() {
        let layout = layout_of(vec![
            cl("abc", gid(0, 0), 0), // claims 1 byte, is 3
            cl("def", gid(0, 1), 0),
            cl("👍", gid(0, 2), 0), // multi-byte at a bogus offset
        ]);
        for id in [gid(0, 0), gid(0, 1), gid(0, 2)] {
            let range = select_word_at_cursor(&cursor_at(id), &layout);
            assert!(range.is_some(), "{id:?} must still resolve");
        }
    }

    #[test]
    fn select_word_large_layout_stays_correct_and_does_not_panic() {
        // 4000 clusters: 500 × "word " (word chars + a separator).
        let text = "word ".repeat(800);
        let layout = layout_from_str(&text, 0);

        // Cursor in the middle of the 400th word.
        let word_start = 400 * 5;
        let range = select_word_at_cursor(&cursor_at(gid(0, word_start as u32 + 2)), &layout)
            .expect("mid-word cursor must select");
        assert_eq!(range.start.cluster_id, gid(0, word_start as u32));
        assert_eq!(range.end.cluster_id, gid(0, word_start as u32 + 3));

        // Cursor on the separator selects just the separator.
        let sep = word_start + 4;
        let range = select_word_at_cursor(&cursor_at(gid(0, sep as u32)), &layout)
            .expect("separator cursor must select");
        assert_eq!(range.start.cluster_id, gid(0, sep as u32));
        assert_eq!(range.end.cluster_id, gid(0, sep as u32));
    }

    // ------------------------------------------------------------------
    // select_paragraph_at_cursor — invariants
    // ------------------------------------------------------------------

    #[test]
    fn select_paragraph_empty_layout_is_none() {
        let layout = layout_of(vec![]);
        assert!(select_paragraph_at_cursor(&cursor_at(gid(0, 0)), &layout).is_none());
    }

    #[test]
    fn select_paragraph_unknown_cursor_is_none() {
        let layout = layout_from_str("abc", 0);
        assert!(select_paragraph_at_cursor(&cursor_at(gid(9, 9)), &layout).is_none());
        assert!(
            select_paragraph_at_cursor(&cursor_at(gid(u32::MAX, u32::MAX)), &layout).is_none()
        );
    }

    #[test]
    fn select_paragraph_covers_only_the_cursors_line() {
        let layout = layout_of(vec![
            cl("a", gid(0, 0), 0),
            cl("b", gid(0, 1), 0),
            cl("c", gid(0, 2), 1),
            cl("d", gid(0, 3), 1),
        ]);

        let range = select_paragraph_at_cursor(&cursor_at(gid(0, 3)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 2), "line 1 starts at 'c'");
        assert_eq!(range.end.cluster_id, gid(0, 3));
        assert_eq!(range.start.affinity, CursorAffinity::Leading);
        assert_eq!(range.end.affinity, CursorAffinity::Trailing);

        let range = select_paragraph_at_cursor(&cursor_at(gid(0, 0)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 0));
        assert_eq!(range.end.cluster_id, gid(0, 1), "must not spill onto line 1");
    }

    #[test]
    fn select_paragraph_ignores_non_cluster_items_at_the_line_edges() {
        let layout = layout_of(vec![
            tab(0),
            cl("a", gid(0, 0), 0),
            cl("b", gid(0, 1), 0),
            tab(0),
        ]);
        let range = select_paragraph_at_cursor(&cursor_at(gid(0, 1)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 0));
        assert_eq!(range.end.cluster_id, gid(0, 1));
    }

    #[test]
    fn select_paragraph_handles_saturated_line_index() {
        let layout = layout_of(vec![
            cl("a", gid(0, 0), 0),
            cl("b", gid(0, 1), usize::MAX),
            cl("c", gid(0, 2), usize::MAX),
        ]);
        let range = select_paragraph_at_cursor(&cursor_at(gid(0, 2)), &layout).unwrap();
        assert_eq!(range.start.cluster_id, gid(0, 1));
        assert_eq!(range.end.cluster_id, gid(0, 2));
    }

    /// PINNED QUIRK: paragraph selection walks `layout.items` in VISUAL order,
    /// unlike `select_word_at_cursor`, which sorts into logical order. For a
    /// visually reordered (RTL) run the returned range is therefore logically
    /// INVERTED (start > end). Recorded as-is — a caller that assumes
    /// `start <= end` will mis-highlight RTL lines.
    #[test]
    fn select_paragraph_returns_a_logically_inverted_range_for_reordered_runs() {
        let layout = layout_of(vec![
            cl("o", gid(0, 4), 0),
            cl("l", gid(0, 3), 0),
            cl("l", gid(0, 2), 0),
            cl("e", gid(0, 1), 0),
            cl("H", gid(0, 0), 0),
        ]);
        let range = select_paragraph_at_cursor(&cursor_at(gid(0, 2)), &layout).unwrap();

        assert_eq!(range.start.cluster_id, gid(0, 4), "visually-first cluster");
        assert_eq!(range.end.cluster_id, gid(0, 0), "visually-last cluster");
        assert!(
            range.start.cluster_id > range.end.cluster_id,
            "pinned: the range is logically inverted for visual order"
        );
    }

    /// Whenever the cursor resolves to a cluster, paragraph selection must resolve
    /// too (its line always contains at least that cluster) — never `None`.
    #[test]
    fn select_paragraph_is_some_for_every_reachable_cursor() {
        for &text in NASTY {
            let layout = layout_from_str(text, 0);
            for (byte_idx, _) in text.char_indices() {
                let cur = cursor_at(gid(0, byte_idx as u32));
                assert!(
                    select_paragraph_at_cursor(&cur, &layout).is_some(),
                    "{text:?} @ {byte_idx}: cursor found a cluster but no paragraph"
                );
            }
        }
    }
}
