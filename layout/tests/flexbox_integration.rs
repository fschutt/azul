//! Flexbox Integration Tests
//!
//! Tests for flexbox layout behavior, particularly edge cases and
//! CSS-spec compliance.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
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

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

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
}

fn get_root_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    }
}

#[test]
fn test_flexbox_row_direction() {
    // Test basic flex-direction: row
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            flex-direction: row;
            width: 300px;
            height: 100px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let container_rect = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");

    assert!(
        (container_rect.size.width - 300.0).abs() < 1.0,
        "Container width should be 300px, got {}",
        container_rect.size.width
    );
}

#[test]
fn test_flexbox_column_direction() {
    // Test flex-direction: column
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            flex-direction: column;
            width: 100px;
            height: 200px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let container_rect = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");

    assert!(
        (container_rect.size.height - 200.0).abs() < 1.0,
        "Container height should be 200px, got {}",
        container_rect.size.height
    );
}

#[test]
fn test_flexbox_justify_content_center() {
    // Test justify-content: center
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            justify-content: center;
            width: 300px;
            height: 100px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    // Test passes if no panic occurs during layout
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_justify_content_space_between() {
    // Test justify-content: space-between
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            justify-content: space-between;
            width: 400px;
            height: 100px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_align_items_center() {
    // Test align-items: center
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            align-items: center;
            width: 300px;
            height: 200px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_flex_grow() {
    // Test flex-grow distributes remaining space
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("grow".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("fixed".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 300px;
            height: 100px;
        }
        .grow {
            flex-grow: 1;
            height: 50px;
        }
        .fixed {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_flex_shrink() {
    // Test flex-shrink when items overflow
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 200px;
            height: 100px;
        }
        .item {
            flex-shrink: 1;
            width: 150px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_flex_wrap() {
    // Test flex-wrap: wrap
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            flex-wrap: wrap;
            width: 200px;
            height: 200px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_gap() {
    // Test gap property
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            gap: 10px;
            width: 400px;
            height: 100px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_order() {
    // Test order property
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("first".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("second".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 200px;
            height: 100px;
        }
        .first {
            order: 2;
            width: 50px;
            height: 50px;
        }
        .second {
            order: 1;
            width: 50px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_nested() {
    // Test nested flex containers
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("outer".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("inner".into())].into())
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
                )
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
                ),
        );

    let css = r#"
        .outer {
            display: flex;
            flex-direction: column;
            width: 300px;
            height: 300px;
        }
        .inner {
            display: flex;
            flex-direction: row;
            height: 100px;
        }
        .item {
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("outer rect");
}

#[test]
fn test_flexbox_min_max_size() {
    // Test min/max size constraints in flex context
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 400px;
            height: 100px;
        }
        .item {
            flex-grow: 1;
            min-width: 100px;
            max-width: 200px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_auto_margin() {
    // Test auto margins in flex context (for centering)
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 300px;
            height: 100px;
        }
        .item {
            margin: auto;
            width: 100px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_align_self() {
    // Test align-self override
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("start".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("end".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            align-items: flex-start;
            width: 200px;
            height: 200px;
        }
        .start {
            width: 50px;
            height: 50px;
        }
        .end {
            align-self: flex-end;
            width: 50px;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_empty_container() {
    // Test empty flex container
    let dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("container".into())].into());

    let css = r#"
        .container {
            display: flex;
            width: 300px;
            height: 100px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let container_rect = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");

    assert!(
        (container_rect.size.width - 300.0).abs() < 1.0,
        "Empty flex container width should be 300px, got {}",
        container_rect.size.width
    );
}

#[test]
fn test_flexbox_single_child() {
    // Test flex container with single child
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            justify-content: center;
            align-items: center;
            width: 300px;
            height: 300px;
        }
        .item {
            width: 100px;
            height: 100px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_percentage_width() {
    // Test percentage widths in flex items
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 400px;
            height: 100px;
        }
        .item {
            width: 50%;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_flex_basis_auto() {
    // Test flex-basis: auto uses content size
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("item".into())].into())
                .with_child(Dom::create_text("Content")),
        );

    let css = r#"
        .container {
            display: flex;
            width: 400px;
            height: 100px;
        }
        .item {
            flex-basis: auto;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}

#[test]
fn test_flexbox_flex_basis_zero() {
    // Test flex-basis: 0 ignores content size for distribution
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        )
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("item".into())].into()),
        );

    let css = r#"
        .container {
            display: flex;
            width: 400px;
            height: 100px;
        }
        .item {
            flex: 1 1 0;
            height: 50px;
        }
    "#;

    let layout_window = layout_dom(dom, css, 1024.0, 768.0);
    let root_id = get_root_id();
    let _ = layout_window
        .get_node_layout_rect(root_id)
        .expect("container rect");
}
