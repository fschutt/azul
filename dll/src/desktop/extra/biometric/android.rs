//! Android biometric backend (JNI).
//!
//! `request` calls a Java helper `com.azul.biometric.AzulBiometric`
//! (same Rust/Java split as `AzulGeolocation` / `AzulFilePicker`):
//! `authenticate(Activity, long handle, String reason, String cancelLabel,
//! boolean allowDeviceCredential)`. The Java side drives AndroidX
//! `BiometricPrompt.authenticate(promptInfo)` and, from its
//! `AuthenticationCallback`, calls back into the `nativeOnBiometricResult`
//! symbol below with a result code — which parks a `BiometricResult` in
//! azul-layout's async result channel (`push_biometric_result`), where the
//! layout pass folds it into the `BiometricManager`.
//!
//! `probe_availability` calls `canAuthenticate(Activity) -> int`, where the
//! Java side combines `BiometricManager.canAuthenticate(BIOMETRIC_STRONG)`
//! with `PackageManager` `FEATURE_FINGERPRINT/FACE/IRIS` to report which
//! sensor (Android's `BiometricManager` reports usability, not the kind).
//!
//! Pending (non-Rust): the `AzulBiometric.java` helper itself plus the
//! manifest `USE_BIOMETRIC` permission. Until it ships, `find_class` fails
//! and `request` resolves to `Unavailable` / `probe` to `NotAvailable`.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::biometric::push_biometric_result;

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};

// One request is in flight at a time. A nonzero handle lets a late result
// from a superseded request be dropped instead of applied (the user could
// dismiss + re-tap unlock before the first prompt's callback fires).
#[cfg(target_os = "android")]
static REQUEST_HANDLE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "android")]
pub fn request(prompt: &BiometricPrompt) {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    REQUEST_HANDLE.store(handle, Ordering::Relaxed);
    let ok = attach(|env, activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/biometric/AzulBiometric").ok()?;
        let reason = env.new_string(prompt.reason.as_str()).ok()?;
        let cancel = env.new_string(prompt.cancel_label.as_str()).ok()?;
        env.call_static_method(
            class,
            "authenticate",
            "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;Z)V",
            &[
                JValue::Object(&activity),
                JValue::Long(handle as i64),
                JValue::Object(&reason),
                JValue::Object(&cancel),
                JValue::Bool(prompt.allow_device_credential as u8),
            ],
        )
        .ok()?;
        Some(())
    });
    if ok.is_none() {
        // No JNI / Java helper — resolve the round-trip rather than hang.
        REQUEST_HANDLE.store(0, Ordering::Relaxed);
        push_biometric_result(BiometricResult::Unavailable);
    }
}

#[cfg(not(target_os = "android"))]
pub fn request(prompt: &BiometricPrompt) {
    let _ = prompt;
}

#[cfg(target_os = "android")]
pub fn probe_availability() -> BiometricKind {
    attach(|env, activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/biometric/AzulBiometric").ok()?;
        let res = env
            .call_static_method(
                class,
                "canAuthenticate",
                "(Landroid/app/Activity;)I",
                &[JValue::Object(&activity)],
            )
            .ok()?;
        Some(map_kind(res.i().ok()?))
    })
    .unwrap_or(BiometricKind::NotAvailable)
}

#[cfg(not(target_os = "android"))]
pub fn probe_availability() -> BiometricKind {
    BiometricKind::NotAvailable
}

// Kind contract with the Java side: 0=NotAvailable, 1=Fingerprint,
// 2=Face, 3=Iris (the Java side derives the sensor from PackageManager
// features when canAuthenticate reports SUCCESS).
#[cfg(target_os = "android")]
fn map_kind(code: i32) -> BiometricKind {
    match code {
        1 => BiometricKind::Fingerprint,
        2 => BiometricKind::Face,
        3 => BiometricKind::Iris,
        _ => BiometricKind::NotAvailable,
    }
}

// Result contract with the Java `AuthenticationCallback`:
// 0=Authenticated, 1=Failed, 2=Cancelled, 3=FellBackToPasscode,
// 4=Unavailable, 5=Error. (Java maps onAuthenticationSucceeded with
// AUTHENTICATION_RESULT_TYPE_DEVICE_CREDENTIAL → 3, BIOMETRIC → 0;
// onAuthenticationError USER_CANCELED/NEGATIVE_BUTTON → 2,
// NO_BIOMETRICS/HW_* → 4, else → 5.)
#[cfg(target_os = "android")]
fn map_result(code: i32) -> BiometricResult {
    match code {
        0 => BiometricResult::Authenticated,
        1 => BiometricResult::Failed,
        2 => BiometricResult::Cancelled,
        3 => BiometricResult::FellBackToPasscode,
        4 => BiometricResult::Unavailable,
        _ => BiometricResult::Error,
    }
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. `None` if the VM/activity aren't
/// published or `f` short-circuits. Mirrors the geolocation / file-picker
/// backend attach sequence.
#[cfg(target_os = "android")]
fn attach<R>(
    f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Option<R>,
) -> Option<R> {
    use jni::objects::JObject;
    use jni::JavaVM;

    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut env, activity)
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives the result from `AzulBiometric`'s `AuthenticationCallback`.
/// Drops results whose `handle` doesn't match the live request (a late
/// delivery from one already superseded), otherwise parks a
/// `BiometricResult` in the async channel for the next layout pass.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_biometric_AzulBiometric_nativeOnBiometricResult(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    handle: jni::sys::jlong,
    result_code: jni::sys::jint,
) {
    if (handle as u64) != REQUEST_HANDLE.load(Ordering::Relaxed) {
        return;
    }
    REQUEST_HANDLE.store(0, Ordering::Relaxed);
    push_biometric_result(map_result(result_code));
}
