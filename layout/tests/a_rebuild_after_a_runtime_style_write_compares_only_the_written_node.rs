//! A rebuild after a runtime style write compares only the written node.
//!
//! A rebuild compares a matched node's properties one type at a time only
//! where the node's cascade inputs moved. A runtime write (`set_css_property`,
//! one frame of a transition) sizes the old tree's override table to the
//! whole tree, empty for every node but the written one, while a freshly built
//! tree has no table yet. When "no overrides" and "an empty override list"
//! count as different inputs, every node of every rebuild after the window's
//! first runtime write is compared, ~200 property lookups per node on both
//! sides: AzWidgets paid ~85 ms of each click for it.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{property::CssProperty, style::StyleOpacity};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A body of 30 paragraphs, each holding one text: the body is node 0,
/// paragraph `i` is node `1 + 2 * i` and its text the node after it.
fn page() -> StyledDom {
    let mut body = Dom::create_body().with_css("margin: 0;");
    for i in 0..30 {
        body = body.with_child(Dom::create_p_with_text(format!("Line {i}").as_str()));
    }
    StyledDom::create_from_dom(body)
}

#[test]
fn a_rebuild_after_one_runtime_style_write_compares_only_the_written_node() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 900.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page(),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");

    // The fourth paragraph's opacity, written at runtime as a callback or a
    // transition frame writes it.
    let written = NodeId::new(7);
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("the page has a layout")
        .styled_dom
        .set_user_property_override_fast(
            &written,
            &[CssProperty::const_opacity(StyleOpacity::default())],
        );

    let mut next = page();
    let pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    assert_eq!(
        pending.node_moves.len(),
        next.node_data.as_ref().len(),
        "every node of the rebuilt page is matched"
    );
    // The written paragraph and its text: the override is an input of the
    // paragraph, and the text inherits from it.
    assert!(
        pending.compared <= 2,
        "the rebuild compared the properties of {} nodes; only the written paragraph (and its \
         text) had a cascade input that moved",
        pending.compared
    );
}
