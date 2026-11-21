//! A block of many out-of-flow children is looked at once per child.
//!
//! Every box that holds text asks `markers_on_first_line` whether it starts its parent's first
//! line (a list item's `::marker` rides it), and an anonymous block asks whether it holds its
//! container's first formatted line (`text-indent`). Both asked by scanning the parent's
//! children for the first in-flow one: in a block of k absolutely positioned labels - a map's
//! label layer - no child is in flow, so each of the k labels scanned all k: k^2 style lookups,
//! 155 million at 12,470 labels, 40 % of a cold settle after a zoom. Each box is now asked about
//! itself and its earlier siblings only until the first that is in flow.

use azul_core::{
    dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks, solver3::fc::IN_FLOW_CHECKS, window::LayoutWindow,
    window_state::FullWindowState,
};

/// How many boxes were asked whether they are in flow while a block of `k` absolutely
/// positioned spans with a text was laid out.
fn checks_for(k: usize) -> usize {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        layer = layer.with_child(Dom::create_span_with_text(format!("label {i}")).with_css(&format!(
            "position: absolute; left: {}px; top: {}px;",
            (i % 40) * 14,
            (i / 40) * 12
        )));
    }
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(layer);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    IN_FLOW_CHECKS.with(|n| n.set(0));
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    IN_FLOW_CHECKS.with(std::cell::Cell::get)
}

#[test]
fn a_block_of_out_of_flow_labels_asks_once_per_label_whether_it_starts_the_first_line() {
    let (small, large) = (checks_for(250), checks_for(1_000));
    assert!(small >= 250, "harness: the labels were asked about ({small})");
    assert!(
        large <= small * 5,
        "four times the labels took {large} checks for {small}: more than linear"
    );
}
