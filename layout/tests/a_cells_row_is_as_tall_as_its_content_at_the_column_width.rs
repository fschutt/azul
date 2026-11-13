//! A table cell's row is as tall as the cell's content laid out at the
//! cell's COLUMN width (CSS 2.2 17.5.3), not at the width the table measured
//! the cell with.
//!
//! The table measures every cell's min- and max-content first
//! (`measure_cell_content_width`), and that measurement leaves the cell's
//! `used_size` behind. `layout_cell_for_height` then laid the cell out at its
//! column width - and took the LARGER of that height and the measurement's
//! (`laid_out.max(measured)`), a term from before a cell's own `height` was
//! read separately (`cell_specified_border_box_height`). A nested table with
//! a percentage width is laid out at its min-content in the min-content
//! measurement - one word per line - so the measurement was several lines
//! taller than the final content, and the row kept that height with the
//! content centred in it (`vertical-align: middle`): Mailgun's billing mail
//! (`<table class="invoice" style="width: 80%">` in a cell, "Lee Munroe<br>
//! Invoice #12345<br>June 01 2014") had a 412px row for Chrome's 323, and
//! the 26 boxes below it 90px low (scripts/refci/mail_boxes.py, MAILREF8
//! group C).
//!
//! Chrome 154 (scripts/refci probe, 16px Arial): case 1 cell 24 / inner table
//! 24 at y 0 (azul 42, the table at y 9); case 2 cell 60 / table 60 (azul 96,
//! the table at y 18). Font-independent: the cell holds only the inner
//! table. Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

fn cell_around(inner_cell: &str) -> azul_layout::window::LayoutWindow {
    body(&format!(
        "<table width=\"518\" cellpadding=\"0\" cellspacing=\"0\"><tr><td id=\"cell\">\
         <table id=\"inner\" style=\"width: 80%\"><tr><td>{inner_cell}</td></tr></table>\
         </td></tr></table>"
    ))
}

#[test]
fn a_percentage_width_table_in_a_cell_does_not_make_its_row_taller() {
    let lw = cell_around("one two");
    let cell = rect(&lw, "cell");
    let inner = rect(&lw, "inner");
    assert!(
        near(cell.size.height, inner.size.height, 0.5),
        "the row is as tall as the 80% table laid out at the column width (Chrome 24 / 24), \
         not as the min-content measurement where its words wrapped: cell {cell:?}, inner \
         {inner:?}"
    );
    assert!(
        near(inner.origin.y, cell.origin.y, 0.5),
        "so nothing is left to centre the table in: cell {cell:?}, inner {inner:?}"
    );
}

#[test]
fn line_breaks_in_the_inner_table_count_once() {
    let lw = cell_around("Lee Munroe<br/>Invoice #12345<br/>June 01 2014");
    let cell = rect(&lw, "cell");
    let inner = rect(&lw, "inner");
    assert!(
        near(cell.size.height, inner.size.height, 0.5),
        "three lines at the column width (Chrome cell 60 / table 60), not five at the \
         min-content width: cell {cell:?}, inner {inner:?}"
    );
}
