//! K30c differential gate (design doc §6.1): on the corpus subset where
//! post-hoc slicing is PROVABLY correct — plain block stacks, nested
//! wrappers, zero margins, no floats/avoid/tables/sequences, no text
//! straddling a boundary, and EXACT-FILL geometry (block boundaries
//! coincide with interval break positions — plain slicing cuts mid-block
//! otherwise, while tokens are block-atomic by construction) — the token
//! engine's GENERATED pages must show the same content geometry as the
//! slicer's CUT pages.
//!
//! Comparison is content-level (marker structure differs BY DESIGN: the
//! slicer re-derives Push/Pop chains post-hoc (E17) while token pages
//! emit them naturally): same content-item kind sequence, bounds within
//! 0.75px. Inline (mid-paragraph) tokens are K31+ — text fixtures here
//! keep paragraphs page-atomic.

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};
use azul_layout::solver3::paged_layout::{
    layout_document_paged_with_config, layout_document_tokenized,
};
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::{BTreeMap, HashMap};

fn fresh_cache() -> Solver3LayoutCache {
    Solver3LayoutCache {
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
        ..Default::default()
        },
        ..Default::default()
    }
}

fn run_slicer(html: &str, page_h: f32) -> Vec<DisplayList> {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, page_h),
    };
    let rr = RendererResources::default();
    let mut dbg = None;
    let loader = PathLoader::new();
    layout_document_paged_with_config(
        &mut cache,
        &mut text_cache,
        FragmentationContext::new_paged(LogicalSize::new(800.0, page_h)),
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut dbg,
        None,
        &rr,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("slicer pages")
}

fn run_tokens(html: &str, page_h: f32) -> Vec<DisplayList> {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, page_h),
    };
    let rr = RendererResources::default();
    let mut dbg = None;
    let loader = PathLoader::new();
    layout_document_tokenized(
        &mut cache,
        &mut text_cache,
        &styled_dom,
        viewport,
        &mut font_manager,
        &mut dbg,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        &rr,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        page_h,
        16,
    )
    .expect("token pages")
    .into_iter()
    .map(|p| p.display_list)
    .collect()
}

/// Content-item digest: (kind tag, rounded bounds). Markers and zero-size
/// items are structure, not content.
fn content_digest(dl: &DisplayList) -> Vec<(&'static str, i64, i64, i64, i64)> {
    dl.items
        .iter()
        .filter_map(|item| {
            if item.is_push_marker() || item.is_pop_marker() {
                return None;
            }
            let kind = match item {
                DisplayListItem::Rect { .. } => "rect",
                DisplayListItem::Border { .. } => "border",
                DisplayListItem::Text { .. } => "text",
                DisplayListItem::TextLayout { .. } => "textlayout",
                DisplayListItem::Image { .. } => "image",
                _ => return None,
            };
            let b = item.visual_bounds()?;
            if b.size.width <= 0.0 || b.size.height <= 0.0 {
                return None;
            }
            // 0.75px tolerance via quantization to 1.5px cells
            let q = |v: f32| (v / 1.5).round() as i64;
            Some((
                kind,
                q(b.origin.x),
                q(b.origin.y),
                q(b.size.width),
                q(b.size.height),
            ))
        })
        .collect()
}

fn assert_differential(html: &str, page_h: f32, name: &str) {
    let slicer = run_slicer(html, page_h);
    let tokens = run_tokens(html, page_h);
    assert_eq!(
        slicer.len(),
        tokens.len(),
        "{name}: page count must agree (slicer {} vs tokens {})",
        slicer.len(),
        tokens.len()
    );
    for (i, (s, t)) in slicer.iter().zip(tokens.iter()).enumerate() {
        let ds = content_digest(s);
        let dt = content_digest(t);
        assert_eq!(
            ds, dt,
            "{name}: page {i} content diverges\nslicer: {ds:#?}\ntokens: {dt:#?}"
        );
    }
}

#[test]
fn flat_block_stack_pages_are_identical() {
    assert_differential(
        r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; background: #ddd; }
    </style></head>
    <body>
        <div class="p">a</div><div class="p">b</div><div class="p">c</div>
        <div class="p">d</div><div class="p">e</div><div class="p">f</div>
    </body></html>"#,
        300.0,
        "flat",
    );
}

#[test]
fn nested_wrapper_pages_are_identical() {
    assert_differential(
        r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; background: #cce; }
        .wrap { display: block; }
    </style></head>
    <body>
        <div class="wrap">
            <div class="p">a</div><div class="p">b</div><div class="p">c</div>
            <div class="p">d</div><div class="p">e</div><div class="p">f</div>
        </div>
    </body></html>"#,
        300.0,
        "nested",
    );
}

#[test]
fn page_atomic_paragraph_pages_are_identical() {
    // Text INSIDE page-atomic blocks (no paragraph straddles a boundary —
    // inline tokens are K31+). Each block: fixed 190px, one short line.
    assert_differential(
        r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        body { font-size: 14px; }
        .p { height: 200px; background: #eee; }
    </style></head>
    <body>
        <div class="p">alpha</div><div class="p">beta</div>
        <div class="p">gamma</div><div class="p">delta</div>
    </body></html>"#,
        200.0,
        "text",
    );
}
