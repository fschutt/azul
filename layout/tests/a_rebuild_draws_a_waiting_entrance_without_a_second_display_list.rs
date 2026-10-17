//! A rebuild draws an entrance that waits for its first tick without building
//! its display list a second time.
//!
//! An entering node's `-azul-animation-in` track is drawn from its first
//! keyframe on the frame it comes in: `finish_reconciliation` publishes the
//! track's frame-zero sample and builds the list again, because the list of
//! that pass was built before the track had its keys. A track that has not
//! ticked yet (a spinner nobody can see is not driven) is published again on
//! every later rebuild, and the list was built again each time although its
//! node already had its keys when the pass built it: AzWidgets keeps 27 such
//! tracks, a second full display list (~7 ms) on every click.

use azul_core::{dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body, and when `panel` a box that fades up from nothing as it comes in.
fn page(panel: bool) -> Dom {
    let (keyframes, _) = azul_css::parser2::new_from_str(
        "@keyframes rise { from { opacity: 0; } to { opacity: 1; } }",
    );
    let body = Dom::create_body()
        .with_css("margin: 0;")
        .with_component_css(keyframes);
    if panel {
        body.with_child(Dom::create_div().with_css(
            "position: absolute; left: 0px; top: 0px; width: 100px; height: 40px; background: \
             #ff0000; -azul-animation-in: rise 300ms linear;",
        ))
    } else {
        body
    }
}

fn lay_out(lw: &mut LayoutWindow, ws: &FullWindowState, dom: Dom) {
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

#[test]
fn a_rebuild_draws_a_waiting_entrance_without_a_second_display_list() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lay_out(&mut lw, &ws, page(false));
    // The panel comes in; its entrance has not ticked yet.
    lay_out(&mut lw, &ws, page(true));
    assert_eq!(
        lw.live_tracks.len(),
        1,
        "premise: the panel's entrance runs"
    );

    // The app rebuilds the same page before the first tick.
    lw.frame_report.reset_counters();
    lay_out(&mut lw, &ws, page(true));
    assert_eq!(
        lw.live_tracks.len(),
        1,
        "premise: the entrance still waits"
    );
    assert_eq!(
        lw.frame_report.dl_rebuilds, 0,
        "the rebuild built its display list again for a track whose node had its keys already"
    );
}
