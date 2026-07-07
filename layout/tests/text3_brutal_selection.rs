#![cfg(feature = "text_layout")]
//! Brutal, deterministic selection / caret / edit tests for the REAL text3
//! pipeline on the synthetic fake font (`common/fakefont.rs`).
//!
//! Layout is produced by the same stage-1..4 calls `window.rs` uses, then the
//! public `UnifiedLayout` hit-testing (`hittest_cursor`, `get_selection_rects`,
//! `get_cursor_rect`), the `text3::selection` word helper, and the pure
//! `text3::edit` operations are exercised.
//!
//! Fake metrics at font-size 20 (scale 0.02): 'a' 600u => 12px, space 250u =>
//! 5px, combining U+0301 => 0px. In "aaaa aaaa aaaa" the clusters land at:
//!   a@0..12 a@12..24 a@24..36 a@36..48 space@48..53 a@53..65 a@65..77 a@77..89
//!   a@89..101 space@101..106 a@106..118 a@118..130 a@130..142 a@142..154

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
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent,
    LoadedFonts, OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi,
    UnifiedConstraints, UnifiedLayout, WhiteSpaceMode,
};
use azul_layout::text3::edit::{delete_backward, edit_text, insert_text, TextEdit};
use azul_layout::text3::selection::select_word_at_cursor;
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

// --- Font + pipeline plumbing ---------------------------------------------

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
        return UnifiedLayout {
            items: Vec::new(),
            overflow: OverflowInfo::default(),
        };
    }
    let base_direction = constraints.direction.unwrap_or(BidiDirection::Ltr);
    let visual = reorder_logical_items(&logical, base_direction, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder must succeed");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None)
        .expect("shaping must succeed");
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, constraints, &mut None, &loaded)
        .expect("fragment layout must succeed")
}

fn layout(text: &str, width: AvailableSpace) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let content = make_content(text, &font_ref);
    let constraints = UnifiedConstraints {
        available_width: width,
        ..UnifiedConstraints::default()
    };
    layout_content(&content, &font_ref, &constraints)
}

fn max_content_width(content: &[InlineContent], font_ref: &FontRef) -> f32 {
    let l = layout_content(
        content,
        font_ref,
        &UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            ..UnifiedConstraints::default()
        },
    );
    let mut mn = f32::MAX;
    let mut mx = f32::MIN;
    for it in &l.items {
        if let ShapedItem::Cluster(c) = &it.item {
            mn = mn.min(it.position.x);
            mx = mx.max(it.position.x + c.advance);
        }
    }
    if mx < mn {
        0.0
    } else {
        mx - mn
    }
}

fn cursor_at(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// The `line_index` of the positioned cluster whose id matches `cursor`.
fn line_of(layout: &UnifiedLayout, cursor: &TextCursor) -> Option<usize> {
    layout.items.iter().find_map(|it| match &it.item {
        ShapedItem::Cluster(c) if c.source_cluster_id == cursor.cluster_id => Some(it.line_index),
        _ => None,
    })
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
// Caret hit-testing (UnifiedLayout::hittest_cursor)
// ===========================================================================

#[test]
fn click_leading_half_places_caret_before_glyph() {
    // Hit-testing: clicking the left half of a glyph yields a Leading cursor.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let c = l
        .hittest_cursor(LogicalPosition::new(3.0, 8.0))
        .expect("caret");
    assert_eq!(c.cluster_id.start_byte_in_run, 0);
    assert_eq!(c.affinity, CursorAffinity::Leading);
}

#[test]
fn click_trailing_half_places_caret_after_glyph() {
    // Hit-testing: clicking the right half of a glyph (x > mid = 6px) yields Trailing.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let c = l
        .hittest_cursor(LogicalPosition::new(10.0, 8.0))
        .expect("caret");
    assert_eq!(c.cluster_id.start_byte_in_run, 0);
    assert_eq!(c.affinity, CursorAffinity::Trailing);
}

#[test]
fn click_past_line_end_snaps_to_line_end() {
    // Hit-testing: a click far past the last glyph snaps to the trailing edge
    // of the final cluster (byte 13, the last 'a').
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let c = l
        .hittest_cursor(LogicalPosition::new(1000.0, 8.0))
        .expect("caret");
    assert_eq!(c.cluster_id.start_byte_in_run, 13);
    assert_eq!(c.affinity, CursorAffinity::Trailing);
}

#[test]
fn click_on_second_line_y_lands_on_second_line() {
    // Hit-testing: vertical proximity dominates, so a click at the line-1 baseline
    // band resolves to a cluster on line index 1.
    // Layout "aaaa aaaa aaaa" @60px => one word per line; line 1 tops at y=20px.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::Definite(60.0));
    let c = l
        .hittest_cursor(LogicalPosition::new(6.0, 30.0))
        .expect("caret");
    assert_eq!(line_of(&l, &c), Some(1), "y=30 must resolve to line index 1");
}

#[test]
fn caret_never_lands_between_base_and_combining_mark() {
    // UAX#29: 'a'+U+0301 is one grapheme cluster; a caret anywhere over it maps to
    // the cluster start (byte 0), never to the interior mark byte (byte 1).
    let l = layout("a\u{0301}a", AvailableSpace::MaxContent);
    for x in [1.0_f32, 5.0, 9.0, 11.0] {
        let c = l
            .hittest_cursor(LogicalPosition::new(x, 8.0))
            .expect("caret");
        assert_eq!(
            c.cluster_id.start_byte_in_run, 0,
            "caret at x={x} must snap to the base, not the combining mark"
        );
    }
}

#[test]
fn empty_line_reserves_a_line_box() {
    // CSS Text §3 / UAX#14: two consecutive LFs create an empty line box between
    // the paragraphs; the second "aa" is therefore pushed to y = 2*line-height.
    // pins spec: blank line between two forced breaks reserves a 20px line box.
    let font_ref = fake_font_ref();
    let l = layout_content(
        &make_content("aa\n\naa", &font_ref),
        &font_ref,
        &UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            white_space_mode: WhiteSpaceMode::Pre,
            ..UnifiedConstraints::default()
        },
    );
    let second_aa_y = l
        .items
        .iter()
        .filter_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.source_cluster_id.start_byte_in_run >= 4 => {
                Some(it.position.y)
            }
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    assert_px(second_aa_y, 40.0);
}

// ===========================================================================
// Selection rectangles (UnifiedLayout::get_selection_rects)
// ===========================================================================

#[test]
fn selection_of_two_glyphs_is_one_rect() {
    // Selecting clusters 2..3 (bytes 1 and 2) yields a single line rect spanning
    // x 12..36 (width 24) at the full line-box height (20px).
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let range = SelectionRange {
        start: cursor_at(1, CursorAffinity::Leading),
        end: cursor_at(2, CursorAffinity::Trailing),
    };
    let rects = l.get_selection_rects(&range);
    assert_eq!(rects.len(), 1, "single-line selection => one rect");
    assert_px(rects[0].origin.x, 12.0);
    assert_px(rects[0].size.width, 24.0);
    assert_px(rects[0].size.height, 20.0);
}

#[test]
fn selection_across_line_break_is_two_rects() {
    // A selection spanning a soft line break produces one rect per line
    // (start line tail + end line head). @60px puts one word per line, so byte 0
    // is on line 0 and byte 5 (start of the 2nd word) is on the adjacent line 1.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::Definite(60.0));
    let range = SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(5, CursorAffinity::Trailing),
    };
    let rects = l.get_selection_rects(&range);
    assert_eq!(rects.len(), 2, "selection across one line break => 2 rects");
}

#[test]
fn selection_including_trailing_space_adds_5px() {
    // Extending a selection over the inter-word space adds exactly the space
    // advance (250u => 5px): "aaaa" is 48px, "aaaa " is 53px.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let word = l.get_selection_rects(&SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(3, CursorAffinity::Trailing),
    });
    let word_and_space = l.get_selection_rects(&SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(4, CursorAffinity::Trailing),
    });
    assert_px(word[0].size.width, 48.0);
    assert_px(word_and_space[0].size.width, 53.0);
}

#[test]
fn bidi_selection_over_rtl_run_splits_into_multiple_rects() {
    // A selection covering a mixed-direction span should produce a visual rect
    // per directional segment (LTR head, RTL middle) — >= 2 rects.
    // known-suspect: get_selection_rects currently emits ONE rect per line with
    // no bidi visual split (cache.rs single-line branch), so this pins the
    // spec-correct behavior the impl does not yet provide.
    let l = layout("aa אב aa", AvailableSpace::MaxContent);
    // Select from the first 'a' (byte 0) through the last Hebrew letter.
    let hebrew_last_byte = "aa ".len() as u32 + 'א'.len_utf8() as u32; // start byte of 'ב'
    let range = SelectionRange {
        start: cursor_at(0, CursorAffinity::Leading),
        end: cursor_at(hebrew_last_byte, CursorAffinity::Trailing),
    };
    let rects = l.get_selection_rects(&range);
    assert!(
        rects.len() >= 2,
        "bidi selection should yield >= 2 visual rects, got {}",
        rects.len()
    );
}

// ===========================================================================
// Word selection (text3::selection::select_word_at_cursor)
// ===========================================================================

#[test]
fn double_click_selects_the_word() {
    // Word selection: a cursor inside the middle word selects exactly that run of
    // word characters (bytes 5..=8), stopping at the surrounding spaces.
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let range = select_word_at_cursor(&cursor_at(6, CursorAffinity::Leading), &l)
        .expect("word selection");
    assert_eq!(range.start.cluster_id.start_byte_in_run, 5);
    assert_eq!(range.end.cluster_id.start_byte_in_run, 8);
}

// ===========================================================================
// Editing (text3::edit) — pure content transforms, then re-layout
// ===========================================================================

#[test]
fn insert_widens_layout_by_inserted_advance() {
    // Editing: inserting "bb" (2 * 12px) at byte 2 must widen the re-laid-out
    // max-content by exactly 24px, and the run's style span is preserved.
    let font_ref = fake_font_ref();
    let content = make_content("aaaa aaaa", &font_ref);
    let before = max_content_width(&content, &font_ref);

    let (edited, _new_cursor) =
        insert_text(&content, &cursor_at(2, CursorAffinity::Leading), "bb");
    assert_eq!(text_of(&edited), "aabbaa aaaa", "insert lands at byte 2");
    // Style span preserved: still a single text run carrying the FontRef.
    assert!(matches!(edited.first(), Some(InlineContent::Text(_))));

    let after = max_content_width(&edited, &font_ref);
    assert_px(after - before, 24.0);
}

#[test]
fn backspace_deletes_whole_grapheme_no_orphan_mark() {
    // UAX#29: Backspace over 'a'+U+0301 removes the entire grapheme cluster,
    // leaving no orphaned combining mark and never panicking.
    let content = make_content("a\u{0301}a", &fake_font_ref());
    let (edited, _cursor) =
        delete_backward(&content, &cursor_at(0, CursorAffinity::Trailing));
    let remaining = text_of(&edited);
    assert_eq!(remaining, "a", "the base+mark grapheme is deleted as a unit");
    assert!(
        !remaining.contains('\u{0301}'),
        "no orphaned combining mark may survive"
    );
}

#[test]
fn multi_cursor_insert_remaps_later_selection_indices() {
    // Editing: a multi-cursor insert must shift every selection at or after each
    // edit point. Inserting "XX" (len 2) at bytes 2 and 8 remaps the cursors to
    // bytes 4 and 12 (the byte-8 cursor gains +2 for its own insert and +2 for
    // the earlier byte-2 insert).
    let content = make_content("aaaa aaaa", &fake_font_ref());
    let selections = vec![
        Selection::Cursor(cursor_at(2, CursorAffinity::Leading)),
        Selection::Cursor(cursor_at(8, CursorAffinity::Leading)),
    ];
    let (edited, new_selections) =
        edit_text(&content, &selections, &TextEdit::Insert("XX".to_string()));
    assert_eq!(text_of(&edited), "aaXXaa aaaXXa");
    assert_eq!(new_selections.len(), 2);
    let bytes: Vec<u32> = new_selections
        .iter()
        .map(|s| match s {
            Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
            Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
        })
        .collect();
    assert_eq!(bytes, vec![4, 12], "later selection index remapped past both inserts");
}

#[test]
fn cursor_rect_is_one_px_wide_at_line_height() {
    // A collapsed caret rect is 1px wide and spans the full line box (20px);
    // a Trailing caret on byte 3 sits at x = 48 (end of the fourth 'a').
    let l = layout("aaaa aaaa aaaa", AvailableSpace::MaxContent);
    let rect: LogicalRect = l
        .get_cursor_rect(&cursor_at(3, CursorAffinity::Trailing))
        .expect("cursor rect");
    assert_px(rect.origin.x, 48.0);
    assert_px(rect.size.width, 1.0);
    assert_px(rect.size.height, 20.0);
}
