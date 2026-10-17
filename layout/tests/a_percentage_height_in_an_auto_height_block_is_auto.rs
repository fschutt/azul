//! A percentage `height` against a containing block whose height depends on
//! its content computes to `auto` (CSS 2.2 10.5; MAILENG6 item 2).
//!
//! Mail templates write `body { height: 100% }` (AzMail maps it onto the
//! paper `div`). In Chrome (standards mode, the `<!DOCTYPE html>` page
//! scripts/refci/mail_boxes.py renders) the paper's containing block - the
//! auto-height `<body>` - has no definite height, so the paper is as tall as
//! its content and its background reaches the end of the mail. Azul resolved
//! the percentage against the height it had inherited from the viewport
//! (`layout_bfc`'s `children_containing_block_size`, the auto-height branch),
//! so the paper was exactly one window tall and the mail ran on below its
//! background (mailgun x3, postmark x3, cerberus: the paper's azr-1/2).
//!
//! What stays: a definite parent (`height: 300px`), a flex item (its size is
//! its container's - CSS Flexbox 9.8), the root's own `height: 100%` against
//! the viewport.
//! Not compiled by the author (house rule); the first test is RED before the
//! fix, the others pin what must not move.

use crate::table_markup::{laid_out, near, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!("<html><head></head><body style=\"margin: 0\">{body}</body></html>"),
        800.0,
        600.0,
    )
}

#[test]
fn a_height_100_percent_box_in_an_auto_height_parent_is_as_tall_as_its_content() {
    let lw = page(
        "<div id=\"outer\"><div id=\"paper\" style=\"height: 100%; background: \
         rgb(240, 240, 240)\"><div style=\"height: 2000px\"></div></div></div>",
    );
    let paper = rect(&lw, "paper");
    assert!(
        near(paper.size.height, 2000.0, 0.5),
        "the containing block's height depends on its content, so `height: 100%` is auto: \
         the paper is as tall as the mail (2000px, as in Chrome), not the 600px window: \
         {paper:?}"
    );
    let outer = rect(&lw, "outer");
    assert!(
        near(outer.size.height, 2000.0, 0.5),
        "and its parent with it: {outer:?}"
    );
}

#[test]
fn a_percentage_height_against_a_definite_parent_still_resolves() {
    let lw =
        page("<div style=\"height: 300px\"><div id=\"half\" style=\"height: 50%\"></div></div>");
    let half = rect(&lw, "half");
    assert!(
        near(half.size.height, 150.0, 0.5),
        "50% of a 300px parent is 150px: {half:?}"
    );
}

#[test]
fn a_percentage_height_inside_a_stretched_flex_item_still_resolves() {
    let lw = page(
        "<div style=\"display: flex; height: 300px\"><div style=\"width: 100px\">\
         <div id=\"fill\" style=\"height: 100%\"></div></div></div>",
    );
    let fill = rect(&lw, "fill");
    assert!(
        near(fill.size.height, 300.0, 0.5),
        "a stretched flex item's height is definite (CSS Flexbox 9.8), its child's 100% is \
         300px: {fill:?}"
    );
}
