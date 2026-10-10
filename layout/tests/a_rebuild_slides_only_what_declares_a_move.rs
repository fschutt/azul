//! A rebuild slides only what DECLARES a move (user, 2026-10-07: "it should
//! be opt-in"): `animation: move <duration> [timing]`, the diff-driven
//! `animation` list's entry for the node's place - read on the OLD tree like
//! every other entry of that list. Enters (`-azul-animation-in`) and exits
//! (`-azul-animation-out`) were already opt-in; moves were not: every node a
//! rebuild shifted got a spring FLIP, so a sheet's range selection slid the
//! cells under it, a PDF's sidebar animated and every map tile sprang after
//! a pan.

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

/// body > spacer (`spacer_px` tall) > target (`target_css`).
fn page(spacer_px: f32, target_css: &str) -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(Dom::create_div().with_css(&format!("width: 100px; height: {spacer_px}px;")))
            .with_child(
                Dom::create_div()
                    .with_id(azul_css::AzString::from("target"))
                    .with_css(&format!("width: 50px; height: 20px; {target_css}")),
            ),
    )
}

fn state() -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    ws
}

/// The target laid out, then the next DOM (the spacer 40px taller, so the
/// target moves down 40px) installed as the shells install one: the
/// reconciliation, the layout, its completion. Returns how many moves the
/// rebuild seeded.
fn moves_after_rebuild(target_css: &str) -> usize {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let ws = state();
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page(20.0, target_css),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    assert!(lw.animations.is_empty(), "harness: nothing moves before the rebuild");

    let mut next = page(60.0, target_css);
    let pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    assert!(pending.animate_moves, "harness: a state change, not a resize");
    lw.layout_new_generation(
        next,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the next DOM lays out");
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
    lw.animations.len()
}

#[test]
fn a_node_that_declares_no_move_lands_at_its_new_place() {
    assert_eq!(
        moves_after_rebuild(""),
        0,
        "the target moved 40px and declares no move: it is laid out there, it does not slide"
    );
}

#[test]
fn a_node_that_declares_a_move_slides_to_its_new_place() {
    assert_eq!(
        moves_after_rebuild("animation: move 200ms ease-out;"),
        1,
        "the target declares `animation: move`: the rebuild slides it from its old place"
    );
}
