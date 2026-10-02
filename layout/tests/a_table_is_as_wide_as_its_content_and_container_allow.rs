//! A table is as wide as its content and its container allow.
//!
//! CSS 2.1 17.5.2.2 (automatic table layout): an auto-width table's used
//! width is the greater of its minimum width (MIN, every column at its
//! min-content plus the cell spacing) and the smaller of its maximum width
//! (MAX) and the containing block's width - `max(MIN, min(MAX, available))`.
//! A table with a `width` uses `max(width, MIN)`. `min-width` and
//! `max-width` constrain the table like any box, and the result is then
//! distributed over the columns.
//!
//! The bug (REFCI sweep, 2026-09-30): `calculate_used_size_for_node` gave
//! an auto-width table its MAX-content width whatever its container, so a
//! 600 px newsletter table came out 2886 px wide at a 760 px viewport and
//! every box in the mail shifted.
//!
//! Sizes come from fixed-size inline boxes (`table_markup::block` /
//! `words`), so the numbers do not depend on the machine's fonts.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{block, body, near, prose, rect, right};

#[test]
fn an_auto_table_of_long_content_is_capped_by_its_container() {
    // Max-content: sixty words in one line, far wider than 600px.
    let lw = body(&format!(
        "<div style=\"width: 600px\">\
         <table id=\"t\" style=\"border-spacing: 0\"><tr>\
         <td id=\"c\" style=\"padding: 0\">{}</td></tr></table></div>",
        prose(60)
    ));
    let t = rect(&lw, "t");
    assert!(
        t.size.width <= 600.5,
        "the auto-width table is no wider than its 600px container: {}",
        t.size.width
    );
    assert!(
        t.size.width >= 599.5,
        "content wider than the container fills it: {}",
        t.size.width
    );
    let c = rect(&lw, "c");
    assert!(
        right(&c) <= right(&t) + 0.5,
        "the cell stays inside the table: cell {c:?}, table {t:?}"
    );
    assert!(
        c.size.height >= 19.5,
        "the boxes wrap onto at least two lines inside the 600px cell: height {}",
        c.size.height
    );
}

#[test]
fn an_auto_table_of_short_content_stays_at_its_max_content() {
    let lw = body(&format!(
        "<div style=\"width: 600px\">\
         <table id=\"t\" style=\"border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table></div>",
        block(120)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 120.0, 0.5),
        "an auto table is as wide as its content when that fits: {}",
        t.size.width
    );
}

#[test]
fn a_percentage_width_resolves_against_the_container() {
    let lw = body(&format!(
        "<div style=\"width: 600px\">\
         <table id=\"t\" style=\"width: 50%; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table></div>",
        block(20)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 300.0, 0.5),
        "width: 50% of a 600px container is 300px: {}",
        t.size.width
    );
}

#[test]
fn a_width_below_the_min_content_grows_to_the_min_content() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 10px; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table>",
        block(200)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 200.0, 0.5),
        "a table is never narrower than its columns' minimum (CSS 2.1 17.5.2.2): {}",
        t.size.width
    );
}

#[test]
fn max_width_caps_an_auto_table() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"max-width: 300px; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table>",
        prose(40)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 300.0, 0.5),
        "max-width: 300px caps the table: {}",
        t.size.width
    );
}

#[test]
fn min_width_floors_an_auto_table() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"min-width: 400px; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table>",
        block(50)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 400.0, 0.5),
        "min-width: 400px widens the table: {}",
        t.size.width
    );
}

#[test]
fn border_spacing_is_part_of_the_tables_width() {
    // Two 100px columns, 10px spacing on three gutters: 230px.
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 10px\"><tr>\
         <td id=\"a\" style=\"padding: 0\">{0}</td><td id=\"b\" style=\"padding: 0\">{0}</td>\
         </tr></table>",
        block(100)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 230.0, 0.5),
        "the table's width counts its columns and its border-spacing: {}",
        t.size.width
    );
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(near(a.origin.x - t.origin.x, 10.0, 0.5), "a at 10: {a:?}");
    assert!(near(b.origin.x - t.origin.x, 120.0, 0.5), "b at 120: {b:?}");
    assert!(
        near(right(&b), right(&t) - 10.0, 0.5),
        "b ends 10px before the table: {b:?}"
    );
}

#[test]
fn a_table_element_measures_its_width_as_a_border_box() {
    // The HTML rendering rules give `table` `box-sizing: border-box`: a
    // 300px table with 5px borders is 300px wide outside, not 310px.
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 300px; border: 5px solid black; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{}</td></tr></table>",
        block(20)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 300.0, 0.5),
        "the <table> element's width is its border box: {}",
        t.size.width
    );
}

#[test]
fn the_table_elements_default_spacing_is_two_pixels() {
    // HTML's `table { border-spacing: 2px }`: one 100px cell, 104px.
    let lw = body(&format!(
        "<table id=\"t\"><tr><td style=\"padding: 0\">{}</td></tr></table>",
        block(100)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 104.0, 0.5),
        "the UA's border-spacing: 2px puts 2px on each side of the cell: {}",
        t.size.width
    );
}
