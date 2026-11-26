//! What the window tells a recorder: the layout's, the navigation's and the pens' events, in the recorder's vocabulary,
//! and nothing at all where there is no recorder.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use azul::prelude::*;

use super::*;
use crate::{code, layout::Panel, model::Semantic, AppState};

/// A hook that keeps what it is told.
#[derive(Default)]
struct Collect(Mutex<Vec<(String, String)>>);

impl RecorderHook for Collect {
    fn event(&self, kind: &str, fields: &str) {
        self.0.lock().unwrap().push((kind.to_string(), fields.to_string()));
    }
}

fn app_with(hook: Arc<Collect>) -> AppState {
    let file = |name: &str, n: usize| code::SourceFile {
        path: PathBuf::from(name),
        display: name.to_string(),
        lines: (0..n).map(|i| format!("fn f{i}() {{}}")).collect(),
        big: None,
    };
    let mut s = AppState::create(PathBuf::from("/work/p"), vec![file("a.rs", 3), file("b.rs", 120)], RefAny::new(0u8), PathBuf::new());
    s.recorder = hook;
    s
}

fn told(hook: &Collect) -> Vec<(String, serde_json::Value)> {
    hook.0
        .lock()
        .unwrap()
        .iter()
        .map(|(k, f)| (k.clone(), serde_json::from_str(f).expect("the fields are one JSON object")))
        .collect()
}

#[test]
fn with_no_recorder_the_window_tells_nobody_and_nothing_fails() {
    let mut s = AppState::create(PathBuf::new(), Vec::new(), RefAny::new(0u8), PathBuf::new());
    s.select_pen(2);
    s.set_panel_open(Panel::Tree, false);
    // the default hook is the one that does nothing
    NoRecorder.event("layout.toggle", "{}");
}

#[test]
fn opening_a_file_says_which_and_how_long_it_is() {
    let hook = Arc::new(Collect::default());
    let mut s = app_with(hook.clone());
    assert!(s.open_path("b.rs"));
    let events = told(&hook);
    let nav: Vec<_> = events.iter().filter(|(k, _)| k == "nav.file").collect();
    assert_eq!(nav.len(), 1, "{events:?}");
    assert_eq!(nav[0].1["file"], "b.rs");
    assert_eq!(nav[0].1["lines"], 120);
}

#[test]
fn taking_a_pen_says_its_meaning_and_its_nib_in_the_recorders_words() {
    let hook = Arc::new(Collect::default());
    let mut s = app_with(hook.clone());
    s.select_pen(2);
    s.double_click_pen(2);
    let events = told(&hook);
    let pens: Vec<_> = events.iter().filter(|(k, _)| k == "pen.select").collect();
    assert_eq!(pens.len(), 2, "{events:?}");
    assert_eq!((pens[0].1["meaning"].as_str(), pens[0].1["nib"].as_str()), (Some("question"), Some("pen")));
    assert_eq!(pens[1].1["nib"], "audio", "the audio pen is `audio` in the recorder's vocabulary");
    // a key on a pen that is not there says nothing
    let before = told(&hook).len();
    assert_eq!(s.press_pen_key(9), crate::pens::Pressed::Ignored);
    assert_eq!(told(&hook).len(), before);
    let _ = Semantic::Scope;
}

#[test]
fn a_panel_opening_or_closing_says_which_and_whether_it_is_open() {
    let hook = Arc::new(Collect::default());
    let mut s = app_with(hook.clone());
    s.set_panel_open(Panel::Tree, false);
    s.set_panel_open(Panel::Right, true);
    let events = told(&hook);
    let toggles: Vec<_> = events.iter().filter(|(k, _)| k == "layout.toggle").collect();
    assert_eq!(toggles.len(), 2, "{events:?}");
    assert_eq!((toggles[0].1["panel"].as_str(), toggles[0].1["open"].as_bool()), (Some("tree"), Some(false)));
    assert_eq!((toggles[1].1["panel"].as_str(), toggles[1].1["open"].as_bool()), (Some("whiteboard"), Some(true)));
}

#[test]
fn a_panel_that_stays_as_it_is_says_nothing() {
    let hook = Arc::new(Collect::default());
    let mut s = app_with(hook.clone());
    s.set_panel_open(Panel::Tree, true);
    assert!(told(&hook).iter().all(|(k, _)| k != "layout.toggle"), "the tree was open already");
}

#[test]
fn every_event_the_window_makes_is_a_kind_of_the_recorders_vocabulary() {
    let hook = Arc::new(Collect::default());
    let mut s = app_with(hook.clone());
    assert!(s.open_path("b.rs"));
    s.select_pen(3);
    s.set_panel_open(Panel::Tree, false);
    for (kind, _) in told(&hook) {
        assert!(crate::recorder_events::is_kind_name(&kind), "{kind} is not in the recorder's table");
    }
}
