//! A frame of a LAYOUT-property tween changes no DOM node - only the user
//! override the tween walks - so its relayout takes the retained layout tree
//! as it is and patches the display list, as the resize fast path does.
//!
//! Measured on the wave-8 AzWidgets (ANIMFRAME8), one frame of the switch
//! knob's `margin-left` glide: `reconcile_and_invalidate` 4.4 ms (fingerprints
//! and clones of all 2234 layout nodes, every one of them clean) and a FULL
//! display list 6 ms (`structure_ok` refused every css-dirty pass), around a
//! layout pass of 0.4 ms. The retained tree IS what the reconcile rebuilds for
//! such a frame: the reconcile reads node data and states, and an override is
//! neither.
//!
//! The latch is armed by the tween's tick and stamped with what makes the
//! claim true (node count, cascade epoch, every node's interaction state); a
//! pass handed a DOM that no longer matches the stamp reconciles as before.

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

/// A row of three cards with text, and a track whose knob glides by
/// `margin-left` (a declared 150 ms linear tween).
fn page() -> Dom {
    let card = |i: usize| {
        Dom::create_div_with_text(format!("card {i} with a few words of text"))
            .with_css("padding: 4px; margin: 2px; border: 1px solid #888888;")
    };
    Dom::create_body()
        .with_css("display: flex; flex-direction: column;")
        .with_child(card(0))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; width: 36px; \
                     height: 20px; padding: 2px; background: #cccccc;",
                )
                .with_child(Dom::create_div().with_class("knob".into()).with_css(
                    "width: 16px; height: 16px; flex-grow: 0; background: #ffffff; \
                     margin-left: 0px; animation: margin-left 150ms linear;",
                )),
        )
        .with_child(card(1))
        .with_child(card(2))
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
    let knob = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        (0..node_data.len())
            .map(NodeId::new)
            .find(|n| {
                node_data[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_class(), Some(s) if s == "knob"))
            })
            .expect("the page has a knob")
    };
    (lw, knob)
}

/// What the shells do for `ShouldIncrementalRelayout`: lay the EXISTING
/// styled DOM out again through the relayout entry.
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

fn knob_x(lw: &LayoutWindow, knob: NodeId) -> f32 {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(knob).into(),
    })
    .expect("the knob has a box")
    .origin
    .x
}

/// Toggle the knob to 16 px and settle the write's own relayout.
fn toggled() -> (LayoutWindow, NodeId) {
    let (mut lw, knob) = window();
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: knob,
        props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
            16,
        ))],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == knob),
        "harness: the knob's margin-left write seeds its tween"
    );
    (lw, knob)
}

#[test]
fn a_layout_tween_frame_reuses_the_retained_tree_and_patches_the_list() {
    let (mut lw, knob) = toggled();
    let mut xs = vec![knob_x(&lw, knob)];
    for frame in 0..4 {
        let _ = lw.tick_animations(0.016);
        assert!(
            lw.take_transition_relayout(),
            "frame {frame}: harness - a margin-left step owes a relayout"
        );
        relayout(&mut lw);
        assert!(
            lw.layout_cache.last_reconcile_was_skipped,
            "frame {frame}: only an override moved - the relayout must take the retained tree, \
             not reconcile the unchanged DOM (reused {}, fresh {})",
            lw.layout_cache.last_reconcile_reused, lw.layout_cache.last_reconcile_fresh
        );
        assert!(
            lw.frame_report.last_dl_build_patched,
            "frame {frame}: the frame moved one box - its display list must be PATCHED (the \
             knob re-emitted, the rest spliced), not built in full"
        );
        let mismatches = lw.verify_patched_display_list(DomId::ROOT_ID);
        assert!(
            mismatches.is_empty(),
            "frame {frame}: the patched list must equal a wholesale build of the same layout: \
             {mismatches:?}"
        );
        xs.push(knob_x(&lw, knob));
    }
    assert!(
        xs.windows(2).all(|w| w[1] >= w[0]) && xs.last() > xs.first(),
        "the knob's box must glide right frame by frame, frames: {xs:?}"
    );
}

#[test]
fn a_frame_whose_dom_moved_since_the_tick_reconciles() {
    let (mut lw, knob) = toggled();
    let _ = lw.tick_animations(0.016);
    assert!(
        lw.take_transition_relayout(),
        "harness: a margin-left step owes a relayout"
    );
    // Between the tick and its relayout, the pointer moves onto the first
    // card: a STATE change, which the reconcile must see (it re-cascades the
    // card's text).
    if let Some(lr) = lw.layout_results.get_mut(&DomId::ROOT_ID) {
        let mut states = lr.styled_dom.styled_nodes.as_container_mut();
        if let Some(n) = states.get_mut(NodeId::new(1)) {
            n.styled_node_state.hover = true;
        }
    }
    relayout(&mut lw);
    assert!(
        !lw.layout_cache.last_reconcile_was_skipped,
        "a node's interaction state changed after the tick - the relayout must reconcile"
    );
    assert!(
        knob_x(&lw, knob).is_finite(),
        "harness: the knob is still laid out"
    );
}
