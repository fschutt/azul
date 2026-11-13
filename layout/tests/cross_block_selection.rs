//! AZUL-STILL-TODO C9/C10: selection spanning multiple text blocks and the
//! selection-spanning delete.
//!
//! - `set_cross_block_selection` precomputes the per-IFC ranges (anchor node from its cursor to its
//!   end, the blocks between fully, focus node from its start to its cursor) and stores them
//!   render-ready; the display-list pass consumes them through `build_text_selections_map`.
//! - `delete_cross_block_selection` records ONE `ReplaceChildren` on the two ends' nearest common
//!   ancestor whose fragment holds the first block merged with what is left of the last (Word
//!   semantics); the caret collapses to the selection start.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
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

/// The text block of node `n` of the root DOM, through the resolver.
fn text_block(lw: &LayoutWindow, n: usize) -> TextBlock {
    lw.text_block_of(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id(n))),
    })
    .unwrap_or_else(|| panic!("node {n} is in a text block"))
}

/// The text runs directly inside `block` - its inline content, joined.
fn text_children(block: &Dom) -> String {
    block
        .children
        .as_ref()
        .iter()
        .filter_map(|c| match c.root.get_node_type() {
            azul_core::dom::NodeType::Text(t) => Some(t.as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn layout_three_paragraphs() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "first paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "second paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "third paragraph",
                )),
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
/// The same three paragraphs, but NESTED: two in one wrapper div and the
/// third in another. Nothing about a document selection should care.
///
/// body(0) > div.box(1) > [p(2)>text(3), p(4)>text(5)], div.box(6) > p(7)>text(8)
fn layout_paragraphs_in_two_boxes() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .box { display: block; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let para = |txt: &str| {
        Dom::create_div()
            .with_ids_and_classes(class("p"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(txt))
    };
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("box"))
                .with_child(para("first paragraph"))
                .with_child(para("second paragraph")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("box"))
                .with_child(para("third paragraph")),
        );
    let (css, warnings) = azul_css::parser2::new_from_str(CSS);
    assert!(warnings.is_empty(), "css warnings: {warnings:?}");
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

const P1: usize = 1;
const P2: usize = 3;
const P3: usize = 5;

#[test]
fn cross_block_selection_builds_ranges_for_every_spanned_block() {
    let mut lw = layout_three_paragraphs();
    let ok = lw.set_cross_block_selection(
        text_block(&lw, P1),
        cursor(6), // after "first "
        text_block(&lw, P3),
        cursor(5), // before " paragraph" in "third paragraph"
    );
    assert!(ok, "sibling blocks must accept a cross-block selection");

    let map = lw.text_edit_manager.build_text_selections_map();
    let sel = map
        .get(&DomId::ROOT_ID)
        .expect("selection for the root DOM");
    assert!(sel.is_forward);
    assert_eq!(
        sel.affected_blocks.len(),
        3,
        "anchor + middle + focus: {:?}",
        sel.affected_blocks
    );
    // One range per spanned block: a cross-block selection never multi-selects
    // inside a block (that is the Ctrl+D session's job).
    for (block, ranges) in &sel.affected_blocks {
        assert_eq!(
            ranges.len(),
            1,
            "block {block:?} contributes exactly one range"
        );
    }
    let r1 = sel
        .get_range_for_block(&text_block(&lw, P1))
        .expect("anchor range");
    assert_eq!(r1.start.cluster_id.start_byte_in_run, 6);
    // End = Trailing on the LAST cluster ("after the final grapheme"), so
    // the byte names the final cluster's START, not the string length.
    assert_eq!(
        r1.end.cluster_id.start_byte_in_run as usize,
        "first paragraph".len() - 1,
        "anchor end sits on the last cluster (Trailing)"
    );
    let r2 = sel
        .get_range_for_block(&text_block(&lw, P2))
        .expect("middle range");
    assert_eq!(r2.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(
        r2.end.cluster_id.start_byte_in_run as usize,
        "second paragraph".len() - 1,
        "middle end sits on its last cluster (Trailing)"
    );
    let r3 = sel
        .get_range_for_block(&text_block(&lw, P3))
        .expect("focus range");
    assert_eq!(r3.start.cluster_id.start_byte_in_run, 0);
    assert_eq!(r3.end.cluster_id.start_byte_in_run, 5);
}

#[test]
fn backward_cross_block_selection_normalizes_to_document_order() {
    let mut lw = layout_three_paragraphs();
    let ok = lw.set_cross_block_selection(
        text_block(&lw, P3),
        cursor(5),
        text_block(&lw, P1),
        cursor(6),
    );
    assert!(ok);
    let map = lw.text_edit_manager.build_text_selections_map();
    let sel = map.get(&DomId::ROOT_ID).unwrap();
    assert!(!sel.is_forward, "anchor after focus = backward selection");
    assert_eq!(sel.affected_blocks.len(), 3);
    // Ranges are stored in DOCUMENT order regardless of drag direction.
    assert_eq!(
        sel.get_range_for_block(&text_block(&lw, P1))
            .unwrap()
            .start
            .cluster_id
            .start_byte_in_run,
        6
    );
    assert_eq!(
        sel.get_range_for_block(&text_block(&lw, P3))
            .unwrap()
            .end
            .cluster_id
            .start_byte_in_run,
        5
    );
}

/// A text leaf is not a block of its own: it names its paragraph's block, so
/// a "selection" from P1's text to P1 is a selection inside ONE block - the
/// session's job, not a document selection.
#[test]
fn a_selection_inside_one_block_is_not_a_document_selection() {
    let mut lw = layout_three_paragraphs();
    // text node 2 is a CHILD of P1: the same block.
    assert_eq!(text_block(&lw, 2), text_block(&lw, P1));
    let ok = lw.set_cross_block_selection(
        text_block(&lw, 2),
        cursor(0),
        text_block(&lw, P1),
        cursor(1),
    );
    assert!(!ok, "one block is not a document selection");
    assert!(lw.text_edit_manager.get_cross_block_selection().is_none());
}

#[test]
fn selection_spanning_delete_merges_into_one_replace_changeset() {
    let mut lw = layout_three_paragraphs();
    assert!(lw.set_cross_block_selection(
        text_block(&lw, P1),
        cursor(6), // after "first "
        text_block(&lw, P3),
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
            // The payload is a FRAGMENT: `document_edit::apply_replace` and the
            // overlay preview both insert its CHILDREN and ignore its root. So
            // the merged paragraph is its one child - not the root, which an
            // app applying the edit drops, leaving a bare text run where the
            // paragraph was.
            let blocks = r.content.children.as_ref();
            assert_eq!(blocks.len(), 1, "ONE merged paragraph replaces the three");
            assert_eq!(
                text_children(&blocks[0]),
                "first paragraph",
                "merged text = 'first ' + 'paragraph' (kept head + kept tail)"
            );
            assert!(
                format!("{:?}", blocks[0].root.get_node_type()).contains("Div"),
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
    assert_eq!(mc.block, text_block(&lw, P1));
}

#[test]
fn drag_across_blocks_extends_the_selection_and_back_collapses_it() {
    let mut lw = layout_three_paragraphs();

    // Mouse-down analog: caret in P1 (the drag reads its anchor from here).
    let key = 0;
    let p1 = text_block(&lw, P1);
    lw.text_edit_manager.multi_cursor = Some(
        azul_core::selection::MultiCursorState::new_with_cursor(cursor(6), p1, key),
    );

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
    let res =
        lw.process_mouse_drag_for_selection(azul_core::geom::LogicalPosition::zero(), inside_p3);
    assert!(res.is_some(), "drag into another block must be handled");
    let cb = lw
        .text_edit_manager
        .get_cross_block_selection()
        .expect("cross-block selection active after dragging into P3");
    assert_eq!(cb.affected_blocks.len(), 3, "{:?}", cb.affected_blocks);
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
                bounds, ..
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
    let res =
        lw.process_mouse_drag_for_selection(azul_core::geom::LogicalPosition::zero(), inside_p1);
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
        text_block(&lw, P1),
        cursor(6), // after "first "
        text_block(&lw, P3),
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
            // A fragment: its one child is the merged paragraph.
            let blocks = r.content.children.as_ref();
            assert_eq!(blocks.len(), 1, "ONE merged paragraph replaces the three");
            assert_eq!(text_children(&blocks[0]), "first PASTEDparagraph");
        }
        other => panic!("expected ReplaceChildren, got {other:?}"),
    }
    // Caret resumes AFTER the pasted text: 6 + len("PASTED") = 12.
    let pos = format!("{:?}", edit.resume.position);
    assert!(pos.contains("12"), "caret after the insert: {pos}");
}

/// A DOCUMENT selection spans whatever text blocks lie between its ends, in
/// document order - it is not a sibling walk. Dragging from a paragraph in
/// one container into a paragraph in another (every real document: a heading
/// in a wrapper, a list, a card) used to run off the sibling chain and be
/// rejected, and the drag collapsed back to the anchor paragraph.
#[test]
fn a_selection_spans_text_blocks_in_other_containers() {
    let mut lw = layout_paragraphs_in_two_boxes();
    const FIRST_P: usize = 2;
    const THIRD_P: usize = 7;
    let ok = lw.set_cross_block_selection(
        text_block(&lw, FIRST_P),
        cursor(6),
        text_block(&lw, THIRD_P),
        cursor(5),
    );
    assert!(ok, "a selection across containers must be accepted");
    let sel = lw
        .text_edit_manager
        .get_cross_block_selection()
        .expect("the selection is stored");
    assert_eq!(
        sel.affected_blocks.len(),
        3,
        "both ends and the paragraph between them: {:?}",
        sel.affected_blocks.keys().collect::<Vec<_>>()
    );
}

/// Reversed drag (bottom-up) selects the same blocks.
#[test]
fn a_selection_across_containers_works_in_both_directions() {
    let mut lw = layout_paragraphs_in_two_boxes();
    assert!(lw.set_cross_block_selection(
        text_block(&lw, 7),
        cursor(5),
        text_block(&lw, 2),
        cursor(6),
    ));
    assert_eq!(
        lw.text_edit_manager
            .get_cross_block_selection()
            .expect("stored")
            .affected_blocks
            .len(),
        3
    );
}

/// Backspace over a DOCUMENT selection deletes every block it spans, not just
/// the anchor's. The cross-block selection lives beside the primary cursor,
/// so the single-node delete path trimmed one paragraph and left the rest.
#[test]
fn deleting_a_document_selection_trims_every_block_it_spans() {
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};

    let mut lw = layout_three_paragraphs();
    assert!(lw.set_cross_block_selection(
        text_block(&lw, P1),
        cursor(6), // after "first "
        text_block(&lw, P3),
        cursor(6), // after "third "
    ));
    let host = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id(P1))),
    };
    let affected = lw
        .delete_selection(host, false)
        .expect("the delete reports the blocks it touched");
    assert!(
        affected.len() >= 3,
        "every spanned block is reported: {affected:?}"
    );
    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "the selection is consumed by the delete"
    );
}

/// Deleting a DOCUMENT selection whose two ends sit in DIFFERENT containers.
///
/// `replace_cross_block_selection` still carried the sibling rule that
/// `set_cross_block_selection` dropped: it took the selection off the
/// manager, found that the two ends' parents differ and returned `None`. So
/// Backspace, Cut and Paste over such a selection did nothing at all - and
/// the selection they were aimed at was gone too.
///
/// The edit is ONE `ReplaceChildren` on the ends' nearest common ancestor,
/// over its children that hold them: the first keeps what comes before the
/// selection plus the merged paragraph, the second keeps what comes after it
/// and goes entirely when the selection emptied it.
#[test]
fn deleting_a_selection_across_containers_joins_its_ends() {
    // body(0) > div.box(1) > [p(2)>text(3), p(4)>text(5)], div.box(6) > p(7)>text(8)
    const FIRST_P: usize = 2;
    const THIRD_P: usize = 7;
    let mut lw = layout_paragraphs_in_two_boxes();
    assert!(lw.set_cross_block_selection(
        text_block(&lw, FIRST_P),
        cursor(6), // after "first "
        text_block(&lw, THIRD_P),
        cursor(6), // after "third "
    ));

    let id = lw.delete_cross_block_selection();
    assert!(
        id.is_some(),
        "a selection across two containers deletes like any other"
    );

    let edit = lw
        .get_pending_document_edit()
        .expect("structural changeset pending");
    match &edit.operation {
        azul_layout::managers::changeset::DocumentOperation::ReplaceChildren(r) => {
            assert_eq!(
                r.parent.node.into_crate_internal(),
                Some(node_id(0)),
                "the ends' nearest common ancestor is the body"
            );
            assert_eq!((r.start, r.end), (0, 2), "both containers are replaced");
            let boxes = r.content.children.as_ref();
            assert_eq!(
                boxes.len(),
                1,
                "the second container held nothing but the selection's end, so it goes"
            );
            let paras = boxes[0].children.as_ref();
            assert_eq!(
                paras.len(),
                1,
                "the first container keeps the merged paragraph and loses the one the selection \
                 covered"
            );
            assert_eq!(
                text_children(&paras[0]),
                "first paragraph",
                "'first ' + 'paragraph' (kept head + kept tail)"
            );
        }
        other => panic!("expected ReplaceChildren, got {other:?}"),
    }
    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "the selection is consumed by the delete"
    );
    let pos = format!("{:?}", edit.resume.position);
    assert!(pos.contains('6'), "caret resumes at the join byte: {pos}");
}

/// A BLOCK THAT HOLDS MORE THAN PLAIN TEXT STILL COPIES.
///
/// A `TextCursor`'s `source_run` indexes the inline content `solver3::fc`
/// built for the IFC, where a `<br>` is a `LineBreak` item of its own
/// (`layout/src/solver3/fc.rs`, the `NodeType::Br` arm). The clipboard
/// extraction indexes a DIFFERENT vector — the DOM-child recursion of
/// `get_text_before_textinput`, which drops `<br>` (and `::marker`, and
/// replaced content) entirely. With a leading `<br>` the block's text is
/// run 1 to the selection and item 0 to the copy, so the copy's
/// `if i < sr { continue }` walks past the only run it has and the whole
/// block is dropped from the clipboard.
///
/// RED today: the copy yields `"\ngamma"` — the anchor block contributes
/// nothing and only the joiner and the second block survive.
#[test]
fn a_document_selection_copies_a_block_whose_text_is_not_its_first_run() {
    use azul_core::geom::LogicalPosition;

    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    // body(0) > div.p(1) [display: list-item] > text(2), div.p(3) > text(4)
    //
    // MEASURED on this tree: a `display: list-item` block's clusters carry
    // `source_run` 0 AND 1 - the `::marker` fc emits takes run 0, so the
    // text is run 1 - while the DOM-child walk the clipboard indexes with
    // that number sees the text at 0. A leading `<br>` does NOT diverge
    // here, which is why the first version of this fixture could not fail.
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_css("display: list-item;")
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "alpha",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "gamma",
                )),
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

    // Mint both ends the way a real drag does — through the hit test, so the
    // cursors carry the LAYOUT's run numbering and not a hand-written 0.
    let a_rect = lw
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id(1))),
        })
        .expect("the first block is laid out");
    let b_rect = lw
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id(3))),
        })
        .expect("the second block is laid out");

    let (a_node, a_cursor) = lw
        .hittest_text_position_global(
            DomId::ROOT_ID,
            LogicalPosition::new(a_rect.origin.x + 1.0, a_rect.origin.y + a_rect.size.height * 0.5),
        )
        .expect("the pointer resolves a cursor in the first block");
    let (b_node, b_cursor) = lw
        .hittest_text_position_global(
            DomId::ROOT_ID,
            LogicalPosition::new(
                b_rect.origin.x + b_rect.size.width - 1.0,
                b_rect.origin.y + b_rect.size.height * 0.5,
            ),
        )
        .expect("the pointer resolves a cursor in the second block");

    assert_eq!(
        a_node,
        text_block(&lw, 1),
        "the first end is the first block"
    );
    assert_eq!(
        b_node,
        text_block(&lw, 3),
        "the second end is the second block"
    );
    // The premise this test stands on: the `<br>` occupies run 0, so the
    // block's text is run 1 — a number the DOM-derived content has no item for.
    assert_eq!(
        a_cursor.cluster_id.source_run, 1,
        "the <br> is run 0 of the layout's inline content, so 'alpha' is run 1"
    );

    assert!(lw.set_cross_block_selection(a_node, a_cursor, b_node, b_cursor));

    let clip = lw
        .get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .expect("a document selection puts something on the clipboard");
    assert_eq!(
        clip.plain_text.as_str(),
        "alpha\ngamma",
        "both blocks' text comes out, newline-joined, in document order"
    );
}
