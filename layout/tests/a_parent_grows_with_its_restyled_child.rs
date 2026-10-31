//! A block whose height changes on a relayout grows (or shrinks) its
//! auto-height parent, and moves what follows the parent, the way a fresh
//! layout of the same page does.
//!
//! An empty block in a block is re-solved as its own layout root. Its
//! siblings were re-stacked inside its parent, but nothing re-sized the
//! ancestors whose height is their content's: the child came out 60 tall,
//! its parent stayed 20 and the next block overlapped it (LAYOUTPERF8 bug
//! C; `scripts/layoutperf8_e2e/a_parent_grows_with_its_restyled_child.json`).
//! A child holding text is laid out by its parent's inline formatting
//! context, which hid it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutHeight, property::CssProperty};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `#parent` (auto height, holding `#child`), then `#after`, in a column;
/// the child `child_height` tall. The ids are the node order: root 1,
/// parent 2, child 3, after 4 (the body is 0).
fn page(child_height: u32) -> Dom {
    Dom::create_body()
        .with_css("margin: 0px; padding: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 100%;")
                .with_child(
                    Dom::create_div()
                        .with_css("background: #eeeeee;")
                        .with_child(
                            Dom::create_div()
                                .with_css(&format!("width: 50px; height: {child_height}px;")),
                        ),
                )
                .with_child(Dom::create_div().with_css("height: 20px;")),
        )
}

const PARENT: usize = 2;
const CHILD: usize = 3;
const AFTER: usize = 4;

fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) {
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

fn rect(lw: &LayoutWindow, node: usize) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(NodeId::new(node)).into(),
    })
    .expect("laid out")
}

fn assert_grown(lw: &LayoutWindow, how: &str) {
    let child = rect(lw, CHILD);
    let parent = rect(lw, PARENT);
    let after = rect(lw, AFTER);
    assert!(
        (child.size.height - 60.0).abs() < 0.01,
        "{how}: the child is 60 tall: {child:?}"
    );
    assert!(
        (parent.size.height - 60.0).abs() < 0.01,
        "{how}: the auto-height parent grows with it, as in a fresh layout: {parent:?}"
    );
    assert!(
        (after.origin.y - (parent.origin.y + 60.0)).abs() < 0.01,
        "{how}: the block after the parent moves down below it: parent {parent:?}, after \
         {after:?}"
    );
}

#[test]
fn a_fresh_layout_of_the_taller_page_is_the_reference() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(60));
    assert_grown(&lw, "fresh");
}

#[test]
fn a_rebuilt_page_whose_child_grew_grows_the_parent() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(20));
    assert!((rect(&lw, PARENT).size.height - 20.0).abs() < 0.01, "harness: 20 tall first");
    lay_out(&mut lw, page(60));
    assert_grown(&lw, "rebuilt");
}

#[test]
fn a_child_restyled_in_place_grows_the_parent() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(20));
    let prop = CssProperty::height(LayoutHeight::px(60.0));
    let scope = prop.get_type().relayout_scope(false);
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(CHILD),
        props: vec![prop],
        override_only: true,
    });
    lw.pending_css_dirty = Some((DomId::ROOT_ID, vec![(NodeId::new(CHILD), scope)]));
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
    assert_grown(&lw, "restyled");
}

#[test]
fn a_child_that_shrinks_shrinks_the_parent_back() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(60));
    lay_out(&mut lw, page(20));
    let parent = rect(&lw, PARENT);
    let after = rect(&lw, AFTER);
    assert!((parent.size.height - 20.0).abs() < 0.01, "{parent:?}");
    assert!((after.origin.y - (parent.origin.y + 20.0)).abs() < 0.01, "{after:?}");
}
