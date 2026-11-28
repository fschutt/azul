//! The touch hit-slop rule: the effective rect, the visual beating the slop, the nearest centre,
//! the tie by paint order, the clips. No tree, no window.

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

fn at(x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(x, y)
}

fn candidate(id: u32, visual: LogicalRect, order: usize) -> SlopCandidate<u32> {
    SlopCandidate { id, visual, clip: None, order }
}

#[test]
fn the_floor_of_a_platform_is_the_one_of_the_other_touch_floors() {
    assert_eq!(touch_floor("android"), Some(48.0));
    assert_eq!(touch_floor("ios"), Some(44.0));
    for desktop in ["linux", "macos", "windows", "freebsd", ""] {
        assert_eq!(touch_floor(desktop), None, "{desktop}: a pointer needs no slop");
    }
}

#[test]
fn a_small_box_grows_to_the_floor_round_its_centre() {
    // A switch's track: 36 x 20 at (100, 200).
    let grown = effective_rect(rect(100.0, 200.0, 36.0, 20.0), 48.0);
    assert_eq!(grown, rect(94.0, 186.0, 48.0, 48.0));
    // The centre did not move.
    assert_eq!(centre(grown), centre(rect(100.0, 200.0, 36.0, 20.0)));
}

#[test]
fn only_the_axis_that_is_short_grows() {
    assert_eq!(effective_rect(rect(0.0, 0.0, 200.0, 16.0), 48.0), rect(0.0, -16.0, 200.0, 48.0));
    assert_eq!(effective_rect(rect(0.0, 0.0, 14.0, 80.0), 48.0), rect(-17.0, 0.0, 48.0, 80.0));
}

#[test]
fn a_box_at_or_above_the_floor_is_unchanged() {
    for r in [rect(5.0, 6.0, 48.0, 48.0), rect(5.0, 6.0, 120.0, 64.0), rect(0.0, 0.0, 48.0, 400.0)] {
        assert_eq!(effective_rect(r, 48.0), r);
    }
    // Under iOS's 44 a 46 px box needs nothing, under Android's 48 it grows.
    assert_eq!(effective_rect(rect(0.0, 0.0, 46.0, 46.0), 44.0), rect(0.0, 0.0, 46.0, 46.0));
    assert_eq!(effective_rect(rect(0.0, 0.0, 46.0, 46.0), 48.0), rect(-1.0, -1.0, 48.0, 48.0));
}

/// The four numbers of both rects are the same bits (a NaN equals itself here).
fn same(a: LogicalRect, b: LogicalRect) -> bool {
    [a.origin.x, a.origin.y, a.size.width, a.size.height]
        .iter()
        .zip([b.origin.x, b.origin.y, b.size.width, b.size.height])
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn a_rect_that_is_no_finite_size_is_left_alone() {
    for r in [
        rect(f32::NAN, 0.0, 10.0, 10.0),
        rect(0.0, 0.0, f32::INFINITY, 10.0),
        rect(0.0, 0.0, -4.0, 10.0),
    ] {
        assert!(same(effective_rect(r, 48.0), r), "{r:?}");
    }
}

#[test]
fn a_touch_beside_a_small_box_is_for_it() {
    let switch = [candidate(1, rect(100.0, 200.0, 36.0, 20.0), 0)];
    // 5 px left of the track (inside the 6 px the floor adds), and 13 px under it (the floor adds 14).
    assert_eq!(pick(at(95.0, 210.0), &switch, 48.0), Some(1));
    assert_eq!(pick(at(118.0, 233.0), &switch, 48.0), Some(1));
    // Further than the floor reaches: nothing.
    assert_eq!(pick(at(93.0, 210.0), &switch, 48.0), None);
    assert_eq!(pick(at(118.0, 235.0), &switch, 48.0), None);
}

#[test]
fn a_touch_on_the_box_itself_is_the_normal_hit_tests() {
    // The point is inside the visual rect: the slop has nothing to say, the normal hit test answers.
    let switch = [candidate(1, rect(100.0, 200.0, 36.0, 20.0), 0)];
    assert_eq!(pick(at(118.0, 210.0), &switch, 48.0), None);
}

#[test]
fn the_visual_of_a_neighbour_beats_a_slop() {
    // Two switches 4 px apart: the left one's track ends at x = 136, the right one's starts at 140;
    // their grown rects reach 142 and 134.
    let two = [
        candidate(1, rect(100.0, 200.0, 36.0, 20.0), 0),
        candidate(2, rect(140.0, 200.0, 36.0, 20.0), 1),
    ];
    // A touch on the right one's own pixels, which lie in the left one's slop too, is the right
    // one's: the normal hit test answers, the slop says nothing.
    assert_eq!(pick(at(141.0, 210.0), &two, 48.0), None);
    // In the gap, outside both visuals, the nearer centre (118 and 158) wins: 137 is nearer to the
    // left one, 139 to the right one, and 138 is a tie that goes to the one in front.
    assert_eq!(pick(at(137.0, 210.0), &two, 48.0), Some(1));
    assert_eq!(pick(at(139.0, 210.0), &two, 48.0), Some(2));
    assert_eq!(pick(at(138.0, 210.0), &two, 48.0), Some(2));
}

#[test]
fn the_nearest_centre_wins_and_a_tie_goes_to_the_one_in_front() {
    let a = candidate(1, rect(0.0, 0.0, 20.0, 20.0), 0);
    let b = candidate(2, rect(30.0, 0.0, 20.0, 20.0), 5);
    // Halfway between the two centres (10 and 40): a tie, the front one (order 5) wins.
    assert_eq!(pick(at(25.0, 10.0), &[a, b], 48.0), Some(2));
    assert_eq!(pick(at(25.0, 10.0), &[b, a], 48.0), Some(2), "whatever the order of the list");
    // Closer to the first.
    assert_eq!(pick(at(22.0, 10.0), &[a, b], 48.0), Some(1));
}

#[test]
fn a_clip_cuts_the_slop_as_it_cuts_the_visual() {
    // A box at the top of a scroller whose viewport starts at y = 0: its slop above it is cut.
    let clip = rect(0.0, 0.0, 300.0, 400.0);
    let at_the_top = SlopCandidate { id: 1, visual: rect(50.0, 2.0, 36.0, 20.0), clip: Some(clip), order: 0 };
    // y = -5 lies in the grown rect (it starts at y = -12) but outside the viewport.
    assert_eq!(pick(at(60.0, -5.0), &[at_the_top], 48.0), None);
    // Below it, inside the viewport, the slop works.
    assert_eq!(pick(at(60.0, 30.0), &[at_the_top], 48.0), Some(1));
    // A box clipped away completely is nowhere.
    let gone = SlopCandidate { id: 2, visual: rect(50.0, 500.0, 36.0, 20.0), clip: Some(clip), order: 0 };
    assert_eq!(pick(at(60.0, 505.0), &[gone], 48.0), None);
}

#[test]
fn a_hostile_point_or_floor_is_nothing() {
    let b = [candidate(1, rect(0.0, 0.0, 20.0, 20.0), 0)];
    assert_eq!(pick(at(f32::NAN, 5.0), &b, 48.0), None);
    assert_eq!(pick(at(5.0, f32::INFINITY), &b, 48.0), None);
    assert_eq!(pick(at(-5.0, 5.0), &b, f32::NAN), None);
    assert_eq!(pick(at(-5.0, 5.0), &b, 0.0), None);
    assert_eq!(pick(at(-5.0, 5.0), &[] as &[SlopCandidate<u32>], 48.0), None);
}
