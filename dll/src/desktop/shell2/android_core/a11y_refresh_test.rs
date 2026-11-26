//! When the accessibility snapshot is rebuilt after frames that changed no layout. No window, no clock.

use super::*;

/// Nothing was repainted: there is nothing the snapshot has not seen, however long it has been.
#[test]
fn a_snapshot_nothing_has_outdated_is_never_rebuilt() {
    let state = ScrollRefresh::new();
    assert!(!state.is_due(0));
    assert!(!state.is_due(10_000));
}

/// A repaint outdates the bounds (a scroll moved every node under its container) but the rebuild waits for the repaints
/// to stop: a flick repaints every 16 ms and a rebuild per frame would walk the whole tree 60 times a second.
///
/// Mutation: due at once after the first repaint (the snapshot rebuilt at every scroll frame).
#[test]
fn the_rebuild_waits_until_the_repaints_have_stopped() {
    let mut state = ScrollRefresh::new();
    state.note_repaint(1_000);
    assert!(!state.is_due(1_000), "not in the frame of the repaint");
    assert!(!state.is_due(1_000 + QUIET_MS - 1));
    assert!(state.is_due(1_000 + QUIET_MS), "quiet for QUIET_MS: the scroll has settled");
}

/// While the repaints go on the snapshot is at most [`MAX_STALE_MS`] behind: a dump taken in the middle of a long flick
/// is stale by a bounded amount, not by the whole flick.
///
/// Mutation: no upper bound (a flick of ten seconds leaves ten seconds of wrong bounds).
#[test]
fn a_scroll_that_goes_on_is_published_every_so_often() {
    let mut state = ScrollRefresh::new();
    let mut now = 5_000;
    state.note_repaint(now);
    let mut rebuilds = 0;
    // a flick: a repaint every 16 ms for three seconds
    for _ in 0..(3_000 / 16) {
        now += 16;
        state.note_repaint(now);
        if state.is_due(now) {
            rebuilds += 1;
            state.note_rebuilt();
        }
    }
    assert!((5..=7).contains(&rebuilds), "about one rebuild per {MAX_STALE_MS} ms of a 3 s flick, got {rebuilds}");
}

/// A layout regeneration publishes a snapshot of its own (it rebuilt the bounds with the scroll offsets as they were):
/// the repaints before it no longer owe a rebuild.
#[test]
fn a_snapshot_published_by_a_layout_settles_the_debt() {
    let mut state = ScrollRefresh::new();
    state.note_repaint(100);
    state.note_rebuilt();
    assert!(!state.is_due(100 + 10 * MAX_STALE_MS));
}

/// The next repaint after a rebuild starts a new debt, with its own quiet period.
#[test]
fn the_next_repaint_after_a_rebuild_owes_a_new_one() {
    let mut state = ScrollRefresh::new();
    state.note_repaint(100);
    state.note_rebuilt();
    state.note_repaint(2_000);
    assert!(!state.is_due(2_000 + QUIET_MS - 1));
    assert!(state.is_due(2_000 + QUIET_MS));
}

/// A clock that stepped back (or a caller that passes a time before the last repaint) is never due and never panics.
#[test]
fn a_time_before_the_last_repaint_is_not_due() {
    let mut state = ScrollRefresh::new();
    state.note_repaint(5_000);
    assert!(!state.is_due(0));
    assert!(!state.is_due(4_999));
}
