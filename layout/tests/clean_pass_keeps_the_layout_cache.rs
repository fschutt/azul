//! A layout pass with nothing to lay out keeps the cache it was handed.
//!
//! `layout_document` moves the per-node cache out of `LayoutCache` for the
//! duration of a pass and moves it back at the end. The "nothing changed,
//! re-emit the display list" fast path returned from the middle of that,
//! which left the window holding an EMPTY cache: the next pass re-measured
//! the whole tree, and - because each entry also records the containing
//! block its parent handed it - a dirty subtree re-solved on its own had to
//! guess that block from its parent's grown used size. A `height: 100%` body
//! under an auto-height `<html>` grew by the menu bar above it on the first
//! restyle after any idle frame.

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

fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
}

/// How much of the per-node cache is populated.
fn cached_nodes(lw: &LayoutWindow) -> usize {
    lw.layout_cache
        .cache_map
        .entries
        .iter()
        .filter(|e| e.last_containing_block.is_some() || e.layout_entry.is_some())
        .count()
}

#[test]
fn a_pass_with_nothing_to_do_keeps_the_per_node_cache() {
    let mut dom = Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%;")
            .with_child(Dom::create_div().with_css("height: 38px;")),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    let after_layout = cached_nodes(&lw);
    assert!(
        after_layout > 0,
        "harness: the first pass populated the cache"
    );

    // Nothing changed: the pass re-emits the display list and lays out
    // nothing. It must still hand the cache back.
    relayout(&mut lw);
    assert_eq!(
        cached_nodes(&lw),
        after_layout,
        "the idle pass dropped the layout cache it had taken"
    );
}
