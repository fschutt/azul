//! An edit re-wraps its paragraph as a fresh layout of the edited text does.
//!
//! A keystroke lays the edited paragraph out again from the constraints its
//! last full layout kept, `word-break`, `hyphens` and `line-break` among
//! them. With `word-break: break-all` the paragraph breaks between any two
//! letters, with `hyphens: none` not at its soft hyphen, and the edited
//! paragraph has to keep doing so: its lines are the lines a fresh window
//! lays the edited text out in.
//!
//! The page is `body > div[contenteditable] > div > text`, the shape the
//! text widgets use; the typed letter goes in front of the text.

use azul_core::{
    dom::{AttributeType, Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The paragraph's element: body(0) > div[contenteditable](1) > div(2).
pub(crate) const PARAGRAPH: usize = 2;

/// The page with `text` in a paragraph styled `css`.
pub(crate) fn page(text: &str, css: &str) -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_div()
                .with_css(css)
                .with_attribute(AttributeType::Lang("en".into()))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
        ),
    )
}

/// A window that has laid out `dom` once.
pub(crate) fn window(mut dom: Dom) -> LayoutWindow {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// Types `letter` in front of the paragraph's text.
pub(crate) fn type_in_front(lw: &mut LayoutWindow, letter: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(PARAGRAPH))),
    }));
    let at_start = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };
    lw.start_editing_at(at_start, DomId::ROOT_ID, NodeId::new(PARAGRAPH), 0);
    let _ = lw.record_text_input(letter);
    let _ = lw.apply_text_changeset();
}

/// The paragraph's text, as the edit model reads it.
pub(crate) fn text_of(lw: &LayoutWindow) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(PARAGRAPH));
    lw.extract_text_from_inline_content(&content)
}

/// The text of each line of the paragraph's inline layout.
fn lines_of(lw: &LayoutWindow) -> Vec<String> {
    let tree = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
    let layout = (0..tree.nodes.len())
        .filter(|&index| tree.nodes[index].dom_node_id == Some(NodeId::new(PARAGRAPH)))
        .find_map(|index| tree.materialized_inline_layout_for_node(index))
        .expect("the paragraph has an inline layout");
    let mut lines: Vec<String> = Vec::new();
    for item in &layout.items {
        while lines.len() <= item.line_index {
            lines.push(String::new());
        }
        if let Some(cluster) = item.item.as_cluster() {
            lines[item.line_index].push_str(cluster.text());
        }
    }
    lines
}

/// Asserts that typing a letter in front of `text` in a paragraph styled
/// `css` gives the lines a fresh layout of the edited text has.
fn assert_an_edit_wraps_as_a_fresh_layout(text: &str, css: &str) {
    let edited = format!("b{text}");
    let mut lw = window(page(text, css));
    type_in_front(&mut lw, "b");
    assert_eq!(text_of(&lw), edited, "harness: the letter is typed in");

    let fresh = window(page(&edited, css));
    let expected = lines_of(&fresh);
    let got = lines_of(&lw);
    assert_eq!(
        got, expected,
        "{css}: after an edit the paragraph has the lines {got:?}, a fresh layout of the same \
         text {expected:?}: the edit broke it without its paragraph's line breaking style"
    );
}

#[test]
fn an_edit_in_a_break_all_paragraph_breaks_between_any_letters() {
    let css = "width: 60px; font-size: 20px; word-break: break-all; hyphens: auto;";
    let text = "aaaaaaaaaaaaaaaa";
    let fresh = window(page(&format!("b{text}"), css));
    assert!(
        lines_of(&fresh).len() > 1,
        "harness: break-all wraps the long word in its 60px box: {:?}",
        lines_of(&fresh)
    );
    assert_an_edit_wraps_as_a_fresh_layout(text, css);
}

#[test]
fn an_edit_in_a_paragraph_with_hyphens_none_does_not_break_at_its_soft_hyphen() {
    let css = "width: 60px; font-size: 20px; hyphens: none;";
    let text = "aa\u{AD}aaaaaaaaaaaa";
    let fresh = window(page(&format!("b{text}"), css));
    assert_eq!(
        lines_of(&fresh).len(),
        1,
        "harness: with hyphens: none the word stays whole: {:?}",
        lines_of(&fresh)
    );
    assert_an_edit_wraps_as_a_fresh_layout(text, css);
}
