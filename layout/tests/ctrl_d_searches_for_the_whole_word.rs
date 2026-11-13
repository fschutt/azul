//! Ctrl+D selects the word at the caret, then its next occurrence - the
//! WHOLE word.
//!
//! The word's range ends `Trailing` on its last grapheme, as every word range
//! does. The search text was cut at the raw `start_byte_in_run` of that end -
//! the START of the last grapheme - so for "foo" it searched "fo", and "fox"
//! was the next occurrence. The occurrence it added ended `Trailing` on the
//! byte AFTER the match, one grapheme past it. And a caret at the start of the
//! word searched from there, found the word itself, and added nothing.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, Selection, TextCursor},
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

/// `body(0) > div.host[contenteditable](1) > div.p(2) > text(3)`
const P: usize = 2;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn paragraph(s: &str) -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(s)),
            ),
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
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn caret_at(lw: &mut LayoutWindow, byte: u32) {
    let cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    };
    assert!(
        lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(P), 0),
        "premise: a session opens in the paragraph"
    );
}

/// Every local selection of the session, as byte ranges of the paragraph's
/// text, in position order.
fn selected_bytes(lw: &LayoutWindow) -> Vec<(usize, usize)> {
    let block = lw
        .text_block_of(dnid(P))
        .expect("the paragraph is a text block");
    let content = lw.block_content(block);
    let mc = lw
        .text_edit_manager
        .multi_cursor
        .as_ref()
        .expect("a session is open");
    mc.local_selections()
        .map(|s| match s.selection {
            Selection::Cursor(c) => {
                let at = content.flat_byte_of(&c).0;
                (at, at)
            }
            Selection::Range(r) => {
                let (a, b) = (
                    content.flat_byte_of(&r.start).0,
                    content.flat_byte_of(&r.end).0,
                );
                (a.min(b), a.max(b))
            }
        })
        .collect()
}

#[test]
fn ctrl_d_selects_the_next_whole_word_not_a_word_that_starts_like_it() {
    let mut lw = paragraph("foo fox foo bar");
    caret_at(&mut lw, 1);

    assert!(lw.select_next_occurrence());

    assert_eq!(
        selected_bytes(&lw),
        vec![(0, 3), (8, 11)],
        "\"foo\" and the next \"foo\" - not \"fo\" of \"fox\", and not the space after it"
    );
}

#[test]
fn ctrl_d_at_the_start_of_a_word_selects_its_next_occurrence() {
    let mut lw = paragraph("foo bar foo");
    caret_at(&mut lw, 0);

    assert!(lw.select_next_occurrence());

    assert_eq!(
        selected_bytes(&lw),
        vec![(0, 3), (8, 11)],
        "the search starts after the word, so it cannot find the word itself"
    );
}

#[test]
fn a_second_ctrl_d_selects_the_occurrence_after_the_last() {
    let mut lw = paragraph("foo fox foo foo!");
    caret_at(&mut lw, 1);

    assert!(lw.select_next_occurrence());
    assert!(lw.select_next_occurrence());

    assert_eq!(
        selected_bytes(&lw),
        vec![(0, 3), (8, 11), (12, 15)],
        "each occurrence is the word alone - the third does not take the '!'"
    );
}
