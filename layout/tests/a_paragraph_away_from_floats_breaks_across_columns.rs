//! In a block multi-column container with a float, a paragraph the float
//! does not reach breaks across columns between its lines.
//!
//! A paragraph whose lines wrap around a float cannot be broken again in a
//! column (its lines are what they are beside the float). The multi-column
//! layout kept every paragraph of a container whole as soon as the
//! container held one float.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::mono_lines::page_glyphs;

#[test]
fn a_paragraph_below_the_float_breaks_between_its_lines() {
    // Two 190px columns. The float stands beside the first paragraph only;
    // the second, five lines of `bbbb`, starts below it and breaks across
    // the columns as a container without floats would break it.
    let words = ["bbbb"; 20].join(" ");
    let pens = page_glyphs(
        ".c { width: 400px; column-count: 2; column-gap: 20px } .f { float: left; width: 20px; \
         height: 20px }",
        &format!("<div class=\"c\"><div class=\"f\"></div><p>aa</p><p>{words}</p></div>"),
    );
    assert!(
        pens.iter().any(|&(x, top)| x < 190.0 && top >= 20.0 - 0.01),
        "the second paragraph starts in the first column: {pens:?}"
    );
    assert!(
        pens.iter().any(|&(x, _)| x >= 210.0 - 0.01),
        "and goes on in the second: {pens:?}"
    );
}
