//! A float's `shape-margin` grows its float area by that distance all
//! round, never past the float's margin box.
//!
//! CSS Shapes 1, 6.1: the shape margin is the outline of every point the
//! margin's distance away from the shape, round at its corners; it applies
//! to every basic shape, polygons and paths included, and the result is
//! clipped to the margin box like the shape. The margin was read (and added
//! to `exclusion-margin` on the wrong box) and never applied.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::{
    a_floats_shape_outside_shapes_the_text_beside_it::{right_of_circle, WORDS},
    mono_lines::{page_glyphs, GLYPH, LINE},
};

#[test]
fn a_shape_margin_grows_a_round_float_area() {
    let pens = page_glyphs(
        ".c { width: 400px } .fl { float: left; width: 200px; height: 120px; shape-outside: \
         circle(40px at 60px 60px); shape-margin: 20px }",
        &format!("<div class=\"c\"><div class=\"fl\"></div>{WORDS}</div>"),
    );
    for &(left, top) in &pens {
        if let Some(edge) = right_of_circle(60.0, top) {
            assert!(
                left >= edge - 0.01,
                "a glyph at {left} on the line at {top} comes within 20px of the 40px circle"
            );
        }
    }
    // The grown circle ends at x 120: well inside the 200px margin box, so
    // the lines beside it start before the box's edge.
    assert!(
        pens.iter().any(|&(left, top)| top < 100.0 && left < 150.0),
        "the lines beside the shape use the room the margin box leaves: {pens:?}"
    );
}

#[test]
fn a_shape_margin_never_grows_past_the_margin_box() {
    let pens = page_glyphs(
        ".c { width: 400px } .fl { float: left; width: 100px; height: 100px; shape-outside: \
         inset(0px); shape-margin: 50px }",
        &format!("<div class=\"c\"><div class=\"fl\"></div>{WORDS}</div>"),
    );
    let first = pens.first().copied().expect("the text is laid out");
    assert!(
        (first.0 - 100.0).abs() < 0.01 && first.1.abs() < 0.01,
        "the first line starts at the margin box's edge, not 50px further: {first:?}"
    );
    assert!(
        pens.iter().any(|&(_, top)| (100.0..120.0).contains(&top)),
        "and the lines below the float use the whole width again"
    );
}

#[test]
fn a_shape_margin_grows_a_polygon_by_its_distance_all_round() {
    // A triangle in a left float of 300 x 160, grown by 20px: the text on its
    // right keeps 20px from the triangle's edges and corners.
    let pens = page_glyphs(
        ".c { width: 600px } .fl { float: left; width: 300px; height: 160px; shape-outside: \
         polygon(0px 40px, 100px 40px, 50px 100px); shape-margin: 20px }",
        &format!("<div class=\"c\"><div class=\"fl\"></div>{WORDS}</div>"),
    );
    let corners = [(0.0_f32, 40.0_f32), (100.0, 40.0), (50.0, 100.0)];
    let to_segment = |(px, py): (f32, f32), (ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        (px - (ax + t * dx)).hypot(py - (ay + t * dy))
    };
    // The distance between a glyph's box and the triangle: for two convex
    // outlines that do not meet, the nearest points are a corner of one and
    // an edge of the other.
    let distance = |left: f32, top: f32| {
        let (right, bottom) = (left + GLYPH, top + LINE);
        let boxed = [(left, top), (right, top), (right, bottom), (left, bottom)];
        let mut nearest = f32::MAX;
        for i in 0..3 {
            for &corner in &boxed {
                nearest = nearest.min(to_segment(corner, corners[i], corners[(i + 1) % 3]));
            }
        }
        for &vertex in &corners {
            for i in 0..4 {
                nearest = nearest.min(to_segment(vertex, boxed[i], boxed[(i + 1) % 4]));
            }
        }
        nearest
    };
    assert!(!pens.is_empty(), "the text is laid out");
    for &(left, top) in &pens {
        assert!(
            distance(left, top) >= 20.0 - 0.5,
            "a glyph at {left} on the line at {top} comes {} px near the triangle",
            distance(left, top)
        );
    }
    assert!(
        pens.iter().any(|&(left, top)| top < 100.0 && top + LINE > 40.0 && left < 200.0),
        "the lines beside the grown triangle start left of the 300px margin box's edge"
    );
}
