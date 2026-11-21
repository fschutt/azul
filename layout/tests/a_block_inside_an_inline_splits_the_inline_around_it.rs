//! A block inside an inline splits the inline around it (CSS 2.2 s9.2.1.1;
//! LAYOUT7 item 2, MAILENG6 "Seen broken": `<a><img style="display: block">`
//! in mail, every linked picture of a newsletter, had no box).
//!
//! "When an inline box contains an in-flow block-level box, the inline box
//! (and its inline ancestors within the same line box) are broken around the
//! block-level box ..., splitting the inline box into two boxes (even if
//! either side is empty), one on each side of the block-level box(es). The
//! line boxes before the break and after the break are enclosed in anonymous
//! block boxes, and the block-level box becomes a sibling of those anonymous
//! boxes."
//!
//! Azul "inlinified" such a block (`fc::collect_inline_span_recursive`'s
//! catch-all arm): its children poured into the line, the block itself no
//! box - `<div><a><span style="display: block; height: 40px">` laid out
//! 19.2px tall, the span without a box (Chrome: 40px).
//!
//! Measured in headless Chrome 154 (16px Arial, `line-height: 20px`).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{laid_out, near, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head></head><body style=\"margin: 0; font-size: 16px; line-height: \
             20px\">{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn a_block_inside_a_link_is_a_block_of_the_links_container() {
    let lw = page(
        "<div id=\"outer\"><a href=\"#\"><span id=\"block\" style=\"display: block; height: \
         40px\"></span></a></div><p id=\"after\" style=\"margin: 0\">x</p>",
    );
    let block = rect(&lw, "block");
    assert!(
        near(block.size.height, 40.0, 0.5) && near(block.size.width, 800.0, 0.5),
        "the block has its own box, as wide as the container (Chrome 800 x 40): {block:?}"
    );
    let outer = rect(&lw, "outer");
    assert!(
        near(outer.size.height, 40.0, 0.5),
        "the container is as tall as the block (Chrome 40px), no line box around it: {outer:?}"
    );
    let after = rect(&lw, "after");
    assert!(
        near(after.origin.y, 40.0, 0.5),
        "the next block follows the block: {after:?}"
    );
}

#[test]
fn the_text_around_a_block_in_an_inline_goes_on_lines_before_and_after_it() {
    let lw = page(
        "<div id=\"outer\" style=\"width: 300px\">before <a href=\"#\">x<div id=\"block\" \
         style=\"height: 30px\">blk</div>y</a> after</div>",
    );
    let block = rect(&lw, "block");
    assert!(
        near(block.origin.y, 20.0, 0.5),
        "\"before x\" is one line above the block (Chrome y 20): {block:?}"
    );
    assert!(
        near(block.size.width, 300.0, 0.5) && near(block.size.height, 30.0, 0.5),
        "the block fills the container's width (Chrome 300 x 30): {block:?}"
    );
    let outer = rect(&lw, "outer");
    assert!(
        near(outer.size.height, 70.0, 0.5),
        "a line, the block, a line (Chrome 70px): {outer:?}"
    );
}

#[test]
fn white_space_around_a_block_in_an_inline_makes_no_lines() {
    let lw = page(
        "<div id=\"outer\">\n <a href=\"#\">\n  <div id=\"block\" style=\"height: 40px\"></div>\n \
         </a>\n</div>",
    );
    let outer = rect(&lw, "outer");
    assert!(
        near(outer.size.height, 40.0, 0.5),
        "collapsible white space beside the block makes no line box (Chrome 40px): {outer:?}"
    );
    let block = rect(&lw, "block");
    assert!(near(block.origin.y, 0.0, 0.5), "{block:?}");
}

#[test]
fn a_block_inside_nested_inlines_splits_all_of_them() {
    let lw = page(
        "<div id=\"outer\"><span><a href=\"#\"><div id=\"block\" style=\"height: 40px; width: \
         100px\"></div></a></span></div>",
    );
    let block = rect(&lw, "block");
    assert!(
        near(block.size.width, 100.0, 0.5) && near(block.size.height, 40.0, 0.5),
        "{block:?}"
    );
    let outer = rect(&lw, "outer");
    assert!(
        near(outer.size.height, 40.0, 0.5),
        "the span and the link both break around the block (Chrome 40px): {outer:?}"
    );
}
