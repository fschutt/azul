//! Regression pins for the text-input / caret / selection seam audit.
//!
//! Each test drives the public engine API the shells drive and asserts on what
//! the user ends up with — the text buffer, or the items in the display list.
//!
//! - IME composition must live in the SHAPING only, never in the text store,
//!   so committing inserts once and cancelling leaves nothing behind.
//! - The end-of-pass focus finalize must not overwrite a caret a click just
//!   placed on the IFC root of a `container > p > text` editable.
//! - Typing into a node that only INHERITS `contenteditable` must land.
//! - A click on plain selectable text must not claim the blink timer, and a
//!   focus change must not erase the selection that same click made.
//! - The FIRST character typed into an EMPTY editable must land: it has no
//!   text run for the insert to splice into.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState, CursorBlinkTimerAction,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .p { display: block; } \
                   .big { font-size: 40px; }";

fn block(child: Dom) -> Dom {
    with_class(Dom::create_div(), "p").with_child(child)
}

/// A child `Dom`'s own `with_css` never reaches the cascade here:
/// `StyledDom::create` takes ONE stylesheet and, unlike `create_from_dom`,
/// never collects the per-subtree sheets `Dom::set_css` attaches. Every rule a
/// fixture needs lives in [`CSS`] and is selected by class.
fn with_class(dom: Dom, class: &'static str) -> Dom {
    let ids: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class(class.into())].into();
    dom.with_ids_and_classes(ids)
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn layout(mut dom: Dom) -> LayoutWindow {
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

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
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

fn text_of(lw: &LayoutWindow, node: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(node));
    lw.extract_text_from_inline_content(&content)
}

fn glyph_count(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.len(),
            _ => 0,
        })
        .sum()
}

fn selection_band_count(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::SelectionRect { .. }))
        .count()
}

fn session_node(lw: &LayoutWindow) -> Option<NodeId> {
    lw.text_edit_manager
        .multi_cursor
        .as_ref()
        .and_then(|mc| mc.node_id.node.into_crate_internal())
}

fn selections(lw: &LayoutWindow) -> Vec<Selection> {
    lw.text_edit_manager
        .multi_cursor
        .as_ref()
        .map(azul_core::selection::MultiCursorState::to_selections)
        .unwrap_or_default()
}

/// The caret's own first position, read the way the caret path reads it.
///
/// Under the default `AZ_DENSE_TEXT=1` the sparse `UnifiedLayout` stored on an
/// IFC root is the shared RETIREMENT SENTINEL — zero items, whatever the text
/// says. `session_inline_geometry` and the paint pass both expand the dense
/// arrays instead, so a pin that reads the stored Arc directly measures the
/// sentinel and not the layout.
fn first_cluster_cursor(lw: &LayoutWindow, node: usize) -> TextCursor {
    let tree = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .layout_tree;
    let index = tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|indices| indices.first())
        .expect("the node has a layout box");
    let layout = tree
        .materialized_inline_layout_for_node(index.index())
        .expect("the node establishes an inline layout");
    layout
        .items
        .iter()
        .find_map(|item| match &item.item {
            azul_layout::text3::cache::ShapedItem::Cluster(c) => Some(TextCursor {
                cluster_id: c.source_cluster_id,
                affinity: CursorAffinity::Leading,
            }),
            _ => None,
        })
        .expect("the inline layout has at least one cluster")
}

// ---------------------------------------------------------------------------
// IME preedit: the composition is glyphs, never text
// ---------------------------------------------------------------------------

/// `body(0) > div[contenteditable](1) > text(2)` — the div is the IFC root.
const EDITOR: usize = 1;

fn flat_editable(content: &str) -> LayoutWindow {
    layout(
        Dom::create_body()
            .with_child(Dom::create_div().with_contenteditable(true).with_child(text(content))),
    )
}

fn start_editing(lw: &mut LayoutWindow, node: usize, at: u32) {
    lw.focus_manager.set_focused_node(Some(dnid(node)));
    lw.text_edit_manager
        .initialize_editing(cursor(at), DomId::ROOT_ID, NodeId::new(node), 0);
}

#[test]
fn an_ime_composition_never_reaches_the_text_store() {
    let mut lw = flat_editable("ab");
    start_editing(&mut lw, EDITOR, 0);

    lw.text_edit_manager.set_preedit("xy".to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));

    assert_eq!(
        text_of(&lw, EDITOR),
        "ab",
        "the composition is on screen, but the document text is untouched"
    );
}

#[test]
fn committing_an_ime_composition_inserts_the_text_exactly_once() {
    let mut lw = flat_editable("ab");
    start_editing(&mut lw, EDITOR, 0);

    lw.text_edit_manager.set_preedit("xy".to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));

    // The commit sequence every shell runs: drop the composition, then deliver
    // the committed string as ordinary text input.
    lw.text_edit_manager.clear_preedit();
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));
    let _ = lw.record_text_input("xy");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, EDITOR), "xyab");
}

#[test]
fn cancelling_an_ime_composition_puts_the_clean_base_back_on_screen() {
    let mut lw = flat_editable("ab");
    start_editing(&mut lw, EDITOR, 0);
    let base = glyph_count(&lw);

    lw.text_edit_manager.set_preedit("xy".to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));
    assert_eq!(
        glyph_count(&lw),
        base + 2,
        "premise: the composed glyphs are painted"
    );

    lw.text_edit_manager.clear_preedit();
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));

    assert_eq!(glyph_count(&lw), base, "no composed glyph survives the cancel");
    assert_eq!(text_of(&lw, EDITOR), "ab");
}

// ---------------------------------------------------------------------------
// Focus finalize vs. the click caret
// ---------------------------------------------------------------------------

/// `body(0) > div[contenteditable](1) > div.p(2) > text(3)` — the P-wrap
/// convention. The click keys the session on node 2, which is neither the
/// pending container (1) nor `find_last_text_child`'s node (3).
const CONTAINER: usize = 1;
const IFC_ROOT: usize = 2;

#[test]
fn the_first_click_into_a_p_wrapped_editable_keeps_the_clicked_caret() {
    let mut lw = layout(Dom::create_body().with_child(
        Dom::create_div().with_contenteditable(true).with_child(block(text("hello world"))),
    ));

    lw.process_mouse_click_for_selection(LogicalPosition::new(6.0, 6.0), 0)
        .expect("the click lands in the paragraph");
    let clicked = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("the click opened an editing session");
    assert_eq!(session_node(&lw), Some(NodeId::new(IFC_ROOT)));
    assert!(
        clicked.cluster_id.start_byte_in_run < 4,
        "premise: the caret is near the START of the text, got {clicked:?}"
    );

    // Same pass: click-to-focus resolves SetFocus on the container.
    let window_state = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(CONTAINER)), &window_state);
    lw.finalize_pending_focus_changes();

    assert_eq!(
        session_node(&lw),
        Some(NodeId::new(IFC_ROOT)),
        "the session stays on the IFC root the click keyed it to"
    );
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor(),
        Some(clicked),
        "the end-of-text seed must not override a caret the user just placed"
    );
}

// ---------------------------------------------------------------------------
// Inherited contenteditable
// ---------------------------------------------------------------------------

#[test]
fn typing_into_a_node_that_only_inherits_contenteditable_reaches_the_text() {
    // body(0) > div[contenteditable](1) > div.p(2) > text(3)
    let mut lw = layout(Dom::create_body().with_child(
        Dom::create_div().with_contenteditable(true).with_child(block(text("hi"))),
    ));
    const INNER: usize = 2;

    lw.focus_manager.set_focused_node(Some(dnid(INNER)));
    let affected = lw.record_text_input("X");
    assert_eq!(affected.len(), 1, "the input is recorded against the focus");
    let _ = lw.apply_text_changeset();

    assert_eq!(
        text_of(&lw, INNER),
        "Xhi",
        "the editable ancestor makes this node editable too"
    );
}

// ---------------------------------------------------------------------------
// Clicks on plain selectable text
// ---------------------------------------------------------------------------

/// `body(0) > [ div.p(1) > text(2), second(3) > text(4) ]` — two stacked
/// blocks, so a click at y=6 lands in the static one.
const STATIC_BLOCK: usize = 1;
const SECOND_BLOCK: usize = 3;

fn static_text_then(second: Dom) -> LayoutWindow {
    layout(
        Dom::create_body()
            .with_child(block(text("static text")))
            .with_child(second),
    )
}

#[test]
fn clicking_plain_selectable_text_does_not_claim_the_blink_timer() {
    let mut lw = static_text_then(
        Dom::create_div().with_contenteditable(true).with_child(text("editable")),
    );

    lw.process_mouse_click_for_selection(LogicalPosition::new(6.0, 6.0), 0)
        .expect("the click lands in the static paragraph");
    assert_eq!(session_node(&lw), Some(NodeId::new(STATIC_BLOCK)));
    assert!(
        !lw.text_edit_manager.blink.is_blink_timer_active(),
        "nothing armed an OS timer, so the flag must stay down"
    );

    let window_state = lw.current_window_state.clone();
    assert!(
        matches!(
            lw.handle_focus_change_for_cursor_blink(Some(dnid(SECOND_BLOCK)), &window_state),
            CursorBlinkTimerAction::Start(_)
        ),
        "the first focus into a real editable must still ARM the blink timer"
    );
}

#[test]
fn focusing_a_plain_node_keeps_the_selection_the_same_click_made() {
    let mut lw = static_text_then(block(text("a button")));

    lw.process_mouse_click_for_selection(LogicalPosition::new(6.0, 6.0), 0)
        .expect("the click lands in the static paragraph");
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("the click opened a session")
        .set_single_range(SelectionRange {
            start: cursor(0),
            end: cursor(6),
        });
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert!(
        selection_band_count(&lw) > 0,
        "premise: the drag painted a highlight"
    );
    assert!(lw.text_edit_manager.blink.is_visible);

    let window_state = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(SECOND_BLOCK)), &window_state);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);

    assert!(
        selection_band_count(&lw) > 0,
        "focusing elsewhere must not erase a selection over static text"
    );
    assert!(
        !lw.text_edit_manager.blink.is_visible,
        "the caret machinery still goes away"
    );
}

// ---------------------------------------------------------------------------
// Cursor movement: extend, and what Home/End does to a live range
// ---------------------------------------------------------------------------

#[test]
fn handle_cursor_movement_extends_from_the_anchor_instead_of_collapsing() {
    let mut lw = flat_editable("hello world");
    start_editing(&mut lw, EDITOR, 0);

    lw.handle_cursor_movement(DomId::ROOT_ID, NodeId::new(EDITOR), cursor(5), true);
    assert_eq!(
        selections(&lw),
        vec![Selection::Range(SelectionRange {
            start: cursor(0),
            end: cursor(5),
        })],
        "shift+move turns the caret into a range"
    );

    lw.handle_cursor_movement(DomId::ROOT_ID, NodeId::new(EDITOR), cursor(8), true);
    assert_eq!(
        selections(&lw),
        vec![Selection::Range(SelectionRange {
            start: cursor(0),
            end: cursor(8),
        })],
        "a second extend keeps the anchor and travels only the focus"
    );

    lw.handle_cursor_movement(DomId::ROOT_ID, NodeId::new(EDITOR), cursor(2), false);
    assert_eq!(
        selections(&lw),
        vec![Selection::Cursor(cursor(2))],
        "a bare move still collapses"
    );
}

#[test]
fn home_with_an_active_selection_travels_to_the_line_start() {
    let mut lw = flat_editable("hello world");
    start_editing(&mut lw, EDITOR, 0);
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("session")
        .set_single_range(SelectionRange {
            start: cursor(4),
            end: cursor(8),
        });

    let home = SelectionOp::new(
        SelectionDirection::Backward,
        SelectionStep::Line,
        SelectionMode::Move,
    );
    assert!(lw.apply_selection_op(dnid(EDITOR), &home));

    assert_eq!(
        lw.text_edit_manager
            .get_primary_cursor()
            .map(|c| c.cluster_id.start_byte_in_run),
        Some(0),
        "Home performs the movement; collapsing to the range's min edge would answer 4"
    );

    // Left/Right are the ONE step kind that still collapses to the boundary.
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("session")
        .set_single_range(SelectionRange {
            start: cursor(4),
            end: cursor(8),
        });
    let left = SelectionOp::new(
        SelectionDirection::Backward,
        SelectionStep::Character,
        SelectionMode::Move,
    );
    assert!(lw.apply_selection_op(dnid(EDITOR), &left));
    assert_eq!(
        lw.text_edit_manager
            .get_primary_cursor()
            .map(|c| c.cluster_id.start_byte_in_run),
        Some(4),
    );
}

// ---------------------------------------------------------------------------
// Caret geometry
// ---------------------------------------------------------------------------

#[test]
fn the_caret_rect_answers_for_the_session_node_not_the_focused_node() {
    // body(0) > container(1) > div.p(2) > text(3)
    let mut lw = layout(Dom::create_body().with_child(
        Dom::create_div().with_contenteditable(true).with_child(block(text("hello world"))),
    ));
    let at_start = first_cluster_cursor(&lw, IFC_ROOT);

    lw.text_edit_manager
        .initialize_editing(at_start, DomId::ROOT_ID, NodeId::new(3), 0);
    assert!(
        lw.focus_manager.get_focused_node().is_none(),
        "premise: nothing is focused, only a session exists"
    );

    let from_text_node = lw
        .get_focused_cursor_rect()
        .expect("the session's text node resolves through the IFC root above it");
    assert!(from_text_node.size.height > 0.0);

    lw.text_edit_manager
        .initialize_editing(at_start, DomId::ROOT_ID, NodeId::new(IFC_ROOT), 0);
    assert_eq!(
        lw.get_focused_cursor_rect(),
        Some(from_text_node),
        "the IFC root and the text node inside it name the same caret"
    );
}

#[test]
fn a_caret_two_levels_below_the_ifc_root_is_painted() {
    // body(0) > container(1) > div.p(2) > span(3) > text(4)
    let mut lw = layout(Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_child(block(Dom::create_span_with_text("rich text"))),
    ));
    const DEEP_TEXT: usize = 4;

    let at_start = first_cluster_cursor(&lw, IFC_ROOT);
    lw.text_edit_manager
        .initialize_editing(at_start, DomId::ROOT_ID, NodeId::new(DEEP_TEXT), 0);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);

    assert!(
        lw.get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .any(|item| matches!(item, DisplayListItem::CursorRect { .. })),
        "the caret belongs to the IFC root's paint pass however deep its text sits"
    );
}

// ---------------------------------------------------------------------------
// Preedit underline geometry
// ---------------------------------------------------------------------------

fn underline_width(lw: &LayoutWindow) -> Option<f32> {
    lw.get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Underline { bounds, .. } => Some(bounds.0.size.width),
            _ => None,
        })
}

/// Total advance of the clusters the composition spliced in at byte 0 of the
/// run — the quantity the underline is supposed to span.
fn composed_advance(lw: &LayoutWindow, node: usize, byte_len: u32) -> f32 {
    let tree = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .layout_tree;
    let index = tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|indices| indices.first())
        .expect("the node has a layout box");
    let layout = tree
        .materialized_inline_layout_for_node(index.index())
        .expect("the node establishes an inline layout");
    layout
        .items
        .iter()
        .filter_map(|item| match &item.item {
            azul_layout::text3::cache::ShapedItem::Cluster(c)
                if c.source_cluster_id.source_run == 0
                    && c.source_cluster_id.start_byte_in_run < byte_len =>
            {
                Some(c.advance)
            }
            _ => None,
        })
        .sum()
}

/// The `caret-width` the paint pass resolves for this node — the same value
/// the fallback estimate is built from.
fn caret_width(lw: &LayoutWindow, node: usize) -> f32 {
    azul_layout::solver3::getters::get_caret_style(
        &lw.get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .styled_dom,
        Some(NodeId::new(node)),
    )
    .width
}

#[test]
fn the_preedit_underline_spans_the_measured_advance_not_an_eight_pixel_guess() {
    let mut lw = layout(
        Dom::create_body().with_child(
            with_class(Dom::create_div(), "big")
                .with_contenteditable(true)
                .with_child(text("ab")),
        ),
    );
    start_editing(&mut lw, EDITOR, 0);

    let composed = "MM";
    lw.text_edit_manager.set_preedit(composed.to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(EDITOR));

    let width = underline_width(&lw).expect("a live composition is underlined");
    let measured = composed_advance(&lw, EDITOR, composed.len() as u32);
    // The estimate the fallback still draws when the composition is not in the
    // cache: `char_count * max(caret_width, 8px)`.
    let estimated = composed.chars().count() as f32 * caret_width(&lw, EDITOR).max(8.0);

    assert!(
        (width - measured).abs() < 0.01,
        "the underline spans the shaped advance of the composed clusters: \
         measured {measured}, drew {width}"
    );
    assert!(
        (width - estimated).abs() > 1.0,
        "premise: at 40px the two shapings disagree, so the assertion above can \
         tell them apart — measured {measured}, estimate {estimated}"
    );
}

// ---------------------------------------------------------------------------
// Deleting a selection
// ---------------------------------------------------------------------------

#[test]
fn deleting_a_range_removes_all_of_it_and_leaves_an_undo_entry() {
    let mut lw = flat_editable("hello world");
    start_editing(&mut lw, EDITOR, 0);
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("session")
        .set_single_range(SelectionRange {
            start: cursor(0),
            end: cursor(6),
        });

    lw.delete_selection(dnid(EDITOR), false)
        .expect("the delete applies");

    assert_eq!(
        text_of(&lw, EDITOR),
        "world",
        "the whole range goes, not one grapheme at the primary cursor"
    );
    assert!(
        lw.undo_redo_manager.can_undo(NodeId::new(EDITOR)),
        "a delete is an undoable edit"
    );
}

// ---------------------------------------------------------------------------
// Focus finalize retry
// ---------------------------------------------------------------------------

#[test]
fn a_pending_focus_on_an_unlaid_node_retries_before_seeding_the_start() {
    let mut lw = flat_editable("hello world");
    // A node the layout tree has never heard of: exactly the pre-first-layout
    // shape, where seeding cluster (0,0) means a caret at the start of a text
    // nobody has measured.
    let unlaid = NodeId::new(999);
    lw.focus_manager
        .set_pending_contenteditable_focus(DomId::ROOT_ID, NodeId::new(EDITOR), unlaid);

    for attempt in 0..2 {
        assert!(
            !lw.finalize_pending_focus_changes(),
            "attempt {attempt} must defer, not seed"
        );
        assert!(
            lw.focus_manager.needs_cursor_initialization(),
            "attempt {attempt} must put the request back"
        );
        assert!(lw.text_edit_manager.multi_cursor.is_none());
    }

    assert!(
        lw.finalize_pending_focus_changes(),
        "the retry budget is bounded — the caret is seeded rather than deferred forever"
    );
    assert!(!lw.focus_manager.needs_cursor_initialization());
    assert_eq!(session_node(&lw), Some(unlaid));
}

/// Two keystrokes arriving before one pass must BOTH land.
///
/// `record_input` overwrote a single slot, so the first character was gone
/// before it was ever applied. Making it a queue is only half the fix: a queue
/// drained one entry per pass and then cleared loses exactly the same
/// characters, so the apply path has to run to empty.
#[test]
fn two_keystrokes_recorded_before_one_pass_both_land() {
    let mut lw = flat_editable("");
    start_editing(&mut lw, EDITOR, 0);

    let _ = lw.record_text_input("a");
    let _ = lw.record_text_input("b");
    let _ = lw.apply_text_changeset();

    assert_eq!(
        text_of(&lw, EDITOR),
        "ab",
        "the first keystroke was dropped before it reached the document"
    );
}

/// And a whole burst, in order — a fast typist outrunning one frame.
#[test]
fn a_burst_of_keystrokes_lands_in_order() {
    let mut lw = flat_editable("");
    start_editing(&mut lw, EDITOR, 0);

    for ch in ["h", "e", "l", "l", "o"] {
        let _ = lw.record_text_input(ch);
    }
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, EDITOR), "hello");
}

/// An IME composition two blocks down: `div[contenteditable] > div > div > text`.
///
/// `reshape_text_node` looks for the box that owns the editable's inline
/// layout by scanning the editable's children — and that scan walked ONE
/// level, so in ordinary nested markup it found nothing and returned early.
/// The composition never appeared on screen.
#[test]
fn an_ime_composition_in_a_deeply_nested_editable_is_shaped() {
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_div().with_child(Dom::create_div().with_child(text("ab"))),
        ),
    );
    let mut lw = layout(dom);

    // body(0) > div[ce](1) > div(2) > div(3) > text(4)
    let editable = 1;
    let leaf = 4;
    lw.focus_manager.set_focused_node(Some(dnid(leaf)));
    lw.text_edit_manager
        .initialize_editing(cursor(0), DomId::ROOT_ID, NodeId::new(leaf), 0);

    let before = glyph_count(&lw);
    lw.text_edit_manager.set_preedit("xy".to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(editable));

    assert!(
        glyph_count(&lw) > before,
        "the composition was never shaped: {before} glyphs before, {} after",
        glyph_count(&lw)
    );
}

// ---------------------------------------------------------------------------
// The first character typed into an EMPTY editable
// ---------------------------------------------------------------------------

/// `text3::edit::insert_text` splices into `content[cursor.source_run]`. An
/// editable with no text at all collects to an EMPTY buffer, so run 0 does not
/// exist, the splice found nothing and the function returned the content
/// unchanged — the first keystroke was silently dropped and the editable could
/// never be typed into at all. A caret blinked in it the whole time.
///
/// This is the shape a brand-new AzWriter document has (an empty `<p>` under
/// the editing host) and the shape every text widget has before its first
/// character.
///
/// NOT yet fixed, and deliberately not asserted here: the character lands in
/// the BUFFER but still paints no glyph. `reshape_text_node` shapes through
/// `shape_text_for_relayout`, which can only use fonts a previous layout
/// already LOADED — an editable that has never held text never loaded one, so
/// stage 3 returns zero shaped items for the first character and the IFC keeps
/// its empty strut layout. `cargo test ... the_first_character` is green while
/// AzWriter still looks dead on a new document; the missing half is font
/// loading on the incremental reshape path.
#[test]
fn the_first_character_typed_into_an_empty_p_wrapped_editable_lands() {
    // body(0) > div[contenteditable](1) > div.p(2) — no text node anywhere.
    let mut lw = layout(Dom::create_body().with_child(
        Dom::create_div().with_contenteditable(true).with_child(with_class(Dom::create_div(), "p")),
    ));
    const HOST: usize = 1;
    assert_eq!(text_of(&lw, HOST), "", "premise: the editable starts empty");

    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw.record_text_input("X");
    let _ = lw.apply_text_changeset();

    assert_eq!(
        text_of(&lw, HOST),
        "X",
        "the first character typed into an empty editable must land — with no \
         run to splice into, the insert was a silent no-op and the document \
         could not be started"
    );
}

/// The same defect without the block wrapper: a bare `div[contenteditable]`
/// with no children at all.
#[test]
fn the_first_character_typed_into_a_flat_empty_editable_lands() {
    let mut lw = layout(
        Dom::create_body().with_child(Dom::create_div().with_contenteditable(true)),
    );
    const HOST: usize = 1;
    assert_eq!(text_of(&lw, HOST), "", "premise: the editable starts empty");

    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw.record_text_input("X");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, HOST), "X");
}

/// The seeded run must carry the BLOCK's style, not `StyleProperties::default()`
/// — the first character has to be the size the editable is styled at, or every
/// new document starts one character of the wrong font.
#[test]
fn the_first_character_typed_into_an_empty_editable_keeps_the_blocks_style() {
    // `.big` is 40px; the default would be the 14px body size.
    let mut lw = layout(Dom::create_body().with_child(
        with_class(Dom::create_div(), "big").with_contenteditable(true),
    ));
    const HOST: usize = 1;

    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw.record_text_input("X");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, HOST), "X");

    let size = lw
        .get_text_before_textinput(DomId::ROOT_ID, NodeId::new(HOST))
        .iter()
        .find_map(|c| match c {
            azul_layout::text3::cache::InlineContent::Text(run) => Some(run.style.font_size_px),
            _ => None,
        })
        .expect("the edited buffer has a text run");
    assert!(
        (size - 40.0).abs() < 0.01,
        "the seeded run must inherit the editable's own font-size, got {size}"
    );
}
