//! The flip rules of a tab panel: a table of gestures against content states, the rubber band,
//! the hint, the cancel, the wheel.
//!
//! Not compiled by the author (house rule).

use super::*;

fn names(i: usize) -> String {
    ["Home", "Share", "View"]
        .get(i)
        .map_or_else(|| format!("Tab {i}"), |s| (*s).to_string())
}

/// A panel of `count` tabs showing `tab`, content that overflows by `max` px and is scrolled `scroll`.
fn panel(flip: TabFlip, tab: usize, count: usize, scroll: f32, max: f32) -> FlipPanel {
    FlipPanel {
        flip,
        tab,
        tab_count: count,
        overflows: max > 0.0,
        scroll,
        scroll_max: max,
    }
}

/// A finger down at (100, 400), then moved by `dy` in steps of 8 px, 16 ms apart, then lifted: what
/// the lift answers.
fn swipe(panel: FlipPanel, dy: f32) -> FlipOutcome {
    let mut gesture = FlipGesture::begin(panel, 100.0, 400.0, 0, false);
    let steps = (dy.abs() / 8.0).ceil() as i32;
    let mut y = 400.0;
    let mut t = 0;
    for _ in 0..steps {
        y += 8.0_f32.copysign(dy);
        t += 16;
        let _ = gesture.moved(100.0, y, t, &names);
    }
    gesture.lifted(t + 16)
}

#[test]
fn there_is_no_wrap_the_row_has_two_ends() {
    assert_eq!(FlipDirection::Previous.neighbour(0, 3), None);
    assert_eq!(FlipDirection::Next.neighbour(2, 3), None);
    assert_eq!(FlipDirection::Next.neighbour(0, 3), Some(1));
    assert_eq!(FlipDirection::Previous.neighbour(2, 3), Some(1));
    assert_eq!(FlipDirection::Next.neighbour(0, 0), None);
    assert_eq!(FlipDirection::Next.neighbour(0, 1), None);
    // An index past the last tab (none selected) still has a previous, as long as it is in the row.
    assert_eq!(FlipDirection::Previous.neighbour(7, 3), None);
}

#[test]
fn the_rubber_follows_the_finger_at_first_then_ever_less_and_never_past_its_limit() {
    assert_eq!(rubber(0.0, 1.0), 0.0);
    assert_eq!(rubber(-5.0, 1.0), 0.0);
    assert_eq!(rubber(f32::NAN, 1.0), 0.0);
    let mut previous = 0.0;
    for d in 1..400 {
        let r = rubber(d as f32, 1.0);
        assert!(r > previous, "grows with the pull");
        assert!(r < d as f32, "never more than the finger");
        assert!(r < RUBBER_MAX, "never past the limit");
        previous = r;
    }
    assert!(
        rubber(40.0, 2.0) < rubber(40.0, 1.0),
        "the end of the row is stiffer"
    );
}

#[test]
fn content_that_does_not_scroll_flips_on_a_swipe_either_way() {
    let up = swipe(panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0), -80.0);
    assert_eq!(
        up,
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        }
    );
    let down = swipe(panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0), 80.0);
    assert_eq!(
        down,
        FlipOutcome::Flip {
            direction: FlipDirection::Previous,
            to: 0
        }
    );
}

#[test]
fn a_short_pull_springs_back() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    // 56 of pull beyond the 8 of slop flips; a swipe of 56 in all is 48 of pull: short.
    assert_eq!(swipe(p, -56.0), FlipOutcome::Release);
    assert_eq!(
        swipe(p, -64.0),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        }
    );
}

#[test]
fn a_fling_flips_with_a_shorter_pull_but_not_with_a_tiny_one() {
    let p = panel(TabFlip::AtEdges, 0, 3, 0.0, 0.0);
    // 40 px in 40 ms: 1000 px per second, a pull of 32: flung.
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 380.0, 20, &names);
    let _ = g.moved(100.0, 360.0, 40, &names);
    assert_eq!(
        g.lifted(41),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 1
        }
    );
    // The same speed for 20 px: a pull of 12, under the fling's minimum.
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 390.0, 10, &names);
    let _ = g.moved(100.0, 380.0, 20, &names);
    assert_eq!(g.lifted(21), FlipOutcome::Release);
    // A slow pull of the same length is no fling.
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 380.0, 400, &names);
    let _ = g.moved(100.0, 368.0, 800, &names);
    assert_eq!(g.lifted(1200), FlipOutcome::Release);
}

#[test]
fn the_first_tab_has_no_previous_and_the_last_no_next_the_pull_is_a_stretch() {
    let first = panel(TabFlip::AtEdges, 0, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(first, 100.0, 400.0, 0, false);
    let outcome = g.moved(100.0, 480.0, 100, &names);
    match outcome {
        FlipOutcome::Pulling {
            direction,
            target,
            hint,
            progress,
            offset,
        } => {
            assert_eq!(direction, FlipDirection::Previous);
            assert_eq!(target, None);
            assert_eq!(hint, None, "nothing to name");
            assert_eq!(progress, 0.0);
            assert!(
                offset > 0.0 && offset < rubber(72.0, 1.0),
                "stiffer than a pull with a target"
            );
        }
        other => panic!("a stretch, got {other:?}"),
    }
    assert_eq!(g.lifted(120), FlipOutcome::Release);
    assert_eq!(
        swipe(panel(TabFlip::AtEdges, 2, 3, 0.0, 0.0), -90.0),
        FlipOutcome::Release
    );
}

#[test]
fn content_that_scrolls_scrolls_first_and_flips_only_from_its_end() {
    let max = 400.0;
    // In the middle: a swipe either way is the content's.
    for dy in [-90.0, 90.0] {
        assert_eq!(
            swipe(panel(TabFlip::AtEdges, 1, 3, 200.0, max), dy),
            FlipOutcome::Idle
        );
    }
    // At the bottom: a finger moving up pulls past the end and flips; moving down scrolls back.
    let bottom = panel(TabFlip::AtEdges, 1, 3, max, max);
    assert_eq!(
        swipe(bottom, -90.0),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        }
    );
    assert_eq!(swipe(bottom, 90.0), FlipOutcome::Idle);
    // At the top: the other way round.
    let top = panel(TabFlip::AtEdges, 1, 3, 0.0, max);
    assert_eq!(
        swipe(top, 90.0),
        FlipOutcome::Flip {
            direction: FlipDirection::Previous,
            to: 0
        }
    );
    assert_eq!(swipe(top, -90.0), FlipOutcome::Idle);
    // A hair from the end counts as the end (a scroll that stopped at 399.6).
    assert_eq!(
        swipe(panel(TabFlip::AtEdges, 1, 3, 399.6, max), -90.0),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        }
    );
}

#[test]
fn a_gesture_that_is_more_sideways_than_vertical_is_not_a_flip() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    assert_eq!(g.moved(160.0, 410.0, 30, &names), FlipOutcome::Idle);
    assert_eq!(
        g.moved(170.0, 490.0, 60, &names),
        FlipOutcome::Idle,
        "decided: not ours, for good"
    );
    assert_eq!(g.lifted(80), FlipOutcome::Idle);
}

#[test]
fn a_gesture_that_went_down_on_something_that_claims_it_is_never_a_flip() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, true);
    assert_eq!(g.moved(100.0, 300.0, 100, &names), FlipOutcome::Idle);
    assert_eq!(g.lifted(120), FlipOutcome::Idle);
}

#[test]
fn off_and_a_single_tab_never_flip() {
    assert_eq!(
        swipe(panel(TabFlip::Off, 1, 3, 0.0, 0.0), -90.0),
        FlipOutcome::Idle
    );
    assert_eq!(
        swipe(panel(TabFlip::AtEdges, 0, 1, 0.0, 0.0), -90.0),
        FlipOutcome::Idle
    );
    assert_eq!(
        swipe(panel(TabFlip::AtEdges, 0, 0, 0.0, 0.0), -90.0),
        FlipOutcome::Idle
    );
}

#[test]
fn below_the_slop_nothing_has_started() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    assert_eq!(g.moved(100.0, 394.0, 16, &names), FlipOutcome::Idle);
    assert!(!g.is_pulling());
    assert_eq!(g.lifted(32), FlipOutcome::Idle);
}

#[test]
fn the_hint_shows_from_a_sixth_of_the_threshold_on_and_names_the_tab() {
    let p = panel(TabFlip::AtEdges, 0, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    // 8 slop + 4 pull: progress 4/56 = 0.07: no hint yet.
    match g.moved(100.0, 388.0, 16, &names) {
        FlipOutcome::Pulling { hint, progress, .. } => {
            assert!(progress < HINT_MIN_PROGRESS);
            assert_eq!(hint, None);
        }
        other => panic!("{other:?}"),
    }
    // 8 slop + 20 pull: progress 0.36.
    match g.moved(100.0, 372.0, 32, &names) {
        FlipOutcome::Pulling {
            hint,
            target,
            progress,
            ..
        } => {
            assert_eq!(hint.as_deref(), Some("Next: Share"));
            assert_eq!(target, Some(1));
            assert!(progress > HINT_MIN_PROGRESS && progress < 1.0);
        }
        other => panic!("{other:?}"),
    }
    // Past the threshold the progress is over 1: the app shows the hint as "release to flip".
    match g.moved(100.0, 330.0, 48, &names) {
        FlipOutcome::Pulling { progress, .. } => assert!(progress >= 1.0),
        other => panic!("{other:?}"),
    }
    let mut back = FlipGesture::begin(
        panel(TabFlip::AtEdges, 2, 3, 0.0, 0.0),
        100.0,
        400.0,
        0,
        false,
    );
    match back.moved(100.0, 450.0, 50, &names) {
        FlipOutcome::Pulling { hint, .. } => assert_eq!(hint.as_deref(), Some("Previous: Share")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_finger_that_goes_back_past_where_it_began_pulls_nothing_and_does_not_flip() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 340.0, 50, &names);
    match g.moved(100.0, 420.0, 100, &names) {
        FlipOutcome::Pulling {
            progress, offset, ..
        } => {
            assert_eq!(progress, 0.0);
            assert_eq!(offset, 0.0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(g.lifted(120), FlipOutcome::Release);
}

#[test]
fn a_cancelled_touch_never_flips_however_far_it_had_come() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 300.0, 50, &names);
    assert!(g.is_pulling());
    // The system took the finger (the home gesture, the back gesture, the shade).
    assert_eq!(g.cancel(), FlipOutcome::Release);
    assert!(!g.is_pulling());
    // Nothing after it counts: a late lift is nothing, a second cancel is nothing.
    assert_eq!(g.lifted(60), FlipOutcome::Idle);
    assert_eq!(g.cancel(), FlipOutcome::Idle);
    assert_eq!(g.moved(100.0, 200.0, 70, &names), FlipOutcome::Idle);
}

#[test]
fn a_cancel_before_the_pull_began_is_nothing() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    assert_eq!(g.cancel(), FlipOutcome::Idle);
}

#[test]
fn a_position_that_is_not_a_number_ends_the_gesture_without_a_flip() {
    let p = panel(TabFlip::AtEdges, 1, 3, 0.0, 0.0);
    let mut g = FlipGesture::begin(p, 100.0, 400.0, 0, false);
    let _ = g.moved(100.0, 300.0, 50, &names);
    assert_eq!(g.moved(f32::NAN, 300.0, 60, &names), FlipOutcome::Release);
    assert_eq!(g.lifted(70), FlipOutcome::Idle);
}

#[test]
fn the_hint_text_says_which_way_and_where() {
    assert_eq!(hint_text(FlipDirection::Next, "Share"), "Next: Share");
    assert_eq!(hint_text(FlipDirection::Previous, "Home"), "Previous: Home");
}

#[test]
fn a_wheel_never_flips_unless_the_bar_says_always() {
    for flip in [TabFlip::Off, TabFlip::AtEdges] {
        let p = panel(flip, 1, 3, 0.0, 0.0);
        let mut wheel = WheelPush::default();
        for i in 0..10 {
            assert_eq!(wheel.step(&p, 60.0, i * 20), FlipOutcome::Idle);
        }
    }
}

#[test]
fn a_wheel_pushing_against_the_end_flips_once_per_push() {
    let p = panel(TabFlip::Always, 1, 3, 400.0, 400.0);
    let mut wheel = WheelPush::default();
    assert_eq!(wheel.step(&p, 50.0, 0), FlipOutcome::Idle);
    assert_eq!(wheel.step(&p, 50.0, 30), FlipOutcome::Idle);
    assert_eq!(
        wheel.step(&p, 50.0, 60),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        },
        "150 of push is past the 120"
    );
    // The trackpad's tail right after: the cooldown eats it.
    assert_eq!(wheel.step(&p, 80.0, 120), FlipOutcome::Idle);
    assert_eq!(wheel.step(&p, 80.0, 400), FlipOutcome::Idle);
    // After the cooldown a new push counts again.
    assert_eq!(
        wheel.step(&p, 130.0, 1500),
        FlipOutcome::Flip {
            direction: FlipDirection::Next,
            to: 2
        }
    );
}

#[test]
fn a_wheel_that_still_scrolls_the_content_is_the_contents() {
    let middle = panel(TabFlip::Always, 1, 3, 200.0, 400.0);
    let mut wheel = WheelPush::default();
    for i in 0..10 {
        assert_eq!(wheel.step(&middle, 60.0, i * 20), FlipOutcome::Idle);
        assert_eq!(wheel.step(&middle, -60.0, i * 20 + 10), FlipOutcome::Idle);
    }
}

#[test]
fn a_wheel_push_ends_with_a_silence_or_a_turn() {
    let p = panel(TabFlip::Always, 1, 3, 400.0, 400.0);
    let mut wheel = WheelPush::default();
    assert_eq!(wheel.step(&p, 100.0, 0), FlipOutcome::Idle);
    // The push stopped for longer than the reset: it starts from nothing.
    assert_eq!(
        wheel.step(&p, 100.0, WHEEL_RESET_MS + 50),
        FlipOutcome::Idle
    );
    // Turning round resets it as well (the content scrolls back: not at the top, so idle).
    assert_eq!(
        wheel.step(&p, -100.0, WHEEL_RESET_MS + 80),
        FlipOutcome::Idle
    );
}

#[test]
fn a_wheel_at_the_end_of_the_row_goes_nowhere() {
    let last = panel(TabFlip::Always, 2, 3, 0.0, 0.0);
    let mut wheel = WheelPush::default();
    let mut outcome = FlipOutcome::Idle;
    for i in 0..4 {
        outcome = wheel.step(&last, 60.0, i * 20);
    }
    assert_eq!(
        outcome,
        FlipOutcome::Release,
        "no tab after the last: no flip, no wrap"
    );
}

#[test]
fn the_defaults_flip_with_a_finger_and_not_with_a_wheel() {
    assert_eq!(TabFlip::default(), TabFlip::AtEdges);
}
