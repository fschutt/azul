//! A pass hands its layout tree to the window's result for that DOM
//! (`DomLayoutResult::layout_tree`) and keeps it in the solver's cache for
//! the next pass. Both hold the SAME tree: the result copied it, node by
//! node, on every pass - a fixed cost of every frame that lays out
//! anything, proportional to the page (3587 nodes for AzWidgets).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn page() -> StyledDom {
    let mut body = Dom::create_body();
    for i in 0..50 {
        body = body.with_child(Dom::create_div_with_text(format!("row {i}").as_str()));
    }
    StyledDom::create_from_dom(body)
}

#[test]
fn the_window_and_the_layout_cache_share_one_tree() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 600.0);
    lw.current_window_state = ws.clone();
    for pass in 0..2 {
        let dom = match lw.layout_results.remove(&DomId::ROOT_ID) {
            Some(previous) => previous.styled_dom,
            None => page(),
        };
        lw.layout_and_generate_display_list(
            dom,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let cached = lw.layout_cache.tree.as_ref().expect("the cache keeps the tree");
        let stored = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
        assert_eq!(stored.nodes.len(), cached.nodes.len(), "pass {pass}: harness");
        assert!(
            core::ptr::eq(stored.nodes.as_ptr(), cached.nodes.as_ptr()),
            "pass {pass}: the window's result holds a copy of the cached tree, not the tree"
        );
    }
}
