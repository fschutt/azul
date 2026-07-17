//! Regression test: a replaced element (image) that is a flex item with
//! `flex-grow: 1` must grow to fill the flex container's main axis (and
//! stretch on the cross axis), exactly like a `<div>` does.
//!
//! Bug found 2026-06-10 (AzulPaint canvas): an `<img>` with no intrinsic size
//! (a `RenderImageCallback` image, whose intrinsic size is 0×0) and
//! `flex-grow: 1` in a `flex-direction: column` parent was laid out 300×0 —
//! the 300px replaced-element default width with a collapsed 0 height — so the
//! canvas had a zero-height hit area (no mouse events) and rendered nothing.
//! See memory `azul-flexgrow-image-bug`.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::{ImageRef, RawImageFormat, RendererResources},
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

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
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

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// An image with NO intrinsic size (like a callback image): 0×0.
fn sizeless_image() -> ImageRef {
    ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new())
}

#[test]
fn image_flex_grow_fills_column() {
    // root: flex column, full viewport height.
    //   child 0: header, fixed 50px tall.
    //   child 1: image, flex-grow: 1 -> should fill the remaining 430px.
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("header".into())].into()),
        )
        .with_child(
            Dom::create_image(sizeless_image())
                .with_ids_and_classes(vec![IdOrClass::Class("canvas".into())].into()),
        );

    let css = r#"
        .root { height: 100%; display: flex; flex-direction: column; }
        .header { height: 50px; }
        .canvas { flex-grow: 1; }
    "#;

    let (vw, vh) = (640.0, 480.0);
    let lw = layout_dom(dom, css, vw, vh);

    let canvas = lw
        .get_node_layout_rect(node_id(2))
        .expect("canvas layout rect");
    println!("canvas rect = {canvas:?}");

    // Main axis (height): flex-grow:1 should fill 480 - 50 = 430.
    assert!(
        canvas.size.height > 400.0,
        "canvas height = {} (expected ~430 from flex-grow); bug collapses it to 0",
        canvas.size.height
    );
    // Cross axis (width): align-items default stretch should give full width.
    assert!(
        canvas.size.width > 600.0,
        "canvas width = {} (expected ~640 from stretch); bug uses the 300px replaced default",
        canvas.size.width
    );
}
