//! AzReview's ink, drawn as AzPaint draws its canvas: the engine's brush into a
//! frame the page's layer keeps, a stroke being drawn adding only its new
//! segments, the frame drawn anew only when the kept strokes of its page
//! change. No window.

use azul::image::{RawImage, RawImageData};

use crate::{
    ink::{self, InkRevisions, Layer, LiveProgress},
    model::{InkPoint, Semantic, Stroke},
};

const SIZE: (u32, u32) = (100, 100);

fn point(x: f32, y: f32) -> InkPoint {
    ink::point_from(x, y, 0.0, 0.0, 0.0)
}

fn stroke(id: u64, semantic: Semantic, points: &[(f32, f32)]) -> Stroke {
    Stroke {
        page: 0,
        semantic,
        points: points.iter().map(|&(x, y)| point(x, y)).collect(),
        id,
        epoch: 0,
    }
}

/// The alpha of the pixel at (`x`, `y`) of an RGBA frame.
fn alpha(img: &RawImage, x: usize, y: usize) -> u8 {
    match &img.pixels {
        RawImageData::U8(v) => v.as_ref()[(y * img.width + x) * 4 + 3],
        _ => panic!("the frame is 8-bit RGBA"),
    }
}

#[test]
fn a_highlighter_stroke_goes_under_the_code_and_a_pen_stroke_over_it() {
    assert_eq!(Layer::of(Semantic::Scope), Layer::Marker);
    for semantic in [
        Semantic::Issue,
        Semantic::Question,
        Semantic::Duplicate,
        Semantic::Praise,
    ] {
        assert_eq!(Layer::of(semantic), Layer::Pen, "{semantic:?}");
    }
}

#[test]
fn a_stroke_being_drawn_adds_only_its_new_segments_to_the_frame() {
    let mut live = stroke(
        1,
        Semantic::Issue,
        &[(10.0, 10.0), (20.0, 10.0), (30.0, 10.0)],
    );
    let mut shown = LiveProgress::default();
    let mut segments: Vec<(f32, f32)> = Vec::new();

    assert!(shown.paint(Some(&live), 1.0, &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert_eq!(
        segments,
        vec![(10.0, 20.0), (20.0, 30.0)],
        "the first frame draws the stroke so far"
    );

    live.points.push(point(40.0, 10.0));
    live.points.push(point(50.0, 10.0));
    segments.clear();
    assert!(shown.paint(Some(&live), 1.0, &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert_eq!(
        segments,
        vec![(30.0, 40.0), (40.0, 50.0)],
        "the next frame only what was added"
    );

    segments.clear();
    assert!(!shown.paint(Some(&live), 1.0, &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert!(segments.is_empty(), "nothing added, nothing drawn");
}

#[test]
fn a_new_stroke_is_drawn_from_its_first_point() {
    let mut shown = LiveProgress::default();
    let first = stroke(1, Semantic::Issue, &[(10.0, 10.0), (20.0, 10.0)]);
    assert!(shown.paint(Some(&first), 1.0, &mut |_, _, _, _, _| {}));

    let dab = stroke(2, Semantic::Issue, &[(60.0, 60.0)]);
    let mut dabs: Vec<(f32, f32, f32, f32)> = Vec::new();
    assert!(shown.paint(Some(&dab), 1.0, &mut |x0, y0, x1, y1, _| dabs
        .push((x0, y0, x1, y1))));
    assert_eq!(
        dabs,
        vec![(60.0, 60.0, 60.0, 60.0)],
        "a stroke of one point is a dab"
    );
}

#[test]
fn a_page_is_drawn_anew_only_when_its_kept_strokes_change() {
    let mut revisions = InkRevisions::default();
    let (page0, page1) = (revisions.key(0, SIZE), revisions.key(1, SIZE));

    revisions.touch(1);

    assert_eq!(revisions.key(0, SIZE), page0, "page 0 keeps its frame");
    assert_ne!(revisions.key(1, SIZE), page1, "page 1 is drawn anew");
    assert_ne!(
        revisions.key(0, (120, 100)),
        page0,
        "a layer of another size is drawn anew"
    );
}

#[test]
fn opening_another_file_draws_every_page_anew() {
    let mut revisions = InkRevisions::default();
    revisions.touch(0);
    let (touched, untouched) = (revisions.key(0, SIZE), revisions.key(3, SIZE));

    revisions.open();

    assert_ne!(revisions.key(0, SIZE), touched);
    assert_ne!(revisions.key(3, SIZE), untouched);
}

#[test]
fn the_marker_lies_under_the_letters_in_its_colour_washed_into_the_paper() {
    let brush = ink::brush(Semantic::Scope, 0.5);
    let ink_colour = Semantic::Scope.color();
    assert_eq!(brush.color.a, 255, "opaque: the code is drawn over it");
    let washed = |c: u8| 255.0 - (255.0 - f32::from(c)) * ink::MARKER_ALPHA;
    for (got, of) in [
        (brush.color.r, ink_colour.r),
        (brush.color.g, ink_colour.g),
        (brush.color.b, ink_colour.b),
    ] {
        assert!(
            (f32::from(got) - washed(of)).abs() <= 1.0,
            "{got} vs {}",
            washed(of)
        );
    }
}

#[test]
fn a_pen_pressed_hard_draws_wider_than_one_touched_lightly() {
    assert!(ink::brush(Semantic::Issue, 1.0).radius > ink::brush(Semantic::Issue, 0.1).radius);
}

#[test]
fn a_frame_drawn_on_the_cpu_has_ink_along_the_stroke_and_none_elsewhere() {
    let strokes = vec![stroke(1, Semantic::Issue, &[(10.0, 50.0), (90.0, 50.0)])];

    let img = ink::draw_cpu(SIZE.0, SIZE.1, 1.0, &strokes);

    assert!(alpha(&img, 50, 50) > 200, "the stroke's middle is inked");
    assert_eq!(alpha(&img, 50, 10), 0, "far from it the paper shows");
}

// ---- the display's pixels: a layer is rasterised at the window's device pixel ratio ----

#[test]
fn a_layer_is_rasterised_at_the_display_scale() {
    assert_eq!(ink::raster_scale(2.0), 2.0);
    assert_eq!(ink::raster_scale(1.0), 1.0);
    assert_eq!(ink::raster_scale(1.5), 1.5);
    assert_eq!(ink::raster_size((1000.0, 651.0), 2.0), (2000, 1302));
    assert_eq!(ink::raster_size((1000.0, 651.0), 1.0), (1000, 651));
    assert_eq!(ink::raster_size((1000.0, 651.5), 1.5), (1500, 977), "whole pixels, rounded");
}

#[test]
fn a_scale_that_is_no_scale_is_one_and_a_huge_one_is_capped() {
    for bad in [0.0, -2.0, 0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(ink::raster_scale(bad), 1.0, "{bad}");
    }
    assert_eq!(ink::raster_scale(9.0), ink::MAX_RASTER_SCALE);
}

#[test]
fn a_frame_is_never_less_than_one_pixel() {
    assert_eq!(ink::raster_size((0.0, 0.2), 2.0), (1, 1));
}

#[test]
fn the_nib_is_as_wide_on_the_screen_at_any_scale() {
    let at_one = ink::brush(Semantic::Issue, 1.0);
    let at_two = ink::brush_at(at_one, 2.0);
    assert!((at_two.radius - 2.0 * at_one.radius).abs() < 1.0e-4, "{} vs {}", at_two.radius, at_one.radius);
    assert_eq!((at_two.hardness, at_two.flow, at_two.spacing), (at_one.hardness, at_one.flow, at_one.spacing));
}

#[test]
fn a_stroke_painted_at_two_times_covers_twice_the_distance_in_pixels_and_is_twice_as_thick() {
    let strokes = vec![stroke(1, Semantic::Issue, &[(10.0, 50.0), (90.0, 50.0)])];
    let one = ink::draw_cpu(100, 100, 1.0, &strokes);
    let two = ink::draw_cpu(200, 200, 2.0, &strokes);

    assert!(alpha(&two, 100, 100) > 200, "the middle of the stroke is at twice its place");
    assert_eq!(alpha(&two, 100, 20), 0, "and far from it the paper shows");
    let thickness = |img: &RawImage, x: usize| (0..img.height).filter(|y| alpha(img, x, *y) > 128).count();
    let (t1, t2) = (thickness(&one, 50), thickness(&two, 100));
    assert!(t1 >= 2, "a stroke has a body: {t1}");
    assert!(t2 + 1 >= 2 * t1 && t2 <= 2 * t1 + 2, "{t1} px at 1x, {t2} px at 2x");
    let length = |img: &RawImage, y: usize| (0..img.width).filter(|x| alpha(img, *x, y) > 128).count();
    let (l1, l2) = (length(&one, 50), length(&two, 100));
    assert!(l2 + 4 >= 2 * l1 && l2 <= 2 * l1 + 4, "{l1} px long at 1x, {l2} px at 2x");
}

#[test]
fn the_stroke_being_drawn_is_painted_in_the_pixels_of_the_frame() {
    let live = stroke(3, Semantic::Issue, &[(10.0, 10.0), (20.0, 12.0), (30.0, 14.0)]);
    let mut shown = LiveProgress::default();
    let mut segments = Vec::new();
    assert!(shown.paint(Some(&live), 2.0, &mut |x0, y0, x1, y1, b| segments.push((x0, y0, x1, y1, b.radius))));
    assert_eq!(segments.len(), 2);
    assert_eq!((segments[0].0, segments[0].1, segments[0].2, segments[0].3), (20.0, 20.0, 40.0, 24.0));
    assert!(segments[0].4 > 0.0);
}

#[test]
fn a_fresh_layer_is_cleared_to_nothing_not_to_transparent_white() {
    // The frame is composited as premultiplied alpha: a colour above the alpha adds light. A layer cleared
    // to (255, 255, 255, 0) washed the page under the pen's layer white.
    let c = ink::CLEAR;
    assert_eq!(c.a, 0);
    assert!(c.r <= c.a && c.g <= c.a && c.b <= c.a, "premultiplied: {c:?}");
}
