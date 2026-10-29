//! An edit before an inline-block moves the inline-block where a fresh
//! layout of the edited text puts it.
//!
//! An inline-block is an atomic inline: the paragraph's lines carry it as
//! one box, and once they are broken it stands where its line puts it. A
//! keystroke lays the edited paragraph out again, and the inline-block has
//! to move with the text in front of it, to where a fresh window puts it
//! for the edited text, and the letters after it with it.
//!
//! The page is `body > div[contenteditable] > div > (text, inline-block,
//! text)`; the typed letter goes in front of the first text.

use azul_core::dom::{Dom, DomId, NodeId};
use azul_layout::window::LayoutWindow;

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::{items_of, same_layout},
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{
        text_of, type_in_front, window, PARAGRAPH,
    },
};

/// The inline-block's element: body(0) > div(1) > div(2) > text(3), div(4).
const INLINE_BLOCK: usize = 4;

/// The page with `before`, a 30 x 10 px inline-block and `after` in one
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
                    Dom::create_div()
                        .with_css("display: inline-block; width: 30px; height: 10px;"),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    after,
                )),
        ),
    )
}

/// Where the inline-block stands.
fn inline_block_at(lw: &LayoutWindow) -> (f32, f32) {
    let result = &lw.layout_results[&DomId::ROOT_ID];
    let index = result.layout_tree.dom_to_layout[&NodeId::new(INLINE_BLOCK)][0].index();
    let position = result.calculated_positions[index];
    (position.x, position.y)
}

#[test]
fn typing_before_an_inline_block_moves_it_as_a_fresh_layout_does() {
    let before = window(page("ab", "cd"));
    let fresh = window(page("bab", "cd"));
    assert!(
        (inline_block_at(&before).0 - inline_block_at(&fresh).0).abs() > 1.0,
        "harness: a letter in front of the inline-block moves it: {:?} and {:?}",
        inline_block_at(&before),
        inline_block_at(&fresh)
    );

    let mut lw = window(page("ab", "cd"));
    type_in_front(&mut lw, "b");
    assert_eq!(text_of(&lw), "babcd", "harness: the letter is typed in");
    let (got, expected) = (inline_block_at(&lw), inline_block_at(&fresh));
    assert!(
        (got.0 - expected.0).abs() < 0.01 && (got.1 - expected.1).abs() < 0.01,
        "after an edit in front of it the inline-block stands at {got:?}, where a fresh layout \
         of the same text puts it at {expected:?}: the edit laid the paragraph out without \
         placing its inline-block"
    );
    let (got, expected) = (items_of(&lw), items_of(&fresh));
    assert!(
        same_layout(&got, &expected),
        "after the edit the paragraph's letters stand at {got:?}, a fresh layout puts them at \
         {expected:?}: the edit laid the paragraph out without its inline-block"
    );
}
