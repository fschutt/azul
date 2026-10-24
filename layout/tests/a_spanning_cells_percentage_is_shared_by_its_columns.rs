//! A spanning cell's percentage is shared by its columns.
//!
//! CSS Tables 3 3.8 (and every browser): a cell spanning several columns
//! with a percentage `width` gives that percentage to the columns it spans -
//! what their own percentages leave of it, to the columns without one,
//! in proportion to their max-content (equally when they have none).
//! `table_width::distribute_spanning_cell` spread a spanning cell's min-
//! and max-content and a FIXED width, and dropped a percentage: `<td
//! colspan="2" width="50%">` in a 400px table came out as three equal
//! columns.
//!
//! Chrome 154's numbers (probe). Fixed-size boxes only.

use crate::table_markup::{body, near, rect};

fn bx() -> &'static str {
    "<span style=\"display: inline-block; width: 10px; height: 10px\"></span>"
}

#[test]
fn a_fifty_percent_two_column_cell_gives_each_column_twenty_five() {
    let b = bx();
    let lw = body(&format!(
        "<table style=\"width: 400px; border-spacing: 0\"><tr>\
         <td id=\"a\" colspan=\"2\" style=\"padding: 0; width: 50%\">{b}</td>\
         <td id=\"b\" style=\"padding: 0\">{b}</td></tr><tr>\
         <td id=\"c\" style=\"padding: 0\">{b}</td><td id=\"d\" style=\"padding: 0\">{b}</td>\
         <td id=\"e\" style=\"padding: 0\">{b}</td></tr></table>"
    ));
    for (id, width) in [
        ("a", 200.0),
        ("b", 200.0),
        ("c", 100.0),
        ("d", 100.0),
        ("e", 200.0),
    ] {
        let r = rect(&lw, id);
        assert!(
            near(r.size.width, width, 0.5),
            "#{id} is {width}px (Chrome): {}",
            r.size.width
        );
    }
}
