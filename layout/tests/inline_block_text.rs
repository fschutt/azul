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
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
            cache_map: Default::default(),
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
        &styled_dom,
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
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
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
            cache_map: Default::default(),
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
        &styled_dom,
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
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

/// Test that inline text and inline-block elements are positioned horizontally
/// This verifies the IFC (Inline Formatting Context) positions inline and inline-block
/// elements on the same line.
#[test]
fn test_inline_text_and_inline_block_on_same_line() {
    // HTML structure: inline text "5" followed by inline-block button
    // Expected: both should be on the same horizontal line
    //
    // This test specifically checks that:
    // 1. inline text and inline-block are on the same line
    // 2. The button's X position is after the text (not at the left margin)
    // 3. calculated_positions contains absolute positions (not relative)
    let html = r#"
    <html>
        <head>
            <style>
                body { margin: 8px; }
                .counter { font-size: 50px; display: inline; }
                .button { 
                    display: inline-block; 
                    padding: 5px 10px;
                    background: #efefef;
                }
            </style>
        </head>
        <body>
            <span class="counter">5</span>
            <span class="button">Increase counter</span>
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
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
            cache_map: Default::default(),
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
        &styled_dom,
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    )
    .expect("Layout should succeed");

    // Print debug messages
    if let Some(msgs) = &debug_messages {
        println!("\n=== Layout Debug Messages ===");
        for msg in msgs.iter().take(50) {
            println!("{}", msg.message);
        }
    }

    // ===== CRITICAL: Check calculated_positions contains ABSOLUTE positions =====
    // This is the data structure that get_node_position uses
    println!("\n=== Calculated Positions (ABSOLUTE) ===");
    for (layout_idx, pos) in layout_cache.calculated_positions.iter().enumerate() {
        println!("Layout node {}: x={:.2}, y={:.2}", layout_idx, pos.x, pos.y);
    }

    // ===== Check the layout tree structure =====
    if let Some(tree) = &layout_cache.tree {
        println!("\n=== Layout Tree ===");
        for (idx, node) in tree.nodes.iter().enumerate() {
            let dom_idx = node.dom_node_id.map(|id| id.index() as i64).unwrap_or(-1);
            let rel_pos = node.relative_position.map(|p| format!("({:.2}, {:.2})", p.x, p.y)).unwrap_or("None".to_string());
            let abs_pos = layout_cache.calculated_positions.get(idx).map(|p| format!("({:.2}, {:.2})", p.x, p.y)).unwrap_or("None".to_string());
            let fc = format!("{:?}", node.formatting_context);
            
            println!(
                "  [{}] dom={:2}, fc={:20}, rel_pos={:15}, ABS_pos={}",
                idx, dom_idx, fc, rel_pos, abs_pos
            );
        }
    }

    // Find all Text items
    let text_items: Vec<_> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| {
            if let DisplayListItem::Text { glyphs, clip_rect, font_size_px, .. } = item {
                Some((glyphs.len(), clip_rect.clone(), *font_size_px))
            } else {
                None
            }
        })
        .collect();

    println!("\n=== Text Items ===");
    for (i, (glyph_count, clip_rect, font_size)) in text_items.iter().enumerate() {
        println!(
            "Text[{}]: {} glyphs, font_size={}, clip_rect=({}, {}) {}x{}",
            i, glyph_count, font_size, 
            clip_rect.origin.x, clip_rect.origin.y,
            clip_rect.size.width, clip_rect.size.height
        );
    }

    // Find Rect items (backgrounds)
    let rect_items: Vec<_> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                Some((bounds.clone(), *color))
            } else {
                None
            }
        })
        .collect();

    println!("\n=== Rect Items ===");
    for (i, (bounds, color)) in rect_items.iter().enumerate() {
        println!(
            "Rect[{}]: ({}, {}) {}x{} color=#{:02x}{:02x}{:02x}",
            i, bounds.origin.x, bounds.origin.y,
            bounds.size.width, bounds.size.height,
            color.r, color.g, color.b
        );
    }

    // The counter text (font-size 50px) and button should be on the same line
    // "Same line" means they should have overlapping Y ranges or be very close
    
    assert!(text_items.len() >= 2, "Should have at least 2 text items (counter '5' and button text)");
    
    // The first text should be the counter "5" (1 glyph, font-size 50)
    // The second should be "Increase counter" (16 glyphs, font-size ~16)
    let counter_text = &text_items[0];
    let button_text = &text_items[1];
    
    println!("\nCounter text: y={}", counter_text.1.origin.y);
    println!("Button text: y={}", button_text.1.origin.y);
    
    // Find the button background (should be #efefef)
    let button_bg = rect_items.iter().find(|(_, color)| {
        color.r == 0xef && color.g == 0xef && color.b == 0xef
    });
    
    if let Some((button_bounds, _)) = button_bg {
        println!("Button background: x={}, y={}", button_bounds.origin.x, button_bounds.origin.y);
        
        // The button's X position should be after the counter text
        // Counter is at body margin (8px) + counter width
        // If inline/inline-block works correctly, button.x should be > 8 + counter_width
        // NOT at x=8 which would indicate a new line
        
        // The counter "5" at 50px font size is roughly 25-35px wide
        // So the button should be at x > 30 if on same line
        // If button is at x < 20, it's on a new line (just body margin)
        
        // CRITICAL ASSERTION: Button should NOT be at the left margin
        // If it is, the inline-block is being placed on a new line incorrectly
        assert!(
            button_bounds.origin.x > 30.0,
            "FAIL: Button is at x={}, should be > 30 if on same line as counter. \
             The inline-block is being placed on a NEW LINE instead of inline with the text!",
            button_bounds.origin.x
        );
        
        // Also check that button is NOT below the counter's baseline
        // The counter at 50px has a line height of ~50px, starting at y=8 (margin)
        // So counter occupies roughly y=8 to y=58
        // Button should be within this range, not below it
        let counter_y_start = 8.0; // body margin
        let counter_line_height = 50.0;
        let counter_y_end = counter_y_start + counter_line_height;
        
        assert!(
            button_bounds.origin.y < counter_y_end + 10.0, // allow small tolerance
            "FAIL: Button is at y={}, should be < {} (within counter's line). \
             The inline-block is being placed BELOW the inline text!",
            button_bounds.origin.y, counter_y_end
        );
        
        println!("\nSUCCESS: Button is positioned inline with the counter text!");
    } else {
        panic!("Could not find button background rect");
    }
}

/// Test for Live-App DOM structure: Body as root WITHOUT HTML wrapper
/// This simulates what happens with Dom::create_body().with_child(text).with_child(button)
#[test]
fn test_body_as_root_inline_block_positioning() {
    // Simulates the Live-App structure using the DOM API directly:
    // Dom::create_body()
    //   .with_child(label) // Text "5" with font-size: 50px
    //   .with_child(button) // inline-block button
    //
    // NO HTML wrapper - body is the root node (DOM index 0)
    use azul_core::dom::NodeData;
    use azul_core::styled_dom::StyledDom;
    
    // Create the DOM structure programmatically (like the Live-App does)
    let mut label = Dom::create_text("5");
    label.set_inline_style("font-size: 50px; display: inline;");

    let mut button = Dom::create_text("Increase counter");
    button.set_inline_style("display: inline-block; padding: 5px 10px; background: #efefef;");

    let mut body_dom = Dom::create_body()
        .with_child(label)
        .with_child(button);
    let styled_dom = StyledDom::create(&mut body_dom, azul_css::css::Css::empty());

    // Create font cache and font manager
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");

    // Create layout cache and text cache
    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
            cache_map: Default::default(),
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
        &styled_dom,
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    )
    .expect("Layout should succeed");

    // Print debug messages
    if let Some(msgs) = &debug_messages {
        println!("\n=== Layout Debug Messages ===");
        for msg in msgs.iter().take(30) {
            println!("{}", msg.message);
        }
    }

    // ===== Check calculated_positions =====
    println!("\n=== Calculated Positions (ABSOLUTE) ===");
    for (layout_idx, pos) in layout_cache.calculated_positions.iter().enumerate() {
        println!("Layout node {}: x={:.2}, y={:.2}", layout_idx, pos.x, pos.y);
    }

    // ===== Check the layout tree structure =====
    if let Some(tree) = &layout_cache.tree {
        println!("\n=== Layout Tree (Body as Root) ===");
        for (idx, node) in tree.nodes.iter().enumerate() {
            let dom_idx = node.dom_node_id.map(|id| id.index() as i64).unwrap_or(-1);
            let rel_pos = node.relative_position.map(|p| format!("({:.2}, {:.2})", p.x, p.y)).unwrap_or("None".to_string());
            let abs_pos = layout_cache.calculated_positions.get(idx).map(|p| format!("({:.2}, {:.2})", p.x, p.y)).unwrap_or("None".to_string());
            let fc = format!("{:?}", node.formatting_context);
            
            println!(
                "  [{}] dom={:2}, fc={:20}, rel_pos={:15}, ABS_pos={}",
                idx, dom_idx, fc, rel_pos, abs_pos
            );
        }
    }

    // Find the button background rect
    let rect_items: Vec<_> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                Some((bounds.clone(), *color))
            } else {
                None
            }
        })
        .collect();

    println!("\n=== Rect Items ===");
    for (i, (bounds, color)) in rect_items.iter().enumerate() {
        println!(
            "Rect[{}]: ({}, {}) {}x{} color=#{:02x}{:02x}{:02x}",
            i, bounds.origin.x, bounds.origin.y,
            bounds.size.width, bounds.size.height,
            color.r, color.g, color.b
        );
    }

    // Find button background (#efefef)
    let button_bg = rect_items.iter().find(|(_, color)| {
        color.r == 0xef && color.g == 0xef && color.b == 0xef
    });

    if let Some((button_bounds, _)) = button_bg {
        println!("\nButton background: x={}, y={}", button_bounds.origin.x, button_bounds.origin.y);

        // CRITICAL: Button should be at x > 15 (after margin 8 + text ~25, minus padding ~10)
        // The background rect now correctly includes the padding area, so its origin
        // is shifted left by padding-left. If button is at x ~= 0, the body margin bug exists.
        assert!(
            button_bounds.origin.x > 15.0,
            "BUG: Button is at x={:.1}, expected > 15 (margin 8 + text ~25 - padding 10). \
             Body margin is NOT being applied to calculated_positions!",
            button_bounds.origin.x
        );

        println!("\nSUCCESS: Body margin is correctly applied!");
    } else {
        println!("\nWARNING: Could not find button background rect (this may be ok if using different styling)");
        // Still check calculated_positions for the inline-block node
        if let Some(tree) = &layout_cache.tree {
            // Find the inline-block node
            for (idx, node) in tree.nodes.iter().enumerate() {
                if matches!(node.formatting_context, azul_core::dom::FormattingContext::InlineBlock) {
                    if let Some(pos) = layout_cache.calculated_positions.get(idx) {
                        println!("InlineBlock at layout idx {}: x={:.1}, y={:.1}", idx, pos.x, pos.y);
                        assert!(
                            pos.x > 30.0,
                            "BUG: InlineBlock is at x={:.1}, expected > 30. Body margin not applied!",
                            pos.x
                        );
                        println!("\nSUCCESS: Body margin is correctly applied!");
                        return;
                    }
                }
            }
        }
    }
}
