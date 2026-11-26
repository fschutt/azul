//! The touch slop: when a finger that went down is a tap and when it is a pan. No window, no clock.

use super::*;
use super::super::metrics::{frame_factor, to_logical};

/// A finger on a 440 dpi phone (density scale 2.75): the engine's logical pixel is the Android dp.
fn at_440_dpi(physical_px: f32) -> f32 {
    to_logical(physical_px, 264)
}

fn finger(down: (f32, f32)) -> TapSlop {
    let mut tap = TapSlop::default();
    tap.down(true, down, TOUCH_SLOP_DP);
    tap
}

/// A real finger jitters by a few physical pixels between the touch and the lift: that is a tap, the pointer stays where the
/// finger LANDED (the click is there), and no scroll starts. (`adb shell input tap` sends no move at all; a finger does.)
///
/// Mutation: no slop (every move pans at once: the page scrolled by the jitter and the click was lost whenever the node
/// under the finger changed).
#[test]
fn a_jitter_of_a_few_pixels_is_a_tap_that_clicks_where_the_finger_landed() {
    let down = (100.0, 200.0);
    let mut tap = finger(down);
    for jitter in [(2.0, 1.0), (-3.0, 2.0), (3.0, -3.0), (0.0, 0.0), (-2.0, -2.0)] {
        let pos = (down.0 + at_440_dpi(jitter.0), down.1 + at_440_dpi(jitter.1));
        assert_eq!(tap.moved(pos), Move::Hold, "{jitter:?} physical px is inside the slop");
    }
    assert_eq!(tap.up(), Lift::Click { at: Some(down) }, "the click is at the DOWN position, not at the last one");
}

/// A drag of 10 dp is a pan: the move that crosses the slop starts it, and the lift is no click.
///
/// Mutation: the lift of a pan reported as a click (a drag that ended on the node it began on activated it).
#[test]
fn a_drag_of_ten_dp_is_a_pan_and_the_lift_is_no_click() {
    let mut tap = finger((100.0, 200.0));
    // 10 dp straight down: 8 dp of it is the slop, 2 dp is the first pan delta
    let Move::Start { delta } = tap.moved((100.0, 210.0)) else {
        panic!("a move of 10 dp must start the pan");
    };
    assert_eq!(delta.0, 0.0);
    assert!((delta.1 - 2.0).abs() < 1e-5, "the first delta is the displacement beyond the slop: {delta:?}");
    // from then on every move is the pan's
    assert_eq!(tap.moved((100.0, 230.0)), Move::Follow);
    assert_eq!(tap.up(), Lift::Pan);
}

/// The first delta does not jump: it is the displacement MINUS the slop along the direction (Android's ScrollView does
/// the same: `deltaY -= mTouchSlop`), also on a diagonal.
#[test]
fn the_first_pan_delta_is_the_displacement_beyond_the_slop_along_its_direction() {
    let mut tap = finger((0.0, 0.0));
    // (6, 8): length 10, 8 of it slop: 2 of 10 remain
    let Move::Start { delta } = tap.moved((6.0, 8.0)) else {
        panic!("length 10 > 8");
    };
    assert!((delta.0 - 1.2).abs() < 1e-5 && (delta.1 - 1.6).abs() < 1e-5, "{delta:?}");
    // to the left and up as well
    let mut tap = finger((50.0, 50.0));
    let Move::Start { delta } = tap.moved((50.0 - 12.0, 50.0)) else {
        panic!("12 > 8");
    };
    assert!((delta.0 + 4.0).abs() < 1e-5 && delta.1 == 0.0, "{delta:?}");
}

/// The slop is 8 dp: 22 physical pixels at the density scale 2.75, and the distance is the straight line (a diagonal of
/// 5 by 5 is inside it, 6 by 6 is outside). Exactly at the slop is still a tap (Android: `> mTouchSlop`).
#[test]
fn the_slop_is_eight_dp_scaled_by_the_density() {
    assert_eq!(TOUCH_SLOP_DP, 8.0);
    assert!((slop_px(frame_factor(264)) - 22.0).abs() < 1e-4, "22 px at 440 dpi (scale 2.75)");
    assert_eq!(slop_px(1.0), 8.0);
    // 21 physical px at 2.75 is 7.6 dp: a tap; 23 px is 8.4 dp: a pan
    assert_eq!(finger((0.0, 0.0)).moved((0.0, at_440_dpi(21.0))), Move::Hold);
    assert!(matches!(finger((0.0, 0.0)).moved((0.0, at_440_dpi(23.0))), Move::Start { .. }));
    // the straight-line distance
    assert_eq!(finger((0.0, 0.0)).moved((5.0, 5.0)), Move::Hold, "7.07 dp");
    assert!(matches!(finger((0.0, 0.0)).moved((6.0, 6.0)), Move::Start { .. }), "8.49 dp");
    assert_eq!(finger((0.0, 0.0)).moved((8.0, 0.0)), Move::Hold, "exactly the slop");
}

/// Once past the slop the contact is a pan for good: a drag that comes back to where it started is still a pan, and its
/// lift is still no click.
///
/// Mutation: the slop checked against the last position again at every move (a pan that returned became a tap).
#[test]
fn a_pan_that_returns_to_the_start_stays_a_pan() {
    let mut tap = finger((10.0, 10.0));
    assert!(matches!(tap.moved((10.0, 30.0)), Move::Start { .. }));
    assert_eq!(tap.moved((10.0, 10.0)), Move::Follow, "back on the starting point");
    assert_eq!(tap.moved((11.0, 10.0)), Move::Follow);
    assert_eq!(tap.up(), Lift::Pan);
}

/// A mouse, a stylus and an eraser have no slop (`applies` is false for them): every move is a pointer move at once and the
/// lift is a click wherever the pointer is.
#[test]
fn a_contact_the_slop_does_not_apply_to_moves_at_once() {
    let mut tap = TapSlop::default();
    tap.down(false, (0.0, 0.0), TOUCH_SLOP_DP);
    assert_eq!(tap.moved((1.0, 0.0)), Move::Follow);
    assert_eq!(tap.moved((50.0, 50.0)), Move::Follow);
    assert_eq!(tap.up(), Lift::Click { at: None });
}

/// A cancelled contact (the system took the gesture) leaves nothing behind: the next contact starts fresh, and a lift with
/// no contact is the plain click it always was.
#[test]
fn a_cancel_and_a_new_contact_start_fresh() {
    let mut tap = finger((0.0, 0.0));
    assert!(matches!(tap.moved((0.0, 40.0)), Move::Start { .. }));
    tap.cancel();
    assert_eq!(tap.up(), Lift::Click { at: None }, "no contact");
    tap.down(true, (5.0, 5.0), TOUCH_SLOP_DP);
    assert_eq!(tap.moved((6.0, 6.0)), Move::Hold, "the new contact is a tap candidate again");
    assert_eq!(tap.up(), Lift::Click { at: Some((5.0, 5.0)) });
}

/// A slop a caller chose (the platform's own, read from `ViewConfiguration`, would be one) is used as given; a nonsense
/// one (zero, negative, NaN) is the default, never a contact that pans at once or never.
#[test]
fn a_slop_that_makes_no_sense_is_the_default() {
    let mut tap = TapSlop::default();
    tap.down(true, (0.0, 0.0), 0.0);
    assert_eq!(tap.moved((3.0, 0.0)), Move::Hold, "a zero slop is the default 8 dp");
    let mut tap = TapSlop::default();
    tap.down(true, (0.0, 0.0), f32::NAN);
    assert_eq!(tap.moved((3.0, 0.0)), Move::Hold);
    let mut tap = TapSlop::default();
    tap.down(true, (0.0, 0.0), 20.0);
    assert_eq!(tap.moved((15.0, 0.0)), Move::Hold, "a given slop of 20 dp");
    assert!(matches!(tap.moved((21.0, 0.0)), Move::Start { .. }));
}
