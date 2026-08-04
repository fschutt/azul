//! The break-Y -> DOM-path keystone of the DOM-materialized-breaks editor
//! architecture (pdf2html AZUL-STILL-TODO section A):
//!
//! - `compute_document_pagination` estimates break Y coordinates without
//!   materializing pages, leaving tree + positions in the caller's cache.
//! - `pagination_to_dom_breaks` maps every break to the child-index path of
//!   the first block at/after it (the spine the cut runs along) so the
//!   application can insert its break nodes at DOM positions.
//! - Forced breaks carry `causing_node` end to end (display-list recording
//!   through `PageBreakPosition`).

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::{compute_document_pagination, pagination_to_dom_breaks};
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::{BreakKind, Solver3LayoutCache};
use std::collections::{BTreeMap, HashMap};

fn paginate(
    html: &str,
    w: f32,
    h: f32,
) -> (
    Solver3LayoutCache,
    azul_core::styled_dom::StyledDom,
    azul_layout::PaginationInfo,
) {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
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
    let content_size = LogicalSize::new(w, h);
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
    let pagination = compute_document_pagination(
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
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    )
    .expect("pagination should succeed");
    (layout_cache, styled_dom, pagination)
}

#[test]
fn interval_breaks_map_to_dom_paths() {
    // Three 150px blocks on 200px pages: breaks fall inside block 2 and
    // block 3's territory; each break must address a real block by path.
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="p">two</div>
        <div class="p">three</div>
    </body></html>"#;
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 200.0);
    assert!(
        pagination.page_count >= 2,
        "450px of content on 200px pages must span pages, got {}",
        pagination.page_count
    );
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination)
        .expect("cache holds tree+positions right after compute_document_pagination");
    assert_eq!(breaks.len(), pagination.breaks.len());
    for b in &breaks {
        assert_eq!(b.kind, BreakKind::Interval);
    }
    // A break with a block-level box at/after it maps to that box's path;
    // a break inside the LAST block (no following block) maps to None —
    // there is no "insert before this node" target, the content tail stays
    // on its page. With 150px blocks at y 0/150/300 and 200px pages:
    // break y=200 -> block 3 (top 300), break y=400 -> None (tail).
    assert!(
        breaks[0].path.is_some(),
        "a break followed by more blocks must map to a spine path: {:?}",
        breaks[0]
    );
    if let Some(last) = breaks.last() {
        if last.y >= 400.0 {
            assert!(
                last.path.is_none(),
                "a break inside the final block maps to None (tail semantics): {last:?}"
            );
        }
    }
    // Present paths address blocks in document order.
    let paths: Vec<_> = breaks.iter().filter_map(|b| b.path.clone()).collect();
    assert!(
        paths.windows(2).all(|w| w[0] <= w[1]),
        "spine paths must be document-ordered: {paths:?}"
    );
}

#[test]
fn forced_break_carries_its_causing_node() {
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 50px; }
        .brk { break-before: page; height: 50px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="brk">two</div>
    </body></html>"#;
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 400.0);
    let forced: Vec<_> = pagination
        .breaks
        .iter()
        .filter(|b| b.kind == BreakKind::Forced)
        .collect();
    assert_eq!(
        forced.len(),
        1,
        "break-before: page must yield exactly one forced break: {:?}",
        pagination.breaks
    );
    let causing = forced[0]
        .causing_node
        .expect("forced break must carry the node whose break property caused it");
    let node_data = &styled_dom.node_data.as_container()[causing];
    assert!(
        format!("{:?}", node_data.get_node_type()).contains("Div"),
        "causing node should be the .brk div, got {:?}",
        node_data.get_node_type()
    );
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination).expect("mapped");
    let fb = breaks.iter().find(|b| b.kind == BreakKind::Forced).unwrap();
    assert_eq!(fb.causing_node, Some(causing));
    assert!(fb.path.is_some(), "forced break maps to a spine path too");
}
