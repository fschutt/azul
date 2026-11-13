//! A DOCUMENT selection (one that spans text blocks) ends the moment the user
//! does anything that collapses or replaces the selection.
//!
//! It is stored beside the editing session, not in it:
//! `TextEditManager::cross_block` next to `TextEditManager::multi_cursor`.
//! Paint, copy and delete all prefer it while it exists. So every path that
//! gives the session a new caret - a click, a Tab into another field, an arrow
//! key - or edits through it - typing - has to end it too. When one did not, the
//! selection stayed painted AND stayed the thing Backspace deleted: drag from
//! the first paragraph into the third, click inside the second without moving,
//! press Backspace, and all three paragraphs were gone.
//!
//! `delete_selection` must also only ever delete a selection that belongs to
//! the host whose key was pressed.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .host { display: block; }
    .p { display: block; }
"#;

/// `body(0) > div.host[contenteditable](1) > [p(2)>text(3), p(4)>text(5), p(6)>text(7)]`
/// and, for the two-field tests, a second editable after it:
/// `div.host[contenteditable](8) > p(9) > text(10)`.
const HOST: usize = 1;
const P1: usize = 2;
const P2: usize = 4;
const P3: usize = 6;
const OTHER_HOST: usize = 8;
const OTHER_P: usize = 9;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(text: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            text,
        ))
}

fn editor() -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("host"))
        .with_contenteditable(true)
        .with_child(para("first paragraph"))
        .with_child(para("second paragraph"))
        .with_child(para("third paragraph"))
}

fn layout(mut dom: Dom) -> LayoutWindow {
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
    lw
}

fn one_editor() -> LayoutWindow {
    layout(Dom::create_body().with_child(editor()))
}

fn two_editors() -> LayoutWindow {
    layout(
        Dom::create_body().with_child(editor()).with_child(
            Dom::create_div()
                .with_ids_and_classes(class("host"))
                .with_contenteditable(true)
                .with_child(para("other field")),
        ),
    )
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn rect_of(lw: &LayoutWindow, n: usize) -> LogicalRect {
    lw.get_node_layout_rect(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is laid out"))
}

fn text_of(lw: &LayoutWindow, n: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(n));
    lw.extract_text_from_inline_content(&content)
}

/// What a drag from "first |paragraph" to "third |paragraph" leaves behind:
/// the session's caret at the anchor, the document selection beside it, the
/// focus on the editing host.
fn drag_selected_p1_to_p3(lw: &mut LayoutWindow) {
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw.start_editing_at(cursor(6), DomId::ROOT_ID, NodeId::new(P1), 0);
    let (p1, p3) = (block_of(lw, P1), block_of(lw, P3));
    assert!(
        lw.set_cross_block_selection(p1, cursor(6), p3, cursor(6)),
        "premise: the drag's selection is accepted"
    );
}

/// The text block of node `n`, through the resolver.
fn block_of(lw: &LayoutWindow, n: usize) -> azul_core::selection::TextBlock {
    lw.text_block_of(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is in a text block"))
}

fn assert_editor_untouched(lw: &LayoutWindow) {
    assert!(
        lw.get_pending_document_edit().is_none(),
        "no structural edit may be recorded: {:?}",
        lw.get_pending_document_edit().map(|e| &e.operation)
    );
    assert_eq!(text_of(lw, P1), "first paragraph");
    assert_eq!(text_of(lw, P3), "third paragraph");
}

#[test]
fn a_click_inside_the_selection_collapses_it_to_the_clicked_caret() {
    let mut lw = one_editor();
    drag_selected_p1_to_p3(&mut lw);

    // Click inside the middle paragraph without moving the mouse.
    let p2 = rect_of(&lw, P2);
    let inside_p2 = LogicalPosition::new(p2.origin.x + 30.0, p2.origin.y + p2.size.height * 0.5);
    lw.process_mouse_click_for_selection(inside_p2, 0)
        .expect("the click lands in the second paragraph");
    assert_eq!(
        lw.text_edit_manager.get_editing_node_id(),
        Some(NodeId::new(P2)),
        "premise: the click put the caret in the second paragraph"
    );

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "a click collapses the selection: the drag's selection must be gone, got {:?}",
        lw.text_edit_manager
            .get_cross_block_selection()
            .map(|s| s.affected_blocks.keys().collect::<Vec<_>>())
    );

    // Backspace now deletes ONE character in the second paragraph.
    lw.delete_selection(dnid(HOST), false)
        .expect("backspace after a mid-paragraph click deletes a character");
    assert_editor_untouched(&lw);
    assert_eq!(
        text_of(&lw, P2).len(),
        "second paragraph".len() - 1,
        "exactly one character left the clicked paragraph"
    );
}

#[test]
fn an_arrow_key_collapses_the_selection_to_its_edge() {
    let mut lw = one_editor();
    drag_selected_p1_to_p3(&mut lw);

    assert!(lw.apply_selection_op(
        dnid(HOST),
        &SelectionOp::new(
            SelectionDirection::Backward,
            SelectionStep::Character,
            SelectionMode::Move,
        ),
    ));

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "Left collapses the selection, it must not stay painted"
    );
    // Left with a selection lands on its START and does not move past it.
    assert_eq!(
        lw.text_edit_manager.get_editing_node_id(),
        Some(NodeId::new(P1))
    );
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor(),
        Some(cursor(6)),
        "the caret sits at the selection's start"
    );

    // ...and Backspace from there is a one-character edit.
    lw.delete_selection(dnid(HOST), false)
        .expect("backspace after the collapse deletes a character");
    assert!(lw.get_pending_document_edit().is_none());
    assert_eq!(text_of(&lw, P1), "firstparagraph");
    assert_eq!(text_of(&lw, P3), "third paragraph");
}

#[test]
fn typing_replaces_the_selection() {
    let mut lw = one_editor();
    drag_selected_p1_to_p3(&mut lw);

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "typed text replaces the selection; the selection must not survive it"
    );
    let edit = lw
        .get_pending_document_edit()
        .expect("typing over a document selection replaces it, like paste does");
    let merged = match &edit.operation {
        azul_layout::managers::changeset::DocumentOperation::ReplaceChildren(r) => {
            all_text(&r.content)
        }
        other => panic!("expected ReplaceChildren, got {other:?}"),
    };
    assert_eq!(merged, "first xparagraph", "kept head + typed text + kept tail");
    assert_eq!(
        text_of(&lw, P1),
        "first paragraph",
        "the character did not ALSO land at the old caret"
    );
}

/// Every text node's text in `dom`, depth-first.
fn all_text(dom: &Dom) -> String {
    let mut out = String::new();
    if let azul_core::dom::NodeType::Text(t) = dom.root.get_node_type() {
        out.push_str(t.as_str());
    }
    for child in dom.children.as_ref() {
        out.push_str(&all_text(child));
    }
    out
}

#[test]
fn focus_moving_into_another_field_ends_the_selection() {
    let mut lw = two_editors();
    drag_selected_p1_to_p3(&mut lw);

    // Tab into the second field: the focus path seeds its caret there.
    let window_state = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(OTHER_HOST)), &window_state);
    lw.finalize_pending_focus_changes();
    lw.focus_manager.set_focused_node(Some(dnid(OTHER_HOST)));
    assert!(
        lw.text_edit_manager
            .get_editing_node_id()
            .is_some_and(|n| n.index() >= OTHER_HOST),
        "premise: the caret moved into the second field, got {:?}",
        lw.text_edit_manager.get_editing_node_id()
    );

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "the first field's selection ends when the caret leaves for another field"
    );

    // Backspace in the second field edits the second field.
    lw.delete_selection(dnid(OTHER_HOST), false)
        .expect("backspace at the end of 'other field' deletes the 'd'");
    assert_editor_untouched(&lw);
    assert_eq!(text_of(&lw, OTHER_P), "other fiel");
}

#[test]
fn delete_only_deletes_a_selection_inside_its_own_host() {
    let mut lw = two_editors();
    // The caret is in the second field...
    lw.focus_manager.set_focused_node(Some(dnid(OTHER_HOST)));
    lw.start_editing_at(cursor(3), DomId::ROOT_ID, NodeId::new(OTHER_P), 0);
    // ...and a document selection exists in the first one.
    let (p1, p3) = (block_of(&lw, P1), block_of(&lw, P3));
    assert!(lw.set_cross_block_selection(p1, cursor(6), p3, cursor(6)));

    // Backspace pressed in the SECOND field.
    lw.delete_selection(dnid(OTHER_HOST), false)
        .expect("backspace before 'e' in 'other field' deletes the 'h'");
    assert_editor_untouched(&lw);
    assert_eq!(text_of(&lw, OTHER_P), "oter field");
}
