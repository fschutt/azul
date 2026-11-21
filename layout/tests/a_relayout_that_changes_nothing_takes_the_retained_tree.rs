//! A relayout that changes nothing takes the retained layout tree as it is:
//! no reconcile (4-7 ms on an AzWidgets-sized page, RUNTIME23's R9).
//!
//! The shell's producer diffs the pre-cascade DOM fingerprints and hands the
//! solver `dom_diff_clean`: every node proved unchanged with its ancestors,
//! on both the structure and the style tier. With that, no layout-relevant
//! CSS dirt and the same viewport, the reconcile could only rebuild the tree
//! it already has. Where something did change, it still reconciles.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn page() -> Dom {
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_div_with_text("a card with some words").with_css("padding: 8px;"))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; gap: 4px;")
                .with_child(Dom::create_div_with_text("one"))
                .with_child(Dom::create_div_with_text("two")),
        )
}

fn window() -> LayoutWindow {
    let mut dom = page();
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
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

/// Lays the window's own styled DOM out again, with the producer's diff
/// saying `clean` for every node.
fn relayout(lw: &mut LayoutWindow, clean: bool) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let n = result.styled_dom.node_data.as_ref().len();
    lw.layout_cache.dom_diff_clean = Some(vec![clean; n]);
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

fn rects(lw: &LayoutWindow) -> Vec<Option<LogicalRect>> {
    let n = lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .node_data
        .as_ref()
        .len();
    (0..n)
        .map(|i| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: Some(NodeId::new(i)).into(),
            })
        })
        .collect()
}

#[test]
fn an_unchanged_page_is_not_reconciled_again() {
    let mut lw = window();
    let before = rects(&lw);
    relayout(&mut lw, true);
    assert!(
        lw.layout_cache.last_reconcile_was_skipped,
        "every node proved unchanged: the retained tree is the answer (reused {}, fresh {})",
        lw.layout_cache.last_reconcile_reused,
        lw.layout_cache.last_reconcile_fresh
    );
    assert_eq!(rects(&lw), before, "and lays out exactly as before");
}

#[test]
fn a_page_with_a_changed_node_is_still_reconciled() {
    let mut lw = window();
    relayout(&mut lw, false);
    assert!(
        !lw.layout_cache.last_reconcile_was_skipped,
        "nodes not proved unchanged go through the reconcile"
    );
}
