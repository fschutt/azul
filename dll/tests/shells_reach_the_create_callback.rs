//! Every shell fires `WindowCreateOptions::create_callback` once, when its window exists and before its first layout.
//!
//! The desktop shells and headless did; the phone shells never did: `AndroidWindow::new` and `IOSWindow::new` took the
//! options' window state and colours and DROPPED the callback, so every app that starts a timer or a worker there was
//! dead on a phone (AzCompanion's 30 Hz timer, AzMaps' typeahead and pins file, AzWriter's pagination timer, AzMeet's
//! `pump_link` and its keyring request: 'azmeet-loading-key' for ever on the lobby; the Language setting never reached
//! `CallbackInfo::set_locale`).
//!
//! The one shared function is `shell2::common::create_callback::invoke_create_callback`. The desktop shells fire it with
//! their own, older code; this guard reads every shell's source and fails when one does not reach the callback.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/shells_reach_the_create_callback.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of a Rust file: comments and doc comments dropped (they discuss the callback).
fn code_of(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_shared_function_makes_the_call_and_applies_what_the_callback_asked() {
    let code = code_of("dll/src/desktop/shell2/common/create_callback.rs");
    for needed in [
        "pub fn invoke_create_callback",
        "prepare_callback_invocation()",
        "invoke_single_callback(",
        "apply_user_change(",
        "request_regeneration(",
    ] {
        assert!(code.contains(needed), "common/create_callback.rs must contain `{needed}`");
    }
}

/// macOS, X11, Wayland and Windows take the callback out of the options before they consume them and call it when the
/// window is ready; headless keeps it until `run()` and has a method for it.
#[test]
fn every_desktop_shell_and_headless_fires_the_create_callback() {
    for shell in [
        "dll/src/desktop/shell2/macos/mod.rs",
        "dll/src/desktop/shell2/linux/x11/mod.rs",
        "dll/src/desktop/shell2/linux/wayland/mod.rs",
        "dll/src/desktop/shell2/windows/mod.rs",
    ] {
        let code = code_of(shell);
        assert!(
            code.contains("create_callback.into_option()") || code.contains("invoke_create_callback("),
            "{shell} must fire WindowCreateOptions::create_callback"
        );
    }
    let headless = code_of("dll/src/desktop/shell2/headless/mod.rs");
    assert!(headless.contains("fn invoke_create_callback") && headless.contains("self.invoke_create_callback()"));
}

/// The Android window keeps the callback the options carry (it dropped it) and fires it ONCE, at the first `InitWindow`:
/// the surface exists, the window state has its real size and density, the first layout is queued and has not run. The
/// regeneration the surface asks for is requested BEFORE the callback (the first layout stays tagged as the first), the
/// pass that applies what the callback asked comes AFTER it.
///
/// Mutation: `AndroidWindow::new` without `options.create_callback` (the state of 0f3e1eb093).
#[test]
fn the_android_shell_fires_it_once_when_the_surface_exists_and_before_the_first_layout() {
    // without whitespace: rustfmt breaks the calls over lines
    let code: String = code_of("dll/src/desktop/shell2/android/mod.rs").chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        code.contains("options.create_callback"),
        "AndroidWindow::new drops the options' create_callback: no app timer, locale or worker ever started"
    );
    assert!(
        code.contains("mem::replace(&mutwindow.create_callback"),
        "the callback is taken out of the window when it fires: once per window lifetime"
    );
    let init = code.find("MainEvent::InitWindow").expect("the InitWindow arm");
    let arm = &code[init..init + code[init..].find("MainEvent::TerminateWindow").expect("the next arm")];
    let regeneration = arm.find("request_regeneration(reason)").expect("InitWindow requests the first layout");
    let fire = arm.find("invoke_create_callback(").expect("InitWindow fires the create callback");
    let pass = arm.find("process_window_events(0)").expect("InitWindow runs a pass");
    assert!(regeneration < fire, "the first layout is requested before the callback (it stays the first layout)");
    assert!(fire < pass, "the pass that applies the callback's changes comes after it");
}

/// The iOS window keeps the callback the options carry (it dropped it) and fires it ONCE, from `application:
/// didFinishLaunching:` as soon as the window is the app's window: its real size and scale are in the window state
/// (read from the screen in `IOSWindow::new`), the first layout is still owed (it runs from the display link) and has
/// not run. The pass that applies what the callback asked comes AFTER it.
///
/// Mutation: `IOSWindow::new` without `options.create_callback` (the state of 0f3e1eb093).
#[test]
fn the_ios_shell_fires_it_once_when_the_window_exists_and_before_the_first_layout() {
    // without whitespace: rustfmt breaks the calls over lines
    let code: String = code_of("dll/src/desktop/shell2/ios/mod.rs").chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        code.contains("options.create_callback"),
        "IOSWindow::new drops the options' create_callback: no app timer or worker ever starts on an iPhone"
    );
    assert!(
        code.contains("mem::replace(&mutself.create_callback"),
        "the callback is taken out of the window when it fires: once per window lifetime"
    );
    let fire_fn = code.find("fnfire_create_callback(").expect("IOSWindow::fire_create_callback");
    let body = &code[fire_fn..fire_fn + 900.min(code.len() - fire_fn)];
    let fire = body.find("invoke_create_callback(").expect("fire_create_callback calls the shared function");
    let pass = body.find("process_window_events(0)").expect("fire_create_callback runs a pass");
    assert!(fire < pass, "the pass that applies the callback's changes comes after it");
    // the delegate fires it once the window is the app's window (the callbacks of UIKit find it), and the first layout
    // (display link / displayLayer:) has not run yet because the display link only ticks after didFinishLaunching returns
    let launching = code.find("fndid_finish_launching(").expect("the launching delegate method");
    let launching_end = code[launching..].find("fnapp_did_become_active(").expect("the next delegate method");
    let launching_body = &code[launching..launching + launching_end];
    let assigned = launching_body.find("AZUL_IOS_WINDOW=Box::into_raw").expect("the window is made the app's window");
    let called = launching_body.find(".fire_create_callback()").expect("didFinishLaunching fires the create callback");
    assert!(assigned < called, "the callback fires after the window is the app's window");
}
