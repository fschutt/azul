//! An edit keeps the generated content of its paragraph.
//!
//! A `::before` and an `::after` are nodes of the paragraph that the layout
//! lays out with its text, in front of it and behind it; they are no text of
//! the document, so the edit model holds the text alone. A keystroke lays
//! the edited paragraph out again, and the generated text has to stand
//! where a fresh window puts it for the edited text, and the carets that
//! number the paragraph's runs behind the `::before` have to type into the
//! text they stand in.
//!
//! The page is `body > div[contenteditable] > div.p > text`, with
//! `.p::before` and `.p::after` rules.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::{items_of, same_layout},
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{text_of, PARAGRAPH},
};

/// The paragraph's generated content.
const CSS: &str = ".p::before { content: \"X\"; } .p::after { content: \"Y\"; }";

/// A window that has laid out the page with `text` in the paragraph once.
fn window(text: &str) -> LayoutWindow {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_div()
                .with_class("p".into())
                .with_css("width: 300px; font-size: 20px;")
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
        ),
    );
    let styled_dom = StyledDom::create(&mut dom, Css::from_string(CSS.into()));
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

/// Types `letter` at the caret `at`, numbered as the paragraph's layout
/// numbers its runs.
fn type_at(lw: &mut LayoutWindow, at: TextCursor, letter: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(PARAGRAPH))),
    }));
    lw.start_editing_at(at, DomId::ROOT_ID, NodeId::new(PARAGRAPH), 0);
    let _ = lw.record_text_input(letter);
    let _ = lw.apply_text_changeset();
}

/// The caret `at` bytes into the run `run`, leading or trailing.
const fn caret(run: u32, at: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: run,
            start_byte_in_run: at,
        },
        affinity,
    }
}

/// Asserts that the edited window `lw` lays its paragraph out as a fresh
/// window of `edited` does.
fn assert_laid_out_as_fresh(lw: &LayoutWindow, edited: &str) {
    let fresh = items_of(&window(edited));
    assert!(
        fresh.first().is_some_and(|item| item.1 == "X") && fresh.last().is_some_and(|i| i.1 == "Y"),
        "harness: the generated text is laid out around the paragraph's: {fresh:?}"
    );
    let got = items_of(lw);
    assert!(
        same_layout(&got, &fresh),
        "after an edit the paragraph's letters stand at {got:?}, a fresh layout of the same text \
         puts them at {fresh:?}: the edit laid the paragraph out without its generated content"
    );
}

#[test]
fn typing_in_front_of_the_text_keeps_the_before_and_after() {
    let mut lw = window("ab");
    // The start of the paragraph's text: run 0 is the `::before`'s.
    type_at(&mut lw, caret(1, 0, CursorAffinity::Leading), "b");
    assert_eq!(text_of(&lw), "bab", "the letter goes in front of the text");
    assert_laid_out_as_fresh(&lw, "bab");
}

#[test]
fn typing_at_the_end_of_the_text_goes_into_it() {
    let mut lw = window("ab");
    // After the text's last letter, in its run behind the `::before`'s.
    type_at(&mut lw, caret(1, 1, CursorAffinity::Trailing), "c");
    assert_eq!(
        text_of(&lw),
        "abc",
        "a caret behind the generated run types into the text it stands in"
    );
    assert_laid_out_as_fresh(&lw, "abc");
}
