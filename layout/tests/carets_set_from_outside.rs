//! Carets and selections the app, the IME or a screen reader set from outside
//! land where they say, in the block they name.
//!
//! Three seams hand the engine a position it did not compute itself: the IME's
//! byte offsets (`setSelectedTextRange:`, `firstRectForCharacterRange:`), an
//! accessibility `SetTextSelection` on a node, and the app's
//! `AddCursor` / `AddSelectionRange` on a node. A position is only meaningful
//! in the text block it was made for; each of these used to drop it into
//! whatever session happened to exist, or lose part of it.

use azul_core::{
    dom::{AccessibilityAction, Dom, DomId, DomNodeId, IdOrClass, NodeId, TextSelectionStartEnd},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use azul_core::geom::LogicalSize;
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
    .chrome { display: block; user-select: none; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn para(class_name: &str, s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class(class_name))
        .with_child(text(s))
}

/// A flat editable: the host is its own text block.
fn editable(s: &str) -> Dom {
    para("p", s).with_contenteditable(true)
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

/// `body(0) > [div.p(1) > "one"(2), div.p(3) > "two"(4)]`
fn two_paragraphs() -> LayoutWindow {
    layout(
        Dom::create_body()
            .with_child(para("p", "one"))
            .with_child(para("p", "two")),
    )
}

/// `body(0) > div.p[contenteditable](1) > "hello"(2)`
fn one_field() -> LayoutWindow {
    layout(Dom::create_body().with_child(editable("hello")))
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

fn start() -> TextCursor {
    at(0, CursorAffinity::Leading)
}

fn open_session_in(lw: &mut LayoutWindow, n: usize) {
    assert!(
        lw.start_editing_at(start(), DomId::ROOT_ID, NodeId::new(n), 0),
        "premise: a session opens in node {n}'s block"
    );
}

fn select_by_a11y(lw: &mut LayoutWindow, n: usize, selection_start: usize, selection_end: usize) {
    let _ = lw.process_accessibility_action(
        DomId::ROOT_ID,
        NodeId::new(n),
        AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
            selection_start,
            selection_end,
        }),
        Instant::from(std::time::Instant::now()),
    );
}

fn editing_block(lw: &LayoutWindow) -> Option<TextBlock> {
    lw.text_edit_manager.get_editing_block()
}

fn session_key(lw: &LayoutWindow) -> Option<u64> {
    lw.text_edit_manager
        .multi_cursor
        .as_ref()
        .map(|mc| mc.contenteditable_key)
}

/// The key a click into the editable `host` keys its session on.
fn host_key(lw: &LayoutWindow, host: usize) -> u64 {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("layout result");
    azul_core::diff::calculate_contenteditable_key(
        lr.styled_dom.node_data.as_ref(),
        lr.styled_dom.node_hierarchy.as_ref(),
        NodeId::new(host),
    )
}

// ---------------------------------------------------------------------------
// The IME's byte offsets
// ---------------------------------------------------------------------------

#[test]
fn every_byte_offset_round_trips_through_the_ime_seam() {
    let mut lw = one_field();
    open_session_in(&mut lw, 1);

    for offset in 0..="hello".len() {
        assert!(
            lw.set_focused_selection_from_byte_range(offset, offset),
            "byte {offset} resolves to a caret"
        );
        assert_eq!(
            lw.focused_caret_byte_offset(),
            Some(offset),
            "the caret set at byte {offset} must read back as byte {offset}"
        );
    }
}

#[test]
fn byte_zero_is_before_the_first_character() {
    let mut lw = one_field();
    open_session_in(&mut lw, 1);

    let zero = lw
        .focused_rect_for_byte_offset(0)
        .expect("byte 0 has a caret rect");
    let one = lw
        .focused_rect_for_byte_offset(1)
        .expect("byte 1 has a caret rect");
    assert!(
        zero.origin.x < one.origin.x,
        "byte 0 is before \"h\", byte 1 after it: {zero:?} vs {one:?}"
    );
}

// ---------------------------------------------------------------------------
// A screen reader's SetTextSelection
// ---------------------------------------------------------------------------

#[test]
fn an_accessibility_selection_keeps_its_range() {
    let mut lw = one_field();
    open_session_in(&mut lw, 1);

    select_by_a11y(&mut lw, 1, 1, 4);

    assert_eq!(lw.focused_selection_byte_range(), Some((1, 4)));
}

#[test]
fn an_accessibility_selection_lands_in_the_paragraph_it_names() {
    let mut lw = two_paragraphs();
    open_session_in(&mut lw, 1);

    select_by_a11y(&mut lw, 3, 0, 3);

    assert_eq!(
        editing_block(&lw),
        Some(block_of(&lw, 3)),
        "the selection is in \"two\", not in the session that was open in \"one\""
    );
}

#[test]
fn an_accessibility_selection_opens_a_session() {
    let mut lw = two_paragraphs();

    select_by_a11y(&mut lw, 3, 0, 3);

    assert_eq!(editing_block(&lw), Some(block_of(&lw, 3)));
}

#[test]
fn an_accessibility_selection_skips_unselectable_text() {
    // `body(0) > [div.p(1) > "one"(2), div.chrome(3) > "label"(4)]`
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("p", "one"))
            .with_child(para("chrome", "label")),
    );
    open_session_in(&mut lw, 1);

    select_by_a11y(&mut lw, 3, 2, 2);

    assert_eq!(editing_block(&lw), Some(block_of(&lw, 1)));
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor(),
        Some(start()),
        "a selection in `user-select: none` text is refused, not moved into \"one\""
    );
}

// ---------------------------------------------------------------------------
// The app's AddCursor / AddSelectionRange
// ---------------------------------------------------------------------------

#[test]
fn an_app_cursor_in_another_paragraph_moves_the_session_there() {
    let mut lw = two_paragraphs();
    open_session_in(&mut lw, 1);

    assert!(lw.add_app_cursor(dnid(3), at(1, CursorAffinity::Leading)));

    assert_eq!(
        editing_block(&lw),
        Some(block_of(&lw, 3)),
        "a caret made for \"two\" must not be added to the session in \"one\""
    );
}

#[test]
fn an_app_range_in_another_paragraph_moves_the_session_there() {
    let mut lw = two_paragraphs();
    open_session_in(&mut lw, 1);

    let range = SelectionRange {
        start: start(),
        end: at(2, CursorAffinity::Trailing),
    };
    assert!(lw.add_app_selection_range(dnid(3), range));

    assert_eq!(editing_block(&lw), Some(block_of(&lw, 3)));
}

#[test]
fn an_app_cursor_naming_the_host_joins_its_session() {
    // `body(0) > div.p[contenteditable](1) > [div.p(2) > "one"(3),
    // div.p(4) > "two"(5)]`
    let mut lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_contenteditable(true)
                .with_child(para("p", "one"))
                .with_child(para("p", "two")),
        ),
    );
    open_session_in(&mut lw, 4);

    assert!(lw.add_app_cursor(dnid(1), at(2, CursorAffinity::Leading)));

    assert_eq!(editing_block(&lw), Some(block_of(&lw, 4)));
    assert_eq!(
        lw.text_edit_manager
            .multi_cursor
            .as_ref()
            .map(|mc| mc.selections.len()),
        Some(2),
        "a node that contains the session's block means that session"
    );
}

#[test]
fn sessions_opened_from_outside_carry_their_hosts_key() {
    let mut lw = one_field();
    let expected = host_key(&lw, 1);
    assert_ne!(expected, 0, "premise: a real editable has a real key");

    assert!(lw.add_app_cursor(dnid(1), start()));
    assert_eq!(session_key(&lw), Some(expected), "an app's AddCursor");

    lw.text_edit_manager.clear_editing();
    let range = SelectionRange {
        start: start(),
        end: at(2, CursorAffinity::Trailing),
    };
    assert!(lw.add_app_selection_range(dnid(1), range));
    assert_eq!(session_key(&lw), Some(expected), "an app's AddSelectionRange");

    lw.text_edit_manager.clear_editing();
    select_by_a11y(&mut lw, 1, 0, 2);
    assert_eq!(
        session_key(&lw),
        Some(expected),
        "a screen reader's SetTextSelection"
    );
}
