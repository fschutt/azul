//! The static position of an out-of-flow box moves into the columns with
//! the flow it was taken out of.
//!
//! CSS 2.1 10.6.4: the static position is the place of "a hypothetical box
//! that would have been the first box of the element" in the flow. A
//! multi-column container lays its children out as one column and then cuts
//! that column into its columns (CSS Multicol 1); the in-flow children were
//! moved to their columns, the static positions of the out-of-flow ones
//! were not: an absolute box written after the first block of the second
//! column stood in the first column, below the container's last line.
//!
//! Two 200px columns 20px apart (`width: 420px`); every box 40px tall.

use crate::table_markup::{laid_out, near, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} \
             .cols {{ column-count: 2; column-gap: 20px; width: 420px; line-height: 20px }} \
             .b {{ height: 40px }} \
             .abs {{ position: absolute; width: 8px; height: 8px }}</style></head>\
             <body>{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn an_absolute_child_of_the_container_stands_in_the_column_of_the_block_before_it() {
    let lw = page(
        "<div class=\"cols\"><div id=\"b0\" class=\"b\"></div><div id=\"b1\" class=\"b\"></div>\
         <div id=\"b2\" class=\"b\"></div><div id=\"a\" class=\"abs\"></div>\
         <div id=\"b3\" class=\"b\"></div></div>",
    );
    let (b2, b3, a) = (rect(&lw, "b2"), rect(&lw, "b3"), rect(&lw, "a"));
    assert!(
        near(b2.origin.x, 220.0, 0.5) && near(b2.origin.y, 0.0, 0.5),
        "the third box tops the second column: {b2:?}"
    );
    assert!(
        near(a.origin.x, b3.origin.x, 0.5) && near(a.origin.y, b3.origin.y, 0.5),
        "the box stands where the block after it starts {b3:?}: {a:?}"
    );
}

#[test]
fn an_absolute_box_in_a_line_that_went_to_the_next_column_stands_there() {
    // Four lines of one 150px inline block each (a `<br/>` between them):
    // two a column.
    let word = |id: &str| {
        format!("<i id=\"{id}\" style=\"display: inline-block; width: 150px; height: 10px\"></i>")
    };
    let lw = page(&format!(
        "<div class=\"cols\"><p>{}<br/>{}<br/>{}<br/>{}<span id=\"a\" class=\"abs\"></span></p></div>",
        word("w1"),
        word("w2"),
        word("w3"),
        word("w4")
    ));
    let (w4, a) = (rect(&lw, "w4"), rect(&lw, "a"));
    assert!(
        near(w4.origin.x, 220.0, 0.5),
        "the fourth line is in the second column: {w4:?}"
    );
    assert!(
        near(a.origin.x, w4.origin.x + 150.0, 0.5) && near(a.origin.y, w4.origin.y, 0.5),
        "the box stands at the end of the fourth line ({}, {}): {a:?}",
        w4.origin.x + 150.0,
        w4.origin.y
    );
}
