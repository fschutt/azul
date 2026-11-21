//! A rebuild that changed one container's own paint compares that container
//! alone, and only the property types its cascade can answer differently.
//!
//! A click rebuilds AzWidgets' whole DOM and cost 85-300 ms: the diff that
//! decides what to restyle compared every matched node whose cascade inputs
//! moved type by type over every `CssPropertyType` (two cascade walks per
//! type), and counted a node as moved whenever its PARENT had moved, so one
//! changed container dragged its whole subtree into the comparison. A child
//! whose parent changed only what it does not inherit is unchanged, and a
//! node's values can only differ in the types either side declares or
//! inherits differently: any other type is its node type's UA default on
//! both sides.

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

/// A body holding one container (`background: <colour>`) of 30 paragraphs.
fn page(colour: &str) -> StyledDom {
    let mut container = Dom::create_div().with_css(&format!("background: {colour}; padding: 4px;"));
    for i in 0..30 {
        container = container.with_child(Dom::create_p_with_text(format!("Line {i}").as_str()));
    }
    StyledDom::create_from_dom(Dom::create_body().with_child(container))
}

#[test]
fn a_rebuild_compares_a_changed_container_alone_by_its_own_types() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 900.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page("#ff0000"),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");

    let mut next = page("#0000ff");
    let pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    assert_eq!(pending.node_moves.len(), next.node_data.as_ref().len(), "all matched");
    assert_eq!(
        pending.restyled.len(),
        1,
        "only the container's value changed: {:?}",
        pending.restyled
    );
    assert_eq!(
        pending.compared, 1,
        "its paragraphs and texts inherit nothing that moved, so they are not compared"
    );
    assert!(
        pending.compared_properties < 40,
        "the container compared {} property types; it declares a handful and inherits the rest \
         unchanged",
        pending.compared_properties
    );
}
