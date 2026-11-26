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
