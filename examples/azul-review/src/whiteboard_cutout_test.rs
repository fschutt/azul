//! The picture of a cutout: pixels put together on the CPU (paper, marker, code, pen, cut to the
//! loop), and the plan of what is drawn at which scale. No window, no azul.

use super::*;
use crate::whiteboard::{Limits, Notice, LIMITS};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

const WHITE: [u8; 4] = [255, 255, 255, 255];

fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    vec![Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)]
}

#[test]
fn a_raster_is_as_many_bytes_as_its_size_says_and_a_wrong_one_is_refused() {
    assert!(Raster::from_premultiplied(2, 2, vec![0; 16]).is_some());
    assert!(Raster::from_premultiplied(2, 2, vec![0; 15]).is_none());
    assert!(Raster::from_premultiplied(0, 2, vec![]).is_none());
    assert!(Raster::from_straight(2, 1, vec![0; 7]).is_none());
    let r = Raster::new(3, 2);
    assert_eq!((r.w, r.h, r.px.len()), (3, 2, 24));
    assert_eq!(r.get(1, 1), [0, 0, 0, 0]);
}

#[test]
fn a_colour_with_alpha_is_stored_multiplied_by_it_and_read_back_straight() {
    let r = Raster::filled(1, 1, [200, 100, 50, 128]);
    assert_eq!(r.get(0, 0), [100, 50, 25, 128]);
    let straight = r.to_straight();
    assert!((i32::from(straight[0]) - 200).abs() <= 1 && (i32::from(straight[1]) - 100).abs() <= 1, "{straight:?}");
    assert_eq!(straight[3], 128);
    // Nothing: a fully transparent pixel has no colour.
    assert_eq!(Raster::filled(1, 1, [9, 9, 9, 0]).to_straight(), vec![0, 0, 0, 0]);
}

#[test]
fn a_picture_survives_the_trip_through_straight_alpha_as_it_goes_to_a_png() {
    let mut px = Vec::new();
    for a in [0_u8, 1, 40, 128, 200, 255] {
        for c in [0_u8, 77, 255] {
            let c = (u16::from(c) * u16::from(a) / 255) as u8;
            px.extend_from_slice(&[c, c, c, a]);
        }
    }
    let n = (px.len() / 4) as u32;
    let r = Raster::from_premultiplied(n, 1, px.clone()).expect("a raster");
    let back = Raster::from_straight(n, 1, r.to_straight()).expect("a raster");
    for (i, (a, b)) in px.iter().zip(&back.px).enumerate() {
        let alpha = px[i / 4 * 4 + 3];
        let allowed = if alpha >= 128 { 1 } else { 3 };
        assert!((i32::from(*a) - i32::from(*b)).abs() <= allowed, "byte {i}: {a} became {b} at alpha {alpha}");
    }
}

#[test]
fn something_over_nothing_is_itself_and_something_opaque_hides_what_is_under_it() {
    let mut under = Raster::filled(2, 1, WHITE);
    let over = Raster::from_premultiplied(2, 1, vec![0, 0, 0, 0, 10, 20, 30, 255]).expect("a raster");
    under.over(&over, 0, 0);
    assert_eq!(under.get(0, 0), WHITE, "a clear pixel changes nothing");
    assert_eq!(under.get(1, 0), [10, 20, 30, 255]);
}

#[test]
fn half_covering_ink_mixes_with_the_paper() {
    let mut paper = Raster::filled(1, 1, WHITE);
    // Red at 50%: colour 128 0 0 already multiplied.
    let ink = Raster::from_premultiplied(1, 1, vec![128, 0, 0, 128]).expect("a raster");
    paper.over(&ink, 0, 0);
    let p = paper.get(0, 0);
    assert_eq!(p[3], 255);
    assert!(p[0] >= 253 && (126..=128).contains(&p[1]) && (126..=128).contains(&p[2]), "{p:?}");
}

#[test]
fn a_raster_put_over_another_at_an_offset_is_clipped_to_it() {
    let mut under = Raster::filled(4, 4, WHITE);
    let patch = Raster::filled(3, 3, [0, 0, 0, 255]);
    under.over(&patch, 2, 2);
    assert_eq!(under.get(1, 1), WHITE);
    assert_eq!(under.get(2, 2), [0, 0, 0, 255]);
    assert_eq!(under.get(3, 3), [0, 0, 0, 255]);
    under.over(&patch, -2, -2);
    assert_eq!(under.get(0, 0), [0, 0, 0, 255]);
    assert_eq!(under.get(1, 1), WHITE, "only the one pixel of the patch reached in");
    under.over(&patch, 100, -100);
}

#[test]
fn the_loop_cuts_the_picture_out_with_a_soft_edge() {
    let mut r = Raster::filled(40, 40, WHITE);
    r.mask_polygon(&square(10.0, 10.0, 30.0, 30.0), 1.0);
    assert_eq!(r.get(20, 20), WHITE, "inside, untouched");
    assert_eq!(r.get(2, 2)[3], 0, "outside, gone");
    assert_eq!(r.get(37, 20)[3], 0);
    // The edge pixels are in between, and the cut is where the loop is.
    let edge = r.get(10, 20)[3];
    assert!(edge > 0 && edge < 255, "{edge}");
    let total: f32 = r.px.chunks_exact(4).map(|p| f32::from(p[3]) / 255.0).sum();
    assert!((total - 400.0).abs() < 8.0, "the cutout is as large as the loop: {total}");
}

#[test]
fn a_loop_with_no_feather_still_cuts_cleanly_and_one_that_leaves_the_picture_is_clipped() {
    let mut r = Raster::filled(20, 20, WHITE);
    r.mask_polygon(&square(5.0, 5.0, 15.0, 15.0), 0.0);
    assert_eq!(r.get(5, 5), WHITE);
    assert_eq!(r.get(14, 14), WHITE);
    assert_eq!(r.get(4, 4)[3], 0);
    assert_eq!(r.get(15, 15)[3], 0);

    let mut big = Raster::filled(10, 10, WHITE);
    big.mask_polygon(&square(-50.0, -50.0, 80.0, 80.0), 1.0);
    assert_eq!(big.get(0, 0), WHITE);
    assert_eq!(big.get(9, 9), WHITE);

    // No polygon, no cut: the picture is left as it is.
    let mut same = Raster::filled(4, 4, WHITE);
    same.mask_polygon(&[], 1.0);
    assert_eq!(same, Raster::filled(4, 4, WHITE));
}

#[test]
fn a_concave_loop_leaves_its_notch_out() {
    let l = vec![
        Pt::new(0.0, 0.0),
        Pt::new(20.0, 0.0),
        Pt::new(20.0, 20.0),
        Pt::new(40.0, 20.0),
        Pt::new(40.0, 40.0),
        Pt::new(0.0, 40.0),
    ];
    let mut r = Raster::filled(40, 40, WHITE);
    r.mask_polygon(&l, 0.0);
    assert_eq!(r.get(10, 10), WHITE);
    assert_eq!(r.get(30, 30), WHITE);
    assert_eq!(r.get(30, 10)[3], 0, "the notch");
}

#[test]
fn a_crop_is_the_part_asked_for_and_stays_inside_the_picture() {
    let mut r = Raster::new(4, 3);
    r.over(&Raster::filled(1, 1, [9, 9, 9, 255]), 2, 1);
    let c = r.cropped(1, 1, 2, 2);
    assert_eq!((c.w, c.h), (2, 2));
    assert_eq!(c.get(1, 0), [9, 9, 9, 255]);
    let far = r.cropped(3, 2, 10, 10);
    assert_eq!((far.w, far.h), (1, 1), "clipped at the picture's edge");
    assert_eq!(r.cropped(9, 9, 2, 2).w, 0, "nothing of it is in the picture");
}

#[test]
fn what_is_drawn_is_the_part_that_is_not_clear() {
    let mut r = Raster::new(10, 10);
    assert_eq!(r.alpha_box(), None);
    r.over(&Raster::filled(2, 3, [1, 1, 1, 255]), 4, 5);
    assert_eq!(r.alpha_box(), Some((4, 5, 2, 3)));
}

// ---- the plan

fn page() -> Size {
    Size::new(1000.0, 730.0)
}

#[test]
fn a_loop_is_drawn_at_the_scale_of_the_screen_in_whole_pixels_with_a_margin_for_its_edge() {
    let outline = square(300.0, 200.0, 400.0, 240.0);
    let p = plan(&outline, 2.0, page(), &LIMITS).expect("a plan");
    assert!(close(p.scale, 2.0));
    // The loop's box and PAD around it, on the grid of device pixels.
    assert!(close(p.region.x, 300.0 - PAD) && close(p.region.y, 200.0 - PAD), "{:?}", p.region);
    assert!(close(p.region.w, 100.0 + 2.0 * PAD) && close(p.region.h, 40.0 + 2.0 * PAD), "{:?}", p.region);
    assert_eq!((p.px_w, p.px_h), (212, 92));
    assert!(close(p.logical.w, 106.0) && close(p.logical.h, 46.0));
    // The outline in the picture's own pixels.
    assert!(close(p.outline[0].x, PAD * 2.0) && close(p.outline[0].y, PAD * 2.0), "{:?}", p.outline[0]);
    assert!(close(p.outline[2].x, (400.0 - p.region.x) * 2.0));
    assert!(p.notice.is_none());
}

#[test]
fn the_region_is_on_the_grid_of_the_device_pixels_so_the_text_falls_as_it_does_on_the_page() {
    // A box starting at a third of a pixel: at 2x the grid is every half pixel.
    let outline = square(300.33, 200.1, 400.9, 240.7);
    let p = plan(&outline, 2.0, page(), &LIMITS).expect("a plan");
    for v in [p.region.x, p.region.y] {
        let device = v * 2.0;
        assert!((device - device.round()).abs() < 1e-3, "{v} is not on the device grid");
    }
    assert!(p.region.x <= 300.33 - PAD + 1e-3 && p.region.right() >= 400.9 + PAD - 1e-3, "{:?}", p.region);
    assert!((p.px_w as f32 - p.region.w * 2.0).abs() < 1e-3);
}

#[test]
fn the_region_stays_on_the_page() {
    let outline = square(2.0, 2.0, 60.0, 40.0);
    let p = plan(&outline, 2.0, page(), &LIMITS).expect("a plan");
    assert!(p.region.x >= 0.0 && p.region.y >= 0.0, "{:?}", p.region);
    let corner = square(960.0, 700.0, 998.0, 729.0);
    let p = plan(&corner, 2.0, page(), &LIMITS).expect("a plan");
    assert!(p.region.right() <= 1000.0 + 1e-3 && p.region.bottom() <= 730.0 + 1e-3, "{:?}", p.region);
}

#[test]
fn a_large_loop_is_drawn_at_a_smaller_scale_to_fit_and_the_board_says_so() {
    // The whole page at 2x is 2.9 million pixels; a picture may have 1.5 million.
    let outline = square(0.0, 0.0, 1000.0, 730.0);
    let p = plan(&outline, 2.0, page(), &LIMITS).expect("a plan");
    assert!(p.scale < 2.0 && p.scale >= 1.0, "{}", p.scale);
    assert!(u64::from(p.px_w) * u64::from(p.px_h) <= LIMITS.max_image_px, "{} x {}", p.px_w, p.px_h);
    assert!(matches!(p.notice, Some(Notice::ScaleLowered { wanted_pct: 200, used_pct }) if used_pct < 200 && used_pct >= 100), "{:?}", p.notice);
    // The size on the board is the same as at 2x: only the sharpness is less.
    assert!((p.logical.w - 1000.0).abs() < 8.0, "{:?}", p.logical);
}

#[test]
fn a_loop_that_cannot_be_drawn_in_the_room_there_is_is_refused() {
    let tight = Limits { max_image_px: 10_000, ..LIMITS };
    let outline = square(0.0, 0.0, 1000.0, 730.0);
    assert!(matches!(plan(&outline, 2.0, page(), &tight), Err(Notice::PictureTooBig { .. })));
}

#[test]
fn the_scale_is_one_a_picture_can_have() {
    let outline = square(300.0, 200.0, 400.0, 240.0);
    assert!(close(plan(&outline, 0.0, page(), &LIMITS).expect("a plan").scale, MIN_SCALE));
    assert!(close(plan(&outline, 50.0, page(), &LIMITS).expect("a plan").scale, MAX_SCALE));
    assert!(close(plan(&outline, f32::NAN, page(), &LIMITS).expect("a plan").scale, 1.0));
    assert!(plan(&[], 2.0, page(), &LIMITS).is_err(), "no outline, no picture");
}

// ---- putting the layers together

fn layer(w: u32, h: u32, colour: [u8; 4]) -> Raster {
    Raster::filled(w, h, colour)
}

#[test]
fn the_layers_go_paper_then_marker_then_code_then_pen_and_the_loop_cuts_them_out() {
    let outline = square(10.0, 10.0, 90.0, 50.0);
    let p = plan(&outline, 1.0, Size::new(200.0, 100.0), &LIMITS).expect("a plan");
    let (w, h) = (p.px_w, p.px_h);
    let mut marker = Raster::new(w, h);
    marker.over(&layer(40, 10, [255, 220, 120, 255]), 10, 10);
    let mut code = Raster::new(w, h);
    code.over(&layer(10, 10, [0, 0, 0, 255]), 12, 12);
    let mut pen = Raster::new(w, h);
    pen.over(&layer(6, 6, [214, 45, 32, 255]), 14, 14);
    let picture = compose(&p, WHITE, Some(&marker), &code, Some(&pen)).expect("composed");
    assert_eq!((picture.w, picture.h), (w, h));
    // The region begins PAD before the loop; the layers are in the picture's pixels.
    let at = |x: u32, y: u32| picture.get(x, y);
    assert_eq!(at(15, 15), [214, 45, 32, 255], "the pen is on top of the code");
    assert_eq!(at(12, 12), [0, 0, 0, 255], "the code is over the marker");
    assert_eq!(at(30, 15), [255, 220, 120, 255], "the marker is over the paper");
    assert_eq!(at(60, 40), WHITE, "the paper where nothing is");
    assert_eq!(at(1, 1)[3], 0, "outside the loop nothing");
}

#[test]
fn layers_that_are_not_the_size_of_the_plan_are_refused_not_stretched() {
    let outline = square(10.0, 10.0, 90.0, 50.0);
    let p = plan(&outline, 1.0, Size::new(200.0, 100.0), &LIMITS).expect("a plan");
    let wrong = Raster::new(p.px_w + 1, p.px_h);
    assert!(compose(&p, WHITE, None, &wrong, None).is_err());
    let right = Raster::new(p.px_w, p.px_h);
    assert!(compose(&p, WHITE, Some(&wrong), &right, None).is_err());
    assert!(compose(&p, WHITE, None, &right, None).is_ok());
}

#[test]
fn a_picture_becomes_a_cutout_image_with_its_scale_and_its_png() {
    let outline = square(10.0, 10.0, 90.0, 50.0);
    let p = plan(&outline, 2.0, Size::new(200.0, 100.0), &LIMITS).expect("a plan");
    let code = Raster::filled(p.px_w, p.px_h, [0, 0, 0, 255]);
    let picture = compose(&p, WHITE, None, &code, None).expect("composed");
    let image = picture.into_image(p.scale, vec![137, 80, 78, 71]);
    assert!(image.is_consistent());
    assert_eq!((image.width, image.height), (p.px_w, p.px_h));
    assert!(close(image.scale, 2.0));
    assert_eq!(image.png, vec![137, 80, 78, 71]);
}
