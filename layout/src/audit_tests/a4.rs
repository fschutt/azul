//! A4, the GPU path (WebRender integration).
//!
//! What the window reports of its frames: the frame clock.

use rust_fontconfig::FcFontCache;

use crate::window::{FrameClock, FrameDamage, LayoutWindow};

/// A4-04. A frame that is noted when it is handed to its renderer and recorded
/// when it is drawn is ONE frame: counted once, its interval the time since the
/// frame before, its work reaching to the draw.
///
/// By hand (the clock stores `now + 1` ns): work starts at 1 ms, the frame is
/// handed over at 3 ms and drawn at 9 ms: 1 frame, no interval yet, work
/// 9 - 1 = 8 ms. The next frame needs no layout, is handed over at 19 ms and
/// drawn at 24 ms: 2 frames, interval 19 - 3 = 16 ms, work 0. Before, both
/// calls counted: 2 frames after the first one, "interval" 9 - 3 = 6 ms (the
/// renderer's time), work 0.
/// Mutation: `note_rendered` calling `note_present` unconditionally fails this.
#[test]
fn a4_04_a_frame_noted_and_then_recorded_is_one_frame() {
    let clock = FrameClock::default();
    clock.note_work(1_000_000);
    clock.note_present(3_000_000);
    clock.note_rendered(9_000_000);
    let first = clock.timings();
    assert_eq!(first.frame_count, 1, "one frame, not two");
    assert_eq!(first.frame_interval_us, 0, "no interval before a second frame");
    assert_eq!(first.frame_work_us, 8_000, "from the layout pass to the draw");

    clock.note_present(19_000_000);
    clock.note_rendered(24_000_000);
    let second = clock.timings();
    assert_eq!(second.frame_count, 2);
    assert_eq!(second.frame_interval_us, 16_000, "between the two frames");
    assert_eq!(second.frame_work_us, 0, "no layout started this frame");

    // A frame nobody noted (a renderer that only records) is still a frame.
    clock.note_rendered(40_000_000);
    let third = clock.timings();
    assert_eq!(third.frame_count, 3);
    assert_eq!(third.frame_interval_us, 21_000);
}

/// A4-04, through the window: `note_frame_presented` + `record_frame` is how the
/// macOS shell and the CPU backends frame one frame.
#[test]
fn a4_04_a_window_counts_a_noted_and_recorded_frame_once() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    for frame in 1..=3u64 {
        lw.note_frame_presented();
        lw.record_frame(FrameDamage::None, FrameDamage::None);
        assert_eq!(lw.frame_timings().frame_count, frame);
    }
}
