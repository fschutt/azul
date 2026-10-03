//! `<hr>` is drawn as the HTML Standard's UA stylesheet draws it (15.3.11 "The
//! hr element": `border-style: inset; border-width: 1px; margin-block: 0.5em;
//! margin-inline: auto`) - a 2px rule (the top and the bottom border, no
//! height), as wide as its block (`width` auto), centred when it is given a
//! width.
//!
//! Azul's UA sheet drew only the top border, 1px tall, at `width: 100%`: every
//! `<hr>` of a mail 1px short (04_receipt), an `<hr>` with a left margin (the
//! rich text editor's quoted rule) as wide as its block AND shifted right, so
//! it overflowed by its margin, and a narrowed one not centred. User ruling
//! 2026-10-03: Chrome is the reference (MAILREF8 E2).
//!
//! Chrome 154 (scripts/refci probe): h1 800 x 2 (azul 800 x 1); a 20px left
//! margin in a 300px block 280 wide at x 20 (azul 300); `width: 50%` in a
//! 400px block 202 wide (200 + the two side borders) at x 99 (azul 200 at
//! x 0). An author's `border: none; border-top: 1px solid` keeps 1px (pin).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

#[test]
fn an_hr_is_two_pixels_tall_and_as_wide_as_its_block() {
    let lw = body("<hr id=\"h1\"/>");
    let h1 = rect(&lw, "h1");
    assert!(
        near(h1.size.height, 2.0, 0.01),
        "a 1px border on top and bottom, no height (Chrome 2): {h1:?}"
    );
    assert!(near(h1.size.width, 800.0, 0.5), "the block's width: {h1:?}");
}

#[test]
fn an_hr_with_a_side_margin_fits_its_block() {
    let lw = body("<div style=\"width: 300px\"><hr id=\"h2\" style=\"margin-left: 20px\"/></div>");
    let h2 = rect(&lw, "h2");
    assert!(
        near(h2.origin.x, 20.0, 0.5) && near(h2.size.width, 280.0, 0.5),
        "width auto: the block less the margin (Chrome x 20, 280 wide), not 100% beside it: \
         {h2:?}"
    );
}

#[test]
fn a_narrowed_hr_is_centred() {
    let lw = body("<div style=\"width: 400px\"><hr id=\"h3\" style=\"width: 50%\"/></div>");
    let h3 = rect(&lw, "h3");
    assert!(
        near(h3.origin.x, 99.0, 0.5) && near(h3.size.width, 202.0, 0.5),
        "margin-inline auto centres it; its side borders add 2px (Chrome x 99, 202 wide): \
         {h3:?}"
    );
}

#[test]
fn an_authors_single_border_keeps_the_rule_one_pixel() {
    let lw = body(
        "<div style=\"width: 400px\"><hr id=\"h4\" style=\"border: none; border-top: 1px solid \
         black\"/></div>",
    );
    let h4 = rect(&lw, "h4");
    assert!(
        near(h4.size.height, 1.0, 0.01) && near(h4.size.width, 400.0, 0.5),
        "the author's border wins (Chrome 400 x 1): {h4:?}"
    );
}
