//! [g147] NATIVE repro of the hello-world web-lift blocker.
//!
//! On the lifted (web) backend, a block <div> containing inline text lays out to
//! height 0 — its text never reaches `layout_ifc`. This test runs the SAME layout
//! NATIVELY (no remill lift). If `nested_div_with_text_has_nonzero_height` PASSES,
//! the div→text inline path is correct in source ⇒ the blocker is LIFT-only. If it
//! FAILS, it is a real layout-logic bug fixable + verifiable here in seconds.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    // Layout ONLY (skip display-list/a11y/scroll generation, which crashes in the
    // headless test env and is absent in the web build anyway).
    layout_window
        .layout_dom_recursive(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// web-text-min's WORKING case: body directly contains text (body FC = Inline).
#[test]
fn body_with_direct_text_lays_out() {
    let dom = Dom::create_body().with_child(Dom::create_text("Hello"));
    let lw = layout_dom(dom, "", 800.0, 600.0);
    let body = lw.get_node_layout_rect(node_id(0)).expect("body rect");
    eprintln!("[direct] body rect = {:?}", body);
    assert!(
        body.size.height > 0.0,
        "body with direct text should have height>0 (got {})",
        body.size.height
    );
}

/// hello-world's BROKEN-ON-LIFT case: body > div(font-size:32) > text.
#[test]
fn nested_div_with_text_has_nonzero_height() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("counter".into())].into())
            .with_child(Dom::create_text("5")),
    );
    let lw = layout_dom(dom, ".counter { font-size: 32px; }", 800.0, 600.0);
    // node 0 = body, node 1 = div.counter, node 2 = text "5"
    let div = lw.get_node_layout_rect(node_id(1)).expect("div rect");
    eprintln!("[nested] div.counter rect = {:?}", div);
    assert!(
        div.size.height > 0.0,
        "div containing inline text should have height>0 (got {}) — this is the hello-world blocker",
        div.size.height
    );
}
