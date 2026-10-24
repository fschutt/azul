//! A `vertical-align` length in viewport units resolves against the viewport
//! (CSS 2.2 s10.8.1: a `<length>` raises the box by that distance; CSS Values
//! 4: 1vh is 1% of the viewport's height). Found by TEXT7 (wave 7), handed
//! to LAYOUT7: `getters::get_vertical_align_for_node` had no viewport, so
//! `vertical-align: 5vh` raised the box by 5px (the bare number), and a rem
//! was taken against the element's own font size.
//!
//! In an 800 x 600 window 5vh is 30px.
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

#[test]
fn a_box_raised_by_5vh_sits_30px_above_its_neighbour() {
    let lw = body(
        "<div style=\"line-height: 100px\"><i id=\"flat\" style=\"display: inline-block; width: \
         10px; height: 10px\"></i><i id=\"raised\" style=\"display: inline-block; width: 10px; \
         height: 10px; vertical-align: 5vh\"></i></div>",
    );
    let flat = rect(&lw, "flat");
    let raised = rect(&lw, "raised");
    assert!(
        near(flat.origin.y - raised.origin.y, 30.0, 0.5),
        "5vh of a 600px viewport is 30px: flat {flat:?}, raised {raised:?}"
    );
}

#[test]
fn a_box_raised_by_1rem_takes_the_root_font_size() {
    let lw = body(
        "<div style=\"line-height: 100px; font-size: 40px\"><i id=\"flat\" style=\"display: \
         inline-block; width: 10px; height: 10px\"></i><i id=\"raised\" style=\"display: \
         inline-block; width: 10px; height: 10px; vertical-align: 1rem\"></i></div>",
    );
    let flat = rect(&lw, "flat");
    let raised = rect(&lw, "raised");
    assert!(
        near(flat.origin.y - raised.origin.y, 16.0, 0.5),
        "1rem is the root's 16px, not the element's 40px: flat {flat:?}, raised {raised:?}"
    );
}
