#![cfg(feature = "text_layout")]
//! Deterministic caret-motion, selection, and edit regression tests on the fake
//! font (`common/fakefont.rs`). Layout is produced by the same stage-1..4 calls
//! `window.rs` uses, then the public `UnifiedLayout` hit-testing / caret-motion /
//! selection helpers and the pure `text3::edit` transforms are exercised.
//!
//! Fake metrics @ size 20: 'a'/'b'/'c' 600u => 12px · space 250u => 5px ·
//! Hebrew 550u => 11px · U+0301 combining => 0px (stays in its base cluster).

#[path = "common/fakefont.rs"]
mod fakefont;

use std::collections::HashMap;
use std::sync::Arc;

use azul_core::geom::{LogicalPosition, LogicalRect};
use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor,
};
use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LoadedFonts,
    OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout,
};
use azul_layout::text3::edit::{delete_backward, delete_forward, edit_text, insert_text, TextEdit};
use azul_layout::text3::selection::{select_paragraph_at_cursor, select_word_at_cursor};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

use fakefont::simple_test_font;

const FONT_SIZE: f32 = 20.0;

fn assert_px(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= 0.05,
        "assert_px failed: expected {expected:.4}px, got {actual:.4}px (|delta| {delta:.4}px > 0.05px)"
    );
}

fn fake_parsed() -> ParsedFont {
    let bytes = simple_test_font();
    let mut warnings = Vec::new();
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("fake font must parse")
        .with_source_bytes(arc)
}

fn fake_font_ref() -> FontRef {
    parsed_font_to_font_ref(fake_parsed())
}

fn make_content(text: &str, font_ref: &FontRef) -> Vec<InlineContent> {
    let style = StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    };
    vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: Arc::new(style),
        logical_start_byte: 0,
        source_node_id: None,
    })]
}

fn layout_content(
    content: &[InlineContent],
    font_ref: &FontRef,
    constraints: &UnifiedConstraints,
) -> UnifiedLayout {
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());

    let logical = create_logical_items(content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout { items: Vec::new(), overflow: OverflowInfo::default() };
    }
    let base_dir = constraints.direction.unwrap_or(BidiDirection::Ltr);
    let visual = reorder_logical_items(&logical, base_dir, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, constraints, &mut None, &loaded)
        .expect("fragment layout")
}

fn layout(text: &str, width: AvailableSpace) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let content = make_content(text, &font_ref);
    layout_content(
        &content,
        &font_ref,
        &UnifiedConstraints { available_width: width, ..UnifiedConstraints::default() },
    )
}

fn layout_rtl(text: &str) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let content = make_content(text, &font_ref);
    layout_content(
        &content,
        &font_ref,
        &UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            direction: Some(BidiDirection::Rtl),
            ..UnifiedConstraints::default()
        },
    )
}

fn cursor_at(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: byte },
        affinity,
    }
}

fn cursor_at2(run: u32, byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId { source_run: run, start_byte_in_run: byte },
        affinity,
    }
}

fn max_content_width(content: &[InlineContent], font_ref: &FontRef) -> f32 {
    let l = layout_content(
        content,
        font_ref,
        &UnifiedConstraints { available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default() },
    );
    let mut mn = f32::MAX;
    let mut mx = f32::MIN;
    for it in &l.items {
        if let ShapedItem::Cluster(c) = &it.item {
            mn = mn.min(it.position.x);
            mx = mx.max(it.position.x + c.advance);
        }
    }
    if mx < mn { 0.0 } else { mx - mn }
}

fn text_of(content: &[InlineContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            InlineContent::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

// ===========================================================================
// Grapheme-aware caret motion (combining marks are atomic — UAX#29)
// ===========================================================================

#[test]
fn caret_right_skips_over_combining_mark() {
    // UAX#29: 'a'+U+0301 is one grapheme cluster (bytes 0..2). Arrow-right from its
    // start must land on the NEXT grapheme ('b' at byte 3), never the mark byte 1.
    let l = layout("a\u{0301}bc", AvailableSpace::MaxContent);
    let moved = l.move_cursor_right(cursor_at(0, CursorAffinity::Leading), &mut None);
    assert_eq!(moved.cluster_id.start_byte_in_run, 3, "right must skip the whole 'á' cluster");
}

#[test]
fn caret_left_skips_over_combining_mark() {
    // UAX#29: Arrow-left from 'b' (byte 3) lands on the base 'á' cluster (byte 0),
    // never the interior combining-mark byte 1.
    let l = layout("a\u{0301}bc", AvailableSpace::MaxContent);
    let moved = l.move_cursor_left(cursor_at(3, CursorAffinity::Leading), &mut None);
    assert_eq!(moved.cluster_id.start_byte_in_run, 0, "left must land on the 'á' cluster start");
}

#[test]
fn caret_down_preserves_column_onto_next_line() {
    // Vertical caret motion: from byte 0 (line 0) Arrow-down keeps the x-column and
    // lands on line 1. @60px each word is its own line, so it reaches the 2nd word (byte 5).
    let l = layout("aaaa aaaa aaaa", AvailableSpace::Definite(60.0));
    let mut goal_x = None;
    let moved = l.move_cursor_down(cursor_at(0, CursorAffinity::Leading), &mut goal_x, &mut None);
    assert_eq!(moved.cluster_id.start_byte_in_run, 5, "down must reach the 2nd word on line 1");
}

#[test]
fn caret_line_start_and_end() {
    // Home/End: from mid-line (byte 2) move to the line's leading edge (byte 0) and
    // trailing edge (byte 3, the last 'a' of the first word).
    let l = layout("aaaa aaaa aaaa", AvailableSpace::Definite(60.0));
    let start = l.move_cursor_to_line_start(cursor_at(2, CursorAffinity::Leading), &mut None);
    assert_eq!(start.cluster_id.start_byte_in_run, 0);
    assert_eq!(start.affinity, CursorAffinity::Leading);
    let end = l.move_cursor_to_line_end(cursor_at(2, CursorAffinity::Leading), &mut None);
    assert_eq!(end.cluster_id.start_byte_in_run, 3);
    assert_eq!(end.affinity, CursorAffinity::Trailing);
}

// ===========================================================================
// Hit-testing: caret-from-x on LTR and RTL
// ===========================================================================

#[test]
fn caret_from_x_ltr_leading_and_trailing_halves() {
    // Hit-testing: the left half of a glyph yields Leading, the right half Trailing.
    // 'a' spans 0..12 (mid 6). x=4 => byte 0 Leading; x=9 => byte 0 Trailing.
    let l = layout("abc", AvailableSpace::MaxContent);
    let lead = l.hittest_cursor(LogicalPosition::new(4.0, 8.0)).expect("caret");
    assert_eq!(lead.cluster_id.start_byte_in_run, 0);
    assert_eq!(lead.affinity, CursorAffinity::Leading);
    let trail = l.hittest_cursor(LogicalPosition::new(9.0, 8.0)).expect("caret");
    assert_eq!(trail.cluster_id.start_byte_in_run, 0);
    assert_eq!(trail.affinity, CursorAffinity::Trailing);
}

#[test]
fn caret_from_x_rtl_maps_visual_to_logical() {
    // Hit-testing in RTL "אבג": visual x runs ג@0..11, ב@11..22, א@22..33. A click on
    // the visually-leftmost glyph resolves to the LAST logical char (ג, byte 4).
    let l = layout_rtl("אבג");
    let left = l.hittest_cursor(LogicalPosition::new(2.0, 8.0)).expect("caret");
    assert_eq!(left.cluster_id.start_byte_in_run, 4, "leftmost visual glyph is ג (byte 4)");
    assert_eq!(left.affinity, CursorAffinity::Leading);
    // A click deep in the visually-rightmost glyph resolves to the FIRST logical char.
    let right = l.hittest_cursor(LogicalPosition::new(31.0, 8.0)).expect("caret");
    assert_eq!(right.cluster_id.start_byte_in_run, 0, "rightmost visual glyph is א (byte 0)");
    assert_eq!(right.affinity, CursorAffinity::Trailing);
}

// ===========================================================================
// Selection rects & word/line selection
// ===========================================================================

#[test]
fn cursor_rect_leading_and_trailing_x() {
    // Caret geometry: a Leading caret on byte 1 sits at x=12; a Trailing caret at
    // x=24. Both are 1px wide and span the 20px line box.
    let l = layout("abc", AvailableSpace::MaxContent);
    let lead: LogicalRect = l.get_cursor_rect(&cursor_at(1, CursorAffinity::Leading)).expect("rect");
    assert_px(lead.origin.x, 12.0);
    assert_px(lead.size.width, 1.0);
    assert_px(lead.size.height, 20.0);
    let trail: LogicalRect = l.get_cursor_rect(&cursor_at(1, CursorAffinity::Trailing)).expect("rect");
    assert_px(trail.origin.x, 24.0);
}

#[test]
fn select_all_is_one_rect_of_full_width() {
    // A contiguous LTR selection over the whole run collapses to a single rect
    // spanning the full 36px ("abc").
    let l = layout("abc", AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(2, CursorAffinity::Trailing),
    });
    assert_eq!(rects.len(), 1, "contiguous LTR selection => one rect");
    assert_px(rects[0].origin.x, 0.0);
    assert_px(rects[0].size.width, 36.0);
}

#[test]
fn selection_over_ltr_rtl_boundary_splits_into_two_rects() {
    // UBA: a selection crossing a direction boundary is NOT visually contiguous, so
    // it emits one rect per directional segment. "aaאב": select a(byte0)..ב(byte4).
    let l = layout_content(
        &make_content("aaאב", &fake_font_ref()),
        &fake_font_ref(),
        &UnifiedConstraints { available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default() },
    );
    let rects = l.get_selection_rects(&SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(4, CursorAffinity::Trailing),
    });
    assert!(rects.len() >= 2, "bidi selection must split into >= 2 visual rects, got {}", rects.len());
}

#[test]
fn double_click_word_stops_at_punctuation() {
    // Word selection: a double-click inside "aa" of "aa.bb" selects just "aa" (bytes
    // 0..1), stopping at the '.' boundary.
    let l = layout("aa.bb", AvailableSpace::MaxContent);
    let range = select_word_at_cursor(&cursor_at(0, CursorAffinity::Leading), &l).expect("word");
    assert_eq!(range.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(range.end.cluster_id.start_byte_in_run, 1);
}

#[test]
fn select_paragraph_covers_whole_line() {
    // Line selection: on a single-line layout the paragraph selection spans the first
    // (byte 0) through the last (byte 2) cluster.
    let l = layout("abc", AvailableSpace::MaxContent);
    let range = select_paragraph_at_cursor(&cursor_at(1, CursorAffinity::Leading), &l).expect("line");
    assert_eq!(range.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(range.end.cluster_id.start_byte_in_run, 2);
}

// ===========================================================================
// Editing across cluster boundaries, then re-layout
// ===========================================================================

#[test]
fn delete_forward_removes_whole_grapheme() {
    // UAX#29: Delete over 'á' (base+mark) removes the entire grapheme, leaving "b".
    let content = make_content("a\u{0301}b", &fake_font_ref());
    let (edited, _c) = delete_forward(&content, &cursor_at(0, CursorAffinity::Leading));
    assert_eq!(text_of(&edited), "b");
    assert!(!text_of(&edited).contains('\u{0301}'), "no orphan combining mark");
}

#[test]
fn backspace_at_end_of_run_deletes_last_grapheme() {
    // Editing across runs: a Trailing caret at the end of run 0 ("aa") Backspaces the
    // last 'a', leaving ["a","bb"] => "abb". The caret collapses into run 0.
    let font_ref = fake_font_ref();
    let content = vec![
        make_content("aa", &font_ref).remove(0),
        make_content("bb", &font_ref).remove(0),
    ];
    let (edited, cursor) = delete_backward(&content, &cursor_at2(0, 2, CursorAffinity::Trailing));
    assert_eq!(text_of(&edited), "abb", "backspace at the run seam deletes run 0's last char");
    assert_eq!(cursor.cluster_id.source_run, 0);
}

#[test]
fn insert_then_relayout_widens_by_inserted_advance() {
    // Editing + re-layout: inserting "b" (12px) at byte 1 widens max-content by 12px.
    let font_ref = fake_font_ref();
    let content = make_content("aa", &font_ref);
    let before = max_content_width(&content, &font_ref);
    let (edited, _c) = insert_text(&content, &cursor_at(1, CursorAffinity::Leading), "b");
    assert_eq!(text_of(&edited), "aba");
    assert_px(max_content_width(&edited, &font_ref) - before, 12.0);
}

#[test]
fn delete_then_relayout_narrows_by_removed_advance() {
    // Editing + re-layout: deleting one 'a' from "aaa" narrows max-content by 12px.
    let font_ref = fake_font_ref();
    let content = make_content("aaa", &font_ref);
    let before = max_content_width(&content, &font_ref);
    let (edited, _c) = delete_backward(&content, &cursor_at(1, CursorAffinity::Trailing));
    assert_eq!(text_of(&edited), "aa");
    assert_px(before - max_content_width(&edited, &font_ref), 12.0);
}

// ===========================================================================
// Direction-aware caret geometry & one-press-one-position motion
// ===========================================================================

#[test]
fn cursor_rect_rtl_leading_sits_at_glyph_right_edge() {
    // For an RTL cluster the logical-start (Leading) caret edge is the glyph's
    // RIGHT side. In "אבג" the first logical char א is the visually-rightmost
    // glyph (x 22..33), so a Leading caret on it sits at x=33, not the left x=22.
    let l = layout_rtl("אבג");
    let rect = l.get_cursor_rect(&cursor_at(0, CursorAffinity::Leading)).expect("rect");
    assert_px(rect.origin.x, 33.0);
    // Its Trailing (logical-end) edge is the mirror: the glyph's LEFT side.
    let trail = l.get_cursor_rect(&cursor_at(0, CursorAffinity::Trailing)).expect("rect");
    assert_px(trail.origin.x, 22.0);
}

#[test]
fn multiline_rtl_selection_fills_lines_in_reading_order() {
    // A multi-line RTL selection must fill the START line LEFTWARD from the start
    // cursor to the line's left edge, and the END line from the line's right edge
    // to the end cursor. The old LTR assumption collapsed the start-line rect to
    // zero width. "אבג אבג" @40px wraps to two words, one per line (each x 0..33).
    let font_ref = fake_font_ref();
    let content = make_content("אבג אבג", &font_ref);
    let l = layout_content(
        &content,
        &font_ref,
        &UnifiedConstraints {
            available_width: AvailableSpace::Definite(40.0),
            direction: Some(BidiDirection::Rtl),
            ..UnifiedConstraints::default()
        },
    );
    let rects = l.get_selection_rects(&SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(11, CursorAffinity::Trailing),
    });
    assert_eq!(rects.len(), 2, "adjacent 2-line selection => start-line + end-line rect");
    // Start line: filled from the start cursor (right edge, x=33) leftward to x=0.
    assert_px(rects[0].origin.x, 0.0);
    assert_px(rects[0].size.width, 33.0);
    // End line: filled to the end cursor (left edge, x=0) — a full word too.
    assert_px(rects[1].origin.x, 0.0);
    assert_px(rects[1].size.width, 33.0);
    assert!((rects[0].origin.y - rects[1].origin.y).abs() > 1.0, "the two rects span two lines");
}

#[test]
fn caret_right_reaches_document_end_and_left_returns_to_start() {
    // Each Left/Right press moves exactly one grapheme stop, and BOTH the document
    // end (after the last glyph = last cluster Trailing) and start (before the
    // first = first cluster Leading) are reachable — previously Right could never
    // reach the end nor Left the start, and a press could be silently swallowed.
    let l = layout("abc", AvailableSpace::MaxContent);
    let start = cursor_at(0, CursorAffinity::Leading);
    let mut c = start;
    let forward: Vec<(u32, CursorAffinity)> = (0..3)
        .map(|_| {
            c = l.move_cursor_right(c, &mut None);
            (c.cluster_id.start_byte_in_run, c.affinity)
        })
        .collect();
    assert_eq!(
        forward,
        vec![
            (1, CursorAffinity::Leading),
            (2, CursorAffinity::Leading),
            (2, CursorAffinity::Trailing),
        ],
        "three Rights reach byte1, byte2, then the document end (byte2 Trailing)"
    );
    assert_eq!(l.move_cursor_right(c, &mut None), c, "Right at the document end is a no-op");
    let backward: Vec<(u32, CursorAffinity)> = (0..3)
        .map(|_| {
            c = l.move_cursor_left(c, &mut None);
            (c.cluster_id.start_byte_in_run, c.affinity)
        })
        .collect();
    assert_eq!(
        backward,
        vec![
            (2, CursorAffinity::Leading),
            (1, CursorAffinity::Leading),
            (0, CursorAffinity::Leading),
        ],
        "three Lefts return through byte2, byte1, to the document start (byte0 Leading)"
    );
    assert_eq!(c, start, "walking right to the end then left returns to the start");
    assert_eq!(l.move_cursor_left(c, &mut None), c, "Left at the document start is a no-op");
}

#[test]
fn multi_cursor_insert_remaps_all_later_indices() {
    // Multi-cursor edit: inserting "X" at bytes 1 and 3 of "aaaa" shifts each later
    // cursor by the cumulative inserted length. Result "aXaaXa"; cursors at 2 and 5.
    let content = make_content("aaaa", &fake_font_ref());
    let selections = vec![
        Selection::Cursor(cursor_at(1, CursorAffinity::Leading)),
        Selection::Cursor(cursor_at(3, CursorAffinity::Leading)),
    ];
    let (edited, new_sel) = edit_text(&content, &selections, &TextEdit::Insert("X".to_string()));
    assert_eq!(text_of(&edited), "aXaaXa");
    let bytes: Vec<u32> = new_sel
        .iter()
        .map(|s| match s {
            Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
            Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
        })
        .collect();
    assert_eq!(bytes, vec![2, 5]);
}
