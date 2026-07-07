#![cfg(feature = "text_layout")]
//! Brutal, deterministic shaping + line-breaking tests for the REAL text3
//! pipeline, driven by a synthetic fake font (`common/fakefont.rs`).
//!
//! Two entry points are exercised:
//!   * `shape_text_for_parsed_font` — raw allsorts shaping (advances, kerning,
//!     cmap, combining marks) straight off a `ParsedFont`.
//!   * `create_logical_items` -> `reorder_logical_items` -> `shape_visual_items`
//!     -> `BreakCursor` -> `perform_fragment_layout` — the exact stage-1..4
//!     pipeline `window.rs::relayout_text_node_internal` runs, producing a
//!     real `UnifiedLayout` with positioned clusters and line indices.
//!
//! Every expected number is derived from the fake metrics (upem 1000, ascent
//! 800, descent -200, gap 0). At font-size 20 the scale is 0.02, so:
//!   'a'..'z' 600u => 12px · 'A'..'Z' 700u => 14px · space/NBSP 250u => 5px
//!   '-' 300u => 6px · CJK 1000u => 20px · Hebrew 550u => 11px · U+0301 => 0px
//!   kern (A,V)=(V,A)=-100u => -2px.

#[path = "common/fakefont.rs"]
mod fakefont;

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, Glyph, InlineContent,
    LoadedFonts, ShapedItem, Spacing, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout, WhiteSpaceMode,
};
use azul_layout::text3::default::shape_text_for_parsed_font;
use azul_layout::text3::script::{Language, Script};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

use fakefont::simple_test_font;

const FONT_SIZE: f32 = 20.0;

/// Assert two pixel measurements agree within a 0.05px epsilon.
fn assert_px(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= 0.05,
        "assert_px failed: expected {expected:.4}px, got {actual:.4}px (|delta| {delta:.4}px > 0.05px)"
    );
}

// --- Font construction -----------------------------------------------------

/// Parse the fake font, RETAINING the source bytes so `hmtx` advances are
/// readable (`from_bytes` alone drops `original_bytes` and every advance would
/// come back zero — see test_glyph_cache_shaping.rs).
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

fn base_style(font_ref: &FontRef) -> StyleProperties {
    StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    }
}

// --- Raw shaping helpers ---------------------------------------------------

fn shape_latin(text: &str) -> Vec<Glyph> {
    let parsed = fake_parsed();
    let style = StyleProperties {
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        &parsed,
        text,
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping must succeed")
}

/// Total advance width = sum of base advances plus kerning adjustments (px).
fn advance_sum(glyphs: &[Glyph]) -> f32 {
    glyphs.iter().map(|g| g.advance + g.kerning).sum()
}

// --- Full pipeline helpers -------------------------------------------------

/// Run stages 1-4 of the real text3 pipeline (the same calls as
/// `window.rs::relayout_text_node_internal`) and return the positioned layout.
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
            overflow: azul_layout::text3::cache::OverflowInfo::default(),
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

fn run(text: &str, style: StyleProperties, constraints: UnifiedConstraints) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let style = StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        ..style
    };
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: Arc::new(style),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    layout_content(&content, &font_ref, &constraints)
}

/// Convenience: default 20px style, given available inline width.
fn layout(text: &str, width: AvailableSpace) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let style = base_style(&font_ref);
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: Arc::new(style),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: width,
        ..UnifiedConstraints::default()
    };
    layout_content(&content, &font_ref, &constraints)
}

/// Number of distinct line indices that carry a shaped cluster.
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

/// Visual width of the widest line = max(cluster.x + advance) - min(cluster.x)
/// per line, then the max across lines. Trailing letter-spacing / hung spaces
/// do not carry a glyph, so they never extend this extent.
fn widest_line(layout: &UnifiedLayout) -> f32 {
    let mut per_line: HashMap<usize, (f32, f32)> = HashMap::new();
    for it in &layout.items {
        if let ShapedItem::Cluster(c) = &it.item {
            let e = per_line.entry(it.line_index).or_insert((f32::MAX, f32::MIN));
            e.0 = e.0.min(it.position.x);
            e.1 = e.1.max(it.position.x + c.advance);
        }
    }
    per_line
        .values()
        .map(|(mn, mx)| mx - mn)
        .fold(0.0, f32::max)
}

/// Visual width of one specific line index.
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
    if mx < mn {
        0.0
    } else {
        mx - mn
    }
}

fn clusters_on_line(layout: &UnifiedLayout, line: usize) -> usize {
    layout
        .items
        .iter()
        .filter(|it| it.line_index == line && matches!(it.item, ShapedItem::Cluster(_)))
        .count()
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn cmap_abc_maps_three_glyphs_each_12px() {
    // cmap: each of a,b,c resolves to a distinct glyph; hmtx advance 600u => 12px.
    let g = shape_latin("abc");
    assert_eq!(g.len(), 3, "abc must shape to exactly 3 glyphs");
    for glyph in &g {
        assert_px(glyph.advance, 12.0);
    }
    assert_px(advance_sum(&g), 36.0);
}

#[test]
fn four_a_is_48px() {
    // Sum of advances: 4 * 12px (no kerning between 'a' pairs).
    assert_px(advance_sum(&shape_latin("aaaa")), 48.0);
}

#[test]
fn aa_space_aa_is_53px() {
    // 12+12 + space 5 + 12+12 = 53px (single collapsible space width 250u => 5px).
    assert_px(advance_sum(&shape_latin("aa aa")), 53.0);
}

#[test]
fn wrap_two_words_at_60_breaks_into_two_lines() {
    // UAX#14: the space is the sole break opportunity; 48+5+48=101 > 60 forces a break.
    let l = layout("aaaa aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "aaaa aaaa @60px must wrap to 2 lines");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn wrap_two_words_at_200_stays_one_line() {
    // 101px content fits inside 200px, so no soft break is taken.
    let l = layout("aaaa aaaa", AvailableSpace::Definite(200.0));
    assert_eq!(line_count(&l), 1, "aaaa aaaa @200px must stay on 1 line");
}

#[test]
fn break_at_exact_48_hangs_trailing_space() {
    // CSS Text §4.1.2: a soft-wrap opportunity's trailing space hangs; the first
    // word fills exactly 48px, the space hangs, the second word wraps.
    let l = layout("aaaa aaaa", AvailableSpace::Definite(48.0));
    assert_eq!(line_count(&l), 2, "aaaa aaaa @48px must wrap after the exact-fit word");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn trailing_space_only_stays_one_line() {
    // CSS Text §4.1.2: a line's trailing space is trimmed, so "aaaa " (48+5) fits in 48.
    let l = layout("aaaa ", AvailableSpace::Definite(48.0));
    assert_eq!(line_count(&l), 1, "'aaaa ' @48px keeps 1 line (trailing space trimmed)");
    assert_px(line_width(&l, 0), 48.0);
}

#[test]
fn double_space_collapses_to_one_gap() {
    // CSS Text §4.1.1: a sequence of collapsible spaces collapses to a single space,
    // so "aa  aa" measures the same 53px as "aa aa".
    // pins spec: CSS Text L3 §4.1.1 white-space collapse (may be applied only in the
    // DOM white-space phase, not the raw text3 pipeline).
    let l = layout("aa  aa", AvailableSpace::MaxContent);
    assert_px(widest_line(&l), 53.0);
}

#[test]
fn newline_forces_two_lines() {
    // UAX#14 §5: LF (U+000A) is a mandatory break (class BK). white-space:pre
    // preserves it as a forced line break regardless of available width.
    // pins spec: UAX#14 mandatory break on LF.
    let l = run(
        "aa\naa",
        StyleProperties {
            font_size_px: FONT_SIZE,
            ..StyleProperties::default()
        },
        UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            white_space_mode: WhiteSpaceMode::Pre,
            ..UnifiedConstraints::default()
        },
    );
    assert_eq!(line_count(&l), 2, "'aa\\naa' must always render on 2 lines");
}

#[test]
fn overlong_unbreakable_word_terminates() {
    // No break opportunity exists inside a run of 20 'a's (240px). The line
    // breaker must terminate (overflow the 50px box) rather than spin forever.
    let l = layout("aaaaaaaaaaaaaaaaaaaa", AvailableSpace::Definite(50.0));
    assert!(line_count(&l) >= 1, "overlong word must still place >= 1 line");
}

#[test]
fn break_after_hyphen_keeps_hyphen_on_first_line() {
    // UAX#14: a break opportunity exists AFTER U+002D (class BA), before the next word.
    // Line 1 keeps "aaaa-" = 4*12 + 6 = 54px.
    let l = layout("aaaa-aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 2, "hyphenated pair must break after '-'");
    assert_px(line_width(&l, 0), 54.0);
}

#[test]
fn nbsp_does_not_offer_a_break() {
    // UAX#14: U+00A0 NBSP is class GL (glue) — no break opportunity, so the whole
    // 101px string stays on one line even though it overflows the 60px box.
    let l = layout("aaaa\u{00A0}aaaa", AvailableSpace::Definite(60.0));
    assert_eq!(line_count(&l), 1, "NBSP must not offer a soft-wrap opportunity");
}

#[test]
fn letter_spacing_2px_applies_after_each_cluster() {
    // CSS Text §8: letter-spacing adds 2px after every typographic cluster
    // (cache.rs position_one_line adds it after each cluster incl. the last).
    // The trailing spacing extends the pen but carries no glyph, so the visual
    // extent of "aaa" = 3*12 + (3-1)*2 = 40px.
    // pins spec: letter-spacing applied-after-every-glyph => 40px visual extent
    // (as opposed to 38px if the last inter-glyph gap were trimmed).
    let l = run(
        "aaa",
        StyleProperties {
            font_size_px: FONT_SIZE,
            letter_spacing: Spacing::Px(2),
            ..StyleProperties::default()
        },
        UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            ..UnifiedConstraints::default()
        },
    );
    assert_px(widest_line(&l), 40.0);
}

#[test]
fn word_spacing_3px_adds_between_words() {
    // CSS Text §8: word-spacing adds 3px at the space (word separator). The space
    // precedes the following glyphs, so it falls inside the visual extent:
    // 53 (base "aa aa") + 3 = 56px.
    let l = run(
        "aa aa",
        StyleProperties {
            font_size_px: FONT_SIZE,
            word_spacing: Spacing::Px(3),
            ..StyleProperties::default()
        },
        UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            ..UnifiedConstraints::default()
        },
    );
    assert_px(widest_line(&l), 56.0);
}

#[test]
fn kern_pair_av_subtracts_2px() {
    // GPOS/kern: (A,V) = -100u => -2px. 14 + 14 - 2 = 26px.
    assert_px(advance_sum(&shape_latin("AV")), 26.0);
}

#[test]
fn kern_pair_va_subtracts_2px() {
    // kern is symmetric in the fake font: (V,A) = -100u => -2px. 26px.
    assert_px(advance_sum(&shape_latin("VA")), 26.0);
}

#[test]
fn no_kern_between_aa_is_28px() {
    // No kern pair (A,A): 14 + 14 = 28px, proving kerning is pair-specific.
    assert_px(advance_sum(&shape_latin("AA")), 28.0);
}

#[test]
fn cjk_breaks_between_ideographs() {
    // UAX#14: CJK ideographs (class ID) offer a break opportunity between each
    // other. Each is 1000u => 20px; "你好" (40px) fills the 45px box, "世界" wraps.
    let l = layout("你好世界", AvailableSpace::Definite(45.0));
    assert_eq!(line_count(&l), 2, "CJK must break between ideographs at 45px");
    assert_eq!(clusters_on_line(&l, 0), 2, "line 1 holds 2 ideographs");
    assert_px(line_width(&l, 0), 40.0);
}

#[test]
fn combining_mark_stays_in_cluster_and_has_zero_advance() {
    // UAX#29: 'a' + U+0301 COMBINING ACUTE form one grapheme cluster; the mark's
    // advance is 0, so width("a\u{0301}") = 12px and the cluster is unbreakable.
    let g = shape_latin("a\u{0301}");
    assert_px(advance_sum(&g), 12.0);
    // The mark contributes a glyph but adds no advance.
    assert!(g.len() >= 2, "base + combining mark should produce >= 2 glyphs");
    // In the positioned layout the pair is a single cluster of advance 12.
    let l = layout("a\u{0301}a", AvailableSpace::MaxContent);
    if let Some(ShapedItem::Cluster(c)) = l.items.iter().find_map(|it| match &it.item {
        ShapedItem::Cluster(_) => Some(&it.item),
        _ => None,
    }) {
        assert_px(c.advance, 12.0);
    } else {
        panic!("expected at least one positioned cluster");
    }
}

#[test]
fn hebrew_run_is_rtl_reversed_and_33px_wide() {
    // UBA: a Hebrew run reorders right-to-left. Each letter 550u => 11px, 3 => 33px.
    // Logical order (by source byte) must map to DESCENDING visual x (reversed).
    let l = layout("אבג", AvailableSpace::MaxContent);
    assert_px(widest_line(&l), 33.0);

    let mut clusters: Vec<(u32, f32)> = l
        .items
        .iter()
        .filter_map(|it| match &it.item {
            ShapedItem::Cluster(c) => {
                Some((c.source_cluster_id.start_byte_in_run, it.position.x))
            }
            _ => None,
        })
        .collect();
    clusters.sort_by_key(|(byte, _)| *byte);
    for pair in clusters.windows(2) {
        assert!(
            pair[0].1 > pair[1].1,
            "RTL: earlier logical char must sit at a larger x ({} then {})",
            pair[0].1,
            pair[1].1
        );
    }
}

#[test]
fn bidi_mixed_run_is_80px_and_reverses_hebrew() {
    // UBA: "aa אב aa" = LTR + RTL + LTR. Advances are direction-agnostic:
    // 24 + 5 + 22 + 5 + 24 = 80px. The embedded Hebrew "אב" reverses internally.
    let l = layout("aa אב aa", AvailableSpace::MaxContent);
    assert_px(widest_line(&l), 80.0);

    // The two Hebrew letters (bytes within the middle run) must be visually
    // reversed: the earlier logical byte gets the larger x.
    let mut hebrew: Vec<(u32, f32)> = l
        .items
        .iter()
        .filter_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.text.chars().any(|ch| ('\u{0590}'..='\u{05FF}').contains(&ch)) => {
                Some((c.source_cluster_id.start_byte_in_run, it.position.x))
            }
            _ => None,
        })
        .collect();
    hebrew.sort_by_key(|(byte, _)| *byte);
    assert_eq!(hebrew.len(), 2, "expected the two Hebrew clusters");
    assert!(
        hebrew[0].1 > hebrew[1].1,
        "embedded RTL run must be visually reversed"
    );
}

#[test]
fn line_height_normal_is_20px_baseline_16px() {
    // line-height:normal = (ascent 800 + descent 200 + gap 0) * 0.02 = 20px;
    // the alphabetic baseline sits ascent*0.02 = 16px below the line top.
    let l = layout("a", AvailableSpace::MaxContent);
    let cluster = l
        .items
        .iter()
        .find(|it| matches!(it.item, ShapedItem::Cluster(_)))
        .expect("one cluster");
    assert_px(cluster.item.bounds().height, 20.0);
    assert_px(l.first_baseline().expect("a baseline"), 16.0);
}

#[test]
fn min_content_is_widest_word_max_content_is_full_line() {
    // CSS Sizing §4: min-content = widest unbreakable unit ("aaaa" = 48px);
    // max-content = single unwrapped line (48+5+48 = 101px).
    let min = layout("aaaa aaaa", AvailableSpace::MinContent);
    assert_px(widest_line(&min), 48.0);
    let max = layout("aaaa aaaa", AvailableSpace::MaxContent);
    assert_px(widest_line(&max), 101.0);
}

#[test]
fn empty_string_does_not_panic() {
    // Degenerate input: an empty run must lay out to zero items without panicking.
    let l = layout("", AvailableSpace::MaxContent);
    assert_eq!(line_count(&l), 0, "empty text => no lines");
    assert!(l.items.is_empty(), "empty text => no positioned items");
}

#[test]
fn zero_width_terminates_with_at_least_one_line() {
    // width:0 with unbreakable-per-word content must not loop forever; the greedy
    // breaker overflows and still emits >= 1 line.
    let l = layout("aaaa aaaa", AvailableSpace::Definite(0.0));
    assert!(line_count(&l) >= 1, "width:0 must complete with >= 1 line");
}
