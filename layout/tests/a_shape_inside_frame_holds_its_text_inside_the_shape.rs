//! A box with `shape-inside` lays its text out inside the shape.
//!
//! CSS Shapes 2: `shape-inside` takes the area outside the shape away from
//! the element's content area, and each line box is fitted into what is
//! left. The line breaker got no room at all for a shaped box (the branch
//! that should have turned the shape into the room of each line was empty),
//! so every line was skipped and a box with `shape-inside` drew no text.
//!
//! Glyphs here are Azul Mock Mono at 20px: each one 10px wide, each line box
//! 20px tall, so a glyph's box can be checked against the shape exactly.

use azul_css::shape_parser::parse_shape;
use azul_layout::text3::cache::{Rect, ShapeBoundary};

use crate::{
    mono_lines::{box_of_width, glyph_boxes, inked_pens, lay_out, GLYPH, LINE},
    table_markup::laid_out,
};

/// The text every frame here is filled with.
const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                     tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                     quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                     consequat.";

/// The shape `css` resolved against a `width` x `height` box at the origin.
fn shape(css: &str, width: f32, height: f32) -> ShapeBoundary {
    let parsed = parse_shape(css).expect("the shape parses");
    let reference = Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    };
    ShapeBoundary::from_css_shape(&parsed, reference, &mut None)
}

#[test]
fn every_glyph_in_a_round_frame_lies_inside_the_circle() {
    let mut frame = box_of_width(200.0);
    frame.shape_boundaries = vec![shape("circle(100px at 100px 100px)", 200.0, 200.0)];
    let layout = lay_out(LOREM, &frame);
    let glyphs = glyph_boxes(&layout);
    assert!(
        glyphs.len() > 40,
        "the circle holds lines of text: {} glyphs",
        glyphs.len()
    );
    let inside = |x: f32, y: f32| {
        let (dx, dy) = (x - 100.0, y - 100.0);
        dx.hypot(dy) <= 100.0 + 0.01
    };
    for g in glyphs.iter().filter(|g| g.bottom <= 200.0) {
        assert!(
            inside(g.left, g.top)
                && inside(g.right, g.top)
                && inside(g.left, g.bottom)
                && inside(g.right, g.bottom),
            "{:?} on line {} spans x {}..{} y {}..{}, outside the circle",
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
fn a_triangular_frame_fills_from_its_wide_top_down_to_its_tip() {
    // A triangle pointing down: at height y the shape spans
    // 0.625 y .. 300 - 0.625 y.
    let mut frame = box_of_width(300.0);
    frame.shape_boundaries = vec![shape("polygon(0px 0px, 300px 0px, 150px 240px)", 300.0, 240.0)];
    let layout = lay_out(LOREM, &frame);
    let glyphs = glyph_boxes(&layout);
    let first_line: Vec<_> = glyphs.iter().filter(|g| g.line == 0).collect();
    assert!(
        !first_line.is_empty() && first_line[0].top.abs() < 0.01,
        "the first line starts at the top of the frame: {first_line:?}"
    );
    for g in glyphs.iter().filter(|g| g.bottom <= 240.0) {
        // The narrowest part of the band is its bottom edge.
        let (left, right) = (0.625 * g.bottom, 300.0 - 0.625 * g.bottom);
        assert!(
            g.left >= left - 0.01 && g.right <= right + 0.01,
            "{:?} on line {} spans {}..{}, the triangle only {left}..{right} at y {}",
            g.text,
            g.line,
            g.left,
            g.right,
            g.bottom
        );
    }
}

#[test]
fn a_frame_with_shape_inside_draws_all_of_its_text() {
    // R1 of the fragmentainer audit, in pixels of Azul Mock Mono: the
    // triangle holds ten lines, 129 letters; a word too long for the room
    // left at its tip moves down past the box, where the shape cuts nothing,
    // and the rest follows it there across the box's width.
    let html = format!(
        "<html><head><style>body {{ margin: 0 }} .f {{ position: absolute; left: 40px; top: \
         40px; width: 300px; height: 240px; font-family: 'Azul Mock Mono'; font-size: 20px; \
         line-height: 20px; shape-inside: polygon(0px 0px, 300px 0px, 150px 240px) }}\
         </style></head><body><div class=\"f\">{LOREM}</div></body></html>"
    );
    let lw = laid_out(&html, 800.0, 600.0);
    let pens = inked_pens(&lw);
    let letters = LOREM.chars().filter(|c| !c.is_whitespace()).count();
    assert_eq!(
        pens.len(),
        letters,
        "every letter is drawn, inside the triangle or below the box"
    );
    let mut inside_the_box = 0;
    for &(x, baseline) in &pens {
        // The pen stands on the baseline, 16px below the line box's top.
        let (left, top) = (x - 40.0, baseline - 16.0 - 40.0);
        let bottom = top + LINE;
        if bottom <= 240.0 + 0.01 {
            inside_the_box += 1;
            let (l, r) = (0.625 * bottom, 300.0 - 0.625 * bottom);
            assert!(
                left >= l - 0.01 && left + GLYPH <= r + 0.01,
                "a glyph at x {left} y {top}..{bottom} is outside the triangle ({l}..{r})"
            );
        } else {
            assert!(
                top >= 240.0 - 0.01 && left >= -0.01 && left + GLYPH <= 300.0 + 0.01,
                "a glyph past the triangle overflows below the box, across its width: x {left} y \
                 {top}"
            );
        }
    }
    assert!(
        inside_the_box > 100,
        "the triangle holds its ten lines: {inside_the_box} letters"
    );
}
