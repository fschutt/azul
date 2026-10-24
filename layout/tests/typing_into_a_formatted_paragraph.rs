//! Typing and Backspace after a keyboard focus land in a paragraph whose last
//! text sits inside an inline element.
//!
//! `host[contenteditable] > p > ["Hello ", <b>"world"</b>]`. Tab into the host:
//! the focus path puts the caret at the end of the paragraph - the layout's
//! last cluster, in run 1 of the paragraph's inline content ("world"). The
//! edit is then keyed to the node that owns that content. That used to be
//! found by walking up from the node the session was opened on - the LEAF
//! "world" - to the nearest element with any layout box: the `<b>`, which has
//! a box of its own but owns no inline layout. Its content is ONE run, the
//! caret says run 1, and the keystroke went nowhere (a debug build trips the
//! "insert missed every selection" assertion instead).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
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

/// `body(0) > div.host[contenteditable](1) > div.p(2) > [text(3) "Hello ",
/// b(4) > text(5) "world"]`
const HOST: usize = 1;
const P: usize = 2;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn editor() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(text("Hello "))
                    .with_child(Dom::create_b().with_child(text("world"))),
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

fn text_of(lw: &LayoutWindow, n: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(n));
    lw.extract_text_from_inline_content(&content)
}

/// Tab into the editor: the focus path, then the post-layout finalize that
/// seeds the caret at the end of the text.
fn tab_into_the_editor(lw: &mut LayoutWindow) {
    let ws = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(HOST)), &ws);
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    assert!(
        lw.finalize_pending_focus_changes(),
        "premise: the focus opened an editing session"
    );
    let caret = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("premise: the session has a caret");
    assert_eq!(
        caret.cluster_id.source_run, 1,
        "premise: the caret is at the end of the paragraph, in its second run (\"world\")"
    );
    assert_eq!(text_of(lw, P), "Hello world", "premise: the paragraph's text");
}

#[test]
fn typing_after_tab_appends_to_the_formatted_word() {
    let mut lw = editor();
    tab_into_the_editor(&mut lw);

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, P), "Hello worldx");
}

#[test]
fn backspace_after_tab_deletes_the_last_character() {
    let mut lw = editor();
    tab_into_the_editor(&mut lw);

    let _ = lw.delete_selection(dnid(HOST), false);

    assert_eq!(text_of(&lw, P), "Hello worl");
}
