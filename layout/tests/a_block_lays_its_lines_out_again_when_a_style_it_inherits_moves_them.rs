//! A box in block flow lays its lines out again when a style it inherits
//! breaks or places them differently.
//!
//! `word-break` and `text-align` are inherited, and an inline formatting
//! context reads them from its root when it breaks and places its lines. A
//! restyle of an ancestor that changes one keeps the root's box and its text:
//! their own data did not change, nor did the style the text is shaped with.
//! In block flow nothing else asks the root to lay out again - its size
//! cache answers for it at the same width, with the lines it kept. Each page
//! below is restyled back and forth, so the text has kept its shaping style
//! once before the restyle that matters.

use azul_core::dom::Dom;

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{
        assert_lays_out_like_a_fresh_window, node_with_class, painted_glyphs, rebuild, rect,
    },
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

/// One word, far wider than the 72 px boxes that hold it.
const WORD: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis";

/// The page: a 300 px column declaring `style`, holding a heading, what
/// `content` makes, and a box below it.
fn page(style: &str, content: fn() -> Dom) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css(&format!("width: 300px; {style}"))
                .with_child(Dom::create_div_with_text("Files"))
                .with_child(content())
                .with_child(Dom::create_div().with_css("height: 20px;")),
        ),
    )
}

/// A 72 px label holding the long word.
fn narrow_label() -> Dom {
    Dom::create_div_with_text(WORD)
        .with_class("label".into())
        .with_css("width: 72px;")
}

/// A label as wide as the column, holding a short word.
fn wide_label() -> Dom {
    Dom::create_div_with_text("Documents").with_class("label".into())
}

/// A 72 px box holding the long word and a block after it: the word sits in
/// an anonymous block of its own.
fn word_beside_a_block() -> Dom {
    Dom::create_div_with_text(WORD)
        .with_class("label".into())
        .with_css("width: 72px;")
        .with_child(Dom::create_div().with_css("height: 10px;"))
}

/// A line holding nothing but a 40 px inline-block.
fn icon_line() -> Dom {
    Dom::create_div().with_child(
        Dom::create_div()
            .with_class("icon".into())
            .with_css("display: inline-block; width: 40px; height: 10px;"),
    )
}

/// Restyles the column to `style` and back, twice; every page lays out as a
/// window opened on it does.
fn restyle_back_and_forth(content: fn() -> Dom, style: &str) {
    let mut lw = window(page("", content));
    for styled in [true, false, true, false] {
        let now = if styled { style } else { "" };
        rebuild(&mut lw, page(now, content));
        let what = if styled {
            format!("the column sets `{style}`")
        } else {
            format!("the column drops `{style}` again")
        };
        assert_lays_out_like_a_fresh_window(&lw, page(now, content), &what);
    }
}

/// The column starts breaking words and stops again: the label, laid out
/// again at its 72 px, wraps its word into several lines and back into one.
#[test]
fn a_label_re_wraps_when_its_column_starts_breaking_words() {
    let height = |style: &str| {
        let lw = window(page(style, narrow_label));
        rect(&lw, node_with_class(&lw, "label")).size.height
    };
    assert!(
        height("word-break: break-all;") > height("") + 1.0,
        "harness: word-break: break-all wraps the label's word ({} px high, {} px without)",
        height("word-break: break-all;"),
        height("")
    );
    restyle_back_and_forth(narrow_label, "word-break: break-all;");
}

/// The column centres its text and stops again: every line moves to the
/// middle of the column and back.
#[test]
fn a_label_moves_its_line_when_its_column_centres_it() {
    let leftmost_glyph = |style: &str| {
        painted_glyphs(&window(page(style, wide_label)))
            .iter()
            .map(|glyph| glyph.1)
            .fold(f32::MAX, f32::min)
    };
    assert!(
        leftmost_glyph("text-align: center;") > leftmost_glyph("") + 50.0,
        "harness: text-align: center moves the text right ({} px, {} px without)",
        leftmost_glyph("text-align: center;"),
        leftmost_glyph("")
    );
    restyle_back_and_forth(wide_label, "text-align: center;");
}

/// The same for a word that shares its box with a block: the anonymous block
/// around the word re-wraps it.
#[test]
fn a_word_beside_a_block_re_wraps_when_its_column_starts_breaking_words() {
    let height = |style: &str| {
        let lw = window(page(style, word_beside_a_block));
        rect(&lw, node_with_class(&lw, "label")).size.height
    };
    assert!(
        height("word-break: break-all;") > height("") + 1.0,
        "harness: word-break: break-all wraps the word ({} px high, {} px without)",
        height("word-break: break-all;"),
        height("")
    );
    restyle_back_and_forth(word_beside_a_block, "word-break: break-all;");
}

/// A line that holds no text at all is placed by the same styles: the
/// column centres it, and its inline-block moves to the middle and back.
#[test]
fn an_icon_alone_on_its_line_moves_when_its_column_centres_it() {
    let icon_x = |style: &str| {
        let lw = window(page(style, icon_line));
        rect(&lw, node_with_class(&lw, "icon")).origin.x
    };
    assert!(
        icon_x("text-align: center;") > icon_x("") + 50.0,
        "harness: text-align: center moves the icon right ({} px, {} px without)",
        icon_x("text-align: center;"),
        icon_x("")
    );
    restyle_back_and_forth(icon_line, "text-align: center;");
}
