//! The debug op that says what AzReview's state is: for the scripts that drive a window and cannot read an icon.

use std::path::PathBuf;

use azul::prelude::*;

use super::state_json;
use crate::{model::Semantic, AppState};

fn app() -> AppState {
    AppState::create(PathBuf::from("/work/p"), Vec::new(), RefAny::new(0u8), PathBuf::new())
}

fn parsed(s: &AppState) -> serde_json::Value {
    serde_json::from_str(&state_json(s)).expect("the op answers JSON")
}

#[test]
fn a_new_state_names_the_scope_pen_with_the_marker_and_the_panels_as_they_start() {
    let v = parsed(&app());
    assert_eq!(v["pen"], 0);
    assert_eq!(v["meaning"], "scope");
    assert_eq!(v["nib"], "marker");
    assert_eq!(v["tree_open"], true);
    assert_eq!(v["right_open"], false);
    assert_eq!(v["chips"], 0);
}

#[test]
fn the_pen_and_the_nib_in_hand_are_told_by_name() {
    let mut s = app();
    s.pick(Semantic::Question);
    let v = parsed(&s);
    assert_eq!((v["pen"].as_u64(), v["meaning"].as_str(), v["nib"].as_str()), (Some(2), Some("question"), Some("pen")));
    s.pens.cycle(false);
    assert_eq!(parsed(&s)["nib"], "audio pen");
}

#[test]
fn the_widths_of_the_panels_are_told() {
    let mut s = app();
    s.panels.tree_w = 311.0;
    s.panels.right_open = true;
    s.panels.right_w = 481.0;
    let v = parsed(&s);
    assert_eq!(v["tree_w"], 311.0);
    assert_eq!(v["right_open"], true);
    assert_eq!(v["right_w"], 481.0);
}

#[test]
fn the_microphone_and_the_dictation_are_told_for_the_scripts_that_wait_for_them() {
    use crate::{dictation::Mic, voice::Running};
    let mut s = app();
    let v = parsed(&s);
    assert_eq!((v["mic"].as_str(), v["dictating"].as_bool(), v["ready"].as_bool()), (Some("cold"), Some(false), Some(false)));
    assert_eq!(v["partial"], "");

    let mut running = Running::for_test(1, 1);
    running.partial = "this lo".to_string();
    s.voice.running = Some(running);
    let v = parsed(&s);
    assert_eq!((v["dictating"].as_bool(), v["ready"].as_bool()), (Some(true), Some(false)), "opening");
    assert_eq!(v["partial"], "this lo");

    if let Some(r) = s.voice.running.as_mut() {
        r.ready = true;
    }
    s.voice.mic = Mic::Listening;
    let v = parsed(&s);
    assert_eq!((v["ready"].as_bool(), v["mic"].as_str()), (Some(true), Some("listening")));
}

#[test]
fn the_standby_is_told_with_what_the_policy_asked_so_far() {
    let mut s = app();
    let v = parsed(&s);
    assert_eq!(v["standby"]["mode"], "near", "the pen near is the default");
    assert_eq!(v["standby"]["enabled"], false, "nothing runs where the session cannot open the microphone ahead");
    assert_eq!((v["standby"]["armed"].as_bool(), v["standby"]["arms"].as_u64(), v["standby"]["disarms"].as_u64()), (Some(false), Some(0), Some(0)));

    s.voice.can_arm = true;
    s.voice.dwell.set_config(crate::dwell::Config { allow: crate::dwell::Allow::Any, ..crate::dwell::Config::default() });
    s.voice.pen_near(false);
    let v = parsed(&s);
    assert_eq!((v["standby"]["enabled"].as_bool(), v["standby"]["armed"].as_bool(), v["standby"]["arms"].as_u64()), (Some(true), Some(true), Some(1)));
    s.voice.window_deactivated();
    let v = parsed(&s);
    assert_eq!((v["standby"]["armed"].as_bool(), v["standby"]["disarms"].as_u64()), (Some(false), Some(1)));
}
