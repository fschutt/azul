//! Row groups are boxes, stacked in order.
//!
//! CSS 2.1 17.2 / 17.5: `tbody`, `thead` and `tfoot` (`table-row-group`,
//! `table-header-group`, `table-footer-group`) and the rows in them are
//! boxes of the table grid, with a position, a size, a background (layer 4
//! and 5 of the six table layers, 17.5.1). A header group is placed before
//! every other row group and a footer group after them, wherever they are in
//! the markup; the rows of a later row group come after the rows of an
//! earlier one. `colgroup` / `col` are boxes spanning their columns. The
//! table itself paints its own background and border like any box, then
//! the layers above it. `caption-side` puts the caption above or below the
//! rows.
//!
//! The bugs (REFCI mail corpus, 2026-09-30): `<tbody>` had no box at all
//! (`tbody` 119/119 boxes missing), and in the postmark receipt a `<tr>` of
//! the second row group sat at the top of the table (y = 2 instead of
//! y = 1152): only the cells were positioned, relative to the table, and
//! rows and row groups kept no rect. The table's own border was never
//! painted in the separated model (WPT `table-row-group-001`).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{block, body, borders, bottom, near, rect, rects_of_color, right};

fn cell(content: &str) -> String {
    format!("<td style=\"padding: 0\">{content}</td>")
}

#[test]
fn a_second_tbody_starts_below_the_first() {
    let b = block(50);
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 0\">\
         <tbody id=\"g1\"><tr id=\"r1\">{c}</tr><tr id=\"r2\">{c}</tr></tbody>\
         <tbody id=\"g2\"><tr id=\"r3\"><td id=\"c3\" style=\"padding: 0\">{b}</td></tr></tbody>\
         </table>",
        c = cell(&b),
    ));
    let t = rect(&lw, "t");
    let (g1, g2) = (rect(&lw, "g1"), rect(&lw, "g2"));
    let (r1, r2, r3) = (rect(&lw, "r1"), rect(&lw, "r2"), rect(&lw, "r3"));
    let c3 = rect(&lw, "c3");

    assert!(
        g1.size.height > 0.0 && g1.size.width > 0.0,
        "the first tbody has a box: {g1:?}"
    );
    assert!(
        g2.size.height > 0.0 && g2.size.width > 0.0,
        "the second tbody has a box: {g2:?}"
    );
    assert!(
        near(g1.origin.y, t.origin.y, 0.5),
        "the first group starts at the table top: {g1:?} {t:?}"
    );
    assert!(r1.origin.y < r2.origin.y, "rows stack: {r1:?} {r2:?}");
    assert!(
        r3.origin.y >= bottom(&r2) - 0.5,
        "the second group's row comes after the first group's rows: r2 {r2:?}, r3 {r3:?}"
    );
    assert!(
        g2.origin.y >= bottom(&g1) - 0.5,
        "the second group starts below the first: {g1:?} {g2:?}"
    );
    assert!(
        near(g1.size.height, r1.size.height + r2.size.height, 0.5),
        "a group is as tall as its rows: {g1:?} {r1:?} {r2:?}"
    );
    assert!(
        near(g2.size.width, t.size.width, 0.5),
        "a group spans the table: {g2:?} {t:?}"
    );
    assert!(
        near(c3.origin.y, r3.origin.y, 0.5),
        "the cell sits in its row: {c3:?} {r3:?}"
    );
    assert!(
        near(c3.origin.x, t.origin.x, 0.5),
        "and in its column: {c3:?} {t:?}"
    );
}

#[test]
fn the_thead_is_placed_first_and_the_tfoot_last() {
    let b = block(50);
    let lw = body(&format!(
        "<table style=\"border-spacing: 0\">\
         <tfoot id=\"f\"><tr>{c}</tr></tfoot>\
         <tbody id=\"b\"><tr>{c}</tr></tbody>\
         <thead id=\"h\"><tr>{c}</tr></thead>\
         </table>",
        c = cell(&b),
    ));
    let (h, bd, f) = (rect(&lw, "h"), rect(&lw, "b"), rect(&lw, "f"));
    assert!(
        bottom(&h) <= bd.origin.y + 0.5,
        "the header group is placed before the body: thead {h:?}, tbody {bd:?}"
    );
    assert!(
        bottom(&bd) <= f.origin.y + 0.5,
        "the footer group is placed after the body: tbody {bd:?}, tfoot {f:?}"
    );
}

#[test]
fn a_caption_sits_above_the_rows_or_below_them_with_caption_side_bottom() {
    let b = block(50);
    let top = body(&format!(
        "<table style=\"border-spacing: 0\"><caption id=\"cap\">{b}</caption>\
         <tr id=\"r\">{c}</tr></table>",
        c = cell(&b),
    ));
    let (cap, r) = (rect(&top, "cap"), rect(&top, "r"));
    assert!(
        bottom(&cap) <= r.origin.y + 0.5,
        "a caption is above the rows: {cap:?} {r:?}"
    );

    let below = body(&format!(
        "<table style=\"border-spacing: 0; caption-side: bottom\"><caption id=\"cap\">{b}</caption>\
         <tr id=\"r\">{c}</tr></table>",
        c = cell(&b),
    ));
    let (cap, r) = (rect(&below, "cap"), rect(&below, "r"));
    assert!(
        cap.origin.y >= bottom(&r) - 0.5,
        "caption-side: bottom puts it below: {cap:?} {r:?}"
    );
}

#[test]
fn a_row_groups_background_is_painted_over_its_rows() {
    let b = block(50);
    let lw = body(&format!(
        "<table style=\"border-spacing: 0\">\
         <tbody id=\"g\" style=\"background-color: rgb(0, 0, 255)\"><tr>{c}{c}</tr><tr>{c}{c}</tr></tbody>\
         <tbody><tr>{c}{c}</tr></tbody></table>",
        c = cell(&b),
    ));
    let g = rect(&lw, "g");
    let blue = rects_of_color(&lw, (0, 0, 255));
    assert!(
        blue.iter().any(|r| near(r.origin.x, g.origin.x, 0.5)
            && near(r.origin.y, g.origin.y, 0.5)
            && near(r.size.width, g.size.width, 0.5)
            && near(r.size.height, g.size.height, 0.5)
            && r.size.height > 0.0),
        "the tbody's background covers the tbody's box {g:?}: {blue:?}"
    );
}

#[test]
fn the_tables_own_border_is_painted_in_the_separated_model() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"border: 3px solid rgb(1, 2, 3); border-spacing: 0\">\
         <tr>{}</tr></table>",
        cell(&block(50))
    ));
    let t = rect(&lw, "t");
    let painted = borders(&lw);
    assert!(
        painted.iter().any(|r| near(r.origin.x, t.origin.x, 0.5)
            && near(r.origin.y, t.origin.y, 0.5)
            && near(r.size.width, t.size.width, 0.5)
            && near(r.size.height, t.size.height, 0.5)),
        "the table's border box is stroked {t:?}: {painted:?}"
    );
}

#[test]
fn columns_and_column_groups_span_their_columns() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 0\">\
         <colgroup id=\"cg\"><col id=\"c1\"/><col id=\"c2\"/></colgroup>\
         <tr>{}{}</tr></table>",
        cell(&block(100)),
        cell(&block(50))
    ));
    let t = rect(&lw, "t");
    let (cg, c1, c2) = (rect(&lw, "cg"), rect(&lw, "c1"), rect(&lw, "c2"));
    assert!(
        near(c1.origin.x, t.origin.x, 0.5),
        "col 1 starts the table: {c1:?}"
    );
    assert!(
        near(c1.size.width, 100.0, 0.5),
        "col 1 is its column's width: {c1:?}"
    );
    assert!(
        near(c2.origin.x, right(&c1), 0.5),
        "col 2 follows col 1: {c2:?}"
    );
    assert!(
        near(c2.size.width, 50.0, 0.5),
        "col 2 is its column's width: {c2:?}"
    );
    assert!(
        near(cg.size.width, 150.0, 0.5),
        "the group spans both: {cg:?}"
    );
    assert!(
        c1.size.height > 0.0,
        "a column is as tall as the rows: {c1:?}"
    );
}
