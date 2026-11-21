//! A node that comes in is drawn from its first keyframe (AZPLAYER11, 2026-10-07).
//!
//! `finish_reconciliation` starts an entering node's `-azul-animation-in` track, but the
//! track's keys and its frame-zero values reached the GPU value cache only on the first real
//! tick (`run_track_frames`), and the display list of the rebuild had been built with the
//! layout - before the reconciliation finished - without the node's opacity group. So the frame
//! the rebuild presented showed the node AT REST, its end state, fully opaque; the next frame
//! jumped to its first keyframe and began the animation: one flash of the finished page on
//! every page that comes in (each of AzPlayer's pages, each slide of its slide show).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
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
fn a_node_that_comes_in_is_drawn_from_its_first_keyframe() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lay_out(&mut lw, &ws, page(false));

    // The app rebuilds with the panel: it comes in, fading up from nothing.
    lay_out(&mut lw, &ws, page(true));
    assert_eq!(lw.live_tracks.len(), 1, "premise: the panel's entrance runs");
    let panel = *lw.live_tracks.keys().next().expect("the panel's track");

    // The list of THIS frame - no tick has run yet - draws the panel at its first keyframe.
    let cache = lw.gpu_state_manager.get_cache(DomId::ROOT_ID);
    let key = cache.and_then(|c| c.anim_opacity_keys.get(&panel).copied());
    let value = cache.and_then(|c| c.anim_current_opacity_values.get(&panel).copied());
    let list = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list;
    let drawn_from_nothing = list.items.iter().any(|item| match item {
        DisplayListItem::PushOpacity {
            opacity,
            opacity_key,
            ..
        } => match opacity_key {
            Some(k) => Some(*k) == key && value.is_some_and(|v| v < 0.05),
            None => *opacity < 0.05,
        },
        _ => false,
    });
    assert!(
        drawn_from_nothing,
        "the frame the rebuild shows must draw the panel at opacity 0 (its first keyframe), not \
         at rest; its opacity key {key:?}, value {value:?}"
    );
}
