//! What the Android shell tells an app about the device, and what it does with the device's input, pinned in
//! source (a phone cannot run where the tests run). Each test names the ledger row it came from
//! (briefs/mobile_capabilities_ledger_2026-11-28.md, the Android register AN-n).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_capability_glue_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its line comments.
fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rust source without its line comments and without whitespace (rustfmt breaks the calls over lines).
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The body of a method or function of `src` starting at `signature` (up to its matching brace).
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("no `{signature}` in the source"));
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

/// `PlatformCapability::sensors / gamepad / geolocation` told an app that an Android phone has none of them
/// ("helper pending", "backend not implemented yet") while `AzulSensors`, `AzulGamepad` and `AzulGeolocation` ship
/// and work: an app that gated its compass, its pad UI or its map on the probe never started the feature (AN-2).
///
/// Mutation: the Android arm of `sensors()` answering `cap(false, ..)` (the state of 46d6ad5ffd).
#[test]
fn the_capability_probes_do_not_deny_the_helpers_that_ship() {
    for helper in ["AzulSensors", "AzulGamepad", "AzulGeolocation"] {
        let path = format!("scripts/android/{helper}.java");
        assert!(!read(&path).is_empty(), "{helper}.java ships with the APK");
    }
    let rust = rust_code("dll/src/desktop/extra/capability.rs");
    for (function, backend) in [
        ("pubfnsensors()->PlatformCapability{", "\"SensorManager(JNI)\""),
        ("pubfngamepad()->PlatformCapability{", "\"InputDevice(JNI)\""),
        ("pubfngeolocation()->PlatformCapability{", "\"LocationManager(JNI)\""),
    ] {
        let probe = body(&rust, function);
        let at = probe
            .find(&format!("cap(true,{backend}"))
            .unwrap_or_else(|| panic!("the Android arm of {function} answers available with {backend}"));
        let android = probe[..at].rfind("cfg!(target_os=\"android\")").expect("it is the Android arm");
        assert!(android < at);
        assert!(!probe[android..at].contains("cap(false"), "no refusal between the Android test and its answer");
    }
}

/// `AzulMediaSession.start` runs on the engine's LOOP thread, which has no Looper: `MediaSession.setCallback(cb)`
/// without a handler builds `new Handler()` for the calling thread and throws there. The `catch (Throwable)` swallowed
/// it and the session stayed null: a music app never appeared on the lock screen, and its headset buttons did nothing
/// (AN-1). The callbacks run on the main looper, and a session that was made and not finished is released.
///
/// Mutation: `}, new Handler(Looper.getMainLooper()));` replaced by `});` (the state of 46d6ad5ffd).
#[test]
fn the_media_session_callback_runs_on_the_main_looper() {
    let java = java_code("scripts/android/AzulMediaSession.java");
    let start = body(&java, "public static void start(Activity activity)");
    let set = start.find("session.setCallback(").expect("the session gets its callback");
    let handler = start.find("}, new Handler(Looper.getMainLooper()));").expect("with the main looper's handler");
    let active = start.find("session.setActive(true)").expect("then it is activated");
    assert!(set < handler && handler < active);
    let catch_at = start.find("catch (Throwable t)").expect("a failure is caught");
    assert!(
        start[catch_at..].contains("session.release()"),
        "a half-made session is released, not left claimed and inert"
    );
}

/// A game pad's sticks and triggers are `MotionEvent`s that reach the native input queue like a finger's, with no
/// position: the shell collected them as a pointer, so a stick deflection became a phantom touch near (0, 0)
/// (AN-8). The native loop leaves them UNHANDLED (they continue to `AzulGamepad.onMotion`, the pad's own path), and
/// the triggers are read from whichever axis pair the pad moves (`AXIS_LTRIGGER`/`RTRIGGER` or `AXIS_BRAKE`/`GAS`).
///
/// Mutation: the early return removed from the shell (the state of 46d6ad5ffd).
#[test]
fn a_pad_stick_is_not_collected_as_a_finger() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let guard = shell
        .find("ifcrate::desktop::shell2::android_core::touch::is_a_pad_stick(u32::from(m.source())){returnInputStatus::Unhandled;}")
        .expect("the native loop leaves a pad's axes to the pad's path");
    let collected = shell[guard..]
        .find("letmuttouch_points:Vec<azul_core::window::TouchPoint>")
        .expect("the touch points are collected after it");
    assert!(collected > 0);
    let before_guard = &shell[..guard];
    assert!(
        before_guard.contains("Source::RotaryEncoder"),
        "the guard sits with the other source early returns, before the pointer is read"
    );

    let java = java_code("scripts/android/AzulGamepad.java");
    let motion = body(&java, "public static boolean onMotion(");
    assert!(motion.contains("MotionEvent.AXIS_BRAKE") && motion.contains("MotionEvent.AXIS_GAS"));
    assert!(motion.contains("Math.max(event.getAxisValue(MotionEvent.AXIS_LTRIGGER)"));
    assert!(motion.contains("Math.max(event.getAxisValue(MotionEvent.AXIS_RTRIGGER)"));
}

/// A haptic composition on hardware that cannot play its primitive plays NOTHING and answers no error: `vibrate`
/// of an unsupported primitive is silent, and the shell answered Ok(true), so the caller never fell back to the
/// standard `performHapticFeedback` (AN-9). The shell asks `Vibrator.areAllPrimitivesSupported` first and answers
/// "not played" when the answer is no (or the call is missing).
///
/// Mutation: the `areAllPrimitivesSupported` check removed (the state of 46d6ad5ffd).
#[test]
fn a_haptic_primitive_the_hardware_cannot_play_falls_back() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let vibrator = body(
        &shell,
        "fnplay_via_vibrator(env:&mutjni::JNIEnv,activity:&jni::objects::JObject,request:&azul_core::haptics::HapticRequest,)->bool{",
    );
    let has = vibrator.find("\"hasVibrator\"").expect("a device without an actuator is asked first");
    let supported = vibrator.find("\"areAllPrimitivesSupported\"").expect("then whether the actuator plays the primitive");
    let compose = vibrator.find("\"startComposition\"").expect("before a composition is made");
    assert!(has < supported && supported < compose);
    let after = &vibrator[supported..compose];
    assert!(after.contains("Ok(false)=>returnOk(false)"), "unsupported: not played, the caller falls back");
    assert!(after.contains("Err(_)=>"), "a missing method does not end in an exception");
}

/// Home and back gives the window a NEW surface, which showed black until the layout and the render of the next pass were
/// done (about 130 ms, QA's flicker audit, ledger E47). The CPU frame that was on the screen is posted on it at once, in
/// the surface-created handler, before the layout is requested, when it is the size of the surface (the rule is
/// `android_core::present::retained_frame_fits`, tested on every host) and only for a window that draws on the CPU (the
/// GPU path owns its surface and presents its own frame as soon as it has one).
///
/// Mutation: the early `render_frame` removed from `InitWindow` (the state of 6ce827b284).
#[test]
fn the_retained_frame_goes_on_a_new_surface_before_the_layout() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = shell.find("MainEvent::InitWindow{..}=>{").expect("the surface-created arm") + "MainEvent::InitWindow{..}".len();
    let init = body(&shell[arm..], "=>");
    let gpu = init.find("window.gpu_surface_created();").expect("the GPU path takes the surface first");
    let gate = init
        .find("window.draws_on_cpu()&&crate::desktop::shell2::android_core::present::retained_frame_fits(retained,surface_px)")
        .expect("the retained frame is posted only to a CPU window and only at the surface's size");
    let post = init[gate..].find("render_frame(window)").expect("it is posted") + gate;
    let layout = init.find("window.common.request_regeneration(reason)").expect("the layout is requested");
    assert!(gpu < gate && gate < post && post < layout, "the frame is posted before the layout is requested");
}
