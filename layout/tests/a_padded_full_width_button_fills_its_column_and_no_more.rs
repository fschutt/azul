//! A `<button>` sizes its border box, as in every browser's UA stylesheet.
//!
//! Chrome, Firefox and Safari give buttons `box-sizing: border-box`, so a
//! `width: 100%` button with padding and a border is exactly as wide as its
//! column. Measured in headless Chrome: a padded `width: 100%` button in a
//! 290 px column is 290 px wide, and its computed `box-sizing` is
//! `border-box`. A list of such rows (a route's turn-by-turn steps) must not
//! stick out of its column and lose its right edge to the column's clip.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Lays `dom` out in a 400x300 window and answers the border-box width of every button in it,
/// in DOM order.
fn button_widths(dom: Dom) -> Vec<f32> {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("lays out");
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let buttons: Vec<usize> = sd
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, nd)| matches!(nd.get_node_type(), NodeType::Button))
        .map(|(i, _)| i)
        .collect();
    buttons
        .into_iter()
        .map(|i| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
            })
            .expect("the button has a rect")
            .size
            .width
        })
        .collect()
}

/// A padded button (the UA's 1 px border stays) with `extra` CSS.
fn padded_button(extra: &str) -> Dom {
    Dom::create_button_no_a11y("Turn left onto the main street".into())
        .with_css(&format!("padding: 10px; {extra}"))
}

#[test]
fn a_padded_full_width_button_in_a_290px_flex_column_is_290px_wide() {
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 290px;")
        .with_child(padded_button("width: 100%;"));
    let widths = button_widths(Dom::create_body().with_child(column));
    assert_eq!(widths.len(), 1);
    assert!(
        (widths[0] - 290.0).abs() <= 0.5,
        "Chrome draws the button 290 px wide (its border box), got {}",
        widths[0]
    );
}

#[test]
fn a_padded_full_width_button_in_a_290px_block_is_290px_wide() {
    let column = Dom::create_div()
        .with_css("width: 290px;")
        .with_child(padded_button("width: 100%;"));
    let widths = button_widths(Dom::create_body().with_child(column));
    assert!(
        (widths[0] - 290.0).abs() <= 0.5,
        "Chrome draws the button 290 px wide (its border box), got {}",
        widths[0]
    );
}

#[test]
fn a_button_told_to_size_its_content_box_still_grows_by_its_padding_and_border() {
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: flex-start; width: 290px;")
        .with_child(padded_button("width: 200px; box-sizing: content-box;"));
    let widths = button_widths(Dom::create_body().with_child(column));
    assert!(
        (widths[0] - 222.0).abs() <= 0.5,
        "200 px of content, 10 px of padding and 1 px of border each side: 222 px, got {}",
        widths[0]
    );
}
