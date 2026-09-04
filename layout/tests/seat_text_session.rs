//! 9b-ii-a-i-d-ii: a second SEAT's typing lands in ITS focused node, at ITS
//! own caret - never at the primary's, which may sit in another field.
//!
//! Two people, two fields, one window: the primary edits field A with its
//! caret at the start; seat 7 focuses field B and types. Field B grows at
//! the end (a seat without a caret starts where a fresh focus would), field
//! A is untouched, and the seat's caret follows its own edits. When the
//! primary later edits field B in front of the seat's caret, the seat's
//! caret shifts with the text like a peer caret would (U3).

use azul_core::dom::{Dom, DomId, DomNodeId, NodeId};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; } \
                   div { display: block; }";

/// body(0) > div A(1) > text(2), div B(3) > text(4)
const TEXT_A: usize = 2;
const TEXT_B: usize = 4;
const SEAT: u64 = 7;

fn two_fields() -> LayoutWindow {
    let field = |text: &str| {
        Dom::create_div()
            .with_contenteditable(true)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                text.to_string(),
            ))
    };
    let mut dom = Dom::create_body()
        .with_child(field("aaa"))
        .with_child(field("bbb"));
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

fn at(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

/// The primary focuses `index` with its caret at byte `byte`.
fn primary_edits(lw: &mut LayoutWindow, index: usize, byte: u32) {
    lw.focus_manager.set_focused_node(Some(node(index)));
    lw.text_edit_manager
        .initialize_editing(at(byte), DomId::ROOT_ID, NodeId::new(index), 0);
}

fn text_of(lw: &LayoutWindow, index: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(index));
    lw.extract_text_from_inline_content(&content)
}

fn seat_caret_byte(lw: &LayoutWindow) -> Option<(usize, u32)> {
    lw.text_edit_manager.seat_caret(SEAT).map(|c| {
        (
            c.node.node.into_crate_internal().unwrap().index(),
            c.cursor.cluster_id.start_byte_in_run,
        )
    })
}

#[test]
fn a_seat_types_into_its_own_field_at_its_own_caret() {
    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 0);
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));

    // The seat's first keystroke: field B, appended - the seat has no caret
    // in B yet, so it starts at the end, where a fresh focus would.
    let affected = lw.record_text_input_for_seat(SEAT, "x");
    assert!(affected.contains_key(&node(TEXT_B)), "the seat's field is the one affected");
    assert!(!affected.contains_key(&node(TEXT_A)));
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "bbbx");
    assert_eq!(text_of(&lw, TEXT_A), "aaa", "the primary's field is untouched");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 4)), "the seat's caret follows its edit");

    // The second keystroke continues at the seat's caret, not at the end of
    // whatever the primary is doing.
    let _ = lw.record_text_input_for_seat(SEAT, "y");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "bbbxy");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 5)));

    // The primary keeps typing in ITS field at ITS caret (the start).
    let _ = lw.record_text_input("P");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_A), "Paaa");
    assert_eq!(text_of(&lw, TEXT_B), "bbbxy", "the primary's typing never reaches B");
    assert_eq!(
        seat_caret_byte(&lw),
        Some((TEXT_B, 5)),
        "an edit of another node leaves the seat's caret alone"
    );
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run),
        Some(1),
        "the primary's caret advanced in A"
    );
}

#[test]
fn carets_of_both_seats_shift_across_each_others_edits_in_one_field() {
    let mut lw = two_fields();
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));
    let _ = lw.record_text_input_for_seat(SEAT, "xy");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "bbbxy");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 5)));

    // The primary now edits B at the START, in front of the seat's caret.
    primary_edits(&mut lw, TEXT_B, 0);
    let _ = lw.record_text_input("Q");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "Qbbbxy");
    assert_eq!(
        seat_caret_byte(&lw),
        Some((TEXT_B, 6)),
        "the seat's caret moved with the text in front of it (the peer rule)"
    );

    // And the seat's next keystroke, at its (shifted) end caret, leaves the
    // primary's caret - in front of it - where it was.
    let _ = lw.record_text_input_for_seat(SEAT, "z");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "Qbbbxyz");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 7)));
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run),
        Some(1),
        "the primary's caret, before the seat's insertion, is unmoved"
    );
}

#[test]
fn a_seat_without_focus_types_into_nothing() {
    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 0);
    let affected = lw.record_text_input_for_seat(SEAT, "x");
    assert!(affected.is_empty(), "no focus for the seat = nothing recorded");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_A), "aaa", "and certainly not the primary's field");
    assert_eq!(seat_caret_byte(&lw), None);
}
