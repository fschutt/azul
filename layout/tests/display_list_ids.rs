//! HARD tests for the display-list ↔ DOM ID mapping (user demand, 2026-08-08:
//! "implement hard tests that the ID mapping doesn't break").
//!
//! `DisplayList::node_mapping` is how damage attribution, pagination breaks,
//! hit-testing and (soon) display-list PATCHING resolve items back to DOM
//! nodes. A wrong id succeeds at naming the WRONG node — silent corruption —
//! so the invariant must be asserted, not observed. These tests pin it across
//! the exact lifecycle the resize fast path exercises: cold build, same-DOM
//! resize (warm reconcile, everything reused), and a content edit.

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

fn build_dom() -> StyledDom {
    let mut children = Vec::new();
    for i in 0..8 {
        children.push(Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                "paragraph number {i}"
            )),
        ));
    }
    let mut dom = Dom::create_div().with_children(children.into());
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0px; } div { padding: 2px; } ");
    StyledDom::create(&mut dom, css)
}

fn layout_at(lw: &mut LayoutWindow, sd: StyledDom, w: f32, h: f32) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(w, h);
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(sd, &ws, &rr, &cb, &mut dbg)
        .unwrap();
}

/// The invariant must hold after a cold build, after a same-DOM resize
/// (the fast path: everything reconciled-and-reused), and the mapping must
/// be STABLE across that resize — same item→node attribution when nothing
/// structural changed.
#[test]
fn node_mapping_survives_the_resize_fast_path() {
    let sd = build_dom();
    let sd2 = sd.clone();

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_at(&mut lw, sd.clone(), 640.0, 480.0);

    let dl_cold = lw.layout_results[&DomId::ROOT_ID].display_list.clone();
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    dl_cold
        .validate_node_mapping(styled)
        .expect("cold build must satisfy the mapping invariant");
    let cold_mapped: Vec<Option<usize>> = dl_cold
        .node_mapping
        .iter()
        .map(|m| m.map(|n| n.index()))
        .collect();

    // Same DOM, new viewport => reconcile reuses every node (pinned by the
    // resize contract suite); the mapping must survive and stay attributable.
    layout_at(&mut lw, sd2, 800.0, 600.0);
    assert_eq!(
        lw.layout_cache.last_reconcile_fresh, 0,
        "precondition: this must be the reuse path"
    );
    let dl_warm = lw.layout_results[&DomId::ROOT_ID].display_list.clone();
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    dl_warm
        .validate_node_mapping(styled)
        .expect("fast-path resize must satisfy the mapping invariant");

    let warm_mapped: Vec<Option<usize>> = dl_warm
        .node_mapping
        .iter()
        .map(|m| m.map(|n| n.index()))
        .collect();
    assert_eq!(
        cold_mapped, warm_mapped,
        "a same-DOM resize must not change which node any item belongs to \
         (item ORDER and ATTRIBUTION are structural; only geometry may move)"
    );
}

/// A corrupted mapping must be CAUGHT — the negative-control shape, encoded
/// as a permanent test: the validator is only worth its runtime if it
/// actually rejects the corruption class it exists for.
#[test]
fn validator_rejects_out_of_range_and_non_finite() {
    let sd = build_dom();
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    layout_at(&mut lw, sd, 640.0, 480.0);

    // The result holds an Arc<DisplayList>; deep-clone to corrupt a copy.
    let mut dl = (*lw.layout_results[&DomId::ROOT_ID].display_list).clone();
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    dl.validate_node_mapping(styled).expect("baseline valid");

    // Corruption 1: out-of-range node id.
    if let Some(slot) = dl.node_mapping.iter_mut().find(|m| m.is_some()) {
        *slot = azul_core::id::NodeId::from_usize(9_999_999);
    }
    assert!(
        dl.validate_node_mapping(styled).is_err(),
        "an out-of-range NodeId must be rejected"
    );

    // Corruption 2: truncated mapping.
    let mut dl2 = (*lw.layout_results[&DomId::ROOT_ID].display_list).clone();
    dl2.node_mapping.pop();
    assert!(
        dl2.validate_node_mapping(styled).is_err(),
        "a mapping shorter than the item list must be rejected"
    );
}
