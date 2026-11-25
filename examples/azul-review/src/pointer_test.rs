//! A stroke's way from the press to the end, as sequences of pointer events on the app's state, and the
//! log of the events. No window: the handlers feed these same functions.

use azul::{callbacks::Update, prelude::*};

use super::*;
use crate::{ink::Layer, model::InkPoint, AppState};

fn app() -> AppState {
    crate::lib_test::app_with_file()
}

fn at(x: f32, y: f32) -> InkPoint {
    InkPoint {
        x,
        y,
        pressure: 0.5,
        tilt_x: 0.0,
        tilt_y: 0.0,
    }
}

/// A press at (10, 10) and `n` moves along a line, the pointer pressed.
fn drawn(s: &mut AppState, n: usize) {
    s.stroke_begin(0, at(10.0, 10.0));
    for i in 1..=n {
        s.stroke_move(at(10.0 + 5.0 * i as f32, 10.0), true);
    }
}

// ---- the stroke from the press to its end ----

#[test]
fn a_press_moves_and_the_release_keep_the_stroke() {
    let mut s = app();
    drawn(&mut s, 5);
    assert!(s.live.is_some(), "the stroke is live while the pointer is down");

    let ended = s.stroke_end();

    assert_eq!(ended.map(|e| e.up), Some(Up::Kept));
    assert!(s.live.is_none());
    assert_eq!(s.strokes.len(), 1);
    assert_eq!(s.strokes[0].points.len(), 6, "the press and five moves");
}

#[test]
fn a_move_that_finds_the_pointer_not_pressed_ends_the_stroke_whose_release_was_lost() {
    // The release went to another node, to nobody (the pointer was off the page, a dialog took it): the
    // next move over the page finds the button up.
    let mut s = app();
    drawn(&mut s, 4);

    let moved = s.stroke_move(at(200.0, 80.0), false);

    assert!(matches!(moved, Moved::Ended(Ended { up: Up::Kept, .. })), "{moved:?}");
    assert!(s.live.is_none(), "no stroke is left to follow the cursor");
    assert_eq!(s.strokes.len(), 1, "what was drawn is kept");
    assert_eq!(s.strokes[0].points.len(), 5, "the unpressed point is not part of it");
}

#[test]
fn the_pointer_hovering_after_a_lost_release_adds_no_point_to_any_stroke() {
    let mut s = app();
    drawn(&mut s, 4);
    s.stroke_move(at(200.0, 80.0), false);

    for i in 0..30 {
        let moved = s.stroke_move(at(210.0 + i as f32, 90.0), false);
        assert_eq!(moved, Moved::Nothing);
    }
    assert!(s.live.is_none());
    assert_eq!(s.strokes[0].points.len(), 5, "the stroke did not grow from its last point");
}

#[test]
fn a_release_heard_anywhere_ends_the_stroke_and_a_second_one_ends_nothing() {
    let mut s = app();
    drawn(&mut s, 3);
    assert!(s.stroke_end().is_some());
    assert!(s.stroke_end().is_none(), "the layer's release and the window's: one stroke, one end");
    assert_eq!(s.strokes.len(), 1);
}

#[test]
fn a_press_while_a_stroke_is_live_keeps_that_stroke_and_begins_the_next() {
    let mut s = app();
    drawn(&mut s, 4);

    let began = s.stroke_begin(0, at(300.0, 300.0));

    assert_eq!(began.stale.map(|e| e.up), Some(Up::Kept), "the stroke whose release was lost is kept");
    assert_eq!(s.strokes.len(), 1);
    assert_eq!(s.live.as_ref().map(|l| l.points.len()), Some(1), "the new stroke starts at the press");
    assert_eq!(s.live.as_ref().map(|l| (l.points[0].x, l.points[0].y)), Some((300.0, 300.0)));
}

#[test]
fn a_hovering_pointer_makes_no_stroke() {
    let mut s = app();
    for i in 0..10 {
        assert_eq!(s.stroke_move(at(i as f32 * 9.0, 20.0), false), Moved::Nothing);
    }
    assert!(s.live.is_none() && s.strokes.is_empty());
}

#[test]
fn a_tap_is_no_ink_it_cycles_the_nib() {
    let mut s = app();
    s.stroke_begin(0, at(40.0, 40.0));
    let ended = s.stroke_end().expect("the tap ends");
    assert_eq!(ended.up, Up::Click);
    assert!(s.strokes.is_empty(), "a tap leaves no stroke");
}

#[test]
fn a_tap_while_a_dictation_listens_is_a_dab_and_changes_nothing() {
    let mut s = app();
    s.voice.running = Some(crate::voice::Running::for_test(1, 1));
    s.stroke_begin(0, at(40.0, 40.0));
    assert_eq!(s.stroke_end().map(|e| e.up), Some(Up::Dab));
}

#[test]
fn a_move_that_adds_less_than_a_third_of_a_pixel_is_not_a_point() {
    let mut s = app();
    s.stroke_begin(0, at(10.0, 10.0));
    assert_eq!(s.stroke_move(at(10.1, 10.1), true), Moved::Nothing);
    assert_eq!(s.stroke_move(at(11.0, 10.0), true), Moved::Grew(Layer::of(s.live.as_ref().unwrap().semantic)));
}

#[test]
fn the_ended_stroke_names_its_page_and_layer_so_the_frame_is_drawn_anew() {
    let mut s = app();
    s.stroke_begin(2, at(10.0, 10.0));
    s.stroke_move(at(30.0, 10.0), true);
    s.stroke_move(at(60.0, 10.0), true);
    let ended = s.stroke_end().expect("a stroke");
    assert_eq!(ended.page, 2);
    assert_eq!(ended.layer, Layer::of(s.strokes[0].semantic));
}

// ---- what a move may ask of the window ----

#[test]
fn a_move_asks_for_no_new_window_while_a_stroke_is_live() {
    // A rebuilt window replaces the pen's layer under the stroke.
    assert_eq!(refresh_after_move(true, Update::RefreshDom), Update::DoNothing);
    assert_eq!(refresh_after_move(false, Update::RefreshDom), Update::RefreshDom);
    assert_eq!(refresh_after_move(false, Update::DoNothing), Update::DoNothing);
}

// ---- the log ----

fn logged(kind: Kind, x: f32) -> Logged {
    Logged {
        kind,
        page: Some(0),
        x,
        y: 2.0,
        pressure: 0.0,
        pressed: true,
        pen: false,
        live: None,
    }
}

#[test]
fn the_log_keeps_the_last_events_and_counts_all_of_them() {
    let mut log = PointerLog::new(Echo::Off);
    for i in 0..(LOG_CAPACITY + 50) {
        log.push(logged(Kind::Move, i as f32));
    }
    assert_eq!(log.len(), LOG_CAPACITY);
    assert_eq!(log.total(), (LOG_CAPACITY + 50) as u64);
    let events = log.events();
    assert_eq!(events.first().map(|e| e.x), Some(50.0), "the oldest 50 went");
    assert_eq!(events.last().map(|e| e.x), Some((LOG_CAPACITY + 49) as f32));
}

#[test]
fn a_line_of_the_log_says_what_where_and_whether_a_stroke_was_live() {
    let mut log = PointerLog::new(Echo::Off);
    log.push(Logged {
        kind: Kind::WindowUp,
        page: None,
        x: 0.0,
        y: 0.0,
        pressure: 0.0,
        pressed: false,
        pen: false,
        live: Some(7),
    });
    let text = log.text();
    assert!(text.contains("window-up"), "{text}");
    assert!(text.contains("page=-"), "{text}");
    assert!(text.contains("pressed=0"), "{text}");
    assert!(text.contains("live=7pts"), "{text}");
    assert!(text.starts_with("AZREVIEW_EVENT #1 "), "{text}");
}

#[test]
fn the_log_as_json_is_an_array_with_one_object_per_event() {
    let mut log = PointerLog::new(Echo::Off);
    assert_eq!(log.json(), "[]");
    log.push(logged(Kind::Down, 5.0));
    log.push(Logged {
        pen: true,
        pressure: f32::NAN,
        ..logged(Kind::Up, f32::INFINITY)
    });
    let json = log.json();
    assert!(json.starts_with("[{") && json.ends_with("}]"), "{json}");
    assert_eq!(json.matches("\"kind\"").count(), 2, "{json}");
    assert!(json.contains("\"kind\":\"down\"") && json.contains("\"kind\":\"up\""), "{json}");
    assert!(!json.contains("NaN") && !json.contains("inf"), "numbers a JSON text can hold: {json}");
}

#[test]
fn the_switch_says_where_the_events_are_echoed() {
    assert_eq!(Echo::parse(None), Echo::Off);
    assert_eq!(Echo::parse(Some("")), Echo::Off);
    assert_eq!(Echo::parse(Some("0")), Echo::Off);
    assert_eq!(Echo::parse(Some("1")), Echo::Stderr);
    assert_eq!(
        Echo::parse(Some("/tmp/events.log")),
        Echo::File(std::path::PathBuf::from("/tmp/events.log"))
    );
}
