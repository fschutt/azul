//! A shape's lengths are measured in their own units, its percentages
//! against its reference box, and a `path()` takes a fill rule.
//!
//! CSS Shapes 1 (basic shapes): a `<length-percentage>` of a polygon point is
//! a percentage of the reference box's width or height, every CSS length unit
//! counts, and `polygon()` and `path()` take a `<fill-rule>` first. The shape
//! parser read `50%` as 50 pixels, refused `pt` and every other unit but
//! `px`, refused `path(evenodd, "...")` and single quotes, and dropped the
//! fill rule of a polygon, so the same frame written in percentages or points
//! held its text in another place or in no shape at all.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use crate::{
    mono_lines::{inked_pens, GLYPH, LINE},
    table_markup::laid_out,
};

/// The text every frame here is filled with.
const TEXT: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                    tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam.";

/// The glyph pens of `TEXT` in a 300 x 240 frame at (40, 40) with `shape`.
fn pens_in_frame(shape: &str) -> Vec<(f32, f32)> {
    let html = format!(
        "<html><head><style>body {{ margin: 0 }} .f {{ position: absolute; left: 40px; top: \
         40px; width: 300px; height: 240px; font-family: 'Azul Mock Mono'; font-size: 20px; \
         line-height: 20px; shape-inside: {shape} }}</style></head><body><div class=\"f\">\
         {TEXT}</div></body></html>"
    );
    inked_pens(&laid_out(&html, 800.0, 600.0))
}

#[test]
fn a_triangle_in_percentages_or_points_holds_its_text_where_the_one_in_pixels_does() {
    let in_pixels = pens_in_frame("polygon(0px 0px, 300px 0px, 150px 240px)");
    assert!(!in_pixels.is_empty(), "the triangle holds text");
    let in_percent = pens_in_frame("polygon(0% 0%, 100% 0%, 50% 100%)");
    assert_eq!(in_percent, in_pixels, "50% of the 300px box is 150px");
    let in_points = pens_in_frame("polygon(0pt 0pt, 225pt 0pt, 112.5pt 180pt)");
    assert_eq!(in_points, in_pixels, "225pt is 300px");
}

#[test]
fn an_even_odd_path_leaves_its_inner_ring_as_a_hole() {
    // The frame with a 100 x 100 hole at the middle of its top edge.
    let pens = pens_in_frame("path(evenodd, 'M0 0H300V240H0Z M100 0H200V100H100Z')");
    assert!(pens.len() > 40, "the frame holds text around the hole: {pens:?}");
    for &(x, baseline) in &pens {
        let (left, top) = (x - 40.0, baseline - 16.0 - 40.0);
        let (right, bottom) = (left + GLYPH, top + LINE);
        let in_hole = right > 100.0 + 0.01 && left < 200.0 - 0.01 && bottom > 0.01 && top < 100.0;
        assert!(
            !in_hole,
            "a glyph at x {left}..{right} y {top}..{bottom} lies in the hole 100..200 x 0..100"
        );
    }
}

#[test]
fn a_nonzero_path_fills_a_ring_drawn_the_same_way_round() {
    // The inner ring runs the same way as the outer one: under the initial
    // fill rule (nonzero) it is no hole, and text runs through it - inside
    // the outer ring, 250px of the 300px box.
    let pens = pens_in_frame("path('M0 0H250V240H0Z M100 0H200V100H100Z')");
    assert!(!pens.is_empty(), "the path holds text");
    for &(x, _) in &pens {
        assert!(
            x - 40.0 + GLYPH <= 250.0 + 0.01,
            "a glyph at x {} runs past the path's right edge at 250",
            x - 40.0
        );
    }
    let through = pens
        .iter()
        .any(|&(x, baseline)| x - 40.0 >= 100.0 && x - 40.0 < 200.0 && baseline - 56.0 < 100.0);
    assert!(through, "text runs where an even-odd path has its hole: {pens:?}");
}
