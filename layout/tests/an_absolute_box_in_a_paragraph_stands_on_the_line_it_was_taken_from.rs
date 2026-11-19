//! An out-of-flow box inside a paragraph has its static position on the
//! line it was taken out of.
//!
//! CSS 2.1 10.3.7 (`left`) and 10.6.4 (`top`): when the insets of an
//! absolutely positioned box are `auto` it stands at its static position,
//! the place of "a hypothetical box that would have been the first box of
//! the element" had it been `position: static`. For a child of a block
//! formatting context the engine recorded that place; for a child of an
//! INLINE formatting context it recorded nothing, and the box stood at the
//! paragraph's content-box origin whatever line it was written in.
//!
//! The lines are made of fixed-size inline blocks, one a line (a `<br/>`
//! after each), so every number is independent of the fonts.

use crate::table_markup::{laid_out, near, rect};

/// A 60 x 10 inline block `id`.
fn word(id: &str) -> String {
    format!("<i id=\"{id}\" style=\"display: inline-block; width: 60px; height: 10px\"></i>")
}

/// A paragraph of the inline content `content`, 100px wide, 30px from the
/// left and 50px from the top of the page.
fn paragraph(content: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} \
             #p {{ margin: 50px 0 0 30px; width: 100px; line-height: 20px }} \
             .abs {{ position: absolute; width: 8px; height: 8px }}</style></head>\
             <body><p id=\"p\">{content}</p></body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn an_inline_absolute_box_stands_after_the_content_before_it_on_its_line() {
    let lw = paragraph(&format!(
        "{}<br/>{}<span id=\"a\" class=\"abs\"></span><br/>{}",
        word("w1"),
        word("w2"),
        word("w3")
    ));
    let (w1, w2, a) = (rect(&lw, "w1"), rect(&lw, "w2"), rect(&lw, "a"));
    assert!(
        w2.origin.y >= w1.origin.y + 19.0,
        "the second word is on the second line: {w1:?} {w2:?}"
    );
    assert!(
        near(a.origin.x, w2.origin.x + 60.0, 0.5) && near(a.origin.y, w2.origin.y, 0.5),
        "the box stands at the end of the second word ({}, {}), on its line: {a:?}",
        w2.origin.x + 60.0,
        w2.origin.y
    );
}

#[test]
fn a_block_absolute_box_stands_below_the_line_before_it_at_the_start_edge() {
    let lw = paragraph(&format!(
        "{}<br/>{}<span id=\"a\" class=\"abs\" style=\"display: block\"></span><br/>{}",
        word("w1"),
        word("w2"),
        word("w3")
    ));
    let (p, w2, a) = (rect(&lw, "p"), rect(&lw, "w2"), rect(&lw, "a"));
    assert!(
        near(a.origin.x, p.origin.x, 0.5) && near(a.origin.y, w2.origin.y + 10.0, 0.5),
        "the box stands at the paragraph's left edge {}, below the second word ({}): {a:?}",
        p.origin.x,
        w2.origin.y + 10.0
    );
}

#[test]
fn an_absolute_box_nothing_comes_before_stands_where_the_first_content_starts() {
    let lw = paragraph(&format!(
        "<span id=\"a\" class=\"abs\"></span>{}<br/>{}",
        word("w1"),
        word("w2")
    ));
    let (w1, a) = (rect(&lw, "w1"), rect(&lw, "a"));
    assert!(
        near(a.origin.x, w1.origin.x, 0.5) && near(a.origin.y, w1.origin.y, 0.5),
        "the box stands where the first word starts {w1:?}: {a:?}"
    );
}

#[test]
fn an_absolute_box_inside_nested_inline_boxes_stands_on_its_line_too() {
    // The content of an inline box is content of the paragraph's lines (CSS
    // 2.1 9.2.2): a box taken out of `<em><b>..</b></em>` would have stood
    // on the line the text around it stands on. Only the paragraph's own
    // children got that line; one inside an inline box stood at the
    // paragraph's origin.
    let lw = paragraph(&format!(
        "{}<br/><em><b>{}<span id=\"a\" class=\"abs\"></span></b></em><br/>{}",
        word("w1"),
        word("w2"),
        word("w3")
    ));
    let (w1, w2, a) = (rect(&lw, "w1"), rect(&lw, "w2"), rect(&lw, "a"));
    assert!(w2.origin.y >= w1.origin.y + 19.0, "{w1:?} {w2:?}");
    assert!(
        near(a.origin.x, w2.origin.x + 60.0, 0.5) && near(a.origin.y, w2.origin.y, 0.5),
        "the box stands at the end of the second word ({}, {}), on its line: {a:?}",
        w2.origin.x + 60.0,
        w2.origin.y
    );
}
