/// Test inline-block text rendering
/// Verifies that text inside inline-block elements generates TextLayout / Text items
use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::display_list::DisplayListItem;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::BTreeMap;

#[test]
fn test_inline_block_text_generates_text_items() {
    let html = r#"
    <html>
        <head>
            <style>
                .box {
                    display: inline-block;
                    width: 100px;
                    height: 50px;
                    background: red;
                }
            </style>
        </head>
        <body>
            <div class="box">Hello</div>
            <div class="box">World</div>
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

    // Check that we have at least one page
    assert!(!display_lists.is_empty(), "Should have at least one page");

    // Check for TextLayout items
    let text_layout_count: usize = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter(|item| matches!(item, DisplayListItem::TextLayout { .. }))
        .count();

    // Check for Text items (glyphs)
    let text_glyph_count: usize = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter(|item| matches!(item, DisplayListItem::Text { .. }))
        .count();

    println!("TextLayout items: {}", text_layout_count);
    println!("Text (glyph) items: {}", text_glyph_count);

    // We should have text items for "Hello" and "World"
    assert!(
        text_layout_count >= 2,
        "Should have at least 2 TextLayout items for 'Hello' and 'World', got {}",
        text_layout_count
    );

    assert!(
        text_glyph_count >= 2,
        "Should have at least 2 Text items for 'Hello' and 'World', got {}",
        text_glyph_count
    );
}

#[test]
fn test_inline_block_css_width_is_applied() {
    use azul_core::dom::NodeType;
    use azul_css::props::layout::{LayoutDisplay, LayoutHeight, LayoutWidth};
    use azul_layout::solver3::getters::MultiValue;

    // This test verifies that explicit CSS width on inline-block is correctly parsed and applied
    let html = r#"
    <html>
        <head>
            <style>
                .box {
                    display: inline-block;
                    width: 150px;
                    height: 80px;
                    padding: 10px;
                    background: red;
                }
            </style>
        </head>
        <body>
            <div class="box">Box with text inside</div>
        </body>
    </html>
    "#;

    let styled_dom = Dom::from_xml_string(html);

    let node_data = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();

    // Find the div.box node (not a text node and has display: inline-block)
    let mut box_node_id = None;
    for (idx, nd) in node_data.iter().enumerate() {
        // Skip text nodes
        if matches!(nd.node_type, NodeType::Text(_)) {
            continue;
        }

        let node_id = azul_core::dom::NodeId::new(idx);
        let display =
            azul_layout::solver3::getters::get_display_property(&styled_dom, Some(node_id));
        if display == MultiValue::Exact(LayoutDisplay::InlineBlock) {
            box_node_id = Some(node_id);
            break;
        }
    }

    let box_id = box_node_id.expect("Should find a node with display: inline-block");
    let node_state = &styled_nodes[box_id].styled_node_state;

    // Now check the CSS width
    let css_width = azul_layout::solver3::getters::get_css_width(&styled_dom, box_id, node_state);

    println!("box_id = {:?}", box_id);
    println!("css_width = {:?}", css_width);

    // The width should be Exact(150px), not Auto!
    match css_width {
        MultiValue::Exact(w) => match w {
            LayoutWidth::Px(px) => {
                let px_value = px.number.get();
                assert!(
                    (px_value - 150.0).abs() < 0.01,
                    "Width should be 150px, got {}px",
                    px_value
                );
            }
            other => panic!("Width should be Px(150), got {:?}", other),
        },
        MultiValue::Auto => {
            panic!("Width should be Exact(150px), but got Auto! CSS width is not being parsed correctly.");
        }
        other => {
            panic!("Width should be Exact(150px), but got {:?}!", other);
        }
    }

    // Also check height
    let css_height = azul_layout::solver3::getters::get_css_height(&styled_dom, box_id, node_state);
    println!("css_height = {:?}", css_height);

    match css_height {
        MultiValue::Exact(h) => match h {
            LayoutHeight::Px(px) => {
                let px_value = px.number.get();
                assert!(
                    (px_value - 80.0).abs() < 0.01,
                    "Height should be 80px, got {}px",
                    px_value
                );
            }
            other => panic!("Height should be Px(80), got {:?}", other),
        },
        MultiValue::Auto => {
            panic!("Height should be Exact(80px), but got Auto! CSS height is not being parsed correctly.");
        }
        other => {
            panic!("Height should be Exact(80px), but got {:?}!", other);
        }
    }
}

/// Test that text wraps correctly when constrained to a specific width.
/// "This is a longer text that definitely needs to wrap" has intrinsic width ~340px
/// When constrained to 150px, the text should wrap to multiple lines.
#[test]
fn test_text_wraps_at_constrained_width() {
    // Use the full layout system to test text wrapping
    let html = r#"
    <html>
        <head>
            <style>
                .box {
                    display: inline-block;
                    width: 150px;
                    padding: 0;
                    margin: 0;
                    background: red;
                }
            </style>
        </head>
        <body>
            <div class="box">This is a longer text that definitely needs to wrap</div>
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

    // Find the TextLayout item for the .box element
    let text_layouts: Vec<_> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| {
            if let DisplayListItem::TextLayout { bounds, .. } = item {
                Some(bounds)
            } else {
                None
            }
        })
        .collect();

    println!("Found {} TextLayout items", text_layouts.len());
    for (i, bounds) in text_layouts.iter().enumerate() {
        println!(
            "TextLayout[{}]: {}x{} @ ({}, {})",
            i, bounds.size.width, bounds.size.height, bounds.origin.x, bounds.origin.y
        );
    }

    // The text "Box 1 with text inside" has intrinsic width ~153.77px
    // When constrained to 150px width, text should wrap, making height > 1 line height (~16-18px)
    // Expected: multiple lines, so height should be > 20px (at least 2 lines)

    // Find Rect items (for the background)
    let rectangles: Vec<_> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                // Look for red background (our .box)
                if color.r == 255 && color.g == 0 && color.b == 0 {
                    Some(bounds)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    println!("Found {} red Rectangle items", rectangles.len());
    for (i, rect) in rectangles.iter().enumerate() {
        println!(
            "Rectangle[{}]: {}x{} @ ({}, {})",
            i, rect.size.width, rect.size.height, rect.origin.x, rect.origin.y
        );
    }

    // The rectangle should have width close to 150px
    assert!(
        !rectangles.is_empty(),
        "Should have at least one red rectangle for .box"
    );

    let box_rect = rectangles[0];

    // Width should be exactly 150px (CSS specified)
    assert!(
        (box_rect.size.width - 150.0).abs() < 1.0,
        "Box width should be 150px, got {}",
        box_rect.size.width
    );

    // Height should be > 1 line height (~16px) because text should wrap
    // If text doesn't wrap, height would be around 16-18px (1 line)
    // If text wraps, height should be at least 32px (2 lines)
    let min_expected_height = 28.0; // At least close to 2 lines
    assert!(
        box_rect.size.height >= min_expected_height,
        "Box height should be >= {}px (text should wrap to 2 lines), got {}px. Text is NOT wrapping!",
        min_expected_height, box_rect.size.height
    );

    println!(
        "SUCCESS: Box size = {}x{}",
        box_rect.size.width, box_rect.size.height
    );
    println!("Text appears to wrap correctly (height indicates multiple lines)");
}
