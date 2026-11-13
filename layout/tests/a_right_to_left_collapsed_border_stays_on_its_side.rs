//! In a right-to-left table with collapsing borders, a cell's left border is
//! painted on its left (MAILENG6 item 5, TABLES' open note in 3.8).
//!
//! CSS 2.2 17.5 / 17.6.2: in a `direction: rtl` table the first column is
//! the rightmost, and the collapsed grid edge between two cells is decided
//! by the borders that physically meet there: the LEFT border of the cell on
//! its right and the RIGHT border of the cell on its left.
//! `fc::resolve_collapsed_borders` walks the column lines in the table's
//! order and took the cell BEFORE a line (logically) as the one on its left:
//! in an rtl table each cell's left and right borders swapped places, and
//! the layout's half-borders went to the wrong side of the cell box.
//!
//! Fixed-size boxes; the borders' positions are read off the painted strips
//! (each collapsed edge is one strip centred on its grid line, and a cell's
//! border box ends on that line).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect, rects_of_color};

const TABLE: &str = "<table dir=\"rtl\" style=\"border-collapse: collapse\"><tr>\
    <td id=\"a\" style=\"padding: 0; border-left: 10px solid rgb(200, 0, 0); \
    border-right: 2px solid rgb(0, 0, 200)\">\
    <span style=\"display: inline-block; width: 50px; height: 10px\"></span></td>\
    <td id=\"b\" style=\"padding: 0\">\
    <span style=\"display: inline-block; width: 50px; height: 10px\"></span></td>\
    </tr></table>";

#[test]
fn a_cells_left_border_in_an_rtl_table_is_between_it_and_the_cell_on_its_left() {
    let lw = body(TABLE);
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(
        b.origin.x < a.origin.x,
        "the first cell is on the right in an rtl table: a {a:?}, b {b:?}"
    );
    let red = rects_of_color(&lw, (200, 0, 0));
    assert_eq!(red.len(), 1, "the 10px left border is one strip: {red:?}");
    let red_centre = red[0].origin.x + red[0].size.width / 2.0;
    assert!(
        near(red_centre, a.origin.x, 1.0) && near(red[0].size.width, 10.0, 0.5),
        "the cell's LEFT border is on the grid line between it and #b (x {}): {red:?}",
        a.origin.x
    );
    let blue = rects_of_color(&lw, (0, 0, 200));
    assert_eq!(blue.len(), 1, "the 2px right border is one strip: {blue:?}");
    let blue_centre = blue[0].origin.x + blue[0].size.width / 2.0;
    assert!(
        near(blue_centre, a.origin.x + a.size.width, 1.0),
        "the cell's RIGHT border is the table's right edge (x {}): {blue:?}",
        a.origin.x + a.size.width
    );
}
