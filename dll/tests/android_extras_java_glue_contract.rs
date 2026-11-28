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
    assert!(error.contains("nativeOnBiometricResult(handle, mapError(code))"));
}

/// One cause per biometric result (the model of 2026-11-28): the user closing the sheet - back, outside, the negative
/// button, a timeout - is Cancelled and NEVER a fall back to the passcode (the old mapping answered FellBackToPasscode
/// for the negative button and Failed for everything it did not know, lockouts included); too many tries are
/// LockedOut; nothing enrolled is NotEnrolled; a passcode is reported only when the system says it was USED.
///
/// Mutation: `case ERR_NEGATIVE_BUTTON` returning `allowFallback ? RESULT_FALLBACK : RESULT_CANCELLED`.
#[test]
fn a_biometric_result_names_one_cause() {
    let src = java_code("scripts/android/AzulBiometric.java");
    let map = method_body(&src, "private static int mapError(");
    let arm = |label: &str| -> String {
        let at = map.find(label).unwrap_or_else(|| panic!("no `{label}` in mapError"));
        let tail = &map[at..];
        let end = tail.find("return ").unwrap();
        tail[end..].split(';').next().unwrap().trim().to_string()
    };
    for cancel in ["ERR_USER_CANCELED", "ERR_CANCELED", "ERR_NEGATIVE_BUTTON", "ERR_TIMEOUT"] {
        assert_eq!(arm(&format!("case {cancel}:")), "return RESULT_CANCELLED", "{cancel} is the user closing the sheet");
    }
    assert_eq!(arm("case ERR_LOCKOUT:"), "return RESULT_LOCKED_OUT");
    assert_eq!(arm("case ERR_LOCKOUT_PERMANENT:"), "return RESULT_LOCKED_OUT");
    assert_eq!(arm("case ERR_NO_BIOMETRICS:"), "return RESULT_NOT_ENROLLED");
    assert_eq!(arm("case ERR_NO_DEVICE_CREDENTIAL:"), "return RESULT_NOT_ENROLLED");
    assert_eq!(arm("case ERR_HW_NOT_PRESENT:"), "return RESULT_UNAVAILABLE");
    assert!(!map.contains("RESULT_FALLBACK"), "the passcode is never inferred from an error code");
    let succeeded = method_body(&src, "public void onAuthenticationSucceeded(");
    assert!(
        succeeded.contains("AUTHENTICATION_RESULT_TYPE_DEVICE_CREDENTIAL") && succeeded.contains("RESULT_FALLBACK"),
        "FellBackToPasscode comes from the system's own statement of what the user used"
    );
    assert!(src.contains("RESULT_LOCKED_OUT = 6") && src.contains("RESULT_NOT_ENROLLED = 7"), "the codes the Rust side maps");
}

/// The keyring model on Android (briefs/mobile_capabilities_ledger_2026-11-28.md, MODEL): ONE Keystore key PER ENTRY
/// (the old master key made the first store decide `setUserAuthenticationRequired` for every entry, and a protected
/// read never went through the BiometricPrompt: the key was bound to the sensor on paper only).
///
/// Mutation: a constant `KEY_ALIAS = "azul_keyring_master"` used for every entry.
#[test]
fn every_keyring_entry_has_a_keystore_key_of_its_own() {
    let src = java_code("scripts/android/AzulKeyring.java");
    assert!(!src.contains("azul_keyring_master") && !src.contains("KEY_ALIAS"), "no master key for all entries");
    assert!(src.contains("ALIAS_PREFIX + id"), "the alias is derived from the entry");
    let id = method_body(&src, "private static String idOf(");
    assert!(id.contains("SHA-256"), "the id of an entry is a digest of its name: any characters, any length");
    let pair = method_body(&src, "private static PublicKey generatePair(");
    assert!(
        pair.contains("setUserAuthenticationRequired(true)") && pair.contains("KeyProperties.PURPOSE_DECRYPT)"),
        "the private half of a protected entry's key pair requires the user check (the public half never does)"
    );
    assert!(
        pair.contains("setInvalidatedByBiometricEnrollment(level == LEVEL_BIOMETRIC)"),
        "only the Biometric level is invalidated by a change of the enrolled set"
    );
    assert!(
        pair.contains("AUTH_BIOMETRIC_STRONG") && pair.contains("AUTH_DEVICE_CREDENTIAL") && pair.contains("setUserAuthenticationParameters(0,"),
        "API 30+: per-use authentication (timeout 0) with the authenticators of the level"
    );
    let plain = method_body(&src, "private static SecretKey generate(");
    assert!(
        !plain.contains("setUserAuthenticationRequired"),
        "the unprotected level never asks the user: its key has no user check"
    );
}

/// The secret of a protected entry is released by the secure hardware only after the sheet: the cipher of the entry's
/// key is the sheet's CryptoObject and the secret is decrypted with the cipher the SUCCESSFUL sheet hands back. A quiet
/// read of a protected item answers Denied (Invalidated when its key is gone) and never decrypts.
///
/// Mutation: `unlock` decrypting with `o.cipher` straight after the sheet (a boolean 'the finger matched').
#[test]
fn a_protected_secret_is_released_only_by_the_cipher_of_the_sheet() {
    let src = java_code("scripts/android/AzulKeyring.java");
    let sheet = method_body(&src, "private static void sheet(");
    assert!(sheet.contains("new BiometricPrompt.CryptoObject(cipher)"), "the sheet carries the cipher");
    assert!(sheet.contains("result.getCryptoObject()") && sheet.contains("crypto.getCipher()"), "the authenticated cipher comes back");
    let unlock = method_body(&src, "public static void unlock(");
    assert!(unlock.contains("sheet(activity, o.cipher"), "unlock opens the sheet with the cipher of the entry's private key");
    assert!(unlock.contains("unlocked.doFinal(o.wrapped)"), "and unwraps the data key with the cipher the sheet unlocked");
    let after_sheet = &unlock[unlock.find("sheet(activity, o.cipher").unwrap()..];
    assert!(!after_sheet.contains("o.cipher.doFinal"), "never with the locked cipher");
    let quiet = method_body(&src, "public static String[] quiet(");
    assert!(quiet.contains("level > LEVEL_DEVICE_UNLOCKED") && quiet.contains("WOULD_PROMPT") && quiet.contains("INVALIDATED"));
    assert!(
        quiet.find("level > LEVEL_DEVICE_UNLOCKED").unwrap() < quiet.find("open(activity, id)").unwrap(),
        "the quiet read answers for a protected item BEFORE it opens a cipher"
    );
    let get = method_body(&src, "public static void get(");
    assert!(get.contains("quiet(activity, OP_GET, key, null)"), "the async read is the quiet one: it never shows a sheet");
    // Storing needs no sheet: the data key is wrapped with the PUBLIC half of the entry's key pair.
    let store = method_body(&src, "public static void store(");
    assert!(!store.contains("sheet("), "a store shows no sheet");
    assert!(store.contains("wrap.init(Cipher.ENCRYPT_MODE, pub, OAEP)"), "the data key is wrapped with the public key");
    assert!(store.contains("supportOf(activity, protection)"), "NotEnrolled / Unavailable are answered, never a weaker store");
    assert!(store.contains("deleteKey(id)"), "a failed store leaves no key behind");
}

/// The result codes of the model, and what makes a protected item Invalidated rather than NotFound: the meta record
/// (level, made when) is readable without a sheet and outlives the key, and a key the Keystore invalidated is
/// KeyPermanentlyInvalidatedException at the cipher init.
///
/// Mutation: `open` answering NOT_FOUND when the key is missing but the meta record is there.
#[test]
fn a_protected_item_whose_key_is_gone_is_invalidated_not_missing() {
    let src = java_code("scripts/android/AzulKeyring.java");
    for (name, value) in [
        ("CANCELLED", 7),
        ("FAILED", 8),
        ("LOCKED_OUT", 9),
        ("NOT_ENROLLED", 10),
        ("INVALIDATED", 11),
    ] {
        assert!(src.contains(&format!("{name} = {value};")), "result code {name} = {value}");
    }
    let open = method_body(&src, "private static Opened open(");
    assert!(open.contains("catch (KeyPermanentlyInvalidatedException"), "an invalidated key at the cipher init");
    assert!(open.contains("INVALIDATED"), "...is Invalidated");
    assert!(open.contains("levelOf(ctx, id) >= 0 ? INVALIDATED : NOT_FOUND"), "a meta record without its item is Invalidated");
    let keep = method_body(&src, "private static void keep(");
    assert!(keep.contains("\"m.\" + id") && keep.contains("level=") && keep.contains("b."), "the meta record is written with the blob");
    let quiet = method_body(&src, "public static String[] quiet(");
    assert!(
        quiet.contains("remove(\"b.\" + id)") && quiet.contains("remove(\"w.\" + id)") && quiet.contains("remove(\"m.\" + id)") && quiet.contains("deleteKey(id)"),
        "a delete removes blob, wrapped key, meta and key"
    );
    assert!(src.contains("WOULD_PROMPT = 12;"), "a read without the user of a protected item answers WouldPrompt");
    assert!(src.contains("FEATURE_STRONGBOX_KEYSTORE") && src.contains("catch (StrongBoxUnavailableException"), "StrongBox when present, dropped when it cannot");
    // the quiet probes the Rust side asks before it decides between a blocking answer and WouldPrompt
    assert!(src.contains("public static int needsPrompt(Activity activity, String key)"));
    assert!(src.contains("public static int support(Activity activity, int level)"));
}

/// The copy of a picked document (`ContentResolver.openInputStream` and a loop of reads: as slow as its provider, minutes for a video
/// on a cloud drive) must not run on the thread `onActivityResult` is delivered on, the UI thread. Since ledger WP-F the Java class is
/// ONLY the hand-over of the result to a native; the import is Rust's, on a worker thread, into the app's inbox.
///
/// Mutation: a copy (`openInputStream`, `FileOutputStream`, the cache folder) back in `AzulFilePicker.java`, or `import_picked`
/// called without its worker.
#[test]
fn a_picked_document_is_copied_off_the_ui_thread() {
    let java = java_code("scripts/android/AzulFilePicker.java");
    let proxy = method_body(&java, "public static boolean onActivityResultProxy(");
    for copy in ["openInputStream", "FileOutputStream", "getCacheDir", "uriToCachedFile", "new Thread(", "static final ConcurrentHashMap", "AtomicInteger"] {
        assert!(!java.contains(copy), "the Java proxy copies nothing, starts nothing and keeps no state: `{copy}`");
    }
    assert!(proxy.contains("nativeOnActivityResult("), "the result goes to the engine");
    assert!(proxy.contains("catch (Throwable"), "a result the engine cannot take must not kill the app");

    // the import: a worker thread, started before the UI thread returns
    let opened = rust_code("dll/src/desktop/extra/open_files/android.rs");
    let at = opened.find("fnimport_picked(").expect("import_picked");
    // the text has no line breaks: the next function after it is `run`
    let import = &opened[at..opened[at..].find("fnrun(").map_or(opened.len(), |e| at + e)];
    assert!(import.contains("std::thread::Builder::new()") && import.contains(".spawn("), "the files are copied on a worker thread");
    let native = rust_code("dll/src/desktop/extra/file_picker/android.rs");
    assert!(native.contains("import_picked("), "the picker's result is imported by the shared worker");
    assert!(
        !native.contains("copy_in(") && !native.contains("openFileDescriptor") && !native.contains("fs::copy"),
        "the native itself copies nothing: it runs on the UI thread"
    );
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
