//! An anonymous table cell keeps its blocks' margins (LAYOUT7 item 5; MAIL6:
//! `display: table; width: 100%; padding: 12px` around `<p>Hi</p>` was 42px
//! tall, Chrome 74 - the `<p>`'s margins were gone).
//!
//! CSS 2.2 s17.2.1: the `<p>` of a `display: table` box sits in an anonymous
//! row and an anonymous cell; s9.4.1: a table cell establishes a new block
//! formatting context, so its first child's top margin and its last child's
//! bottom margin stay inside it (s8.3.1: no collapsing through a BFC root).
//! `fc::establishes_new_bfc` answered "no" for every box without a DOM node
//! but the table wrapper: the anonymous cell let the margins escape, and
//! nothing outside a cell takes them.
//!
//! Measured in headless Chrome 154 (16px Arial, `line-height: 20px`).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{laid_out, near, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head></head><body style=\"margin: 0; font-size: 16px; line-height: \
             20px\">{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn a_paragraph_in_a_display_table_box_keeps_its_margins() {
    let lw = page(
        "<div style=\"width: 500px\"><div id=\"table\" style=\"display: table; width: 100%; \
         padding: 12px\"><p id=\"p\">Hi</p></div></div>",
    );
    let p = rect(&lw, "p");
    assert!(
        near(p.origin.y, 28.0, 0.5),
        "12px of padding and the paragraph's 16px top margin above it (Chrome y 28): {p:?}"
    );
    let table = rect(&lw, "table");
    assert!(
        near(table.size.height, 76.0, 0.5),
        "12 + 16 + 20 + 16 + 12 (Chrome 76): {table:?}"
    );
}

#[test]
fn the_first_and_last_margins_stay_inside_an_anonymous_cell() {
    let lw = page(
        "<div style=\"width: 500px\"><div id=\"table\" style=\"display: table; width: \
         100%\"><p id=\"a\" style=\"margin: 10px 0px 20px 0px\">Hi</p><p id=\"b\" \
         style=\"margin: 30px 0px\">Ho</p></div></div>",
    );
    let a = rect(&lw, "a");
    assert!(
        near(a.origin.y, 10.0, 0.5),
        "the top margin is inside (Chrome 10): {a:?}"
    );
    let b = rect(&lw, "b");
    assert!(
        near(b.origin.y, 60.0, 0.5),
        "siblings still collapse: 10 + 20 + max(20, 30) (Chrome 60): {b:?}"
    );
    let table = rect(&lw, "table");
    assert!(
        near(table.size.height, 110.0, 0.5),
        "the last bottom margin is inside too: 60 + 20 + 30 (Chrome 110): {table:?}"
    );
}
