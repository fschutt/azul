//! An edit keeps an empty inline box with edges in its paragraph.
//!
//! An empty `span` with padding and borders holds no text, but the layout
//! lays it out as a box of its edges' width in its line, and numbers it as
//! an item of the paragraph's content. A keystroke lays the edited
//! paragraph out again, and the box has to stand where a fresh window puts
//! it for the edited text, and the carets behind it have to type into the
//! text they stand in.
//!
//! The page is `body > div[contenteditable] > div > (text, span, text)`.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::window::LayoutWindow;

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::{items_of, same_layout},
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{text_of, window, PARAGRAPH},
};

/// The page with `before`, an empty span with edges and `after` in one
/// paragraph.
fn page(before: &str, after: &str) -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_div()
                .with_css("width: 300px; font-size: 20px;")
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    before,
                ))
                .with_child(
                    Dom::create_span()
                        .with_css("padding: 0px 15px; border: 3px solid black;"),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    after,
                )),
        ),
    )
}

/// Types `letter` at the start of run `run` of the paragraph, numbered as
/// its layout numbers its runs.
fn type_at_run(lw: &mut LayoutWindow, run: u32, letter: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(PARAGRAPH))),
    }));
    let at = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: run,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };
    lw.start_editing_at(at, DomId::ROOT_ID, NodeId::new(PARAGRAPH), 0);
    let _ = lw.record_text_input(letter);
    let _ = lw.apply_text_changeset();
}

/// Asserts that the edited window `lw` lays its paragraph out as a fresh
/// window of `before` and `after` around the box does.
fn assert_laid_out_as_fresh(lw: &LayoutWindow, before: &str, after: &str) {
    let fresh = items_of(&window(page(before, after)));
    let without = items_of(&window(page(&format!("{before}{after}"), "")));
    assert!(
        !same_layout(&fresh, &without),
        "harness: the box takes room in the line: {fresh:?}"
    );
    let got = items_of(lw);
    assert!(
        same_layout(&got, &fresh),
        "after an edit the paragraph's letters stand at {got:?}, a fresh layout of the same \
         text puts them at {fresh:?}: the edit laid the paragraph out without its empty box"
    );
}

#[test]
fn typing_in_front_of_the_text_keeps_the_empty_box() {
    let mut lw = window(page("ab", "cd"));
    type_at_run(&mut lw, 0, "z");
    assert_eq!(text_of(&lw), "zabcd", "the letter goes in front of the text");
    assert_laid_out_as_fresh(&lw, "zab", "cd");
}

#[test]
fn typing_behind_the_empty_box_goes_into_the_text_behind_it() {
    let mut lw = window(page("ab", "cd"));
    // "ab", the box, then "cd": the text behind the box is run 2.
    type_at_run(&mut lw, 2, "z");
    assert_eq!(text_of(&lw), "abzcd", "the letter goes into the text behind the box");
    assert_laid_out_as_fresh(&lw, "ab", "zcd");
}
