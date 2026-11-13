//! A rebuild under the pointer starts no transition (ANIM8, 2026-10-03).
//!
//! `begin_reconciliation` diffs the old tree's cascade against the new one's
//! to find the properties a rebuild changed, and turns those a node declares
//! an `animation` for into transitions (the old tree governs). The new tree
//! comes out of the app's `layout()` with every node at rest: `:hover`,
//! `:active`, `:focus` are put back on it only AFTER the diff
//! (`apply_runtime_states_before_layout` in the shell). So a hovered button
//! that declares a fade read as "hover colour -> resting colour" on EVERY
//! rebuild: it faded out under the pointer and snapped back when the tween
//! ended and the re-applied `:hover` showed through - one flicker per click
//! on any button whose callback returns `RefreshDom`.
//!
//! The interaction states are the window's, not the DOM's: the diff reads the
//! new tree in the states its nodes are about to get back, like it reads it
//! in the window's dynamic-selector context
//! (`a_rebuild_transitions_only_what_its_window_sees_change`).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{HoverChange, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body > the button, which fades its background between rest and hover.
fn page() -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div().with_css(
            "width: 100px; height: 40px; background: #ff0000; \
             animation: background 200ms linear; \
             :hover { background: #0000ff; }",
        ),
    )
}

#[test]
fn a_rebuild_under_the_pointer_starts_no_transition() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");

    // The pointer is on the button (node 1: body is 0), and its fade is over.
    let button = NodeId::new(1);
    let _ = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![button],
            }),
            None,
        );
    let _ = lw.tick_animations(1.0);
    lw.css_transitions.clear();

    // The app rebuilds the same page (a click elsewhere, a counter).
    let mut next = StyledDom::create_from_dom(page());
    let _pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());

    assert!(
        lw.css_transitions.is_empty(),
        "nothing the window shows changed - the button is still under the pointer - so \
         nothing may fade; started: {:?}",
        lw.css_transitions
            .iter()
            .map(|t| (t.node, t.prop_type.to_str()))
            .collect::<Vec<_>>()
    );
}
