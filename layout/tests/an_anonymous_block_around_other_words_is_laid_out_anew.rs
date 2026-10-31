//! An anonymous block that holds other words after a rebuild is laid out
//! anew.
//!
//! A box holding words and a block puts each run of words in an anonymous
//! block. A rebuild matches the runs with the old ones by their place in the
//! box and keeps a run's block when it holds the same words. A run that
//! holds other words is a new block. Its words can all be kept nodes - a
//! span after them was removed, a block was put between them - so nothing
//! inside it is dirty, and only the box around it is laid out again. The new
//! block must then be laid out with what it holds now, not answered with the
//! size of the run that had its place before.

use azul_core::dom::Dom;

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{
        assert_lays_out_like_a_fresh_window, node_with_class, rebuild, rect,
    },
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

/// A 60 px box of 30 px lines.
fn narrow_box() -> Dom {
    Dom::create_div()
        .with_class("words".into())
        .with_css("width: 60px; font-size: 20px; line-height: 30px;")
}

/// The block every box ends with: its words are in an anonymous block.
fn block_below() -> Dom {
    Dom::create_div()
        .with_class("below".into())
        .with_css("height: 10px;")
}

/// "Alpha" and " Beta Gamma" in two spans, then the block: three lines of
/// words.
fn all_words() -> Dom {
    narrow_box()
        .with_child(Dom::create_span_with_text("Alpha"))
        .with_child(Dom::create_span_with_text(" Beta Gamma"))
        .with_child(block_below())
}

/// The first span alone, then the block: one line of words.
fn first_word() -> Dom {
    narrow_box()
        .with_child(Dom::create_span_with_text("Alpha"))
        .with_child(block_below())
}

/// A block between the two spans: two runs of words.
fn a_block_between_the_words() -> Dom {
    narrow_box()
        .with_child(Dom::create_span_with_text("Alpha"))
        .with_child(Dom::create_div().with_css("height: 10px;"))
        .with_child(Dom::create_span_with_text(" Beta Gamma"))
        .with_child(block_below())
}

/// The page: a column holding a heading and the box `content` makes.
fn page(content: fn() -> Dom) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("width: 300px;")
                .with_child(Dom::create_div_with_text("Files"))
                .with_child(content()),
        ),
    )
}

/// Where the block below the words starts, in a window opened on the page.
fn below_the_words(content: fn() -> Dom) -> f32 {
    let lw = window(page(content));
    rect(&lw, node_with_class(&lw, "below")).origin.y
}

/// Rebuilds the page from `first` to `then` and back, twice; every page
/// lays out as a window opened on it does.
fn rebuild_back_and_forth(first: fn() -> Dom, then: fn() -> Dom) {
    let mut lw = window(page(first));
    for (content, what) in [
        (then, "the box changed its words"),
        (first, "the box has its old words back"),
        (then, "the box changed its words again"),
        (first, "the box has its old words back again"),
    ] {
        rebuild(&mut lw, page(content));
        assert_lays_out_like_a_fresh_window(&lw, page(content), what);
    }
}

/// The span after the first word is removed: the anonymous block holds one
/// line, and the block below moves up to it.
#[test]
fn the_words_left_after_a_span_is_removed_are_laid_out_anew() {
    assert!(
        below_the_words(all_words) > below_the_words(first_word) + 25.0,
        "harness: three lines of words push the block down further than one ({} px, {} px)",
        below_the_words(all_words),
        below_the_words(first_word)
    );
    rebuild_back_and_forth(all_words, first_word);
}

/// A block is put between the words and taken out again: the words before
/// it are a run of their own, then one run with the words after it.
#[test]
fn the_words_before_a_block_put_between_them_are_laid_out_anew() {
    rebuild_back_and_forth(all_words, a_block_between_the_words);
}
