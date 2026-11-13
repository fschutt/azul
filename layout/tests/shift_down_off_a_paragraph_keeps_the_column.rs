//! Shift+Down off a paragraph's last line lands in the next paragraph's first
//! line at the same column; Shift+Up off its first line, in the previous
//! paragraph's last line at the same column - as it does between two lines
//! of one paragraph.
//!
//! The step off the edge of a block landed on the neighbour's first caret
//! (Down) or its last caret (Up), wherever the focus stood: from "abc|def",
//! Shift+Down selected "def" and the line break, not "def" and "abc".

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::LogicalSize,
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

/// `body(0) > div.host[contenteditable](1) > [div.p(2) > "abcdef"(3),
/// div.p(4) > "abcdef"(5)]`
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

/// Two one-line paragraphs with the same text, so a column in one is the
/// same byte in the other.
fn two_paragraphs() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(para("abcdef"))
            .with_child(para("abcdef")),
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

fn caret_in(lw: &mut LayoutWindow, n: usize, byte: u32) {
    let cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    };
    assert!(
        lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(n), 0),
        "premise: a session opens in node {n}'s paragraph"
    );
}

fn shift(lw: &mut LayoutWindow, direction: SelectionDirection) {
    let _ = lw.apply_selection_op(
        dnid(HOST),
        &SelectionOp::new(direction, SelectionStep::VisualLine, SelectionMode::Extend),
    );
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

#[test]
fn shift_down_off_a_paragraphs_last_line_keeps_the_column() {
    let mut lw = two_paragraphs();
    caret_in(&mut lw, P1, 3);

    shift(&mut lw, SelectionDirection::Forward);

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_some(),
        "premise: the selection runs into the next paragraph"
    );
    assert_eq!(
        copied(&lw).as_deref(),
        Some("def\nabc"),
        "from \"abc|def\" down to \"abc|def\" below"
    );
}

#[test]
fn shift_up_off_a_paragraphs_first_line_keeps_the_column() {
    let mut lw = two_paragraphs();
    caret_in(&mut lw, P2, 3);

    shift(&mut lw, SelectionDirection::Backward);

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_some(),
        "premise: the selection runs into the previous paragraph"
    );
    assert_eq!(
        copied(&lw).as_deref(),
        Some("def\nabc"),
        "from \"abc|def\" up to \"abc|def\" above"
    );
}
