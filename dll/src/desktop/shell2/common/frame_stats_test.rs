//! Frame times measured inside the engine: the lines a window writes about its own presents. No clock, no window.

use super::*;

/// `count` frames `interval_us` apart from `start_us`, each with `draw_us`; the lines the burst wrote.
fn feed(
    stats: &mut FrameStats,
    start_us: u64,
    interval_us: u64,
    count: u64,
    draw_us: Option<u64>,
) -> Vec<String> {
    (0..count)
        .filter_map(|i| stats.record(start_us + i * interval_us, draw_us))
        .collect()
}

/// A steady 60 Hz scroll longer than a second is reported once a second, with the interval's median and 95th
/// percentile and the draw's.
///
/// Mutation: a median or a percentile read from the wrong rank; a line every frame.
#[test]
fn a_steady_scroll_is_reported_once_a_second() {
    let mut stats = FrameStats::new();
    // 130 frames of 16 ms: 2.07 s
    let lines = feed(&mut stats, 1_000_000, 16_000, 130, Some(4_100));
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(
        lines[0],
        "[render] frames n=64 over_ms=1008 interval_ms median=16.0 p95=16.0 max=16.0 draw_ms median=4.1 p95=4.1"
    );
}

/// The percentiles are nearest-rank over the sorted intervals: twenty frames of which three are slow have a median
/// of the fast ones and a 95th percentile of the slow ones.
///
/// Mutation: a percentile taken from the unsorted samples; `p95` that is the median.
#[test]
fn the_median_and_the_95th_percentile_are_nearest_rank() {
    let mut stats = FrameStats::new();
    let mut at = 5_000_000;
    let mut lines = Vec::new();
    // first frame, then 19 intervals: 16 of 10 ms and 3 of 40 ms (n = 20, rank 10 and 19)
    lines.extend(stats.record(at, None));
    for gap in [10_000, 10_000, 40_000, 10_000, 10_000, 10_000, 40_000, 10_000, 10_000, 10_000, 10_000, 10_000, 40_000, 10_000, 10_000, 10_000, 10_000, 10_000, 10_000] {
        at += gap;
        lines.extend(stats.record(at, None));
    }
    assert!(lines.is_empty(), "the burst is not a second long yet");
    // the UI goes idle: the next present, 400 ms later, is the first of another burst and closes this one
    lines.extend(stats.record(at + 400_000, None));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        lines[0],
        "[render] frames n=20 over_ms=280 interval_ms median=10.0 p95=40.0 max=40.0"
    );
}

/// A burst of fewer than eight frames says nothing: a tap is not a scroll.
///
/// Mutation: a line for every burst.
#[test]
fn a_short_burst_is_not_worth_a_line() {
    let mut stats = FrameStats::new();
    let mut lines = feed(&mut stats, 0, 16_000, 7, Some(3_000));
    lines.extend(stats.record(2_000_000, None));
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(stats.frames(), 1, "the frame after the idle gap begins the next burst");
}

/// The time an idle UI spends between two bursts is not a frame time: the two bursts are reported on their own, each
/// with the intervals of its own frames.
///
/// Mutation: an idle gap recorded as an interval (the maximum would be a second and a half).
#[test]
fn an_idle_gap_is_not_an_interval() {
    let mut stats = FrameStats::new();
    let mut lines = feed(&mut stats, 0, 16_000, 20, None);
    lines.extend(feed(&mut stats, 1_500_000, 16_000, 20, None));
    // the second burst is closed by a frame after another idle gap
    lines.extend(stats.record(5_000_000, None));
    assert_eq!(lines.len(), 2, "{lines:?}");
    for line in &lines {
        assert!(line.contains("max=16.0"), "{line}");
        assert!(line.contains("n=20"), "{line}");
    }
}

/// A draw time is in the line only when the shell knew it; the CPU path's frames have none.
#[test]
fn a_line_has_draw_times_only_when_they_are_known() {
    let mut stats = FrameStats::new();
    let lines = feed(&mut stats, 0, 20_000, 60, None);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("draw_ms"), "{}", lines[0]);
    let mut stats = FrameStats::new();
    let lines = feed(&mut stats, 0, 20_000, 60, Some(2_000));
    assert!(lines[0].contains("draw_ms median=2.0 p95=2.0"), "{}", lines[0]);
}

/// What a burst holds is bounded: a window that presents ten thousand times in a second keeps the cap.
#[test]
fn the_samples_are_capped() {
    let mut stats = FrameStats::new();
    for i in 0..10_000 {
        let _ = stats.record(i * 50, Some(10));
    }
    assert!(stats.frames() <= MAX_SAMPLES + 1, "{}", stats.frames());
}

/// Time that goes backwards (a clock that was adjusted) is a zero interval, not a panic or a huge one.
#[test]
fn a_clock_that_goes_backwards_does_not_panic() {
    let mut stats = FrameStats::new();
    let _ = stats.record(1_000_000, None);
    let _ = stats.record(900_000, None);
    let _ = stats.record(950_000, None);
    assert!(stats.frames() >= 1);
}
