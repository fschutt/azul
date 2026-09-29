//! Source-text invariants for notification platform code that a host test
//! binary cannot run: it is `#[cfg(target_os = ...)]` code of another OS, or
//! Java in the APK. What it decides that a host CAN check lives in
//! `azul_layout::managers::notification::wire` and is tested there
//! (`layout/tests/native_notifications.rs`); these pin the SHAPES whose
//! absence was the defect, the way `loop_wakeup_invariants` does for the run
//! loops.

use crate::desktop::loop_wakeup_invariants::top_level_fn_body;

const PERMISSION_ANDROID_RS: &str = include_str!("../extra/permission/android.rs");
const AZUL_PERMISSIONS_JAVA: &str =
    include_str!("../../../../scripts/android/AzulPermissions.java");
const RUN_RS: &str = include_str!("../shell2/run.rs");
const IOS_RS: &str = include_str!("../shell2/ios/mod.rs");
const NOTIFY_LINUX_RS: &str = include_str!("linux.rs");

/// COM hands the click that started the process to the toast activator only
/// once its class object is registered - so the Windows `run()` registers it
/// (`install_launch_hooks`) before it builds its first window, not at the
/// first post.
#[test]
fn the_windows_loop_registers_the_toast_activator_before_its_first_window() {
    let start = RUN_RS
        .find("#[cfg(target_os = \"windows\")]\npub fn run(")
        .expect("the Windows run()");
    let body = top_level_fn_body(&RUN_RS[start..], "pub fn run(");
    let hook = body
        .find("notifications::install_launch_hooks()")
        .expect("the Windows run() installs the notification launch hooks");
    let window = body.find("Win32Window::new(").expect("the first window");
    assert!(
        hook < window,
        "the toast activator must be registered before the first window"
    );
}

/// iOS names no launch notification for a local notification: the launch
/// response is the first one before the app first becomes active, so the
/// delegate's `applicationDidBecomeActive:` must end that window.
#[test]
fn ios_ends_the_launch_window_when_the_app_becomes_active() {
    let body = top_level_fn_body(IOS_RS, "extern \"C\" fn app_did_become_active(");
    assert!(
        body.contains("notifications::app_became_active()"),
        "{body}"
    );
}

/// `Notify` is sent without waiting for the server's reply.
#[test]
fn every_linux_notify_is_sent_without_waiting() {
    let start = NOTIFY_LINUX_RS
        .find("fn send_notify(")
        .expect("the Notify sender");
    let end = NOTIFY_LINUX_RS[start..]
        .find("fn post_to_portal(")
        .map_or(NOTIFY_LINUX_RS.len(), |e| start + e);
    let body = &NOTIFY_LINUX_RS[start..end];
    assert!(body.contains("send_async("), "{body}");
    assert!(
        !body.contains("send_with_reply_and_block") && !body.contains("= call("),
        "Notify blocks the loop for the reply again:\n{body}"
    );
}

/// `Activity.requestPermissions` starts the system dialog's activity and must
/// run on the UI thread. `CallbackInfo::request_notification_permission` (and
/// every other runtime permission) is dispatched on `android_main`'s thread,
/// which is not it - the permission backend called it from there directly
/// ("works in theory"). It must hop to the UI thread through the Java helper.
#[test]
fn the_android_permission_dialog_is_requested_on_the_ui_thread() {
    let rust = top_level_fn_body(PERMISSION_ANDROID_RS, "fn request_permission(");
    assert!(
        !rust.contains("\"requestPermissions\""),
        "Activity.requestPermissions is called from android_main's thread:\n{rust}"
    );
    assert!(
        rust.contains("com/azul/permission/AzulPermissions") && rust.contains("\"request\""),
        "the request goes through AzulPermissions.request:\n{rust}"
    );
    assert!(
        rust.contains("find_app_class"),
        "an APK class resolves from a native thread only through the activity's class loader:\n{rust}"
    );

    let start = AZUL_PERMISSIONS_JAVA
        .find("public static void request(")
        .expect("AzulPermissions.request(Activity, String[], int) exists");
    let java = &AZUL_PERMISSIONS_JAVA[start..];
    let java = &java[..java.find("\n    }\n").unwrap_or(java.len())];
    assert!(
        java.contains("runOnUiThread"),
        "the helper runs the request on the UI thread:\n{java}"
    );
    assert!(java.contains("requestPermissions("), "{java}");
}
