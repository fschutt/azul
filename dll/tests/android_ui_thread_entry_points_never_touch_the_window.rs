//! The JNI entry points of the Android shell run on the Java UI thread and never see the `AndroidWindow`.
//!
//! NativeActivity runs `android_main` on a thread of its own; every callback of the Java glue (the
//! `InputConnection`, the insets listener, the gesture detectors, the selection toolbar, TalkBack's
//! provider) arrives on the UI thread. The shell handed those callbacks the address of the window on
//! `android_main`'s stack and they formed `&mut AndroidWindow` from it while the loop thread held its own:
//! two live `&mut` to one window (undefined behaviour), the IME replacing the preedit `String` while the
//! layout read it, `getTextBeforeCursor` walking a layout window a regeneration was freeing, and an address
//! that dangled the moment `android_main` returned (the process then died in
//! `nativeOnUiModeChanged` on the first configuration change: QA run 45771255766).
//!
//! The rule that replaces it is a TYPE rule (`android_core::ui_bridge`): the entry points live in
//! `android/bridge.rs` (and the accessibility and web view bridges), the Java side holds the address of a
//! leaked `UiBridge` made of locks and atomics, and the loop applies what was queued. This test keeps the
//! window out of those files by reading them:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_ui_thread_entry_points_never_touch_the_window.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of a Rust file: comments and doc comments dropped (they discuss the window).
fn code_of(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_bridge_that_java_calls_never_names_the_window() {
    let code = code_of("dll/src/desktop/shell2/android/bridge.rs");
    for forbidden in ["AndroidWindow", "layout_window", "LayoutWindow", "common.", "text_edit_manager"] {
        assert!(
            !code.contains(forbidden),
            "android/bridge.rs holds the JNI entry points the UI thread calls: it must not touch the window ({forbidden})"
        );
    }
    // and its only state is the leaked bridge
    assert!(code.contains("Arc::into_raw"), "the cookie is a leaked bridge, never a window");
    assert!(!code.contains("&mut *"), "no `&mut` is formed from a cookie");
}

#[test]
fn the_accessibility_entry_points_reach_only_the_adapter_of_the_bridge() {
    let code = code_of("dll/src/desktop/shell2/android/accessibility.rs");
    assert!(
        !code.contains("AndroidWindow"),
        "TalkBack's calls arrive on the UI thread: they reach the adapter through the bridge, not the window"
    );
    assert!(code.contains("from_cookie"));
}

/// Every `extern "system"` function of the shell is in a file that is audited here: the window's own
/// module has none.
#[test]
fn the_window_module_has_no_jni_entry_point_and_no_window_address_is_published() {
    let code = code_of("dll/src/desktop/shell2/android/mod.rs");
    assert!(
        !code.contains("extern \"system\""),
        "a JNI entry point in the window's module would be tempted to use the window"
    );
    for gone in ["ANDROID_WINDOW_PTR", "with_window", "&mut window as *mut"] {
        assert!(!code.contains(gone), "{gone} hands the window's address to another thread");
    }
}

/// The IME asks which kind of field has the focus (`onCreateInputConnection` -> `nativeGetInputHints`) within
/// milliseconds of `showSoftInput`, and the answer comes from the mirror the loop publishes at the end of its
/// iteration. A keyboard raised from inside the event handling (where the focus changed) reached the UI thread
/// before that mirror existed: the IME read the PREVIOUS field's kind (a plain keyboard for an e-mail field, no
/// phone pad). The engine's request is therefore only recorded, and the loop raises the keyboard after the
/// publication.
///
/// Mutation: `set_soft_keyboard_visible` calling the JNI helper itself (the state of 0f3e1eb093).
#[test]
fn the_soft_keyboard_is_raised_after_the_mirror_it_reads_is_published() {
    let code = code_of("dll/src/desktop/shell2/android/mod.rs");

    // the engine's entry point only records the request
    let at = code
        .find("pub fn set_soft_keyboard_visible(visible: bool) {")
        .expect("the engine calls set_soft_keyboard_visible");
    let body_end = at + code[at..].find("\n}\n").expect("end of function");
    let entry = &code[at..body_end];
    assert!(entry.contains("request_keyboard("), "the request is recorded in the bridge");
    assert!(!entry.contains("attach_current_thread"), "no JNI call from inside the event handling");

    // the loop takes the request, publishes the mirror, then raises the keyboard
    let take = code.find("take_keyboard_request()").expect("the loop takes the request");
    let publish = code[take..].find("queues.publish(").map(|i| take + i).expect("the loop publishes the mirror after taking it");
    let raise = code[publish..].find("raise_soft_keyboard(visible)").map(|i| publish + i).expect("the loop raises the keyboard after publishing");
    assert!(take < publish && publish < raise);
}
