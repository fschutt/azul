//! A transformed box is hit where it is painted - its OWN box, not only its
//! descendants.
//!
//! The display list wraps a transformed element's own items (its background,
//! its hit-test area) in the element's reference frame, so the raster paints
//! the element moved. The CPU hit tester applied a node's transform to its
//! DESCENDANTS only (`compute_node_chains`: `t(n) = t(parent) (+ parent)`),
//! so the element's own box stayed clickable at its static position - where
//! nothing of it is painted - and shadowed whatever WAS painted there.
//!
//! Found by the /e2e corpus (`e2e/bug-transform-offsets-hit-test.json`,
//! HEADLESS6 2026-10-03; red in-process and against AzPaint): `#mover`
//! (translate(120px, 80px)) holds the focusable `#box`; `#below` is an
//! absolutely positioned focusable box at the movers static place. A click
//! there hit `#mover`'s untransformed box, which has nothing focusable in
//! its chain, and the click blurred instead of focusing `#below`.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > mover(1) > box(2); body > below(3).
const MOVER: NodeId = NodeId::new(1);
const BOX: NodeId = NodeId::new(2);
const BELOW: NodeId = NodeId::new(3);

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0;")
        .with_child(
            Dom::create_div()
                .with_css("width: 100px; height: 50px; transform: translate(120px, 80px);")
                .with_child(
                    Dom::create_div().with_css("width: 100px; height: 50px; background: red;"),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css("position: absolute; left: 0px; top: 0px; width: 100px; height: 50px;"),
        );
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

/// The nodes of the root dom under `(x, y)`, topmost first.
fn hits_at(lw: &LayoutWindow, x: f32, y: f32) -> Vec<NodeId> {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    let gpu = &lw.gpu_state_manager;
    let resolve_scroll = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
    let resolve_tf = |d: DomId, n: NodeId| {
        gpu.caches
            .get(&d)
            .and_then(|c| c.css_current_transform_values.get(&n))
            .copied()
    };
    tester
        .hit_test_scrolled(LogicalPosition::new(x, y), &resolve_scroll, &resolve_tf)
        .into_iter()
        .filter(|(d, _, _)| *d == DomId::ROOT_ID)
        .map(|(_, n, _)| n)
        .collect()
}

#[test]
fn a_transformed_box_is_hit_where_it_is_painted() {
    let lw = window();
    let painted = hits_at(&lw, 160.0, 105.0);
    assert!(
        painted.contains(&BOX),
        "harness: the transformed box's child is hit where it is painted, got {painted:?}"
    );
    assert!(
        painted.contains(&MOVER),
        "the transformed box itself is painted at (120,80)-(220,130): a press there hits it, \
         got {painted:?}"
    );
}

#[test]
fn a_transformed_box_is_not_hit_at_its_untransformed_place() {
    let lw = window();
    let hits = hits_at(&lw, 50.0, 25.0);
    assert!(
        !hits.contains(&MOVER),
        "nothing of the transformed box is painted at its static place (0,0)-(100,50): it must \
         not take a press there, got {hits:?}"
    );
    assert_eq!(
        hits.first(),
        Some(&BELOW),
        "the box painted there is the topmost hit, got {hits:?}"
    );
}
