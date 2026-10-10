//! In `white-space: normal` text, a caret after a collapsed run of spaces is
//! typed into and read where it stands.
//!
//! The layout collapses "a   b c" to "a b c" before it shapes it, so every
//! caret's byte counts the COLLAPSED text: after the 'c' is `Trailing` on
//! byte 4. The edit model kept the raw text, where byte 4 is the 'b': a
//! keystroke after the 'c' went in after the 'b', and the IME read the
//! caret there.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, TextCursor},
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
    .p { display: block; white-space: normal; }
"#;

/// `body(0) > div.host[contenteditable](1) > div.p(2) > "a   b c"(3)`
const HOST: usize = 1;
const P: usize = 2;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn editor() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "a   b c",
                    )),
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
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// What `white-space: normal` shows of `s`: every run of spaces as one.
fn shown(s: &str) -> String {
    s.split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The caret after the 'c' - the layout's own end-of-text caret.
fn caret_after_c(lw: &mut LayoutWindow) -> TextCursor {
    let block = lw
        .text_block_of(dnid(P))
        .expect("the paragraph is a text block");
    let end = lw
        .text_target(block)
        .and_then(|t| t.last_caret())
        .expect("premise: the paragraph has a last caret");
    assert_eq!(
        (end.cluster_id.start_byte_in_run, end.affinity),
        (4, CursorAffinity::Trailing),
        "premise: the layout shapes the collapsed \"a b c\", whose 'c' is byte 4"
    );
    assert!(lw.start_editing_at(end, DomId::ROOT_ID, NodeId::new(P), 0));
    end
}

#[test]
fn typing_after_collapsed_spaces_lands_at_the_caret() {
    let mut lw = editor();
    caret_after_c(&mut lw);

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(P));
    assert_eq!(
        shown(&lw.extract_text_from_inline_content(&content)),
        "a b cx",
        "the 'x' goes in after the 'c', not after the 'b'"
    );
}

#[test]
fn the_ime_reads_a_caret_after_collapsed_spaces_where_it_stands() {
    let mut lw = editor();
    caret_after_c(&mut lw);

    let (text, at) = lw.ime_surrounding_text().expect("an editable has focus");
    assert_eq!(
        (shown(&text[..at]), shown(&text[at..])),
        ("a b c".to_string(), String::new()),
        "the caret is after the 'c' in {text:?}, at {at}"
    );
}
