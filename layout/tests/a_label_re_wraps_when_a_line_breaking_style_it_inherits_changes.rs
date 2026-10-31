//! A label re-wraps when a line-breaking style it inherits changes above it.
//!
//! `word-break`, `overflow-wrap`, `line-break`, `hyphens` and `text-justify`
//! are inherited, and the inline formatting context reads them from its root
//! when it breaks its lines. A restyle of an ancestor that changes one keeps
//! the label's box and its text (their own data did not change, nor did the
//! style the text is shaped with), and lays the label out again at the same
//! width. Its kept lines may answer that layout only if they were broken under
//! the same styles.

use azul_core::dom::Dom;

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{
        assert_lays_out_like_a_fresh_window, node_with_class, rebuild, rect,
    },
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

/// A row holding a 72 px wide label whose one word is far wider, and a box
/// filling the rest. `break_all`: the row breaks words anywhere, and the
/// label inherits it.
fn page(break_all: bool) -> Dom {
    let row = if break_all {
        "display: flex; flex-direction: row; word-break: break-all;"
    } else {
        "display: flex; flex-direction: row;"
    };
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
            .with_child(
                Dom::create_div()
                    .with_css(row)
                    .with_child(
                        Dom::create_div_with_text("Pneumonoultramicroscopicsilicovolcanoconiosis")
                            .with_class("label".into())
                            .with_css("width: 72px; flex-shrink: 0;"),
                    )
                    .with_child(Dom::create_div().with_css("flex-grow: 1; height: 20px;")),
            ),
    )
}

/// The row starts breaking words, and stops again: the label, laid out again
/// at its 72 px, wraps its word into several lines and back into one, as a
/// window opened on either page lays it out.
#[test]
fn a_label_in_a_row_re_wraps_when_the_row_starts_breaking_words() {
    let height = |break_all: bool| {
        let lw = window(page(break_all));
        rect(&lw, node_with_class(&lw, "label")).size.height
    };
    assert!(
        height(true) > height(false) + 1.0,
        "harness: word-break: break-all wraps the label's word ({} px high, {} px without)",
        height(true),
        height(false)
    );

    let mut lw = window(page(false));
    rebuild(&mut lw, page(true));
    assert_lays_out_like_a_fresh_window(&lw, page(true), "the row breaks words");
    rebuild(&mut lw, page(false));
    assert_lays_out_like_a_fresh_window(&lw, page(false), "the row keeps words whole again");
}
