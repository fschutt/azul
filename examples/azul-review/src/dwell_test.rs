//! The dwell, as a pure state machine on an injected clock (milliseconds the caller passes in):
//! no window, no timer, no azul.

use super::*;

fn config() -> Config {
    Config {
        allow: Allow::Pen,
        rest_ms: 1_200,
        jitter_px: 8.0,
        away_px: 48.0,
    }
}

fn hover(page: usize, x: f32, y: f32) -> Sample {
    Sample { page, x, y, pen: true, contact: false }
}

fn touch(page: usize, x: f32, y: f32) -> Sample {
    Sample { page, x, y, pen: true, contact: true }
}

fn mouse(page: usize, x: f32, y: f32) -> Sample {
    Sample { page, x, y, pen: false, contact: false }
}

fn anchor(page: usize, x: f32, y: f32, contact: bool) -> Anchor {
    Anchor { page, x, y, contact }
}

#[test]
fn a_pen_hovering_still_fires_one_dwell_once_the_rest_time_is_up() {
    let mut d = Dwell::new(config());
    assert_eq!(d.observe(hover(0, 100.0, 50.0), 0), Reaction::Stay);
    assert_eq!(d.tick(1_199), None, "one millisecond short");
    assert_eq!(d.tick(1_200), Some(anchor(0, 100.0, 50.0, false)));
    assert_eq!(d.tick(9_000), None, "one dwell per rest, not one per tick");
}

#[test]
fn the_rest_is_timed_from_the_last_move_not_from_the_first_sample() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 100.0, 50.0), 0);
    d.observe(hover(0, 130.0, 50.0), 800);
    assert_eq!(d.tick(1_300), None, "the rest began at 800");
    assert_eq!(d.tick(1_999), None);
    assert_eq!(d.tick(2_000), Some(anchor(0, 130.0, 50.0, false)));
}

#[test]
fn jitter_inside_the_radius_keeps_the_rest_and_its_anchor() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 100.0, 50.0), 0);
    d.observe(hover(0, 103.0, 52.0), 500);
    d.observe(hover(0, 101.0, 49.0), 900);
    assert_eq!(
        d.tick(1_200),
        Some(anchor(0, 100.0, 50.0, false)),
        "the anchor is where the rest began"
    );
}

#[test]
fn a_slow_drift_past_the_radius_starts_a_new_rest() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 100.0, 50.0), 0);
    d.observe(hover(0, 104.0, 50.0), 400);
    d.observe(hover(0, 109.0, 50.0), 800);
    assert_eq!(d.tick(1_300), None, "9 px from where the rest began: a new rest at 800");
    assert_eq!(d.tick(2_000), Some(anchor(0, 109.0, 50.0, false)));
}

#[test]
fn a_mouse_rests_without_a_dwell_unless_mice_are_allowed() {
    let mut pen_only = Dwell::new(config());
    pen_only.observe(mouse(0, 10.0, 10.0), 0);
    assert_eq!(pen_only.tick(5_000), None, "the pen starts a dictation, a resting mouse does not");

    let mut any = Dwell::new(Config { allow: Allow::Any, ..config() });
    any.observe(mouse(0, 10.0, 10.0), 0);
    assert_eq!(any.tick(1_200), Some(anchor(0, 10.0, 10.0, false)));
}

#[test]
fn off_never_fires() {
    let mut d = Dwell::new(Config { allow: Allow::Off, ..config() });
    d.observe(hover(0, 10.0, 10.0), 0);
    assert_eq!(d.tick(60_000), None);
    assert!(!d.is_waiting());
}

#[test]
fn a_pen_touching_without_moving_after_a_stroke_fires_a_contact_dwell() {
    let mut d = Dwell::new(config());
    d.observe(touch(2, 10.0, 10.0), 0);
    d.observe(touch(2, 20.0, 12.0), 200);
    d.observe(touch(2, 40.0, 15.0), 400);
    assert_eq!(d.tick(1_599), None, "the stroke's last move was at 400");
    assert_eq!(d.tick(1_600), Some(anchor(2, 40.0, 15.0, true)));
}

#[test]
fn lifting_the_pen_without_moving_does_not_fire_a_second_dwell() {
    let mut d = Dwell::new(config());
    d.observe(touch(0, 40.0, 15.0), 0);
    assert!(d.tick(1_200).is_some());
    d.observe(hover(0, 40.0, 15.0), 2_000);
    assert_eq!(d.tick(9_000), None, "no movement since: the same rest");
}

#[test]
fn a_second_rest_after_a_nudge_inside_the_anchors_reach_fires_again() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 100.0, 50.0), 0);
    let first = d.tick(1_200).expect("the first dwell");
    d.session_started(first);
    assert_eq!(d.observe(hover(0, 120.0, 50.0), 3_000), Reaction::Stay, "20 px: still near");
    assert_eq!(
        d.tick(4_200),
        Some(anchor(0, 120.0, 50.0, false)),
        "resting again is a second dwell: the app stops the session on it"
    );
}

#[test]
fn moving_away_from_the_anchor_while_a_session_runs_is_reported_once() {
    let mut d = Dwell::new(config());
    d.session_started(anchor(0, 100.0, 50.0, false));
    assert_eq!(d.observe(hover(0, 140.0, 50.0), 2_000), Reaction::Stay, "40 px: inside 48");
    assert_eq!(d.observe(hover(0, 160.0, 50.0), 2_100), Reaction::MovedAway, "60 px: away");
    assert_eq!(
        d.observe(hover(0, 200.0, 50.0), 2_200),
        Reaction::Stay,
        "the session is over: nothing to end again"
    );
}

#[test]
fn drawing_is_judged_by_the_small_radius() {
    let mut d = Dwell::new(config());
    d.session_started(anchor(0, 100.0, 50.0, false));
    assert_eq!(
        d.observe(touch(0, 105.0, 50.0), 1_000),
        Reaction::Stay,
        "the tip set down at the anchor to rest on it: not a stroke yet"
    );
    assert_eq!(
        d.observe(touch(0, 112.0, 50.0), 1_050),
        Reaction::MovedAway,
        "12 px with the tip down is a stroke: the session ends"
    );
}

#[test]
fn another_page_is_away() {
    let mut d = Dwell::new(config());
    d.session_started(anchor(0, 100.0, 50.0, false));
    assert_eq!(d.observe(hover(1, 100.0, 50.0), 500), Reaction::MovedAway);
}

#[test]
fn the_pointer_leaving_clears_the_rest_and_ends_a_session() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 100.0, 50.0), 0);
    assert_eq!(d.left(), Reaction::Stay, "no session: nothing to end");
    assert_eq!(d.tick(5_000), None, "a pen that left has no rest to time");

    d.session_started(anchor(0, 100.0, 50.0, false));
    assert_eq!(d.left(), Reaction::MovedAway, "the pen went away from the anchor");
    assert_eq!(d.left(), Reaction::Stay, "reported once");
}

#[test]
fn a_mouse_during_a_pen_session_counts_as_moving_away() {
    let mut d = Dwell::new(config());
    d.session_started(anchor(0, 100.0, 50.0, false));
    assert_eq!(d.observe(mouse(0, 100.0, 50.0), 500), Reaction::MovedAway);
}

#[test]
fn a_clock_that_goes_backwards_never_fires() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 10.0, 10.0), 1_000);
    assert_eq!(d.tick(500), None);
    assert_eq!(d.tick(1_000 + 1_199), None);
}

#[test]
fn a_timer_is_wanted_only_while_a_rest_waits() {
    let mut d = Dwell::new(config());
    assert!(!d.is_waiting(), "nothing observed");
    d.observe(hover(0, 10.0, 10.0), 0);
    assert!(d.is_waiting());
    assert!(d.tick(1_200).is_some());
    assert!(!d.is_waiting(), "the dwell fired: nothing left to wait for until the pen moves");
    d.observe(hover(0, 60.0, 10.0), 1_300);
    assert!(d.is_waiting());
    d.left();
    assert!(!d.is_waiting());
}

#[test]
fn a_new_rest_time_takes_effect_at_once() {
    let mut d = Dwell::new(config());
    d.observe(hover(0, 10.0, 10.0), 0);
    d.set_config(Config { rest_ms: 300, ..config() });
    assert_eq!(d.tick(300), Some(anchor(0, 10.0, 10.0, false)));
}

#[test]
fn a_rest_time_outside_what_a_hand_can_mean_is_capped_with_a_word_about_it() {
    assert_eq!(cap_rest_ms(1_200), (1_200, None));
    let (low, why) = cap_rest_ms(5);
    assert_eq!(low, MIN_REST_MS);
    assert!(why.is_some_and(|w| w.contains("rest time")), "a diagnostic, not a silent change");
    let (high, why) = cap_rest_ms(10 * 60 * 1_000);
    assert_eq!(high, MAX_REST_MS);
    assert!(why.is_some());
}

#[test]
fn the_allow_names_round_trip() {
    for allow in [Allow::Off, Allow::Pen, Allow::Any] {
        assert_eq!(Allow::parse(allow.name()), Some(allow));
    }
    assert_eq!(Allow::parse(" PEN "), Some(Allow::Pen));
    assert_eq!(Allow::parse("stylus"), None);
}
