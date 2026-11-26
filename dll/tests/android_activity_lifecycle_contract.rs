//! What `AzulActivity` and the manifest must do for the lifecycle of a native app, pinned in source.
//!
//! The Java side of a NativeActivity app cannot be run where the tests run, and these are the things the
//! first emulator runs of 2026-11-26 found it got wrong. Each test names its evidence:
//!
//! * the process ends with its activity: libazul is not re-entrant (its load-time constructor hands the
//!   app's window options to `android_main` ONCE per process, the JNI cookies and the input view are
//!   per-process), so a second activity in the same process (BACK, then the launcher icon; any
//!   configuration change the manifest does not list) found no options, `android_main` returned at once
//!   and the user saw a window that never drew (QA runs 45771255766, "BACK then relaunch");
//! * the manifest lists every configuration change the app handles: "apps targeting API 13+ must declare
//!   screenSize AND smallestScreenSize" (android:configChanges), and a font size changed in Settings
//!   (`fontScale`) restarted every running app;
//! * the bridges attach when `android_main` has published its object, not at "the next focus event";
//! * sensors stopped at the first `onPause` and never came back (a permission dialog pauses the activity);
//! * no media session until the app asks (`AppConfig::expose_system_media_controls`);
//! * a device without a WebView provider does not take the app with it: `new WebView` throws there
//!   (`WebViewFactory.MissingWebViewPackageException`, a RuntimeException on the UI thread = process death).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_activity_lifecycle_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its comments.
fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body of a Java method of `src` starting at `signature` (up to its matching brace).
fn method_body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("no `{signature}` in AzulActivity.java"));
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

#[test]
fn the_process_ends_with_the_activity() {
    let src = java_code("scripts/android/AzulActivity.java");
    let body = method_body(&src, "protected void onDestroy()");
    let super_at = body.find("super.onDestroy()").expect("onDestroy calls super");
    let kill_at = body
        .find("Process.killProcess(")
        .expect("onDestroy must end the process: a second activity in it cannot run (the options are stashed once)");
    assert!(kill_at > super_at, "the process ends AFTER super.onDestroy() returned (the native side has finished)");
}

#[test]
fn the_manifest_lists_every_configuration_change_the_app_handles() {
    let manifest = read("scripts/android/AndroidManifest.xml");
    let at = manifest.find("android:configChanges=\"").expect("the activity declares configChanges");
    let list = manifest[at + 23..].split('"').next().unwrap();
    for needed in [
        "orientation",
        "screenSize",
        "smallestScreenSize",
        "screenLayout",
        "keyboard",
        "keyboardHidden",
        "navigation",
        "density",
        "uiMode",
        "locale",
        "layoutDirection",
        "fontScale",
        "colorMode",
        // "Bold text" (API 31) and the grammatical gender (API 34) are configuration changes too: all 18
        // values of `attr/configChanges` in android-34 are listed, so no setting restarts the activity.
        "fontWeightAdjustment",
        "grammaticalGender",
    ] {
        assert!(
            list.split('|').any(|v| v == needed),
            "android:configChanges lacks `{needed}`: the system would restart the activity and a native app cannot restart in its process"
        );
    }
}

#[test]
fn the_bridges_attach_when_the_native_side_is_ready_not_at_the_next_focus_event() {
    let src = java_code("scripts/android/AzulActivity.java");
    let attach = method_body(&src, "private void attachBridges(");
    assert!(
        attach.contains("postDelayed("),
        "attachBridges must ask again until android_main has published its object"
    );
    let focus = method_body(&src, "public void onWindowFocusChanged(");
    assert!(focus.contains("attachBridges("), "the focus event starts the attach");
}

#[test]
fn sensors_are_paused_and_resumed_with_the_activity_and_stopped_for_good_only_at_the_end() {
    let src = java_code("scripts/android/AzulActivity.java");
    assert!(method_body(&src, "protected void onPause()").contains("AzulSensors.pause("));
    assert!(method_body(&src, "protected void onResume()").contains("AzulSensors.resume("));
    assert!(!method_body(&src, "protected void onPause()").contains("AzulSensors.stop("), "stop forgets what the app asked for");
    assert!(method_body(&src, "protected void onDestroy()").contains("AzulSensors.stop("));
    let sensors = java_code("scripts/android/AzulSensors.java");
    assert!(sensors.contains("wanted"), "the sensors remember that the app asked for readings");
}

#[test]
fn the_activity_does_not_start_the_media_session() {
    let src = java_code("scripts/android/AzulActivity.java");
    assert!(
        !src.contains("AzulMediaSession.start("),
        "an app asks for a media session (AppConfig::expose_system_media_controls); the activity must not claim one for every app"
    );
}

/// A hardware keyboard's letters (and `adb shell input text`) reach the activity after the IME and the
/// native input queue passed them by, and only Java can read the character off the event
/// (`KeyEvent.getUnicodeChar()`): the activity forwards it, the shell decides whether it types.
#[test]
fn the_activity_forwards_the_character_of_a_key_nobody_consumed() {
    let src = java_code("scripts/android/AzulActivity.java");
    let body = method_body(&src, "public boolean dispatchKeyEvent(");
    assert!(
        body.contains("NativeTextBridge.keyText("),
        "dispatchKeyEvent must hand the key's character to the shell (a hardware keyboard could not type at all)"
    );
}

/// An app process has no HOME and no XDG_* variables: `dirs::home_dir()` is None on Android (dirs-sys'
/// getpwuid fallback is compiled out), so `FilePath::get_data_dir()`, `get_cache_dir()` and
/// `get_config_dir()` answered None and `std::env::temp_dir()` was `/data/local/tmp`, which an app cannot
/// write: an app that keeps a pairing or a cache through them kept nothing (AzCompanion paired again at
/// every start). The activity sets them from `getFilesDir()` / `getCacheDir()` before the library loads
/// (the app's load-time constructor runs inside `System.loadLibrary`).
#[test]
fn the_apps_private_folders_are_in_the_environment_before_the_library_loads() {
    let src = java_code("scripts/android/AzulActivity.java");
    let on_create = method_body(&src, "protected void onCreate(");
    let prepare_at = on_create.find("prepareAppDirs()").expect("onCreate prepares the app's folders");
    let load_at = on_create.find("System.loadLibrary(").expect("onCreate loads the library");
    assert!(prepare_at < load_at, "the folders are set BEFORE System.loadLibrary (the constructor runs inside it)");
    let prepare = method_body(&src, "private void prepareAppDirs()");
    for var in ["HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "TMPDIR"] {
        assert!(prepare.contains(&format!("\"{var}\"")), "{var} must be set from the app's private folders");
    }
    assert!(prepare.contains("getFilesDir()") && prepare.contains("getCacheDir()"));
}

/// `<webview>` on a device that has no WebView provider (an emulator image without one, a de-Googled phone,
/// a provider that is being updated or was disabled): `new WebView(context)` and `CookieManager.getInstance()`
/// throw `MissingWebViewPackageException` (android.webkit.WebViewFactory), and `AzulWebView.create` ran them
/// inside `runOnUiThread` with nothing to catch it: an uncaught exception on the UI thread ends the process.
/// The app asked for a page, not for its own death: the engine's view is told the load failed.
///
/// Mutation: the creation outside a try block (the state of 0f3e1eb093).
#[test]
fn a_device_without_a_web_view_provider_does_not_take_the_app_with_it() {
    let src = java_code("scripts/android/AzulWebView.java");
    let body = method_body(&src, "public static void create(");
    let try_at = body.find("try {").expect("create must guard the creation of the web view with a try block");
    let new_at = body.find("new WebView(").expect("create makes the WebView");
    let catch_at = body
        .find("catch (Throwable")
        .expect("catch Throwable: the provider's exception is a RuntimeException, a missing class can be an Error");
    let failed_at = body.rfind("nativeOnLoadFailed(").expect("the engine's view is told that it will not load");
    assert!(try_at < new_at, "`new WebView` must be inside the try block");
    assert!(new_at < catch_at, "the catch follows the creation");
    assert!(catch_at < failed_at, "the catch handler reports the failure to the engine");
    let clear_at = body.find("clearStore()").expect("create clears the store of the first ephemeral view");
    assert!(try_at < clear_at, "`clearStore` (CookieManager.getInstance) needs the provider too: inside the try block");
}
