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

/// The step resolver reads the SHAPED layout, which this harness does not
/// re-run after an edit, so every arrow here moves within the original
/// "bbb" - the same constraint the primary's tests live under.
#[test]
fn a_seats_backspace_arrows_and_shift_selection_act_on_its_own_caret() {
    use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};
    let op = |direction, mode| SelectionOp::new(direction, SelectionStep::Character, mode);

    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 0);
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));

    // Left arrow: a seat with no caret in B starts at its end (3) and moves to 2.
    assert!(lw.apply_selection_op_for_seat(
        SEAT,
        node(TEXT_B),
        &op(SelectionDirection::Backward, SelectionMode::Move)
    ));
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 2)));
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run),
        Some(0),
        "the primary's caret in A is not consulted"
    );

    // Backspace: the seat's own caret, in its own field.
    assert!(lw.apply_selection_op_for_seat(
        SEAT,
        node(TEXT_B),
        &op(SelectionDirection::Backward, SelectionMode::Delete)
    ));
    assert_eq!(text_of(&lw, TEXT_B), "bb");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 1)));
    assert_eq!(text_of(&lw, TEXT_A), "aaa", "the primary's field is untouched");

    // Shift+Right selects the second "b"; typing replaces the selection.
    assert!(lw.apply_selection_op_for_seat(
        SEAT,
        node(TEXT_B),
        &op(SelectionDirection::Forward, SelectionMode::Extend)
    ));
    let caret = lw.text_edit_manager.seat_caret(SEAT).unwrap();
    assert_eq!(caret.anchor.map(|a| a.cluster_id.start_byte_in_run), Some(1));
    assert_eq!(caret.cursor.cluster_id.start_byte_in_run, 2);
    let _ = lw.record_text_input_for_seat(SEAT, "Z");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, TEXT_B), "bZ");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 2)));
    assert!(lw.text_edit_manager.seat_caret(SEAT).unwrap().anchor.is_none());

    // Delete forward at the end is a no-op that reports so.
    assert!(!lw.apply_selection_op_for_seat(
        SEAT,
        node(TEXT_B),
        &op(SelectionDirection::Forward, SelectionMode::Delete)
    ));
    assert_eq!(text_of(&lw, TEXT_B), "bZ");

    // The primary's own Backspace at byte 0 of A is a no-op too, and the
    // seat's caret is not consulted for it.
    assert!(!lw.apply_selection_op(
        node(TEXT_A),
        &op(SelectionDirection::Backward, SelectionMode::Delete)
    ));
    assert_eq!(text_of(&lw, TEXT_A), "aaa");
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 2)));
}

/// The edit primitive used to decide "applied" from the byte and run
/// deltas, so overwriting one selected character with one character - no
/// delta either way - was reported as `EverySelectionMissed` and dropped.
#[test]
fn overwriting_a_one_character_selection_with_one_character_is_applied() {
    use azul_core::selection::{Selection, SelectionRange};
    use azul_layout::text3::edit::{edit_text_outcome, EditOutcome, TextEdit};
    let lw = two_fields();
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(TEXT_B));
    let selection = Selection::Range(SelectionRange {
        start: at(1),
        end: at(2),
    });
    match edit_text_outcome(&content, &[selection], &TextEdit::Insert("Z".into())) {
        EditOutcome::Applied { content, .. } => {
            assert_eq!(lw.extract_text_from_inline_content(&content), "bZb");
        }
        EditOutcome::NoOp(reason) => panic!("a same-length replacement was dropped: {reason:?}"),
    }
}

/// A seat's Ctrl+A / Ctrl+C / Ctrl+X (9b-ii-a-i-d-ii-b-i): select-all spans the
/// seat's node, the selected text is what Copy puts on the clipboard, and Cut
/// is that plus the seat's delete op - with the primary's field and caret
/// untouched throughout.
#[test]
fn a_seats_select_all_copy_text_and_cut_act_on_its_own_field() {
    use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};
    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 1);
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));

    assert_eq!(lw.seat_selected_text(SEAT), None, "a bare caret copies nothing");
    assert!(lw.select_all_for_seat(SEAT, node(TEXT_B)));
    let caret = lw.text_edit_manager.seat_caret(SEAT).expect("a seat selection");
    assert_eq!(caret.anchor.map(|a| a.cluster_id.start_byte_in_run), Some(0));
    assert_eq!(lw.seat_selected_text(SEAT).as_deref(), Some("bbb"));

    // Cut = the seat's delete op over its anchored selection.
    let delete = SelectionOp::new(
        SelectionDirection::Backward,
        SelectionStep::Character,
        SelectionMode::Delete,
    );
    assert!(lw.apply_selection_op_for_seat(SEAT, node(TEXT_B), &delete));
    assert_eq!(text_of(&lw, TEXT_B), "");
    assert_eq!(text_of(&lw, TEXT_A), "aaa");
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run),
        Some(1),
        "the primary's caret in A is untouched"
    );
    // Select-all is a seat-only helper: the primary keeps its cross-block path.
    assert!(!lw.select_all_for_seat(azul_core::window::PRIMARY_POINTER_SEAT, node(TEXT_A)));
}

/// A seat's Enter (9b-ii-a-i-d-ii-b-ii): the structural split is recorded at
/// the SEAT's caret in the seat's node, and the editing query that decides
/// whether Backspace merges blocks reads the seat's caret - both while the
/// primary's caret sits in another field.
#[test]
fn a_seats_enter_splits_at_its_own_caret() {
    use azul_core::events::{
        DefaultAction, SelectionDirection, SelectionMode, SelectionOp, SelectionStep,
    };
    use azul_layout::managers::changeset::{DocumentOperation, NodePosition};

    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 1);
    // The seat focuses field B's HOST (the contenteditable div), as a click
    // would; its caret lives in the text child.
    const DIV_B: usize = 3;
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(DIV_B)));
    let left = SelectionOp::new(
        SelectionDirection::Backward,
        SelectionStep::Character,
        SelectionMode::Move,
    );
    assert!(lw.apply_selection_op_for_seat(SEAT, node(TEXT_B), &left));
    assert_eq!(seat_caret_byte(&lw), Some((TEXT_B, 2)));

    // The editing query answers for the SEAT's caret (mid-block), not the
    // primary's.
    let q = lw
        .build_editing_query_state_for_seat(SEAT, Some(node(DIV_B)))
        .expect("the host is contenteditable");
    assert!(q.is_contenteditable);
    assert!(!q.cursor_at_block_start);
    assert!(!q.cursor_at_block_end);

    // Enter: split the host at the seat's caret - text child 0, byte 2.
    let id = lw
        .record_structural_default_action_for_seat(
            SEAT,
            &DefaultAction::SplitBlockAtCursor { target: node(DIV_B) },
        )
        .expect("a split is recorded");
    let pending = lw
        .pending_document_edit
        .as_ref()
        .expect("the recorded edit awaits the app");
    assert_eq!(pending.id, id);
    match &pending.operation {
        DocumentOperation::SplitNode(split) => {
            assert_eq!(split.node, node(DIV_B));
            assert_eq!(split.at, NodePosition::in_text_child(0, 2));
        }
        other => panic!("expected a split, got {other:?}"),
    }
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run),
        Some(1),
        "the primary's caret in A is untouched"
    );

    // And a seat caret at the block's start makes Backspace a merge question.
    lw.text_edit_manager.set_seat_caret(SEAT, node(TEXT_B), at(0));
    let q = lw
        .build_editing_query_state_for_seat(SEAT, Some(node(DIV_B)))
        .expect("the host is contenteditable");
    assert!(q.cursor_at_block_start);
}

/// A seat's caret and selection are DRAWN (9b-ii-a-i-d-ii-a): in the seat's
/// own colour, on the seat's node, solid while no primary session runs -
/// the display list carries a `CursorRect` for the seat with no primary
/// editing at all, and `SelectionRect` bands once the seat selects.
#[test]
fn a_seats_caret_and_selection_are_drawn_in_its_colour() {
    use azul_core::selection::SelectionOwner;
    use azul_layout::solver3::display_list::DisplayListItem;

    let items = |lw: &LayoutWindow| -> Vec<DisplayListItem> {
        lw.get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .clone()
    };
    let cursor_rects = |lw: &LayoutWindow| {
        items(lw)
            .iter()
            .filter(|i| matches!(i, DisplayListItem::CursorRect { .. }))
            .count()
    };
    let selection_rects = |lw: &LayoutWindow| {
        items(lw)
            .iter()
            .filter(|i| matches!(i, DisplayListItem::SelectionRect { .. }))
            .count()
    };

    let mut lw = two_fields();
    assert_eq!(cursor_rects(&lw), 0, "premise: nothing edits, nothing is drawn");

    // No primary session at all; the seat alone types into B.
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));
    let _ = lw.record_text_input_for_seat(SEAT, "x");
    let _ = lw.apply_text_changeset();
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(cursor_rects(&lw), 1, "the seat's caret is painted without a primary session");
    assert_eq!(selection_rects(&lw), 0);
    let owner = SelectionOwner::seat(SEAT);
    assert!(owner.is_seat() && !owner.is_local());
    assert_eq!(owner.seat_id(), Some(SEAT));
    assert_eq!(
        lw.text_edit_manager.owner_color(owner),
        Some(azul_layout::managers::text_edit::seat_owner_color(SEAT)),
        "the seat got its palette colour"
    );
    let locations = lw.text_edit_manager.build_cursor_locations();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].owner, owner);
    assert_eq!(locations[0].node, NodeId::new(TEXT_B));

    // Select-all: the selection is painted as the seat's tinted bands.
    assert!(lw.select_all_for_seat(SEAT, node(TEXT_B)));
    assert!(selection_rects(&lw) >= 1, "the seat's selection paints bands");
    let map = lw.text_edit_manager.build_text_selections_map();
    let sel = map.get(&DomId::ROOT_ID).expect("a selection entry for the seat's dom");
    let remote = sel.remote_ranges.get(&NodeId::new(TEXT_B)).expect("the seat's range");
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].0, owner);
    // Seat 0 is the primary: never a seat owner.
    assert!(SelectionOwner::seat(0).is_local());
}

/// A seat's input-method composition (9b-ii-a-i-d-ii-c) is its own: stored per
/// seat, spliced into the text at the SEAT's caret for shaping, underlined at
/// the seat's location, and gone on commit - the primary's preedit untouched.
#[test]
fn a_seats_composition_is_shaped_at_its_caret_and_underlined() {
    use azul_layout::solver3::display_list::DisplayListItem;
    let underlines = |lw: &LayoutWindow| {
        lw.get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter(|i| matches!(i, DisplayListItem::Underline { .. }))
            .count()
    };

    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 1);
    lw.focus_manager.set_focused_node_for(SEAT, Some(node(TEXT_B)));
    lw.text_edit_manager.set_seat_caret(SEAT, node(TEXT_B), at(3));
    assert_eq!(underlines(&lw), 0, "premise: nothing composes");

    // The seat composes "ni" at the end of B: stored per seat, shaped in, underlined.
    lw.text_edit_manager
        .set_preedit_for_seat(SEAT, "ni".to_string(), 0, 2);
    assert_eq!(
        lw.text_edit_manager.seat_preedit(SEAT).map(|p| p.text.as_str()),
        Some("ni")
    );
    assert!(lw.text_edit_manager.preedit_text.is_none(), "the primary's preedit is untouched");
    lw.apply_seat_preedit_to_text_cache(SEAT, DomId::ROOT_ID, NodeId::new(TEXT_B));
    assert_eq!(text_of(&lw, TEXT_B), "bbb", "the committed text is unchanged by a composition");
    let locations = lw.text_edit_manager.build_cursor_locations();
    let seat_loc = locations
        .iter()
        .find(|l| l.owner == azul_core::selection::SelectionOwner::seat(SEAT))
        .expect("the seat's location");
    assert_eq!((seat_loc.preedit_bytes, seat_loc.preedit_chars), (2, 2));
    assert!(underlines(&lw) >= 1, "the seat's composition is underlined");

    // Commit: the composition ends, the text lands at the seat's caret.
    lw.text_edit_manager
        .commit_composition_for_seat(SEAT, "ni".to_string());
    lw.end_seat_preedit_shaping(SEAT);
    let _ = lw.record_text_input_for_seat(SEAT, "ni");
    let _ = lw.apply_text_changeset();
    assert!(lw.text_edit_manager.seat_preedit(SEAT).is_none());
    assert_eq!(text_of(&lw, TEXT_B), "bbbni");
    assert_eq!(text_of(&lw, TEXT_A), "aaa");
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(underlines(&lw), 0, "nothing composes any more");
}

/// 9b-ii-a-i-d-ii-c-ii: the rectangle a seat's input method is told about is
/// the SEAT caret's, in its own field, not the primary's.
#[test]
fn a_seats_caret_rect_is_the_seats_not_the_primarys() {
    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 0);
    lw.text_edit_manager.set_seat_caret(SEAT, node(TEXT_B), at(2));
    let primary = lw
        .get_focused_cursor_rect_viewport()
        .expect("the primary's caret has a rectangle");
    let seat = lw
        .seat_cursor_rect_viewport(SEAT)
        .expect("the seat's caret has a rectangle");
    assert_eq!(
        lw.seat_cursor_rect_viewport(azul_core::window::PRIMARY_POINTER_SEAT),
        Some(primary),
        "seat 0 is the primary"
    );
    assert!(
        seat.origin.y > primary.origin.y,
        "field B lies below field A: seat {seat:?} vs primary {primary:?}"
    );
    assert!(seat.origin.x > primary.origin.x, "byte 2 of B sits right of byte 0 of A");
    assert!(lw.seat_cursor_rect_viewport(99).is_none(), "no caret, no rectangle");
}

/// 9b-ii-a-i-d-ii-c-i: a seat's input method raises CompositionStart /
/// Update / End stamped with the seat, so the Focus filter delivers them to
/// the seat's focused node - and the primary's composition queue stays empty.
#[test]
fn a_seats_composition_raises_its_own_events() {
    use azul_core::events::{EventData, EventProvider, EventType};
    use azul_core::task::Instant;
    let ts = || Instant::from(std::time::Instant::now());
    let phases = |lw: &LayoutWindow| -> Vec<(EventType, String, u64)> {
        lw.text_edit_manager
            .get_pending_events(ts())
            .iter()
            .filter_map(|e| match &e.data {
                EventData::Composition(c) => Some((e.event_type, c.data.clone(), c.seat_id)),
                _ => None,
            })
            .collect()
    };

    let mut lw = two_fields();
    primary_edits(&mut lw, TEXT_A, 0);
    lw.text_edit_manager.set_seat_caret(SEAT, node(TEXT_B), at(3));

    lw.text_edit_manager
        .set_preedit_for_seat(SEAT, "n".to_string(), 0, 1);
    assert_eq!(
        phases(&lw),
        vec![(EventType::CompositionStart, "n".to_string(), SEAT)]
    );
    assert_eq!(lw.text_edit_manager.take_pending_composition(), None, "not the primary's");
    let _ = lw.text_edit_manager.take_pending_seat_compositions();
    assert!(phases(&lw).is_empty(), "drained after the pass");

    lw.text_edit_manager
        .set_preedit_for_seat(SEAT, "ni".to_string(), 0, 2);
    assert_eq!(
        phases(&lw),
        vec![(EventType::CompositionUpdate, "ni".to_string(), SEAT)]
    );
    let _ = lw.text_edit_manager.take_pending_seat_compositions();

    lw.text_edit_manager
        .commit_composition_for_seat(SEAT, "ni".to_string());
    assert_eq!(
        phases(&lw),
        vec![(EventType::CompositionEnd, "ni".to_string(), SEAT)],
        "End carries the COMMITTED text"
    );
    assert!(lw.text_edit_manager.seat_preedit(SEAT).is_none());
    let _ = lw.text_edit_manager.take_pending_seat_compositions();

    // A composition that vanishes without a commit is a cancel: End, empty.
    lw.text_edit_manager
        .set_preedit_for_seat(SEAT, "x".to_string(), 0, 1);
    let _ = lw.text_edit_manager.take_pending_seat_compositions();
    lw.text_edit_manager.clear_preedit_for_seat(SEAT);
    assert_eq!(
        phases(&lw),
        vec![(EventType::CompositionEnd, String::new(), SEAT)]
    );
    assert_eq!(lw.text_edit_manager.take_pending_composition(), None);
}
