#![cfg(feature = "text_layout")]
//! Deterministic full-document (DOM -> solver3 -> positioned boxes) regression
//! tests on the fake fonts (`common/fakefont.rs`), injected into the fontconfig
//! cache via `with_memory_fonts` (NO system fonts). Mirrors the harness of
//! `text3_brutal_solver3.rs`.
//!
//! Fake metrics @ size 20: 'a' 600u => 12px · 'A' 700u => 14px · space 5px ·
//! line-height normal 20px. FakeFallback covers Greek α/β/γ/δ + '#' at 16px.

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

use fakefont::{simple_fallback_font, simple_test_font, FAKE_FALLBACK_FAMILY, FAKE_FAMILY};

/// Deterministic built-in mock font (auto-registered by `FontManager::new`):
/// every glyph, space included, advances 0.5 em = 10px @ font-size 20;
/// line-height:normal = 20px. Monospace, so spacing/fallback maths are exact.
const MONO: &str = "Azul Mock Mono";

/// On-disk proportional stress font (`tests/fonts/azul-mock-prop.ttf`),
/// registered by [`run_layout`]. Advances @ font-size 20: 'i'/'l'/'.'/',' =
/// 200u = 4px, 'M'/'W'/'m'/'w' = 900u = 18px, space = 250u = 5px, every other
/// printable ASCII = 500u = 10px. Its case pairs 'I'(10px)/'i'(4px) differ, so
/// a text-transform actually changes the measured width (a monospace mock could
/// not detect the transform at all).
const PROP: &str = "Azul Mock Prop";

const EPS: f32 = 0.05;

/// An `FcFontCache` with the primary fake font (under `FakeTest` + generic
/// fallbacks) AND the disjoint `FakeFallback` face registered separately.
fn fake_font_cache() -> FcFontCache {
    let bytes = simple_test_font();
    let fb = simple_fallback_font();
    let fc_cache = FcFontCache::default();
    fc_cache.with_memory_fonts(vec![
        (
            FcPattern { name: Some(FAKE_FAMILY.to_string()), family: Some(FAKE_FAMILY.to_string()), ..Default::default() },
            FcFont { bytes: bytes.clone(), font_index: 0, id: "faketest".to_string() },
        ),
        (
            FcPattern { name: Some("serif sans-serif monospace".to_string()), family: Some("serif sans-serif monospace".to_string()), ..Default::default() },
            FcFont { bytes, font_index: 0, id: "faketest_fallback".to_string() },
        ),
        (
            FcPattern { name: Some(FAKE_FALLBACK_FAMILY.to_string()), family: Some(FAKE_FALLBACK_FAMILY.to_string()), ..Default::default() },
            FcFont { bytes: fb, font_index: 0, id: "fakefallback".to_string() },
        ),
    ]);
    fc_cache
}

/// Absolute path to `layout/tests/fonts/<name>`.
fn font_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts")).join(name)
}

fn run_layout(html: &str) -> Solver3LayoutCache {
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager = FontManager::new(fake_font_cache()).expect("font manager");
    // Register the on-disk proportional stress font so text-transform tests can
    // assert a REAL width change (its case pairs differ in advance).
    let prop_bytes = std::fs::read(font_path("azul-mock-prop.ttf"))
        .unwrap_or_else(|e| panic!("read azul-mock-prop.ttf: {e}"));
    font_manager.register_named_font(
        PROP,
        &prop_bytes,
        vec![rust_fontconfig::UnicodeRange { start: 0x20, end: 0x7E }],
    );
    // Register the disjoint fallback face with EXPLICIT Greek coverage so the
    // font-chain resolver can find it for a codepoint the primary lacks. (The
    // bare `with_memory_fonts` entry in `fake_font_cache` carries no coverage
    // ranges, so it is not a reliable fallback candidate on its own.)
    font_manager.register_named_font(
        FAKE_FALLBACK_FAMILY,
        &simple_fallback_font(),
        vec![
            rust_fontconfig::UnicodeRange { start: 0x0023, end: 0x0023 }, // '#'
            rust_fontconfig::UnicodeRange { start: 0x03B1, end: 0x03B4 }, // α..δ
        ],
    );

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
        prev_viewport: LogicalRect { origin: LogicalPosition::zero(), size: LogicalSize::zero() },
    };
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: content_size };
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    )
    .expect("Layout should succeed");

    layout_cache
}

fn intrinsics(cache: &Solver3LayoutCache) -> Vec<(f32, f32)> {
    let tree = cache.tree.as_ref().expect("layout tree");
    (0..64)
        .filter_map(|i| tree.warm(i).and_then(|w| w.intrinsic_sizes))
        .map(|s| (s.min_content_width, s.max_content_width))
        .collect()
}

fn used_sizes(cache: &Solver3LayoutCache) -> Vec<(f32, f32)> {
    let tree = cache.tree.as_ref().expect("layout tree");
    (0..64)
        .filter_map(|i| tree.get(i).and_then(|n| n.used_size))
        .map(|s| (s.width, s.height))
        .collect()
}

fn any_max_content(cache: &Solver3LayoutCache, expected: f32) -> bool {
    intrinsics(cache).iter().any(|(_, mx)| (mx - expected).abs() <= EPS)
}

fn has_box(cache: &Solver3LayoutCache, w: f32, h: f32) -> bool {
    used_sizes(cache).iter().any(|(bw, bh)| (bw - w).abs() <= EPS && (bh - h).abs() <= EPS)
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn text_transform_uppercase_widens_to_56() {
    // CSS Text §2.1: text-transform:uppercase maps "aaaa" (48px) to "AAAA"
    // (4 * 14px = 56px) BEFORE shaping, in the DOM layer.
    let html = format!(
        "<html><head><style>.t {{ display: inline-block; text-transform: uppercase; \
            font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"t\">aaaa</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(any_max_content(&cache, 56.0), "uppercase 'AAAA' = 56px; intrinsics = {:?}", intrinsics(&cache));
}

#[test]
fn text_transform_lowercase_narrows_to_16() {
    // CSS Text §2.1: text-transform:lowercase maps "IIII" to "iiii" BEFORE shaping.
    // In Azul Mock Prop, 'I' advances 500u (10px) but 'i' advances 200u (4px), so
    // the transform NARROWS the run from 40px to 4 * 4 = 16px. (A monospace mock
    // could not reveal the transform, since 'I' and 'i' would share a width.)
    let html = format!(
        "<html><head><style>.t {{ display: inline-block; text-transform: lowercase; \
            font-size: 20px; font-family: '{PROP}'; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"t\">IIII</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(any_max_content(&cache, 16.0), "lowercase 'iiii' = 16px; intrinsics = {:?}", intrinsics(&cache));
}

#[test]
fn fixed_width_wraps_three_words_to_two_lines() {
    // CSS Text/UAX#14: "aaaa aaaa aaaa" (154px) in a 110px box fits "aaaa aaaa"
    // (101px) on line 0 and wraps the third word, auto-heighting to 40px.
    let html = format!(
        "<html><head><style>.b {{ width: 110px; font-size: 20px; font-family: {FAKE_FAMILY}; \
            margin: 0; padding: 0; }}</style></head><body><div class=\"b\">aaaa aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(has_box(&cache, 110.0, 40.0), "expected 110x40 (2 line boxes); used sizes = {:?}", used_sizes(&cache));
}

#[test]
fn white_space_nowrap_keeps_one_line_box() {
    // CSS Text §3: white-space:nowrap keeps "aaaa aaaa" on one 20px line even in a
    // 60px box (it overflows horizontally); the block is 60x20.
    let html = format!(
        "<html><head><style>.b {{ width: 60px; white-space: nowrap; font-size: 20px; \
            font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(has_box(&cache, 60.0, 20.0), "nowrap => single 20px line; used sizes = {:?}", used_sizes(&cache));
}

#[test]
fn br_forces_second_line_box() {
    // CSS Text / HTML <br>: a forced break splits "aaaa" and "aaaa" onto two lines,
    // so a 200px block auto-heights to 40px.
    let html = format!(
        "<html><head><style>.b {{ width: 200px; font-size: 20px; font-family: {FAKE_FAMILY}; \
            margin: 0; padding: 0; }}</style></head><body><div class=\"b\">aaaa<br/>aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(has_box(&cache, 200.0, 40.0), "expected 200x40 (<br> => 2 lines); used sizes = {:?}", used_sizes(&cache));
}

#[test]
fn word_spacing_widens_max_content() {
    // CSS Text §8: word-spacing:3px adds 3px at the inter-word space, so "aa aa"
    // (50px in Azul Mock Mono: 20 + 10 + 20) measures 53px at max-content.
    let html = format!(
        "<html><head><style>.b {{ display: inline-block; word-spacing: 3px; font-size: 20px; \
            font-family: '{MONO}'; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"b\">aa aa</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(any_max_content(&cache, 53.0), "word-spacing 3px => 53px; intrinsics = {:?}", intrinsics(&cache));
}

#[test]
fn letter_spacing_widens_max_content_after_every_cluster() {
    // CSS Text §8: letter-spacing:2px adds after every cluster of "aaaa" (4 clusters):
    // 40 (Azul Mock Mono, 4*10px) + 4*2 = 48px at max-content.
    let html = format!(
        "<html><head><style>.b {{ display: inline-block; letter-spacing: 2px; font-size: 20px; \
            font-family: '{MONO}'; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"b\">aaaa</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(any_max_content(&cache, 48.0), "letter-spacing 2px * 4 clusters => 48px; intrinsics = {:?}", intrinsics(&cache));
}

#[test]
fn border_reduces_content_width_like_padding() {
    // CSS box model: a 70px border-box with 5px borders leaves 60px of content, so
    // "aaaa aaaa" wraps to 2 lines; the border box is 70 wide and 40 + 2*5 = 50 tall.
    let html = format!(
        "<html><head><style>.b {{ width: 70px; border: 5px solid; box-sizing: border-box; \
            font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"b\">aaaa aaaa</div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(has_box(&cache, 70.0, 50.0), "expected 70x50 border box; used sizes = {:?}", used_sizes(&cache));
}

#[test]
fn two_block_divs_stack_to_double_height() {
    // CSS block flow: two block <div>s each hold one 20px line, so their 100px-wide
    // container auto-heights to 40px.
    let html = format!(
        "<html><head><style>.wrap {{ width: 100px; margin: 0; padding: 0; }}\
            .wrap div {{ font-size: 20px; font-family: {FAKE_FAMILY}; margin: 0; padding: 0; }}\
         </style></head><body><div class=\"wrap\"><div>aaaa</div><div>aaaa</div></div></body></html>"
    );
    let cache = run_layout(&html);
    assert!(has_box(&cache, 100.0, 40.0), "two stacked 20px divs => 100x40; used sizes = {:?}", used_sizes(&cache));
}

#[test]
fn font_fallback_covers_uncovered_codepoint() {
    // CSS Fonts §5: a codepoint absent from the primary face (Greek 'α' is not in
    // Azul Mock Mono, which covers ASCII only) is resolved through the CSS
    // font-family stack to the next face that covers it — FakeFallback (α = 800u
    // = 16px). So "aα" measures 10 ('a' in Mock Mono) + 16 (α in FakeFallback) =
    // 26px — NOT 10 + 10 (a .notdef box) = 20px. Both faces are deterministic
    // (a registered mock + a disjoint in-memory font), and the 16px α width is
    // reachable from no other font, so it is unmistakably from the fallback.
    let html = format!(
        "<html><head><style>.b {{ display: inline-block; font-size: 20px; \
            font-family: '{MONO}', '{FAKE_FALLBACK_FAMILY}'; margin: 0; padding: 0; }}\
         </style></head><body><span class=\"b\">a\u{03B1}</span></body></html>"
    );
    let cache = run_layout(&html);
    assert!(any_max_content(&cache, 26.0), "fallback 'α' (16px) => 26px total; intrinsics = {:?}", intrinsics(&cache));
}
