//! An inline box paints its border, padding and margin (CSS 2.2 s8, s9.4.2,
//! s10.8; CSS Inline 3).
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, gap E-INLINE): "inline boxes have no
//! box decoration". Probed on the 2026-10-03 prebuilt dylib (WPT8):
//!
//! - a `<span>` with a border or padding but NO background (and no
//!   text-decoration) drew nothing and moved nothing: the intrinsic-sizing
//!   pass collects the span's text with the TEXT node's style (no border), and
//!   the text cache keys its first stage on `StyleProperties`' hash, which left
//!   the border out - so the final layout reused the sizing pass's border-less
//!   items (WPT inline-formatting-context-004, split-inline-borders);
//! - a span's margins were never applied (WPT inline-formatting-context-002);
//! - a span's background was painted twice (a translucent one came out twice
//!   as dark);
//! - a float shrinks to its content without the inline boxes' padding.
//!
//! The lengths are measured as DIFFERENCES between a page and its twin without
//! the decoration, so they do not depend on the machine's fonts.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{close, painted, Painted, BLUE, RED};

const STYLE: &str = "body { margin: 0; font-size: 16px; } p { margin: 0; } \
                     .ib { display: inline-block; width: 20px; height: 20px; background: red; }";

fn page(body: &str) -> Painted {
    painted(
        &format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>"),
        300,
        80,
    )
}

/// The red inline-block's box in `page`.
fn red_box(page: &Painted) -> (usize, usize, usize, usize) {
    page.bounds_of(RED, 10)
        .expect("the red inline-block is painted")
}

#[test]
fn a_span_with_only_a_border_draws_it() {
    let p = page("<p>A <span style=\"border: 5px solid blue\">kkk</span> A</p>");
    let blue = p.count((0, 0, p.width, p.height), BLUE, 10);
    assert!(
        blue > 100,
        "a 5px blue border around the span's text: got {blue} blue pixels"
    );
}

#[test]
fn an_inline_boxs_left_margin_border_and_padding_push_what_follows() {
    let plain = page("<p>X<span>Y</span><i class=\"ib\"></i></p>");
    let decorated = page(
        "<p>X<span style=\"margin-left: 15px; border-left: 10px solid blue; \
         padding-left: 30px\">Y</span><i class=\"ib\"></i></p>",
    );
    let shift = red_box(&decorated).0 as i64 - red_box(&plain).0 as i64;
    assert!(
        (shift - 55).abs() <= 1,
        "15px margin + 10px border + 30px padding before the span's text move the box after it \
         by 55px; moved {shift}px"
    );
}

#[test]
fn an_inline_boxs_right_padding_border_and_margin_push_what_follows() {
    let plain = page("<p>X<span>Y</span><i class=\"ib\"></i></p>");
    let decorated = page(
        "<p>X<span style=\"padding-right: 30px; border-right: 10px solid blue; \
         margin-right: 15px\">Y</span><i class=\"ib\"></i></p>",
    );
    let shift = red_box(&decorated).0 as i64 - red_box(&plain).0 as i64;
    assert!(
        (shift - 55).abs() <= 1,
        "30px padding + 10px border + 15px margin after the span's text move the box after it \
         by 55px; moved {shift}px"
    );
}

#[test]
fn an_inline_boxs_background_covers_its_padding_but_not_its_margin() {
    // The red box is x 0..20; the span's margin is x 20..50, its padding
    // x 50..70, then its text.
    let p = page(
        "<p><i class=\"ib\"></i><span style=\"margin-left: 30px; padding-left: 20px; \
         background: lime\">Y</span></p>",
    );
    let (_, y0, x1, y1) = red_box(&p);
    assert_eq!(x1, 20, "the red box ends at x=20");
    let y = (y0 + y1) / 2;
    let lime = (0, 255, 0);
    assert!(
        !close(p.rgb(35, y), lime, 10),
        "the margin (x 20..50) shows no background; (35, {y}) is {:?}",
        p.rgb(35, y)
    );
    assert!(
        close(p.rgb(60, y), lime, 10),
        "the padding (x 50..70) shows the background; (60, {y}) is {:?}",
        p.rgb(60, y)
    );
}

#[test]
fn a_translucent_span_background_is_painted_once() {
    let p = page(
        "<p><span style=\"padding-left: 20px; background: rgba(0, 0, 255, 0.5)\">Y</span>\
         <i class=\"ib\"></i></p>",
    );
    let (_, y0, _, y1) = red_box(&p);
    let y = (y0 + y1) / 2;
    assert!(
        close(p.rgb(10, y), (127, 127, 255), 6),
        "one layer of 50% blue over white in the span's padding; (10, {y}) is {:?}",
        p.rgb(10, y)
    );
}

#[test]
fn a_float_shrinks_to_its_content_with_the_inline_boxes_padding() {
    let width = |body: &str| {
        let p = page(body);
        let (x0, _, x1, _) = p.bounds_of(RED, 10).expect("the red float is painted");
        x1 as i64 - x0 as i64
    };
    let plain = width("<div style=\"float: left; background: red\"><span>Y</span></div>");
    let padded = width(
        "<div style=\"float: left; background: red\"><span style=\"padding-left: 40px\">Y</span>\
         </div>",
    );
    assert!(
        (padded - plain - 40).abs() <= 1,
        "the float is 40px wider with the span's 40px padding; {plain}px -> {padded}px"
    );
}
