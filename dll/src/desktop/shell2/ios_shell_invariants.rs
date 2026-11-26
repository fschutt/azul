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
const FILE_PICKER_RS: &str = include_str!("../extra/file_picker/ios.rs");
const CAMERA_RS: &str = include_str!("../extra/camera/avfoundation.rs");
const AVF_AUTH_RS: &str = include_str!("../extra/camera/avf_auth.rs");
const SENSORS_APPLE_RS: &str = include_str!("../extra/sensors/apple.rs");
const SPEECH_APPLE_RS: &str = include_str!("../extra/speech/apple.rs");
const AUDIO_SINK_RS: &str = include_str!("../extra/audio/avfoundation_sink.rs");
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
        ("extra/file_picker/ios.rs", FILE_PICKER_RS),
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
/// and nothing ever read them, so no e2e run could work on a phone. The channel travels to the
/// delegate in the one launch value (`IosLaunch::debug_channel`, no second global) and the
/// delegate registers the timer that the display tick drives.
#[test]
fn the_debug_channel_of_an_ios_run_reaches_the_window_instead_of_being_dropped() {
    let run = body(RUN_RS, "#[cfg(target_os = \"ios\")]\npub fn run(");
    at(run, "debug_channel: _debug_request_rx.zip(_component_map)", "iOS run()");
    let launch = body(IOS_MOD_RS, "extern \"C\" fn did_finish_launching(");
    at(launch, "debug_channel,", "did_finish_launching destructures the launch value");
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

/// The escalation ladder: a pass's result is acted on at the weight it was classified. The iOS
/// shell rebuilt the DOM (the app's `layout()`) for every result but `DoNothing` - at the touch
/// rate while a finger scrolled, and over a committed IME string (the app's model had not caught
/// up) - and again on every timer tick that asked for a repaint.
#[test]
fn the_ios_shell_acts_on_a_result_at_the_weight_the_pass_classified_it() {
    // One tail for every input site.
    let settle = body(IOS_MOD_RS, "fn settle(window: &mut IOSWindow");
    at(settle, "apply_event_result(result)", "settle");
    assert!(!settle.contains("RefreshDom"), "settle must not rebuild the DOM by itself");

    let apply = body(IOS_MOD_RS, "pub(crate) fn apply_event_result(");
    at(apply, "frame_step(result)", "apply_event_result");
    at(apply, "incremental_relayout_dispatching(", "apply_event_result");
    at(apply, "request_relayout_only()", "apply_event_result");

    // The touch path goes through it and writes no DOM rebuild of its own.
    let touch = body(IOS_MOD_RS, "fn handle_touch(");
    at(touch, "settle(window, r)", "handle_touch");
    assert!(
        !touch.contains("RelayoutReason::RefreshDom"),
        "handle_touch must not rebuild the DOM per touch event"
    );

    // A timer tick that asks for a redraw asks for a REPAINT.
    let tick = body(IOS_MOD_RS, "extern \"C\" fn display_tick(");
    at(tick, "window.needs_rerender = true", "display_tick");
    assert!(
        !tick.contains("RelayoutReason::RefreshDom"),
        "display_tick must not force a DOM rebuild on every timer tick"
    );
    at(
        tick,
        "regeneration_pending() || window.needs_rerender",
        "display_tick presents a pending repaint as well as a pending regeneration",
    );

    // A relayout that was already performed is not thrown away by a regeneration.
    let layer = body(IOS_MOD_RS, "fn present_pending_frame(");
    let only = layer.find("take_relayout_only()").expect("the frame function reads relayout-only");
    let regenerate = layer.find("regeneration_pending()").expect("and the regeneration");
    assert!(only < regenerate, "relayout-only is tested before the regeneration request");
}

/// A latched native gesture only becomes an event inside a pass, and the pass that consumes it
/// clears it: macOS injects, snapshots and runs that pass at once. The iOS recognizers asked for a
/// DOM rebuild and waited for a later pass, so no double-tap, long-press, swipe, pinch or rotation
/// callback ran in time.
#[test]
fn a_recognized_ios_gesture_is_dispatched_by_a_pass_of_its_own() {
    let inject = body(IOS_MOD_RS, "fn inject(window: &mut IOSWindow");
    in_order(
        inject,
        "inject_native_gesture(gesture)",
        "snapshot_window_state_baseline(",
        "inject",
    );
    in_order(inject, "snapshot_window_state_baseline(", "process_window_events(0)", "inject");
    in_order(inject, "process_window_events(0)", "settle(window, result)", "inject");
}

/// State first, then the pass: the pen's pressure and tilt are written before the pass that
/// dispatches the touch's callbacks (macOS writes them in the tablet handler, ahead of the mouse
/// event). After it, a callback read the previous sample's.
#[test]
fn the_pen_state_of_an_ios_touch_is_written_before_the_pass_that_reads_it() {
    let touch = body(IOS_MOD_RS, "fn handle_touch(");
    in_order(
        touch,
        "update_pen_state_full(",
        "process_window_events(0)",
        "handle_touch",
    );
}

fn in_order(haystack: &str, first: &str, then: &str, what: &str) {
    let a = haystack
        .find(first)
        .unwrap_or_else(|| panic!("{what}: `{first}` is missing"));
    let b = haystack
        .find(then)
        .unwrap_or_else(|| panic!("{what}: `{then}` is missing"));
    assert!(a < b, "{what}: `{first}` must come before `{then}`");
}

/// UIKit sets `UITraitCollection.current` (thread-local) only around `drawRect:`,
/// `layoutSubviews` and the trait-change callbacks, and documents its contents as undefined
/// anywhere else. The display tick is not one of them: the device's appearance is read from the
/// view's own `traitCollection`.
#[test]
fn the_ios_shell_reads_the_appearance_from_the_view_not_from_the_current_trait_collection() {
    assert!(
        !code_of(IOS_MOD_RS).contains("currentTraitCollection"),
        "UITraitCollection.currentTraitCollection is undefined outside UIKit's own callbacks"
    );
    let probe = body(IOS_MOD_RS, "unsafe fn probe_user_interface_style(");
    at(probe, "msg_send![view, traitCollection]", "probe_user_interface_style");
}

/// Every `CallbackInfo` of a window carries the window's handle: macOS passes
/// `self.get_raw_window_handle()`, and the iOS shell passed an all-null one, so what needs the
/// UIWindow (the web-authentication anchor, the document picker) fell back to the deprecated
/// `[UIApplication keyWindow]`.
#[test]
fn a_callback_of_an_ios_window_carries_the_real_window_handle() {
    let prepare = body(IOS_MOD_RS, "fn prepare_callback_invocation(");
    at(prepare, "self.get_raw_window_handle()", "prepare_callback_invocation");
    assert!(
        !prepare.contains("null_mut()"),
        "prepare_callback_invocation must not hand out null handles"
    );
}

/// UIKit validates the edit menu's actions again and again; answering "is there something to
/// paste" by READING the pasteboard's string copies the whole contents each time, and is the read
/// iOS reports to the user as a paste (14+) and asks permission for (16+). `hasStrings` is the
/// documented question ("Do not use the string or strings properties to determine whether a
/// pasteboard contains string data, because doing so consumes resources needlessly").
#[test]
fn the_ios_edit_menu_asks_the_pasteboard_whether_it_has_strings_without_reading_them() {
    let has_text = body(IOS_CLIPBOARD_RS, "pub fn has_text()");
    at(has_text, "hasStrings", "has_text");
    assert!(
        !has_text.contains("get_clipboard_content") && !has_text.contains("string]"),
        "has_text must not read the pasteboard's contents"
    );
}

/// The picker asked `UTType` for its well-known types with `performSelector:` (`png`, `pdf`,
/// `data`, `folder`): they are Objective-C global constants, not class methods, so the runtime
/// answered "unrecognized selector" - an exception that aborts through Rust frames - and the
/// selector itself was registered from a string with a NUL in it, which panics first. Every use of
/// the picker died. The types are asked for by identifier, through the pure mapping.
#[test]
fn the_ios_file_picker_asks_uttype_only_for_selectors_that_exist() {
    let code = code_of(FILE_PICKER_RS);
    assert!(!code.contains("performSelector"), "no performSelector: on UTType");
    assert!(
        !code.contains("Sel::register("),
        "no selector is built at run time (a NUL in the name is a panic)"
    );
    assert!(
        !code.contains("class!(UTType)"),
        "class! panics when UniformTypeIdentifiers is not loaded: Class::get"
    );
    for missing in ["UTType), data]", "UTType), folder]"] {
        assert!(!code.contains(missing), "+[UTType ...] has no such class method: {missing}");
    }
    at(&code, "typeWithIdentifier:", "the picker");
    at(&code, "uti_query_for_descriptor(descriptor)", "the picker");
    // A folder opened in place is security-scoped; the caller only receives a path.
    at(&code, "startAccessingSecurityScopedResource", "the picker's delegate");
}

/// UIKit's documented shapes of the view's overrides and of the display link: `layoutSubviews`
/// calls super (the default implementation applies constraints and autoresizing to the subviews,
/// and the `<webview>` containers are subviews), super is `UIView` itself and not `[self
/// superclass]` (a KVO subclass of the view would send it back into the same override), the
/// press sequence's cancellation is handled (a stuck key after an app switch), and the display
/// link runs in the COMMON run-loop modes (in the default mode alone it stops while UIKit
/// tracks a scroll, a menu or a keyboard drag).
#[test]
fn the_ios_view_follows_uikits_documented_override_and_run_loop_rules() {
    let layout = body(IOS_MOD_RS, "extern \"C\" fn layout_subviews(");
    at(layout, "super(this_mut, class!(UIView)), layoutSubviews", "layout_subviews");

    let forward = body(IOS_MOD_RS, "unsafe fn forward_presses_to_super(");
    assert!(
        !code_of(forward).contains("superclass]"),
        "the superclass is the UIView constant, not [self superclass]"
    );
    assert_eq!(forward.matches("super(this_mut, class!(UIView))").count(), 3);
    at(IOS_MOD_RS, "sel!(pressesCancelled:withEvent:)", "the view class");
    at(IOS_MOD_RS, "fn release_pressed_keys(", "the shell");
    let resign = body(IOS_MOD_RS, "extern \"C\" fn app_will_resign_active(");
    at(resign, "release_pressed_keys(window)", "applicationWillResignActive:");

    let link = body(IOS_MOD_RS, "unsafe fn install_display_link(");
    at(link, "NSRunLoopCommonModes", "install_display_link");
    assert!(
        !link.contains("kCFRunLoopDefaultMode"),
        "a display link in the default mode alone stops during tracking"
    );

    // The iOS 17.5 spelling of the pencil tap is handled next to the deprecated one.
    at(IOS_MOD_RS, "sel!(pencilInteraction:didReceiveTap:)", "the view class");
    at(IOS_MOD_RS, "sel!(pencilInteractionDidTap:)", "the view class");
}

/// An editable `UITextInteraction` brings tap, double-tap and long-press recognizers of its own;
/// on a view that fills the window they would turn a tap on a plain button into a request for the
/// keyboard and a system caret. It is added while an editable node has the focus and removed when
/// it does not (UIKit: "add the interaction that matches the state of text to the view while
/// removing the other").
#[test]
fn the_ios_text_interaction_belongs_to_the_view_only_while_text_is_being_edited() {
    let create = body(IOS_MOD_RS, "pub fn new(\n        mut options: WindowCreateOptions,");
    assert!(
        !create.contains("addInteraction: interaction];\n                    text_interaction"),
        "the text interaction is created in new(), not added"
    );
    let keyboard = body(IOS_MOD_RS, "pub fn set_soft_keyboard_visible(visible: bool)");
    in_order(keyboard, "addInteraction: interaction", "becomeFirstResponder", "set_soft_keyboard_visible");
    in_order(keyboard, "resignFirstResponder", "removeInteraction: interaction", "set_soft_keyboard_visible");
}

/// A keyboard another app raised (iPad multitasking tells every visible app) is not this app's.
#[test]
fn the_ios_keyboard_notification_ignores_another_apps_keyboard() {
    let frame = body(IOS_MOD_RS, "extern \"C\" fn ui_keyboard_frame_changed(");
    at(frame, "UIKeyboardIsLocalUserInfoKey", "ui_keyboard_frame_changed");
    at(frame, "keyboard_event_is_ours(is_local)", "ui_keyboard_frame_changed");
}

/// `delete_selection(target, forward)`: `false` is Backspace (`CallbackChange::DeleteBackward`
/// passes `false`, `DeleteForward` `true`). `UIKeyInput.deleteBackward` is the soft keyboard's
/// Backspace; the shell passed `true` and so deleted FORWARD: nothing at the end of a field, the
/// next character anywhere else.
#[test]
fn the_soft_keyboards_backspace_deletes_backward() {
    let backspace = body(IOS_MOD_RS, "extern \"C\" fn ui_delete_backward(");
    at(backspace, "lw.delete_selection(focused, false)", "ui_delete_backward");
    assert!(
        !code_of(backspace).contains("delete_selection(focused, true)"),
        "true is the Delete key"
    );
}

/// UIKit's positions are UTF-16 units and the engine's are bytes: every position the text-input
/// methods take or hand out crosses the bridge, and none is sliced or stepped as if both were
/// the same unit.
#[test]
fn the_ios_text_input_converts_between_utf16_units_and_bytes_at_every_seam() {
    let code = code_of(IOS_TEXT_INPUT_RS);
    for needle in [
        "utf16_offset_to_byte(",
        "byte_offset_to_utf16(",
        "utf16_range_to_bytes(",
        "position_from_offset(",
        "one_character_around(",
    ] {
        at(&code, needle, "text_input.rs");
    }
    assert!(
        !code.contains("clamp_to_char_boundary"),
        "a UTF-16 offset becomes a byte offset on a boundary by construction"
    );
    // The marked text's selection is UTF-16 relative to the marked string.
    let marked = body(IOS_TEXT_INPUT_RS, "extern \"C\" fn set_marked_text(");
    at(marked, "utf16_range_to_bytes(&text", "set_marked_text");
    // The range replacement counts characters, not bytes.
    let replace = body(IOS_TEXT_INPUT_RS, "extern \"C\" fn replace_range_with_text(");
    at(replace, "chars().count()", "replace_range_with_text");
    assert!(!code_of(replace).contains("0..(end - start)"));
}

/// The CPU renderer's pixels are sRGB (CSS colours); handed to Core Graphics as a DEVICE colour
/// space they are read in the display's own gamut (Display P3 on a phone) and come out more
/// saturated than the author set them.
#[test]
fn the_ios_frame_is_presented_in_the_srgb_colour_space() {
    let layer = body(IOS_MOD_RS, "fn present_pending_frame(");
    at(layer, "CGColorSpaceCreateWithName(kCGColorSpaceSRGB)", "present_pending_frame");
}

/// A Rust `extern static` is a STRONG import: the Mach-O requires the symbol and dyld refuses to
/// launch the whole app where it is missing - a null check on its value comes too late. The camera
/// module named `AVCaptureDeviceTypeExternal` and `AVCaptureDeviceTypeContinuityCamera` (iOS 17.0)
/// and `AVCaptureDeviceTypeExternalUnknown` (unavailable on iOS) as statics in an app whose
/// `MinimumOSVersion` is 16.0. They are looked up by name at run time (`dlsym`); the build's
/// `scripts/ci/ios_symbol_gate.py` fails on any other symbol newer than the target.
#[test]
fn the_camera_does_not_import_device_types_newer_than_the_deployment_target() {
    let code = code_of(CAMERA_RS);
    for newer in [
        "AVCaptureDeviceTypeExternal",
        "AVCaptureDeviceTypeExternalUnknown",
        "AVCaptureDeviceTypeContinuityCamera",
    ] {
        assert!(
            !code.contains(&format!("addr_of!({newer})")) && !code.contains(&format!("{newer},")),
            "{newer} must not be an extern static: dlsym it"
        );
    }
    at(&code, "dlsym(RTLD_DEFAULT", "the camera's device types");
}

/// A bundle that asks the system for the camera, the microphone or motion data without the
/// purpose string in its Info.plist is stopped by the system. The engine asks `info_plist` first and
/// answers "unavailable" with the missing key named (never a crash, never a silent no-op).
#[test]
fn the_protected_requests_of_the_ios_build_check_their_purpose_string_first() {
    let auth = code_of(AVF_AUTH_RS);
    assert_eq!(
        auth.matches("request_would_stop_the_process(key)").count(),
        2,
        "the blocking and the non-blocking request both ask first"
    );
    at(&auth, "missing_key_message(", "avf_auth");
    let request = auth.find("requestAccessForMediaType_completionHandler(media").expect("a request");
    let check = auth.find("request_would_stop_the_process(key)").expect("a check");
    assert!(check < request, "the check comes before the first request");

    let sensors = body(SENSORS_APPLE_RS, "fn start_push_only_sensors(");
    in_order(
        sensors,
        "request_would_stop_the_process(keys::MOTION)",
        "CMAltimeter::isRelativeAltitudeAvailable()",
        "start_push_only_sensors",
    );
    in_order(
        sensors,
        "request_would_stop_the_process(keys::MOTION)",
        "startPedometerUpdatesFromDate_withHandler",
        "start_push_only_sensors",
    );
}

/// The default iOS audio session "disallows audio recording" (the engine's input reads zero Hz:
/// "there is no microphone to listen with", every time) and is silenced by the Ring/Silent switch
/// and the screen lock. The microphone of the speech recognizer and the audio sink set a session
/// that allows what they do before they build their engine.
#[test]
fn the_audio_users_of_the_ios_build_set_an_audio_session_before_their_engine() {
    let mic = body(SPEECH_APPLE_RS, "unsafe fn start() -> Result<Self, (Outcome, String)>");
    in_order(
        mic,
        "SessionUse::Record",
        "msg_send![engine, inputNode]",
        "AppleMicrophone::start",
    );
    let sink = body(AUDIO_SINK_RS, "pub(super) fn open(rate: u32, channels: u16)");
    in_order(
        sink,
        "SessionUse::Playback",
        "AVAudioEngine::new()",
        "AvfSink::open",
    );
}

/// The keyboard hangs on `UIKeyInput` and `UITextInput`: when neither the name nor the classes
/// that adopt them yield a protocol object, they are registered by name (conformance is by name),
/// not skipped.
#[test]
fn the_keyboard_protocols_are_registered_by_name_when_the_runtime_has_none() {
    let class = body(IOS_MOD_RS, "fn get_or_create_view_class()");
    assert_eq!(
        class.matches("register_protocol_by_name(").count(),
        2,
        "UIKeyInput and UITextInput"
    );
    at(IOS_MOD_RS, "fn register_protocol_by_name(", "the shell");
    at(IOS_MOD_RS, "ProtocolDecl::new(name)", "register_protocol_by_name");
}

/// The notification name, the user-info keys and the run-loop mode are exported constants: they
/// are linked, not rebuilt from a C string holding their conventional value.
#[test]
fn the_ios_shell_links_the_exported_constants_it_names() {
    let code = code_of(IOS_MOD_RS);
    for literal in [
        "b\"UIKeyboardWillChangeFrameNotification",
        "b\"UIKeyboardFrameEndUserInfoKey",
        "b\"UIKeyboardIsLocalUserInfoKey",
        "b\"kCFRunLoopDefaultMode",
    ] {
        assert!(!code.contains(literal), "{literal}: link the constant");
    }
    for linked in [
        "static UIKeyboardWillChangeFrameNotification",
        "static UIKeyboardFrameEndUserInfoKey",
        "static UIKeyboardIsLocalUserInfoKey",
        "static NSRunLoopCommonModes",
    ] {
        at(&code, linked, "the extern blocks of the iOS shell");
    }
}

/// The first finger of a group is primary and the next primary is born only once every finger has
/// lifted: the touch handler asks the pure rule instead of promoting any landing finger.
#[test]
fn the_touch_handler_asks_the_pure_rule_for_the_primary_finger() {
    let touch = body(IOS_MOD_RS, "fn handle_touch(");
    at(touch, "primary_after_began(", "handle_touch");
    assert!(
        !code_of(touch).contains("window.primary_touch_id = points.first()"),
        "no landing finger is promoted without asking the rule"
    );
}

/// "It is strongly recommended that you implement this method. If your app does not release enough
/// memory during low-memory conditions, the system may terminate it outright": the delegate
/// implements it and gives the frame pool back.
#[test]
fn the_ios_delegate_answers_a_memory_warning_by_releasing_what_it_can() {
    at(IOS_MOD_RS, "sel!(applicationDidReceiveMemoryWarning:)", "the app delegate class");
    let warning = body(IOS_MOD_RS, "extern \"C\" fn app_did_receive_memory_warning(");
    at(warning, "FrameBufferPool::new()", "app_did_receive_memory_warning");
}

/// A finger that went down is a TAP CANDIDATE: until it has moved more than the touch slop it scrolls nothing and the
/// pointer stays where it landed, a lift inside the slop is a click at the landing point, the lift of a pan clicks
/// nothing, and `touchesCancelled:` ("clean up any state": the system took the gesture over) releases the button over
/// NOTHING. The handler moved the pointer and the page by any move above a hundredth of a point (a real finger's
/// jitter scrolled the page and lost the click) and treated a cancelled touch as an ended one (a click on whatever
/// the finger was over). The rule is the Android shell's (`android_core::slop`), the decision table is pure
/// (`ios_logic::mouse_plan`).
#[test]
fn the_touch_handler_applies_the_touch_slop_and_never_clicks_a_cancelled_gesture() {
    at(IOS_MOD_RS, "tap: TapSlop,", "IOSWindow: the slop state of the primary finger");
    let touch = body(IOS_MOD_RS, "fn handle_touch(");
    let code = code_of(touch);
    for needed in [
        "window.tap.down(",
        "window.tap.moved(",
        "window.tap.up()",
        "window.tap.cancel()",
        "mouse_plan(",
        "TouchPhase::Cancelled",
        "CursorPlan::Leave",
        "HitPlan::Nowhere",
        "FullHitTest::empty(",
        "SlopMove::Hold",
        "SlopMove::Start",
    ] {
        assert!(code.contains(needed), "handle_touch: `{needed}` is missing");
    }
    assert!(
        !code.contains("2 | 3 if primary_in_set => ms.left_down = false"),
        "a cancelled touch is released by the plan, not as an ended one (a click)"
    );
}

/// THE BLACK WINDOW of the first Simulator launch (iOS 26.2: the app ran, the display link ticked, the debug server
/// answered, and nothing was ever laid out or drawn). The render view implements `displayLayer:` (the layer's delegate
/// method) and NOT `drawRect:`, and every request for a frame was `-[UIView setNeedsDisplay]` sent to the VIEW: UIKit
/// documents that call for a view that draws in `drawRect:`, a view without one is given no backing store to draw
/// into, and the request did not reach `displayLayer:`. So (1) every request goes to the view's LAYER
/// (`request_display`), the object `displayLayer:` is a delegate method of, and (2) the display tick, which runs
/// at the screen's refresh rate while a frame is owed, produces that frame itself (`present_pending_frame`, the body
/// of `displayLayer:`) instead of waiting for Core Animation to ask: the first layout never depends on a
/// UIKit call whose rules are not ours.
#[test]
fn a_frame_is_requested_from_the_layer_and_the_display_tick_produces_it_itself() {
    for (file, source) in [("ios/mod.rs", IOS_MOD_RS), ("ios/webview.rs", IOS_WEBVIEW_RS)] {
        let code = code_of(source);
        let mut rest = code.as_str();
        let mut sends = 0;
        while let Some(at) = rest.find("setNeedsDisplay") {
            sends += 1;
            rest = &rest[at + "setNeedsDisplay".len()..];
        }
        assert_eq!(
            sends, 1,
            "{file}: a redraw request is sent to the layer (one place: `request_display`, the web views' report), never to the view itself"
        );
    }
    at(
        &code_of(IOS_WEBVIEW_RS),
        "msg_send![self.wake_view, layer]",
        "the web views' report asks the render view's layer",
    );
    let request = body(IOS_MOD_RS, "unsafe fn request_display(");
    at(request, "msg_send![view, layer]", "request_display: the layer of the view");
    at(request, "setNeedsDisplay", "request_display");
    let layer_delegate = body(IOS_MOD_RS, "extern \"C\" fn display_layer(");
    at(layer_delegate, "present_pending_frame(", "displayLayer: delegates to the one frame function");
    let tick = body(IOS_MOD_RS, "extern \"C\" fn display_tick(");
    let tick_code = code_of(tick);
    assert!(
        tick_code.contains("present_pending_frame("),
        "the display tick produces the owed frame itself: the first layout does not wait for Core Animation to ask"
    );
}
