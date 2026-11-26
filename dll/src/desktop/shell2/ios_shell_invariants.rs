//! Source-text invariants of the iOS shell, pinned the way `window_teardown_invariants` pins the
//! desktop backends: a test binary on one host cannot run `shell2/ios` (it needs UIKit), and the
//! shapes below are the ones a phone app dies of or silently loses its input to.
//!
//! Each pin names what went wrong when it did not hold.

const IOS_MOD_RS: &str = include_str!("ios/mod.rs");
const IOS_A11Y_RS: &str = include_str!("ios/accessibility.rs");
const IOS_TEXT_INPUT_RS: &str = include_str!("ios/text_input.rs");
const IOS_CLIPBOARD_RS: &str = include_str!("ios/clipboard.rs");
const IOS_WEBVIEW_RS: &str = include_str!("ios/webview.rs");
const RUN_RS: &str = include_str!("run.rs");

/// The body of the first fn declared by `decl`, from its opening brace to the matching closing one.
fn body<'a>(source: &'a str, decl: &str) -> &'a str {
    let start = source
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found - was it renamed?"));
    let open = start + source[start..].find('{').expect("a fn body");
    let mut depth = 0_usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + i];
                }
            }
            _ => {}
        }
    }
    &source[open..]
}

fn at(haystack: &str, needle: &str, what: &str) {
    assert!(
        haystack.contains(needle),
        "{what}: `{needle}` is missing"
    );
}

/// The code of `source` without its comments (line comments, doc comments): what a ban on a call
/// has to look at, or a comment that explains the ban would trip it.
fn code_of(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A panic in a phone app is an abort: no window, a crash report and nothing on screen. The
/// launch of the iOS app died of `Protocol::get("UIApplicationDelegate").unwrap()` (the runtime
/// had no such protocol registered by name on the iOS 26.2 simulator), and `ClassDecl::new(..)
/// .unwrap()` / `.expect(..)` on a class name that is taken are the same abort. A failed lookup
/// degrades or logs; it does not panic.
#[test]
fn the_ios_shell_does_not_unwrap_what_the_objective_c_runtime_hands_it() {
    for (file, source) in [
        ("ios/mod.rs", IOS_MOD_RS),
        ("ios/accessibility.rs", IOS_A11Y_RS),
        ("ios/text_input.rs", IOS_TEXT_INPUT_RS),
        ("ios/clipboard.rs", IOS_CLIPBOARD_RS),
        ("ios/webview.rs", IOS_WEBVIEW_RS),
    ] {
        for (n, line) in code_of(source).lines().enumerate() {
            // The one documented exception: the trait's `prepare_callback_invocation` has no way
            // to answer "no layout window", and a window that dispatches a callback has one.
            if line.contains("Layout window must exist for callback invocation") {
                continue;
            }
            assert!(
                !line.contains(".unwrap()") && !line.contains(".expect("),
                "{file}:{}: `{}` - a panic aborts a phone app; handle the None/Err instead",
                n + 1,
                line.trim()
            );
        }
    }
}

/// UIKit's recognizers own the touches they analyse unless told not to: `cancelsTouchesInView`
/// (default YES) sends the view `touchesCancelled:` once a recognizer succeeds - a scrolling flick
/// IS a swipe, so every quick scroll was cut off after its first fifty points - and
/// `delaysTouchesEnded` (default YES) holds `touchesEnded:` back until the double-tap recognizer
/// has failed, which put every tap on every button a third of a second late.
#[test]
fn every_gesture_recognizer_of_the_ios_shell_only_observes_the_touches() {
    let install = body(IOS_MOD_RS, "unsafe fn install_gesture_recognizers(");
    at(install, "setCancelsTouchesInView: false", "install_gesture_recognizers");
    at(install, "setDelaysTouchesEnded: false", "install_gesture_recognizers");
    at(install, "setDelaysTouchesBegan: false", "install_gesture_recognizers");
    // Every recognizer goes through the one helper that does it.
    assert_eq!(
        install.matches("observe_only(r)").count(),
        2,
        "the basic recognizers (via attach_basic) and the four swipes (via attach_swipe)"
    );
    assert_eq!(
        install.matches("msg_send![view, addGestureRecognizer: r]").count(),
        2,
        "no recognizer is added anywhere but in the two helpers"
    );
}

/// `UIKeyboardWillChangeFrameNotification` writes the keyboard's inset and every layout pass of
/// the view reads the others: the pass must keep what the notification wrote (it rebuilt the whole
/// struct with `keyboard: None`), and a change of the insets alone - landscape-left to
/// landscape-right keeps the bounds - must ask for a layout.
#[test]
fn a_layout_pass_of_the_ios_view_keeps_the_keyboard_inset_and_asks_for_a_layout_when_the_insets_move() {
    let pass = body(IOS_MOD_RS, "extern \"C\" fn layout_subviews(");
    at(pass, "safe_area_after_layout", "layout_subviews");
    assert!(
        !pass.contains("keyboard:"),
        "layout_subviews must not write the keyboard field: it belongs to the keyboard notification"
    );
    at(pass, "inset_changed", "layout_subviews");
    at(pass, "request_regeneration", "layout_subviews");
}

/// `becomeFirstResponder` / `resignFirstResponder` call back into the view SYNCHRONOUSLY, and the
/// view's `UITextInput` answers read the window through the global pointer: the display tick takes
/// the request first and calls UIKit after the borrow of the layout window has ended.
#[test]
fn the_display_tick_calls_uikit_for_the_keyboard_outside_its_borrow_of_the_layout_window() {
    let tick = body(IOS_MOD_RS, "extern \"C\" fn display_tick(");
    let take = tick
        .find("take_soft_keyboard_request")
        .expect("display_tick takes the soft keyboard request");
    let call = tick
        .find("set_soft_keyboard_visible(visible)")
        .expect("display_tick raises or dismisses the keyboard");
    assert!(take < call);
    assert!(
        tick[..call].contains(".and_then(|lw| lw.text_edit_manager.take_soft_keyboard_request())"),
        "the request is taken by a closure that returns it, so the borrow ends before the call"
    );
    assert!(
        !tick[take..call].contains("if let Some(lw)"),
        "set_soft_keyboard_visible must not run inside an `if let Some(lw) = ...as_mut()`"
    );
}

/// `run()` called `setup_debug_and_e2e` and dropped the receiver: `AZ_E2E` queued its scenarios
/// and nothing ever read them, so no e2e run could work on a phone. The channel goes to the
/// delegate (as `ANDROID_DEBUG_CHANNEL` goes to `android_main`) and the delegate registers the
/// timer that the display tick drives.
#[test]
fn the_debug_channel_of_an_ios_run_reaches_the_window_instead_of_being_dropped() {
    let run = body(RUN_RS, "#[cfg(target_os = \"ios\")]\npub fn run(");
    at(run, "IOS_DEBUG_CHANNEL = Some((rx, cm))", "iOS run()");
    let launch = body(IOS_MOD_RS, "extern \"C\" fn did_finish_launching(");
    at(launch, "IOS_DEBUG_CHANNEL.take()", "did_finish_launching");
    at(launch, "register_debug_timer(&mut window", "did_finish_launching");
    // The timer is registered before the window is published to UIKit's callbacks.
    let register = launch.find("register_debug_timer").expect("registered");
    let publish = launch.find("AZUL_IOS_WINDOW = ").expect("published");
    assert!(register < publish);
}

/// A panic is an abort in a phone app and its `stderr` goes nowhere: the hook also writes the
/// message to the system log, where the simulator's `log stream`, Console.app and a device's
/// syslog read it. The launch failures say so there too.
#[test]
fn a_panic_and_a_failed_launch_of_the_ios_app_are_written_to_the_system_log() {
    const LOGGING_RS: &str = include_str!("../logging.rs");
    at(
        LOGGING_RS,
        "crate::desktop::shell2::ios::system_log(&error_str)",
        "the panic hook",
    );
    let launch = body(IOS_MOD_RS, "pub unsafe fn launch_app(");
    at(launch, "system_log(", "launch_app");
    let finish = body(IOS_MOD_RS, "extern \"C\" fn did_finish_launching(");
    at(finish, "system_log(&message)", "did_finish_launching");
}
