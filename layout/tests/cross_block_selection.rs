//! AZUL-STILL-TODO C9/C10: selection spanning multiple sibling blocks and
//! the selection-spanning delete.
//!
//! - `set_cross_block_selection` precomputes the per-IFC ranges (anchor
//!   node from its cursor to its end, middles fully, focus node from its
//!   start to its cursor) and stores them render-ready; the display-list
//!   pass consumes them through `build_text_selections_map`.
//! - `delete_cross_block_selection` trims the two end nodes through the
//!   text overlay and emits ONE `RemoveChildren` structural changeset for
//!   the fully-covered middles; the caret collapses to the selection
//!   start. (Word-style paragraph MERGE of the two remaining part-blocks
//!   is the separate merge gap, not part of this slice.)

use azul_core::dom::{Dom, DomId, IdOrClass, NodeId};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::StyledDom;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn node_id(n: usize) -> NodeId {
    NodeId::new(n)
}

fn layout_three_paragraphs() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let class = |name: &str| -> azul_core::dom::IdOrClassVec {
        vec![IdOrClass::Class(name.into())].into()
    };
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text("first paragraph")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text("second paragraph")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text("third paragraph")),
        );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    layout_window.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// Node layout: body=0, div1=1, text=2, div2=3, text=4, div3=5, text=6.
const P1: usize = 1;
const P2: usize = 3;
const P3: usize = 5;

#[test]
fn cross_block_selection_builds_ranges_for_every_spanned_block() {
    let mut lw = layout_three_paragraphs();
    let ok = lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(P1),
        cursor(6), // after "first "
        node_id(P3),
        cursor(5), // before " paragraph" in "third paragraph"
    );
    assert!(ok, "sibling blocks must accept a cross-block selection");

    let map = lw.text_edit_manager.build_text_selections_map();
    let sel = map.get(&DomId::ROOT_ID).expect("selection for the root DOM");
    assert!(sel.is_forward);
    assert_eq!(
        sel.affected_nodes.len(),
        3,
        "anchor + middle + focus: {:?}",
        sel.affected_nodes
    );
    let r1 = sel.affected_nodes.get(&node_id(P1)).expect("anchor range");
    assert_eq!(r1.start.cluster_id.start_byte_in_run, 6);
    assert_eq!(
        r1.end.cluster_id.start_byte_in_run as usize,
        "first paragraph".len(),
        "anchor node selects to its end"
    );
    let r2 = sel.affected_nodes.get(&node_id(P2)).expect("middle range");
    assert_eq!(r2.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(
        r2.end.cluster_id.start_byte_in_run as usize,
        "second paragraph".len(),
        "middle node is fully selected"
    );
    let r3 = sel.affected_nodes.get(&node_id(P3)).expect("focus range");
    assert_eq!(r3.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(r3.end.cluster_id.start_byte_in_run, 5);
}

#[test]
fn backward_cross_block_selection_normalizes_to_document_order() {
    let mut lw = layout_three_paragraphs();
    let ok = lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(P3),
        cursor(5),
        node_id(P1),
        cursor(6),
    );
    assert!(ok);
    let map = lw.text_edit_manager.build_text_selections_map();
    let sel = map.get(&DomId::ROOT_ID).unwrap();
    assert!(!sel.is_forward, "anchor after focus = backward selection");
    assert_eq!(sel.affected_nodes.len(), 3);
    // Ranges are stored in DOCUMENT order regardless of drag direction.
    assert_eq!(
        sel.affected_nodes.get(&node_id(P1)).unwrap().start.cluster_id.start_byte_in_run,
        6
    );
    assert_eq!(
        sel.affected_nodes.get(&node_id(P3)).unwrap().end.cluster_id.start_byte_in_run,
        5
    );
}

#[test]
fn non_siblings_are_rejected() {
    let mut lw = layout_three_paragraphs();
    // text node 2 is a CHILD of P1, not a sibling of P3.
    let ok = lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(2),
        cursor(0),
        node_id(P3),
        cursor(1),
    );
    assert!(!ok, "v1 requires sibling IFC roots");
    assert!(lw.text_edit_manager.get_cross_block_selection().is_none());
}

#[test]
fn selection_spanning_delete_trims_ends_and_emits_remove_children() {
    let mut lw = layout_three_paragraphs();
    assert!(lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(P1),
        cursor(6),
        node_id(P3),
        cursor(6), // after "third "
    ));
    let changeset_id = lw.delete_cross_block_selection();
    assert!(
        changeset_id.is_some(),
        "one fully-covered middle block must emit a structural changeset"
    );

    // End nodes trimmed through the overlay (no DOM mutation).
    let text_of = |lw: &LayoutWindow, n: usize| -> String {
        lw.get_text_before_textinput(DomId::ROOT_ID, node_id(n))
            .iter()
            .filter_map(|c| match c {
                azul_layout::text3::cache::InlineContent::Text(run) => {
                    Some(run.text.clone())
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(text_of(&lw, P1), "first ", "anchor keeps its head");
    assert_eq!(text_of(&lw, P3), "paragraph", "focus keeps its tail");

    // The middle block is emitted as RemoveChildren on the body.
    let edit = lw
        .get_pending_document_edit()
        .expect("structural changeset pending");
    match &edit.operation {
        azul_layout::managers::changeset::DocumentOperation::RemoveChildren(r) => {
            assert_eq!(r.parent.node.into_crate_internal(), Some(node_id(0)));
            assert_eq!((r.start, r.end), (1, 2), "removes exactly the middle div");
        }
        other => panic!("expected RemoveChildren, got {other:?}"),
    }

    // Caret collapsed at the selection start, selection cleared.
    assert!(lw.text_edit_manager.get_cross_block_selection().is_none());
    let mc = lw.text_edit_manager.multi_cursor.as_ref().expect("caret");
    assert_eq!(mc.node_id.node.into_crate_internal(), Some(node_id(P1)));
}
