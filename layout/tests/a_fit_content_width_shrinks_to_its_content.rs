//! `width: fit-content` shrinks to its content (LAYOUT7 item 4; MAIL6: a
//! `width: fit-content; min-width: 100%` paper around a 600px table was
//! 524px, Chrome 624 - `fit-content` acted like the available width).
//!
//! CSS Sizing 3 s3.2: the fit-content size is min(max-content, max(
//! min-content, stretch-fit)), the stretch-fit size being the available space
//! less the box's margins, borders and padding. The keyword never parsed
//! (only the `fit-content(<length-percentage>)` function did): the
//! declaration was dropped and the box was as wide as `auto` makes it. The
//! intrinsic-size keywords (`min-content`, `max-content`, `fit-content`) on a
//! block also need the block's own intrinsic sizes, which the static-DOM
//! short-circuit of the intrinsic pass skipped: `width: max-content` was 0px.
//!
//! Measured in headless Chrome 154 (a 500px container; fixed-size inline
//! boxes, so the numbers do not depend on fonts).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect, words};

#[test]
fn a_fit_content_box_is_as_wide_as_its_content() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"box\" style=\"width: fit-content; padding: \
         12px\"><i style=\"display: inline-block; width: 50px; height: 10px\"></i></div></div>",
    );
    let r = rect(&lw, "box");
    assert!(
        near(r.size.width, 74.0, 0.5),
        "50px of content and 24px of padding (Chrome 74), not the container's width: {r:?}"
    );
}

#[test]
fn a_fit_content_box_around_wide_content_is_as_wide_as_the_content() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"paper\" style=\"width: fit-content; min-width: \
         100%; padding: 12px\"><div style=\"width: 600px; height: 10px\"></div></div></div>",
    );
    let r = rect(&lw, "paper");
    assert!(
        near(r.size.width, 624.0, 0.5),
        "the 600px content and 24px of padding (Chrome 624): {r:?}"
    );
}

#[test]
fn a_fit_content_box_of_wrapping_content_fills_the_available_width() {
    let lw = body(&format!(
        "<div style=\"width: 500px\"><div id=\"box\" style=\"width: fit-content; padding: \
         12px\">{}</div></div>",
        words(6, 100)
    ));
    let r = rect(&lw, "box");
    assert!(
        near(r.size.width, 500.0, 0.5),
        "min-content 100px < available 476px < max-content: the content box is 476px, the \
         border box 500px (Chrome 500), not 524: {r:?}"
    );
}

#[test]
fn a_max_content_block_is_as_wide_as_its_content() {
    let lw = body(
        "<div style=\"width: 500px\"><div id=\"box\" style=\"width: max-content\"><i \
         style=\"display: inline-block; width: 50px; height: 10px\"></i></div></div>",
    );
    let r = rect(&lw, "box");
    assert!(
        near(r.size.width, 50.0, 0.5),
        "its max-content width (Chrome 50), not 0: {r:?}"
    );
}
