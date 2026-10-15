//! An inline box's border widths, paddings and margins resolve their units
//! and follow `zoom`, like a block's (wave-9 N.4).
//!
//! The border widths were read as raw numbers (`2em` was 2px, `15pt` was
//! 15px), and none of the edges followed the box's zoom: a zoomed paragraph
//! scaled its text but left its spans' paddings at their declared size.

use crate::painted::{painted, Painted, RED};

const STYLE: &str = "body { margin: 0; font-size: 16px; } p { margin: 0; } \
                     .ib { display: inline-block; width: 20px; height: 20px; background: red; }";

fn page(body: &str) -> Painted {
    painted(
        &format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>"),
        400,
        120,
    )
}

/// How far the red inline-block after the span moves when the span gets
/// `decoration` (the same page without it as the reference), in `wrapper`.
fn shift(wrapper: &str, decoration: &str) -> i64 {
    let left = |span_style: &str| {
        let p = page(&format!(
            "<p style=\"{wrapper}\">X<span style=\"{span_style}\">Y</span><i class=\"ib\"></i></p>"
        ));
        p.bounds_of(RED, 10).expect("the red inline-block is painted").0 as i64
    };
    left(decoration) - left("")
}

#[test]
fn an_em_border_is_as_wide_as_its_font_size_says() {
    let moved = shift("", "font-size: 20px; border-left: 0.5em solid blue;");
    assert!(
        (moved - 10).abs() <= 1,
        "0.5em of a 20px font is 10px, not the raw 0.5: moved {moved}px"
    );
}

#[test]
fn a_point_border_is_converted_to_pixels() {
    let moved = shift("", "border-left: 15pt solid blue;");
    assert!(
        (moved - 20).abs() <= 1,
        "15pt is 20px, not the raw 15: moved {moved}px"
    );
}

#[test]
fn a_zoomed_spans_padding_is_zoomed_with_its_text() {
    let moved = shift("zoom: 2;", "padding-left: 10px;");
    assert!(
        (moved - 20).abs() <= 1,
        "10px of padding under zoom 2 is 20px on the screen: moved {moved}px"
    );
}
