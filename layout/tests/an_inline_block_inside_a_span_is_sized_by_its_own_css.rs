//! An inline-block inside an inline span is laid out exactly as it is
//! without the span: an inline box is a transparent wrapper (CSS Inline 3
//! s2), so the atomic inline in it takes its used size from its own CSS -
//! width, height, padding, border - against the IFC root's content box, and
//! its place on the line from the line layout.
//!
//! `collect_inline_span_recursive` sized it from its max-content width
//! alone (no width, padding or border), took its content height as its
//! height, and never recorded it for positioning.

use crate::table_markup::{body, near, rect};

/// 120 x 30 content box, 6px / 4px padding, 2px border: a 136 x 42 border box.
const BOX: &str = "<i id=\"ib\" style=\"display: inline-block; width: 120px; height: 30px; \
                   padding: 4px 6px; border: 2px solid black\">x</i>";

#[test]
fn an_inline_block_inside_a_span_takes_its_css_width_and_height() {
    let lw = body(&format!("<p>ab <span>cd {BOX} ef</span> gh</p>"));
    let r = rect(&lw, "ib");
    assert!(near(r.size.width, 136.0, 0.5), "the border box is 136px wide: {r:?}");
    assert!(near(r.size.height, 42.0, 0.5), "the border box is 42px tall: {r:?}");
}

#[test]
fn an_inline_block_in_nested_spans_takes_its_css_width_and_height() {
    let lw = body(&format!("<p><span>ab <b>cd {BOX}</b></span></p>"));
    let r = rect(&lw, "ib");
    assert!(near(r.size.width, 136.0, 0.5), "the border box is 136px wide: {r:?}");
    assert!(near(r.size.height, 42.0, 0.5), "the border box is 42px tall: {r:?}");
}

#[test]
fn an_inline_block_inside_a_span_sits_where_it_sits_without_the_span() {
    let in_span = rect(&body(&format!("<p>ab <span>cd {BOX} ef</span> gh</p>")), "ib");
    let alone = rect(&body(&format!("<p>ab cd {BOX} ef gh</p>")), "ib");
    for (got, want, what) in [
        (in_span.origin.x, alone.origin.x, "x"),
        (in_span.origin.y, alone.origin.y, "y"),
        (in_span.size.width, alone.size.width, "width"),
        (in_span.size.height, alone.size.height, "height"),
    ] {
        assert!(
            near(got, want, 0.5),
            "{what}: {got} inside the span, {want} without it: {in_span:?} vs {alone:?}"
        );
    }
}
