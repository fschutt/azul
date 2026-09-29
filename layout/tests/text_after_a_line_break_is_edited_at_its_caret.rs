//! The text after a `<br>` is edited, split and read at the caret that
//! stands in it.
//!
//! The layout lays out `p > ["one", <br>, "two"]` as three runs - "one", a
//! hard `LineBreak`, "two" - and every caret in "two" names run 2. The edit
//! model (`get_text_before_textinput`) had no item for the `<br>`: "two" was
//! its run 1. A keystroke in "two" spliced into a run that did not exist,
//! Enter mapped the caret's bytes onto the wrong child of the paragraph, and
//! the IME's text lost the line break and put the caret at the end.
//!
//! Guards (green before and after): a paragraph that is only a `<br>` - the
//! empty paragraph of a rich-text editor - takes the first keystroke, and a
//! trailing `<br>` does not stop Delete at the end of its paragraph from
//! joining the next.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::DefaultAction,
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, VirtualKeyCode},
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
    .p { display: block; }
"#;

/// `body(0) > div.host[contenteditable](1) > div.p(2) > ["one"(3), br(4),
/// "two"(5)]` (+ `div.p(6) > "next"(7)` in [`with_next`]).
const HOST: usize = 1;
const P: usize = 2;
const TWO: usize = 5;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn para(children: Vec<Dom>) -> Dom {
    children.into_iter().fold(
        Dom::create_div().with_ids_and_classes(class("p")),
        Dom::with_child,
    )
}

fn layout(paragraphs: Vec<Dom>) -> LayoutWindow {
    let host = paragraphs.into_iter().fold(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true),
        Dom::with_child,
    );
    let mut dom = Dom::create_body().with_child(host);
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

fn one_br_two() -> LayoutWindow {
    layout(vec![para(vec![text("one"), Dom::create_br(), text("two")])])
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

/// A session with the caret at byte `byte` of "two", in the layout's own
/// numbering of the paragraph's runs.
fn caret_in_two(lw: &mut LayoutWindow, byte: u32) {
    let block = block_of(lw, P);
    let cursor = lw
        .caret_at_node_byte(block, NodeId::new(TWO), byte)
        .expect("premise: \"two\" is laid out in the paragraph");
    assert_eq!(
        cursor.cluster_id.source_run, 2,
        "premise: the layout numbers \"one\", the <br>, \"two\" as runs 0, 1, 2"
    );
    assert!(
        lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(P), 0),
        "premise: a session opens in the paragraph"
    );
}

#[test]
fn typing_after_a_line_break_lands_in_the_text_after_it() {
    let mut lw = one_br_two();
    caret_in_two(&mut lw, 1);

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, P), "one\ntxwo");
}

#[test]
fn enter_after_a_line_break_splits_the_text_after_it_at_the_caret() {
    let mut lw = one_br_two();
    caret_in_two(&mut lw, 1);

    lw.record_structural_default_action(&DefaultAction::SplitBlockAtCursor { target: dnid(HOST) })
        .expect("a split is recorded");

    let pending = lw
        .pending_document_edit
        .as_ref()
        .expect("the recorded edit awaits the app");
    match &pending.operation {
        DocumentOperation::SplitNode(split) => {
            assert_eq!(split.node, dnid(P), "the paragraph is what splits");
            assert_eq!(
                split.at,
                NodePosition::in_text_child(2, 1),
                "\"t|wo\" is byte 1 of the paragraph's THIRD child - the <br> is its second"
            );
        }
        other => panic!("expected a split, got {other:?}"),
    }
}

#[test]
fn the_ime_reads_a_line_break_and_the_caret_after_it() {
    let mut lw = one_br_two();
    caret_in_two(&mut lw, 1);

    assert_eq!(
        lw.ime_surrounding_text(),
        Some(("one\ntwo".to_string(), 5)),
        "the IME's text has the line break, and \"t|wo\" is byte 5 of it"
    );
    assert_eq!(lw.focused_caret_byte_offset(), Some(5));
}

/// Guard: the empty paragraph of a rich-text editor is a `<br>` alone. Its
/// one caret types the first character.
#[test]
fn a_paragraph_that_is_only_a_line_break_takes_the_first_keystroke() {
    let mut lw = layout(vec![para(vec![Dom::create_br()])]);
    let block = block_of(&lw, P);
    let caret = lw
        .text_target(block)
        .and_then(|t| t.first_caret())
        .expect("premise: the empty paragraph has a caret");
    lw.start_editing_at(caret, DomId::ROOT_ID, NodeId::new(P), 0);

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    assert!(
        text_of(&lw, P).starts_with('x'),
        "the keystroke landed: {:?}",
        text_of(&lw, P)
    );
}

/// Guard: a `<br>` that ends its paragraph starts no line of its own, so a
/// caret after the paragraph's last letter is at its end - Delete joins the
/// next paragraph onto it.
#[test]
fn delete_before_a_trailing_line_break_joins_the_next_paragraph() {
    let mut lw = layout(vec![
        para(vec![text("one"), Dom::create_br()]),
        para(vec![text("next")]),
    ]);
    let end = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 2,
        },
        affinity: CursorAffinity::Trailing,
    };
    lw.start_editing_at(end, DomId::ROOT_ID, NodeId::new(P), 0);

    let focused = Some(dnid(HOST));
    let editing = lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    let keys = KeyboardState {
        current_virtual_keycode: Some(VirtualKeyCode::Delete).into(),
        pressed_virtual_keycodes: vec![VirtualKeyCode::Delete].into(),
        ..Default::default()
    };
    let action = azul_layout::default_actions::determine_keyboard_default_action_with_editing(
        &keys,
        focused,
        &lw.layout_results,
        false,
        Some(&editing),
    )
    .action;
    assert!(
        matches!(action, DefaultAction::MergeWithNext { .. }),
        "\"one|\" before a trailing <br> is the paragraph's end: {action:?}"
    );
}
