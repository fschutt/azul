//! Percentage and fixed columns share the table like browsers.
//!
//! The automatic table layout distributes the table's width over its
//! columns in the order of CSS Tables 3 section 3.9.3 (what Chrome does):
//! every column gets its min-content width first; then percentage columns
//! grow towards their percentage of the table; then fixed (`width: <px>`)
//! columns towards their specified width; then auto columns towards their
//! max-content. Width left over after every column reached its maximum
//! goes to the auto columns (in proportion to their max-content), only then
//! to fixed columns, then to percentage columns. Percentages that sum over
//! 100 % are cut back, left to right, so they sum to 100 %.
//!
//! The bug: every column got a share of the excess in proportion to its
//! max-content width, whatever its `width` said: `<td width="100">a</td>
//! <td>a</td>` in a 400px table came out 200 / 200 instead of 100 / 300
//! (WPT `html/rendering/.../table-cell-width-s`), and percentages were not
//! read at all.
//!
//! Sizes come from fixed-size inline boxes, so the numbers are font-free.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{block, body, near, rect};

/// A 400px table (no spacing) with the given cells (inner markup of a `tr`).
fn table(cells: &str) -> azul_layout::window::LayoutWindow {
    body(&format!(
        "<table id=\"t\" style=\"width: 400px; border-spacing: 0\"><tr>{cells}</tr></table>"
    ))
}

fn cell(id: &str, style: &str, w: u32) -> String {
    format!(
        "<td id=\"{id}\" style=\"padding: 0; {style}\">{}</td>",
        block(w)
    )
}

#[test]
fn a_fixed_column_keeps_its_width_and_the_auto_column_takes_the_rest() {
    let lw = table(&format!(
        "{}{}",
        cell("a", "width: 100px", 10),
        cell("b", "", 10)
    ));
    let (a, b) = (rect(&lw, "a"), rect(&lw, "b"));
    assert!(near(a.size.width, 100.0, 0.5), "the 100px column: {a:?}");
    assert!(
        near(b.size.width, 300.0, 0.5),
        "the auto column takes the rest: {b:?}"
    );
}

#[test]
fn a_percentage_column_takes_its_share_of_the_table() {
    let lw = table(&format!(
        "{}{}",
        cell("a", "width: 25%", 10),
        cell("b", "", 10)
    ));
    let (a, b) = (rect(&lw, "a"), rect(&lw, "b"));
    assert!(near(a.size.width, 100.0, 0.5), "25% of 400: {a:?}");
    assert!(
        near(b.size.width, 300.0, 0.5),
        "the auto column takes the rest: {b:?}"
    );
}

#[test]
fn percentages_over_a_hundred_are_cut_back_left_to_right() {
    let lw = table(&format!(
        "{}{}",
        cell("a", "width: 80%", 10),
        cell("b", "width: 80%", 10)
    ));
    let (a, b) = (rect(&lw, "a"), rect(&lw, "b"));
    assert!(
        near(a.size.width, 320.0, 0.5),
        "the first keeps its 80%: {a:?}"
    );
    assert!(
        near(b.size.width, 80.0, 0.5),
        "the second is cut to the 20% left: {b:?}"
    );
}

#[test]
fn auto_columns_share_the_excess_in_proportion_to_their_max_content() {
    let lw = table(&format!("{}{}", cell("a", "", 50), cell("b", "", 150)));
    let (a, b) = (rect(&lw, "a"), rect(&lw, "b"));
    assert!(near(a.size.width, 100.0, 0.5), "50 + 200 x 50/200: {a:?}");
    assert!(near(b.size.width, 300.0, 0.5), "150 + 200 x 150/200: {b:?}");
}

#[test]
fn fixed_percentage_and_auto_columns_together() {
    let lw = table(&format!(
        "{}{}{}",
        cell("f", "width: 100px", 10),
        cell("p", "width: 50%", 10),
        cell("a", "", 10)
    ));
    let (f, p, a) = (rect(&lw, "f"), rect(&lw, "p"), rect(&lw, "a"));
    assert!(near(p.size.width, 200.0, 0.5), "50% of 400: {p:?}");
    assert!(near(f.size.width, 100.0, 0.5), "the fixed column: {f:?}");
    assert!(
        near(a.size.width, 100.0, 0.5),
        "the auto column, the rest: {a:?}"
    );
}

#[test]
fn a_narrow_table_gives_every_column_at_least_its_min_content() {
    // 400px are not enough for 3 x 200px fixed columns with 150px content:
    // each keeps its min-content and the rest is shared towards the fixed
    // widths (min-content-specified guess interpolation).
    let lw = table(&format!(
        "{}{}{}",
        cell("a", "width: 200px", 100),
        cell("b", "width: 200px", 100),
        cell("c", "width: 200px", 100)
    ));
    for id in ["a", "b", "c"] {
        let r = rect(&lw, id);
        assert!(
            r.size.width >= 99.5,
            "#{id} keeps its 100px min-content: {r:?}"
        );
        assert!(
            near(r.size.width, 400.0 / 3.0, 0.5),
            "#{id} gets a third: {r:?}"
        );
    }
}

#[test]
fn an_auto_table_with_a_percentage_column_is_wide_enough_for_the_percentage() {
    // CSS Tables 3 3.9.1: a 50% column holding 100px makes an auto table at
    // least 200px wide, so that 100px can be its 50%.
    let lw = body(&format!(
        "<div style=\"width: 600px\"><table id=\"t\" style=\"border-spacing: 0\"><tr>{}{}</tr></table></div>",
        cell("p", "width: 50%", 100),
        cell("a", "", 20)
    ));
    let t = rect(&lw, "t");
    assert!(near(t.size.width, 200.0, 0.5), "100px at 50%: {t:?}");
    assert!(
        near(rect(&lw, "p").size.width, 100.0, 0.5),
        "the 50% column"
    );
}
