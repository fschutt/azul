//! A flex container's gap in `em` or `rem` is that many font sizes, as in Chrome:
//! `display: flex; font-size: 20px; column-gap: 0.5em` puts its items 10px apart. The compact
//! cache keeps a gap in px only, and a gap in any other unit was laid out as no gap at all
//! (AzDrive's payment popover ran its host into the words beside it).

use azul_layout::window::LayoutWindow;

use crate::table_markup::{laid_out, near, rect};

/// Two 50 x 10 boxes `#a`, `#b` in the flex container `#r` styled `css` (20px font, the root
/// 16px), every margin and padding zeroed.
fn two_boxes(css: &str) -> LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} html {{ font-size: 16px; }} \
             #r {{ display: flex; font-size: 20px; {css} }} \
             #a, #b {{ width: 50px; height: 10px; flex-shrink: 0; }}</style></head>\
             <body><div id=\"r\"><div id=\"a\"></div><div id=\"b\"></div></div></body></html>"
        ),
        400.0,
        300.0,
    )
}

/// The space between `#a`'s end and `#b`'s start: across a row, or down a column.
fn space(lw: &LayoutWindow, column: bool) -> f32 {
    let (a, b) = (rect(lw, "a"), rect(lw, "b"));
    if column {
        b.origin.y - (a.origin.y + a.size.height)
    } else {
        b.origin.x - (a.origin.x + a.size.width)
    }
}

#[test]
fn a_flex_rows_column_gap_in_em_is_that_many_of_its_font_size() {
    let gap = space(&two_boxes("column-gap: 0.5em;"), false);
    assert!(
        near(gap, 10.0, 0.5),
        "column-gap: 0.5em at 20px: {gap} px apart, Chrome 10"
    );
}

#[test]
fn a_flex_rows_column_gap_in_rem_is_that_many_of_the_roots_font_size() {
    let gap = space(&two_boxes("column-gap: 0.25rem;"), false);
    assert!(
        near(gap, 4.0, 0.5),
        "column-gap: 0.25rem at a 16px root: {gap} px apart, Chrome 4"
    );
}

#[test]
fn a_flex_columns_row_gap_and_the_gap_shorthand_in_em_count_too() {
    let gap = space(&two_boxes("flex-direction: column; row-gap: 1em;"), true);
    assert!(
        near(gap, 20.0, 0.5),
        "row-gap: 1em at 20px: {gap} px apart, Chrome 20"
    );
    let gap = space(&two_boxes("gap: 0.5em;"), false);
    assert!(
        near(gap, 10.0, 0.5),
        "gap: 0.5em at 20px: {gap} px apart, Chrome 10"
    );
}

#[test]
fn a_gap_in_px_stays_as_it_is() {
    let gap = space(&two_boxes("column-gap: 7px;"), false);
    assert!(near(gap, 7.0, 0.5), "column-gap: 7px: {gap} px apart");
}
