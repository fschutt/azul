//! The ink's own timings: what a pointer move costs in AzReview's handler, what a frame of the
//! ink layer costs to draw, what the engine says its frames cost. Pure bookkeeping, no window.

use super::*;

#[test]
fn nothing_is_kept_unless_the_profile_is_asked_for() {
    let mut perf = Perf::new(false);
    perf.moved(250);
    perf.drawn(900);
    perf.frame(4_000, 8_300);
    assert_eq!(perf.summary(), None, "off: no samples, no line");
}

#[test]
fn a_summary_gives_the_median_the_99th_and_the_worst_of_each() {
    let mut perf = Perf::new(true);
    for us in 1..=100 {
        perf.moved(us);
    }
    perf.drawn(500);
    perf.frame(2_000, 8_000);
    let line = perf.summary().expect("samples were taken");
    assert!(line.starts_with("AZREVIEW_PERF "), "{line}");
    assert!(line.contains("moves=100"), "{line}");
    assert!(line.contains("move_us_p50=50"), "{line}");
    assert!(line.contains("move_us_p99=99"), "{line}");
    assert!(line.contains("move_us_max=100"), "{line}");
    assert!(line.contains("draw_us_p50=500"), "{line}");
    assert!(line.contains("frame_work_us_p50=2000"), "{line}");
    assert!(line.contains("frame_interval_us_p50=8000"), "{line}");
}

#[test]
fn a_summary_starts_the_next_stretch_afresh() {
    let mut perf = Perf::new(true);
    perf.moved(10);
    assert!(perf.summary().is_some());
    assert_eq!(perf.summary(), None, "what was said is not said again");
    perf.moved(20);
    assert!(perf.summary().is_some_and(|l| l.contains("moves=1")));
}

#[test]
fn the_quantile_of_nothing_is_zero_and_of_one_is_that_one() {
    assert_eq!(quantile(&mut [], 0.99), 0);
    assert_eq!(quantile(&mut [7], 0.5), 7);
    assert_eq!(quantile(&mut [5, 1, 9, 3], 1.0), 9);
    assert_eq!(quantile(&mut [5, 1, 9, 3], 0.0), 1);
}

#[test]
fn a_run_of_samples_is_capped_so_a_long_session_cannot_grow_without_end() {
    let mut perf = Perf::new(true);
    for _ in 0..(MAX_SAMPLES + 500) {
        perf.moved(1);
    }
    let line = perf.summary().expect("samples");
    assert!(line.contains(&format!("moves={MAX_SAMPLES}")), "{line}");
}

#[test]
fn the_time_to_build_the_archive_is_in_the_summary_too() {
    let mut perf = Perf::new(true);
    perf.archived(1_500);
    let line = perf.summary().expect("an archive was timed");
    assert!(line.contains("archive_us_p50=1500"), "{line}");
}
