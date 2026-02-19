/// Test inline and inline-block elements with gradient backgrounds and borders
/// Verifies that gradient backgrounds and borders are correctly generated in the display list
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

/// Helper function to run layout and return display list items
fn run_layout(html: &str) -> Vec<DisplayListItem> {
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
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    )
    .expect("Layout should succeed");

    display_lists
        .into_iter()
        .flat_map(|dl| dl.items.into_iter())
        .collect()
}

#[test]
fn test_inline_block_with_gradient_background() {
    let html = r#"
    <html>
        <head>
            <style>
                .gradient-box {
                    display: inline-block;
                    width: 200px;
                    height: 100px;
                    background: linear-gradient(to right, red, blue);
                }
            </style>
        </head>
        <body>
            <div class="gradient-box">Gradient content</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    // Look for gradient items in the display list
    let gradient_count = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayListItem::LinearGradient { .. }
                    | DisplayListItem::RadialGradient { .. }
                    | DisplayListItem::ConicGradient { .. }
            )
        })
        .count();

    println!("Display list items:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
        match item {
            DisplayListItem::LinearGradient { bounds, .. } => {
                println!(
                    "      LinearGradient bounds: {}x{} @ ({}, {})",
                    bounds.size.width, bounds.size.height, bounds.origin.x, bounds.origin.y
                );
            }
            DisplayListItem::Rect { bounds, color, .. } => {
                println!(
                    "      Rect bounds: {}x{} @ ({}, {}), color: rgba({},{},{},{})",
                    bounds.size.width,
                    bounds.size.height,
                    bounds.origin.x,
                    bounds.origin.y,
                    color.r,
                    color.g,
                    color.b,
                    color.a
                );
            }
            _ => {}
        }
    }

    assert!(
        gradient_count >= 1,
        "Should have at least 1 gradient item for the inline-block element, got {}",
        gradient_count
    );
}

#[test]
fn test_inline_block_with_border() {
    let html = r#"
    <html>
        <head>
            <style>
                .bordered-box {
                    display: inline-block;
                    width: 200px;
                    height: 100px;
                    border: 5px solid green;
                    background: white;
                }
            </style>
        </head>
        <body>
            <div class="bordered-box">Bordered content</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    // Look for border items in the display list
    let border_count = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Border { .. }))
        .count();

    println!("Display list items:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
        match item {
            DisplayListItem::Border {
                bounds,
                widths,
                colors,
                ..
            } => {
                println!(
                    "      Border bounds: {}x{} @ ({}, {})",
                    bounds.size.width, bounds.size.height, bounds.origin.x, bounds.origin.y
                );
                println!("      Border widths: {:?}", widths);
                println!("      Border colors: {:?}", colors);
            }
            _ => {}
        }
    }

    assert!(
        border_count >= 1,
        "Should have at least 1 border item for the inline-block element, got {}",
        border_count
    );
}

#[test]
fn test_inline_block_with_gradient_and_border() {
    let html = r#"
    <html>
        <head>
            <style>
                .fancy-box {
                    display: inline-block;
                    width: 200px;
                    height: 100px;
                    background: linear-gradient(45deg, #ff6b6b, #4ecdc4);
                    border: 3px solid #333;
                    border-radius: 10px;
                }
            </style>
        </head>
        <body>
            <div class="fancy-box">Fancy content</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    let gradient_count = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayListItem::LinearGradient { .. }
                    | DisplayListItem::RadialGradient { .. }
                    | DisplayListItem::ConicGradient { .. }
            )
        })
        .count();

    let border_count = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Border { .. }))
        .count();

    println!("Display list items:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
    }

    println!("\nGradient items: {}", gradient_count);
    println!("Border items: {}", border_count);

    assert!(
        gradient_count >= 1,
        "Should have at least 1 gradient item, got {}",
        gradient_count
    );

    assert!(
        border_count >= 1,
        "Should have at least 1 border item, got {}",
        border_count
    );
}

#[test]
fn test_inline_element_with_gradient_background() {
    let html = r#"
    <html>
        <head>
            <style>
                .inline-gradient {
                    display: inline;
                    background: linear-gradient(to bottom, yellow, orange);
                    padding: 5px 10px;
                }
            </style>
        </head>
        <body>
            <p>This is <span class="inline-gradient">highlighted text</span> in a paragraph.</p>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    let gradient_count = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayListItem::LinearGradient { .. }
                    | DisplayListItem::RadialGradient { .. }
                    | DisplayListItem::ConicGradient { .. }
            )
        })
        .count();

    println!("Display list items for inline element with gradient:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
        match item {
            DisplayListItem::LinearGradient { bounds, .. } => {
                println!(
                    "      LinearGradient bounds: {}x{} @ ({}, {})",
                    bounds.size.width, bounds.size.height, bounds.origin.x, bounds.origin.y
                );
            }
            _ => {}
        }
    }

    assert!(
        gradient_count >= 1,
        "Should have at least 1 gradient item for the inline span element, got {}",
        gradient_count
    );
}

#[test]
fn test_inline_element_with_border() {
    let html = r#"
    <html>
        <head>
            <style>
                .inline-bordered {
                    display: inline;
                    border: 2px dashed purple;
                    padding: 2px 8px;
                }
            </style>
        </head>
        <body>
            <p>This is <span class="inline-bordered">bordered text</span> in a paragraph.</p>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    let border_count = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Border { .. }))
        .count();

    println!("Display list items for inline element with border:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
        match item {
            DisplayListItem::Border {
                bounds,
                widths,
                styles,
                ..
            } => {
                println!(
                    "      Border bounds: {}x{} @ ({}, {})",
                    bounds.size.width, bounds.size.height, bounds.origin.x, bounds.origin.y
                );
                println!("      Border widths: {:?}", widths);
                println!("      Border styles: {:?}", styles);
            }
            _ => {}
        }
    }

    assert!(
        border_count >= 1,
        "Should have at least 1 border item for the inline span element, got {}",
        border_count
    );
}

#[test]
fn test_radial_gradient_on_inline_block() {
    let html = r#"
    <html>
        <head>
            <style>
                .radial-box {
                    display: inline-block;
                    width: 150px;
                    height: 150px;
                    background: radial-gradient(circle, white, black);
                }
            </style>
        </head>
        <body>
            <div class="radial-box">Radial</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    let radial_gradient_count = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::RadialGradient { .. }))
        .count();

    let any_gradient_count = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayListItem::LinearGradient { .. }
                    | DisplayListItem::RadialGradient { .. }
                    | DisplayListItem::ConicGradient { .. }
            )
        })
        .count();

    println!("Display list items for radial gradient:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
    }

    println!("\nRadial gradient items: {}", radial_gradient_count);
    println!("Any gradient items: {}", any_gradient_count);

    assert!(
        any_gradient_count >= 1 || radial_gradient_count >= 1,
        "Should have at least 1 gradient item for the radial gradient, got radial={}, any={}",
        radial_gradient_count,
        any_gradient_count
    );
}

#[test]
fn test_multiple_inline_blocks_with_different_borders() {
    let html = r#"
    <html>
        <head>
            <style>
                .box1 {
                    display: inline-block;
                    width: 100px;
                    height: 50px;
                    border: 2px solid red;
                    margin-right: 10px;
                }
                .box2 {
                    display: inline-block;
                    width: 100px;
                    height: 50px;
                    border: 4px dotted blue;
                    margin-right: 10px;
                }
                .box3 {
                    display: inline-block;
                    width: 100px;
                    height: 50px;
                    border: 3px double green;
                }
            </style>
        </head>
        <body>
            <div class="box1">Box 1</div>
            <div class="box2">Box 2</div>
            <div class="box3">Box 3</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    let border_count = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Border { .. }))
        .count();

    println!("Display list items for multiple bordered boxes:");
    for (i, item) in items.iter().enumerate() {
        match item {
            DisplayListItem::Border {
                bounds,
                widths,
                colors,
                styles,
                ..
            } => {
                println!(
                    "  [{}] Border: {}x{} @ ({}, {}), widths={:?}, styles={:?}, colors={:?}",
                    i,
                    bounds.size.width,
                    bounds.size.height,
                    bounds.origin.x,
                    bounds.origin.y,
                    widths,
                    styles,
                    colors
                );
            }
            _ => {}
        }
    }

    assert!(
        border_count >= 3,
        "Should have at least 3 border items for the 3 boxes, got {}",
        border_count
    );
}

#[test]
fn test_inline_block_gradient_bounds_match_element_size() {
    let html = r#"
    <html>
        <head>
            <style>
                .sized-gradient {
                    display: inline-block;
                    width: 250px;
                    height: 120px;
                    background: linear-gradient(to right, #000, #fff);
                }
            </style>
        </head>
        <body>
            <div class="sized-gradient">Test</div>
        </body>
    </html>
    "#;

    let items = run_layout(html);

    // Find the gradient item
    let gradient_item = items.iter().find(|item| {
        matches!(
            item,
            DisplayListItem::LinearGradient { .. }
                | DisplayListItem::RadialGradient { .. }
                | DisplayListItem::ConicGradient { .. }
        )
    });

    println!("Display list items:");
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {:?}", i, std::mem::discriminant(item));
        match item {
            DisplayListItem::LinearGradient { bounds, .. } => {
                println!(
                    "      LinearGradient bounds: {}x{}",
                    bounds.size.width, bounds.size.height
                );
            }
            DisplayListItem::Rect { bounds, .. } => {
                println!(
                    "      Rect bounds: {}x{}",
                    bounds.size.width, bounds.size.height
                );
            }
            _ => {}
        }
    }

    if let Some(DisplayListItem::LinearGradient { bounds, .. }) = gradient_item {
        // Gradient bounds should match the element size (250x120)
        assert!(
            (bounds.size.width - 250.0).abs() < 1.0,
            "Gradient width should be 250px, got {}",
            bounds.size.width
        );
        assert!(
            (bounds.size.height - 120.0).abs() < 1.0,
            "Gradient height should be 120px, got {}",
            bounds.size.height
        );
        println!(
            "SUCCESS: Gradient bounds match element size: {}x{}",
            bounds.size.width, bounds.size.height
        );
    } else {
        panic!("Should have a gradient item in the display list");
    }
}
