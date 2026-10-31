//! Shift+arrow moves the focus end of a selection that spans paragraphs, and
//! at the edge of a paragraph it carries the selection into the next one.
//!
//! A document selection (`cross_block`) is made by a drag or by Ctrl+A; the
//! keyboard's Extend step only ever moved the editing session's own caret,
//! inside its one block. With a document selection standing, Shift+Right
//! changed that invisible caret and nothing on screen; with none, Shift+Right
//! at the end of a paragraph did nothing at all - the keyboard could neither
//! grow a document selection nor start one.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, Selection, TextBlock, TextCursor},
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

/// `body(0) > div.host[contenteditable](1) > [div.p(2) > "first"(3),
/// div.p(4) > "second"(5), div.p(6) > "third"(7)]`
const HOST: usize = 1;
const P1: usize = 2;
const P2: usize = 4;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(s))
}

fn three_paragraphs() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(para("first"))
            .with_child(para("second"))
            .with_child(para("third")),
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

fn block_of(lw: &LayoutWindow, n: usize) -> TextBlock {
    lw.text_block_of(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is in a text block"))
}

fn at(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// A caret in `n`'s paragraph, as a click puts one there.
fn caret_in(lw: &mut LayoutWindow, n: usize, cursor: TextCursor) {
    assert!(
        lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(n), 0),
        "premise: a session opens in node {n}'s paragraph"
    );
}

/// A selection from "fi|rst" to `focus` in `focus_node`'s paragraph, the
/// session in the anchor's - the state a drag leaves.
fn document_selection(lw: &mut LayoutWindow, focus_node: usize, focus: TextCursor) {
    let anchor = at(2, CursorAffinity::Leading);
    caret_in(lw, P1, anchor);
    let (first, other) = (block_of(lw, P1), block_of(lw, focus_node));
    assert!(
        lw.set_cross_block_selection(first, anchor, other, focus),
        "premise: the selection spans the two paragraphs"
    );
}

fn shift(lw: &mut LayoutWindow, direction: SelectionDirection, step: SelectionStep) {
    let _ = lw.apply_selection_op(
        dnid(HOST),
        &SelectionOp::new(direction, step, SelectionMode::Extend),
    );
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

#[test]
fn shift_right_moves_the_end_of_a_document_selection() {
    let mut lw = three_paragraphs();
    document_selection(&mut lw, P2, at(3, CursorAffinity::Leading));
    assert_eq!(copied(&lw).as_deref(), Some("rst\nsec"), "premise");

    shift(&mut lw, SelectionDirection::Forward, SelectionStep::Character);

    assert_eq!(copied(&lw).as_deref(), Some("rst\nseco"));
}

#[test]
fn shift_left_back_into_the_anchor_paragraph_leaves_a_range_there() {
    let mut lw = three_paragraphs();
    document_selection(&mut lw, P2, at(0, CursorAffinity::Leading));

    shift(&mut lw, SelectionDirection::Backward, SelectionStep::Character);

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "the focus is back in the anchor's paragraph: one block, one range"
    );
    assert_eq!(copied(&lw).as_deref(), Some("rst"));
    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        Some(block_of(&lw, P1))
    );
}

#[test]
fn shift_right_at_the_end_of_a_paragraph_carries_the_selection_into_the_next() {
    let mut lw = three_paragraphs();
    caret_in(&mut lw, P1, at(4, CursorAffinity::Trailing));

    shift(&mut lw, SelectionDirection::Forward, SelectionStep::Character);

    let focus_block = lw
        .text_edit_manager
        .get_cross_block_selection()
        .map(|cb| cb.focus.block);
    assert_eq!(
        focus_block,
        Some(block_of(&lw, P2)),
        "past the end of \"first\" the selection runs into \"second\""
    );

    shift(&mut lw, SelectionDirection::Forward, SelectionStep::Character);

    assert_eq!(
        copied(&lw).map(|c| c.trim_start().to_string()).as_deref(),
        Some("s"),
        "the next step selects the first character of \"second\""
    );
}

#[test]
fn ctrl_shift_end_extends_a_document_selection_to_the_last_paragraph() {
    let mut lw = three_paragraphs();
    document_selection(&mut lw, P2, at(3, CursorAffinity::Leading));

    shift(&mut lw, SelectionDirection::Forward, SelectionStep::Document);

    assert_eq!(copied(&lw).as_deref(), Some("rst\nsecond\nthird"));
}

/// Guard (green before and after): inside one paragraph Shift+Right still
/// grows the session's own range, and no document selection appears.
#[test]
fn shift_right_inside_a_paragraph_grows_its_own_range() {
    let mut lw = three_paragraphs();
    caret_in(&mut lw, P1, at(2, CursorAffinity::Leading));

    shift(&mut lw, SelectionDirection::Forward, SelectionStep::Character);

    assert!(lw.text_edit_manager.get_cross_block_selection().is_none());
    let primary = lw
        .text_edit_manager
        .multi_cursor
        .as_ref()
        .and_then(|mc| mc.get_primary().map(|p| p.selection));
    assert!(
        matches!(primary, Some(Selection::Range(_))),
        "the caret became a range: {primary:?}"
    );
    assert_eq!(copied(&lw).as_deref(), Some("r"));
}
