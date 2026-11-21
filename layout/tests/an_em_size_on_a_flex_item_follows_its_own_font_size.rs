//! An `em` size on a flex item follows the item's own font size.
//!
//! CSS Values 4 §6.1.1: `em` is the computed font size of the element the
//! length is used on. A flex or grid item's `width` / `height` and its
//! `min-` / `max-` sizes reached taffy through a conversion that resolved
//! `em` against a constant 16px, although the item's font size was at hand:
//! an icon drawn as a picture (`width: 1em; height: 1em` on an `<img>` whose
//! font size is 46px, AzPlayer's start strip under flora) came out 16x16.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The rect of `#item` styled `item` in a `display` (`flex` or `grid`) row.
fn item_rect(display: &str, item: &str) -> LogicalRect {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!("display: {display}; width: 400px; height: 200px;"))
            .with_child(
                Dom::create_div()
                    .with_id("item".into())
                    .with_css(&format!("flex-shrink: 0; align-self: flex-start; {item}")),
            ),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the row lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.has_id("item"))
        .expect("the item");
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .expect("the item has a rect")
}

#[test]
fn an_em_width_and_height_on_a_flex_item_are_its_own_font_size() {
    let rect = item_rect("flex", "font-size: 40px; width: 2em; height: 1em;");
    assert!(
        (rect.size.width - 80.0).abs() < 0.5 && (rect.size.height - 40.0).abs() < 0.5,
        "2em x 1em at a 40px font is 80 x 40, got {:?}",
        rect.size
    );
}

#[test]
fn an_em_min_width_on_a_flex_item_is_its_own_font_size() {
    let rect = item_rect("flex", "font-size: 20px; width: 0px; min-width: 3em; height: 10px;");
    assert!(
        (rect.size.width - 60.0).abs() < 0.5,
        "min-width 3em at a 20px font is 60, got {:?}",
        rect.size
    );
}

#[test]
fn an_em_max_height_on_a_flex_item_is_its_own_font_size() {
    let rect = item_rect("flex", "font-size: 10px; width: 10px; height: 100px; max-height: 2em;");
    assert!(
        (rect.size.height - 20.0).abs() < 0.5,
        "max-height 2em at a 10px font is 20, got {:?}",
        rect.size
    );
}

#[test]
fn an_em_width_on_a_grid_item_is_its_own_font_size() {
    let rect = item_rect("grid", "font-size: 30px; width: 2em; height: 10px;");
    assert!(
        (rect.size.width - 60.0).abs() < 0.5,
        "2em at a 30px font is 60, got {:?}",
        rect.size
    );
}
