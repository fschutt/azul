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
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("first paragraph")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("second paragraph")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("third paragraph")),
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
    // One range per spanned block: a cross-block selection never multi-selects
    // inside a block (that is the Ctrl+D session's job).
    for (node, ranges) in &sel.affected_nodes {
        assert_eq!(ranges.len(), 1, "node {node:?} contributes exactly one range");
    }
    let r1 = sel.get_range_for_node(&node_id(P1)).expect("anchor range");
    assert_eq!(r1.start.cluster_id.start_byte_in_run, 6);
    // End = Trailing on the LAST cluster ("after the final grapheme"), so
    // the byte names the final cluster's START, not the string length.
    assert_eq!(
        r1.end.cluster_id.start_byte_in_run as usize,
        "first paragraph".len() - 1,
        "anchor end sits on the last cluster (Trailing)"
    );
    let r2 = sel.get_range_for_node(&node_id(P2)).expect("middle range");
    assert_eq!(r2.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(
        r2.end.cluster_id.start_byte_in_run as usize,
        "second paragraph".len() - 1,
        "middle end sits on its last cluster (Trailing)"
    );
    let r3 = sel.get_range_for_node(&node_id(P3)).expect("focus range");
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
        sel.get_range_for_node(&node_id(P1)).unwrap().start.cluster_id.start_byte_in_run,
        6
    );
    assert_eq!(
        sel.get_range_for_node(&node_id(P3)).unwrap().end.cluster_id.start_byte_in_run,
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
fn selection_spanning_delete_merges_into_one_replace_changeset() {
    let mut lw = layout_three_paragraphs();
    assert!(lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(P1),
        cursor(6), // after "first "
        node_id(P3),
        cursor(6), // after "third "
    ));
    let changeset_id = lw.delete_cross_block_selection();
    assert!(changeset_id.is_some(), "the delete records one changeset");

    // ONE atomic ReplaceChildren covering [P1 ..= P3]: Word semantics -
    // the merged paragraph keeps the FIRST block's element and holds
    // first-kept + last-kept text.
    let edit = lw
        .get_pending_document_edit()
        .expect("structural changeset pending");
    match &edit.operation {
        azul_layout::managers::changeset::DocumentOperation::ReplaceChildren(r) => {
            assert_eq!(r.parent.node.into_crate_internal(), Some(node_id(0)));
            assert_eq!(
                (r.start, r.end),
                (0, 3),
                "replaces the whole spanned block range"
            );
            let merged: String = r
                .content
                .children
                .as_ref()
                .iter()
                .filter_map(|c| match c.root.get_node_type() {
                    azul_core::dom::NodeType::Text(t) => Some(t.as_str().to_string()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                merged, "first paragraph",
                "merged text = 'first ' + 'paragraph' (kept head + kept tail)"
            );
            assert!(
                format!("{:?}", r.content.root.get_node_type()).contains("Div"),
                "the merged block keeps the FIRST paragraph's element"
            );
        }
        other => panic!("expected ReplaceChildren, got {other:?}"),
    }

    // The resume point lands the caret INSIDE the merged text at the join.
    let pos = format!("{:?}", edit.resume.position);
    assert!(
        pos.contains('6'),
        "caret resumes at the join byte (6 = len of 'first '): {pos}"
    );

    // Pre-apply: caret collapsed at the selection start, selection cleared.
    assert!(lw.text_edit_manager.get_cross_block_selection().is_none());
    let mc = lw.text_edit_manager.multi_cursor.as_ref().expect("caret");
    assert_eq!(mc.node_id.node.into_crate_internal(), Some(node_id(P1)));
}


#[test]
fn drag_across_blocks_extends_the_selection_and_back_collapses_it() {
    let mut lw = layout_three_paragraphs();

    // Mouse-down analog: caret in P1 (the drag reads its anchor from here).
    let key = 0;
    lw.text_edit_manager.multi_cursor =
        Some(azul_core::selection::MultiCursorState::new_with_cursor(
            cursor(6),
            azul_core::dom::DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    node_id(P1),
                )),
            },
            key,
        ));

    // Drag far below P1 (into P3's territory): the global hit-test resolves
    // the block under the pointer and the selection goes cross-block.
    let p3_rect = lw.get_node_layout_rect(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id(P3))),
    });
    let p3_rect = p3_rect.expect("P3 laid out");
    let inside_p3 = azul_core::geom::LogicalPosition::new(
        p3_rect.origin.x + 10.0,
        p3_rect.origin.y + p3_rect.size.height * 0.5,
    );
    let res = lw.process_mouse_drag_for_selection(
        azul_core::geom::LogicalPosition::zero(),
        inside_p3,
    );
    assert!(res.is_some(), "drag into another block must be handled");
    let cb = lw
        .text_edit_manager
        .get_cross_block_selection()
        .expect("cross-block selection active after dragging into P3");
    assert_eq!(cb.affected_nodes.len(), 3, "{:?}", cb.affected_nodes);
    assert!(cb.is_forward);

    // VISUAL: the regenerated display list carries SelectionRect items in
    // ALL THREE spanned IFCs (distinct vertical bands).
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result");
    let mut sel_ys: Vec<f32> = result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            azul_layout::solver3::display_list::DisplayListItem::SelectionRect {
                bounds,
                ..
            } => Some(bounds.origin().y),
            _ => None,
        })
        .collect();
    sel_ys.sort_by(f32::total_cmp);
    sel_ys.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    assert!(
        sel_ys.len() >= 3,
        "selection highlights must render in all three paragraphs, got bands at {sel_ys:?}"
    );

    // Drag back inside P1: collapses to a plain single-node range.
    let p1_rect = lw
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id(
                P1,
            ))),
        })
        .expect("P1 laid out");
    let inside_p1 = azul_core::geom::LogicalPosition::new(
        p1_rect.origin.x + 20.0,
        p1_rect.origin.y + p1_rect.size.height * 0.5,
    );
    let res = lw.process_mouse_drag_for_selection(
        azul_core::geom::LogicalPosition::zero(),
        inside_p1,
    );
    assert!(res.is_some());
    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "dragging back into the anchor block returns to a single-node range"
    );
}

#[test]
fn cross_block_copy_joins_paragraphs_and_paste_replaces_atomically() {
    let mut lw = layout_three_paragraphs();
    assert!(lw.set_cross_block_selection(
        DomId::ROOT_ID,
        node_id(P1),
        cursor(6), // after "first "
        node_id(P3),
        cursor(6), // after "third "
    ));

    // Ctrl+C: joined multi-paragraph text.
    let clip = lw
        .get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .expect("cross-block copy yields content");
    assert_eq!(
        clip.plain_text.as_str(),
        "paragraph\nsecond paragraph\nthird ",
        "anchor tail + full middle + focus head, newline-joined"
    );

    // Ctrl+V: one atomic replace with the pasted text at the join.
    let id = lw.replace_cross_block_selection("PASTED");
    assert!(id.is_some());
    let edit = lw.get_pending_document_edit().expect("changeset pending");
    match &edit.operation {
        azul_layout::managers::changeset::DocumentOperation::ReplaceChildren(r) => {
            let merged: String = r
                .content
                .children
                .as_ref()
                .iter()
                .filter_map(|c| match c.root.get_node_type() {
                    azul_core::dom::NodeType::Text(t) => Some(t.as_str().to_string()),
                    _ => None,
                })
                .collect();
            assert_eq!(merged, "first PASTEDparagraph");
        }
        other => panic!("expected ReplaceChildren, got {other:?}"),
    }
    // Caret resumes AFTER the pasted text: 6 + len("PASTED") = 12.
    let pos = format!("{:?}", edit.resume.position);
    assert!(pos.contains("12"), "caret after the insert: {pos}");
}
