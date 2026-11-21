//! A10, the present path and DPI.
//!
//! The arithmetic every shell's CPU present shares (`window::present_geometry`,
//! `FrameDamage::to_rects_physical`): which device pixels a logical damage
//! rect covers, how a rect is flipped between the renderer's rows (top row
//! first) and a y-up window system, how large the frame of a window is. The
//! findings these guard are in `audit/A10.md`.

#![cfg(feature = "text_layout")]

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

use crate::window::{
    present_geometry::{
        area_px, bounding_rect, flip_rect_y, frame_size_px, logical_len_for_px,
        rows_of_bottom_left_rect, surface_len_px, PxRect,
    },
    FrameDamage,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize {
            width: w,
            height: h,
        },
    }
}

/// The strip a pan step repaints (the audit's example): 13 x 1402 logical px
/// at (1267, 20) of a 1280 x 1440 window.
const STRIP: (f32, f32, f32, f32) = (1267.0, 20.0, 13.0, 1402.0);

/// A10-04 (checked: fine). Logical damage becomes device pixels by `floor`
/// of the near edge and `ceil` of the far edge, times the DPI factor.
///
/// The strip at DPI 1 in a 1280 x 1440 buffer: (1267, 20, 13, 1402). At DPI 2
/// in a 2560 x 2880 buffer: (2534, 40, 26, 2804), four times the pixels.
#[test]
fn a_logical_damage_rect_in_device_pixels_at_dpi_1_and_2() {
    let damage = FrameDamage::Rects(vec![rect(STRIP.0, STRIP.1, STRIP.2, STRIP.3)]);
    assert_eq!(
        damage.to_rects_physical(1.0, 1280, 1440),
        Some(vec![(1267, 20, 13, 1402)])
    );
    assert_eq!(
        damage.to_rects_physical(2.0, 2560, 2880),
        Some(vec![(2534, 40, 26, 2804)])
    );
    assert_eq!(area_px(&[(1267, 20, 13, 1402)]), 18_226);
    assert_eq!(area_px(&[(2534, 40, 26, 2804)]), 4 * 18_226);
}

/// A10-04 (checked: fine). A fractional rect is rounded OUTWARD on every
/// side, so no pixel the renderer touched is left out of the present.
///
/// (10.3, 5.7, 4.2, 3.1) at DPI 1.5: near edges 15.45 and 8.55 floor to 15
/// and 8, far edges 14.5 * 1.5 = 21.75 and 8.8 * 1.5 = 13.2 ceil to 22 and
/// 14: (15, 8, 7, 6).
///
/// Mutations this fails: `round` for `floor` gives y0 = 9 (8.55 rounds up),
/// `round` for `ceil` gives y1 = 13 (13.2 rounds down), truncation of the
/// far edge gives x1 = 21.
#[test]
fn a_fractional_damage_rect_is_rounded_outward() {
    let damage = FrameDamage::Rects(vec![rect(10.3, 5.7, 4.2, 3.1)]);
    assert_eq!(
        damage.to_rects_physical(1.5, 1000, 1000),
        Some(vec![(15, 8, 7, 6)])
    );
}

/// A10-04 (checked: fine). Nothing outside the buffer is ever named: a rect
/// that leaves it is cut, one that lies outside is dropped.
#[test]
fn a_damage_rect_is_clamped_to_the_buffer() {
    // Leaves at the right and at the bottom (DPI 2, buffer 2560 x 2880).
    assert_eq!(
        FrameDamage::Rects(vec![rect(1270.0, 1430.0, 20.0, 20.0)]).to_rects_physical(2.0, 2560, 2880),
        Some(vec![(2540, 2860, 20, 20)])
    );
    // Leaves at the left and at the top.
    assert_eq!(
        FrameDamage::Rects(vec![rect(-5.0, -5.0, 10.0, 10.0)]).to_rects_physical(2.0, 2560, 2880),
        Some(vec![(0, 0, 10, 10)])
    );
    // Outside, empty, and a buffer without pixels: nothing to present.
    assert_eq!(
        FrameDamage::Rects(vec![rect(2000.0, 0.0, 10.0, 10.0)]).to_rects_physical(1.0, 1280, 1440),
        None
    );
    assert_eq!(
        FrameDamage::Rects(vec![rect(10.0, 10.0, 0.0, 10.0)]).to_rects_physical(1.0, 1280, 1440),
        None
    );
    assert_eq!(FrameDamage::Full.to_rects_physical(1.0, 0, 1440), None);
    for (x, y, w, h) in FrameDamage::Rects(vec![
        rect(-3.5, 1439.2, 4000.0, 9.0),
        rect(1279.9, -1.0, 0.2, 2.0),
    ])
    .to_rects_physical(1.25, 1600, 1800)
    .unwrap_or_default()
    {
        assert!(w > 0 && h > 0 && x + w <= 1600 && y + h <= 1800, "{:?}", (x, y, w, h));
    }
}

/// A10-04 (checked: fine). What a shell presents for each kind of damage:
/// nothing for `None` (unless the OS asked: then everything), the whole
/// buffer for `Full`, the rects for `Rects`, and the whole buffer once there
/// are more than 16 of them.
#[test]
fn present_rects_of_each_kind_of_damage() {
    let full = Some(vec![(0, 0, 1280, 1440)]);
    assert_eq!(FrameDamage::None.to_present_rects_physical(1.0, 1280, 1440, false), None);
    assert_eq!(FrameDamage::None.to_present_rects_physical(1.0, 1280, 1440, true), full);
    assert_eq!(FrameDamage::Full.to_present_rects_physical(1.0, 1280, 1440, false), full);
    assert_eq!(
        FrameDamage::Rects(Vec::new()).to_present_rects_physical(1.0, 1280, 1440, false),
        None
    );
    let sixteen: Vec<LogicalRect> = (0..16).map(|i| rect(i as f32 * 10.0, 0.0, 5.0, 5.0)).collect();
    assert_eq!(
        FrameDamage::Rects(sixteen.clone())
            .to_present_rects_physical(1.0, 1280, 1440, false)
            .map(|r| r.len()),
        Some(16)
    );
    let mut seventeen = sixteen;
    seventeen.push(rect(500.0, 500.0, 5.0, 5.0));
    assert_eq!(
        FrameDamage::Rects(seventeen.clone()).to_present_rects_physical(1.0, 1280, 1440, false),
        full
    );
    // The exact rects are never capped.
    assert_eq!(
        FrameDamage::Rects(seventeen).to_rects_physical(1.0, 1280, 1440).map(|r| r.len()),
        Some(17)
    );
}

/// A10-01. The flip between the renderer's rows and a y-up window system:
/// `y' = height - (y + h)`, the rect's bottom edge from the buffer's bottom.
///
/// The strip at DPI 1 (buffer 1440 high): bottom edge at 20 + 1402 = 1422,
/// so 1440 - 1422 = 18. At DPI 2 (buffer 2880 high): 2880 - 2844 = 36.
/// Mutation: `buf_h - y` (the TOP edge) gives 1420 and fails; no flip gives 20.
#[test]
fn a_rect_is_flipped_by_its_bottom_edge() {
    assert_eq!(flip_rect_y((1267, 20, 13, 1402), 1440), (1267, 18, 13, 1402));
    assert_eq!(flip_rect_y((2534, 40, 26, 2804), 2880), (2534, 36, 26, 2804));
    // The top row is the highest y, the bottom row is 0.
    assert_eq!(flip_rect_y((0, 0, 8, 1), 6), (0, 5, 8, 1));
    assert_eq!(flip_rect_y((0, 5, 8, 1), 6), (0, 0, 8, 1));
    // A rect that reaches below the buffer does not wrap around.
    assert_eq!(flip_rect_y((0, 4, 8, 5), 6), (0, 0, 8, 5));
}

/// A10-01. The flip is its own inverse, and `rows_of_bottom_left_rect` undoes
/// it: for every rect inside a buffer, the rows that are named after the
/// round trip are the rect's own rows.
#[test]
fn the_flip_round_trips_for_every_rect_in_a_buffer() {
    const W: u32 = 9;
    const H: u32 = 7;
    for x in 0..W {
        for y in 0..H {
            for w in 1..=W - x {
                for h in 1..=H - y {
                    let r: PxRect = (x, y, w, h);
                    let flipped = flip_rect_y(r, H);
                    assert_eq!(flip_rect_y(flipped, H), r, "{r:?}");
                    let (fx, fy, fw, fh) = flipped;
                    let rows = rows_of_bottom_left_rect(
                        false,
                        (f64::from(fx), f64::from(fy), f64::from(fw), f64::from(fh)),
                        W as usize,
                        H as usize,
                    );
                    assert_eq!(
                        rows,
                        (x as usize, (x + w) as usize, y as usize, (y + h) as usize),
                        "{r:?}"
                    );
                }
            }
        }
    }
}

/// A10-01. A fractional y-up rect names every row and column it touches
/// (outward), clamped; an empty one, and `full`, name the whole buffer.
///
/// A 100 x 80 buffer, the rect x 10.5..29.5, y 20.5..49.5 from the bottom:
/// columns 10..30, rows 80 - 49.5 = 30.5 -> 30 to 80 - 20.5 = 59.5 -> 60.
#[test]
fn rows_of_a_fractional_bottom_left_rect() {
    assert_eq!(
        rows_of_bottom_left_rect(false, (10.5, 20.5, 19.0, 29.0), 100, 80),
        (10, 30, 30, 60)
    );
    assert_eq!(
        rows_of_bottom_left_rect(false, (90.0, -5.0, 50.0, 20.0), 100, 80),
        (90, 100, 65, 80)
    );
    assert_eq!(rows_of_bottom_left_rect(true, (1.0, 1.0, 1.0, 1.0), 100, 80), (0, 100, 0, 80));
    assert_eq!(rows_of_bottom_left_rect(false, (1.0, 1.0, 0.0, 1.0), 100, 80), (0, 100, 0, 80));
    assert_eq!(
        rows_of_bottom_left_rect(false, (f64::NAN, 1.0, f64::NAN, 1.0), 100, 80),
        (0, 100, 0, 80)
    );
    // A rect entirely off the buffer names no pixel, and nothing out of range.
    let (x0, x1, y0, y1) = rows_of_bottom_left_rect(false, (500.0, 500.0, 10.0, 10.0), 100, 80);
    assert!(x0 == x1 || y0 == y1, "{:?}", (x0, x1, y0, y1));
    assert!(x1 <= 100 && y1 <= 80);
}

/// A10-05. The frame of a window is `ceil(logical * dpi)` pixels, and at a
/// whole DPI factor that is the window's own pixel size.
#[test]
fn the_frame_size_at_whole_dpi_factors() {
    assert_eq!(frame_size_px(1280.0, 1440.0, 1.0), (1280, 1440));
    assert_eq!(frame_size_px(1280.0, 1440.0, 2.0), (2560, 2880));
    assert_eq!(frame_size_px(2560.0, 1440.0, 2.0), (5120, 2880));
    assert_eq!(frame_size_px(1280.5, 720.25, 2.0), (2561, 1441));
    assert_eq!(frame_size_px(0.0, 10.0, 2.0), (0, 20));
}

/// A10-05. A window of 1003 device pixels at 175 % (DPI 168).
///
/// The shell stores 1003 / 1.75 = 573.142_88 (`f32`) as the logical size;
/// times 1.75 that is 1003.000_06 and the frame's `ceil` makes it 1004 px:
/// one pixel wider than the window. `logical_len_for_px` hands the renderer
/// 573.142_82 (one ulp less), whose frame is 1003 px.
#[test]
fn a_frame_one_pixel_larger_than_the_window_at_175_percent() {
    let dpi = 168.0_f32 / 96.0;
    let stored = 1003.0_f32 / dpi;
    assert_eq!(frame_size_px(stored, stored, dpi), (1004, 1004), "the round trip overshoots");
    let exact = logical_len_for_px(1003, dpi);
    assert_eq!(frame_size_px(exact, exact, dpi), (1003, 1003));
    assert!(exact <= stored && stored - exact < 0.001, "{exact} against {stored}");
    // Where the round trip is exact nothing is changed.
    assert_eq!(logical_len_for_px(1920, dpi).to_bits(), (1920.0_f32 / dpi).to_bits());
    assert_eq!(logical_len_for_px(2560, 2.0), 1280.0);
    assert_eq!(logical_len_for_px(0, 1.5), 0.0);
    // A DPI factor that is not one keeps the length (no division by zero).
    assert_eq!(logical_len_for_px(100, 0.0), 100.0);
    assert_eq!(logical_len_for_px(100, f32::NAN), 100.0);
}

/// A10-05. For every DPI a desktop can report (96 to 384) and every length up
/// to 4096 px, the logical length handed to the renderer makes a frame of
/// exactly that many pixels. Without the correction 272 of the 289 DPI values
/// have lengths that overshoot (5 to 9 % of them each).
///
/// Mutation: returning `px / dpi` unchanged fails at DPI 97, 281 px (a 282 px frame).
#[test]
fn the_frame_is_the_windows_size_at_every_dpi() {
    let mut overshoot_without = 0_u32;
    for dpi_int in 96_u32..=384 {
        let dpi = dpi_int as f32 / 96.0;
        for px in 1_u32..=4096 {
            let len = logical_len_for_px(px, dpi);
            assert_eq!(frame_size_px(len, len, dpi).0, px, "DPI {dpi_int}, {px} px");
            assert!(len <= px as f32 / dpi, "DPI {dpi_int}, {px} px");
            if frame_size_px(px as f32 / dpi, 1.0, dpi).0 != px {
                overshoot_without += 1;
            }
        }
    }
    assert!(overshoot_without > 0, "the round trip is exact everywhere: the correction is not needed");
}

/// A10-06 (cost model). One present copies the bounding box of its rects (a
/// `drawRect:` is handed one rect), so a pan step that reports the strip AND
/// the view's clip presents the whole clip: 1,724,460 px for 18,226 painted
/// at DPI 1, and four times both at DPI 2.
#[test]
fn a_pan_step_presents_the_views_clip_not_the_strip() {
    let strip: PxRect = (1267, 20, 13, 1402);
    let clip: PxRect = (50, 20, 1230, 1402);
    assert_eq!(bounding_rect(&[strip, clip]), Some(clip));
    assert_eq!(area_px(&[strip]), 18_226);
    assert_eq!(area_px(&[clip]), 1_724_460);
    // Two small rects far apart: the box between them is presented too.
    assert_eq!(
        bounding_rect(&[(10, 10, 2, 20), (1200, 1400, 40, 10)]),
        Some((10, 10, 1230, 1400))
    );
    assert_eq!(bounding_rect(&[]), None);
    assert_eq!(bounding_rect(&[(5, 5, 0, 10)]), None);
    // A full view at 2560 x 1440 is 14.7 MB of RGBA, a Retina one 59 MB.
    assert_eq!(area_px(&[(0, 0, 2560, 1440)]) * 4, 14_745_600);
    assert_eq!(area_px(&[(0, 0, 5120, 2880)]) * 4, 58_982_400);
}

/// A10-09. A Wayland toplevel at a fractional scale is `round(logical *
/// scale)` device pixels (half away from zero), and the CPU frame has to be
/// exactly that: a buffer of another size is resampled over the whole surface.
///
/// 125 %: 1000 -> 1250, 1001 -> 1251.25 -> 1251 (the frame's own `ceil`
/// gives 1252: one in four widths), 1002 -> 1252.5 -> 1253, 1003 -> 1253.75
/// -> 1254. Handing the renderer `logical_len_for_px` of the surface's size
/// makes the frame that size.
///
/// Mutation: `ceil` in `surface_len_px` fails at 1001 (1252 for 1251).
#[test]
fn a_wayland_frame_at_a_fractional_scale_is_the_surfaces_size() {
    assert_eq!(surface_len_px(1000.0, 1.25), 1250);
    assert_eq!(surface_len_px(1001.0, 1.25), 1251);
    assert_eq!(surface_len_px(1002.0, 1.25), 1253);
    assert_eq!(surface_len_px(1003.0, 1.25), 1254);
    assert_eq!(surface_len_px(3.0, 1.5), 5);
    assert_eq!(surface_len_px(1001.0, 1.75), 1752);
    assert_eq!(surface_len_px(1280.0, 2.0), 2560);
    // The frame the renderer would make from the logical size itself.
    assert_eq!(frame_size_px(1001.0, 1001.0, 1.25).0, 1252);
    // And the frame it makes from the length the shell hands it instead.
    for scale_120 in [150_u32, 180, 210, 240, 270] {
        let scale = scale_120 as f32 / 120.0;
        for logical in 1_u32..=4096 {
            let px = surface_len_px(logical as f32, scale);
            let len = logical_len_for_px(px, scale);
            assert_eq!(frame_size_px(len, len, scale).0, px, "{logical} units at {scale}");
            assert!(px.abs_diff(frame_size_px(logical as f32, 1.0, scale).0) <= 1);
        }
    }
}
