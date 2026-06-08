/// Regression: the MapWidget must FILL its container. The widget's outer div had
/// no size, so it collapsed to zero height → the VirtualView got zero bounds →
/// no tiles rendered (the azul-maps demo showed only the container background).
/// build_dom now gives the outer div + VirtualView `width/height:100%`.
use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::Solver3LayoutCache;
use std::collections::{BTreeMap, HashMap};

fn fresh_layout_cache() -> Solver3LayoutCache {
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
        prev_viewport: LogicalRect { origin: LogicalPosition::zero(), size: LogicalSize::zero() },
    }
}

/// The azul-maps demo wraps the map in a FLEX container (`MAP_CONTAINER` is
/// `flex-grow:1`). This checks whether `height:100%` resolves through that flex
/// item — if the map widget div collapses here, the demo's white map is a
/// flex+percentage resolution issue (not the VirtualView).
#[test]
fn test_map_widget_fills_flex_container() {
    use azul_core::styled_dom::StyledDom;
    use azul_layout::widgets::map::{MapTileLayer, MapWidget};

    // EXACT azul-maps structure: create_body + ROOT(flex-col,height:100%) →
    // [header(flex-shrink:0), MAP_CONTAINER(flex-grow:1,relative)] → map widget.
    let map = MapWidget::create(MapTileLayer::default()).dom();
    let map_area = Dom::create_div()
        .with_css("flex-grow: 1; position: relative; background: #cbd2d8; overflow: hidden;")
        .with_child(map);
    let header = Dom::create_div().with_css("background: #2b2b2b; padding: 10px 16px; flex-shrink: 0;")
        .with_child(Dom::create_text("AzulMaps"));
    let body = Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100%;")
        .with_child(header)
        .with_child(map_area);
    let styled_dom = StyledDom::create_from_dom(body);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut layout_cache = fresh_layout_cache();
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: content_size };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let content_size2 = LogicalSize::new(640.0, 480.0);
    let viewport2 = LogicalRect { origin: LogicalPosition::zero(), size: content_size2 };
    layout_document_paged_with_config(
        &mut layout_cache, &mut text_cache, FragmentationContext::new_paged(content_size2),
        &styled_dom, viewport2, &mut font_manager, &BTreeMap::new(), &mut debug_messages, None,
        &renderer_resources, azul_core::resources::IdNamespace(0), DomId::ROOT_ID, font_loader,
        FakePageConfig::new(), &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    ).expect("layout");
    let tree = layout_cache.tree.as_ref().expect("tree");
    println!("\n=== FULL CHAIN (640x480, body→header→map_area→mapdiv→vview) ===");
    for i in 0..8 {
        if let Some(n) = tree.get(i) {
            let s = n.used_size.unwrap_or_default();
            let nt = n.dom_node_id;
            println!("Node {}: fc={:?} size=({:.1},{:.1}) dom={:?}", i, n.formatting_context, s.width, s.height, nt);
        }
    }
    // Diagnostic only — print, don't assert, so we always see the chain.
    let map_div_h = tree.get(3).and_then(|n| n.used_size).map(|s| s.height).unwrap_or(0.0);
    println!("=> map widget div height = {:.1} (expect ~440 if chain resolves)", map_div_h);
}

#[test]
fn test_map_widget_fills_container() {
    use azul_core::styled_dom::StyledDom;
    use azul_layout::widgets::map::{MapTileLayer, MapWidget};

    // A DEFINITE-size map area (the azul-maps demo uses flex-grow; here we pin it so
    // the paged test harness — which resolves percentages against an infinite
    // containing block during intrinsic sizing — stays bounded). The widget's
    // width/height:100% must resolve to fill this 600x400 box, not collapse to 0.
    let map = MapWidget::create(MapTileLayer::default()).dom();
    let map_area = Dom::create_div()
        .with_css("width: 600px; height: 400px; position: relative; background: #cbd2d8; overflow: hidden;")
        .with_child(map);
    let body = Dom::create_div()
        .with_css("width: 800px; height: 600px;")
        .with_child(map_area);
    let styled_dom = StyledDom::create_from_dom(body);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("font manager");
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
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: content_size };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };

    layout_document_paged_with_config(
        &mut layout_cache, &mut text_cache, FragmentationContext::new_paged(content_size),
        &styled_dom, viewport, &mut font_manager, &BTreeMap::new(), &mut debug_messages, None,
        &renderer_resources, azul_core::resources::IdNamespace(0), DomId::ROOT_ID, font_loader,
        FakePageConfig::new(), &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    ).expect("layout");

    let tree = layout_cache.tree.as_ref().expect("tree");
    println!("\n=== MAP WIDGET TREE ===");
    let mut max_inner_h = 0.0f32;
    for i in 0..10 {
        if let Some(node) = tree.get(i) {
            let s = node.used_size.unwrap_or_default();
            println!("Node {}: fc={:?} size=({:.1},{:.1})", i, node.formatting_context, s.width, s.height);
            // nodes 2+ are the map widget div + virtual view (inside the 400px area)
            if i >= 2 { max_inner_h = max_inner_h.max(s.height); }
        }
    }
    // The map widget div + VirtualView must fill the 400px map area, not collapse to 0.
    assert!(
        max_inner_h > 350.0,
        "map widget collapsed (max inner height {:.1}); expected it to fill the 400px container",
        max_inner_h
    );
}
