//! A caller that needs real text clusters must read the MATERIALIZED layout.
//!
//! `get_inline_layout_for_node` hands back the SPARSE `UnifiedLayout`, which
//! under the default dense text path is a shared EMPTY retirement sentinel.
//! `get_first_cluster_cursor()` on it is `None` for every node, so any caller
//! that gave up on that `None` silently did nothing - which is exactly how
//! Ctrl+A came to log "blocks have no first/last cluster cursor" and select
//! nothing at all.

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
fn the_materialized_layout_has_clusters_where_the_sparse_one_may_not() {
    let mut dom = Dom::create_body().with_child(Dom::create_p_with_text("alpha beta"));
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    // body(0) > p(1)
    let p = NodeId::new(1);
    let materialized = lw
        .materialized_inline_layout_for_node(DomId::ROOT_ID, p)
        .expect("the paragraph has an inline layout");
    assert!(
        materialized.get_first_cluster_cursor().is_some(),
        "the materialized layout must carry the clusters a caller asks for"
    );
    assert!(
        materialized.get_last_cluster_cursor().is_some(),
        "both ends, or a select-all cannot name its range"
    );

    // The trap this exists for: on the dense path the sparse view is the
    // empty sentinel. If that ever stops being true the law above still
    // holds, so this is reported rather than asserted.
    let sparse_has_clusters = lw
        .get_inline_layout_for_node(DomId::ROOT_ID, p)
        .and_then(|l| l.get_first_cluster_cursor())
        .is_some();
    println!("sparse view carries clusters: {sparse_has_clusters}");
}
