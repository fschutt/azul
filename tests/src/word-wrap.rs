/// Tests for text3 line breaking and word wrapping.
///
/// Exercises `break_one_line` with synthetic `ShapedItem::Cluster` items
/// (known advance widths, no real fonts needed) to verify W3C CSS Text 3
/// overflow-wrap and word-break behavior.
///
/// Scenarios:
/// - Words that fit on a single line
/// - Words that overflow → move to next line (soft wrap at space)
/// - Words wider than container → emergency break (overflow-wrap: break-word)
/// - Available width = 0 → content overflows on one line (not one-char-per-line)
/// - Multiple words with spaces as break opportunities

use std::sync::Arc;

use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_layout::text3::cache::{
    BreakCursor, LineConstraints, LineSegment, OverflowWrap,
    ShapedCluster, ShapedItem, StyleProperties, WhiteSpaceMode,
    break_one_line, get_item_measure,
    LineBreakStrictness, LoadedFonts,
};
use azul_layout::FontRef;

// ============================================================================
// Helpers
// ============================================================================

/// Cluster with given text and advance width (no glyphs — only advance matters
/// for line breaking).
fn cluster(text: &str, advance: f32, byte: u32) -> ShapedItem {
    ShapedItem::Cluster(ShapedCluster {
        text: text.to_string(),
        source_cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        source_content_index: ContentIndex {
            run_index: 0,
            item_index: byte,
        },
        source_node_id: None,
        glyphs: Vec::new(),
        advance,
        direction: azul_core::selection::BidiDirection::Ltr,
        style: Arc::new(StyleProperties::default()),
        is_outside_marker: false,
    })
}

/// Space cluster (break opportunity).
fn space(advance: f32, byte: u32) -> ShapedItem {
    cluster(" ", advance, byte)
}

fn constraints(width: f32) -> LineConstraints {
    LineConstraints {
        segments: vec![LineSegment {
            start_x: 0.0,
            width,
            priority: 0,
        }],
        total_available: width,
    }
}

/// Break all items into lines, return Vec of (line_text, line_width).
fn break_all(
    items: &[ShapedItem],
    width: f32,
    overflow_wrap: OverflowWrap,
) -> Vec<(String, f32)> {
    let mut cursor = BreakCursor::new(items);
    let lc = constraints(width);
    let fonts: LoadedFonts<FontRef> = LoadedFonts::new();
    let mut lines = Vec::new();

    for _ in 0..1000 {
        if cursor.is_done() {
            break;
        }
        let (line_items, _hyph) = break_one_line(
            &mut cursor,
            &lc,
            false,
            None,
            &fonts,
            LineBreakStrictness::default(),
            WhiteSpaceMode::Normal,
            overflow_wrap,
        );
        if line_items.is_empty() {
            break;
        }
        let text: String = line_items.iter().map(|i| match i {
            ShapedItem::Cluster(c) => c.text.as_str(),
            _ => "",
        }).collect();
        let w: f32 = line_items.iter().map(|i| get_item_measure(i, false)).sum();
        lines.push((text, w));
    }
    lines
}

// ============================================================================
// Tests
// ============================================================================

/// Single word fits on one line.
#[test]
fn word_fits_single_line() {
    // "Hello" = 5 × 10px = 50px, container 200px
    let items: Vec<_> = "Hello".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 200.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0.trim(), "Hello");
}

/// Two words: second overflows → wraps to line 2.
#[test]
fn word_wraps_at_space() {
    // "Hello World" — each char 10px, space 5px
    // Container 80px: "Hello" (50) fits, space (5), "World" (50) doesn't → wrap
    let items = vec![
        cluster("H", 10.0, 0), cluster("e", 10.0, 1),
        cluster("l", 10.0, 2), cluster("l", 10.0, 3),
        cluster("o", 10.0, 4), space(5.0, 5),
        cluster("W", 10.0, 6), cluster("o", 10.0, 7),
        cluster("r", 10.0, 8), cluster("l", 10.0, 9),
        cluster("d", 10.0, 10),
    ];
    let lines = break_all(&items, 80.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 2, "Should wrap to 2 lines");
    assert!(lines[0].0.starts_with("Hello"), "Line 1 = Hello");
    assert!(lines[1].0.starts_with("World"), "Line 2 = World");
}

/// Long word + `overflow-wrap: break-word` → emergency break at container edge.
#[test]
fn long_word_emergency_break() {
    // "Abcdefghij" = 10 × 10px = 100px, container 50px
    // break-word: should split into "Abcde" (50) + "fghij" (50)
    let items: Vec<_> = "Abcdefghij".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 50.0, OverflowWrap::BreakWord);
    assert_eq!(lines.len(), 2, "Should break into 2 lines of 5 chars");
    assert_eq!(lines[0].0, "Abcde");
    assert_eq!(lines[1].0, "fghij");
}

/// Long word + `overflow-wrap: normal` → overflows, stays on one line.
#[test]
fn long_word_overflow_normal() {
    let items: Vec<_> = "Abcdefghij".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 50.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 1, "overflow-wrap: normal does not break");
    assert_eq!(lines[0].0, "Abcdefghij");
}

/// Container width = 0, break-word: content overflows on one line.
/// Per CSS spec, `width: 0` means the content box is zero-width — content
/// overflows.  Must NOT produce one-character-per-line.
#[test]
fn zero_width_overflows_not_one_char_per_line() {
    let items: Vec<_> = "Hello".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 0.0, OverflowWrap::BreakWord);
    assert_eq!(lines.len(), 1, "Zero-width container: all content on one line (overflow)");
    assert_eq!(lines[0].0, "Hello");
}

/// Container width = 0, normal: same — one overflowing line.
#[test]
fn zero_width_overflow_normal() {
    let items: Vec<_> = "ABC".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 0.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, "ABC");
}

/// Each char wider than container → one char per line (correct for break-word
/// when each individual cluster overflows).
#[test]
fn each_char_wider_than_container() {
    // Each char 10px, container 5px.  After placing one char it overflows, so
    // each char gets its own line.  This is correct — the container IS wider
    // than zero, there's just not enough room for two chars.
    let items: Vec<_> = "ABC".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 5.0, OverflowWrap::BreakWord);
    assert_eq!(lines.len(), 3, "Each 10px char overflows 5px → 3 lines");
}

/// Long word after short word: gets full line width on line 2.
#[test]
fn long_word_after_short_gets_full_width() {
    // "Hi Superlongword" — container 100px
    // Line 1: "Hi " (25px)
    // Line 2: "Superlongwo" — should fill 100px, NOT just 1 char
    let mut items = vec![
        cluster("H", 10.0, 0), cluster("i", 10.0, 1), space(5.0, 2),
    ];
    for (i, c) in "Superlongword".char_indices() {
        items.push(cluster(&c.to_string(), 10.0, (3 + i) as u32));
    }

    let lines = break_all(&items, 100.0, OverflowWrap::BreakWord);
    assert!(lines.len() >= 2);
    assert!(lines[0].0.starts_with("Hi"), "Line 1 starts with Hi");
    // Line 2 should use the full 100px (10 chars), not just 1 char
    assert!(
        lines[1].0.len() >= 5,
        "Line 2 should have >=5 chars (fills 100px), got '{}'",
        lines[1].0,
    );
}

/// Three equal words in narrow container.
#[test]
fn three_words_three_lines() {
    let items = vec![
        cluster("a", 10.0, 0), cluster("a", 10.0, 1), cluster("a", 10.0, 2),
        space(5.0, 3),
        cluster("b", 10.0, 4), cluster("b", 10.0, 5), cluster("b", 10.0, 6),
        space(5.0, 7),
        cluster("c", 10.0, 8), cluster("c", 10.0, 9), cluster("c", 10.0, 10),
    ];
    let lines = break_all(&items, 40.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 3, "Each 30px word on its own 40px line");
}

/// overflow-wrap: anywhere works like break-word for layout purposes.
#[test]
fn overflow_wrap_anywhere() {
    let items: Vec<_> = "Abcdef".char_indices()
        .map(|(i, c)| cluster(&c.to_string(), 10.0, i as u32))
        .collect();
    let lines = break_all(&items, 30.0, OverflowWrap::Anywhere);
    assert_eq!(lines.len(), 2, "60px in 30px → 2 lines");
    assert_eq!(lines[0].0, "Abc");
    assert_eq!(lines[1].0, "def");
}

/// Empty input → no lines.
#[test]
fn empty_input() {
    let lines = break_all(&[], 100.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 0);
}

/// Single character fits.
#[test]
fn single_char_fits() {
    let items = vec![cluster("X", 10.0, 0)];
    let lines = break_all(&items, 100.0, OverflowWrap::Normal);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, "X");
}
