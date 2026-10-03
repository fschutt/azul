//! A separated table spaces its cells and paints its own border.
//!
//! CSS 2.2 17.6.1 (the separated borders model):
//!
//! - `border-spacing` separates the cells from each other and from the
//!   table's padding edge, so an auto-width table is its columns plus
//!   `columns + 1` spacings wide (and `rows + 1` spacings taller): the
//!   table's intrinsic width left the spacing out, so its box was narrower
//!   than its cells and the table's background stopped short of the last
//!   cell;
//! - a spanning cell's width covers the spacing between the columns it
//!   spans, so the columns themselves share only the rest;
//! - the table paints its own border: `paint_table_items` returned before
//!   the generic painter and drew only the background colour, so
//!   `<table style="border: 10px solid">` showed no border at all;
//! - `empty-cells: hide` paints neither the border nor the background of an
//!   empty cell.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{
    count_colour, laid_out_page as laid_out, near_tenth as near, page, pixels_differing, rect,
    render,
};

const RED: (u8, u8, u8) = (255, 0, 0);

#[test]
fn a_separated_table_paints_its_own_border() {
    let test = page(
        "table { border: 10px solid green; border-spacing: 0 } \
         td { width: 80px; height: 80px; padding: 0; background: green }",
        "<table><tr><td></td></tr></table>",
    );
    let reference = page(
        "",
        "<div style=\"width: 100px; height: 100px; background: green\"></div>",
    );
    assert_eq!(
        pixels_differing(&test, &reference),
        0,
        "an 80px cell inside a 10px green table border: one 100px square"
    );
}

#[test]
fn the_border_spacing_runs_around_every_cell_and_inside_the_table() {
    let style = "table { border-spacing: 10px 5px; background: green } \
                 td { width: 20px; height: 20px; padding: 0; background: green }";
    let test = page(
        style,
        "<table id=\"t\"><tr><td></td><td></td></tr><tr><td></td><td></td></tr></table>",
    );
    let lw = laid_out(&test);
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 70.0),
        "two 20px columns and three 10px spacings: {}",
        t.size.width
    );
    assert!(
        near(t.size.height, 55.0),
        "two 20px rows and three 5px spacings: {}",
        t.size.height
    );
    let reference = page(
        "",
        "<div style=\"width: 70px; height: 55px; background: green\"></div>",
    );
    assert_eq!(
        pixels_differing(&test, &reference),
        0,
        "the table's background covers the spacing around its last cells"
    );
}

#[test]
fn a_spanning_cells_width_covers_the_spacing_between_its_columns() {
    // WPT border-spacing-095 without the presentational attributes.
    let lw = laid_out(&page(
        "table { border-spacing: 20px } td { padding: 0 }",
        "<table id=\"t\"><tr><td id=\"span\" colspan=\"3\" style=\"width: 100px\">\
         <div style=\"height: 100px\"></div></td></tr></table>",
    ));
    let t = rect(&lw, "t");
    let span = rect(&lw, "span");
    assert!(
        near(span.size.width, 100.0),
        "the spanning cell is its own 100px: {}",
        span.size.width
    );
    assert!(
        near(t.size.width, 140.0),
        "100px of cell and the two outer spacings - the two inner ones are inside \
         the cell: {}",
        t.size.width
    );
}

#[test]
fn an_empty_cell_with_empty_cells_hide_paints_nothing() {
    // The text cell keeps the row 40px tall, so the empty cell has a box to
    // paint into.
    let test = page(
        "table { border-spacing: 0 } td { width: 40px; height: 40px; padding: 0 } \
         td.e { empty-cells: hide; background: red; border: 5px solid red }",
        "<table><tr><td>x</td><td class=\"e\"></td></tr></table>",
    );
    assert_eq!(
        count_colour(&render(&test), RED),
        0,
        "neither the background nor the border of the empty cell paints"
    );
}
