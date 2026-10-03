//! Collapsible spaces at the start and the end of a line are removed (CSS
//! Text 3 4.1.2, white-space Phase II), so they are no part of the content's
//! max-content width either: `<td> text </td>` is as wide as `<td>text</td>`.
//!
//! The line breaker removed them (`break_one_line`'s strip_leading /
//! strip_trailing) but the intrinsic scan (`measure_intrinsic_widths`) summed
//! every item of a line, so every shrink-to-fit box around indented markup
//! (a table cell, an inline-block) was two spaces too wide - and its content,
//! laid out without them, sat off-centre in it: Lee Munroe's "Call To Action"
//! button 4px right of Chrome's (`<td align="center"> <a style="display:
//! inline-block">..</a> </td>`, scripts/refci/mail_boxes.py, MAILREF8
//! group F).
//!
//! Chrome 154 (scripts/refci probe, 16px Arial): `<td> text </td>` 25.80 (azul
//! 34.69 = + 2 spaces), `<td> <50px inline-block> </td>` 50 (azul 58.89), the
//! same in an inline-block 50 (azul 58.89). Font-independent: the widths are
//! compared with the same content without the spaces, or with the 50px box.
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

const BOX: &str = "<span style=\"display: inline-block; width: 50px; height: 10px\"></span>";

fn cell(id: &str, content: &str) -> String {
    format!(
        "<table cellpadding=\"0\" cellspacing=\"0\"><tr><td id=\"{id}\">{content}</td></tr></table>"
    )
}

#[test]
fn spaces_around_a_cells_text_do_not_widen_the_cell() {
    let lw = body(&format!(
        "{}{}",
        cell("spaced", " text "),
        cell("tight", "text")
    ));
    let spaced = rect(&lw, "spaced");
    let tight = rect(&lw, "tight");
    assert!(
        near(spaced.size.width, tight.size.width, 0.5),
        "the line's leading and trailing spaces are removed, so the cell is as wide as its \
         text (Chrome 25.80 / 25.80): spaced {spaced:?}, tight {tight:?}"
    );
}

#[test]
fn spaces_around_an_inline_block_do_not_widen_the_cell() {
    let lw = body(&cell("cell", &format!(" {BOX} ")));
    let cell = rect(&lw, "cell");
    assert!(
        near(cell.size.width, 50.0, 0.5),
        "the cell is as wide as the 50px box (Chrome 50): {cell:?}"
    );
}

#[test]
fn spaces_around_an_inline_block_do_not_widen_a_shrink_to_fit_box() {
    let lw = body(&format!(
        "<div id=\"stf\" style=\"display: inline-block\"> {BOX} </div>"
    ));
    let stf = rect(&lw, "stf");
    assert!(
        near(stf.size.width, 50.0, 0.5),
        "a shrink-to-fit box is as wide as the 50px box (Chrome 50): {stf:?}"
    );
}
