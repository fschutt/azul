//! Losing focus must release the mouse buttons, on every backend.
//!
//! When focus leaves while a button is held, the OS delivers the button-UP to
//! whoever took focus. The application never sees it, so `left_down` stays
//! latched forever. From then on every mouse-move reads as a DRAG: text
//! selects instead of buttons clicking, and nothing recovers, because the
//! release that would clear it went to another process.
//!
//! Reported against AzMeet, which provokes it constantly: the camera and
//! screen-recording permission sheets take focus in the middle of the very
//! click that requested them. The symptom is "after backgrounding, the buttons
//! become unclickable and clicking selects the button's text instead".
//!
//! The Windows backend already made this exact argument for the KEYBOARD —
//! "least of all the KEY-UP of the modifier that caused the focus change (Alt
//! of Alt+Tab), which is exactly the key that would stay latched" — and drops
//! held keys on blur. Nobody applied the same reasoning to the mouse, on any
//! platform.
//!
//! This is a source-level check rather than a runtime one because each backend
//! owns its own blur handler and they cannot be driven from a single test
//! process: macOS needs a real NSWindow, Wayland a compositor, Windows an
//! HWND. What can be asserted everywhere is that each blur handler clears the
//! buttons it is responsible for.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("dll/ has a parent")
        .to_path_buf()
}

/// (backend file, marker identifying its blur handler)
const BLUR_HANDLERS: &[(&str, &str)] = &[
    ("dll/src/desktop/shell2/macos/mod.rs", "window_did_resign_key"),
    ("dll/src/desktop/shell2/windows/mod.rs", "Drop every held key"),
    ("dll/src/desktop/shell2/linux/x11/mod.rs", "window_focused = false"),
    ("dll/src/desktop/shell2/linux/wayland/mod.rs", "window_focused = false"),
];

/// Text of the ~80 lines following the blur marker — the handler's body.
fn handler_body(root: &Path, file: &str, marker: &str) -> String {
    let src = std::fs::read_to_string(root.join(file))
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    let at = src
        .find(marker)
        .unwrap_or_else(|| panic!("{file}: no blur handler marker {marker:?} — did it move?"));
    src[at..].lines().take(80).collect::<Vec<_>>().join("\n")
}

#[test]
fn every_blur_handler_releases_the_mouse_buttons() {
    let root = repo_root();
    let mut missing = Vec::new();

    for (file, marker) in BLUR_HANDLERS {
        let body = handler_body(&root, file, marker);
        if !body.contains("left_down = false") {
            missing.push(*file);
        }
    }

    assert!(
        missing.is_empty(),
        "these blur handlers do not release the mouse buttons: {missing:?}\n\n\
         A button held when focus leaves has its button-UP delivered to whoever \
         took focus. `left_down` stays latched, every later move reads as a drag, \
         and the window is left selecting text instead of clicking buttons with \
         no way back. Clear left/right/middle_down in the blur handler and let \
         the state diff emit the MouseUp."
    );
}

/// The keyboard equivalent must stay too — it is the precedent this rests on.
#[test]
fn the_windows_blur_handler_still_drops_held_keys() {
    let body = handler_body(
        &repo_root(),
        "dll/src/desktop/shell2/windows/mod.rs",
        "Drop every held key",
    );
    assert!(
        body.contains("pressed_virtual_keycodes"),
        "the held-key drop on blur disappeared; the modifier that caused the \
         focus change (Alt of Alt+Tab) would stay latched"
    );
}
