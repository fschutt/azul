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
        t_ms: 0,
    }
}

/// A press at (10, 10) and `n` moves along a line, the pointer pressed.
fn drawn(s: &mut AppState, n: usize) {
    s.stroke_begin(0, at(10.0, 10.0), false);
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

    let began = s.stroke_begin(0, at(300.0, 300.0), false);

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
fn a_tap_of_a_pen_is_no_ink_it_cycles_the_nib() {
    let mut s = app();
    s.stroke_begin(0, at(40.0, 40.0), true);
    let ended = s.stroke_end().expect("the tap ends");
    assert_eq!(ended.up, Up::Click);
    assert!(s.strokes.is_empty(), "a tap leaves no stroke");
}

#[test]
fn a_tap_of_a_mouse_or_a_trackpad_changes_no_nib_and_leaves_no_ink() {
    // A tap on a trackpad is a click, and a click cycled marker -> pen -> audio pen: the audio pen mounts the
    // microphone widget (a permission prompt). The nib is chosen in the title row, with the digit keys, with
    // the right button; a tap by a pointer that is no pen is nothing.
    let mut s = app();
    s.stroke_begin(0, at(40.0, 40.0), false);
    let ended = s.stroke_end().expect("the tap ends");
    assert_eq!(ended.up, Up::Dab);
    assert!(s.strokes.is_empty(), "a tap leaves no stroke");
}

#[test]
fn a_stroke_of_a_trackpad_is_kept_like_any_other() {
    let mut s = app();
    s.stroke_begin(0, at(10.0, 10.0), false);
    for i in 1..6 {
        s.stroke_move(at(10.0 + 5.0 * i as f32, 10.0), true);
    }
    assert_eq!(s.stroke_end().map(|e| e.up), Some(Up::Kept));
}

#[test]
fn a_tap_while_a_dictation_listens_is_a_dab_and_changes_nothing() {
    let mut s = app();
    s.voice.running = Some(crate::voice::Running::for_test(1, 1));
    s.stroke_begin(0, at(40.0, 40.0), true);
    assert_eq!(s.stroke_end().map(|e| e.up), Some(Up::Dab));
}

#[test]
fn a_move_that_adds_less_than_a_third_of_a_pixel_is_not_a_point() {
    let mut s = app();
    s.stroke_begin(0, at(10.0, 10.0), false);
    assert_eq!(s.stroke_move(at(10.1, 10.1), true), Moved::Nothing);
    assert_eq!(s.stroke_move(at(11.0, 10.0), true), Moved::Grew(Layer::of(s.live.as_ref().unwrap().semantic)));
}

#[test]
fn the_ended_stroke_names_its_page_and_layer_so_the_frame_is_drawn_anew() {
    let mut s = app();
    s.stroke_begin(2, at(10.0, 10.0), false);
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

// ---- the time of a stroke, and the quick slash ----

use crate::{
    lib_test::{app_with_slash_file, draw_fixture},
    stroke_fixtures::{self as fx, Meaning},
};

#[test]
fn a_stroke_keeps_the_time_of_each_sample_and_the_moment_it_began() {
    let mut s = app();
    let at_ms = |x: f32, t_ms: u32| InkPoint { t_ms, ..at(x, 10.0) };
    s.stroke_begin_at(0, at_ms(10.0, 0), false, 1_764_000_000_000);
    s.stroke_move_at(at_ms(20.0, 16), true);
    s.stroke_move_at(at_ms(30.0, 33), true);
    s.stroke_end();
    let kept = &s.strokes[0];
    assert_eq!(kept.started_unix_ms, 1_764_000_000_000);
    assert_eq!(kept.points.iter().map(|p| p.t_ms).collect::<Vec<_>>(), vec![0, 16, 33]);
}

#[test]
fn the_clock_stamps_the_moves_of_a_live_stroke_with_the_time_since_the_press() {
    let mut s = app();
    s.stroke_begin(0, at(10.0, 10.0), false);
    std::thread::sleep(std::time::Duration::from_millis(6));
    s.stroke_move(at(20.0, 10.0), true);
    std::thread::sleep(std::time::Duration::from_millis(6));
    s.stroke_move(at(30.0, 10.0), true);
    s.stroke_end();
    let kept = &s.strokes[0];
    let times: Vec<u32> = kept.points.iter().map(|p| p.t_ms).collect();
    assert_eq!(times[0], 0, "the press is the start of the time");
    assert!(times[1] >= 6 && times[2] >= times[1] + 6, "time runs forward: {times:?}");
    assert!(times[2] < 5_000, "it is milliseconds since the press: {times:?}");
    assert!(kept.started_unix_ms > 1_700_000_000_000, "the wall clock, Unix ms: {}", kept.started_unix_ms);
}

#[test]
fn a_quick_slash_across_a_word_takes_the_word_and_leaves_no_ink() {
    let mut s = app_with_slash_file();
    let ended = draw_fixture(&mut s, &fx::slash(), 0).expect("the stroke ended");
    assert_eq!(ended.up, Up::Slashed);
    assert_eq!(ended.page, 0);
    assert!(s.strokes.is_empty(), "the slash is a gesture, not a mark on the page");
    assert!(s.findings.is_empty(), "and no finding");
    assert_eq!(s.slashes.len(), 1);
    let d = &s.slashes[0];
    assert_eq!((d.page, d.started_unix_ms), (0, 1_764_000_000_000));
    assert_eq!(d.words.len(), 1);
    assert_eq!((d.words[0].line, d.words[0].start, d.words[0].end), (12, 33, 43));
    assert_eq!(d.words[0].text, "abcdefghij");
    assert_eq!(d.points.len(), fx::slash().samples.len(), "the samples stay with it, for the agent");
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.hidden_ranges(0)[11], vec![(33, 43)]);
}

#[test]
fn the_slash_of_a_mouse_takes_a_word_too() {
    let mut s = app_with_slash_file();
    let f = fx::slash();
    let point = |p: &fx::Sample| InkPoint { t_ms: p.t_ms, ..at(p.x, p.y) };
    s.stroke_begin_at(0, point(&f.samples[0]), false, 1);
    for p in &f.samples[1..] {
        s.stroke_move_at(point(p), true);
    }
    assert_eq!(s.stroke_end().map(|e| e.up), Some(Up::Slashed));
}

#[test]
fn a_slash_across_a_longer_stretch_takes_each_word_it_covers() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash_through_a_long_word(), 0);
    let words: Vec<&str> = s.slashes[0].words.iter().map(|w| w.text.as_str()).collect();
    // x 300 to 460 covers the word at 301.8 and the next one at 374.4 to 394.2, "=" and "1" and ";" after it
    assert_eq!(words, vec!["abcdefghij", "klm", "=", "1", ";"]);
}

#[test]
fn a_quick_slash_where_there_is_no_word_is_ink_like_any_other_stroke() {
    let mut s = app_with_slash_file();
    // line 30 holds "fn f29() {}": nothing at x 300 to 372
    let mut f = fx::slash();
    for p in &mut f.samples {
        p.y += 18.0 * 15.0;
    }
    let ended = draw_fixture(&mut s, &f, 0).expect("ended");
    assert_eq!(ended.up, Up::Kept);
    assert_eq!(s.strokes.len(), 1);
    assert!(s.slashes.is_empty());
}

#[test]
fn no_other_stroke_of_the_fixtures_takes_a_word_where_the_slash_would() {
    for f in fx::all() {
        if f.meaning == Meaning::Slash {
            continue;
        }
        let mut s = app_with_slash_file();
        let ended = draw_fixture(&mut s, &f, 0).expect("ended");
        assert!(s.slashes.is_empty(), "{} took a word", f.name);
        assert_ne!(ended.up, Up::Slashed, "{}", f.name);
    }
}

#[test]
fn a_slow_stroke_across_the_word_is_a_highlight_not_a_deletion() {
    let mut s = app_with_slash_file();
    let mut f = fx::slash();
    let end = f.samples.last().map_or(1, |l| l.t_ms);
    for p in &mut f.samples {
        p.t_ms = p.t_ms * 500 / end;
    }
    assert_eq!(draw_fixture(&mut s, &f, 0).map(|e| e.up), Some(Up::Kept));
    assert!(s.slashes.is_empty());
    assert_eq!(s.strokes.len(), 1);
}

#[test]
fn a_stroke_with_no_time_is_never_a_slash() {
    let mut s = app_with_slash_file();
    let f = fx::slash();
    // drawn through the clock, as the window draws: the samples come within a millisecond of each other
    s.stroke_begin(0, InkPoint { x: f.samples[0].x, y: f.samples[0].y, ..at(0.0, 0.0) }, true);
    for p in &f.samples[1..] {
        s.stroke_move(InkPoint { x: p.x, y: p.y, ..at(0.0, 0.0) }, true);
    }
    assert_eq!(s.stroke_end().map(|e| e.up), Some(Up::Kept), "no hand is that quick");
    assert!(s.slashes.is_empty());
}

#[test]
fn the_slash_of_an_audio_pen_leaves_the_clip_without_the_stroke_that_is_gone() {
    let mut s = app_with_slash_file();
    s.recording = Some(crate::model::VoiceClip::default());
    draw_fixture(&mut s, &fx::slash(), 0);
    assert!(s.recording.as_ref().is_some_and(|c| c.stroke_ids.is_empty()), "{:?}", s.recording);
    assert_eq!(s.slashes.len(), 1);
}

#[test]
fn a_slash_has_the_page_drawn_anew_the_live_stroke_is_gone_from_its_layer() {
    let mut s = app_with_slash_file();
    let before = s.ink.key(0, (1000, 730));
    let ended = draw_fixture(&mut s, &fx::slash(), 0).expect("ended");
    assert_eq!((ended.layer, ended.page), (Layer::of(s.pens.semantic()), 0));
    assert_ne!(s.ink.key(0, (1000, 730)), before, "a frame drawn before shows a stroke that is not kept");
}
