//! The scenes: which there are, how a name picks them, and that each one is a state the app can really be in.

use std::sync::{Arc, Mutex};

use azul_review_link::view::Session;

use super::super::mac::SimMac;
use super::super::phone::SimPhone;
use super::*;
use crate::ui::EnvSource;

/// Plays a scene as the screenshot run does: the reset, its moment, the scene, its moment to settle.
fn play(root: &mut Root, extras: &Extras, name: &str, from: u64, settle: u64) -> bool {
    reset(root, extras);
    for ms in (from..from + RESET_MS + 33).step_by(33) {
        root.tick(ms);
    }
    let start = from + RESET_MS + 33;
    let found = setup(name, root, extras, start);
    for ms in (start..=start + settle).step_by(33) {
        root.tick(ms);
    }
    found
}

fn app() -> (Root, Extras) {
    let mac = Arc::new(Mutex::new(SimMac::new()));
    let commands: super::super::phone::Commands = Arc::new(Mutex::new(Vec::new()));
    let extras = Extras::new(mac.clone(), commands.clone());
    let root = Root::new(Box::new(SimPhone::new(mac, commands)), EnvSource::Fixed(extras.env()));
    (root, extras)
}

#[test]
fn every_screen_of_the_study_has_scenes_and_the_names_are_unique() {
    let names: Vec<&str> = ALL.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "a name twice");
    for letter in ['a', 'b', 'c', 'd', 'e'] {
        assert!(names.iter().any(|n| n.starts_with(letter)), "no scene for screen {letter}");
    }
    assert!(names.iter().all(|n| n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')));
}

#[test]
fn a_spec_picks_all_scenes_a_screen_or_names() {
    assert_eq!(pick("all").len(), ALL.len());
    assert_eq!(pick("").len(), ALL.len());
    assert!(pick("a").iter().all(|n| n.starts_with('a')) && pick("a").len() == 10, "the pairing's nine screens and the first one as the phone's own app draws it");
    assert_eq!(pick("b1-remote"), ["b1-remote"]);
    assert_eq!(pick("a1-intro, e1-settings"), ["a1-intro", "e1-settings"]);
    assert!(pick("zz").is_empty());
}

#[test]
fn a_name_no_scene_has_is_refused() {
    let (mut root, extras) = app();
    assert!(!play(&mut root, &extras, "z9-nothing", 0, 0));
}

#[test]
fn every_scene_can_be_set_up_and_settles_without_a_panic() {
    for (name, _) in ALL {
        let (mut root, extras) = app();
        assert!(play(&mut root, &extras, name, 100_000, SETTLE_MS), "{name}");
    }
}

#[test]
fn the_pairing_scenes_show_the_session_they_name() {
    let expect = [
        ("a1-intro", "Unpaired"),
        ("a2-searching", "Searching"),
        ("a3-found", "Pairing"),
        ("a5-notfound", "Failed"),
        ("a7-address", "Failed"),
    ];
    for (name, kind) in expect {
        let (mut root, extras) = app();
        play(&mut root, &extras, name, 0, 400);
        let shown = format!("{:?}", root.view().session);
        assert!(shown.starts_with(kind), "{name}: {shown}");
    }
}

#[test]
fn the_remote_scenes_have_a_pen_state_a_picture_and_the_four_buttons_after_a_moment() {
    let (mut root, extras) = app();
    play(&mut root, &extras, "b2-remote-buttons", 100_000, 1_700);
    let view = root.view();
    assert!(view.pen.is_some() && view.thumb.is_some());
    assert!(matches!(view.session, Session::Connected { .. }));
    use azul_review_link::frame::ReplyStatus::{Received, Suggested, Working};
    assert_eq!(view.button_status[0], Some(Working), "{:?}", view.button_status);
    assert_eq!(view.button_status[1], Some(Suggested));
    assert!(view.button_status[2].is_some(), "Shorter is done");
    assert_eq!(view.button_status[3], Some(Received));
}

#[test]
fn the_files_scenes_list_what_they_name() {
    let (mut root, extras) = app();
    play(&mut root, &extras, "c3-files-long", 0, 400);
    let files = &root.view().files;
    assert_eq!(files.dir, "tests/fixtures");
    assert_eq!(files.entries.len(), 500);
    assert!(files.more);
    let (mut root, extras) = app();
    play(&mut root, &extras, "c4-files-loading", 0, 700);
    assert!(root.view().files.loading(), "the answer is twenty seconds away");
}

#[test]
fn the_editor_scenes_open_the_editor() {
    let (mut root, extras) = app();
    play(&mut root, &extras, "d2-editor-cut", 0, 0);
    assert_eq!(root.ui.screen, Screen::Editor);
    assert!(root.ui.draft_cut[0].1.is_some() && root.ui.editor_dirty);
    let _ = root.view().buttons.clone();
}
