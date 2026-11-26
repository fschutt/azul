//! The picture of a cutout drawn by azul's CPU renderer at 2x: the lines of a page, the page's ink,
//! cut along a loop. Needs azul and a monospace font (the runner has DejaVu), no window.

use azul::{font::FontCacheSnapshot, image::ImageCacheSnapshot};

use super::*;
use crate::{
    model::{InkPoint, Semantic, Stroke},
    whiteboard::LIMITS,
    whiteboard_archive::decode_png,
    whiteboard_cutout::plan,
    whiteboard_geom::{Pt, Size},
};

fn lines() -> Vec<String> {
    (1..=46).map(|i| format!("pub fn step_{i:03}(x: u32) -> u32 {{ x + {i} }}")).collect()
}

fn stroke(semantic: Semantic, y: f32, id: u64) -> Stroke {
    let p = |x: f32| InkPoint { x, y, pressure: 0.6, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 };
    Stroke { page: 0, semantic, points: vec![p(120.0), p(200.0), p(280.0)], id, epoch: 0, started_unix_ms: 0 }
}

/// A loop (a box, as a lasso's outline can be) round lines 3 to 5 (y 62 to 107) and the first 40 characters.
fn outline() -> Vec<Pt> {
    vec![Pt::new(80.0, 61.0), Pt::new(360.0, 61.0), Pt::new(360.0, 108.0), Pt::new(80.0, 108.0)]
}

fn request(scale: f32, strokes: Vec<Stroke>) -> RenderRequest {
    let plan = plan(&outline(), scale, Size::new(1000.0, 730.0), &LIMITS).expect("a plan");
    RenderRequest {
        plan,
        first_number: 1,
        lines: lines(),
        strokes,
        fonts: FontCacheSnapshot::empty(),
        images: ImageCacheSnapshot::empty(),
    }
}

#[test]
fn the_shared_numbers_are_the_pages() {
    assert!((LINE_H - 15.0).abs() < f32::EPSILON, "the lines of a page are 15 px tall");
    assert!((PAGE_PAD - 32.0).abs() < f32::EPSILON, "the code begins 32 px in");
}

#[test]
fn the_code_is_drawn_at_two_times_and_cut_along_the_loop() {
    let req = request(2.0, vec![]);
    let (w, h) = (req.plan.px_w, req.plan.px_h);
    let image = render(req).expect("drawn");
    assert_eq!((image.width, image.height), (w, h), "the plan's size, in device pixels");
    assert!(image.is_consistent());
    assert!((image.scale - 2.0).abs() < 1e-6);
    let at = |x: u32, y: u32| {
        let i = ((y * image.width + x) * 4) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2], image.rgba[i + 3]]
    };
    assert_eq!(at(0, 0)[3], 0, "outside the loop: clear");
    assert_eq!(at(image.width - 1, image.height - 1)[3], 0);
    // Inside the loop: the paper, and the code on it. Text is dark (#1f1f1f); at 2x its stems are whole
    // pixels wide, so some pixels are as dark as the ink is.
    let inside = |x: u32, y: u32| at(x, y)[3] == 255;
    let (mut dark, mut paper) = (0, 0);
    for y in 0..image.height {
        for x in 0..image.width {
            if !inside(x, y) {
                continue;
            }
            let p = at(x, y);
            if p[0] < 80 && p[1] < 80 {
                dark += 1;
            }
            if p[0] > 245 && p[1] > 245 && p[2] > 245 {
                paper += 1;
            }
        }
    }
    assert!(dark > 150, "the lines of code are drawn: {dark} dark pixels");
    assert!(paper > dark, "most of a page is paper: {paper} against {dark}");
}

#[test]
fn the_ink_of_the_page_is_painted_in_its_colour_over_the_code() {
    let issue = Semantic::Issue.color();
    let marker = Semantic::Scope.color();
    let req = request(2.0, vec![stroke(Semantic::Issue, 75.0, 1), stroke(Semantic::Scope, 90.0, 2)]);
    let region = req.plan.region;
    let scale = req.plan.scale;
    let image = render(req).expect("drawn");
    let px = |x: f32, y: f32| {
        let (ix, iy) = (((x - region.x) * scale) as u32, ((y - region.y) * scale) as u32);
        let i = ((iy * image.width + ix) * 4) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2], image.rgba[i + 3]]
    };
    // The middle of the pen's stroke, at 2x: red, as the engine's brush paints the issue's colour.
    let pen = px(200.0, 75.0);
    assert!(pen[3] == 255 && pen[0] > 150 && i32::from(pen[0]) - i32::from(pen[2]) > 80, "pen {pen:?} against {issue:?}");
    // The marker is a pink wash on the paper, behind the code: pixels of it all along its stroke, wherever
    // a letter is not in the way.
    let washed = (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let i = ((y * image.width + x) * 4) as usize;
            let p = &image.rgba[i..i + 4];
            p[3] == 255 && p[0] > 235 && p[1] > 150 && p[1] < 215 && p[2] > 190 && i32::from(p[0]) - i32::from(p[1]) > 40
        })
        .count();
    assert!(washed > 300, "the marker's wash ({marker:?} on white): {washed} pixels");
    // The nib is as wide on the page as on the screen: about 3 px of pen at 1x is about 6 device px at 2x.
    let across = (0..40)
        .filter(|d| {
            let p = px(200.0, 70.0 + *d as f32 * 0.25);
            p[0] > 150 && i32::from(p[0]) - i32::from(p[2]) > 80
        })
        .count();
    assert!(across >= 6 && across <= 24, "the pen's line is {across} quarter-pixels thick");
}

#[test]
fn the_picture_has_its_png_and_it_reads_back_at_the_same_size() {
    let req = request(2.0, vec![]);
    let (w, h) = (req.plan.px_w, req.plan.px_h);
    let image = render(req).expect("drawn");
    assert!(!image.png.is_empty());
    let back = decode_png(&image.png).expect("a PNG");
    assert_eq!((back.width, back.height), (w, h));
}

#[test]
fn at_one_times_the_picture_is_half_the_pixels_of_two_times_and_the_same_size_on_the_board() {
    let (one, two) = (request(1.0, vec![]), request(2.0, vec![]));
    assert_eq!((one.plan.px_w * 2, one.plan.px_h * 2), (two.plan.px_w, two.plan.px_h));
    assert!((one.plan.logical.w - two.plan.logical.w).abs() < 1e-3);
    let image = render(one).expect("drawn");
    assert!(image.is_consistent());
}

#[test]
fn a_render_on_a_thread_answers_with_one_message() {
    let mut msg = RefAny::new(Some(Rendered { id: 4, image: Err("no font".to_string()) }));
    let got = take_rendered(&mut msg).expect("an answer");
    assert_eq!(got.id, 4);
    assert!(got.image.is_err());
    assert!(take_rendered(&mut msg).is_none(), "taken once");
    assert!(take_rendered(&mut RefAny::new(1_u8)).is_none());
}
