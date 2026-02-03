/// Test for flex container text width bug
/// 
/// Bug: Text inside a flex item gets width=0 because MinContent sizing
/// passes available_width=0 to the text layout engine, causing text to
/// wrap after every character.
///
/// Expected: Text should measure its intrinsic min-content width (widest word)
/// and the flex item should have non-zero width.
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

/// Test that text in a flex row container gets proper width
/// 
/// This is the minimal reproduction case for the bug where:
/// - Body has display: flex, flex-direction: column
/// - Titlebar div has display: flex, flex-direction: row, align-self: stretch
/// - Text inside titlebar gets width: 0
#[test]
fn test_flex_column_child_text_has_nonzero_width() {
    let html = r#"
    <html>
        <head>
            <style>
                body {
                    display: flex;
                    flex-direction: column;
                }
                .titlebar {
                    display: flex;
                    flex-direction: row;
                    align-self: stretch;
                    height: 32px;
                }
                .title-container {
                    flex-grow: 1;
                }
            </style>
        </head>
        <body>
            <div class="titlebar">
                <div class="title-container">Hello World Title</div>
            </div>
            <div class="content">
                <p>Content area</p>
            </div>
        </body>
    </html>
    "#;

    let styled_dom = Dom::from_xml_string(html);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");

    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
        subtree_layout_cache: BTreeMap::new(),
    };
    let mut text_cache = TextLayoutCache::new();

    let content_size = LogicalSize::new(400.0, 300.0);
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

    let display_lists = layout_document_paged_with_config(
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

    // Get the layout tree to inspect node sizes
    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");

    // Find the titlebar node (should be first child of body which is node 0)
    // Node structure:
    // 0: body (flex column)
    // 1: div.titlebar (flex row)
    // 2: div.title-container
    // 3: text "Hello World Title"
    // 4: div.content
    // ...

    // Check titlebar (node 1) has non-zero width
    let titlebar = tree.get(1).expect("Titlebar node should exist");
    let titlebar_size = titlebar.used_size.expect("Titlebar should have used_size");
    
    println!("Titlebar size: {:?}", titlebar_size);
    
    // BUG: This currently fails because titlebar gets width=0
    // The titlebar should stretch to fill the body width (around 400px minus margins)
    assert!(
        titlebar_size.width > 100.0,
        "Titlebar should have significant width due to align-self: stretch, got: {}",
        titlebar_size.width
    );

    // Check title-container (node 2) has non-zero width
    let title_container = tree.get(2).expect("Title container node should exist");
    let container_size = title_container.used_size.expect("Container should have used_size");
    
    println!("Title container size: {:?}", container_size);
    
    // BUG: This currently fails because the container gets width=0
    // With flex-grow: 1, it should expand to fill available space
    assert!(
        container_size.width > 50.0,
        "Title container should have width from flex-grow: 1, got: {}",
        container_size.width
    );
}

/// Simpler test: just a flex row with text
#[test]
fn test_flex_row_text_child_has_intrinsic_width() {
    let html = r#"
    <html>
        <head>
            <style>
                .container {
                    display: flex;
                    flex-direction: row;
                }
            </style>
        </head>
        <body>
            <div class="container">
                <div>Hello World</div>
            </div>
        </body>
    </html>
    "#;

    let styled_dom = Dom::from_xml_string(html);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");

    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
        subtree_layout_cache: BTreeMap::new(),
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

    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");

    // Debug: print all nodes
    for i in 0..10 {
        if let Some(node) = tree.get(i) {
            let size = node.used_size.unwrap_or_default();
            let intrinsic = node.intrinsic_sizes.unwrap_or_default();
            println!("Node {}: size={:?}, intrinsic_min_w={}, intrinsic_max_w={}, fc={:?}", 
                i, size, intrinsic.min_content_width, intrinsic.max_content_width, node.formatting_context);
        }
    }

    // The tree structure varies based on text wrapping nodes.
    // Node 3 = div.container (Flex)
    // Node 6 = div containing "Hello World" (Block)
    // Find the node that contains "Hello World" by looking for the Block node
    // with intrinsic width > 100 (the text div)
    let text_div_idx = (0..10)
        .find(|&i| {
            tree.get(i)
                .map(|n| {
                    matches!(n.formatting_context, azul_core::dom::FormattingContext::Block { .. })
                    && n.intrinsic_sizes.map(|s| s.min_content_width > 100.0).unwrap_or(false)
                    && n.used_size.map(|s| s.width > 0.0).unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .expect("Should find text div with non-zero width");

    let text_div = tree.get(text_div_idx).expect("Text div should exist");
    let text_div_size = text_div.used_size.expect("Text div should have used_size");
    
    println!("Text div size: {:?}", text_div_size);
    
    // The div should have width equal to the text's intrinsic width
    // "Hello World" in a typical font is around 60-80px wide
    assert!(
        text_div_size.width > 20.0,
        "Text div should have intrinsic width from text content, got: {}",
        text_div_size.width
    );
}
