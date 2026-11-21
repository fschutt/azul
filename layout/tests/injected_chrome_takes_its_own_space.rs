//! Chrome injected above the user's DOM takes its space FROM the user's DOM.
//!
//! The Linux shells prepend a software menu bar above the app's `<body>`.
//! The wrapper was a plain block `<html>`, so the bar was simply stacked on
//! top: a `height: 100%` body still resolved to the WHOLE window, and the
//! document ended up taller than the window by the bar plus the UA margins.
//! Measured live on X11 at 640x480: the root came out 640x522 and the last
//! 42px of the page sat below the window.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 640.0;
const H: f32 = 480.0;

/// `html > [menubar(26px), body(height:100%, margin:8px)]`, laid out at 640x480.
fn root_height(html_css: &str) -> f32 {
    let mut dom = Dom::create_html().with_css(html_css).with_children(
        vec![
            Dom::create_div().with_css("height: 26px; flex-shrink: 0;"),
            Dom::create_body().with_css("height: 100%; margin: 8px;"),
        ]
        .into(),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw.get_node_layout_rect(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(NodeId::new(0)).into(),
    })
    .expect("the root was laid out")
    .size
    .height
}

#[test]
fn a_plain_block_wrapper_lets_the_page_outgrow_the_window() {
    // The harness for the law below: stacked, the document IS taller than the
    // window - by the bar (26) plus the body's margins (16), i.e. 42px.
    let stacked = root_height("");
    assert!(
        stacked > H,
        "harness: a block wrapper was supposed to overflow, got {stacked}"
    );
}

#[test]
fn a_column_wrapper_fits_the_document_in_the_window() {
    let fitted = root_height("display: flex; flex-direction: column; height: 100%;");
    assert!(
        (fitted - H).abs() < 1.0,
        "the injected chrome must take its space from the body, not push it off the bottom: the \
         document is {fitted} tall in a {H} window"
    );
}
