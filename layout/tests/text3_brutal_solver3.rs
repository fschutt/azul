#![cfg(feature = "text_layout")]
//! Brutal, deterministic full-document tests: DOM -> solver3 -> positioned
//! boxes, driven by the synthetic fake font (`common/fakefont.rs`) injected
//! into the fontconfig cache via `with_memory_fonts` (NO system fonts).
//!
//! Harness mirrors `whitespace_processing.rs` / `flex_text_width_bug.rs`
//! (`layout_document_paged_with_config`), but swaps `build_font_cache()` for an
//! in-memory `FakeTest` face so every intrinsic size and box height is a fixed
//! function of the fake metrics.
//!
//! Fake metrics @ font-size 20 (scale 0.02): 'a' 600u => 12px, space 250u =>
//! 5px, line-height:normal = (800+200)*0.02 = 20px. So "aaaa" = 48px, the
//! inter-word space = 5px, and "aaaa aaaa" max-content = 101px, min-content 48px.

#[path = "common/fakefont.rs"]
mod fakefont;

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use rust_fontconfig::{FcFont, FcFontCache, FcPattern};
use std::collections::{BTreeMap, HashMap};

use fakefont::{simple_test_font, FAKE_FAMILY};

const EPS: f32 = 0.05;

fn assert_px(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= EPS,
        "assert_px failed: expected {expected:.4}px, got {actual:.4}px (|delta| {delta:.4}px > {EPS}px)"
    );
}

/// An `FcFontCache` containing ONLY the fake font, registered both under
/// `FakeTest` (for explicit `font-family`) and under the generic fallback
/// families so any default resolution also lands on the deterministic face.
fn fake_font_cache() -> FcFontCache {
    let bytes = simple_test_font();
    let fc_cache = FcFontCache::default();
    fc_cache.with_memory_fonts(vec![
        (
            FcPattern {
                name: Some(FAKE_FAMILY.to_string()),
                family: Some(FAKE_FAMILY.to_string()),
                ..Default::default()
            },
            FcFont {
                bytes: bytes.clone(),
                font_index: 0,
                id: "faketest".to_string(),
            },
        ),
        (
            FcPattern {
                name: Some("serif sans-serif monospace".to_string()),
                family: Some("serif sans-serif monospace".to_string()),
                ..Default::default()
            },
            FcFont {
                bytes,
                font_index: 0,
                id: "faketest_fallback".to_string(),
            },
        ),
    ]);
    fc_cache
}

/// Lay out an HTML fragment through the real paged solver on the fake font.
fn run_layout(html: &str) -> Solver3LayoutCache {
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager = FontManager::new(fake_font_cache()).expect("font manager");

    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
    };
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let page_config = FakePageConfig::new();

    let _display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("Layout should succeed");

    layout_cache
}

/// `(min_content_width, max_content_width)` for every node that computed
/// intrinsic sizes.
fn intrinsics(cache: &Solver3LayoutCache) -> Vec<(f32, f32)> {
    let tree = cache.tree.as_ref().expect("layout tree");
    (0..64)
        .filter_map(|i| tree.warm(i).and_then(|w| w.intrinsic_sizes))
        .map(|s| (s.min_content_width, s.max_content_width))
        .collect()
}

/// `(width, height)` of every node's used box.
fn used_sizes(cache: &Solver3LayoutCache) -> Vec<(f32, f32)> {
    let tree = cache.tree.as_ref().expect("layout tree");
    (0..64)
        .filter_map(|i| tree.get(i).and_then(|n| n.used_size))
        .map(|s| (s.width, s.height))
        .collect()
}

fn any_max_content(cache: &Solver3LayoutCache, expected: f32) -> bool {
    intrinsics(cache)
        .iter()
        .any(|(_, mx)| (mx - expected).abs() <= EPS)
}

fn any_min_content(cache: &Solver3LayoutCache, expected: f32) -> bool {
    intrinsics(cache)
        .iter()
        .any(|(mn, _)| (mn - expected).abs() <= EPS)
}

/// Largest `max_content_width` seen among text-bearing nodes (the text run).
fn max_max_content(cache: &Solver3LayoutCache) -> f32 {
    intrinsics(cache)
        .iter()
        .map(|(_, mx)| *mx)
        .fold(0.0, f32::max)
}

/// True if some node's used box is (≈w, ≈h).
fn has_box(cache: &Solver3LayoutCache, w: f32, h: f32) -> bool {
    used_sizes(cache)
        .iter()
        .any(|(bw, bh)| (bw - w).abs() <= EPS && (bh - h).abs() <= EPS)
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn div_60px_wraps_two_words_into_two_line_boxes() {
    // CSS Text/UAX#14: "aaaa aaaa" (101px) in a 60px box breaks at the space into
    // two 20px line boxes, so the block auto-heights to 40px.
    let html = format!(
        "<html><head><style>\
            .b {{ width: 60px; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        has_box(&cache, 60.0, 40.0),
        "expected a 60x40 block (2 line boxes); used sizes = {:?}",
        used_sizes(&cache)
    );
}

#[test]
fn inline_block_shrinks_to_max_content_101px() {
    // CSS Sizing §4: a shrink-to-fit inline-block sizes to max-content = 101px
    // ("aaaa"48 + space 5 + "aaaa"48).
    let html = format!(
        "<html><head><style>\
            .ib {{ display: inline-block; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"ib\">aaaa aaaa</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        any_max_content(&cache, 101.0),
        "expected max-content 101px; intrinsics = {:?}",
        intrinsics(&cache)
    );
}

#[test]
fn flex_item_min_content_is_widest_word_48px() {
    // CSS Sizing §4: flex min-content = widest unbreakable unit = "aaaa" = 48px.
    let html = format!(
        "<html><head><style>\
            .row {{ display: flex; flex-direction: row; font-size: 20px; font-family: {FAKE_FAMILY}; }}\
            .row * {{ margin: 0; padding: 0; }}\
         </style></head><body><div class=\"row\"><div>aaaa aaaa</div></div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        any_min_content(&cache, 48.0),
        "expected min-content 48px (widest word); intrinsics = {:?}",
        intrinsics(&cache)
    );
}

#[test]
fn letter_spacing_half_px_must_not_be_quantized_away() {
    // CSS Text §8: letter-spacing 0.5px is a real, sub-pixel contribution — on the
    // 4 clusters of "aaaa" it must widen max-content by ~2px (4 * 0.5).
    // known-suspect: getters.rs stores Spacing::Px(px.round() as i32), so 0.5px is
    // rounded to 1px and the delta inflates to ~4px. Pinning the spec value makes
    // that quantization visible as a failure.
    let base = format!(
        "<html><head><style>\
            .t {{ font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; letter-spacing: {{LS}}; }}\
         </style></head><body><span class=\"t\">aaaa</span></body></html>"
    );
    let cache0 = run_layout(&base.replace("{LS}", "0px"));
    let cache_half = run_layout(&base.replace("{LS}", "0.5px"));
    let delta = max_max_content(&cache_half) - max_max_content(&cache0);
    // pins spec: 0.5px * 4 clusters = 2.0px, NOT the quantized ~4px.
    assert_px(delta, 2.0);
}

#[test]
fn line_height_30px_makes_a_30px_line_box() {
    // CSS §10.8: an explicit line-height:30px yields a 30px line box for one line
    // of text. (Half-leading is (30-20)/2 = 5px, so the alphabetic baseline sits
    // 5 + 16 = 21px below the line-box top.)
    // FIXME(text3-review): the solver3 tree exposes box sizes, not the per-line
    // baseline, so the 21px baseline offset is not asserted here.
    let html = format!(
        "<html><head><style>\
            .b {{ width: 200px; line-height: 30px; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        has_box(&cache, 200.0, 30.0),
        "expected a 200x30 block (one 30px line box); used sizes = {:?}",
        used_sizes(&cache)
    );
}

#[test]
fn padding_reduces_content_width_and_forces_same_break() {
    // CSS box model: a 70px content-box with 5px padding leaves 60px of content,
    // so "aaaa aaaa" wraps to 2 lines exactly as the bare 60px box does. The
    // border box is then 70 wide and 40 (2 lines) + 2*5 padding = 50 tall.
    let html = format!(
        "<html><head><style>\
            .b {{ width: 70px; padding: 5px; box-sizing: border-box; font-size: 20px; \
                  font-family: {FAKE_FAMILY}; margin: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        has_box(&cache, 70.0, 50.0),
        "expected a 70x50 border box (60px content wraps to 2 lines); used sizes = {:?}",
        used_sizes(&cache)
    );
}

#[test]
fn text_indent_shifts_first_line_only() {
    // CSS Text §7: text-indent:10px offsets the first line's start edge only.
    // FIXME(text3-review): solver3 tree does not expose per-line x offsets, so we
    // only assert the block lays out without error under text-indent; the
    // first-line-only positional detail belongs to a display-list / glyph probe.
    let html = format!(
        "<html><head><style>\
            .b {{ width: 200px; text-indent: 10px; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        !cache.calculated_positions.is_empty(),
        "text-indent layout must produce positions"
    );
}

#[test]
fn white_space_pre_preserves_spaces_and_newline_without_wrapping() {
    // CSS Text §3: white-space:pre preserves the double space and the newline and
    // does NOT wrap, so "aa  aa" (58px, spaces kept) sits on line 1 and "aa" on
    // line 2 even inside a narrow 30px box; the block auto-heights to 2*20 = 40px.
    // pins spec: pre keeps collapsible spaces and honors LF as a forced break.
    let html = format!(
        "<html><head><style>\
            .p {{ white-space: pre; width: 30px; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"p\">aa  aa\naa</div></body></html>"
    );
    let cache = run_layout(&html);
    // max-content of a pre block = widest preserved line = "aa  aa" = 58px.
    assert!(
        any_max_content(&cache, 58.0),
        "pre must preserve the double space (58px line); intrinsics = {:?}",
        intrinsics(&cache)
    );
}

#[test]
fn nested_span_split_keeps_total_width_48px() {
    // CSS Text: a run split across a <span> boundary still measures as one word;
    // "aa" + <span>"aa"</span> coalesces to 48px max-content (no phantom break).
    let html = format!(
        "<html><head><style>\
            .b {{ display: inline-block; font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
            .b span {{ font-size: 20px; font-family: {FAKE_FAMILY}; }}\
         </style></head><body><span class=\"b\">aa<span>aa</span></span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        any_max_content(&cache, 48.0),
        "span-split 'aaaa' must stay 48px; intrinsics = {:?}",
        intrinsics(&cache)
    );
}

#[test]
fn font_size_zero_does_not_panic() {
    // Degenerate CSS: font-size:0 collapses text to zero advances; layout must
    // still complete without panicking.
    let html = format!(
        "<html><head><style>\
            .b {{ font-size: 0px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(
        !cache.calculated_positions.is_empty(),
        "font-size:0 layout must still produce positions"
    );
}
