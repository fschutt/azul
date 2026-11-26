//! The pad's gestures: taps send nothing, a drag sends cumulative pans (the viewport the other way from the finger), a pinch zooms,
//! the two combine in one gesture, two quick taps fit the width, the page strip turns pages.

use super::*;

fn scale() -> PadScale {
    // the sheets take 600 pixels a unit on the pad (the Mac shows half the page's width, zoom 2, in a frame 300 wide)
    PadScale { px_per_unit: 600.0, direction: PanDirection::ContentFollowsFinger }
}

fn gesture_of(out: &[PadOut]) -> Vec<(u32, Phase, (f32, f32), f32)> {
    out.iter()
        .filter_map(|o| match o {
            PadOut::Gesture { id, phase, pan, zoom } => Some((*id, *phase, *pan, *zoom)),
            _ => None,
        })
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

// ---- taps

#[test]
fn a_tap_that_does_not_move_sends_nothing() {
    let mut pad = PadState::new();
    assert!(pad.pointer_down(0, 100.0, 100.0).is_empty());
    assert!(pad.pointer_move(10, 102.0, 101.0, &scale()).is_empty(), "inside the slop");
    assert!(pad.pointer_up(60, 102.0, 101.0).is_empty());
    assert_eq!(pad.live(), None);
}

#[test]
fn two_quick_taps_close_together_fit_the_width() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 100.0, 100.0);
    assert!(pad.pointer_up(50, 100.0, 100.0).is_empty());
    pad.pointer_down(150, 104.0, 98.0);
    assert!(pad.pointer_move(160, 105.0, 99.0, &scale()).is_empty(), "the jitter of the second tap is no drag");
    assert_eq!(pad.pointer_up(200, 105.0, 99.0), vec![PadOut::Fit]);
}

#[test]
fn two_taps_too_far_apart_in_time_are_two_taps() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 100.0, 100.0);
    pad.pointer_up(50, 100.0, 100.0);
    pad.pointer_down(50 + DOUBLE_TAP_MS + 1, 100.0, 100.0);
    assert!(pad.pointer_up(50 + DOUBLE_TAP_MS + 20, 100.0, 100.0).is_empty(), "too slow");
}

#[test]
fn two_taps_too_far_apart_in_space_are_two_taps() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 100.0, 100.0);
    pad.pointer_up(20, 100.0, 100.0);
    let far = 100.0 + DOUBLE_TAP_DISTANCE + 5.0;
    pad.pointer_down(40, far, 100.0);
    assert!(pad.pointer_up(60, far, 100.0).is_empty(), "too far");
}

#[test]
fn a_clock_that_runs_backwards_makes_no_double_tap() {
    let mut pad = PadState::new();
    pad.pointer_down(500, 100.0, 100.0);
    pad.pointer_up(520, 100.0, 100.0);
    pad.pointer_down(10, 100.0, 100.0);
    assert!(pad.pointer_up(30, 100.0, 100.0).is_empty());
}

#[test]
fn a_third_tap_does_not_fit_again_and_a_tap_after_a_drag_does_not_count() {
    let mut pad = PadState::new();
    for t in [0, 100] {
        pad.pointer_down(t, 50.0, 50.0);
        pad.pointer_up(t + 20, 50.0, 50.0);
    }
    pad.pointer_down(200, 50.0, 50.0);
    assert!(pad.pointer_up(220, 50.0, 50.0).is_empty(), "the third tap starts a new pair");
    // a drag, then a tap right after it: no double tap
    let mut pad = PadState::new();
    pad.pointer_down(0, 50.0, 50.0);
    pad.pointer_move(10, 90.0, 50.0, &scale());
    pad.pointer_up(20, 90.0, 50.0);
    pad.pointer_down(60, 90.0, 50.0);
    assert!(pad.pointer_up(80, 90.0, 50.0).is_empty());
}

// ---- a drag

#[test]
fn a_drag_starts_after_the_slop_and_the_view_goes_the_other_way_from_the_finger() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 0.0, 100.0);
    assert!(pad.pointer_move(5, 3.0, 100.0, &scale()).is_empty());
    let began = gesture_of(&pad.pointer_move(10, 8.0, 100.0, &scale()));
    assert_eq!(began.len(), 2, "a Begin and the first Update: {began:?}");
    assert_eq!((began[0].1, began[1].1), (Phase::Begin, Phase::Update));
    assert!(near(began[1].2 .0, 0.0) && near(began[1].2 .1, 0.0), "the pan starts from where the slop ended: {began:?}");
    let moved = gesture_of(&pad.pointer_move(20, 108.0, 100.0, &scale()));
    assert_eq!(moved.len(), 1);
    // 100 px right of the slop point on a 300 px pad, the view half a page wide: the viewport goes left by a third of it
    let expected = -100.0 * (0.5 / 300.0);
    assert!(near(moved[0].2 .0, expected), "{moved:?} wanted {expected}");
    assert!(near(moved[0].2 .1, 0.0));
    assert!(near(moved[0].3, 1.0), "no pinch: the zoom is 1");
}

#[test]
fn a_drag_down_moves_the_viewport_up_by_as_much_per_pixel_as_a_drag_across() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 100.0, 0.0);
    pad.pointer_move(10, 100.0, 10.0, &scale());
    let out = gesture_of(&pad.pointer_move(20, 100.0, 110.0, &scale()));
    // 100 px down: the content goes down by 100 / 600 of a unit, so the viewport goes up by that
    assert!(near(out[0].2 .1, -100.0 / 600.0), "{out:?}");
    assert!(near(out[0].2 .0, 0.0));
}

#[test]
fn a_finer_zoom_makes_a_finer_drag() {
    // zoomed in the sheets are bigger on the pad: more pixels a unit, so a pixel of finger is fewer units
    let zoomed = PadScale { px_per_unit: 6_000.0, ..scale() };
    let pan_for = |s: &PadScale| {
        let mut pad = PadState::new();
        pad.pointer_down(0, 0.0, 0.0);
        pad.pointer_move(1, 10.0, 0.0, s);
        gesture_of(&pad.pointer_move(2, 110.0, 0.0, s))[0].2 .0
    };
    assert!(pan_for(&zoomed).abs() < pan_for(&scale()).abs() / 4.0);
}

#[test]
fn the_content_follows_the_finger_unless_the_mac_says_to_reverse_it() {
    let pan_after = |direction| {
        let mut pad = PadState::new();
        pad.pointer_down(0, 100.0, 100.0);
        pad.pointer_move(1, 110.0, 100.0, &PadScale { direction, ..scale() });
        gesture_of(&pad.pointer_move(2, 160.0, 140.0, &PadScale { direction, ..scale() }))[0].2
    };
    let (right, down) = pan_after(PanDirection::ContentFollowsFinger);
    assert!(right < 0.0 && down < 0.0, "a finger that goes right and down moves the content right and down, the viewport left and up: {right} {down}");
    let (right, down) = pan_after(PanDirection::ViewportFollowsFinger);
    assert!(right > 0.0 && down > 0.0, "reversed, the viewport goes with the finger: {right} {down}");
    // and the sheets drawn under the finger move by the finger's own number of pixels: 50 px across is 50 / 600 of a unit
    assert!(near(pan_after(PanDirection::ViewportFollowsFinger).0, 50.0 / 600.0));
}

#[test]
fn the_end_carries_the_last_totals_and_the_next_gesture_has_a_new_id() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 0.0, 0.0);
    pad.pointer_move(1, 10.0, 0.0, &scale());
    let last = gesture_of(&pad.pointer_move(2, 60.0, 0.0, &scale()))[0];
    let ended = gesture_of(&pad.pointer_up(3, 60.0, 0.0));
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].1, Phase::End);
    assert!(near(ended[0].2 .0, last.2 .0) && ended[0].0 == last.0, "{ended:?} {last:?}");
    assert_eq!(pad.live(), None);
    pad.pointer_down(100, 0.0, 0.0);
    pad.pointer_move(101, 10.0, 0.0, &scale());
    let again = gesture_of(&pad.pointer_move(102, 40.0, 0.0, &scale()))[0];
    assert!(again.0 != last.0, "a new gesture, a new id");
}

#[test]
fn every_update_is_the_total_so_far_and_the_live_view_says_the_same() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 0.0, 0.0);
    pad.pointer_move(1, 10.0, 0.0, &scale());
    let update = gesture_of(&pad.pointer_move(2, 50.0, 20.0, &scale()))[0];
    let live = pad.live().expect("a drag is in progress");
    assert_eq!(live.id, update.0);
    assert!(near(live.pan.0, update.2 .0) && near(live.pan.1, update.2 .1) && near(live.zoom, update.3));
}

// ---- a pinch

#[test]
fn a_pinch_on_its_own_is_a_whole_gesture_begin_updates_end() {
    let mut pad = PadState::new();
    let begin = gesture_of(&pad.pinch(0, 1.0, true, false));
    assert_eq!(begin[0].1, Phase::Begin);
    let update = gesture_of(&pad.pinch(10, 1.5, false, false));
    assert_eq!(update.len(), 1);
    assert_eq!(update[0].1, Phase::Update);
    assert!(near(update[0].3, 1.5));
    let more = gesture_of(&pad.pinch(20, 2.0, false, false));
    assert!(near(more[0].3, 2.0));
    let end = gesture_of(&pad.pinch(30, 2.0, false, true));
    assert_eq!(end.last().map(|g| g.1), Some(Phase::End));
    assert!(near(end.last().unwrap().3, 2.0));
    assert_eq!(pad.live(), None);
}

#[test]
fn a_pinch_in_a_drag_is_the_same_gesture_and_a_second_pinch_multiplies() {
    let mut pad = PadState::new();
    pad.pointer_down(0, 0.0, 0.0);
    let started = gesture_of(&pad.pointer_move(1, 10.0, 0.0, &scale()));
    let id = started[0].0;
    let first = gesture_of(&pad.pinch(5, 1.0, true, false));
    assert!(first.iter().all(|g| g.0 == id && g.1 != Phase::Begin), "the drag's gesture goes on: {first:?}");
    let grown = gesture_of(&pad.pinch(6, 2.0, false, false));
    assert!(near(grown[0].3, 2.0) && grown[0].0 == id);
    assert!(gesture_of(&pad.pinch(7, 2.0, false, true)).iter().all(|g| g.1 != Phase::End), "a finger is still down");
    let second = gesture_of(&pad.pinch(8, 1.0, true, false));
    assert!(second.iter().all(|g| near(g.3, 2.0)), "the zoom so far is kept: {second:?}");
    let again = gesture_of(&pad.pinch(9, 1.5, false, false));
    assert!(near(again[0].3, 3.0), "2.0 times 1.5: {again:?}");
    let end = gesture_of(&pad.pointer_up(10, 20.0, 0.0));
    assert_eq!(end[0].1, Phase::End);
    assert!(near(end[0].3, 3.0) && end[0].0 == id);
}

#[test]
fn a_pinch_that_is_no_number_changes_nothing() {
    let mut pad = PadState::new();
    pad.pinch(0, 1.0, true, false);
    assert!(gesture_of(&pad.pinch(1, f32::NAN, false, false)).is_empty());
    assert!(gesture_of(&pad.pinch(2, 0.0, false, false)).is_empty());
    assert!(gesture_of(&pad.pinch(3, -1.0, false, false)).is_empty());
    assert!(gesture_of(&pad.pinch(4, f32::INFINITY, false, false)).is_empty());
}

// ---- the rest

#[test]
fn a_two_finger_tap_resets_the_view() {
    assert_eq!(PadState::new().secondary_tap(), vec![PadOut::Reset]);
}

#[test]
fn a_cancel_ends_the_gesture_where_it_is_and_a_cancel_with_none_sends_nothing() {
    let mut pad = PadState::new();
    assert!(pad.cancel().is_empty());
    pad.pointer_down(0, 0.0, 0.0);
    pad.pointer_move(1, 10.0, 0.0, &scale());
    pad.pointer_move(2, 50.0, 0.0, &scale());
    let out = gesture_of(&pad.cancel());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1, Phase::End);
    assert_eq!(pad.live(), None);
    assert!(pad.pointer_move(3, 80.0, 0.0, &scale()).is_empty(), "the finger is no longer tracked");
}

#[test]
fn events_out_of_order_or_with_nonsense_numbers_send_nothing_and_never_panic() {
    let mut pad = PadState::new();
    assert!(pad.pointer_move(0, 10.0, 10.0, &scale()).is_empty(), "a move with no finger down");
    assert!(pad.pointer_up(1, 10.0, 10.0).is_empty(), "an up with no finger down");
    pad.pointer_down(2, f32::NAN, 0.0);
    assert!(pad.pointer_move(3, f32::INFINITY, 0.0, &scale()).is_empty());
    assert!(pad.pointer_up(1, 0.0, 0.0).is_empty(), "the clock went back");
    let flat = PadScale { px_per_unit: 0.0, ..scale() };
    let mut pad = PadState::new();
    pad.pointer_down(0, 0.0, 0.0);
    let out = gesture_of(&pad.pointer_move(1, 50.0, 0.0, &flat));
    assert!(out.iter().all(|g| g.2 .0.is_finite() && g.2 .1.is_finite()), "a pad of no width pans by nothing: {out:?}");
}

// ---- the page strip

#[test]
fn a_swipe_on_the_strip_turns_the_page_up_for_the_next_and_down_for_the_previous() {
    assert_eq!(strip_step(300.0, 200.0, 400.0), Some(1));
    assert_eq!(strip_step(100.0, 200.0, 400.0), Some(-1));
    assert_eq!(strip_step(300.0, 300.0 - STRIP_SWIPE, 400.0), Some(1));
}

#[test]
fn a_tap_on_the_strip_turns_the_page_by_the_half_it_is_in() {
    assert_eq!(strip_step(350.0, 352.0, 400.0), Some(1), "the lower half");
    assert_eq!(strip_step(50.0, 48.0, 400.0), Some(-1), "the upper half");
}

#[test]
fn a_move_that_is_neither_a_swipe_nor_a_tap_turns_nothing() {
    assert_eq!(strip_step(100.0, 100.0 + STRIP_SWIPE / 2.0, 400.0), None);
    assert_eq!(strip_step(f32::NAN, 1.0, 400.0), None);
    assert_eq!(strip_step(1.0, f32::INFINITY, 400.0), None);
    assert_eq!(strip_step(1.0, 2.0, 0.0), None);
}
