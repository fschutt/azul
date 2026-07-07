#![cfg(feature = "text_layout")]
//! Deterministic UAX#14 / CSS-Text line-breaking regression tests on the fake
//! font (`common/fakefont.rs`), driven through the exact stage-1..4 text3
//! pipeline `window.rs::relayout_text_node_internal` uses.
//!
//! Fake metrics @ size 20 (scale 0.02): 'a' 600u => 12px · space 250u => 5px ·
//! '-' 300u => 6px · '/' 300u => 6px · '@' 900u => 18px · CJK 1000u => 20px ·
//! U+200B / U+00AD => 0px. Every expected number is exact arithmetic on these.

#[path = "common/fakefont.rs"]
mod fakefont;

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, Hyphens, InlineContent,
    LineBreakStrictness, LoadedFonts, OverflowInfo, OverflowWrap, ShapedItem, StyleProperties,
    StyledRun, UnicodeBidi, UnifiedConstraints, UnifiedLayout, WhiteSpaceMode, WordBreak,
};
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

/// Per-test knobs that live on the `BreakCursor` (`word_break`/`hyphens`/
/// `line_break`) and on the `UnifiedConstraints` (`white_space_mode`/
/// `overflow_wrap`/`line_break`). Defaults mirror CSS `white-space: normal`.
#[derive(Clone, Copy)]
struct Cfg {
    word_break: WordBreak,
    hyphens: Hyphens,
    line_break: LineBreakStrictness,
    overflow_wrap: OverflowWrap,
    white_space: WhiteSpaceMode,
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            word_break: WordBreak::Normal,
            hyphens: Hyphens::Manual,
            line_break: LineBreakStrictness::Auto,
            overflow_wrap: OverflowWrap::Normal,
            white_space: WhiteSpaceMode::Normal,
        }
    }
}

/// Run stages 1-4 (mirroring `window.rs`) with the given width + `Cfg`.
fn layout_cfg(text: &str, width: AvailableSpace, cfg: Cfg) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let style = StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    };
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: Arc::new(style),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: width,
        white_space_mode: cfg.white_space,
        overflow_wrap: cfg.overflow_wrap,
        line_break: cfg.line_break,
        ..UnifiedConstraints::default()
    };

    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());

    let logical = create_logical_items(&content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout { items: Vec::new(), overflow: OverflowInfo::default() };
    }
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");

    let mut cursor = BreakCursor::new(&shaped);
    cursor.word_break = cfg.word_break;
    cursor.hyphens = cfg.hyphens;
    cursor.line_break = cfg.line_break;
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

fn layout(text: &str, width: AvailableSpace) -> UnifiedLayout {
    layout_cfg(text, width, Cfg::default())
}

fn line_count(layout: &UnifiedLayout) -> usize {
    let mut lines: Vec<usize> = layout
        .items
        .iter()
        .filter(|it| matches!(it.item, ShapedItem::Cluster(_)))
        .map(|it| it.line_index)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines.len()
}

fn line_width(layout: &UnifiedLayout, line: usize) -> f32 {
    let mut mn = f32::MAX;
    let mut mx = f32::MIN;
    for it in &layout.items {
        if it.line_index != line {
            continue;
        }
        if let ShapedItem::Cluster(c) = &it.item {
            mn = mn.min(it.position.x);
            mx = mx.max(it.position.x + c.advance);
        }
    }
    if mx < mn { 0.0 } else { mx - mn }
}

fn widest_line(layout: &UnifiedLayout) -> f32 {
    let lines: usize = layout
        .items
        .iter()
        .filter_map(|it| matches!(it.item, ShapedItem::Cluster(_)).then_some(it.line_index))
        .max()
        .map_or(0, |m| m + 1);
    (0..lines).map(|l| line_width(layout, l)).fold(0.0, f32::max)
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn keep_all_suppresses_cjk_break() {
    // UAX#14 / CSS Text §5.2: word-break:keep-all forbids the implicit break
    // between CJK ideographs, so "你好世界" (80px) stays on ONE line even in a 45px box.
    let cfg = Cfg { word_break: WordBreak::KeepAll, ..Cfg::default() };
    let l = layout_cfg("你好世界", AvailableSpace::Definite(45.0), cfg);
    assert_eq!(line_count(&l), 1, "keep-all must suppress the CJK inter-ideograph break");
}

#[test]
fn keep_all_still_breaks_at_space() {
    // §5.2: keep-all only suppresses the *implicit* CJK/letter breaks; an explicit
    // space is still a soft-wrap opportunity, so "aaaa aaaa" @60px wraps to 2 lines.
    let cfg = Cfg { word_break: WordBreak::KeepAll, ..Cfg::default() };
    let l = layout_cfg("aaaa aaaa", AvailableSpace::Definite(60.0), cfg);
    assert_eq!(line_count(&l), 2, "keep-all must still break at spaces");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn break_all_breaks_between_latin_letters() {
    // §5.2: word-break:break-all makes every letter a break opportunity, so a solid
    // run of six 'a's (72px) fits 3 per 40px line (36px), then wraps.
    let cfg = Cfg { word_break: WordBreak::BreakAll, ..Cfg::default() };
    let l = layout_cfg("aaaaaa", AvailableSpace::Definite(40.0), cfg);
    assert!(line_count(&l) >= 2, "break-all must split the unbreakable Latin run");
    assert_px(line_width(&l, 0), 36.0);
}

#[test]
fn break_all_min_content_is_single_cluster() {
    // CSS Sizing §4 + §5.2: with break-all every cluster is its own line for
    // min-content, so min-content of "aaaa" collapses to one 'a' = 12px.
    let cfg = Cfg { word_break: WordBreak::BreakAll, ..Cfg::default() };
    let l = layout_cfg("aaaa", AvailableSpace::MinContent, cfg);
    assert_px(widest_line(&l), 12.0);
}

#[test]
fn overflow_wrap_anywhere_emergency_breaks_long_word() {
    // CSS Text §5.5: overflow-wrap:anywhere breaks an otherwise-unbreakable word at
    // an arbitrary cluster boundary. Ten 'a's (120px) fit 4 per 50px line (48px).
    let cfg = Cfg { overflow_wrap: OverflowWrap::Anywhere, ..Cfg::default() };
    let l = layout_cfg("aaaaaaaaaa", AvailableSpace::Definite(50.0), cfg);
    assert!(line_count(&l) >= 2, "overflow-wrap:anywhere must break the long word");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn overflow_wrap_normal_keeps_long_word_intact() {
    // CSS Text §5.5: overflow-wrap:normal keeps an unbreakable word whole; it
    // overflows the 50px box on a single line rather than being shredded per glyph.
    let l = layout("aaaaaaaaaa", AvailableSpace::Definite(50.0));
    assert_eq!(line_count(&l), 1, "overflow-wrap:normal must not shred the word");
}

#[test]
fn line_break_anywhere_breaks_every_char() {
    // CSS Text §5.3: line-break:anywhere puts a soft-wrap opportunity around every
    // typographic unit, so "aaaaaa" @30px fits 2 'a's (24px) then wraps.
    let cfg = Cfg { line_break: LineBreakStrictness::Anywhere, ..Cfg::default() };
    let l = layout_cfg("aaaaaa", AvailableSpace::Definite(30.0), cfg);
    assert!(line_count(&l) >= 3, "line-break:anywhere must break between every char");
    assert_px(line_width(&l, 0), 24.0);
}

#[test]
fn zwsp_offers_a_soft_wrap_opportunity() {
    // UAX#14 class ZW: U+200B ZERO WIDTH SPACE is an explicit break opportunity, so
    // "aaaa\u{200B}aaaa" (96px, ZWSP=0) wraps at the ZWSP in a 60px box.
    let l = layout("aaaa\u{200B}aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "ZWSP must offer a soft-wrap opportunity");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn zwsp_breaks_even_under_keep_all() {
    // UAX#14: U+200B is honored even when word-break:keep-all suppresses every other
    // break — authors use it as an explicit wrap point.
    let cfg = Cfg { word_break: WordBreak::KeepAll, ..Cfg::default() };
    let l = layout_cfg("aaaa\u{200B}aaaa", AvailableSpace::Definite(60.0), cfg);
    assert_eq!(line_count(&l), 2, "ZWSP must break even under keep-all");
}

#[test]
fn soft_hyphen_creates_break_with_manual_hyphens() {
    // UAX#14 class BA + CSS hyphens:manual (default): a U+00AD SOFT HYPHEN is a
    // conditional break opportunity, so "aaaa\u{00AD}aaaa" wraps in a 60px box.
    let l = layout("aaaa\u{00AD}aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "soft hyphen must offer a break under hyphens:manual");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn soft_hyphen_no_break_when_hyphens_none() {
    // CSS Text §6.1: hyphens:none disables soft-hyphen break points, so the same
    // string stays on one (overflowing) line.
    let cfg = Cfg { hyphens: Hyphens::None, ..Cfg::default() };
    let l = layout_cfg("aaaa\u{00AD}aaaa", AvailableSpace::Definite(60.0), cfg);
    assert_eq!(line_count(&l), 1, "hyphens:none must ignore the soft hyphen");
}

#[test]
fn breaks_at_last_hyphen_that_fits() {
    // UAX#14 class HY: a break follows every U+002D; the greedy breaker takes the
    // last one that fits. "aaaa-aaaa-aaaa" @60px keeps "aaaa-" (54px) per line.
    let l = layout("aaaa-aaaa-aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 3, "each 'aaaa-' unit (54px) takes its own 60px line");
    assert_px(line_width(&l, 0), 54.0);
    assert_px(line_width(&l, 1), 54.0);
    assert_px(line_width(&l, 2), 48.0);
}

#[test]
fn slash_offers_a_soft_wrap_opportunity() {
    // UAX#14 class SY: a break is allowed after U+002F SOLIDUS (non-numeric context).
    // "aaaa/aaaa" (102px) should wrap after the slash in a 60px box, keeping "aaaa/"
    // (54px) on line 0. The impl offers no break after '/', so it stays on one line
    // (and the overflow is dropped by the positioner).
    let l = layout("aaaa/aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "a break opportunity must follow '/'");
    assert_px(line_width(&l, 0), 54.0);
}

#[test]
fn wide_at_sign_width_at_max_content() {
    // Pure width math with the broad '@' (18px), no overflow: "a@a" = 12+18+12 = 42px
    // at max-content. '@' is class AL, so the run is a single unbreakable unit.
    let l = layout("a@a", AvailableSpace::MaxContent);
    assert_eq!(line_count(&l), 1, "no break opportunity in 'a@a'");
    assert_px(widest_line(&l), 42.0);
}

#[test]
fn overlong_word_still_terminates_and_places_content() {
    // Degenerate: a 20-'a' run (240px) with no break opportunity must terminate and
    // place >= 1 line rather than spin, even at width 0.
    let l = layout("aaaaaaaaaaaaaaaaaaaa", AvailableSpace::Definite(0.0));
    assert!(line_count(&l) >= 1, "overlong word must complete with >= 1 line");
}
