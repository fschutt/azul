//! An absolutely positioned child of a flex container is placed against its
//! CONTAINING BLOCK - the nearest positioned ancestor, else the initial
//! containing block - not against the flex container it happens to sit in.
//!
//! Every absolutely positioned child of a flex or grid container skipped the
//! CSS containing-block pass and kept the flex engine's placement, which
//! treats every container as positioned. The Toast widget
//! (`position:absolute; bottom; right`) inside an unpositioned flex column
//! was pinned to the bottom of that column - a label-high strip - and painted
//! over the row above it in AzWidgets.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState};
use rust_fontconfig::FcFontCache;

/// (x, y, width, height) of every node, in DOM order, in a 400x300 window.
fn rects(dom: Dom) -> Vec<(f32, f32, f32, f32)> {
    let styled = StyledDom::create_from_dom(dom);
    let n = styled.node_data.as_container().len();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    (0..n)
        .map(|i| {
            let r = lw
                .get_node_layout_rect(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
                })
                .unwrap_or_default();
            (r.origin.x, r.origin.y, r.size.width, r.size.height)
        })
        .collect()
}

fn body() -> Dom {
    Dom::create_body().with_css("margin: 0; display: flex; flex-direction: column;")
}

const ABS: &str = "position: absolute; bottom: 10px; right: 10px; width: 50px; height: 20px;";

/// No positioned ancestor: bottom/right resolve against the initial
/// containing block (the 400x300 viewport).
#[test]
fn an_abspos_child_of_an_unpositioned_flex_column_uses_the_initial_containing_block() {
    // body(0) > col(1) > [div(2), abs(3)]
    let r = rects(
        body().with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(Dom::create_div().with_css("height: 40px;"))
                .with_child(Dom::create_div().with_css(ABS)),
        ),
    );
    assert_eq!((r[3].0, r[3].1), (340.0, 270.0), "abs box {:?}", r[3]);
}

/// Control: when the flex column IS positioned it is the containing block.
#[test]
fn a_positioned_flex_column_is_the_containing_block() {
    let r = rects(
        body().with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; position: relative;")
                .with_child(Dom::create_div().with_css("height: 40px;"))
                .with_child(Dom::create_div().with_css(ABS)),
        ),
    );
    // col spans y 0..40, x 0..400.
    assert_eq!((r[3].0, r[3].1), (340.0, 10.0), "abs box {:?}", r[3]);
}

/// The containing block is the nearest POSITIONED ancestor, even when an
/// unpositioned flex container sits in between.
#[test]
fn an_abspos_child_uses_the_nearest_positioned_ancestor_past_a_flex_parent() {
    // body(0) > [div(1), wrap(2) > col(3) > [div(4), abs(5)]]
    let r = rects(
        body()
            .with_child(Dom::create_div().with_css("height: 50px;"))
            .with_child(
                Dom::create_div()
                    .with_css("position: relative; height: 200px;")
                    .with_child(
                        Dom::create_div()
                            .with_css("display: flex; flex-direction: column;")
                            .with_child(Dom::create_div().with_css("height: 40px;"))
                            .with_child(Dom::create_div().with_css(ABS)),
                    ),
            ),
    );
    // wrap spans y 50..250.
    assert_eq!((r[5].0, r[5].1), (340.0, 220.0), "abs box {:?}", r[5]);
}

/// The Toast's shape: an auto-sized, padded FLEX box pinned bottom-right.
/// Its size is its content plus padding, counted once.
#[test]
fn an_auto_sized_padded_flex_abspos_box_is_content_plus_padding() {
    // body(0) > col(1) > [div(2), abs(3) > child(4)]
    let r = rects(
        body().with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(Dom::create_div().with_css("height: 40px;"))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "display: flex; position: absolute; bottom: 10px; right: 10px; \
                             padding: 12px;",
                        )
                        .with_child(Dom::create_div().with_css("width: 30px; height: 16px;")),
                ),
        ),
    );
    assert_eq!((r[3].2, r[3].3), (54.0, 40.0), "abs box {:?}", r[3]);
    assert_eq!((r[3].0, r[3].1), (336.0, 250.0), "abs box {:?}", r[3]);
}
