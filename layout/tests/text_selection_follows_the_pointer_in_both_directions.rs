#![cfg(feature = "text_layout")]
//! Text selection follows the pointer in left-to-right, right-to-left and
//! mixed text.
//!
//! A caret stands on one EDGE of a grapheme: its logical start (`Leading`)
//! or end (`Trailing`). In left-to-right text the start is the glyph's left
//! edge, in right-to-left text its right edge. A click resolves to the edge
//! nearest the pointer, so the caret is drawn where the click was and a drag
//! highlights up to the pointer, in Arabic as in English. The hit test took
//! the left half of every glyph for its start: in Arabic the caret landed
//! on the far side of the letter, and a drag's highlight jumped a whole
//! letter back and forth as the pointer crossed each one.
//!
//! Where a right-to-left word meets left-to-right text, one point on screen
//! is two places in the text: the end of the Latin run and the end of the
//! Arabic word are both drawn at the edge between them. A drag reaching that
//! edge takes the place in the run it started in, as Chrome does, so the
//! highlight ends at the pointer instead of covering the whole Arabic word.
//! Inside a reversed word the highlight is the LOGICAL range (Chrome, every
//! native text view); its moving end still stands under the pointer.
//!
//! Layout runs through the real text3 stages on a synthetic font: at
//! font-size 20 a Latin letter is 12 px, a space 5 px and an Arabic letter
//! 10 px wide, one line is 20 px tall.

use std::{collections::HashMap, sync::Arc};

use azul_core::{
    geom::LogicalPosition,
    selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor},
};
use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::cache::{
        create_logical_items, perform_fragment_layout, split_into_bidi_runs, shape_bidi_runs,
        AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent,
        LoadedFonts, OverflowInfo, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
        UnifiedLayout,
    },
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

use crate::fakefont::{FakeFontBuilder, FakeGlyph};

const FONT_SIZE: f32 = 20.0;
const LATIN: f32 = 12.0;
const ARABIC: f32 = 10.0;
/// The middle of the one line every text here is laid out on.
const LINE_Y: f32 = 10.0;

/// Three Arabic letters, logical order beh, teh, theh (two bytes each).
const ARABIC_WORD: &str = "\u{0628}\u{062A}\u{062B}";

fn font() -> FontRef {
    let mut b = FakeFontBuilder::new("BidiSelectionTest", 1000).metrics(800, -200, 0);
    for c in 'a'..='z' {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 600,
                lsb: 50,
                bbox: Some((50, 0, 550, 700)),
                instructions: Vec::new(),
            },
        );
    }
    b.add_glyph(
        Some(' '),
        FakeGlyph {
            advance: 250,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );
    for c in ARABIC_WORD.chars() {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 500,
                lsb: 50,
                bbox: Some((50, 0, 450, 600)),
                instructions: Vec::new(),
            },
        );
    }
    let bytes = b.build();
    let mut warnings = Vec::new();
    let parsed = ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("the synthetic font parses")
        .with_source_bytes(Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice()))));
    parsed_font_to_font_ref(parsed)
}

/// `text` on one line in a left-to-right paragraph.
fn layout(text: &str) -> UnifiedLayout {
    let font_ref = font();
    let style = StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    };
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(style),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        ..UnifiedConstraints::default()
    };
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let logical = create_logical_items(&content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout {
            items: Vec::new(),
            overflow: OverflowInfo::default(),
        };
    }
    let visual = split_into_bidi_runs(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_bidi_runs(&visual, &chain, &FcFontCache::default(), &loaded, &mut None)
        .expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

fn at(x: f32) -> LogicalPosition {
    LogicalPosition::new(x, LINE_Y)
}

fn caret(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// Where the caret `cursor` is drawn.
fn caret_x(l: &UnifiedLayout, cursor: &TextCursor) -> f32 {
    l.get_cursor_rect(cursor)
        .expect("the caret has a rect")
        .origin
        .x
}

/// The x spans the selection from `anchor` to `focus` highlights, sorted,
/// with spans that touch joined: what the eye sees, whichever direction each
/// piece of the highlight was painted for.
fn highlight(l: &UnifiedLayout, anchor: TextCursor, focus: TextCursor) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = l
        .get_selection_rects(&SelectionRange {
            start: anchor,
            end: focus,
        })
        .iter()
        .filter(|r| r.size.width > 0.5)
        .map(|r| (r.origin.x, r.origin.x + r.size.width))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut joined: Vec<(f32, f32)> = Vec::new();
    for (lo, hi) in spans {
        match joined.last_mut() {
            Some(last) if lo <= last.1 + 0.5 => last.1 = last.1.max(hi),
            _ => joined.push((lo, hi)),
        }
    }
    joined
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// The caret a drag from `anchor` reaches with the pointer at `x`.
fn drag_to(l: &UnifiedLayout, anchor: &TextCursor, x: f32) -> TextCursor {
    l.hittest_selection_focus(anchor, at(x))
        .expect("the drag resolves to a caret")
}

// ---------------------------------------------------------------------------
// Left to right: the reference behaviour.
// ---------------------------------------------------------------------------

#[test]
fn a_click_on_the_left_half_of_a_latin_letter_puts_the_caret_before_it() {
    let l = layout("abcd");
    let c = l.hittest_cursor(at(LATIN + 2.0)).expect("a caret");
    assert_eq!(c, caret(1, CursorAffinity::Leading));
    assert!(near(caret_x(&l, &c), LATIN));
}

#[test]
fn a_drag_across_latin_text_highlights_from_the_press_to_the_pointer() {
    let l = layout("abcd");
    let anchor = l.hittest_cursor(at(1.0)).expect("a caret");
    let focus = drag_to(&l, &anchor, 3.0 * LATIN - 2.0);
    assert_eq!(highlight(&l, anchor, focus), vec![(0.0, 3.0 * LATIN)]);
}

// ---------------------------------------------------------------------------
// Right to left: an Arabic word, drawn theh, teh, beh from the left.
// ---------------------------------------------------------------------------

#[test]
fn a_click_on_the_right_half_of_an_arabic_letter_puts_the_caret_before_it() {
    let l = layout(ARABIC_WORD);
    // beh, the first letter, is drawn rightmost: x 20..30.
    let c = l.hittest_cursor(at(28.0)).expect("a caret");
    assert_eq!(
        c,
        caret(0, CursorAffinity::Leading),
        "the right half of a right-to-left letter is its start"
    );
    assert!(near(caret_x(&l, &c), 30.0), "the caret is drawn at the right edge, where the click was");
}

#[test]
fn a_click_on_the_left_half_of_an_arabic_letter_puts_the_caret_after_it() {
    let l = layout(ARABIC_WORD);
    let c = l.hittest_cursor(at(22.0)).expect("a caret");
    assert_eq!(
        c,
        caret(0, CursorAffinity::Trailing),
        "the left half of a right-to-left letter is its end"
    );
    assert!(near(caret_x(&l, &c), 20.0), "the caret is drawn at the left edge, where the click was");
}

#[test]
fn a_click_left_of_an_arabic_line_puts_the_caret_at_its_logical_end() {
    let l = layout(ARABIC_WORD);
    let c = l.hittest_cursor(at(-40.0)).expect("a caret");
    assert_eq!(c, caret(4, CursorAffinity::Trailing), "after theh, the last letter");
}

#[test]
fn a_drag_through_an_arabic_word_highlights_from_the_press_to_the_pointer_letter_by_letter() {
    let l = layout(ARABIC_WORD);
    // Pressed at the word's right edge, its logical start.
    let anchor = l.hittest_cursor(at(29.0)).expect("a caret");
    assert!(near(caret_x(&l, &anchor), 30.0), "premise: the press is at the right edge");
    // The pointer moves left, the way Arabic reads. Over every letter the
    // highlight reaches the letter edge nearest the pointer, never a whole
    // letter short of it or past it.
    for x in [27.0, 24.0, 19.0, 16.0, 14.0, 11.0, 7.0, 3.0, 1.0] {
        let focus = drag_to(&l, &anchor, x);
        let nearest_edge = (x / ARABIC).round() * ARABIC;
        let spans = highlight(&l, anchor, focus);
        if near(nearest_edge, 30.0) {
            assert!(spans.is_empty(), "pointer at {x}: nothing selected yet, got {spans:?}");
            continue;
        }
        assert_eq!(
            spans,
            vec![(nearest_edge, 30.0)],
            "pointer at {x}: the highlight runs from the pointer's nearest edge to the press"
        );
        assert!(near(caret_x(&l, &focus), nearest_edge), "pointer at {x}: the moving end is under it");
    }
}

// ---------------------------------------------------------------------------
// Mixed: "ab " + the Arabic word + " cd". Logical bytes: a0 b1 space2 beh3
// teh5 theh7 space9 c10 d11. Drawn: a 0..12, b 12..24, space 24..29,
// theh 29..39, teh 39..49, beh 49..59, space 59..64, c 64..76, d 76..88.
// ---------------------------------------------------------------------------

fn mixed() -> UnifiedLayout {
    layout(&format!("ab {ARABIC_WORD} cd"))
}

#[test]
fn every_click_in_mixed_text_draws_its_caret_where_the_click_was() {
    let l = mixed();
    let mut x = 0.5;
    while x < 88.0 {
        let c = l.hittest_cursor(at(x)).expect("a caret");
        let drawn = caret_x(&l, &c);
        assert!(
            (drawn - x).abs() <= LATIN / 2.0 + 0.01,
            "a click at {x} drew its caret at {drawn} ({c:?})"
        );
        x += 1.0;
    }
}

#[test]
fn a_drag_from_latin_text_to_the_near_edge_of_an_arabic_word_stops_at_the_edge() {
    let l = mixed();
    let anchor = l.hittest_cursor(at(1.0)).expect("a caret");
    // Just inside the Arabic word's left edge: the edge is both the end of
    // "ab " and the end of the Arabic word.
    let focus = drag_to(&l, &anchor, 30.0);
    assert_eq!(
        highlight(&l, anchor, focus),
        vec![(0.0, 29.0)],
        "the highlight ends at the pointer, it does not take the Arabic word"
    );
    assert_eq!(focus, caret(2, CursorAffinity::Trailing), "after the space");
}

#[test]
fn a_drag_from_latin_text_to_the_far_edge_of_an_arabic_word_takes_the_whole_word() {
    let l = mixed();
    let anchor = l.hittest_cursor(at(1.0)).expect("a caret");
    // Just inside the Arabic word's right edge: both the start of the
    // Arabic word and the start of " cd".
    let focus = drag_to(&l, &anchor, 58.0);
    assert_eq!(
        highlight(&l, anchor, focus),
        vec![(0.0, 59.0)],
        "the highlight reaches the pointer and holds the whole Arabic word"
    );
    assert_eq!(focus, caret(9, CursorAffinity::Leading), "before the second space");
}

#[test]
fn a_drag_from_the_end_back_to_the_far_edge_of_an_arabic_word_stops_at_the_edge() {
    let l = mixed();
    let anchor = l.hittest_cursor(at(87.0)).expect("a caret");
    let focus = drag_to(&l, &anchor, 58.0);
    assert_eq!(
        highlight(&l, anchor, focus),
        vec![(59.0, 88.0)],
        "a backward drag ends at the pointer too"
    );
}

#[test]
fn a_drag_inside_an_arabic_word_keeps_its_moving_end_under_the_pointer() {
    let l = mixed();
    let anchor = l.hittest_cursor(at(1.0)).expect("a caret");
    for x in [33.0, 37.0, 42.0, 47.0, 52.0, 56.0] {
        let focus = drag_to(&l, &anchor, x);
        let drawn = caret_x(&l, &focus);
        assert!(
            (drawn - x).abs() <= ARABIC / 2.0 + 0.01,
            "pointer at {x}: the moving end is drawn at {drawn}"
        );
    }
}

#[test]
fn a_drag_starting_inside_an_arabic_word_crosses_into_latin_text_at_the_pointer() {
    let l = mixed();
    // Pressed between teh and theh (x 39): after teh, logically.
    let anchor = l.hittest_cursor(at(40.0)).expect("a caret");
    assert!(near(caret_x(&l, &anchor), 39.0), "premise: the press is at x 39");
    // To the edge where the Arabic word meets the second space: the start of
    // the word, from inside it.
    let focus = drag_to(&l, &anchor, 60.0);
    assert_eq!(
        highlight(&l, anchor, focus),
        vec![(39.0, 59.0)],
        "beh and teh, the letters between the press and the pointer"
    );
}
