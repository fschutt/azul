#![cfg(feature = "text_layout")]
//! EXACT cursor-movement tests for the text3 engine.
//!
//! Font: built-in "Azul Mock Mono" — every glyph is exactly 10px at 20px, line
//! box 20px tall. Cursor index (source_run, start_byte_in_run, affinity) and
//! the resulting `get_cursor_rect` x/y are asserted at each step.
//!
//! "foo bar baz" byte layout (all 1-byte chars, each 10px):
//!   f0 o1 o2 sp3 b4 a5 r6 sp7 b8 a9 z10  (x = byte*10)

use std::collections::HashMap;
use std::sync::Arc;

use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LoadedFonts,
    OverflowInfo, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints, UnifiedLayout,
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

const FONT_SIZE: f32 = 20.0;

fn assert_px(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.05,
        "{what}: expected {expected:.4}px, got {actual:.4}px"
    );
}

fn mono_ref() -> FontRef {
    let bytes = azul_layout::text3::mock_fonts::MOCK_MONO_TTF;
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.to_vec())));
    let mut warnings = Vec::new();
    let parsed = ParsedFont::from_bytes(bytes, 0, &mut warnings)
        .expect("mock mono must parse")
        .with_source_bytes(arc);
    parsed_font_to_font_ref(parsed)
}

fn layout_text(text: &str, width: AvailableSpace) -> UnifiedLayout {
    let font_ref = mono_ref();
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let logical = create_logical_items(&content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout { items: Vec::new(), overflow: OverflowInfo::default() };
    }
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    let constraints = UnifiedConstraints { available_width: width, ..UnifiedConstraints::default() };
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

fn cur(byte: u32, aff: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: byte },
        affinity: aff,
    }
}

fn byte_of(c: &TextCursor) -> u32 {
    c.cluster_id.start_byte_in_run
}

// ===========================================================================
// Diagnostic
// ===========================================================================

#[test]
fn dump_cursor_walk_right() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let mut c = cur(0, CursorAffinity::Leading);
    for step in 0..14 {
        let rect = l.get_cursor_rect(&c);
        let x = rect.map(|r| r.origin.x);
        println!("step {step}: byte={} aff={:?} caret_x={:?}", byte_of(&c), c.affinity, x);
        c = l.move_cursor_right(c, &mut None);
    }
}

// ===========================================================================
// C1. Left / right across the string
// ===========================================================================

#[test]
fn move_right_advances_one_grapheme() {
    // From the very start, one right press must land at byte 1 (or byte0/Trailing).
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let start = cur(0, CursorAffinity::Leading);
    let one = l.move_cursor_right(start, &mut None);
    // Caret must have moved forward by exactly one 10px cell.
    let x0 = l.get_cursor_rect(&start).unwrap().origin.x;
    let x1 = l.get_cursor_rect(&one).unwrap().origin.x;
    assert_px(x0, 0.0, "start caret x");
    assert_px(x1, 10.0, "after one right, caret x = 10");
}

#[test]
fn move_right_then_left_returns() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let start = cur(2, CursorAffinity::Leading);
    let moved = l.move_cursor_right(start, &mut None);
    let back = l.move_cursor_left(moved, &mut None);
    let xs = l.get_cursor_rect(&start).unwrap().origin.x;
    let xb = l.get_cursor_rect(&back).unwrap().origin.x;
    assert_px(xb, xs, "left after right returns to the same caret x");
}

#[test]
fn cannot_move_left_past_start() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let start = cur(0, CursorAffinity::Leading);
    let moved = l.move_cursor_left(start, &mut None);
    assert_px(
        l.get_cursor_rect(&moved).unwrap().origin.x,
        0.0,
        "left at start stays at x=0",
    );
}

#[test]
fn cannot_move_right_past_end() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    // Walk to the end.
    let mut c = cur(0, CursorAffinity::Leading);
    for _ in 0..40 {
        c = l.move_cursor_right(c, &mut None);
    }
    let end_x = l.get_cursor_rect(&c).unwrap().origin.x;
    assert_px(end_x, 110.0, "caret pinned at end x=110 (11 chars)");
    // Pressing right again must not move further.
    let again = l.move_cursor_right(c, &mut None);
    assert_px(l.get_cursor_rect(&again).unwrap().origin.x, 110.0, "right at end stays");
}

// ===========================================================================
// C2. Word jumps ("foo bar baz")
// ===========================================================================

#[test]
fn next_word_lands_on_word_starts() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let c0 = cur(0, CursorAffinity::Leading);
    let w1 = l.move_cursor_to_next_word(c0, &mut None);
    let w2 = l.move_cursor_to_next_word(w1, &mut None);
    // Word starts are byte 4 ("bar") and byte 8 ("baz").
    assert_eq!(byte_of(&w1), 4, "first next-word => start of 'bar' (byte 4)");
    assert_eq!(byte_of(&w2), 8, "second next-word => start of 'baz' (byte 8)");
}

#[test]
fn prev_word_lands_on_word_starts() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let end = cur(10, CursorAffinity::Trailing);
    let p1 = l.move_cursor_to_prev_word(end, &mut None);
    let p2 = l.move_cursor_to_prev_word(p1, &mut None);
    let p3 = l.move_cursor_to_prev_word(p2, &mut None);
    assert_eq!(byte_of(&p1), 8, "prev-word from end => start of 'baz' (byte 8)");
    assert_eq!(byte_of(&p2), 4, "prev-word => start of 'bar' (byte 4)");
    assert_eq!(byte_of(&p3), 0, "prev-word => start of 'foo' (byte 0)");
}

// ===========================================================================
// C3. Home / End
// ===========================================================================

#[test]
fn home_end_on_single_line() {
    let l = layout_text("foo bar baz", AvailableSpace::MaxContent);
    let mid = cur(5, CursorAffinity::Leading);
    let home = l.move_cursor_to_line_start(mid, &mut None);
    let end = l.move_cursor_to_line_end(mid, &mut None);
    assert_px(l.get_cursor_rect(&home).unwrap().origin.x, 0.0, "home => x=0");
    assert_px(l.get_cursor_rect(&end).unwrap().origin.x, 110.0, "end => x=110");
}

// ===========================================================================
// C4. Up / down between wrapped lines (column preserved)
// ===========================================================================

#[test]
fn down_preserves_column_between_wrapped_lines() {
    // "foobar foobar" @ 60px wraps: line0 "foobar"(x0..60), line1 "foobar".
    // Place caret at byte2 (x=20) on line0, press down -> line1 same column x=20.
    let l = layout_text("foobar foobar", AvailableSpace::Definite(60.0));
    let start = cur(2, CursorAffinity::Leading);
    let x_start = l.get_cursor_rect(&start).unwrap().origin.x;
    assert_px(x_start, 20.0, "start caret x on line0");
    let mut goal = None;
    let down = l.move_cursor_down(start, &mut goal, &mut None);
    let rect = l.get_cursor_rect(&down).expect("down cursor rect");
    assert_px(rect.origin.x, 20.0, "down preserves column x=20");
    assert!(rect.origin.y > 15.0, "down moved to the next line (y>15), got y={}", rect.origin.y);
    // The caret at column x=20 on line1 may be represented either as byte9/Leading
    // or byte8/Trailing (both render at x=20). The engine picks byte8/Trailing.
    // Either is a valid caret at the same visual column; assert it lands in line1's
    // word (bytes 7..12) at column 20, not a specific interior byte.
    assert!(
        (8..=9).contains(&byte_of(&down)),
        "down must land in line1 at column 20 (byte 8 or 9), got byte {}",
        byte_of(&down)
    );
}

#[test]
fn up_from_second_line_returns_to_first() {
    let l = layout_text("foobar foobar", AvailableSpace::Definite(60.0));
    let start = cur(2, CursorAffinity::Leading);
    let mut goal = None;
    let down = l.move_cursor_down(start, &mut goal, &mut None);
    let mut goal2 = None;
    let up = l.move_cursor_up(down, &mut goal2, &mut None);
    let rect = l.get_cursor_rect(&up).expect("up cursor rect");
    assert_px(rect.origin.x, 20.0, "up preserves column x=20");
    assert_px(rect.origin.y, 0.0, "up returns to line 0 (y=0)");
}

#[test]
fn cannot_move_up_past_first_line() {
    let l = layout_text("foobar foobar", AvailableSpace::Definite(60.0));
    let start = cur(2, CursorAffinity::Leading);
    let mut goal = None;
    let up = l.move_cursor_up(start, &mut goal, &mut None);
    assert_eq!(byte_of(&up), 2, "up on the first line stays put");
}

// ===========================================================================
// C5. Grapheme cluster: cursor must skip the whole cluster (never mid-grapheme)
// ===========================================================================

#[test]
fn cursor_skips_whole_grapheme_cluster() {
    // "a" + U+0301 combining acute + "b": moving right from the start must jump
    // over the whole "a\u{0301}" grapheme (byte 0 -> byte 3, the 'b'), never
    // landing on the interior mark byte 1.
    let l = layout_text("a\u{0301}b", AvailableSpace::MaxContent);
    let start = cur(0, CursorAffinity::Leading);
    let r1 = l.move_cursor_right(start, &mut None);
    // After crossing the grapheme, the cursor must be at byte 0 Trailing or byte 3,
    // but NEVER at byte 1 (mid-grapheme).
    assert_ne!(byte_of(&r1), 1, "cursor must never land on the combining-mark byte 1");
    let r2 = l.move_cursor_right(r1, &mut None);
    assert_ne!(byte_of(&r2), 1, "cursor must never land mid-grapheme");
}
