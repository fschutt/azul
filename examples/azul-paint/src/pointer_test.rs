//! One finger, one stroke: the gesture of a phone's finger reaches the canvas as `TouchStart` and as `MouseDown`
//! (the shell mirrors the primary finger into the mouse pipe), and only the first of the two may start a stroke.
//! No window: the decision is a pure function of the source, the time and what came before.

use crate::pointer::{Gesture, Source, ECHO_MS};

/// Plays a list of events `(source, kind, ms)` and returns the number of strokes started and ended.
#[derive(Clone, Copy)]
enum Kind {
    Press,
    Move,
    Release,
}

fn play(events: &[(Source, Kind, u64)]) -> (usize, usize, usize) {
    let mut gesture = Gesture::default();
    let (mut started, mut moved, mut ended) = (0, 0, 0);
    for &(from, kind, ms) in events {
        match kind {
            Kind::Press => started += usize::from(gesture.press(from, ms)),
            Kind::Move => moved += usize::from(gesture.moves(from, ms)),
            Kind::Release => ended += usize::from(gesture.release(from, ms)),
        }
    }
    (started, moved, ended)
}

#[test]
fn a_finger_that_raises_the_touch_first_and_the_mouse_after_it_starts_one_stroke() {
    use Kind::*;
    use Source::*;
    // TouchStart, MouseDown, two moves of each, TouchEnd, MouseUp: the shell's order for one finger
    let (started, moved, ended) = play(&[
        (Touch, Press, 1000),
        (Mouse, Press, 1000),
        (Touch, Move, 1016),
        (Mouse, Move, 1016),
        (Touch, Move, 1032),
        (Mouse, Move, 1032),
        (Touch, Release, 1048),
        (Mouse, Release, 1048),
    ]);
    assert_eq!((started, moved, ended), (1, 2, 1), "one stroke, its two moves, one end");
}

#[test]
fn the_other_order_is_one_stroke_too() {
    use Kind::*;
    use Source::*;
    let (started, moved, ended) = play(&[
        (Mouse, Press, 1000),
        (Touch, Press, 1000),
        (Mouse, Move, 1016),
        (Touch, Move, 1016),
        (Mouse, Release, 1048),
        (Touch, Release, 1048),
    ]);
    assert_eq!((started, moved, ended), (1, 1, 1));
}

#[test]
fn a_mouse_alone_is_accepted_whole() {
    use Kind::*;
    use Source::*;
    let (started, moved, ended) = play(&[
        (Mouse, Move, 900),
        (Mouse, Press, 1000),
        (Mouse, Move, 1016),
        (Mouse, Move, 1032),
        (Mouse, Release, 1048),
        (Mouse, Press, 1500),
        (Mouse, Release, 1600),
    ]);
    assert_eq!((started, moved, ended), (2, 3, 2), "two strokes, every move");
}

#[test]
fn a_touch_alone_is_accepted_whole() {
    use Kind::*;
    use Source::*;
    let (started, moved, ended) = play(&[
        (Touch, Press, 1000),
        (Touch, Move, 1016),
        (Touch, Release, 1048),
        (Touch, Press, 5000),
        (Touch, Release, 5100),
    ]);
    assert_eq!((started, moved, ended), (2, 1, 2));
}

#[test]
fn a_gesture_of_the_other_source_after_the_echo_window_is_a_gesture_of_its_own() {
    use Kind::*;
    use Source::*;
    // a finger, then - seconds later - a mouse: not shut out
    let (started, _, ended) = play(&[
        (Touch, Press, 1000),
        (Touch, Release, 1100),
        (Mouse, Press, 1100 + ECHO_MS + 1),
        (Mouse, Release, 1100 + ECHO_MS + 50),
    ]);
    assert_eq!((started, ended), (2, 2));
}

#[test]
fn a_long_drag_keeps_its_owner_because_every_move_refreshes_the_window() {
    use Kind::*;
    use Source::*;
    let mut events = vec![(Touch, Press, 0), (Mouse, Press, 0)];
    // ten seconds of moves of both sources, one every 16 ms
    let mut ms = 16;
    while ms < 10_000 {
        events.push((Touch, Move, ms));
        events.push((Mouse, Move, ms));
        ms += 16;
    }
    events.push((Touch, Release, ms));
    events.push((Mouse, Release, ms));
    let (started, moved, ended) = play(&events);
    assert_eq!((started, ended), (1, 1), "one stroke however long it lasts");
    assert_eq!(moved, events.iter().filter(|(s, k, _)| *s == Touch && matches!(k, Move)).count());
}

#[test]
fn a_pointer_that_left_the_canvas_owns_nothing() {
    let mut gesture = Gesture::default();
    assert!(gesture.press(Source::Touch, 1000));
    gesture.gone();
    assert!(gesture.press(Source::Mouse, 1010), "nobody owns the gesture after the pointer left");
}
