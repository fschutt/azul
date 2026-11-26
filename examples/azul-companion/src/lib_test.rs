//! What the app does before there is a window.

use azul_review_link::view::Session;

use super::*;

#[test]
fn the_phones_own_app_starts_on_a_stand_in_and_leaves_the_connection_to_the_window() {
    let root = phone_root();
    assert!(root.connect_later, "the real connection is made when the window exists");
    assert!(!root.window_ready && !root.demo);
    assert_eq!(root.env, EnvSource::Window, "the window and the system say what the phone is like");
    assert_eq!(root.view().session, Session::Unpaired, "the stand-in knows no Mac");
}

#[test]
fn what_runs_from_the_libraries_constructor_binds_no_endpoint() {
    // `start` runs from the load-time constructor on Android, during System.loadLibrary: the engine has no Android context yet and the iroh
    // endpoint's bind aborts the process without one (the emulator, 2026-11-26: "android context was not initialized"). A host test cannot
    // run that, so the text of the function is what is checked: it makes no connection and binds nothing.
    let source = include_str!("lib.rs");
    let body = source.split("pub fn start()").nth(1).and_then(|rest| rest.split("\n}\n").next()).expect("the body of start()");
    let code: Vec<&str> = body.lines().filter(|line| !line.trim_start().starts_with("//")).collect();
    assert!(code.iter().any(|line| line.contains("phone_root()")), "start() builds its state with phone_root()");
    for word in ["net::", "IrohLink", "connect", "bind"] {
        assert!(!code.iter().any(|line| line.contains(word)), "start() must not use {word:?}: it runs before the activity has an Android context");
    }
}

#[test]
fn the_phones_window_fills_the_surface_because_the_screens_pad_themselves_by_the_insets() {
    // the engine insets the root by the safe area unless the window says otherwise, and the screens pad by the same insets: both together
    // put the page one top inset too low and one pair of insets taller than the viewport (the emulator, 2026-11-26)
    let window = glue::phone_window_options("test");
    assert!(window.window_state.flags.extend_into_safe_area);
}
