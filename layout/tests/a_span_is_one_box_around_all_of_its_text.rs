//! A span is one inline box around all of its text, the text of the spans
//! nested in it included (CSS 2.2 s9.4.2, s10.8; CSS Inline 3).
//!
//! The line used to see only the innermost span of each piece of text: a
//! `<b>` inside a padded span closed the span's padding before itself and
//! opened it again after, the span's background stopped under the bold word,
//! and an outer span with no text of its own painted nothing and moved
//! nothing (WPT inline-formatting-context-002: its black stripe left of the
//! inner span is the outer span's background under the inner span's
//! margin). A span the wrap splits has no padding or border at the split.
//!
//! The text is transparent where a test reads the background, so the
//! pixels do not depend on the machine's fonts.

use crate::painted::{painted, Painted, BLUE, RED};

const STYLE: &str = "body { margin: 0; font-size: 16px; } p { margin: 0; } \
                     .ib { display: inline-block; width: 20px; height: 20px; background: red; } \
                     .clear { color: rgba(0, 0, 0, 0); }";

fn page(body: &str) -> Painted {
    painted(
        &format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>"),
        300,
        120,
    )
}

/// The red inline-block's left edge in `page`.
fn red_left(page: &Painted) -> i64 {
    page.bounds_of(RED, 10)
        .expect("the red inline-block is painted")
        .0 as i64
}

#[test]
fn a_spans_padding_is_added_once_around_a_bold_word_inside_it() {
    let plain = page("<p>X<span>a<b>b</b>c</span><i class=\"ib\"></i></p>");
    let padded = page(
        "<p>X<span style=\"padding-left: 30px; padding-right: 30px\">a<b>b</b>c</span>\
         <i class=\"ib\"></i></p>",
    );
    let shift = red_left(&padded) - red_left(&plain);
    assert!(
        (shift - 60).abs() <= 1,
        "30px before the span's text and 30px after it, not around each piece of it: moved \
         {shift}px"
    );
}

#[test]
fn an_outer_spans_padding_pushes_the_text_of_the_span_inside_it() {
    let plain = page("<p>X<span><b>b</b></span><i class=\"ib\"></i></p>");
    let padded = page(
        "<p>X<span style=\"padding-left: 25px\"><b>b</b></span><i class=\"ib\"></i></p>",
    );
    let shift = red_left(&padded) - red_left(&plain);
    assert!(
        (shift - 25).abs() <= 1,
        "the outer span's 25px padding is before the bold word: moved {shift}px"
    );
}

#[test]
fn a_spans_background_runs_on_under_a_bold_word_inside_it() {
    let p = page(
        "<p class=\"clear\"><span style=\"background: blue\">aaa <b>bbb</b> ccc</span></p>",
    );
    let (x0, y0, x1, y1) = p.bounds_of(BLUE, 10).expect("the span's background is painted");
    let area = (x1 - x0) * (y1 - y0);
    let blue = p.count((x0, y0, x1, y1), BLUE, 10);
    assert!(
        x1 - x0 > 40,
        "the background spans the three words ({x0}..{x1})"
    );
    assert_eq!(
        blue, area,
        "one unbroken rectangle under all three words, the bold one included"
    );
}

#[test]
fn an_outer_spans_background_shows_under_the_inner_spans_margin() {
    // WPT inline-formatting-context-002 in colour: the outer span has no
    // text of its own; its background covers the inner span's 40px margin.
    let p = page(
        "<p class=\"clear\"><span style=\"background: blue\"><span style=\"margin-left: 40px; \
         background: red\">XXXX</span></span></p>",
    );
    let blue = p.bounds_of(BLUE, 10).expect("the outer span paints its background");
    let red = p.bounds_of(RED, 10).expect("the inner span paints its background");
    assert!(
        (blue.0 as i64).abs() <= 1 && (red.0 as i64 - 40).abs() <= 1,
        "blue from 0 under the margin, red from 40: blue {blue:?}, red {red:?}"
    );
    assert!(blue.2 >= red.2, "the outer box holds the inner one: {blue:?} {red:?}");
}

#[test]
fn a_span_split_by_the_wrap_has_no_padding_at_the_split() {
    // "aaaa" fits beside the padding on the first line, the second word
    // wraps: the second line starts with the span's text, not its padding.
    let p = page(
        "<p class=\"clear\" style=\"width: 60px\"><span style=\"padding-left: 20px; \
         background: blue\">aaaa aaaa</span></p>",
    );
    let lines: Vec<(usize, usize)> = (0..p.height)
        .filter_map(|y| {
            let row: Vec<usize> = (0..p.width).filter(|&x| p.is(x, y, BLUE, 10)).collect();
            Some((*row.first()?, *row.last()? + 1))
        })
        .collect();
    assert!(!lines.is_empty(), "the span paints");
    let first = lines[0];
    let last = *lines.last().expect("rows");
    assert!(first.0 == 0, "the padding starts the first line at x 0: {first:?}");
    assert!(
        last.0 == 0 && last.1 < first.1,
        "the second line starts at x 0 with the text and is shorter than the first (no \
         padding there): first {first:?}, last {last:?}"
    );
}
