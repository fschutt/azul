//! A line keeps off an exclusion over the whole height of its line box.
//!
//! An exclusion takes from a line whatever of the line box it covers, at any
//! height of the box (CSS Exclusions 1, CSS Shapes 1: the float area cuts
//! the line boxes it overlaps). Polygons, paths, circles and ellipses were
//! read at the line's vertical centre only - rectangles over the whole box -
//! so text ran over a slanted or round exclusion near its top and bottom.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use azul_css::{shape::ShapeReference, shape_parser::parse_shape};
use azul_layout::text3::cache::{Point, ShapeBoundary};

use crate::mono_lines::{box_of_width, glyph_boxes, lay_out, GlyphBox};

/// The text every paragraph here is set in.
const TEXT: &str = "text text text text text text text text text text text text text text text \
                    text text text text text text text text text text text text text text text \
                    text text text text text text text text text text text text text text text";

/// The shape `css` resolved against a 400 x 200 box at the origin.
fn shape(css: &str) -> ShapeBoundary {
    let parsed = parse_shape(css).expect("the shape parses");
    let reference = ShapeReference::of_box(400.0, 200.0);
    ShapeBoundary::from_css_shape(&parsed, Point::default(), &reference, &mut None)
}

/// `TEXT` in a 400px box around `exclusion`.
fn around(exclusion: &str) -> Vec<GlyphBox> {
    let mut paragraph = box_of_width(400.0);
    paragraph.shape_exclusions = vec![shape(exclusion).into()];
    let glyphs = glyph_boxes(&lay_out(TEXT, &paragraph));
    assert!(glyphs.len() > 100, "the paragraph is laid out: {glyphs:?}");
    glyphs
}

#[test]
fn a_polygon_between_two_line_centres_still_cuts_both_lines() {
    // The rectangle 100..300 x 10..25 as a polygon: it reaches into line 0
    // (0..20) below its centre and into line 1 (20..40) above its centre.
    let glyphs = around("polygon(100px 10px, 300px 10px, 300px 25px, 100px 25px)");
    for g in glyphs.iter().filter(|g| g.top < 25.0 && g.bottom > 10.0) {
        assert!(
            g.right <= 100.0 + 0.01 || g.left >= 300.0 - 0.01,
            "{:?} on line {} at x {}..{} y {}..{} runs over the polygon",
            g.text,
            g.line,
            g.left,
            g.right,
            g.top,
            g.bottom
        );
    }
}

#[test]
fn a_polygon_cuts_the_lines_the_same_rectangle_cuts() {
    let as_polygon = around("polygon(100px 10px, 300px 10px, 300px 25px, 100px 25px)");
    let as_rectangle = around("inset(10px 100px 175px 100px)");
    assert_eq!(
        as_polygon, as_rectangle,
        "a polygon and an inset of the same rectangle place every glyph alike"
    );
}

#[test]
fn a_circle_keeps_every_glyph_box_outside_it() {
    let glyphs = around("circle(60px at 200px 60px)");
    for g in &glyphs {
        // The point of the glyph's box nearest the centre.
        let nearest_x = 200.0_f32.clamp(g.left, g.right);
        let nearest_y = 60.0_f32.clamp(g.top, g.bottom);
        let distance = (nearest_x - 200.0).hypot(nearest_y - 60.0);
        assert!(
            distance >= 60.0 - 0.01,
            "{:?} on line {} at x {}..{} y {}..{} reaches {distance}px from the centre of a 60px \
             circle",
            g.text,
            g.line,
            g.left,
            g.right,
            g.top,
            g.bottom
        );
    }
}
