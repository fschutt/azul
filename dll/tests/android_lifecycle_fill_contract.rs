//! What the Android shell tells an app of the activity's coming and going (the app's lifecycle handler, ledger AN-18), pinned in
//! source: the pairs are `android_core::lifecycle::AppPhase`'s (tested on every host), this file pins that the loop's arms use it
//! and that the app hears `Paused` before the system may end the process.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_lifecycle_fill_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}`"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

/// The app's handler hears `Paused` when the activity pauses (and when the surface goes without a `Pause` first), `Resumed` only
/// after a pause, `LowMemory` at `onLowMemory` and at every trim from the running-low level, `WillTerminate` once before the window
/// closes (a window that never heard `Paused` hears it first). Each goes through `AppPhase` and `tell_app`.
///
/// Mutation: `MainEvent::Pause` left a no-op (the state of 6ce827b284: AzWriter wrote its document at no event at all).
#[test]
fn the_loop_tells_the_app_through_the_phase_tracker() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = |name: &str| {
        let at = shell.find(name).unwrap_or_else(|| panic!("no `{name}` arm"));
        body(&shell[at..], "=>")
    };
    assert!(arm("MainEvent::Pause=>").contains("window.app_phase.on_pause()") && arm("MainEvent::Pause=>").contains("tell_app(window,tell);"));
    assert!(arm("MainEvent::Resume{..}=>").contains("window.app_phase.on_resume()"));
    assert!(arm("MainEvent::LowMemory=>").contains("window.app_phase.on_low_memory()"));
    let destroy = arm("MainEvent::Destroy=>");
    let last_word = destroy.find("fortellinwindow.app_phase.on_destroy(){tell_app(window,tell);}").expect("the last word");
    let close = destroy.find("window.close();").expect("the window closes");
    assert!(last_word < close, "the app hears the last word BEFORE the window is closed");
    assert!(arm("MainEvent::TerminateWindow{..}=>").contains("window.app_phase.on_surface_lost()"));
    let trim = shell.find("UiEvent::TrimMemory(level)=>").expect("the trim arm");
    assert!(body(&shell[trim..], "=>").contains("window.app_phase.on_trim(level)"));
}

/// `tell_app` maps each thing told to the engine's event and raises it against THIS window on the loop thread; what the handler asked
/// for is applied like any other callback's changes.
#[test]
fn tell_app_raises_the_engines_events() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let tell = body(&shell, "fntell_app(window:&mutAndroidWindow,tell:Option<crate::desktop::shell2::android_core::lifecycle::Tell>){");
    for pair in [
        "Tell::Paused=>LifecycleEvent::Paused",
        "Tell::Resumed=>LifecycleEvent::Resumed",
        "Tell::LowMemory=>LifecycleEvent::LowMemory",
        "Tell::WillTerminate=>LifecycleEvent::WillTerminate",
    ] {
        assert!(tell.contains(pair), "{pair}");
    }
    let raise = tell.find("crate::desktop::lifecycle::raise(window,event)").expect("raised against this window");
    assert!(tell[raise..].contains("follow_ui_result(window,result);"));
}
