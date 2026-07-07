#![cfg(feature = "text_layout")]
//! Deterministic CSS-Text white-space regression tests on the fake font
//! (`common/fakefont.rs`), driven through the real stage-1..4 text3 pipeline.
//!
//! The DOM layer (`fc.rs`) performs CSS "Phase I" collapsing BEFORE the pipeline,
//! so these tests exercise the pipeline's own Phase II duties: leading/trailing
//! trimming at line edges, wrap suppression (pre/nowrap), and tab-stop advance.
//!
//! Fake metrics @ size 20: 'a' 600u => 12px · space 250u => 5px · line-height
//! normal 20px · default tab-size 8 * (0.5*20px space approx) => 80px tab stops.

#[path = "common/fakefont.rs"]
mod fakefont;

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LoadedFonts,
    OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout, WhiteSpaceMode,
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

fn style_of(font_ref: &FontRef) -> Arc<StyleProperties> {
    Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    })
}

fn text_run(text: &str, font_ref: &FontRef) -> InlineContent {
    InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: style_of(font_ref),
        logical_start_byte: 0,
        source_node_id: None,
    })
}

fn layout_content(
    content: &[InlineContent],
    font_ref: &FontRef,
    mode: WhiteSpaceMode,
    width: AvailableSpace,
) -> UnifiedLayout {
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());

    let logical = create_logical_items(content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout { items: Vec::new(), overflow: OverflowInfo::default() };
    }
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");

    let constraints = UnifiedConstraints {
        available_width: width,
        white_space_mode: mode,
        ..UnifiedConstraints::default()
    };
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

/// Single text run through the pipeline under `mode`.
fn layout(text: &str, mode: WhiteSpaceMode, width: AvailableSpace) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    layout_content(&[text_run(text, &font_ref)], &font_ref, mode, width)
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

/// Visual extent of line 0 over its glyph-bearing clusters (spaces are clusters
/// with a 5px advance, so leading/trailing/internal spaces DO count here).
fn line0_width(layout: &UnifiedLayout) -> f32 {
    let mut mn = f32::MAX;
    let mut mx = f32::MIN;
    for it in &layout.items {
        if it.line_index != 0 {
            continue;
        }
        if let ShapedItem::Cluster(c) = &it.item {
            mn = mn.min(it.position.x);
            mx = mx.max(it.position.x + c.advance);
        }
    }
    if mx < mn { 0.0 } else { mx - mn }
}

/// The x of the first cluster whose text equals `needle`.
fn cluster_x(layout: &UnifiedLayout, needle: &str) -> f32 {
    layout
        .items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.text == needle => Some(it.position.x),
            _ => None,
        })
        .unwrap_or_else(|| panic!("cluster {needle:?} not found"))
}

// ===========================================================================
// Wrap suppression: normal/pre-wrap/pre-line WRAP; nowrap/pre DO NOT
// ===========================================================================

#[test]
fn normal_wraps_at_space() {
    // CSS Text §3: white-space:normal wraps at soft-wrap opportunities. "aaaa aaaa"
    // (101px) breaks in a 60px box.
    let l = layout("aaaa aaaa", WhiteSpaceMode::Normal, AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "normal must wrap at the space");
}

#[test]
fn nowrap_suppresses_wrapping() {
    // CSS Text §3: white-space:nowrap suppresses soft wraps; the content stays on a
    // single line even though it overflows the 60px box.
    let l = layout("aaaa aaaa", WhiteSpaceMode::Nowrap, AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 1, "nowrap must not wrap");
}

#[test]
fn pre_suppresses_wrapping() {
    // CSS Text §3: white-space:pre neither collapses nor wraps; single line, overflows.
    let l = layout("aaaa aaaa", WhiteSpaceMode::Pre, AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 1, "pre must not wrap");
}

#[test]
fn pre_wrap_wraps_at_space() {
    // CSS Text §3: white-space:pre-wrap preserves spaces but DOES wrap; 2 lines @60px.
    let l = layout("aaaa aaaa", WhiteSpaceMode::PreWrap, AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "pre-wrap must wrap at the space");
}

#[test]
fn pre_line_wraps_at_space() {
    // CSS Text §3: white-space:pre-line collapses spaces but DOES wrap; 2 lines @60px.
    let l = layout("aaaa aaaa", WhiteSpaceMode::PreLine, AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "pre-line must wrap at the space");
}

// ===========================================================================
// Phase II edge trimming: leading/trailing space at the line edge
// ===========================================================================

#[test]
fn normal_strips_leading_space_at_line_start() {
    // CSS Text §4.1.1: collapsible leading white space is removed at line start, so
    // "  aa" measures as just "aa" = 24px (two 5px spaces gone).
    let l = layout("  aa", WhiteSpaceMode::Normal, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 24.0);
}

#[test]
fn pre_preserves_leading_space() {
    // CSS Text §4.1.1: white-space:pre preserves leading indentation, so "  aa" keeps
    // both 5px spaces: 5 + 5 + 24 = 34px.
    let l = layout("  aa", WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 34.0);
}

#[test]
fn pre_line_strips_leading_space() {
    // CSS Text §4.1.1: pre-line is a collapsing mode for spaces, so leading spaces are
    // trimmed exactly like normal — "  aa" => 24px.
    let l = layout("  aa", WhiteSpaceMode::PreLine, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 24.0);
}

#[test]
fn normal_strips_trailing_space_at_line_end() {
    // CSS Text §4.1.1: trailing collapsible white space is removed at line end, so
    // "aa  " measures as "aa" = 24px.
    let l = layout("aa  ", WhiteSpaceMode::Normal, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 24.0);
}

#[test]
fn pre_preserves_trailing_space() {
    // CSS Text §4.1.1: pre preserves trailing spaces, so "aa  " = 24 + 5 + 5 = 34px.
    let l = layout("aa  ", WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 34.0);
}

#[test]
fn pre_preserves_internal_runs_of_spaces() {
    // CSS Text §4.1.1: pre preserves every internal space, so "a   a" = 12 + 15 + 12
    // = 39px (three 5px spaces kept).
    let l = layout("a   a", WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(line0_width(&l), 39.0);
}

// ===========================================================================
// Tab advance (CSS Text §6.2 tab-size)
// ===========================================================================

#[test]
fn tab_advances_to_next_default_stop() {
    // CSS Text §6.2: tab-size defaults to 8; the tab stop interval is 8 * (0.5*20px
    // space approximation) = 80px. After 'a' (12px) the tab fills to x=80, so 'b'
    // lands at x=80.
    let font_ref = fake_font_ref();
    let content = vec![
        text_run("a", &font_ref),
        InlineContent::Tab { style: style_of(&font_ref) },
        text_run("b", &font_ref),
    ];
    let l = layout_content(&content, &font_ref, WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(cluster_x(&l, "a"), 0.0);
    assert_px(cluster_x(&l, "b"), 80.0);
}

#[test]
fn tab_from_line_start_is_one_full_stop() {
    // CSS Text §6.2: a tab at the content origin advances a full 80px stop, so the
    // following 'a' begins at x=80.
    let font_ref = fake_font_ref();
    let content = vec![
        InlineContent::Tab { style: style_of(&font_ref) },
        text_run("a", &font_ref),
    ];
    let l = layout_content(&content, &font_ref, WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(cluster_x(&l, "a"), 80.0);
}

#[test]
fn tab_size_zero_collapses_tab_to_zero_width() {
    // CSS Text §6.2: tab-size:0 renders the tab with zero width, so 'b' abuts 'a' at x=12.
    let font_ref = fake_font_ref();
    let tab_style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        tab_size: 0.0,
        ..StyleProperties::default()
    });
    let content = vec![
        text_run("a", &font_ref),
        InlineContent::Tab { style: tab_style },
        text_run("b", &font_ref),
    ];
    let l = layout_content(&content, &font_ref, WhiteSpaceMode::Pre, AvailableSpace::MaxContent);
    assert_px(cluster_x(&l, "b"), 12.0);
}
