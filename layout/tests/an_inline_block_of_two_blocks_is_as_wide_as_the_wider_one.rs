//! An inline-block holding two blocks is as wide as the wider of them, as in
//! Chrome: its shrink-to-fit width is the largest max-content width of its
//! block children, which stack, not the sum of their texts on one line.
//!
//! A block holding only text establishes an inline formatting context, and
//! the inline-block's measure took that for inline content of its own: it
//! measured `<div>a</div><div>b</div>` as the line "ab" (17.81px against
//! Chrome's 8.91, RULINGS8).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The width of an inline-block (node 1) holding one block per text.
fn width_of(texts: &[&str]) -> f32 {
    let mut ib = Dom::create_div().with_css("display: inline-block;");
    for t in texts {
        ib = ib.with_child(
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                *t,
            )),
        );
    }
    let dom = Dom::create_body()
        .with_css("margin: 0px; font-family: \"EB Garamond\"; font-size: 20px;")
        .with_child(ib);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(NodeId::new(1)).into(),
    })
    .expect("laid out")
    .size
    .width
}

#[test]
fn an_inline_block_of_two_blocks_is_as_wide_as_the_wider_one() {
    let a = width_of(&["a"]);
    let b = width_of(&["b"]);
    let both = width_of(&["a", "b"]);
    assert!(a > 0.0 && b > 0.0, "one block alone has width: {a} {b}");
    assert!(
        (both - a.max(b)).abs() < 0.5,
        "the blocks stack: as wide as the wider ({}), not the line \"ab\" ({}): {both}",
        a.max(b),
        a + b
    );
}

#[test]
fn a_long_block_under_a_short_one_sets_the_width_alone() {
    let long = width_of(&["a much longer line"]);
    let both = width_of(&["a", "a much longer line"]);
    assert!(
        (both - long).abs() < 0.5,
        "as wide as the long block ({long}), not both lines side by side: {both}"
    );
}
