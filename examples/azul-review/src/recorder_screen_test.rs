//! The shadow of the window and the pacing of what is sent from it: what a tap copies, when a frame
//! is due, the repeats that sharpen text, the keyframes. A clock the test moves by passing numbers.

use super::*;

const MS: u64 = 1_000;

/// A frame of `w` x `h` pixels, every byte `fill`, rows `stride` bytes apart.
fn pixels(_w: u32, h: u32, stride: usize, fill: u8) -> Vec<u8> {
    vec![fill; stride * h as usize + 16]
}

fn frame<'a>(w: u32, h: u32, stride: usize, damage: Damage<'a>, px: &'a [u8]) -> TapFrame<'a> {
    TapFrame { width: w, height: h, stride, scale: 2.0, damage, pixels: px }
}

fn full(w: u32, h: u32, fill: u8) -> Vec<u8> {
    pixels(w, h, w as usize * 4, fill)
}

fn capture() -> ScreenCapture {
    ScreenCapture::new(Pace::at_fps(10))
}

/// The byte at (x, y) of a picture of width `w`.
fn at(bytes: &[u8], w: u32, x: u32, y: u32) -> u8 {
    bytes[(y as usize * w as usize + x as usize) * 4]
}

// ---- the tap ----

#[test]
fn the_first_frame_makes_the_shadow_and_an_odd_window_is_sent_at_an_even_size() {
    let mut c = capture();
    assert_eq!(c.window_size(), (0, 0));
    let px = full(1281, 721, 7);
    assert_eq!(c.tap(&frame(1281, 721, 1281 * 4, Damage::Full, &px), 0), Tapped::Resized);
    assert_eq!(c.window_size(), (1281, 721));
    assert_eq!(c.picture_size(), (1280, 720));
    assert!((c.scale() - 2.0).abs() < f32::EPSILON);
    let (f, key) = c.take_frame(0).unwrap();
    assert!(key);
    assert_eq!((f.width, f.height, f.format), (1280, 720, PixelFormat::Rgba8));
    assert_eq!(f.bytes.len(), 1280 * 720 * 4);
    assert!(f.bytes.iter().all(|b| *b == 7), "the window's pixels, cropped");
}

#[test]
fn only_the_damaged_rows_are_copied_and_the_rest_of_the_shadow_stays() {
    let mut c = capture();
    let first = full(64, 48, 1);
    c.tap(&frame(64, 48, 256, Damage::Full, &first), 0);
    c.take_frame(0).unwrap();
    // The window now shows 2 everywhere, but says only a 20 x 5 rectangle changed.
    let second = full(64, 48, 2);
    let rects = [Rect { x: 10, y: 10, w: 20, h: 5 }];
    assert_eq!(c.tap(&frame(64, 48, 256, Damage::Rects(&rects), &second), 10 * MS), Tapped::Copied { rows: 5 });
    let (f, _) = c.take_frame(500 * MS).unwrap();
    assert_eq!(at(&f.bytes, 64, 10, 10), 2);
    assert_eq!(at(&f.bytes, 64, 29, 14), 2);
    assert_eq!(at(&f.bytes, 64, 9, 10), 1, "left of the rectangle: untouched");
    assert_eq!(at(&f.bytes, 64, 30, 10), 1, "right of it");
    assert_eq!(at(&f.bytes, 64, 10, 9), 1, "above");
    assert_eq!(at(&f.bytes, 64, 10, 15), 1, "below");
    assert_eq!(c.stats().rows_copied, 48 + 5);
}

#[test]
fn full_damage_copies_everything_and_a_new_size_starts_over() {
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 1)), 0);
    assert_eq!(c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 3)), MS), Tapped::Copied { rows: 48 });
    assert_eq!(c.tap(&frame(80, 60, 320, Damage::Rects(&[]), &full(80, 60, 5)), 2 * MS), Tapped::Resized, "the window was resized");
    assert_eq!(c.window_size(), (80, 60));
    let (f, _) = c.take_frame(500 * MS).unwrap();
    assert_eq!((f.width, f.height), (80, 60));
    assert!(f.bytes.iter().all(|b| *b == 5), "a resize copies the whole frame whatever the damage says");
}

#[test]
fn a_rectangle_is_clipped_to_the_window_and_an_empty_one_copies_nothing() {
    let mut c = capture();
    c.tap(&frame(32, 32, 128, Damage::Full, &full(32, 32, 1)), 0);
    let second = full(32, 32, 9);
    let rects = [
        Rect { x: 28, y: 30, w: 100, h: 100 }, // runs past the corner: 4 x 2
        Rect { x: 5, y: 5, w: 0, h: 10 },
        Rect { x: 40, y: 40, w: 5, h: 5 }, // outside
        Rect { x: u32::MAX - 1, y: 1, w: 10, h: 10 }, // would overflow
    ];
    assert_eq!(c.tap(&frame(32, 32, 128, Damage::Rects(&rects), &second), MS), Tapped::Copied { rows: 2 });
    let (f, _) = c.take_frame(500 * MS).unwrap();
    assert_eq!(at(&f.bytes, 32, 31, 31), 9);
    assert_eq!(at(&f.bytes, 32, 27, 31), 1);
}

#[test]
fn a_row_stride_wider_than_the_row_is_read_as_padding() {
    let mut c = capture();
    let mut px = vec![0_u8; 300 * 8 + 16]; // 8 rows of 300 bytes for a window 64 bytes (16 px) wide
    for y in 0..8 {
        px[y * 300..y * 300 + 64].fill(y as u8 + 1);
        px[y * 300 + 64..(y + 1) * 300].fill(0xee); // padding that must not be copied
    }
    c.tap(&frame(16, 8, 300, Damage::Full, &px), 0);
    let (f, _) = c.take_frame(0).unwrap();
    assert_eq!(f.bytes.len(), 16 * 8 * 4);
    assert_eq!(at(&f.bytes, 16, 0, 0), 1);
    assert_eq!(at(&f.bytes, 16, 15, 7), 8);
    assert!(!f.bytes.contains(&0xee));
}

#[test]
fn a_frame_that_cannot_be_a_windows_is_ignored_with_a_reason() {
    let mut c = capture();
    let px = full(64, 48, 1);
    for (w, h, stride, pixels) in [
        (0, 48, 256, &px[..]),
        (64, 1, 256, &px[..]),
        (9_000, 48, 36_000, &px[..]),
        (64, 48, 100, &px[..]),            // a stride shorter than a row
        (64, 48, 256, &px[..256 * 20]),    // fewer pixels than the size says
    ] {
        assert!(matches!(c.tap(&frame(w, h, stride, Damage::Full, pixels), 0), Tapped::Ignored(_)), "{w}x{h} stride {stride}");
    }
    assert_eq!(c.window_size(), (0, 0));
    assert_eq!(c.stats().ignored, 5);
    assert_eq!(c.due(10_000 * MS), Due::Nothing);
}

// ---- pacing ----

#[test]
fn a_change_is_sent_at_once_the_first_time_and_then_no_faster_than_the_pace() {
    let mut c = capture(); // 10 a second: 100 ms apart
    let px = full(64, 48, 1);
    assert_eq!(c.due(0), Due::Nothing, "nothing seen");
    c.tap(&frame(64, 48, 256, Damage::Full, &px), 0);
    assert_eq!(c.due(0), Due::Change);
    assert!(c.take_frame(0).is_some());
    c.tap(&frame(64, 48, 256, Damage::Rects(&[Rect { x: 0, y: 0, w: 4, h: 4 }]), &px), 30 * MS);
    assert_eq!(c.due(30 * MS), Due::Nothing, "too soon after the last frame");
    assert_eq!(c.due(99 * MS), Due::Nothing);
    assert_eq!(c.due(100 * MS), Due::Change, "the change that came in the gap is sent when the gap is over: nothing is lost");
    assert!(c.take_frame(100 * MS).is_some());
    assert!(c.take_frame(100 * MS).is_none(), "and only once");
}

#[test]
fn a_burst_is_one_frame_per_interval_with_the_newest_picture() {
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 0)), 0);
    c.take_frame(0).unwrap();
    // 60 changes in half a second, each a different picture: the encoder gets the frames at 100 ms.
    let mut sent = Vec::new();
    for i in 1..=60_u8 {
        let t = u64::from(i) * 8 * MS;
        c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, i)), t);
        if let Some((f, _)) = c.take_frame(t) {
            sent.push((t, f.bytes[0]));
        }
    }
    assert!(sent.len() >= 4 && sent.len() <= 5, "{sent:?}");
    for pair in sent.windows(2) {
        assert!(pair[1].0 - pair[0].0 >= 100 * MS, "{sent:?}");
    }
}

#[test]
fn after_the_last_change_the_same_picture_is_sent_three_more_times_and_then_nothing() {
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 4)), 1_000 * MS);
    assert_eq!(c.due(1_000 * MS), Due::Change);
    c.take_frame(1_000 * MS).unwrap();
    assert_eq!(c.due(1_050 * MS), Due::Nothing);
    assert_eq!(c.due(1_119 * MS), Due::Nothing);
    assert_eq!(c.due(1_120 * MS), Due::Settle(0), "120 ms after the change");
    assert!(c.take_frame(1_120 * MS).is_some());
    assert_eq!(c.due(1_399 * MS), Due::Nothing);
    assert_eq!(c.due(1_400 * MS), Due::Settle(1), "400 ms after it");
    assert!(c.take_frame(1_400 * MS).is_some());
    assert_eq!(c.due(2_199 * MS), Due::Nothing);
    assert_eq!(c.due(2_200 * MS), Due::Settle(2), "1200 ms after it");
    assert!(c.take_frame(2_200 * MS).is_some());
    for t in [2_300, 3_000, 60_000, 3_600_000] {
        assert_eq!(c.due(t * MS), Due::Nothing, "an idle window sends nothing, at {t} ms");
        assert!(c.take_frame(t * MS).is_none());
    }
    let s = c.stats();
    assert_eq!((s.changes_sent, s.settles_sent), (1, 3));
}

#[test]
fn a_late_settle_is_not_sent_in_a_burst_after_a_stall() {
    // The timer did not run for two seconds: one settle at a time, a gap apart, not three at once.
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 4)), 0);
    c.take_frame(0).unwrap();
    assert_eq!(c.due(2_000 * MS), Due::Settle(0));
    c.take_frame(2_000 * MS).unwrap();
    assert_eq!(c.due(2_000 * MS), Due::Nothing, "the gap is not over");
    assert_eq!(c.due(2_100 * MS), Due::Settle(1));
}

#[test]
fn a_new_change_starts_the_repeats_again() {
    let mut c = capture();
    let px = full(64, 48, 1);
    c.tap(&frame(64, 48, 256, Damage::Full, &px), 0);
    c.take_frame(0).unwrap();
    for t in [120, 400, 1_200] {
        c.take_frame(t * MS).unwrap();
    }
    assert!(c.take_frame(5_000 * MS).is_none());
    c.tap(&frame(64, 48, 256, Damage::Rects(&[Rect { x: 1, y: 1, w: 2, h: 2 }]), &px), 6_000 * MS);
    assert_eq!(c.due(6_000 * MS), Due::Change);
    c.take_frame(6_000 * MS).unwrap();
    assert_eq!(c.due(6_120 * MS), Due::Settle(0), "three repeats again");
}

#[test]
fn a_tap_that_changed_nothing_sends_nothing() {
    let mut c = capture();
    let px = full(64, 48, 1);
    c.tap(&frame(64, 48, 256, Damage::Full, &px), 0);
    c.take_frame(0).unwrap();
    for t in [120, 400, 1_200] {
        c.take_frame(t * MS).unwrap();
    }
    assert_eq!(c.tap(&frame(64, 48, 256, Damage::Rects(&[]), &px), 9_000 * MS), Tapped::Copied { rows: 0 });
    assert_eq!(c.due(9_000 * MS), Due::Nothing);
}

#[test]
fn a_frame_the_encoder_refused_is_sent_again() {
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 1)), 0);
    c.take_frame(0).unwrap();
    c.retry();
    assert_eq!(c.due(50 * MS), Due::Nothing, "not before the gap");
    assert_eq!(c.due(100 * MS), Due::Change);
}

// ---- keyframes ----

#[test]
fn keyframes_come_first_every_two_seconds_and_when_asked_for() {
    let mut c = capture();
    let px = full(64, 48, 1);
    let mut keys = Vec::new();
    for i in 0..60_u64 {
        let t = i * 100 * MS;
        c.tap(&frame(64, 48, 256, Damage::Rects(&[Rect { x: 0, y: 0, w: 2, h: 2 }]), &px), t);
        if let Some((_, key)) = c.take_frame(t) {
            if key {
                keys.push(t / MS);
            }
        }
    }
    // The first frame, then every two seconds of frames being sent.
    assert_eq!(keys, vec![0, 2_000, 4_000]);
    c.request_keyframe();
    c.tap(&frame(64, 48, 256, Damage::Rects(&[Rect { x: 0, y: 0, w: 2, h: 2 }]), &px), 6_000 * MS);
    assert!(c.take_frame(6_000 * MS).unwrap().1, "asked for: a keyframe");
    c.tap(&frame(64, 48, 256, Damage::Rects(&[Rect { x: 0, y: 0, w: 2, h: 2 }]), &px), 6_100 * MS);
    assert!(!c.take_frame(6_100 * MS).unwrap().1, "and only that one");
}

#[test]
fn a_resumed_recording_starts_with_a_keyframe_of_the_picture_as_it_is() {
    let mut c = capture();
    let px = full(64, 48, 1);
    c.tap(&frame(64, 48, 256, Damage::Full, &px), 0);
    c.take_frame(0).unwrap();
    c.mark_changed(600_000 * MS);
    let (_, key) = c.take_frame(600_000 * MS).unwrap();
    assert!(key);
}

#[test]
fn the_pace_is_the_frame_rate_cap() {
    assert_eq!(Pace::at_fps(10).min_gap_us, 100_000);
    assert_eq!(Pace::at_fps(30).min_gap_us, 33_333);
    assert_eq!(Pace::at_fps(1).min_gap_us, 1_000_000);
    assert_eq!(Pace::at_fps(0).min_gap_us, 1_000_000, "zero is not a rate");
}

// ---- the system's own capture of the window ----

fn external(w: u32, h: u32, format: PixelFormat, fill: u8) -> RawFrame {
    let len = match format {
        PixelFormat::Nv12 => w as usize * h as usize * 3 / 2,
        _ => w as usize * h as usize * 4,
    };
    RawFrame { width: w, height: h, format, bytes: vec![fill; len] }
}

#[test]
fn a_frame_of_the_systems_capture_is_the_picture_with_the_same_pace_and_repeats() {
    let mut c = capture();
    assert!(!c.is_external());
    assert_eq!(c.push_external(external(320, 200, PixelFormat::Nv12, 5), 2.0, 0), Tapped::Resized);
    assert!(c.is_external());
    assert_eq!((c.window_size(), c.picture_size()), ((320, 200), (320, 200)));
    assert!((c.scale() - 2.0).abs() < f32::EPSILON);
    let (f, key) = c.take_frame(0).unwrap();
    assert!(key);
    assert_eq!((f.format, f.width, f.height, f.bytes.len()), (PixelFormat::Nv12, 320, 200, 320 * 200 * 3 / 2), "the frame as it came, no conversion");
    assert_eq!(c.due(50 * MS), Due::Nothing);
    assert_eq!(c.due(120 * MS), Due::Settle(0), "the same sharpening repeats");
    assert_eq!(c.take_frame(120 * MS).unwrap().0.bytes[0], 5, "the repeat is the newest frame again");
    // A newer frame replaces it and is a change.
    assert_eq!(c.push_external(external(320, 200, PixelFormat::Nv12, 9), 2.0, 300 * MS), Tapped::Copied { rows: 200 });
    assert_eq!(c.take_frame(300 * MS).unwrap().0.bytes[0], 9);
}

#[test]
fn a_captured_frame_that_cannot_be_a_pictures_is_ignored() {
    let mut c = capture();
    for bad in [
        external(321, 200, PixelFormat::Nv12, 0),
        external(320, 201, PixelFormat::Bgra8, 0),
        external(0, 0, PixelFormat::Bgra8, 0),
        RawFrame { width: 320, height: 200, format: PixelFormat::Bgra8, bytes: vec![0; 100] },
    ] {
        assert!(matches!(c.push_external(bad, 1.0, 0), Tapped::Ignored(_)));
    }
    assert_eq!(c.stats().ignored, 4);
    assert!(!c.is_external() && c.take_frame(10 * MS).is_none());
}

#[test]
fn the_systems_capture_replaces_the_shadow_and_a_resize_is_seen() {
    let mut c = capture();
    c.tap(&frame(64, 48, 256, Damage::Full, &full(64, 48, 1)), 0);
    c.push_external(external(128, 96, PixelFormat::Bgra8, 2), 1.0, 10 * MS);
    let (f, _) = c.take_frame(500 * MS).unwrap();
    assert_eq!((f.width, f.height, f.format), (128, 96, PixelFormat::Bgra8));
    assert_eq!(c.push_external(external(256, 192, PixelFormat::Bgra8, 3), 1.0, 600 * MS), Tapped::Resized);
}
