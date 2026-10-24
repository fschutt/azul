//! A block whose margin a tween animates moves frame by frame, also on the
//! relayout that keeps the retained layout tree.
//!
//! A frame of a layout-property tween changes no DOM node, only the user
//! override the tween walks, so its relayout skips the reconcile and takes
//! the retained tree as it is. That tree holds the margins, borders and
//! paddings the OLD cascade resolved for each box: a block in block flow is
//! placed from them (a flex item is not, taffy reads its style afresh), so
//! the animated block stayed where it was (wave-9 N.1). A restyled box reads
//! its box props again, as the reconcile builds it fresh.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A block holding a block whose `margin-left` glides (a declared 150 ms
/// linear tween), with a paragraph after it.
fn page() -> Dom {
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(
            Dom::create_div().with_css("padding: 2px;").with_child(
                Dom::create_div().with_class("mover".into()).with_css(
                    "width: 16px; height: 16px; background: #000000; margin-left: 0px; \
                     animation: margin-left 150ms linear;",
                ),
            ),
        )
        .with_child(Dom::create_div_with_text("after the block"))
}

fn window() -> (LayoutWindow, NodeId) {
    let mut dom = page();
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    let mover = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        (0..node_data.len())
            .map(NodeId::new)
            .find(|n| {
                node_data[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_class(), Some(s) if s == "mover"))
            })
            .expect("the page has the mover")
    };
    (lw, mover)
}

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
    .expect("the relayout runs");
}

fn mover_x(lw: &LayoutWindow, mover: NodeId) -> f32 {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(mover).into(),
    })
    .expect("the mover has a box")
    .origin
    .x
}

#[test]
fn a_tweened_block_margin_moves_its_box_on_the_retained_tree() {
    let (mut lw, mover) = window();
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: mover,
        props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
            40,
        ))],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == mover),
        "harness: the margin-left write seeds its tween"
    );
    let mut xs = vec![mover_x(&lw, mover)];
    let mut skipped = 0;
    for _ in 0..6 {
        let _ = lw.tick_animations(0.016);
        if !lw.take_transition_relayout() {
            continue;
        }
        relayout(&mut lw);
        if lw.layout_cache.last_reconcile_was_skipped {
            skipped += 1;
        }
        xs.push(mover_x(&lw, mover));
    }
    assert!(skipped > 0, "harness: a tween frame takes the retained tree");
    assert!(
        xs.windows(2).all(|w| w[1] >= w[0]) && xs.last() > xs.first(),
        "the block must glide right frame by frame as its margin grows, frames: {xs:?}"
    );
}
