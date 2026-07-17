#![cfg(feature = "text_layout")]
//! Deterministic Unicode Bidirectional Algorithm (UBA / UAX#9) regression tests
//! on the fake font (`common/fakefont.rs`), driven through the real stage-1..4
//! text3 pipeline. The RTL glyph-level reversal (rule L2) and bidi run ordering
//! were recently fixed on this branch; these lock the exact visual x of every
//! cluster.
//!
//! Fake metrics @ size 20: 'a' 600u => 12px · Hebrew א/ב/ג 550u => 11px · digit
//! 500u => 10px · space/'.' 250u => 5px. Hebrew chars are 2 UTF-8 bytes each.

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
    UnifiedLayout,
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

fn layout_bidi(text: &str, base: Option<BidiDirection>, width: AvailableSpace) -> UnifiedLayout {
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

    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());

    let logical = create_logical_items(&content, &[], &mut None);
    if logical.is_empty() {
        return UnifiedLayout { items: Vec::new(), overflow: OverflowInfo::default() };
    }
    let base_dir = base.unwrap_or(BidiDirection::Ltr);
    let visual = reorder_logical_items(&logical, base_dir, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");

    let constraints = UnifiedConstraints {
        available_width: width,
        direction: base,
        ..UnifiedConstraints::default()
    };
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

/// Visual x of the cluster whose logical start byte == `byte`.
fn x_of(layout: &UnifiedLayout, byte: u32) -> f32 {
    layout
        .items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.source_cluster_id.start_byte_in_run == byte => {
                Some(it.position.x)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster starting at byte {byte}"))
}

/// Line index of the cluster whose logical start byte == `byte`.
fn line_of(layout: &UnifiedLayout, byte: u32) -> usize {
    layout
        .items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.source_cluster_id.start_byte_in_run == byte => {
                Some(it.line_index)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster starting at byte {byte}"))
}

/// Total visual extent across all clusters.
fn total_width(layout: &UnifiedLayout) -> f32 {
    let mut mn = f32::MAX;
    let mut mx = f32::MIN;
    for it in &layout.items {
        if let ShapedItem::Cluster(c) = &it.item {
            mn = mn.min(it.position.x);
            mx = mx.max(it.position.x + c.advance);
        }
    }
    if mx < mn { 0.0 } else { mx - mn }
}

// ===========================================================================
// Tests — every x is exact arithmetic on the fake advances.
// ===========================================================================

#[test]
fn pure_rtl_hebrew_reverses_exact_x() {
    // UBA L2: a pure Hebrew run reverses at the glyph level. "אבג" (each 11px) lays
    // out ג@0, ב@11, א@22 — the earliest logical char at the LARGEST x.
    let l = layout_bidi("אבג", None, AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 22.0); // א (first logical) rightmost
    assert_px(x_of(&l, 2), 11.0); // ב
    assert_px(x_of(&l, 4), 0.0); // ג (last logical) leftmost
    assert_px(total_width(&l), 33.0);
}

#[test]
fn pure_rtl_base_gives_same_visual_order() {
    // UBA: an explicit RTL base direction on a pure-RTL run yields the same visual
    // order as auto-detection (single run, reversed) — positions are identical.
    let l = layout_bidi("אבג", Some(BidiDirection::Rtl), AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 22.0);
    assert_px(x_of(&l, 2), 11.0);
    assert_px(x_of(&l, 4), 0.0);
}

#[test]
fn ltr_base_embedded_rtl_run_exact_x() {
    // UBA: LTR base with an embedded RTL run. "aaאב" => a@0, a@12, then the Hebrew
    // run reversed to the right: ב@24, א@35.
    let l = layout_bidi("aaאב", None, AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 0.0); // a
    assert_px(x_of(&l, 1), 12.0); // a
    assert_px(x_of(&l, 4), 24.0); // ב (later logical) at smaller x
    assert_px(x_of(&l, 2), 35.0); // א (earlier logical) at larger x
    assert_px(total_width(&l), 46.0);
}

#[test]
fn rtl_base_embedded_ltr_run_exact_x() {
    // UBA: RTL base with an embedded LTR run. "אבaa" => the Hebrew run sits at the
    // visual RIGHT (א@35, ב@24) and the Latin run reads left-to-right at the LEFT
    // (a@0, a@12).
    let l = layout_bidi("אבaa", Some(BidiDirection::Rtl), AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 35.0); // א rightmost
    assert_px(x_of(&l, 2), 24.0); // ב
    assert_px(x_of(&l, 4), 0.0); // a (LTR run keeps ascending order)
    assert_px(x_of(&l, 5), 12.0); // a
}

#[test]
fn european_numbers_stay_ltr_after_rtl() {
    // UBA rule L1/L2 + EN class: digits keep their left-to-right internal order even
    // inside RTL context. "אב12" (LTR base) => 1@0, 2@10 (ascending), Hebrew reversed
    // to the right: ב@20, א@31.
    let l = layout_bidi("אב12", None, AvailableSpace::MaxContent);
    assert_px(x_of(&l, 4), 0.0); // '1' before '2'
    assert_px(x_of(&l, 5), 10.0); // '2'
    assert_px(x_of(&l, 2), 20.0); // ב
    assert_px(x_of(&l, 0), 31.0); // א
}

#[test]
fn european_numbers_stay_ltr_in_rtl_base() {
    // UBA: with RTL base, "12אב" places the number as an LTR island at the visual
    // right (1@22, 2@32) and the Hebrew run at the left, reversed (א@11, ב@0).
    let l = layout_bidi("12אב", Some(BidiDirection::Rtl), AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 22.0); // '1'
    assert_px(x_of(&l, 1), 32.0); // '2' (LTR order preserved)
    assert_px(x_of(&l, 2), 11.0); // א
    assert_px(x_of(&l, 4), 0.0); // ב
}

#[test]
fn neutral_between_runs_resolves_to_base() {
    // UBA rules N1/N2: a neutral '.' between an L run and an R run resolves to the
    // base (LTR) direction, staying with the "aa" on the left. "aa.אב" => a@0, a@12,
    // .@24, then Hebrew reversed: ב@29, א@40.
    let l = layout_bidi("aa.אב", None, AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 0.0); // a
    assert_px(x_of(&l, 1), 12.0); // a
    assert_px(x_of(&l, 2), 24.0); // '.' (neutral takes LTR base)
    assert_px(x_of(&l, 5), 29.0); // ב
    assert_px(x_of(&l, 3), 40.0); // א
}

#[test]
fn multiple_runs_keep_ltr_rtl_ltr_visual_order() {
    // UBA: three runs L-R-L ("aa" + "אב" + "cc") keep the RTL island reversed between
    // the two ascending Latin runs. "aaאבcc" => a@0,a@12, ב@24,א@35, c@46,c@58.
    let l = layout_bidi("aaאבcc", None, AvailableSpace::MaxContent);
    assert_px(x_of(&l, 0), 0.0);
    assert_px(x_of(&l, 1), 12.0);
    assert_px(x_of(&l, 4), 24.0); // ב
    assert_px(x_of(&l, 2), 35.0); // א
    assert_px(x_of(&l, 6), 46.0); // c
    assert_px(x_of(&l, 7), 58.0); // c
    assert_px(total_width(&l), 70.0);
}

#[test]
fn bidi_reversal_is_applied_per_line_after_wrap() {
    // UBA rule L2 is applied per line: "אבג אבג" wrapped at 40px puts one Hebrew word
    // per line, each independently reversed (ג@0, ב@11, א@22 on BOTH lines).
    let l = layout_bidi("אבג אבג", None, AvailableSpace::Definite(40.0));
    // Line 0: first word (bytes 0/2/4).
    assert_eq!(line_of(&l, 0), 0);
    assert_px(x_of(&l, 0), 22.0);
    assert_px(x_of(&l, 4), 0.0);
    // Line 1: second word (bytes 7/9/11) — same reversed layout, own line.
    assert_eq!(line_of(&l, 7), 1);
    assert_px(x_of(&l, 7), 22.0);
    assert_px(x_of(&l, 11), 0.0);
}

#[test]
fn bidi_width_is_direction_agnostic() {
    // Advances do not depend on direction: "aaאב" measures 46px whether the base
    // direction is LTR or RTL (only the visual x order flips).
    let ltr = layout_bidi("aaאב", None, AvailableSpace::MaxContent);
    let rtl = layout_bidi("aaאב", Some(BidiDirection::Rtl), AvailableSpace::MaxContent);
    assert_px(total_width(&ltr), 46.0);
    assert_px(total_width(&rtl), 46.0);
}

#[test]
fn mixed_run_with_spaces_total_width() {
    // "aa אב" = 12+12 + space 5 + 11+11 = 51px; the space (neutral) sits between the
    // LTR and RTL runs and counts toward the extent.
    let l = layout_bidi("aa אב", None, AvailableSpace::MaxContent);
    assert_px(total_width(&l), 51.0);
    assert_px(x_of(&l, 2), 24.0); // the space, at LTR base position
}
