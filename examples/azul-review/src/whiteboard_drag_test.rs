//! A circled region and its way to the whiteboard: armed for a while, lifted by its grip or by pressing
//! and holding inside it, dragged to the bar. The strokes of `stroke_fixtures` are replayed through it:
//! a slash across a word inside an armed region is a slash, never a grab. No window, no azul.

use super::*;
use crate::{
    stroke_fixtures::{self as fx, Fixture},
    whiteboard_lasso::recognize,
};

const PAGE: Size = Size { w: 1000.0, h: 730.0 };

fn pts(f: &Fixture) -> Vec<Pt> {
    f.samples.iter().map(|s| Pt::new(s.x, s.y)).collect()
}

/// The page sits at (100, 50) in the window, so a point of the page is that much further in the window.
fn win(p: Pt) -> Pt {
    Pt::new(p.x + 100.0, p.y + 50.0)
}

fn at(page: usize, p: Pt) -> Pointer {
    Pointer { page, in_page: p, in_window: win(p) }
}

/// A region armed from the loop round the word, at time 0.
fn armed() -> (ArmedRegions, RegionId) {
    let lasso = recognize(&pts(&fx::closed_loop())).expect("a lasso");
    let mut regions = ArmedRegions::default();
    let id = regions.arm(Armed { page: 0, polygon: lasso.polygon, first_line: 12, last_line: 12, stroke_id: 7 }, PAGE, 0);
    (regions, id)
}

fn word() -> Pt {
    Pt::new(fx::WORD_CENTRE.0, fx::WORD_CENTRE.1)
}

/// The bar the drop goes to: 300 px wide at the right of a 1280 px window, open or the thin edge.
fn bar(open: bool) -> Bar {
    if open {
        Bar { rect: Rect::new(980.0, 30.0, 300.0, 760.0), open: true, window_w: 1280.0 }
    } else {
        Bar { rect: Rect::new(1268.0, 30.0, 12.0, 760.0), open: false, window_w: 1280.0 }
    }
}

// ---- armed regions

#[test]
fn a_region_is_armed_for_a_while_and_its_data_is_there_for_whoever_asks() {
    let (regions, id) = armed();
    let region = regions.armed_region_at(0, word(), 1_000).expect("the word is in the loop");
    assert_eq!(region.id, id);
    assert_eq!((region.first_line, region.last_line, region.stroke_id), (12, 12, 7));
    assert!(region.shape_svg().starts_with('M') && region.shape_svg().ends_with('Z'));
    assert!(region.bounds.contains(word()));
    assert!(regions.armed_region_at(0, Pt::new(333.0, 300.0), 1_000).is_none(), "outside the loop");
    assert!(regions.armed_region_at(1, word(), 1_000).is_none(), "another page");
    assert!(regions.armed_region_at(0, word(), ARM_MS - 1).is_some());
    assert!(regions.armed_region_at(0, word(), ARM_MS).is_none(), "the time is up");
}

#[test]
fn old_regions_go_and_the_oldest_is_dropped_past_the_cap() {
    let (mut regions, _) = armed();
    assert!(!regions.expire(ARM_MS - 1));
    assert!(regions.expire(ARM_MS), "it went");
    assert!(regions.is_empty());
    let lasso = recognize(&pts(&fx::closed_loop())).expect("a lasso");
    let mut ids = Vec::new();
    for i in 0..(MAX_ARMED + 3) {
        let draft = Armed { page: 0, polygon: lasso.polygon.clone(), first_line: 1, last_line: 1, stroke_id: i as u64 };
        ids.push(regions.arm(draft, PAGE, i as u64));
    }
    assert_eq!(regions.len(), MAX_ARMED);
    assert!(regions.get(ids[0]).is_none() && regions.get(ids[2]).is_none(), "the oldest went");
    assert!(regions.get(ids[ids.len() - 1]).is_some());
    // The newest is the one a point inside several answers with.
    assert_eq!(regions.armed_region_at(0, word(), 100).map(|r| r.id), ids.last().copied());
    regions.clear();
    assert!(regions.is_empty());
}

#[test]
fn a_region_can_be_taken_for_the_cutout_or_let_go() {
    let (mut regions, id) = armed();
    let taken = regions.take(id).expect("armed");
    assert_eq!(taken.id, id);
    assert!(regions.take(id).is_none());
    let (mut regions, id) = armed();
    regions.disarm(id);
    assert!(regions.get(id).is_none());
    let (mut regions, _) = armed();
    regions.clear_page(1);
    assert_eq!(regions.len(), 1);
    regions.clear_page(0);
    assert!(regions.is_empty());
}

#[test]
fn the_grip_sits_on_the_outline_at_the_top_right_and_stays_on_the_page() {
    let (regions, id) = armed();
    let region = regions.get(id).expect("armed");
    let grip = region.grip;
    assert!((grip.w - GRIP_PX).abs() < 1e-3 && (grip.h - GRIP_PX).abs() < 1e-3);
    let c = grip.center();
    // On the outline's top-right: right of the word's middle, above it.
    assert!(c.x > word().x + 20.0 && c.y < word().y - 5.0, "{c:?}");
    assert!(regions.grip_at(0, c, 100).is_some());
    assert!(regions.grip_at(0, word(), 100).is_none());
    assert!(regions.grip_at(1, c, 100).is_none());
    assert!(regions.grip_at(0, c, ARM_MS).is_none());
    // A loop at the very corner of the page keeps its grip on the page.
    let corner = vec![Pt::new(900.0, 5.0), Pt::new(998.0, 5.0), Pt::new(998.0, 60.0), Pt::new(900.0, 60.0)];
    let mut r = ArmedRegions::default();
    let cid = r.arm(Armed { page: 0, polygon: corner, first_line: 1, last_line: 1, stroke_id: 1 }, PAGE, 0);
    let g = r.get(cid).expect("armed").grip;
    assert!(g.x >= 0.0 && g.y >= 0.0 && g.right() <= PAGE.w && g.bottom() <= PAGE.h, "{g:?}");
}

// ---- a press

/// Replays the stroke `f` as a press at its first sample and a motion for every other one, at the times
/// the samples have; the page is `page`. What every motion said, then the release's, and a tick 400 ms on.
fn replay(f: &Fixture, regions: &ArmedRegions, drag: &mut Drag) -> (Press, Vec<Motion>, Option<Tick>, Release) {
    let first = f.samples[0];
    let press = drag.press(regions, at(0, Pt::new(first.x, first.y)), u64::from(first.t_ms));
    let mut moves = Vec::new();
    for s in &f.samples[1..] {
        moves.push(drag.motion(win(Pt::new(s.x, s.y)), u64::from(s.t_ms), &bar(true)));
    }
    let end = f.samples.last().map_or(0, |s| u64::from(s.t_ms));
    let tick = drag.tick(end + 400);
    let last = f.samples.last().map_or(Pt::default(), |s| Pt::new(s.x, s.y));
    let release = drag.release(win(last), &bar(true));
    (press, moves, tick, release)
}

#[test]
fn a_slash_across_the_word_inside_an_armed_region_is_a_slash_and_not_a_grab() {
    let (regions, id) = armed();
    for f in [fx::slash(), fx::slash_through_a_long_word()] {
        let mut drag = Drag::default();
        let (press, moves, tick, release) = replay(&f, &regions, &mut drag);
        assert_eq!(press, Press::Inside(id), "{}: it begins inside the loop", f.name);
        assert!(moves.contains(&Motion::Inking), "{}: it moved past the jitter before the hold: ink", f.name);
        assert!(moves.iter().all(|m| !matches!(m, Motion::Lifted(_) | Motion::Dragging { .. })), "{}: {moves:?}", f.name);
        assert_eq!(tick, None, "{}: there is nothing to hold any more", f.name);
        assert_eq!(release, Release::Nothing, "{}", f.name);
        assert!(drag.ghost().is_none());
    }
}

#[test]
fn an_underline_and_a_tap_inside_the_region_are_no_grab_either() {
    let (regions, id) = armed();
    let mut drag = Drag::default();
    let (press, moves, tick, release) = replay(&fx::underline(), &regions, &mut drag);
    assert_eq!(press, Press::Inside(id));
    assert!(moves.contains(&Motion::Inking));
    assert_eq!((tick, release), (None, Release::Nothing));
    // A tap: down and up in place in 60 ms.
    let mut drag = Drag::default();
    assert_eq!(drag.press(&regions, at(0, word()), 0), Press::Inside(id));
    assert_eq!(drag.tick(60), None, "not held long enough");
    assert_eq!(drag.release(win(word()), &bar(true)), Release::Nothing);
}

#[test]
fn a_press_outside_every_region_is_ink() {
    let (regions, _) = armed();
    let mut drag = Drag::default();
    assert_eq!(drag.press(&regions, at(0, Pt::new(333.0, 400.0)), 0), Press::Elsewhere);
    assert_eq!(drag.motion(win(Pt::new(400.0, 400.0)), 50, &bar(true)), Motion::Nothing);
    assert_eq!(drag.tick(1_000), None);
    assert_eq!(drag.press(&regions, at(1, word()), 0), Press::Elsewhere, "another page");
    // A region that ran out is no region.
    assert_eq!(drag.press(&regions, at(0, word()), ARM_MS), Press::Elsewhere);
}

#[test]
fn pressing_and_holding_inside_lifts_the_region_after_the_hold_time_and_then_it_follows_the_pointer() {
    let (regions, id) = armed();
    let mut drag = Drag::default();
    assert_eq!(drag.press(&regions, at(0, word()), 1_000), Press::Inside(id));
    assert_eq!(drag.tick(1_000 + HOLD_MS - 1), None, "one millisecond short");
    assert_eq!(drag.tick(1_000 + HOLD_MS), Some(Tick::Held(id)));
    assert_eq!(drag.tick(1_000 + HOLD_MS + 50), None, "told once");
    // A little movement while held is still held.
    assert_eq!(drag.motion(win(Pt::new(word().x + 3.0, word().y)), 1_400, &bar(true)), Motion::Nothing);
    // Past the drag distance it is lifted, and the region follows.
    let far = Pt::new(word().x + 40.0, word().y);
    assert_eq!(drag.motion(win(far), 1_450, &bar(true)), Motion::Lifted(id));
    assert_eq!(drag.ghost().map(|g| (g.region, g.at)), Some((id, win(far))));
    let m = drag.motion(Pt::new(1_050.0, 300.0), 1_500, &bar(true));
    assert_eq!(m, Motion::Dragging { region: id, over_bar: true, open_bar: false });
    assert_eq!(drag.ghost().map(|g| g.at), Some(Pt::new(1_050.0, 300.0)));
    // Let go over the bar: dropped there, in the bar's own coordinates.
    assert_eq!(
        drag.release(Pt::new(1_050.0, 300.0), &bar(true)),
        Release::Dropped { region: id, at_bar: Pt::new(70.0, 270.0) }
    );
    assert!(drag.ghost().is_none());
}

#[test]
fn a_press_that_moves_before_the_hold_time_is_ink_whatever_comes_after() {
    let (regions, id) = armed();
    let mut drag = Drag::default();
    assert_eq!(drag.press(&regions, at(0, word()), 0), Press::Inside(id));
    assert_eq!(drag.motion(win(Pt::new(word().x + HOLD_JITTER_PX + 1.0, word().y)), 120, &bar(true)), Motion::Inking);
    assert_eq!(drag.tick(1_000), None);
    assert_eq!(drag.motion(win(Pt::new(word().x + 200.0, word().y)), 1_100, &bar(true)), Motion::Nothing);
    assert_eq!(drag.release(win(word()), &bar(true)), Release::Nothing);
}

#[test]
fn a_hand_that_shakes_a_little_still_holds() {
    let (regions, id) = armed();
    let mut drag = Drag::default();
    drag.press(&regions, at(0, word()), 0);
    for (i, d) in [2.0_f32, -3.0, 4.0, -2.0, 3.0].into_iter().enumerate() {
        assert_eq!(drag.motion(win(Pt::new(word().x + d, word().y + d)), 50 * (i as u64 + 1), &bar(true)), Motion::Nothing);
    }
    assert_eq!(drag.tick(HOLD_MS), Some(Tick::Held(id)));
}

#[test]
fn a_motion_after_the_hold_time_is_a_hold_even_when_no_tick_came() {
    let (regions, id) = armed();
    let mut drag = Drag::default();
    drag.press(&regions, at(0, word()), 0);
    assert_eq!(drag.motion(win(Pt::new(word().x + 2.0, word().y)), HOLD_MS + 20, &bar(true)), Motion::Held(id));
    assert_eq!(drag.tick(HOLD_MS + 40), None, "told once");
}

#[test]
fn pressing_the_grip_lifts_at_once_with_no_wait() {
    let (regions, id) = armed();
    let grip = regions.get(id).expect("armed").grip.center();
    let mut drag = Drag::default();
    assert_eq!(drag.press(&regions, at(0, grip), 0), Press::Grip(id));
    assert_eq!(drag.tick(HOLD_MS * 2), None, "no hold to wait for");
    assert_eq!(drag.motion(win(Pt::new(grip.x + 3.0, grip.y)), 10, &bar(true)), Motion::Nothing, "a touch of the grip is not a drag");
    assert_eq!(drag.motion(win(Pt::new(grip.x + DRAG_START_PX + 2.0, grip.y)), 20, &bar(true)), Motion::Lifted(id));
}

#[test]
fn a_drag_that_reaches_the_edge_opens_the_closed_bar_once_and_a_drop_there_goes_in() {
    let (regions, id) = armed();
    let grip = regions.get(id).expect("armed").grip.center();
    let mut drag = Drag::default();
    drag.press(&regions, at(0, grip), 0);
    assert_eq!(drag.motion(win(Pt::new(grip.x + 20.0, grip.y)), 10, &bar(false)), Motion::Lifted(id));
    let far = drag.motion(Pt::new(1_000.0, 300.0), 30, &bar(false));
    assert_eq!(far, Motion::Dragging { region: id, over_bar: false, open_bar: false }, "still far from the edge");
    let near = drag.motion(Pt::new(1_280.0 - EDGE_OPEN_PX + 1.0, 300.0), 40, &bar(false));
    assert_eq!(near, Motion::Dragging { region: id, over_bar: false, open_bar: true }, "the edge: the bar opens");
    let again = drag.motion(Pt::new(1_270.0, 310.0), 50, &bar(false));
    assert_eq!(again, Motion::Dragging { region: id, over_bar: true, open_bar: false }, "told once");
    // The app opened the bar: the drop goes in.
    let dropped = drag.release(Pt::new(1_150.0, 310.0), &bar(true));
    assert_eq!(dropped, Release::Dropped { region: id, at_bar: Pt::new(170.0, 280.0) });
}

#[test]
fn a_drop_anywhere_else_or_on_a_bar_that_is_still_closed_puts_nothing_down() {
    let (regions, id) = armed();
    let grip = regions.get(id).expect("armed").grip.center();
    let mut drag = Drag::default();
    drag.press(&regions, at(0, grip), 0);
    drag.motion(win(Pt::new(grip.x + 20.0, grip.y)), 10, &bar(true));
    assert_eq!(drag.release(Pt::new(500.0, 300.0), &bar(true)), Release::Cancelled);
    drag.press(&regions, at(0, grip), 100);
    drag.motion(win(Pt::new(grip.x + 20.0, grip.y)), 110, &bar(false));
    assert_eq!(drag.release(Pt::new(1_274.0, 300.0), &bar(false)), Release::Cancelled, "the thin edge takes no drop");
}

#[test]
fn escape_puts_the_region_down_and_the_next_press_starts_afresh() {
    let (regions, id) = armed();
    let grip = regions.get(id).expect("armed").grip.center();
    let mut drag = Drag::default();
    assert!(!drag.cancel(), "nothing to cancel");
    drag.press(&regions, at(0, grip), 0);
    drag.motion(win(Pt::new(grip.x + 20.0, grip.y)), 10, &bar(true));
    assert!(drag.is_active());
    assert!(drag.cancel());
    assert!(!drag.is_active() && drag.ghost().is_none());
    assert_eq!(drag.release(Pt::new(1_050.0, 300.0), &bar(true)), Release::Nothing);
    assert_eq!(drag.press(&regions, at(0, grip), 500), Press::Grip(id));
}
