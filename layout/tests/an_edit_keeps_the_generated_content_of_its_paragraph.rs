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
//! `.p::before` and `.p::after` rules, or the paragraph holds a `span.s`
//! between two texts, with `.s::before` and `.s::after` rules: a span's
//! generated content stands inside the paragraph's text, around the span's.

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

/// The generated content of the paragraph and of the span.
const CSS: &str = ".p::before { content: \"X\"; } .p::after { content: \"Y\"; } \
                   .s::before { content: \"[\"; } .s::after { content: \"]\"; }";

/// A window that has laid out the page with `text` in the paragraph once.
fn window(text: &str) -> LayoutWindow {
    window_of(
        Dom::create_div()
            .with_class("p".into())
            .with_css("width: 300px; font-size: 20px;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    )
}

/// A window that has laid out the page with `before`, a `span.s` holding
/// `inside` and `after` in the paragraph once.
fn span_window(before: &str, inside: &str, after: &str) -> LayoutWindow {
    window_of(
        Dom::create_div()
            .with_css("width: 300px; font-size: 20px;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(before))
            .with_child(
                Dom::create_span()
                    .with_class("s".into())
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(inside)),
            )
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(after)),
    )
}

/// A window that has laid out `paragraph` in the editable page once.
fn window_of(paragraph: Dom) -> LayoutWindow {
    let mut dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_div().with_contenteditable(true).with_child(paragraph));
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
    assert_laid_out_as(lw, &window(edited));
}

/// Asserts that the edited window `lw` lays its paragraph out as the fresh
/// window `fresh` does, and that both lay generated content out.
fn assert_laid_out_as(lw: &LayoutWindow, fresh: &LayoutWindow) {
    let fresh = items_of(fresh);
    assert!(
        fresh.iter().any(|item| ["X", "["].contains(&item.1.as_str()))
            && fresh.iter().any(|item| ["Y", "]"].contains(&item.1.as_str())),
        "harness: the generated text is laid out with the paragraph's: {fresh:?}"
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

#[test]
fn typing_in_front_of_a_span_keeps_its_before_and_after() {
    let mut lw = span_window("ab", "cd", "ef");
    // The paragraph has no generated content of its own: run 0 is "ab".
    type_at(&mut lw, caret(0, 0, CursorAffinity::Leading), "z");
    assert_eq!(text_of(&lw), "zabcdef", "the letter goes in front of the text");
    assert_laid_out_as(&lw, &span_window("zab", "cd", "ef"));
}

#[test]
fn typing_behind_a_span_goes_into_the_text_behind_it() {
    let mut lw = span_window("ab", "cd", "ef");
    // "ab", the span's "[", "cd", "]", then "ef": the text behind the span
    // is run 4 to every caret.
    type_at(&mut lw, caret(4, 0, CursorAffinity::Leading), "z");
    assert_eq!(
        text_of(&lw),
        "abcdzef",
        "a caret behind the span's generated runs types into the text it stands in"
    );
    assert_laid_out_as(&lw, &span_window("ab", "cd", "zef"));
}

#[test]
fn typing_into_a_paragraph_holding_only_generated_content_keeps_the_letter() {
    // No text node: the first keystroke seeds the paragraph's first run.
    let mut lw = window_of(
        Dom::create_div()
            .with_class("p".into())
            .with_css("width: 300px; font-size: 20px;"),
    );
    type_at(&mut lw, caret(0, 0, CursorAffinity::Leading), "z");
    assert_eq!(text_of(&lw), "z", "the letter is typed in, not into the ::before");
    assert_laid_out_as_fresh(&lw, "z");
}
