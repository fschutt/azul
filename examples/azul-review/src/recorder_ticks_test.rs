//! How steadily the window's timer runs after a recording starts: the first tick's distance from the start, the
//! longest gap between two ticks and how many there were. Pure: the times are given.

use super::*;

/// The numbers of a run: the first tick, the longest gap (the start counts as a tick before the first), and the count.
///
/// Mutation: the start not counted as the beginning of the first gap; the maximum taken from the last gap; a count
/// that includes the start.
#[test]
fn the_first_tick_the_longest_gap_and_the_count_are_what_the_times_say() {
    let mut s = TickStats::default();
    s.begin(1_000_000);
    s.tick(1_830_000);
    s.tick(1_930_000);
    s.tick(2_340_000);
    s.tick(2_440_000);
    assert_eq!(s.count, 4);
    assert_eq!(s.first_us, 830_000, "the first tick is 830 ms after the start");
    assert_eq!(s.max_gap_us, 830_000, "the first gap is the longest here");
    s.tick(2_450_000);
    s.tick(3_900_000);
    assert_eq!(s.max_gap_us, 1_450_000, "a later gap is longer");
    assert_eq!(s.line(), "count=6 first_ms=830 max_gap_ms=1450");
}

/// A tick before any start counts for nothing; a start begins afresh (the numbers of the run before are gone); a
/// clock that goes back is a gap of nothing, not a huge one.
///
/// Mutation: a tick counted before the start; a start that keeps the old numbers; a clock going back wrapping round.
#[test]
fn a_tick_before_the_start_counts_for_nothing_and_a_start_begins_afresh() {
    let mut s = TickStats::default();
    s.tick(500);
    assert_eq!((s.count, s.first_us, s.max_gap_us), (0, 0, 0));
    assert_eq!(s.line(), "count=0 first_ms=0 max_gap_ms=0");
    s.begin(1_000);
    s.tick(51_000);
    s.tick(2_051_000);
    assert_eq!((s.count, s.max_gap_us), (2, 2_000_000));
    s.begin(5_000_000);
    assert_eq!((s.count, s.first_us, s.max_gap_us), (0, 0, 0), "afresh");
    s.tick(5_100_000);
    s.tick(5_000_000);
    assert_eq!(s.max_gap_us, 100_000, "a clock that went back is no gap of 18 quintillion");
}
