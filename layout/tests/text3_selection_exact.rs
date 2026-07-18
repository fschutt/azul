#![cfg(feature = "text_layout")]
//! EXACT selection-rectangle tests for the text3 engine, including the
//! MULTI-NODE (multi-span) case the user explicitly asked for.
//!
//! Font: built-in "Azul Mock Mono" — every glyph advances 0.5em, so at
//! font-size 20px every character is EXACTLY 10px wide and the line box is
//! ascent+descent = 20px tall. All coordinates below are exact arithmetic.
//!
//! Multi-run content models sibling <span>s: each `InlineContent::Text` is a
//! separate run, so its clusters get a distinct `source_run` == the content
//! index. A cursor into span N is `cursor(run=N, byte, affinity)`.

use std::collections::HashMap;
use std::sync::Arc;

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};
use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LoadedFonts,
    OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout,
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

const FONT_SIZE: f32 = 20.0;

fn assert_px(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.05,
        "{what}: expected {expected:.4}px, got {actual:.4}px"
    );
}

fn mono_parsed() -> ParsedFont {
    let bytes = azul_layout::text3::mock_fonts::MOCK_MONO_TTF;
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.to_vec())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(bytes, 0, &mut warnings)
        .expect("mock mono must parse")
        .with_source_bytes(arc)
}

fn mono_ref() -> FontRef {
    parsed_font_to_font_ref(mono_parsed())
}

fn style_for(font_ref: &FontRef) -> Arc<StyleProperties> {
    Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    })
}

/// Build multi-run content from `texts`: each &str is one sibling span.
fn spans(texts: &[&str], font_ref: &FontRef) -> Vec<InlineContent> {
    let style = style_for(font_ref);
    let mut byte = 0usize;
    texts
        .iter()
        .map(|t| {
            let run = InlineContent::Text(StyledRun {
                text: (*t).to_string(),
                style: Arc::clone(&style),
                logical_start_byte: byte,
                source_node_id: None,
            });
            byte += t.len();
            run
        })
        .collect()
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

fn layout(texts: &[&str], width: AvailableSpace) -> (UnifiedLayout, FontRef) {
    let font_ref = mono_ref();
    let content = spans(texts, &font_ref);
    let l = layout_content(
        &content,
        &font_ref,
        &UnifiedConstraints {
            available_width: width,
            ..UnifiedConstraints::default()
        },
    );
    (l, font_ref)
}

fn cur(run: u32, byte: u32, aff: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: run,
            start_byte_in_run: byte,
        },
        affinity: aff,
    }
}

fn range(start: TextCursor, end: TextCursor) -> SelectionRange {
    SelectionRange { start, end }
}

// ===========================================================================
// Diagnostic: dump the positioned clusters so expectations are grounded.
// ===========================================================================

#[test]
fn dump_multinode_positions() {
    let (l, _) = layout(&["HELLO", "WORLD"], AvailableSpace::MaxContent);
    for it in &l.items {
        if let ShapedItem::Cluster(c) = &it.item {
            println!(
                "run={} byte={} text={:?} x={} y={} adv={} line={}",
                c.source_cluster_id.source_run,
                c.source_cluster_id.start_byte_in_run,
                c.text,
                it.position.x,
                it.position.y,
                c.advance,
                it.line_index,
            );
        }
    }
}

// ===========================================================================
// B1. Single-run selection
// ===========================================================================

#[test]
fn single_run_select_chars_2_to_5() {
    // "HELLOWORLD" as one run, select chars [2,5): bytes 2,3,4 => L,L,O.
    // x = [20,50], width 30, height 20.
    let (l, _) = layout(&["HELLOWORLD"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(0, 2, CursorAffinity::Leading),
        cur(0, 4, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 1, "single-line selection => 1 rect");
    assert_px(rects[0].origin.x, 20.0, "rect x");
    assert_px(rects[0].size.width, 30.0, "rect width (3 chars)");
    assert_px(rects[0].size.height, 20.0, "rect height");
}

// ===========================================================================
// B2. Multi-node selection across sibling spans (same line)
// ===========================================================================

#[test]
fn multinode_two_spans_same_line_is_one_rect() {
    // Spans "HELLO"(run0, x0..50) + "WORLD"(run1, x50..100), contiguous.
    // Select from run0 byte2 (x20) through run1 byte1 (W,O => x50..70).
    // Visually contiguous on one line => 1 rect x=[20,70] width 50.
    let (l, _) = layout(&["HELLO", "WORLD"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(0, 2, CursorAffinity::Leading),
        cur(1, 1, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 1, "cross-span same-line selection => 1 rect, got {rects:?}");
    assert_px(rects[0].origin.x, 20.0, "rect x");
    assert_px(rects[0].size.width, 50.0, "rect width (3 in span0 + 2 in span1)");
}

#[test]
fn multinode_three_spans_full_union() {
    // "AB"(run0 x0..20) "CD"(run1 x20..40) "EF"(run2 x40..60).
    // Select from run0 byte0 to run2 byte1 (all of it): 1 rect x0..60 width 60.
    let (l, _) = layout(&["AB", "CD", "EF"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(0, 0, CursorAffinity::Leading),
        cur(2, 1, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 1, "3-span same-line selection => 1 rect, got {rects:?}");
    assert_px(rects[0].origin.x, 0.0, "rect x");
    assert_px(rects[0].size.width, 60.0, "rect width (all 6 chars)");
}

#[test]
fn multinode_middle_span_only() {
    // Select exactly the middle span "CD" (run1, bytes 0..1) => x20..40, width 20.
    let (l, _) = layout(&["AB", "CD", "EF"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(1, 0, CursorAffinity::Leading),
        cur(1, 1, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 1);
    assert_px(rects[0].origin.x, 20.0, "middle-span rect x");
    assert_px(rects[0].size.width, 20.0, "middle-span rect width");
}

// ===========================================================================
// B3. Selection spanning a wrapped line break
// ===========================================================================

#[test]
fn selection_across_wrap_is_two_rects() {
    // "HELLO WORLD" as one run @width 60px: "HELLO"(50px) fits, "WORLD" wraps.
    // Select bytes 2..8 (spanning the wrap): 2 rects, one per line.
    // Line 0 tail: chars L,L,O => x20..50. Line 1 head: W,O => x0..20.
    let (l, _) = layout(&["HELLO WORLD"], AvailableSpace::Definite(60.0));
    let rects = l.get_selection_rects(&range(
        cur(0, 2, CursorAffinity::Leading),
        cur(0, 7, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 2, "selection across a wrap => 2 rects, got {rects:?}");
    // Rects order is line-order; first line rect starts at x=20, second at x=0.
    let mut xs: Vec<f32> = rects.iter().map(|r| r.origin.x).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_px(xs[0], 0.0, "second-line rect starts at x=0");
    assert_px(xs[1], 20.0, "first-line rect starts at x=20");
}

// ===========================================================================
// B4. Brutal edge cases
// ===========================================================================

#[test]
fn empty_selection_is_zero_width_caret() {
    // CONTRACT (verified): a collapsed range (start==end) returns a single
    // ZERO-WIDTH rect (a caret) at the cursor x, full line height — NOT 0 rects
    // and not a highlighted region. This paints nothing visible, so it is a
    // sane (if unusual) contract; we pin it exactly.
    let (l, _) = layout(&["HELLOWORLD"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(0, 3, CursorAffinity::Leading),
        cur(0, 3, CursorAffinity::Leading),
    ));
    assert_eq!(rects.len(), 1, "collapsed selection => 1 zero-width caret rect, got {rects:?}");
    assert_px(rects[0].size.width, 0.0, "collapsed selection rect must be zero-width");
    assert_px(rects[0].origin.x, 30.0, "caret sits at byte-3 leading edge x=30");
    assert_px(rects[0].size.height, 20.0, "caret spans full line height");
}

#[test]
fn select_all_single_rect_full_width() {
    let (l, _) = layout(&["HELLOWORLD"], AvailableSpace::MaxContent);
    let rects = l.get_selection_rects(&range(
        cur(0, 0, CursorAffinity::Leading),
        cur(0, 9, CursorAffinity::Trailing),
    ));
    assert_eq!(rects.len(), 1);
    assert_px(rects[0].origin.x, 0.0, "select-all x");
    assert_px(rects[0].size.width, 100.0, "select-all width (10 chars)");
}

#[test]
fn reversed_range_selects_same_span() {
    // end < start: a well-behaved API normalizes and selects the same region.
    let (l, _) = layout(&["HELLOWORLD"], AvailableSpace::MaxContent);
    let forward = l.get_selection_rects(&range(
        cur(0, 2, CursorAffinity::Leading),
        cur(0, 5, CursorAffinity::Trailing),
    ));
    let reversed = l.get_selection_rects(&range(
        cur(0, 5, CursorAffinity::Trailing),
        cur(0, 2, CursorAffinity::Leading),
    ));
    assert_eq!(
        forward.len(),
        reversed.len(),
        "reversed range must select the same number of rects"
    );
    if !forward.is_empty() && !reversed.is_empty() {
        assert_px(reversed[0].origin.x, forward[0].origin.x, "reversed rect x matches");
        assert_px(reversed[0].size.width, forward[0].size.width, "reversed rect width matches");
    }
}

// ===========================================================================
// B5. RTL selection (Arabic mock)
// ===========================================================================

#[test]
fn rtl_selection_maps_to_right_side() {
    // A pure-RTL run: the visual rect for a logical selection must sit on the
    // right. We assert the rect's right edge equals the run's total width, i.e.
    // selecting the FIRST logical chars covers the RIGHTMOST visual region.
    // Uses the Arabic mock via its own font (loaded on disk).
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fonts/azul-mock-arabic.ttf"
    ))
    .expect("read arabic mock");
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    let parsed = ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("arabic parse")
        .with_source_bytes(arc);
    let font_ref = parsed_font_to_font_ref(parsed);

    // "beh teh meem" (U+0628 U+062A U+0645): 3 chars, each 500u => 10px, total 30px.
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: "\u{0628}\u{062A}\u{0645}".to_string(),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let l = layout_content(
        &content,
        &font_ref,
        &UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            direction: Some(BidiDirection::Rtl),
            ..UnifiedConstraints::default()
        },
    );

    // Select the FIRST logical char (beh, byte 0). In RTL it must render at the
    // RIGHT (x near 20..30, not 0..10).
    let rects = l.get_selection_rects(&range(
        cur(0, 0, CursorAffinity::Leading),
        cur(0, 0, CursorAffinity::Trailing),
    ));
    assert!(!rects.is_empty(), "RTL first-char selection must produce a rect");
    let right_edge = rects[0].origin.x + rects[0].size.width;
    assert!(
        right_edge >= 25.0,
        "RTL: first logical char must sit at the RIGHT (right edge {right_edge}, expected ~30)"
    );
}
