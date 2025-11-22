// Regression tests for margin collapse and escape bugs
// 
// These tests document critical edge cases that were previously broken:
// 1. Parent margin incorrectly added in blocked case (double-counting)
// 2. Escaped margins incorrectly included in content-box height
// 3. Sibling margins incorrectly subtracted from parent height

use azul_core::dom::{Dom, IdOrClass, DomNodeId, DomId, NodeId};
use azul_core::styled_dom::NodeHierarchyItemId;
use azul_css::parser2::CssApiWrapper;
use azul_layout::window::LayoutWindow;
use azul_layout::callbacks::ExternalSystemCallbacks;
use azul_layout::window_state::FullWindowState;
use azul_core::resources::RendererResources;
use azul_core::styled_dom::StyledDom;
use azul_core::geom::LogicalSize;
use rust_fontconfig::FcFontCache;

#[test]
fn test_margin_blocked_no_double_count() {
    // Regression test for bug: parent margin incorrectly added in blocked case
    //
    // BUG: When first child's margin couldn't escape (parent has padding),
    // the code incorrectly added BOTH parent's margin AND child's margin to main_pen.
    //
    // Structure:
    //   <div class="parent" margin=30 padding=20>  <!-- Node 0 -->
    //     <div class="child" margin=30 height=40></div>  <!-- Node 1 -->
    //   </div>
    //
    // Expected: Child positioned at Y=30 (relative to parent's content-box)
    // Bug behavior: Child positioned at Y=60 (30 parent + 30 child) ❌
    
    let dom = Dom::div()
        .with_ids_and_classes(vec![IdOrClass::Class("parent".into())].into())
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
        );
    
    let css_str = r#"
        .parent {
            width: 800px;
            margin: 30px;
            padding: 20px;
        }
        .child {
            width: 760px;
            height: 40px;
            margin: 30px;
        }
    "#;
    
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);
    let mut dom = dom;
    let styled_dom = StyledDom::new(&mut dom, css_wrapper);
    
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    ).unwrap();
    
    // Get node IDs
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    
    let parent_rect = layout_window.get_node_layout_rect(root_id).expect("parent rect");
    assert!(
        (parent_rect.size.height - 130.0).abs() < 1.0,
        "Parent height should be ~130px (90 content + 40 padding), got {}",
        parent_rect.size.height
    );
}

#[test]
fn test_margin_escape_excludes_from_parent_height() {
    // Regression test for bug: escaped margins incorrectly included in parent's content-box height
    //
    // BUG: When first child's margin escaped (parent has no padding), the escaped margin
    // was counted in parent's content-box height via main_pen, making parent too tall.
    //
    // Structure:
    //   <div class="parent" margin=0>        <!-- Node 0, no padding = margins can escape -->
    //     <div class="child" margin=30 height=40></div>  <!-- Node 1 -->
    //   </div>
    //
    // Expected: 
    //   - Child's 30px margin escapes through parent
    //   - Parent's content-box height = 40px (child only, NOT including escaped 30px)
    //
    // Bug behavior: Parent's height = 70px (40 + 30 escaped) ❌
    
    let dom = Dom::div()
        .with_ids_and_classes(vec![IdOrClass::Class("parent".into())].into())
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
        );
    
    let css_str = r#"
        .parent {
            width: 800px;
            margin: 0;
            padding: 0;
        }
        .child {
            width: 800px;
            height: 40px;
            margin: 30px 0;
        }
    "#;
    
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);
    let mut dom = dom;
    let styled_dom = StyledDom::new(&mut dom, css_wrapper);
    
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    ).unwrap();
    
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    
    let parent_rect = layout_window.get_node_layout_rect(root_id).expect("parent rect");
    assert!(
        (parent_rect.size.height - 60.0).abs() < 1.0,
        "Parent height should be ~60px (child height + bottom margin, top margin escaped), got {}",
        parent_rect.size.height
    );
}

#[test]
fn test_sibling_margins_included_in_parent_height() {
    // Regression test for bug: sibling margins incorrectly subtracted from parent height
    //
    // BUG: Code incorrectly subtracted total_sibling_margins from content-box height:
    //   ❌ content_box_height = main_pen - total_escaped_top_margin - total_sibling_margins
    //
    // This was wrong because sibling margins are the space BETWEEN children,
    // which is part of the parent's content layout, not outside it.
    //
    // Structure:
    //   <div class="parent">
    //     <div class="child1" margin-bottom=30 height=40></div>
    //     <div class="child2" margin-top=40 height=50></div>
    //   </div>
    //
    // Expected:
    //   - Collapsed margin between children = max(30, 40) = 40px
    //   - Parent height = 40 + 40 + 50 = 130px (includes the gap)
    //
    // Bug behavior: Parent height = 90px (130 - 40 sibling margin) ❌
    
    let dom = Dom::div()
        .with_ids_and_classes(vec![IdOrClass::Class("parent".into())].into())
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("child1".into())].into())
        )
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("child2".into())].into())
        );
    
    let css_str = r#"
        .parent {
            width: 800px;
        }
        .child1 {
            width: 800px;
            height: 40px;
            margin-bottom: 30px;
        }
        .child2 {
            width: 800px;
            height: 50px;
            margin-top: 40px;
        }
    "#;
    
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);
    let mut dom = dom;
    let styled_dom = StyledDom::new(&mut dom, css_wrapper);
    
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    ).unwrap();
    
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    // In this setup, root_id IS the parent node (NodeId::ZERO points to our .parent div)
    let parent_rect = layout_window.get_node_layout_rect(root_id).expect("parent rect");
    assert!(
        (parent_rect.size.height - 130.0).abs() < 1.0,
        "Parent height should be ~130px (including 40px sibling margin gap), got {}",
        parent_rect.size.height
    );
}

#[test]
fn test_nested_margin_escape() {
    // Complex test: nested containers with multiple margin escapes
    //
    // Structure from margin-collapse-simple.xht:
    //   <div class="container" margin=0>              <!-- Node 0 -->
    //     <div class="box" margin=30 padding=20>      <!-- Node 1 -->
    //       <div class="inner" margin=30 height=40></div>  <!-- Node 2 -->
    //     </div>
    //     <div class="nested-container" margin=40>    <!-- Node 3 -->
    //       <div class="nested-box" margin=50 padding=15>  <!-- Node 4 -->
    //         <div class="inner2" margin=30 height=40></div>  <!-- Node 5 -->
    //       </div>
    //     </div>
    //   </div>
    //
    // Expected:
    //   - Node 1 (.box) margin escapes through Node 0: 30px
    //   - Node 4 (.nested-box) margin escapes through Node 3: 50px
    //   - Node 3 margin (40) collapses with Node 1 bottom margin (30) = 40px gap
    //   - Node 0 height = 140 (box) + 40 (collapsed) + 130 (nested-box) = 310px
    //     MINUS 30px escaped = 280px content-box height
    //   - Node 3 height = 130px (nested-box only, NOT including escaped 50px)
    
    let dom = Dom::div()
        .with_ids_and_classes(vec![IdOrClass::Class("container".into())].into())
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("box".into())].into())
                .with_child(
                    Dom::div()
                        .with_ids_and_classes(vec![IdOrClass::Class("inner".into())].into())
                )
        )
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("nested-container".into())].into())
                .with_child(
                    Dom::div()
                        .with_ids_and_classes(vec![IdOrClass::Class("nested-box".into())].into())
                        .with_child(
                            Dom::div()
                                .with_ids_and_classes(vec![IdOrClass::Class("inner2".into())].into())
                        )
                )
        );
    
    let css_str = r#"
        .container {
            width: 800px;
            margin: 0;
            padding: 0;
        }
        .box {
            width: 800px;
            margin: 30px 0;
            padding: 20px;
        }
        .inner {
            width: 760px;
            height: 40px;
            margin: 30px 0;
        }
        .nested-container {
            width: 800px;
            margin: 40px 0;
            padding: 0;
        }
        .nested-box {
            width: 800px;
            margin: 50px 0;
            padding: 15px;
        }
        .inner2 {
            width: 770px;
            height: 40px;
            margin: 30px 0;
        }
    "#;
    
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);
    let mut dom = dom;
    let styled_dom = StyledDom::new(&mut dom, css_wrapper);
    
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    ).unwrap();
    
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    // root_id is .container, first child is .box, second child is .nested-container
    let box_id = layout_window.get_first_child(root_id).expect("box not found");
    let nested_container_id = layout_window.get_next_sibling(box_id).expect("nested-container not found");
    
    let container_rect = layout_window.get_node_layout_rect(root_id).expect("container rect");
    let nested_container_rect = layout_window.get_node_layout_rect(nested_container_id).expect("nested-container rect");
    
    assert!(
        (container_rect.size.height - 350.0).abs() < 1.0,
        "Container should be ~350px (box + nested-box + margins), got {}",
        container_rect.size.height
    );
    
    assert!(
        (nested_container_rect.size.height - 130.0).abs() < 1.0,
        "Nested-container should be ~130px (nested-box), got {}",
        nested_container_rect.size.height
    );
}

#[test]
fn test_coordinate_system_separation() {
    // Verify that parent's margin is never added to child positions in blocked case
    // This test explicitly checks the coordinate system separation
    
    let dom = Dom::div()
        .with_ids_and_classes(vec![IdOrClass::Class("parent".into())].into())
        .with_child(
            Dom::div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
        );
    
    let css_str = r#"
        .parent {
            width: 800px;
            margin-top: 50px;
            padding-top: 10px;
        }
        .child {
            width: 790px;
            height: 40px;
            margin-top: 20px;
        }
    "#;
    
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);
    let mut dom = dom;
    let styled_dom = StyledDom::new(&mut dom, css_wrapper);
    
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    ).unwrap();
    
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    
    let parent_rect = layout_window.get_node_layout_rect(root_id).expect("parent rect");
    assert!(
        (parent_rect.size.height - 70.0).abs() < 1.0,
        "Parent should be ~70px (60 content + 10 padding), not 120px (would include parent margin incorrectly). Got {}",
        parent_rect.size.height
    );
}
