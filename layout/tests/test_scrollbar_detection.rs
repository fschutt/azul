//! Scrollbar Detection Tests
//!
//! Tests that scrollbars are correctly detected when content overflows,
//! and that the display list contains ScrollBar items.

use azul_core::{
    dom::{Dom, IdOrClass},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::{
        display_list::DisplayListItem,
        fc::{check_scrollbar_necessity, OverflowBehavior},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn create_layout_window() -> LayoutWindow {
    let font_cache = FcFontCache::build();
    LayoutWindow::new(font_cache).unwrap()
}

fn create_window_state(width: f32, height: f32) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window_state
}

/// Layout a DOM and return the count of scrollbar items in the display list
fn layout_dom_and_count_scrollbars(dom: Dom, css_str: &str, width: f32, height: f32) -> usize {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let dom_id = styled_dom.dom_id;

    let mut layout_window = create_layout_window();
    let window_state = create_window_state(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // Count scrollbar items in the display list
    layout_window
        .layout_results
        .get(&dom_id)
        .map(|r| {
            r.display_list
                .items
                .iter()
                .filter(|item| matches!(item, DisplayListItem::ScrollBar { .. }))
                .count()
        })
        .unwrap_or(0)
}

// =============================================================================
// Unit Tests for check_scrollbar_necessity
// =============================================================================

#[test]
fn test_scrollbar_necessity_no_overflow_visible() {
    // Content smaller than container, overflow: visible - no scrollbars
    let result = check_scrollbar_necessity(
        LogicalSize::new(100.0, 100.0), // content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Visible,
        OverflowBehavior::Visible,
    );
    
    assert!(!result.needs_horizontal, "Should not need horizontal scrollbar");
    assert!(!result.needs_vertical, "Should not need vertical scrollbar");
}

#[test]
fn test_scrollbar_necessity_overflow_hidden() {
    // Content larger than container, overflow: hidden - no scrollbars (clipped)
    let result = check_scrollbar_necessity(
        LogicalSize::new(300.0, 300.0), // content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Hidden,
        OverflowBehavior::Hidden,
    );
    
    assert!(!result.needs_horizontal, "Hidden should not show horizontal scrollbar");
    assert!(!result.needs_vertical, "Hidden should not show vertical scrollbar");
}

#[test]
fn test_scrollbar_necessity_overflow_scroll() {
    // overflow: scroll always shows scrollbars
    let result = check_scrollbar_necessity(
        LogicalSize::new(100.0, 100.0), // content smaller
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Scroll,
        OverflowBehavior::Scroll,
    );
    
    assert!(result.needs_horizontal, "Scroll should always show horizontal scrollbar");
    assert!(result.needs_vertical, "Scroll should always show vertical scrollbar");
    assert!(result.scrollbar_width > 0.0, "scrollbar_width should be set");
    assert!(result.scrollbar_height > 0.0, "scrollbar_height should be set");
}

#[test]
fn test_scrollbar_necessity_overflow_auto_no_overflow() {
    // overflow: auto, content fits - no scrollbars
    let result = check_scrollbar_necessity(
        LogicalSize::new(100.0, 100.0), // content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
    );
    
    assert!(!result.needs_horizontal, "Auto should not show scrollbar when content fits");
    assert!(!result.needs_vertical, "Auto should not show scrollbar when content fits");
}

#[test]
fn test_scrollbar_necessity_overflow_auto_vertical_overflow() {
    // overflow: auto, content taller than container - vertical scrollbar needed
    let result = check_scrollbar_necessity(
        LogicalSize::new(100.0, 500.0), // tall content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
    );
    
    assert!(!result.needs_horizontal, "Should not need horizontal scrollbar");
    assert!(result.needs_vertical, "Should need vertical scrollbar for tall content");
    assert!(result.scrollbar_width > 0.0, "scrollbar_width should be set");
}

#[test]
fn test_scrollbar_necessity_overflow_auto_horizontal_overflow() {
    // overflow: auto, content wider than container - horizontal scrollbar needed
    let result = check_scrollbar_necessity(
        LogicalSize::new(500.0, 100.0), // wide content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
    );
    
    assert!(result.needs_horizontal, "Should need horizontal scrollbar for wide content");
    assert!(!result.needs_vertical, "Should not need vertical scrollbar");
    assert!(result.scrollbar_height > 0.0, "scrollbar_height should be set");
}

#[test]
fn test_scrollbar_necessity_both_overflow() {
    // Content overflows both dimensions
    let result = check_scrollbar_necessity(
        LogicalSize::new(500.0, 500.0), // large content
        LogicalSize::new(200.0, 200.0), // container
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
    );
    
    assert!(result.needs_horizontal, "Should need horizontal scrollbar");
    assert!(result.needs_vertical, "Should need vertical scrollbar");
}

// =============================================================================
// Integration Tests with DOM Layout
// =============================================================================

#[test]
fn test_layout_overflow_auto_vertical_scrollbar() {
    // Create a container with overflow: auto and content that overflows vertically
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow-y: auto;
        }
        .content {
            width: 100px;
            height: 500px;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    assert!(
        scrollbar_count > 0,
        "Display list should contain ScrollBar items when content overflows with overflow: auto"
    );
}

#[test]
fn test_layout_overflow_scroll_always_shows_scrollbar() {
    // overflow: scroll should always show scrollbars even without overflow
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: scroll;
        }
        .content {
            width: 50px;
            height: 50px;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    // Should have both horizontal and vertical scrollbars
    assert!(
        scrollbar_count >= 2,
        "overflow: scroll should show both scrollbars, got {} items",
        scrollbar_count
    );
}

#[test]
fn test_layout_overflow_hidden_no_scrollbar() {
    // overflow: hidden should never show scrollbars
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: hidden;
        }
        .content {
            width: 500px;
            height: 500px;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    assert!(
        scrollbar_count == 0,
        "overflow: hidden should not show any scrollbars, got {} items",
        scrollbar_count
    );
}

// =============================================================================
// Scrollbar Reflow Tests - verify content is resized when scrollbars appear
// =============================================================================

/// Helper to layout a DOM and get the content width
fn layout_dom_and_get_content_width(dom: Dom, css_str: &str, width: f32, height: f32) -> Option<f32> {
    use azul_core::dom::{DomNodeId, NodeId};
    use azul_core::styled_dom::NodeHierarchyItemId;

    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let dom_id = styled_dom.dom_id;

    let mut layout_window = create_layout_window();
    let window_state = create_window_state(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // Get the content node (node 1, which is the first child of container at node 0)
    let content_node_id = DomNodeId {
        dom: dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };

    layout_window
        .get_node_layout_rect(content_node_id)
        .map(|rect| rect.size.width)
}

#[test]
fn test_scrollbar_reflow_width_100_percent() {
    // Test that when a vertical scrollbar appears, content with width: 100%
    // is reduced by the scrollbar width
    
    // Case 1: No scrollbar needed - content should be full container width
    let dom_no_overflow = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css_no_overflow = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 100%;
            height: 100px;
        }
    "#;

    let width_no_overflow = layout_dom_and_get_content_width(dom_no_overflow, css_no_overflow, 1024.0, 768.0);
    
    // Case 2: Vertical scrollbar needed - content should be 200 - scrollbar_width
    let dom_with_overflow = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css_with_overflow = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 100%;
            height: 500px;
        }
    "#;

    let width_with_overflow = layout_dom_and_get_content_width(dom_with_overflow, css_with_overflow, 1024.0, 768.0);

    // Both should have values
    assert!(width_no_overflow.is_some(), "Should get width for no-overflow case");
    assert!(width_with_overflow.is_some(), "Should get width for with-overflow case");

    let w1 = width_no_overflow.unwrap();
    let w2 = width_with_overflow.unwrap();

    println!("Width without scrollbar: {}", w1);
    println!("Width with scrollbar: {}", w2);

    // When vertical scrollbar is present, content width should be smaller
    // Scrollbar width is typically 16px
    assert!(
        w2 < w1,
        "Content width with scrollbar ({}) should be less than without scrollbar ({})",
        w2, w1
    );
    
    // The difference should be approximately the scrollbar width (16px)
    let diff = w1 - w2;
    assert!(
        diff >= 12.0 && diff <= 20.0,
        "Width difference ({}) should be approximately the scrollbar width (12-20px)",
        diff
    );
}

#[test]
fn test_scrollbar_appears_with_overflow_auto() {
    // Test that overflow: auto shows scrollbar when content exceeds container
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 100%;
            height: 500px;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    assert!(
        scrollbar_count >= 1,
        "overflow: auto should show vertical scrollbar when content overflows, got {} scrollbars",
        scrollbar_count
    );
}

#[test]
fn test_no_scrollbar_when_content_fits_with_overflow_auto() {
    // Test that overflow: auto does NOT show scrollbar when content fits
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 100%;
            height: 100px;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    assert!(
        scrollbar_count == 0,
        "overflow: auto should NOT show scrollbar when content fits, got {} scrollbars",
        scrollbar_count
    );
}

#[test]
fn test_horizontal_scrollbar_reduces_height() {
    // When horizontal scrollbar appears, available height should be reduced
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into())
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 500px;
            height: 100%;
        }
    "#;

    let scrollbar_count = layout_dom_and_count_scrollbars(dom, css, 1024.0, 768.0);
    
    // Should have at least horizontal scrollbar
    assert!(
        scrollbar_count >= 1,
        "overflow: auto should show horizontal scrollbar when content is wider, got {} scrollbars",
        scrollbar_count
    );
}

// =============================================================================
// Scrollbar Rendering Tests - verify bounds are correct
// =============================================================================

/// Helper to get all scrollbar bounds from a display list
fn layout_dom_and_get_scrollbar_bounds(
    dom: Dom,
    css_str: &str,
    width: f32,
    height: f32,
) -> Vec<(f32, f32, f32, f32, String)> {
    // Returns (x, y, width, height, orientation)

    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let dom_id = styled_dom.dom_id;

    let mut layout_window = create_layout_window();
    let window_state = create_window_state(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    layout_window
        .layout_results
        .get(&dom_id)
        .map(|r| {
            r.display_list
                .items
                .iter()
                .filter_map(|item| {
                    if let DisplayListItem::ScrollBar {
                        bounds,
                        orientation,
                        ..
                    } = item
                    {
                        Some((
                            bounds.origin.x,
                            bounds.origin.y,
                            bounds.size.width,
                            bounds.size.height,
                            format!("{:?}", orientation),
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn test_scrollbar_bounds_vertical_at_right_edge() {
    // Vertical scrollbar should be at the right edge of the container
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into()),
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 100px;
            height: 500px;
        }
    "#;

    let bounds = layout_dom_and_get_scrollbar_bounds(dom, css, 1024.0, 768.0);
    
    println!("Scrollbar bounds: {:?}", bounds);
    
    // Should have exactly one vertical scrollbar
    let vertical_scrollbars: Vec<_> = bounds.iter().filter(|b| b.4.contains("Vertical")).collect();
    
    assert!(
        !vertical_scrollbars.is_empty(),
        "Should have a vertical scrollbar when content overflows vertically"
    );
    
    // Vertical scrollbar x position should be at container_width - scrollbar_width
    // Container is 200px, scrollbar is ~16px, so x should be around 184
    let (x, y, w, h, _) = vertical_scrollbars[0];
    
    println!("Vertical scrollbar: x={}, y={}, w={}, h={}", x, y, w, h);
    
    // X position should be near the right edge (200 - scrollbar_width)
    assert!(
        *x >= 180.0 && *x <= 200.0,
        "Vertical scrollbar x ({}) should be at right edge (180-200)",
        x
    );
    
    // Scrollbar should start at top (y=0 or close)
    assert!(
        *y >= 0.0 && *y <= 10.0,
        "Vertical scrollbar y ({}) should be at top",
        y
    );
    
    // Scrollbar height should match container height (or container height - horizontal scrollbar)
    assert!(
        *h >= 180.0 && *h <= 200.0,
        "Vertical scrollbar height ({}) should match container height",
        h
    );
}

#[test]
fn test_scrollbar_bounds_horizontal_at_bottom() {
    // Horizontal scrollbar should be at the bottom of the container
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("content".into())].into()),
        );

    let css = r#"
        .container {
            width: 200px;
            height: 200px;
            overflow: auto;
        }
        .content {
            width: 500px;
            height: 100px;
        }
    "#;

    let bounds = layout_dom_and_get_scrollbar_bounds(dom, css, 1024.0, 768.0);
    
    println!("Scrollbar bounds: {:?}", bounds);
    
    // Should have a horizontal scrollbar
    let horizontal_scrollbars: Vec<_> = bounds.iter().filter(|b| b.4.contains("Horizontal")).collect();
    
    assert!(
        !horizontal_scrollbars.is_empty(),
        "Should have a horizontal scrollbar when content overflows horizontally"
    );
    
    let (x, y, w, h, _) = horizontal_scrollbars[0];
    
    println!("Horizontal scrollbar: x={}, y={}, w={}, h={}", x, y, w, h);
    
    // X position should be at left (x=0 or close)
    assert!(
        *x >= 0.0 && *x <= 10.0,
        "Horizontal scrollbar x ({}) should be at left edge",
        x
    );
    
    // Y position should be at bottom (200 - scrollbar_height)
    assert!(
        *y >= 180.0 && *y <= 200.0,
        "Horizontal scrollbar y ({}) should be at bottom edge (180-200)",
        y
    );
    
    // Scrollbar width should match container width
    assert!(
        *w >= 180.0 && *w <= 200.0,
        "Horizontal scrollbar width ({}) should match container width",
        w
    );
}

#[test]
fn test_scrolling_c_style_layout() {
    // Test a layout similar to scrolling.c:
    // body with flex column, header, scrollable container with many items, footer
    
    // Create header
    let header = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("header".into())].into());
    
    // Create scroll container with tall content
    let mut scroll_container = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("scroll-container".into())].into());
    
    // Add 10 items that overflow
    for _ in 0..10 {
        let item = Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("item".into())].into());
        scroll_container = scroll_container.with_child(item);
    }
    
    // Create footer
    let footer = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("footer".into())].into());
    
    // Build body
    let dom = Dom::create_body()
        .with_ids_and_classes(vec![IdOrClass::Class("body".into())].into())
        .with_child(header)
        .with_child(scroll_container)
        .with_child(footer);

    let css = r#"
        body {
            display: flex;
            flex-direction: column;
            height: 500px;
            width: 600px;
        }
        .header {
            height: 50px;
            background-color: blue;
            flex-shrink: 0;
        }
        .scroll-container {
            flex: 1;
            overflow: auto;
        }
        .item {
            height: 100px;
            margin: 5px;
            padding: 20px;
        }
        .footer {
            height: 30px;
            background-color: gray;
            flex-shrink: 0;
        }
    "#;

    // Window is 600x500, like scrolling.c
    let bounds = layout_dom_and_get_scrollbar_bounds(dom, css, 600.0, 500.0);
    
    println!("Scrolling.c style layout - Scrollbar bounds: {:?}", bounds);
    
    // With 10 items at 100px height + margins, total content is ~1050px
    // Container height is 500 - 50 (header) - 30 (footer) = 420px
    // So we should have a vertical scrollbar
    
    let vertical_scrollbars: Vec<_> = bounds.iter().filter(|b| b.4.contains("Vertical")).collect();
    
    assert!(
        !vertical_scrollbars.is_empty(),
        "Flex container with overflow: auto should show vertical scrollbar when items overflow. Got: {:?}",
        bounds
    );
    
    let (x, y, w, h, _) = vertical_scrollbars[0];
    println!("Vertical scrollbar in flex layout: x={}, y={}, w={}, h={}", x, y, w, h);
    
    // Scrollbar should be within the scroll-container bounds
    // Header is 50px, so scroll container starts at y=50
    assert!(
        *y >= 40.0,
        "Scrollbar y ({}) should be below header (50px)",
        y
    );
}
