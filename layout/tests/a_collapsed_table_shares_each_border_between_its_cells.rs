//! A collapsed table shares each border between its cells.
//!
//! CSS 2.2 17.6.2 (the collapsing border model), as browsers lay it out:
//!
//! - every grid edge has ONE border, the winner of the borders that meet
//!   there (cell, row, row group, column, column group, table): `hidden`
//!   suppresses the edge, then the wider border wins, then the style
//!   (double > solid > dashed > dotted > ridge > outset > groove > inset),
//!   then the element (cell > row > row group > column > column group >
//!   table), then the one further left / further up;
//! - a cell's box takes HALF of each of its edges, and the table's border is
//!   half of the widest edge on each of its sides, the other half spilling
//!   into the margin;
//! - the table has no padding;
//! - the winning border is painted once, centred on its grid line.
//!
//! The engine laid a collapsed table out with every cell's FULL border (a
//! 4x4 table of 20px borders came out 160px instead of 100px), kept the
//! table's padding, and painted per row by pairing cells positionally.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{
    count_colour, laid_out_page as laid_out, near_tenth as near, page, pixels_differing, rect,
    render,
};

const RED: (u8, u8, u8) = (255, 0, 0);

#[test]
fn a_cell_takes_half_of_each_collapsed_edge_and_the_table_the_outer_half() {
    // WPT css/CSS2/tables/collapsing-border-model-001: the cell's 50px border
    // beats the table's 25px on every edge.
    let lw = laid_out(&page(
        "table { border: 25px solid red; border-collapse: collapse; padding: 10px } \
         td { border: 50px solid blue; padding: 10px; width: 100px }",
        "<table id=\"t\"><tr><td id=\"c\"></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let c = rect(&lw, "c");
    assert!(near(t.size.width, 220.0), "table width: {}", t.size.width);
    assert!(
        near(t.size.height, 120.0),
        "table height: {}",
        t.size.height
    );
    assert!(near(c.size.width, 170.0), "cell width: {}", c.size.width);
    assert!(near(c.size.height, 70.0), "cell height: {}", c.size.height);
    assert!(
        near(c.origin.x - t.origin.x, 25.0) && near(c.origin.y - t.origin.y, 25.0),
        "the cell starts after the table's half border, the table's padding ignored: \
         ({}, {})",
        c.origin.x - t.origin.x,
        c.origin.y - t.origin.y
    );
}

#[test]
fn a_collapsed_table_has_no_padding() {
    // WPT collapsing-border-model-011 / -012.
    let reference = page(
        "",
        "<div style=\"width: 100px; height: 100px; background: green\"></div>",
    );
    for sizing in ["content-box", "border-box"] {
        let test = page(
            &format!(
                "table {{ border-collapse: collapse; box-sizing: {sizing}; width: 100px; \
                 height: 100px; padding: 100px; background: green }}"
            ),
            "<table></table>",
        );
        assert_eq!(
            pixels_differing(&test, &reference),
            0,
            "a {sizing} collapsed table of 100x100 paints a 100x100 square"
        );
    }
}

#[test]
fn a_grid_of_20px_borders_is_one_100px_square_painted_by_the_winners() {
    // WPT border-conflict-element-001a: equal borders, the left / upper cell
    // wins, so every red border loses.
    // Every cell but the first column has a red left border, every cell
    // below the first row a red top border.
    let cells = |row: &str| {
        format!(
            "<tr><td class=\"{row}\"></td> <td class=\"{row} x\"></td> \
             <td class=\"{row} x\"></td> <td class=\"{row} x\"></td></tr>"
        )
    };
    let test = page(
        "table { border-collapse: collapse } \
         td { border: 20px solid green; padding: 0 } \
         td.x { border-left-color: red } \
         td.below { border-top-color: red }",
        &format!(
            "<table>{}{}{}{}</table>",
            cells("top"),
            cells("below"),
            cells("below"),
            cells("below")
        ),
    );
    let reference = page(
        "",
        "<div style=\"width: 100px; height: 100px; background: green\"></div>",
    );
    assert_eq!(
        count_colour(&render(&test), RED),
        0,
        "no losing border paints"
    );
    assert_eq!(
        pixels_differing(&test, &reference),
        0,
        "4 x 20px of cells plus two 10px table halves: one 100px green square"
    );
}

#[test]
fn a_spanning_cell_shares_its_edge_with_every_cell_below_it() {
    // WPT css/css-tables/border-conflict-resolution, without the text: the
    // spanning cell's solid border beats a dashed one on the same edge and
    // wins the tie with an equal solid one (it is further up).
    let test = page(
        "table { border-collapse: collapse } td { padding: 0; width: 40px; height: 20px }",
        "<table><tr><td colspan=\"2\" style=\"border: 10px solid green\"></td></tr>\
         <tr><td style=\"border-top: 10px dashed red\"></td>\
         <td style=\"border-top: 10px solid red\"></td></tr></table>",
    );
    assert_eq!(count_colour(&render(&test), RED), 0, "the red tops lose");
}

#[test]
fn a_hidden_border_suppresses_every_border_on_its_edge() {
    let test = page(
        "table { border-collapse: collapse; border: 10px solid red } \
         td { border-style: hidden; padding: 0; width: 50px; height: 50px; \
         background: green }",
        "<table><tr><td></td></tr></table>",
    );
    let reference = page(
        "",
        "<div style=\"width: 50px; height: 50px; background: green\"></div>",
    );
    assert_eq!(
        pixels_differing(&test, &reference),
        0,
        "the hidden cell edges win over the table's red border, which takes no room"
    );
}

#[test]
fn column_backgrounds_cover_their_columns() {
    // WPT table-visual-layout-018: `col` boxes paint under the cells.
    let test = page(
        "table { background: red; border-collapse: collapse } col { background: green } \
         td { width: 20px; height: 20px; padding: 0 }",
        "<table><col/><col/><tr><td></td><td></td></tr><tr><td></td><td></td></tr></table>",
    );
    let reference = page(
        "",
        "<div style=\"width: 40px; height: 40px; background: green\"></div>",
    );
    assert_eq!(pixels_differing(&test, &reference), 0, "no red table shows");
}
