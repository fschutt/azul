//! Pin for the 2026-08-29 lockstep-scroll bug: the display-list scroll id was
//! a hash of the node_data_fingerprint — pure CONTENT, no identity — so two
//! structurally identical widgets (NumberInput literally returns
//! `TextInput::dom()`) shared one id: WebRender applied any offset to both
//! (lockstep), and the last-write-wins reverse map dropped the loser's
//! offsets entirely (its caret reveal never reached the compositor).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn two_identical_scrollers_get_two_scroll_ids() {
    // Two class-identical scroll boxes: identical fingerprints by design.
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div().with_class("scroller".into()))
        .with_child(Dom::create_div().with_class("scroller".into()));
    let (css, _warn) = azul_css::parser2::new_from_str(
        ".scroller { overflow-x: auto; overflow-y: auto; width: 100px; height: 50px; }",
    );
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut dbg,
    )
    .unwrap();

    let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
    let ids: Vec<u64> = lr.scroll_ids.values().copied().collect();
    let mut dedup = ids.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        ids.len(),
        "every scroller must own a distinct scroll id, got {ids:?}"
    );

    // Both divs survive in the id -> node map (the old content hash made the
    // second insert overwrite the first).
    let mapped: Vec<NodeId> = lr.scroll_id_to_node_id.values().copied().collect();
    assert!(
        mapped.contains(&NodeId::new(1)) && mapped.contains(&NodeId::new(2)),
        "both identical scrollers must be addressable, got {mapped:?}"
    );
}
