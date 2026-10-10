//! A `height: 100%` inline-block in an auto-height body - AzMail's paper - is
//! as tall as its content, and so is a `height: 100%` block inside it (CSS 2.2
//! 10.5: a percentage height against a containing block whose height depends
//! on its content computes to `auto`).
//!
//! MAILENG6 made the block case content-sized, but two paths still used the
//! sizing pass's pre-layout ESTIMATE (`intrinsic.max_content_height`) as the
//! height: the atomic-inline measurement (`fc::measure_atomic_inline` kept
//! `tentative_size.height` for every non-`auto` height) and the floor
//! `apply_content_based_height` keeps (`max(placeholder, content)`), which for
//! a percentage was that estimate instead of the 0 an `auto` block gets. The
//! inline-block's percentage child then resolved against the estimate as if
//! it were definite. The estimate is too tall where it counts a clipped
//! preheader's text (418 for 400) and far too short where a table's text
//! wraps (24 for 114): the Cerberus papers ended 372 / 622 / 632 px above the
//! mails' ends (scripts/refci/mail_boxes.py, MAILREF8 group A), their
//! backgrounds with them.
//!
//! Chrome 154 (scripts/refci probe): case 1 p / b 400; case 2 p / b 114 = the
//! table. Font-independent: case 2 compares with the table's own height. Not
//! compiled by the author (house rule); RED before the fix (azul 418 / 24).

use crate::table_markup::{body, near, rect};

#[test]
fn a_clipped_preheader_does_not_make_the_paper_taller() {
    let lw = body(
        "<div id=\"p\" style=\"display: inline-block; width: 300px; height: 100%\">\
         <div id=\"b\" style=\"height: 100%\">\
         <div style=\"max-height: 0; overflow: hidden\">hidden text</div>\
         <div style=\"height: 400px\"></div></div></div>",
    );
    let p = rect(&lw, "p");
    let b = rect(&lw, "b");
    assert!(
        near(p.size.height, 400.0, 0.5),
        "the paper's 100% computes to auto: as tall as its content, 400px (Chrome 400), not \
         the estimate that counted the clipped text: {p:?}"
    );
    assert!(
        near(b.size.height, 400.0, 0.5),
        "and the 100% child inside it resolves against no definite height either: {b:?}"
    );
}

#[test]
fn a_wrapping_table_makes_the_paper_as_tall_as_the_table() {
    let lw = body(
        "<div id=\"p\" style=\"display: inline-block; width: 300px; height: 100%\">\
         <div id=\"b\" style=\"height: 100%\"><table id=\"t\" style=\"width: 100%\"><tr><td>\
         Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
         incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
         exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.\
         </td></tr></table></div></div>",
    );
    let p = rect(&lw, "p");
    let b = rect(&lw, "b");
    let t = rect(&lw, "t");
    assert!(
        t.size.height > 60.0,
        "the prose wraps onto several lines in a 300px table: {t:?}"
    );
    assert!(
        near(b.size.height, t.size.height, 0.5),
        "the 100% block is as tall as the table it holds (Chrome 114 / 114), not the one-line \
         estimate: b {b:?}, t {t:?}"
    );
    assert!(
        near(p.size.height, t.size.height, 0.5),
        "and so is the paper (Chrome 114): p {p:?}, t {t:?}"
    );
}
