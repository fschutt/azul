#![cfg(feature = "text_layout")]
//! Deterministic line-box / baseline / vertical-align regression tests on the
//! fake font (`common/fakefont.rs`), driven through the real text3 pipeline.
//!
//! Font metrics: upem 1000, ascent 800, descent -200, gap 0. At size 20 (scale
//! 0.02): ascent 16px, descent 4px, so line-height:normal = 20px and the
//! alphabetic baseline is 16px below the content-box top. line-height's half-
//! leading L/2 = (line-height - (A+D))/2 is split above and below the glyph box.

#[path = "common/fakefont.rs"]
mod fakefont;

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack, InlineContent, LineHeight,
    LoadedFonts, OverflowInfo, ShapedItem, StyleProperties, StyledRun, UnicodeBidi,
    UnifiedConstraints, UnifiedLayout, VerticalAlign,
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

/// Layout one text run with an explicit style + constraints (full control over
/// line-height, strut, vertical-align).
fn layout_sc(text: &str, style: StyleProperties, mut constraints: UnifiedConstraints) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let style = StyleProperties { font_stack: FontStack::Ref(font_ref.clone()), ..style };
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
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let mut cursor = BreakCursor::new(&shaped);
    constraints.direction = Some(BidiDirection::Ltr);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

/// Base 20px style with `line-height: normal`.
fn base_style() -> StyleProperties {
    StyleProperties { font_size_px: FONT_SIZE, ..StyleProperties::default() }
}

/// The first cluster (in item order).
fn first_cluster(layout: &UnifiedLayout) -> (&azul_layout::text3::cache::PositionedItem, f32) {
    let it = layout
        .items
        .iter()
        .find(|it| matches!(it.item, ShapedItem::Cluster(_)))
        .expect("a cluster");
    (it, it.item.bounds().height)
}

/// position.y of the first cluster whose byte == `byte`.
fn y_of(layout: &UnifiedLayout, byte: u32) -> f32 {
    layout
        .items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.source_cluster_id.start_byte_in_run == byte => {
                Some(it.position.y)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster at byte {byte}"))
}

/// Layout arbitrary pre-built content (for multi-run baseline tests).
fn layout_multi(content: &[InlineContent], constraints: UnifiedConstraints) -> UnifiedLayout {
    let font_ref = fake_font_ref();
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());
    let logical = create_logical_items(content, &[], &mut None);
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi reorder");
    let chain: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("fragment layout")
}

fn y_of_run(layout: &UnifiedLayout, run: u32) -> f32 {
    layout
        .items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.source_cluster_id.source_run == run => Some(it.position.y),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster in run {run}"))
}

// ===========================================================================
// Line-box height from line-height + font metrics
// ===========================================================================

#[test]
fn line_height_normal_box_is_20_and_baseline_16() {
    // §10.8: line-height:normal => (ascent 800 + |descent| 200) * 0.02 = 20px box;
    // the alphabetic baseline is ascent*0.02 = 16px below the content-box top.
    let l = layout_sc("a", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    let (_, h) = first_cluster(&l);
    assert_px(h, 20.0);
    assert_px(l.first_baseline().expect("baseline"), 16.0);
}

#[test]
fn explicit_line_height_px_sets_box_height() {
    // §10.8: line-height:30px yields a 30px content box (half-leading 5px each side:
    // A' = 16+5 = 21, D' = 4+5 = 9).
    let style = StyleProperties { line_height: LineHeight::Px(30.0), ..base_style() };
    let l = layout_sc("a", style, UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    let (_, h) = first_cluster(&l);
    assert_px(h, 30.0);
}

#[test]
fn unitless_line_height_resolves_to_px_box() {
    // §10.8: a unitless line-height (e.g. 1.2) is pre-resolved by CSS to px
    // (1.2 * 20 = 24px); the resulting box height is 24px.
    let style = StyleProperties { line_height: LineHeight::Px(24.0), ..base_style() };
    let l = layout_sc("a", style, UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    let (_, h) = first_cluster(&l);
    assert_px(h, 24.0);
}

#[test]
fn line_height_smaller_than_content_shrinks_box() {
    // §10.8: line-height may be smaller than the font's A+D (negative leading);
    // line-height:16px on a 20px-tall glyph gives a 16px box (L/2 = -2 each side).
    let style = StyleProperties { line_height: LineHeight::Px(16.0), ..base_style() };
    let l = layout_sc("a", style, UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    let (_, h) = first_cluster(&l);
    assert_px(h, 16.0);
}

#[test]
fn line_height_controls_advance_between_wrapped_lines() {
    // §10.8: successive line boxes advance by the line height. With line-height 30px
    // on both the run and the strut, wrapped line 1 sits 30px below line 0.
    let style = StyleProperties { line_height: LineHeight::Px(30.0), ..base_style() };
    let l = layout_sc("aaaa aaaa", style, UnifiedConstraints {
        available_width: AvailableSpace::Definite(60.0),
        line_height: LineHeight::Px(30.0),
        strut_ascent: 16.0,
        strut_descent: 4.0,
        ..UnifiedConstraints::default()
    });
    // byte 0 is on line 0, byte 5 (2nd word) on line 1.
    assert_px(y_of(&l, 5) - y_of(&l, 0), 30.0);
}

// ===========================================================================
// vertical-align (CSS Inline §4 / CSS 2.2 §10.8.1)
// ===========================================================================

#[test]
fn vertical_align_baseline_leaves_cluster_at_top() {
    // Baseline alignment (default): a single dominant run's box top sits at the line
    // top, so position.y = 0.
    let l = layout_sc("a", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        vertical_align: VerticalAlign::Baseline,
        ..UnifiedConstraints::default()
    });
    assert_px(y_of(&l, 0), 0.0);
}

#[test]
fn vertical_align_super_raises_cluster() {
    // §10.8.1: vertical-align:super raises the box by ~0.4 * line-ascent. With a 16px
    // ascent that is 6.4px up, so the box top moves to y = -6.4.
    let l = layout_sc("a", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        vertical_align: VerticalAlign::Super,
        ..UnifiedConstraints::default()
    });
    assert_px(y_of(&l, 0), -6.4);
}

#[test]
fn vertical_align_sub_lowers_cluster() {
    // §10.8.1: vertical-align:sub lowers the box by ~0.3 * line-ascent = 4.8px, so the
    // box top moves to y = 4.8.
    let l = layout_sc("a", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        vertical_align: VerticalAlign::Sub,
        ..UnifiedConstraints::default()
    });
    assert_px(y_of(&l, 0), 4.8);
}

// ===========================================================================
// Baseline sharing + font-driven line box
// ===========================================================================

#[test]
fn different_font_sizes_share_the_alphabetic_baseline() {
    // CSS Inline §4: baseline-aligned runs of different sizes share one baseline. On a
    // line with 'a'@20 (ascent 16) and 'a'@10 (ascent 8), the smaller run drops 8px so
    // its baseline meets the larger run's: y(run0)=0, y(run1)=8 (16-8).
    let font_ref = fake_font_ref();
    let big = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()), font_size_px: 20.0, ..StyleProperties::default()
    });
    let small = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()), font_size_px: 10.0, ..StyleProperties::default()
    });
    let content = vec![
        InlineContent::Text(StyledRun { text: "a".into(), style: big, logical_start_byte: 0, source_node_id: None }),
        InlineContent::Text(StyledRun { text: "a".into(), style: small, logical_start_byte: 1, source_node_id: None }),
    ];
    let l = layout_multi(&content, UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    assert_px(y_of_run(&l, 0), 0.0); // 20px run box top at line top
    assert_px(y_of_run(&l, 1), 8.0); // 10px run drops 8px so baselines meet at y=16
}

#[test]
fn cjk_line_box_uses_font_metrics_not_glyph_bbox() {
    // §10.8: the line box height comes from the FONT's ascent/descent, not a glyph's
    // bounding box. '你' has a tall bbox (-100..800) but the box height is still the
    // font's 20px, and its advance is the full-width 1000u => 20px.
    let l = layout_sc("你", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent, ..UnifiedConstraints::default()
    });
    let it = l.items.iter().find(|it| matches!(it.item, ShapedItem::Cluster(_))).expect("cluster");
    assert_px(it.item.bounds().height, 20.0);
    if let ShapedItem::Cluster(c) = &it.item {
        assert_px(c.advance, 20.0);
    }
}

#[test]
fn tall_strut_sets_the_minimum_line_box() {
    // §10.8.1 strut: the line box is at least the strut's A+D. A 30/10 strut (40px)
    // is taller than the 20px glyph, so the baseline drops to 30 and the small glyph
    // box top sits at y = 30 - 16 = 14.
    let l = layout_sc("a", base_style(), UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        strut_ascent: 30.0,
        strut_descent: 10.0,
        ..UnifiedConstraints::default()
    });
    assert_px(y_of(&l, 0), 14.0);
}
