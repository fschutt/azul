//! When a held pointer has left the point it pressed at (the drag autoscroll of a text selection waits for it). No window.

use azul_core::geom::LogicalPosition;

use super::*;

static PRESS: LogicalPosition = LogicalPosition { x: 120.0, y: 523.0 };

fn at(dx: f32, dy: f32) -> LogicalPosition {
    LogicalPosition::new(PRESS.x + dx, PRESS.y + dy)
}

/// A pointer that has not moved since its press has left nothing, whatever the threshold: a click, not a drag.
///
/// Mutation: `>=` of the squared distance alone (a zero threshold then counts the press point as left).
#[test]
fn a_pointer_still_on_its_press_point_has_not_left_it_whatever_the_threshold() {
    for threshold in [0.0, 1.0, 5.0, f32::NAN, f32::INFINITY, -3.0] {
        assert!(
            !has_left_press_point(PRESS, PRESS, threshold),
            "threshold {threshold}"
        );
    }
}

/// The distance is the straight line, and the threshold itself counts: 3 and 4 px away is 5 px.
#[test]
fn a_pointer_leaves_its_press_point_at_the_threshold_by_the_straight_line() {
    assert!(has_left_press_point(PRESS, at(3.0, 4.0), 5.0));
    assert!(!has_left_press_point(PRESS, at(3.0, 3.9), 5.0));
    assert!(!has_left_press_point(PRESS, at(2.0, 2.0), 5.0), "a jitter of a few px is a click");
    assert!(has_left_press_point(PRESS, at(-6.0, 0.0), 5.0), "to the left, as to the right");
    assert!(has_left_press_point(PRESS, at(0.0, -5.0), 5.0), "upwards, as downwards");
}

/// A zero (or an unusable) threshold means "moved at all": a window that was configured with nothing sensible
/// still stops a press that did not move from scrolling, and a move of any size is a drag.
#[test]
fn an_unusable_threshold_means_the_pointer_has_moved_at_all() {
    for threshold in [0.0, -1.0, f32::NAN, f32::NEG_INFINITY] {
        assert!(
            has_left_press_point(PRESS, at(0.25, 0.0), threshold),
            "threshold {threshold}"
        );
    }
}

/// A pointer without a finite position has left nothing (never a drag out of a NaN).
#[test]
fn a_pointer_without_a_finite_position_has_not_left_the_press_point() {
    assert!(!has_left_press_point(PRESS, LogicalPosition::new(f32::NAN, 523.0), 5.0));
    assert!(!has_left_press_point(PRESS, LogicalPosition::new(120.0, f32::INFINITY), 5.0));
    assert!(!has_left_press_point(LogicalPosition::new(f32::NAN, 0.0), PRESS, 5.0));
}
