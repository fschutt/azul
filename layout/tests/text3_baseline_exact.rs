#![cfg(feature = "text_layout")]
//! EXACT baseline / mixed-font-size / vertical-align tests for text3.
//!
//! Font: built-in "Azul Mock Mono" (ascent 800, descent -200, upem 1000). At
//! font-size F the ascent is 0.8F px, descent 0.2F px, line box F px.
//!
//! Positioned-item y convention (verified via the dump test): `position.y` is
//! the TOP of the cluster's own box; the cluster's alphabetic baseline sits at
//! `position.y + ascent_px`. Two runs share a baseline iff their (y + ascent)
//! agree.

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LoadedFonts,
    OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout, VerticalAlign,
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

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

fn styled_run(text: &str, size: f32, va: VerticalAlign, font_ref: &FontRef, start: usize) -> InlineContent {
    InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: Arc::new(StyleProperties {
            font_stack: FontStack::Ref(font_ref.clone()),
            font_size_px: size,
            vertical_align: va,
            ..StyleProperties::default()
        }),
        logical_start_byte: start,
        source_node_id: None,
    })
}

fn layout_runs(content: &[InlineContent], font_ref: &FontRef) -> UnifiedLayout {
    layout_runs_strut(content, font_ref, None)
}

/// Lay out `content`; if `strut` is Some((ascent, descent)) the line strut (the
/// block container's font metrics that text-top/text-bottom align to) is set
/// explicitly — otherwise the default 16px-based strut is used.
fn layout_runs_strut(
    content: &[InlineContent],
    font_ref: &FontRef,
    strut: Option<(f32, f32)>,
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
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    let mut constraints = UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        ..UnifiedConstraints::default()
    };
    if let Some((a, d)) = strut {
        constraints.strut_ascent = a;
        constraints.strut_descent = d;
    }
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

/// (top y, ascent-scaled baseline y) of the first cluster belonging to `run`.
fn cluster_y_and_baseline(l: &UnifiedLayout, run: u32) -> (f32, f32) {
    for it in &l.items {
        if let ShapedItem::Cluster(c) = &it.item {
            if c.source_cluster_id.source_run == run {
                let fm = &c.glyphs[0].font_metrics;
                let ascent_px = fm.ascent / f32::from(fm.units_per_em) * c.style.font_size_px;
                return (it.position.y, it.position.y + ascent_px);
            }
        }
    }
    panic!("no cluster for run {run}");
}

// ===========================================================================
// Diagnostic
// ===========================================================================

#[test]
fn dump_mixed_sizes() {
    let font_ref = mono_ref();
    let content = vec![
        styled_run("Ab", 20.0, VerticalAlign::Baseline, &font_ref, 0),
        styled_run("Cd", 40.0, VerticalAlign::Baseline, &font_ref, 2),
    ];
    let l = layout_runs(&content, &font_ref);
    for it in &l.items {
        if let ShapedItem::Cluster(c) = &it.item {
            let fm = &c.glyphs[0].font_metrics;
            let ascent_px = fm.ascent / f32::from(fm.units_per_em) * c.style.font_size_px;
            println!(
                "run={} text={:?} size={} y={} height={} ascent_px={} baseline_y={}",
                c.source_cluster_id.source_run, c.text, c.style.font_size_px,
                it.position.y, it.item.bounds().height, ascent_px, it.position.y + ascent_px
            );
        }
    }
    println!("first_baseline={:?}", l.first_baseline());
}

// ===========================================================================
// D1. Mixed font sizes share ONE baseline
// ===========================================================================

#[test]
fn mixed_20_40_share_baseline() {
    // Run0 20px (ascent 16), Run1 40px (ascent 32) on one line.
    // Baseline align: both alphabetic baselines coincide. The line baseline is
    // the taller run's ascent = 32px from line top. So:
    //   run1 (40px) top y = 0,   baseline = 32
    //   run0 (20px) top y = 16,  baseline = 32
    let font_ref = mono_ref();
    let content = vec![
        styled_run("Ab", 20.0, VerticalAlign::Baseline, &font_ref, 0),
        styled_run("Cd", 40.0, VerticalAlign::Baseline, &font_ref, 2),
    ];
    let l = layout_runs(&content, &font_ref);
    let (y20, base20) = cluster_y_and_baseline(&l, 0);
    let (y40, base40) = cluster_y_and_baseline(&l, 1);
    assert_px(base20, base40, "both runs share the SAME baseline y");
    assert_px(base40, 32.0, "line baseline = taller run ascent = 32px");
    assert_px(y40, 0.0, "40px run top sits at line top (y=0)");
    assert_px(y20, 16.0, "20px run top is pushed down so its baseline meets 32px");
}

#[test]
fn mixed_line_box_is_taller_run_height() {
    // Line box height must accommodate the 40px run = 40px, not the 20px run.
    let font_ref = mono_ref();
    let content = vec![
        styled_run("Ab", 20.0, VerticalAlign::Baseline, &font_ref, 0),
        styled_run("Cd", 40.0, VerticalAlign::Baseline, &font_ref, 2),
    ];
    let l = layout_runs(&content, &font_ref);
    // The bottom-most cluster extent = max(y+height) over clusters.
    let bottom = l
        .items
        .iter()
        .filter_map(|it| match &it.item {
            ShapedItem::Cluster(_) => Some(it.position.y + it.item.bounds().height),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    assert_px(bottom, 40.0, "line content bottom = 40px (taller run governs)");
}

// ===========================================================================
// D2. vertical-align sub / super / middle / text-top / text-bottom
// ===========================================================================

/// Lay out a baseline run then a shifted run (all 20px) and return
/// (baseline_cluster_top_y, shifted_cluster_top_y).
fn shifted(va: VerticalAlign) -> (f32, f32) {
    let font_ref = mono_ref();
    let content = vec![
        styled_run("Ab", 20.0, VerticalAlign::Baseline, &font_ref, 0),
        styled_run("x", 20.0, va, &font_ref, 2),
    ];
    let l = layout_runs(&content, &font_ref);
    let (y_base, _) = cluster_y_and_baseline(&l, 0);
    let (y_shift, _) = cluster_y_and_baseline(&l, 1);
    (y_base, y_shift)
}

#[test]
fn vertical_align_super_raises_glyph() {
    // super shifts the baseline up by line_ascent*0.4. Both runs 20px => the
    // shifted cluster top y is 0.4*16 = 6.4px ABOVE the baseline cluster top.
    let (y_base, y_super) = shifted(VerticalAlign::Super);
    assert!(y_super < y_base, "super must raise the glyph (smaller y): base {y_base}, super {y_super}");
    assert_px(y_base - y_super, 6.4, "super raise = line_ascent(16) * 0.4");
}

#[test]
fn vertical_align_sub_lowers_glyph() {
    // sub shifts the baseline down by line_ascent*0.3 = 4.8px.
    let (y_base, y_sub) = shifted(VerticalAlign::Sub);
    assert!(y_sub > y_base, "sub must lower the glyph (larger y): base {y_base}, sub {y_sub}");
    assert_px(y_sub - y_base, 4.8, "sub lower = line_ascent(16) * 0.3");
}

#[test]
fn vertical_align_middle_shifts_glyph() {
    // middle aligns the box midpoint to baseline + half x-height. It must differ
    // from the baseline position (a non-zero shift).
    let (y_base, y_mid) = shifted(VerticalAlign::Middle);
    assert!(
        (y_mid - y_base).abs() > 0.01,
        "middle must shift the glyph off baseline: base {y_base}, middle {y_mid}"
    );
}

#[test]
fn vertical_align_text_top_bottom_coincide_when_strut_matches_run() {
    // text-top aligns the box top with the parent CONTENT-AREA top (the strut).
    // text-bottom aligns box bottom with content-area bottom. When the strut is
    // set to the run's own 20px metrics (ascent 16, descent 4), both must
    // coincide EXACTLY with the baseline position — proving the formulas
    // (baseline - strut_ascent + item_ascent) and (baseline + strut_descent -
    // item_descent) reduce to the baseline when strut == item metrics.
    let font_ref = mono_ref();
    for va in [VerticalAlign::TextTop, VerticalAlign::TextBottom] {
        let content = vec![
            styled_run("Ab", 20.0, VerticalAlign::Baseline, &font_ref, 0),
            styled_run("x", 20.0, va, &font_ref, 2),
        ];
        let l = layout_runs_strut(&content, &font_ref, Some((16.0, 4.0)));
        let (y_base, _) = cluster_y_and_baseline(&l, 0);
        let (y_shift, _) = cluster_y_and_baseline(&l, 1);
        assert_px(y_shift, y_base, "text-top/bottom coincide with baseline when strut==run");
    }
}

#[test]
fn vertical_align_text_top_shifts_by_strut_mismatch() {
    // With the DEFAULT 16px-based strut (content-area top only 12.8px above the
    // baseline) but a 20px run (box top 16px above its baseline), aligning the
    // box top to the content-area top pushes the box DOWN by exactly
    // (item_ascent 16 - strut_ascent 12.8) = 3.2px. This pins that text-top
    // follows the container strut, not the run's own ascent.
    let (y_base, y_top) = shifted(VerticalAlign::TextTop);
    assert_px(y_top - y_base, 3.2, "text-top lowers box by item_ascent(16) - default strut_ascent(12.8)");
}
