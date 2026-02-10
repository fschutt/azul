/// Test body margin with vh units (like example.com)
/// Verifies that margin: 15vh auto on body positions body correctly
use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::BTreeMap;

/// Test case from example.com: body { margin: 15vh auto; }
/// With a 768px viewport height, 15vh = 115.2px
/// The body should be positioned at y=115.2 from the viewport top
#[test]
fn test_body_margin_15vh_auto() {
    let html = r#"
    <html>
        <head>
            <style>
                html {
                    background: #f0f0f2;
                }
                body {
                    background-color: #ffffff;
                    margin: 15vh auto;
                    max-width: 660px;
                    padding: 45px;
                }
                div {
                    background: red;
                    height: 50px;
                }
            </style>
        </head>
        <body>
            <div>Content</div>
        </body>
    </html>
    "#;

    let styled_dom = Dom::from_xml_string(html);

    // Create font cache and font manager
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");

    // Create layout cache and text cache
    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
            cache_map: Default::default(),
    };
    let mut text_cache = TextLayoutCache::new();

    // Use viewport 1024x768 like the debug output
    let content_size = LogicalSize::new(1024.0, 768.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);

    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };

    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());

    let loader = PathLoader::new();
    let font_loader = |bytes: &[u8], index: usize| loader.load_font(bytes, index);
    let page_config = FakePageConfig::new();

    let _display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
    )
    .expect("Layout should succeed");

    // Get body position from layout cache
    // Body should be at node index 1 (html is 0, body is 1)
    let body_position = layout_cache.calculated_positions.get(&1);
    
    println!("Layout cache positions:");
    for (id, pos) in &layout_cache.calculated_positions {
        println!("  Node {}: {:?}", id, pos);
    }
    
    // Expected: 15% of 768px = 115.2px
    let expected_body_y = 768.0 * 0.15;
    println!("Expected body Y: {:.2}", expected_body_y);
    
    if let Some(pos) = body_position {
        println!("Actual body Y: {:.2}", pos.y);
        
        // Allow small floating-point tolerance
        let tolerance = 1.0;
        assert!(
            (pos.y - expected_body_y).abs() < tolerance,
            "Body margin-top should be ~{:.2}px (15vh), but got {:.2}px. \
             This might indicate margin is being applied twice!",
            expected_body_y,
            pos.y
        );
    } else {
        panic!("Body position not found in layout cache");
    }
}
