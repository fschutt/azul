//! A drag that ends where it started selects nothing, and Backspace still
//! deletes a character.
//!
//! The same position between two graphemes has two cursors: `Trailing` on
//! the first and `Leading` on the second. Press on the right half of the
//! 'e' in "hello", move one pixel onto the left half of the 'l', and the
//! drag path compared those as cluster ids - `anchor == focus` is false - so
//! it stored a RANGE from one to the other. That range covers nothing, and
//! `delete_range` deletes nothing over it: Backspace and Delete were dead
//! until the next click.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, Selection},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > div.host[contenteditable](1) > p(2) > text(3) "hello"`
const HOST: usize = 1;
const P: usize = 2;

fn hello() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .host { display: block; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "hello",
                    )),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// A window point `fraction` of the way across cluster `index` of the
/// paragraph, vertically in its middle.
fn across_cluster(lw: &LayoutWindow, index: usize, fraction: f32) -> LogicalPosition {
    let origin = lw
        .get_node_layout_rect(dnid(P))
        .expect("the paragraph is laid out")
        .origin;
    let layout = lw
        .materialized_inline_layout_for_node(DomId::ROOT_ID, NodeId::new(P))
        .expect("the paragraph has an inline layout");
    let (position, advance, height) = layout
        .items
        .iter()
        .filter_map(|item| match &item.item {
            ShapedItem::Cluster(c) => Some((item.position, c.advance, item.item.bounds().height)),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("the paragraph has a cluster {index}"));
    LogicalPosition::new(
        origin.x + position.x + advance * fraction,
        origin.y + position.y + height * 0.5,
    )
}

#[test]
fn a_drag_across_a_glyph_edge_leaves_a_caret_and_backspace_deletes() {
    let mut lw = hello();
    // Right half of the 'e' (cluster 1): the caret AFTER it.
    let press = across_cluster(&lw, 1, 0.75);
    // Left half of the first 'l' (cluster 2): the caret BEFORE it - the same
    // place.
    let release = across_cluster(&lw, 2, 0.25);

    lw.process_mouse_click_for_selection(press, 0)
        .expect("the press lands in the paragraph");
    let anchor = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("premise: the press placed a caret");
    assert_eq!(
        (anchor.cluster_id.start_byte_in_run, anchor.affinity),
        (1, CursorAffinity::Trailing),
        "premise: the press is on the right half of the 'e'"
    );
    lw.process_mouse_drag_for_selection(press, release)
        .expect("the drag is handled");

    let selection = lw
        .text_edit_manager
        .multi_cursor
        .as_ref()
        .and_then(|mc| mc.get_primary())
        .map(|sel| sel.selection);
    assert!(
        matches!(selection, Some(Selection::Cursor(_))),
        "a drag that moved to the same position selects nothing: {selection:?}"
    );

    lw.delete_selection(dnid(HOST), false)
        .expect("Backspace deletes the 'e'");
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(P));
    assert_eq!(lw.extract_text_from_inline_content(&content), "hllo");
}
