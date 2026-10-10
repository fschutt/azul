//! A keyboard focus ring survives a full relayout.
//!
//! The ring is appended by the caret/selection tween post-pass, which reads
//! the focused node's hierarchy from `layout_results`. A full relayout
//! clears that map at its start and inserts the new result only AFTER the
//! post-pass ran, so on every full-layout frame the ring lookup found
//! nothing and no ring was appended. Any widget whose focus callback
//! returns `RefreshDom` (AzWidgets' TextArea does) therefore ended the Tab
//! press on a ring-less frame: focus moved, nothing showed it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    geom::LogicalSize,
    resources::{RendererResources, SystemAnimations},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; }
    .btn { display: block; width: 120px; height: 30px; }
"#;

/// body=0 > btn1=1(text 2) > btn2=3(text 4)
fn styled() -> StyledDom {
    let btn = |label: &str| {
        let mut b = Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("btn".into())].into());
        b.set_tab_index(TabIndex::Auto);
        b.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            label,
        ))
    };
    let mut dom = Dom::create_body().with_child(btn("one")).with_child(btn("two"));
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn full_relayout(lw: &mut LayoutWindow) {
    let ws = lw.current_window_state.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled(), &ws, &rr, &sc, &mut dbg)
        .unwrap();
}

fn build() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(SystemAnimations::default());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    full_relayout(&mut lw);
    lw
}

fn focus_by_keyboard(lw: &mut LayoutWindow, node: usize) {
    lw.focus_manager.focus_is_visible = true;
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }));
}

/// The ring is always the LAST item (the post-pass appends it after
/// everything else).
fn ends_with_ring(lw: &LayoutWindow) -> bool {
    matches!(
        lw.get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .items
            .last(),
        Some(DisplayListItem::Border { .. })
    )
}

/// Control: the display-list-only rebuild has always shown the ring.
#[test]
fn a_display_list_rebuild_shows_the_ring() {
    let mut lw = build();
    focus_by_keyboard(&mut lw, 1);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert!(ends_with_ring(&lw), "the DL-only rebuild appends the ring");
}

/// The bug: the frame a full relayout produces must show the ring too.
#[test]
fn a_full_relayout_shows_the_ring() {
    let mut lw = build();
    focus_by_keyboard(&mut lw, 1);
    full_relayout(&mut lw);
    assert!(
        ends_with_ring(&lw),
        "a full relayout (what a RefreshDom after a focus change runs) must append the ring"
    );
}

/// The AzWidgets sequence: focus is already ringed, a callback forces a
/// full relayout, the ring must not vanish on that frame.
#[test]
fn a_ring_already_shown_survives_a_full_relayout() {
    let mut lw = build();
    focus_by_keyboard(&mut lw, 1);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert!(ends_with_ring(&lw));
    full_relayout(&mut lw);
    assert!(ends_with_ring(&lw), "the ring must survive the full relayout");
}
