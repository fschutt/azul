//! The touch hit-slop rule: the effective rect, the visual beating the slop, the nearest centre,
//! the tie by paint order, the clips. No tree, no window.

use azul_css::system::Platform;

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

fn at(x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(x, y)
}

fn candidate(id: u32, visual: LogicalRect, order: usize) -> SlopCandidate<u32> {
    SlopCandidate { id, visual, point: at(0.0, 0.0), inside_clips: true, order }
}

/// `c` for a touch at (x, y) in its own space.
fn touched(c: SlopCandidate<u32>, x: f32, y: f32) -> SlopCandidate<u32> {
    SlopCandidate { point: at(x, y), ..c }
}

/// The candidates `boxes` all for the touch at (x, y) (one space).
fn all_at(x: f32, y: f32, boxes: &[SlopCandidate<u32>]) -> Vec<SlopCandidate<u32>> {
    boxes.iter().map(|c| touched(*c, x, y)).collect()
}

#[test]
fn the_floor_of_a_platform_is_the_one_of_the_other_touch_floors() {
    assert_eq!(touch_floor(&Platform::Android), Some(48.0));
    assert_eq!(touch_floor(&Platform::Ios), Some(44.0));
    for desktop in [Platform::Windows, Platform::MacOs, Platform::Unknown] {
        assert_eq!(touch_floor(&desktop), None, "{desktop:?}: a pointer needs no slop");
    }
}

#[test]
fn a_node_with_a_click_a_press_or_a_tab_stop_is_interactive_and_a_plain_one_is_not() {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{Dom, TabIndex},
        refany::{OptionRefAny, RefAny},
    };
    let with = |event: EventFilter| {
        Dom::create_div().with_callbacks(
            vec![CoreCallbackData {
                event,
                callback: CoreCallback { cb: 0, ctx: OptionRefAny::None },
                refany: RefAny::new(0_u8),
            }]
            .into(),
        )
    };
    let node = |dom: Dom| dom.root;
    assert!(!node_is_interactive(&node(Dom::create_div())));
    for filter in [
        HoverEventFilter::Click,
        HoverEventFilter::MouseDown,
        HoverEventFilter::MouseUp,
        HoverEventFilter::TouchStart,
        HoverEventFilter::TouchEnd,
    ] {
        assert!(node_is_interactive(&node(with(EventFilter::Hover(filter)))), "{filter:?}");
    }
    // A hover or a scroll listener is not a control.
    for filter in [HoverEventFilter::MouseEnter, HoverEventFilter::MouseLeave, HoverEventFilter::Scroll] {
        assert!(!node_is_interactive(&node(with(EventFilter::Hover(filter)))), "{filter:?}");
    }
    assert!(node_is_interactive(&node(Dom::create_div().with_tab_index(TabIndex::Auto))));
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
    assert_eq!(pick(&all_at(95.0, 210.0, &switch), 48.0), Some(1));
    assert_eq!(pick(&all_at(118.0, 233.0, &switch), 48.0), Some(1));
    // Further than the floor reaches: nothing.
    assert_eq!(pick(&all_at(93.0, 210.0, &switch), 48.0), None);
    assert_eq!(pick(&all_at(118.0, 235.0, &switch), 48.0), None);
}

#[test]
fn a_touch_on_the_box_itself_is_the_normal_hit_tests() {
    // The point is inside the visual rect: the slop has nothing to say, the normal hit test answers.
    let switch = [candidate(1, rect(100.0, 200.0, 36.0, 20.0), 0)];
    assert_eq!(pick(&all_at(118.0, 210.0, &switch), 48.0), None);
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
    assert_eq!(pick(&all_at(141.0, 210.0, &two), 48.0), None);
    // In the gap, outside both visuals, the nearer centre (118 and 158) wins: 137 is nearer to the
    // left one, 139 to the right one, and 138 is a tie that goes to the one in front.
    assert_eq!(pick(&all_at(137.0, 210.0, &two), 48.0), Some(1));
    assert_eq!(pick(&all_at(139.0, 210.0, &two), 48.0), Some(2));
    assert_eq!(pick(&all_at(138.0, 210.0, &two), 48.0), Some(2));
}

#[test]
fn the_nearest_centre_wins_and_a_tie_goes_to_the_one_in_front() {
    let a = candidate(1, rect(0.0, 0.0, 20.0, 20.0), 0);
    let b = candidate(2, rect(30.0, 0.0, 20.0, 20.0), 5);
    // Halfway between the two centres (10 and 40): a tie, the front one (order 5) wins.
    assert_eq!(pick(&all_at(25.0, 10.0, &[a, b]), 48.0), Some(2));
    assert_eq!(pick(&all_at(25.0, 10.0, &[b, a]), 48.0), Some(2), "whatever the order of the list");
    // Closer to the first.
    assert_eq!(pick(&all_at(22.0, 10.0, &[a, b]), 48.0), Some(1));
}

#[test]
fn every_box_is_looked_at_in_its_own_space() {
    // The same touch is (95, 210) for a box in the page and (95, 90) for one in a scrolled frame that
    // moved up by 120: each decides with its own point.
    let page = touched(candidate(1, rect(100.0, 200.0, 36.0, 20.0), 0), 95.0, 210.0);
    let scrolled = touched(candidate(2, rect(100.0, 80.0, 36.0, 20.0), 1), 95.0, 90.0);
    assert_eq!(pick(&[page], 48.0), Some(1));
    assert_eq!(pick(&[scrolled], 48.0), Some(2));
    // Both are within reach (their own spaces) and tie on the distance to their centres: the front one.
    assert_eq!(pick(&[page, scrolled], 48.0), Some(2));
    // A box in another space whose own point is on its pixels still beats the other's slop.
    let on_it = touched(candidate(3, rect(100.0, 200.0, 36.0, 20.0), 2), 105.0, 205.0);
    assert_eq!(pick(&[page, on_it], 48.0), None);
}

#[test]
fn a_clip_cuts_the_slop_as_it_cuts_the_visual() {
    // A box at the top of a scroller whose viewport starts at y = 0: a touch above the viewport is
    // inside the grown rect (it starts at y = -12) but outside the clip.
    let above = SlopCandidate { inside_clips: false, ..touched(candidate(1, rect(50.0, 2.0, 36.0, 20.0), 0), 60.0, -5.0) };
    assert_eq!(pick(&[above], 48.0), None);
    // Below it, inside the viewport, the slop works.
    let below = touched(candidate(1, rect(50.0, 2.0, 36.0, 20.0), 0), 60.0, 30.0);
    assert_eq!(pick(&[below], 48.0), Some(1));
    // A box whose visual the clip cuts away does not beat a neighbour's slop either.
    let cut_away = SlopCandidate { inside_clips: false, ..touched(candidate(2, rect(100.0, 200.0, 36.0, 20.0), 1), 118.0, 210.0) };
    let near = touched(candidate(3, rect(100.0, 222.0, 36.0, 20.0), 2), 118.0, 210.0);
    assert_eq!(pick(&[cut_away, near], 48.0), Some(3));
}

#[test]
fn a_hostile_point_or_floor_is_nothing() {
    let b = candidate(1, rect(0.0, 0.0, 20.0, 20.0), 0);
    assert_eq!(pick(&[touched(b, f32::NAN, 5.0)], 48.0), None);
    assert_eq!(pick(&[touched(b, 5.0, f32::INFINITY)], 48.0), None);
    assert_eq!(pick(&[touched(b, -5.0, 5.0)], f32::NAN), None);
    assert_eq!(pick(&[touched(b, -5.0, 5.0)], 0.0), None);
    assert_eq!(pick::<u32>(&[], 48.0), None);
}
