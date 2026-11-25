//! The frame tap's two pure parts: the rectangles it is told about (device pixels, whole, inside the
//! frame) and the call (once for a frame that changed, with the frame's own bytes and none for one that
//! did not). No window, no renderer.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
}

/// A logical rectangle becomes device pixels at the frame's scale, and a pixel that is only partly
/// inside it is inside: the tap's copy of the changed rows must not stop a pixel short.
///
/// Mutation: rounding to nearest instead of out moves the left edge to 21 and leaves the part of a
/// pixel at the right and the bottom edge out (the 81 and the 122 below); rounding down leaves both
/// out.
#[test]
fn a_rectangle_is_scaled_and_rounded_out_to_whole_device_pixels() {
    // 10.4 -> 20.8 (out: 20); 10.4 + 29.7 = 40.1 -> 80.2 (out: 81); 20.6 -> 41.2 (out: 41);
    // 20.6 + 40 = 60.6 -> 121.2 (out: 122).
    assert_eq!(device_rects(&[rect(10.4, 20.6, 29.7, 40.0)], 2.0, 600, 400), vec![20, 41, 61, 81]);
    // At scale 1 a whole-pixel rectangle stays what it is.
    assert_eq!(device_rects(&[rect(3.0, 4.0, 5.0, 6.0)], 1.0, 100, 100), vec![3, 4, 5, 6]);
    // The order of the rectangles is the order they came in.
    assert_eq!(
        device_rects(&[rect(50.0, 0.0, 10.0, 10.0), rect(0.0, 0.0, 10.0, 10.0)], 1.0, 100, 100),
        vec![50, 0, 10, 10, 0, 0, 10, 10]
    );
}

/// A rectangle is cut to the frame; one that lies outside it, has no size or is not a number is not
/// told at all (a tap that copies rows by these numbers must never be told a row that is not there).
///
/// Mutation: no cut to the frame makes the first rectangle reach to 150 in a frame of 100.
#[test]
fn a_rectangle_is_cut_to_the_frame_and_one_outside_it_is_left_out() {
    assert_eq!(device_rects(&[rect(90.0, 95.0, 60.0, 60.0)], 1.0, 100, 100), vec![90, 95, 10, 5]);
    assert_eq!(device_rects(&[rect(-20.0, -10.0, 30.0, 30.0)], 1.0, 100, 100), vec![0, 0, 10, 20]);
    assert!(device_rects(&[rect(100.0, 0.0, 10.0, 10.0)], 1.0, 100, 100).is_empty(), "at the edge, not in");
    assert!(device_rects(&[rect(500.0, 500.0, 10.0, 10.0)], 1.0, 100, 100).is_empty());
    assert!(device_rects(&[rect(10.0, 10.0, 0.0, 10.0)], 1.0, 100, 100).is_empty(), "no width");
    assert!(device_rects(&[rect(10.0, 10.0, 10.0, -1.0)], 1.0, 100, 100).is_empty(), "no height");
    assert!(device_rects(&[rect(f32::NAN, 0.0, 10.0, 10.0)], 1.0, 100, 100).is_empty(), "no number");
    assert!(device_rects(&[rect(0.0, 0.0, f32::INFINITY, 10.0)], 1.0, 100, 100).len() == 4, "to the edge of the frame");
    // The ones that are fine still come, whatever is around them.
    assert_eq!(
        device_rects(&[rect(500.0, 0.0, 1.0, 1.0), rect(1.0, 2.0, 3.0, 4.0)], 1.0, 100, 100),
        vec![1, 2, 3, 4]
    );
    assert!(device_rects(&[], 2.0, 100, 100).is_empty());
}

/// What one call of a test's tap heard.
#[derive(Debug, Clone, PartialEq)]
struct Heard {
    width: u32,
    height: u32,
    stride: usize,
    scale: f32,
    full: bool,
    damage: Vec<i32>,
    pixels: Vec<u8>,
    /// Where the bytes were: the tap is lent the frame, not a copy of it.
    pixels_at: usize,
}

struct Log(Vec<Heard>);

extern "C" fn hear(mut data: RefAny, info: FrameTapInfo) {
    let mut log = data.downcast_mut::<Log>().expect("the log of the test");
    log.0.push(Heard {
        width: info.width,
        height: info.height,
        stride: info.stride,
        scale: info.scale,
        full: info.full,
        damage: info.damage.as_slice().to_vec(),
        pixels: info.pixels.as_slice().to_vec(),
        pixels_at: info.pixels.ptr as usize,
    });
}

fn tap_with_log() -> (FrameTap, RefAny) {
    let log = RefAny::new(Log(Vec::new()));
    (FrameTap::new(log.clone(), hear as FrameTapCallbackType), log)
}

fn heard(mut log: RefAny) -> Vec<Heard> {
    log.downcast_mut::<Log>().expect("the log").0.clone()
}

/// A frame of `w` x `h` whose bytes say where they are.
fn frame(w: u32, h: u32) -> Vec<u8> {
    (0..w as usize * h as usize * 4).map(|i| (i % 251) as u8).collect()
}

/// A frame that changed is told once, with its size, its scale, the whole of it or the rectangles that
/// changed, and its own bytes, lent (the tap sees the buffer of the frame, not a copy).
///
/// Mutation: a tap that is not called, or called twice; a stride of `width` or of `width * 3`; the
/// pixels copied (the address differs); the damage of a whole frame not marked as `full`.
#[test]
fn a_frame_that_changed_is_told_once_with_its_bytes_lent() {
    let (tap, log) = tap_with_log();
    let pixels = frame(8, 4);
    deliver(&tap, &pixels, 8, 4, 32, 2.0, &FrameDamage::Full);
    let h = heard(log.clone());
    assert_eq!(h.len(), 1);
    assert_eq!((h[0].width, h[0].height, h[0].stride, h[0].scale), (8, 4, 32, 2.0));
    assert!(h[0].full && h[0].damage.is_empty(), "a whole frame names no rectangle");
    assert_eq!(h[0].pixels, pixels, "the bytes of the frame");
    assert_eq!(h[0].pixels_at, pixels.as_ptr() as usize, "lent, not copied");

    // Rectangles come in device pixels, in the order they were.
    deliver(&tap, &pixels, 8, 4, 32, 2.0, &FrameDamage::Rects(vec![rect(0.5, 0.0, 1.0, 1.0), rect(2.0, 1.0, 1.0, 1.0)]));
    let h = heard(log);
    assert_eq!(h.len(), 2, "one call for each frame");
    assert!(!h[1].full);
    assert_eq!(h[1].damage, vec![1, 0, 2, 2, 4, 2, 2, 2]);
}

/// A frame that changed nothing is not told, and neither is one whose damage is all outside it or whose
/// bytes are fewer than its size says (a borrowed buffer that is gone must not be read).
///
/// Mutation: a call for `FrameDamage::None`; a call for rectangles that all fall outside the frame; no
/// length check (the fourth case reads past the end); no check of the stride (the fifth one claims
/// rows that are narrower than the frame).
#[test]
fn a_frame_that_changed_nothing_is_not_told() {
    let (tap, log) = tap_with_log();
    let pixels = frame(8, 4);
    deliver(&tap, &pixels, 8, 4, 32, 1.0, &FrameDamage::Full);
    deliver(&tap, &pixels, 8, 4, 32, 1.0, &FrameDamage::None);
    deliver(&tap, &pixels, 8, 4, 32, 1.0, &FrameDamage::Rects(vec![]));
    deliver(&tap, &pixels, 8, 4, 32, 1.0, &FrameDamage::Rects(vec![rect(100.0, 100.0, 5.0, 5.0)]));
    deliver(&tap, &pixels[..8 * 4 * 4 - 1], 8, 4, 32, 1.0, &FrameDamage::Full);
    deliver(&tap, &pixels, 8, 4, 31, 1.0, &FrameDamage::Full);
    assert_eq!(heard(log).len(), 1, "only the first, whole, frame");
}

/// The first frame a tap hears of a size is the whole frame, whatever the renderer repainted: the tap
/// has no picture yet to lay rectangles over (a recording that starts on a window that is being drawn,
/// a window that was resized). A tap that is registered anew starts with a whole frame again, and the
/// renderer is told that it has to make one (`needs_a_whole_frame`) even if the window is still.
///
/// Mutation: no memory of the size (the first call is told as rectangles, the picture of the tap is
/// empty); the memory kept by the window and not by the registration (the anew registered tap is
/// told rectangles); a size remembered when the frame was not told.
#[test]
fn the_first_frame_of_a_size_is_told_whole_whatever_the_damage_says() {
    let (tap, log) = tap_with_log();
    let small = frame(8, 4);
    let big = frame(10, 6);
    let part = FrameDamage::Rects(vec![rect(1.0, 1.0, 2.0, 2.0)]);
    assert!(tap.needs_a_whole_frame(8, 4), "nothing was told yet");
    deliver(&tap, &small, 8, 4, 32, 1.0, &FrameDamage::None);
    assert!(tap.needs_a_whole_frame(8, 4), "a frame that is not told is not remembered");
    deliver(&tap, &small, 8, 4, 32, 1.0, &part);
    assert!(!tap.needs_a_whole_frame(8, 4));
    assert!(tap.needs_a_whole_frame(10, 6), "another size");
    deliver(&tap, &small, 8, 4, 32, 1.0, &part);
    deliver(&tap, &big, 10, 6, 40, 1.0, &part);
    deliver(&tap, &big, 10, 6, 40, 1.0, &part);
    deliver(&tap, &small, 8, 4, 32, 1.0, &part);
    let h = heard(log);
    let told: Vec<(bool, usize)> = h.iter().map(|h| (h.full, h.damage.len())).collect();
    assert_eq!(
        told,
        vec![(true, 0), (false, 4), (true, 0), (false, 4), (true, 0)],
        "whole, a part; the new size whole, a part; back to the first size whole again"
    );

    // The same tap re-registered (a new one, with the same data) starts over.
    let again = FrameTap::new(tap.data.clone(), hear as FrameTapCallbackType);
    assert!(again.needs_a_whole_frame(8, 4));
    // And a copy of a tap is the same registration: what one was told, the other was.
    let copy = tap.clone();
    assert!(!copy.needs_a_whole_frame(8, 4));
}

/// A frame bigger than its size says is told as the size says: the tap gets `stride * height` bytes and
/// no more (a buffer the renderer keeps larger than the window is not the window's frame).
#[test]
fn only_the_bytes_of_the_frame_are_lent_not_what_follows_them() {
    let (tap, log) = tap_with_log();
    let pixels = frame(8, 5);
    deliver(&tap, &pixels, 8, 4, 32, 1.0, &FrameDamage::Full);
    assert_eq!(heard(log)[0].pixels.len(), 8 * 4 * 4);
}

/// A frame painted into a buffer of the platform's whose rows are wider than the frame (a Wayland
/// slot pads every row) is told with its own width and the buffer's stride: the rectangles are cut to
/// the frame, not to the buffer, and the tap steps from row to row by the stride.
///
/// Mutation: the stride taken as the frame's `width * 4` (24 here); the rectangles cut to the width of
/// the buffer (the 8 below) and not to that of the frame (the 6).
#[test]
fn a_frame_in_a_padded_buffer_is_told_with_the_stride_of_the_buffer() {
    let (tap, log) = tap_with_log();
    let pixels = frame(8, 4); // 8 pixels to a row, 6 of them the frame's
    deliver(&tap, &pixels, 6, 4, 32, 1.0, &FrameDamage::Full);
    deliver(&tap, &pixels, 6, 4, 32, 1.0, &FrameDamage::Rects(vec![rect(5.0, 0.0, 3.0, 1.0)]));
    let h = heard(log);
    assert_eq!((h[1].width, h[1].height, h[1].stride), (6, 4, 32));
    assert_eq!(h[1].damage, vec![5, 0, 1, 1], "cut to the 6 pixels of the frame");
    assert_eq!(h[1].pixels.len(), 32 * 4);
}

/// The info prints its numbers and not its bytes.
#[test]
fn the_info_prints_its_sizes_and_not_the_frame() {
    let pixels = frame(2, 2);
    let rects = [0, 0, 1, 1];
    let info = FrameTapInfo {
        width: 2,
        height: 2,
        stride: 8,
        scale: 1.0,
        full: false,
        damage: I32VecRef::from(&rects[..]),
        pixels: U8VecRef::from(&pixels[..]),
    };
    let text = format!("{info:?}");
    assert_eq!(text, "FrameTapInfo { 2x2 px at 1x, 1 rects, 16 bytes }");
    assert!(!text.contains('['), "the rectangles are counted, not listed: {text}");
}
