//! A `box-sizing: border-box` min-width bounds the BORDER box, also when the
//! width is auto (LAYOUT7 item 3; MAIL6: AzMail's paper as `display:
//! inline-block; min-width: 100%; box-sizing: border-box; padding: 12px` was
//! 524px wide in a 500px pane, Chrome 500).
//!
//! CSS Box Sizing 3 s3: with `border-box` "the specified width and height
//! (and respective min/max properties) on this element determine the border
//! box". An auto width is a content size (shrink-to-fit, CSS 2.2 s10.3.9 /
//! s10.3.5); azul clamped that CONTENT width with the border-box min-width
//! and then added the padding (`calculate_used_size_for_node`), and the
//! intrinsic floor of a length min-width was a content floor too.
//!
//! Measured in headless Chrome 154 (a 500px container).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

#[test]
fn an_inline_block_with_a_border_box_min_width_of_100_percent_fits_its_container() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"paper\" style=\"display: inline-block; \
         min-width: 100%; box-sizing: border-box; padding: 12px\"><i style=\"display: \
         inline-block; width: 20px; height: 10px\"></i></div></div>",
    );
    let paper = rect(&lw, "paper");
    assert!(
        near(paper.size.width, 500.0, 0.5),
        "min-width: 100% of a border box is 500px with its padding inside (Chrome 500): \
         {paper:?}"
    );
}

#[test]
fn an_inline_block_with_a_border_box_min_width_in_px_is_that_wide() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"box\" style=\"display: inline-block; min-width: \
         300px; box-sizing: border-box; padding: 12px\"><i style=\"display: inline-block; \
         width: 20px; height: 10px\"></i></div></div>",
    );
    let r = rect(&lw, "box");
    assert!(
        near(r.size.width, 300.0, 0.5),
        "a border-box min-width of 300px makes a 300px border box (Chrome 300): {r:?}"
    );
}

#[test]
fn a_float_with_a_border_box_min_width_of_100_percent_fits_its_container() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"float\" style=\"float: left; min-width: 100%; \
         box-sizing: border-box; padding: 12px\"><i style=\"display: inline-block; width: \
         20px; height: 10px\"></i></div></div>",
    );
    let r = rect(&lw, "float");
    assert!(
        near(r.size.width, 500.0, 0.5),
        "the float's border box is 500px (Chrome 500): {r:?}"
    );
}

#[test]
fn a_content_box_min_width_still_bounds_the_content() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"box\" style=\"display: inline-block; min-width: \
         100%; padding: 12px\"><i style=\"display: inline-block; width: 20px; height: \
         10px\"></i></div></div>",
    );
    let r = rect(&lw, "box");
    assert!(
        near(r.size.width, 524.0, 0.5),
        "content-box: the content is 500px, the padding outside it (Chrome 524): {r:?}"
    );
}
