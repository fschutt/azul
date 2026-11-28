//! The Java helpers of the platform extras (web view, biometric prompt) must not turn what a device does into the
//! end of the app or into a wrong answer, pinned in source: the JVM cannot run where the tests run, and these are
//! what the review of 2026-11-26 found in the glue (ledger rows O-09, O-10 of
//! briefs/mobile_review_android_2026-11-26.md):
//!
//! * a device without a WebView provider: `new WebView` throws there (`WebViewFactory.MissingWebViewPackageException`,
//!   a RuntimeException), and an exception nobody catches on the UI thread is the death of the process;
//! * a rejected finger is not the end of a biometric attempt: "onAuthenticationFailed: Called when a biometric
//!   (e.g. fingerprint, face, etc.) is presented but not recognized as belonging to the user" - the prompt stays up,
//!   and the terminal answer comes from onAuthenticationSucceeded or onAuthenticationError. The Rust side resolves a
//!   request at its FIRST result, so reporting the rejection made the later success a dropped call;
//! * a location request whose preferred provider is switched off falls back to the OTHER provider (the code asked for
//!   GPS again when GPS was the one that was off: no fix ever came);
//! * a picked document is copied off the UI thread: `onActivityResult` runs on it, and a cloud provider streams the
//!   file while it is read (minutes for a video): "input dispatching timed out" after five seconds, the ANR dialog.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_extras_java_glue_contract.rs -o /tmp/t && /tmp/t

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
        .unwrap_or_else(|| panic!("no `{signature}` in the Java source"));
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

/// The prompt stays up after a rejected finger, and the Rust side resolves the request at the first result it
/// gets: the rejection must not be reported (the later `onAuthenticationSucceeded` would find no request).
///
/// Mutation: `nativeOnBiometricResult(handle, RESULT_FAILED)` in `onAuthenticationFailed` (the state of 0f3e1eb093).
#[test]
fn a_rejected_finger_does_not_end_the_biometric_attempt() {
    let src = java_code("scripts/android/AzulBiometric.java");
    let body = method_body(&src, "public void onAuthenticationFailed()");
    assert!(
        !body.contains("nativeOnBiometricResult"),
        "onAuthenticationFailed must not report a result: the prompt stays up and the request is resolved at its first result"
    );
    // the terminal answers still reach the app
    let succeeded = method_body(&src, "public void onAuthenticationSucceeded(");
    assert!(succeeded.contains("RESULT_AUTHENTICATED"));
    let error = method_body(&src, "public void onAuthenticationError(");
    assert!(error.contains("nativeOnBiometricResult(handle, mapped)"));
}

/// The copy of a picked document into the app's cache (`uriToCachedFile`: `ContentResolver.openInputStream`
/// and a loop of reads) must not run on the thread `onActivityResult` is delivered on, the UI thread.
///
/// Mutation: `uriToCachedFile` called straight from `onActivityResultProxy` (the state of 0f3e1eb093).
#[test]
fn a_picked_document_is_copied_off_the_ui_thread() {
    let src = java_code("scripts/android/AzulFilePicker.java");
    let proxy = method_body(&src, "public static boolean onActivityResultProxy(");
    assert!(
        !proxy.contains("uriToCachedFile("),
        "onActivityResultProxy runs on the UI thread: the copy belongs on a worker thread"
    );
    assert!(proxy.contains("new Thread("), "the result is delivered from a worker thread");
    // whatever the worker does, it answers the request even when the copy throws
    let worker = method_body(&src, "private static void deliver(");
    assert!(worker.contains("uriToCachedFile("));
    assert!(worker.contains("catch (Throwable"), "an error on the worker must still answer the request");
    assert!(worker.contains("nativeOnResult("));
}

/// `LocationManager.requestLocationUpdates` on a provider the user switched off delivers nothing. The fallback when
/// the preferred provider is off must be the OTHER one: it was `fine ? GPS : NETWORK`, which is GPS again for the
/// (usual) case that GPS was preferred and is off. GPS needs the fine permission; the network provider works with
/// either.
///
/// Mutation: the fallback `provider = fine ? LocationManager.GPS_PROVIDER : LocationManager.NETWORK_PROVIDER`.
#[test]
fn a_switched_off_location_provider_falls_back_to_the_other_one() {
    let src = java_code("scripts/android/AzulGeolocation.java");
    let body = method_body(&src, "public static synchronized void subscribe(");
    assert!(
        !body.contains("provider = fine ?"),
        "the fallback must not be a function of the permission alone: it picked the switched-off provider again"
    );
    assert!(
        body.contains("isProviderEnabled(other)"),
        "the other provider is asked whether it is on before it is used"
    );
}

/// Rust source without its comments and without whitespace (rustfmt breaks the calls over lines).
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// A permission dialog comes from a person, once: a denial is REMEMBERED by the app (Android reads "never asked" and
/// "denied, don't ask again" the same, so a denial forgotten at the next start was asked about again at every launch)
/// and the diff-driven path (a probe node in the DOM) never asks again; only a person's explicit request does, once,
/// while the system still offers a rationale (the rule is `android_table::may_show_dialog`, tested there).
///
/// Mutation: `handle_event` calling `request(capability)` (the state of 1dc25e2a8a: a rebuild that added the probe node
/// asked again after every denial).
#[test]
fn a_denied_permission_is_remembered_and_only_a_person_asks_again() {
    let java = java_code("scripts/android/AzulPermissions.java");
    let proxy = method_body(&java, "public static boolean onRequestPermissionsResultProxy(");
    assert!(proxy.contains("remember(activity, permissions, grantResults)"), "the answer of every request is remembered");
    let remember = method_body(&java, "private static void remember(");
    assert!(
        remember.contains("\"denied.\" + permissions[i]") && remember.contains("grantResults[i] != 0"),
        "a refusal is kept per permission, a later grant clears it"
    );
    assert!(java.contains("public static boolean wasDenied(Activity activity, String permission)"));

    let rust = rust_code("dll/src/desktop/extra/permission/android.rs");
    let handle = method_body(&rust, "pubfnhandle_event(event:&PermissionDiffEvent){");
    assert!(handle.contains("request_with(capability,false)"), "the diff never asks again after a denial");
    let request = method_body(&rust, "pubfnrequest(capability:Capability)->bool{");
    assert!(request.contains("request_with(capability,true)"), "a person's request may ask once more");
    let with = method_body(&rust, "fnrequest_with(capability:Capability,reask:bool)->bool{");
    assert!(with.contains("may_show_dialog(probe.state,probe.rationale,reask)"), "the one rule decides");
    assert!(rust.contains("\"wasDenied\""), "the quiet probe reads what the app remembered");
}
