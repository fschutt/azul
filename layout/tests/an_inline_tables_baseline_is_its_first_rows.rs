//! An inline table's baseline is its first row's.
//!
//! CSS 2.2 10.8.1 / 17.5.3: the baseline of a table is the baseline of its
//! first row, and a row in which no cell is aligned on the baseline (the
//! HTML default: cells inherit `vertical-align: middle` from their row) has
//! its baseline at the bottom content edge of its lowest cell. azul kept
//! such a row's baseline at its TOP (no baseline cell set it), so an
//! `inline-table` sat on the line with its top on the baseline - hanging a
//! whole table height below the text it was placed in.
//!
//! Chrome 154's numbers (probe). `font-size: 0` on the line keeps text
//! metrics out of it; fixed-size boxes only.

use crate::table_markup::{body, near, rect};

fn line(cell_style: &str) -> String {
    format!(
        "<div id=\"d\" style=\"width: 600px; font-size: 0\">\
         <span id=\"ref\" style=\"display: inline-block; width: 10px; height: 30px\"></span>\
         <table id=\"t\" style=\"display: inline-table; border-spacing: 0\"><tr>\
         <td style=\"{cell_style}\">\
         <span style=\"display: inline-block; width: 50px; height: 30px\"></span>\
         </td></tr></table></div>"
    )
}

#[test]
fn an_inline_table_sits_on_the_line_like_the_box_beside_it() {
    let lw = body(&line("padding: 0"));
    let d = rect(&lw, "d");
    let t = rect(&lw, "t");
    assert!(
        near(t.origin.y, d.origin.y, 0.5),
        "the table's baseline is its row's bottom content edge (30px), on the line's \
         baseline with the 30px box beside it: table {t:?} in line box {d:?}"
    );
    assert!(near(d.size.height, 30.0, 0.5), "one 30px line: {d:?}");
}

#[test]
fn an_inline_tables_baseline_is_above_its_cells_bottom_padding() {
    let lw = body(&line("padding: 0 0 10px 0"));
    let d = rect(&lw, "d");
    let t = rect(&lw, "t");
    assert!(
        near(t.origin.y, d.origin.y, 0.5) && near(d.size.height, 40.0, 0.5),
        "the baseline is the bottom CONTENT edge (40 - 10 = 30): the table's top on the \
         line's top, its padding below the baseline: table {t:?} in {d:?}"
    );
}
