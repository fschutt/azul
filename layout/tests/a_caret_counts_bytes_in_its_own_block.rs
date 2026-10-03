//! A caret is read and written in the numbering it was made in: the runs of
//! its own text block, behind whatever the layout generated in front of them.
//!
//! A list item's inline layout starts with its `::marker`, so every caret the
//! layout mints in the item's text says run 1. The edit model -
//! `get_text_before_textinput`, the DOM's text through the content overlay -
//! holds the text alone, where the same text is run 0. Every reader that
//! walked the edit model with a layout caret was one run off:
//!
//! - the app's `DocumentPosition` (`document_caret`) clamped to the end;
//! - the IME's caret offset (`focused_caret_byte_offset`) found no run at all,
//!   so its document spliced a composition at the end and Android's
//!   surrounding text put the caret past the text;
//! - the IME's `setSelectedTextRange:` resolved its bytes against the shaped
//!   clusters, the marker's among them;
//! - Enter split the item at the end of its text, wherever the caret was;
//! - the preedit was spliced into no run, and elsewhere a byte short of a
//!   `Trailing` caret;
//! - Ctrl+D found no word to search for.
//!
//! A second seat's keys had the same defect one level up: its typing was keyed
//! to the PRIMARY's block and its Backspace to the focused host, whose
//! flattened text numbers the paragraphs' runs one after the other - a seat
//! caret in the second paragraph edited the first.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{
        DefaultAction, SelectionDirection, SelectionMode, SelectionOp, SelectionStep,
    },
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::changeset::{DocumentOperation, NodePosition},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .host { display: block; }
    .li { display: list-item; }
    .p { display: block; }
"#;

/// `body(0) > div.host[contenteditable](1) > div.li(2) > text(3)`
const HOST: usize = 1;
const ITEM: usize = 2;

/// `body(0) > div.host[contenteditable](1) > [div.p(2) > "one"(3),
/// div.p(4) > "two"(5)]`
const FIRST: usize = 2;
const SECOND: usize = 4;
const SEAT: u64 = 7;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
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

fn list_item(s: &str) -> LayoutWindow {
    layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("host"))
                .with_contenteditable(true)
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("li"))
                        .with_child(text(s)),
                ),
        ),
    )
}

fn two_paragraphs() -> LayoutWindow {
    layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("host"))
                .with_contenteditable(true)
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("p"))
                        .with_child(text("one")),
                )
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("p"))
                        .with_child(text("two")),
                ),
        ),
    )
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

fn text_of(lw: &LayoutWindow, n: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(n));
    lw.extract_text_from_inline_content(&content)
}

fn caret(run: u32, byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: run,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// A click in the list item opens the session there, as a user's does; then
/// the caret is put at byte `byte` of the item's text - run 1, the run every
/// caret the layout mints in it names.
fn caret_in_the_item(lw: &mut LayoutWindow, byte: u32) {
    let item = lw
        .get_node_layout_rect(dnid(ITEM))
        .expect("the list item is laid out");
    let point = LogicalPosition::new(
        item.origin.x + 200.0,
        item.origin.y + item.size.height * 0.5,
    );
    lw.process_mouse_click_for_selection(point, 0)
        .expect("the click lands in the list item");
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    let clicked = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("premise: the click placed a caret");
    assert_eq!(
        clicked.cluster_id.source_run, 1,
        "premise: the marker is run 0 of the item's layout, so its text is run 1"
    );
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("the click opened a session")
        .set_single_cursor(caret(1, byte, CursorAffinity::Leading));
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

// ---------------------------------------------------------------------------
// The app's and the IME's byte offsets
// ---------------------------------------------------------------------------

#[test]
fn the_app_reads_a_list_item_caret_at_its_byte_in_the_text() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);

    let position = lw.document_caret().expect("a session is open");
    assert_eq!(position.node, dnid(ITEM));
    assert_eq!(position.text_byte, 2, "\"al|pha\" is byte 2 of \"alpha\"");
}

#[test]
fn the_ime_reads_a_list_item_caret_at_its_byte_in_the_text() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);

    assert_eq!(lw.focused_caret_byte_offset(), Some(2));
    assert_eq!(lw.focused_selection_byte_range(), Some((2, 2)));
}

#[test]
fn an_ime_selection_in_a_list_item_selects_the_bytes_it_names() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 0);

    assert!(lw.set_focused_selection_from_byte_range(1, 3));

    assert_eq!(lw.focused_selection_byte_range(), Some((1, 3)));
    assert_eq!(copied(&lw).as_deref(), Some("lp"));
}

#[test]
fn the_ime_document_splices_a_composition_at_a_list_item_caret() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);
    lw.text_edit_manager.preedit_text = Some("Z".to_string());

    assert_eq!(
        lw.ime_document(),
        ("alZpha".to_string(), Some((2, 3))),
        "the composition goes in at the caret, not at the end"
    );
}

/// `firstRectForCharacterRange:` asks where a byte of the IME's document is
/// on screen. The caret's own byte must come back as the caret's own rect -
/// not the marker's: resolving the byte against the shaped clusters counted
/// the `::marker`'s bytes into the text, while the document the IME holds
/// (`ime_document`) starts at the item's first letter.
#[test]
fn the_ime_finds_a_list_item_byte_where_its_caret_stands() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);

    let caret = lw
        .get_focused_cursor_rect_viewport()
        .expect("the caret has a rect");
    let asked = lw
        .focused_rect_for_byte_offset(2)
        .expect("byte 2 has a rect");
    assert!(
        (asked.origin.x - caret.origin.x).abs() < 0.5
            && (asked.origin.y - caret.origin.y).abs() < 0.5,
        "the IME's rect for \"al|pha\" is at {:?}, the caret at {:?}",
        asked.origin,
        caret.origin
    );
}

#[test]
fn android_reads_a_list_item_caret_at_its_byte_in_the_text() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);

    assert_eq!(lw.ime_surrounding_text(), Some(("alpha".to_string(), 2)));
}

// ---------------------------------------------------------------------------
// Edits at a list item's caret
// ---------------------------------------------------------------------------

#[test]
fn enter_in_a_list_item_splits_it_at_the_caret() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);

    lw.record_structural_default_action(&DefaultAction::SplitBlockAtCursor {
        target: dnid(HOST),
    })
    .expect("a split is recorded");

    let pending = lw
        .pending_document_edit
        .as_ref()
        .expect("the recorded edit awaits the app");
    match &pending.operation {
        DocumentOperation::SplitNode(split) => {
            assert_eq!(split.node, dnid(ITEM), "the item is what splits");
            assert_eq!(
                split.at,
                NodePosition::in_text_child(0, 2),
                "\"al|pha\": the split is at byte 2 of the text, not at its end"
            );
        }
        other => panic!("expected a split, got {other:?}"),
    }
}

#[test]
fn a_composition_in_a_list_item_is_shaped_at_the_caret() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 2);
    lw.text_edit_manager.preedit_text = Some("Z".to_string());

    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(ITEM));

    let shaped = lw.spliced_text_with_preedits(DomId::ROOT_ID, NodeId::new(ITEM));
    assert_eq!(lw.extract_text_from_inline_content(&shaped), "alZpha");
}

#[test]
fn a_composition_after_a_trailing_caret_is_shaped_after_its_character() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 0);
    // After the last 'a': the layout's own end-of-text caret.
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("a session is open")
        .set_single_cursor(caret(1, 4, CursorAffinity::Trailing));
    lw.text_edit_manager.preedit_text = Some("Z".to_string());

    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(ITEM));

    let shaped = lw.spliced_text_with_preedits(DomId::ROOT_ID, NodeId::new(ITEM));
    assert_eq!(
        lw.extract_text_from_inline_content(&shaped),
        "alphaZ",
        "Trailing on the last 'a' is after it"
    );
}

#[test]
fn ctrl_d_in_a_list_item_selects_the_next_occurrence() {
    let mut lw = list_item("foo bar foo");
    caret_in_the_item(&mut lw, 1);

    assert!(
        lw.select_next_occurrence(),
        "the word at the caret has another occurrence"
    );
    assert_eq!(
        lw.text_edit_manager
            .multi_cursor
            .as_ref()
            .map(|mc| mc.selections.len()),
        Some(2),
        "the word and its next occurrence"
    );
}

#[test]
fn a_line_per_caret_pastes_into_a_list_items_text() {
    let mut lw = list_item("alpha");
    caret_in_the_item(&mut lw, 1);
    let _ = lw
        .text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("a session is open")
        .add_cursor(caret(1, 3, CursorAffinity::Leading));

    assert!(lw.paste_one_line_per_caret("X\nY"));

    assert_eq!(text_of(&lw, ITEM), "aXlpYha");
}

// ---------------------------------------------------------------------------
// A second seat's keys, in a host with paragraphs
// ---------------------------------------------------------------------------

#[test]
fn a_seats_keystroke_lands_in_the_paragraph_its_caret_is_in() {
    let mut lw = two_paragraphs();
    let second = block_of(&lw, SECOND);
    lw.focus_manager.set_focused_node_for(SEAT, Some(dnid(HOST)));
    lw.text_edit_manager.set_seat_caret(
        SEAT,
        dnid(HOST),
        second,
        caret(0, 1, CursorAffinity::Leading),
    );

    let _ = lw.record_text_input_for_seat(SEAT, "x");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, SECOND), "txwo");
    assert_eq!(text_of(&lw, FIRST), "one");
}

#[test]
fn a_seats_backspace_deletes_in_the_paragraph_its_caret_is_in() {
    let mut lw = two_paragraphs();
    let second = block_of(&lw, SECOND);
    lw.text_edit_manager.set_seat_caret(
        SEAT,
        dnid(HOST),
        second,
        caret(0, 2, CursorAffinity::Leading),
    );

    assert!(lw.apply_selection_op_for_seat(
        SEAT,
        dnid(HOST),
        &SelectionOp::new(
            SelectionDirection::Backward,
            SelectionStep::Character,
            SelectionMode::Delete,
        ),
    ));

    assert_eq!(text_of(&lw, SECOND), "to");
    assert_eq!(text_of(&lw, FIRST), "one");
}
