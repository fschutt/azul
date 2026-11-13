//! A `VirtualView`'s child DOM that shrinks between renders lays out against
//! its own cache from the last render: the reconcile matches the smaller DOM
//! to the tree it kept, as it does for a root DOM that loses nodes, and no
//! box keeps a node the new DOM no longer has (a map zoomed out from sixteen
//! tiles to four).

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// How many tiles the view shows on its next render.
struct Tiles {
    count: usize,
}

extern "C" fn render_tiles(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let count = data.downcast_ref::<Tiles>().map_or(0, |t| t.count);
    let mut grid = Dom::create_div().with_css("display: flex; flex-wrap: wrap; width: 400px;");
    for _ in 0..count {
        grid = grid.with_child(Dom::create_div().with_css("width: 100px; height: 100px;"));
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(400.0, 400.0));
    VirtualViewReturn::with_dom(grid, rect, rect)
}

/// A new generation of the host with a dataset of its own: its view renders
/// again.
fn host_page(count: usize) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(Tiles { count }),
            VirtualViewCallback::create(render_tiles),
        )
            .with_css("width: 400px; height: 400px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn lay_out(lw: &mut LayoutWindow, count: usize) {
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        host_page(count),
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the host lays out");
}

#[test]
fn a_virtual_views_child_dom_that_shrinks_lays_out_from_its_own_cache() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    lay_out(&mut lw, 16);
    lay_out(&mut lw, 16);
    lay_out(&mut lw, 4);
    lay_out(&mut lw, 4);

    let child = *lw
        .layout_results
        .keys()
        .find(|d| **d != DomId::ROOT_ID)
        .expect("the view's child DOM was laid out");
    let nodes = lw.layout_results[&child].styled_dom.node_data.as_ref().len();
    assert_eq!(nodes, 5, "the grid and its four tiles");
    for i in 1..nodes {
        let rect = lw
            .get_node_layout_rect(DomNodeId {
                dom: child,
                node: Some(NodeId::new(i)).into(),
            })
            .expect("every tile has a box");
        assert_eq!(
            (rect.size.width, rect.size.height),
            (100.0, 100.0),
            "tile {i} is laid out as the new DOM says"
        );
    }
    let tree = lw.layout_results[&child].layout_tree.clone();
    for node in &tree.nodes {
        if let Some(dom_node) = node.dom_node_id {
            assert!(dom_node.index() < nodes, "a box keeps node {dom_node:?} the DOM no longer has");
        }
    }
}
