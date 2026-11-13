//! An inline-block in a cell sits where its line puts it.
//!
//! A cell whose children are all inline-level is one inline formatting
//! context, and its final layout (`fc::layout_cell_for_height`) lays it out
//! with `layout_ifc`. That layout reports where each atomic inline went in
//! its `LayoutOutput::positions`, but the cell's branch dropped them: the
//! inline-block's background was painted where the line put it (from the
//! line's own items), while the box itself - its position for hit-testing,
//! `get_node_position`, and its CONTENT, painted relative to it - stayed at
//! the cell's content origin. The postmark receipt's centered button
//! (`<td style="text-align: center"><a style="display: inline-block">Use
//! this discount now...</a></td>`) painted its green box in the middle and
//! its label at the cell's left edge (mail_boxes, 2026-10-02).
//!
//! Chrome 154's numbers (probe). Fixed-size boxes where a number is
//! asserted.

use crate::table_markup::{body, glyph_runs, near, rect, right};

#[test]
fn a_centered_inline_block_in_a_cell_is_centered_with_its_content() {
    let lw = body(
        "<table style=\"width: 300px; border-spacing: 0\"><tr>\
         <td id=\"c\" style=\"padding: 0; text-align: center\"> \
         <span id=\"b\" style=\"display: inline-block; width: 100px; height: 20px\">\
         <span id=\"inner\" style=\"display: block; width: 50px; height: 10px\"></span>\
         </span> </td></tr></table>",
    );
    let c = rect(&lw, "c");
    let b = rect(&lw, "b");
    let inner = rect(&lw, "inner");
    assert!(
        near(b.origin.x - c.origin.x, 100.0, 0.5),
        "the 100px inline-block is centered in the 300px cell: {b:?} in {c:?}"
    );
    assert!(
        near(inner.origin.x, b.origin.x, 0.5) && inner.origin.y >= b.origin.y - 0.5,
        "its content is laid out inside it: {inner:?} in {b:?}"
    );
}

#[test]
fn a_button_label_is_painted_inside_its_button() {
    let lw = body(
        "<table style=\"width: 400px; border-spacing: 0\"><tr>\
         <td style=\"padding: 0; text-align: center\"> \
         <a id=\"a\" style=\"display: inline-block; padding: 10px 18px\">Use this discount now</a> \
         </td></tr></table>",
    );
    let a = rect(&lw, "a");
    assert!(
        a.origin.x > 50.0,
        "the button is centered, not at the cell's left edge: {a:?}"
    );
    // The label's run (the cell's own runs are single collapsed spaces).
    let pens: Vec<(f32, f32)> = glyph_runs(&lw)
        .into_iter()
        .filter(|run| run.len() > 3)
        .flatten()
        .collect();
    assert!(!pens.is_empty(), "the label paints");
    for &(x, _) in &pens {
        assert!(
            x >= a.origin.x && x < right(&a),
            "every glyph of the label lies inside the button: pen x={x}, button {a:?}"
        );
    }
}
