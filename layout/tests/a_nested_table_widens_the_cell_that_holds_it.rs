//! A nested table widens the cell that holds it.
//!
//! A table inside a table cell is block-level content of that cell: its
//! min- and max-content widths are the cell's (CSS 2.1 17.5.2.2 measures a
//! cell by its content, whatever kind), so the outer column is at least as
//! wide as the inner table's minimum and the outer table grows with it.
//! Newsletters nest tables three or four deep (`width="100%"` wrapper,
//! `width="600"` container, `width="100%"` row tables with `width="50%"`
//! cells); every level has to see the one below it.
//!
//! The bug: the table's intrinsic sizes measured each cell as an inline
//! formatting context (`calculate_ifc_root_intrinsic_sizes`), and a cell
//! whose content is a table has no inline content, so the outer table's
//! min- and max-content ignored the inner table and a shrink-to-fit outer
//! table came out narrower than the table it holds.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{block, body, near, rect, right};

#[test]
fn an_auto_outer_table_is_as_wide_as_the_table_in_its_cell() {
    let lw = body(&format!(
        "<div style=\"display: inline-block\" id=\"wrap\">\
         <table id=\"outer\" style=\"border-spacing: 0\"><tr><td id=\"oc\" style=\"padding: 5px\">\
         <table id=\"inner\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">{}</td></tr></table>\
         </td></tr></table></div>",
        block(300)
    ));
    let (outer, oc, inner) = (rect(&lw, "outer"), rect(&lw, "oc"), rect(&lw, "inner"));
    assert!(
        near(inner.size.width, 300.0, 0.5),
        "the inner table: {inner:?}"
    );
    assert!(
        near(oc.size.width, 310.0, 0.5),
        "the outer cell is the inner table plus its own padding, once: {oc:?}"
    );
    assert!(
        near(outer.size.width, 310.0, 0.5),
        "the outer table: {outer:?}"
    );
    assert!(
        inner.origin.x >= oc.origin.x + 4.5 && right(&inner) <= right(&oc) - 4.5,
        "the inner table sits inside the outer cell's padding: {inner:?} in {oc:?}"
    );
    let wrap = rect(&lw, "wrap");
    assert!(
        near(wrap.size.width, 310.0, 0.5),
        "a shrink-to-fit parent wraps the outer table: {wrap:?}"
    );
}

#[test]
fn a_newsletter_nests_four_tables_deep_and_keeps_every_width() {
    let half = |id: &str| {
        format!(
            "<td id=\"{id}\" width=\"50%\" style=\"padding: 0\">{}</td>",
            block(40)
        )
    };
    let lw = body(&format!(
        "<table id=\"wrapper\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr>\
         <td align=\"center\" style=\"padding: 20px 10px\">\
           <table id=\"container\" width=\"600\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr>\
           <td style=\"padding: 24px\">\
             <table id=\"row\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr>\
             {}{}\
             </tr></table>\
           </td></tr></table>\
         </td></tr></table>",
        half("left"),
        half("right")
    ));
    let wrapper = rect(&lw, "wrapper");
    assert!(
        near(wrapper.size.width, 800.0, 0.5),
        "the wrapper fills the window: {wrapper:?}"
    );
    let container = rect(&lw, "container");
    assert!(
        near(container.size.width, 600.0, 0.5),
        "the container keeps 600: {container:?}"
    );
    let row = rect(&lw, "row");
    assert!(
        near(row.size.width, 552.0, 0.5),
        "the row table fills the container cell's content box (600 - 2 x 24): {row:?}"
    );
    let (l, r) = (rect(&lw, "left"), rect(&lw, "right"));
    assert!(near(l.size.width, 276.0, 0.5), "50% of 552: {l:?}");
    assert!(near(r.size.width, 276.0, 0.5), "50% of 552: {r:?}");
    assert!(
        near(r.origin.x, right(&l), 0.5),
        "side by side: {l:?} {r:?}"
    );
}
