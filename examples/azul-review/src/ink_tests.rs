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

    assert!(shown.paint(Some(&live), &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert_eq!(
        segments,
        vec![(10.0, 20.0), (20.0, 30.0)],
        "the first frame draws the stroke so far"
    );

    live.points.push(point(40.0, 10.0));
    live.points.push(point(50.0, 10.0));
    segments.clear();
    assert!(shown.paint(Some(&live), &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert_eq!(
        segments,
        vec![(30.0, 40.0), (40.0, 50.0)],
        "the next frame only what was added"
    );

    segments.clear();
    assert!(!shown.paint(Some(&live), &mut |x0, _, x1, _, _| segments.push((x0, x1))));
    assert!(segments.is_empty(), "nothing added, nothing drawn");
}

#[test]
fn a_new_stroke_is_drawn_from_its_first_point() {
    let mut shown = LiveProgress::default();
    let first = stroke(1, Semantic::Issue, &[(10.0, 10.0), (20.0, 10.0)]);
    assert!(shown.paint(Some(&first), &mut |_, _, _, _, _| {}));

    let dab = stroke(2, Semantic::Issue, &[(60.0, 60.0)]);
    let mut dabs: Vec<(f32, f32, f32, f32)> = Vec::new();
    assert!(shown.paint(Some(&dab), &mut |x0, y0, x1, y1, _| dabs
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

    let img = ink::draw_cpu(SIZE.0, SIZE.1, &strokes);

    assert!(alpha(&img, 50, 50) > 200, "the stroke's middle is inked");
    assert_eq!(alpha(&img, 50, 10), 0, "far from it the paper shows");
}
