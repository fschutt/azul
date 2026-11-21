//! A4, the GPU path (WebRender integration).
//!
//! What the window reports of its frames: the frame clock and the GPU path's
//! counters.

use rust_fontconfig::FcFontCache;

use crate::window::{
    FrameClock, FrameDamage, GpuFrameCounters, LayoutWindow, GPU_FRAME_COUNTERS,
};

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

/// The GPU path's counters: the last frame's are what `FrameTimings::gpu_*`
/// and the frame report's `gpu_last` say, the total is their sum.
#[test]
fn a4_counters_of_the_gpu_path_reach_the_frame_timings_and_the_frame_report() {
    let lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    assert_eq!(lw.frame_report_synced().gpu_last, GpuFrameCounters::default());

    // Every counter its own value: a field written to the wrong slot shows.
    let numbered = |base: u64| {
        let mut a = [0u64; GPU_FRAME_COUNTERS];
        for (i, slot) in a.iter_mut().enumerate() {
            *slot = base + i as u64;
        }
        GpuFrameCounters::from_array(a)
    };
    let (one, two) = (numbered(100), numbered(1_000));
    assert_eq!(one.to_array()[0], one.transactions);
    assert_eq!(one.to_array()[GPU_FRAME_COUNTERS - 1], one.render_us);

    lw.frame_clock.note_gpu(&one);
    lw.frame_clock.note_gpu(&two);
    let report = lw.frame_report_synced();
    assert_eq!(report.gpu_last, two, "the last frame's");
    let mut sum = one;
    sum.add(&two);
    assert_eq!(report.gpu_total, sum, "every frame's");

    let timings = lw.frame_timings();
    assert_eq!(timings.gpu_transactions, two.transactions);
    assert_eq!(timings.gpu_items_translated, two.items_translated);
    assert_eq!(timings.gpu_glyphs_rasterized, two.glyphs_rasterized);
    assert_eq!(timings.gpu_blocked_us, two.blocked_us);
    assert_eq!(timings.gpu_render_us, two.render_us);
}
