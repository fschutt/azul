//! A restyle of a box keeps the text inside it when the text's own style did
//! not move: hovering a toolbar whose `:hover` only recolours its background
//! keeps every label's shaped lines (the AzDrive ribbon rebuilt and shaped its
//! ~120 labels again on every hover, ~8 ms a pass). A restyle that does reach
//! the text - an inherited colour - still rebuilds it.

use azul_core::{
    dom::DomId,
    geom::LogicalSize,
    id::NodeId,
    resources::RendererResources,
    styled_dom::{HoverChange, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const LABELS: usize = 12;

fn markup(hover_rule: &str) -> String {
    let labels: String = (0..LABELS).map(|i| format!("<p>label {i}</p>")).collect();
    format!(
        "<style>.bar {{ padding: 4px; }} .bar:hover {{ {hover_rule} }}</style>\
         <div class=\"bar\">{labels}</div>"
    )
}

fn window(markup: &str) -> LayoutWindow {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

fn bar(lw: &LayoutWindow) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class("bar"))
        .expect("the bar")
}

/// The pointer enters (`true`) or leaves the bar, and the window lays its
/// restyled DOM out again; returns how many layout nodes were built fresh.
fn hover(lw: &mut LayoutWindow, enter: bool) -> usize {
    let bar = bar(lw);
    let mut result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let change = if enter {
        HoverChange { left_nodes: Vec::new(), entered_nodes: vec![bar] }
    } else {
        HoverChange { left_nodes: vec![bar], entered_nodes: Vec::new() }
    };
    let _ = result.styled_dom.restyle_on_state_change(None, Some(change), None);
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
    lw.layout_cache.last_reconcile_fresh
}

#[test]
fn a_hover_that_only_recolours_the_bar_rebuilds_no_label() {
    let mut lw = window(&markup("background: #ff0000;"));
    // The first restyle above a label reads its style (it has none yet).
    let _ = hover(&mut lw, true);
    let _ = hover(&mut lw, false);
    for round in 0..3 {
        let fresh_in = hover(&mut lw, true);
        let fresh_out = hover(&mut lw, false);
        assert_eq!(
            (fresh_in, fresh_out),
            (0, 0),
            "round {round}: the labels' style did not move, so none is built again"
        );
    }
}

#[test]
fn a_hover_that_recolours_the_labels_rebuilds_them() {
    let mut lw = window(&markup("color: #ff0000;"));
    let _ = hover(&mut lw, true);
    let _ = hover(&mut lw, false);
    let fresh_in = hover(&mut lw, true);
    assert!(
        fresh_in >= LABELS,
        "an inherited colour reaches every label: {fresh_in} fresh of {LABELS} labels"
    );
    let fresh_out = hover(&mut lw, false);
    assert!(fresh_out >= LABELS, "and back: {fresh_out} fresh");
}
