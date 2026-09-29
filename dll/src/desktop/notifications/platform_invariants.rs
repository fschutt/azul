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
