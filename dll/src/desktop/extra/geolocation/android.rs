//! Android geolocation backend.
//!
//! `handle_event` turns the manager's diff events into JNI calls on a Java
//! helper `com.azul.geolocation.AzulGeolocation` (same Rust/Java split as
//! `AzulFilePicker`): `subscribe(Activity, long handle, boolean
//! highAccuracy, int minIntervalMs)` and `release(long handle)`. The Java
//! side drives `FusedLocationProviderClient.requestLocationUpdates(...)`
//! (or framework `LocationManager`) and, for each
//! `LocationCallback.onLocationResult` fix, calls back into the
//! `nativeOnLocationFix` symbol below — which builds a `LocationFix` and
//! parks it in azul-layout's async-fix channel (`push_location_fix`),
//! where the layout pass folds it into the `GeolocationManager`.
//!
//! Pending (non-Rust): the `AzulGeolocation.java` helper itself, plus the
//! manifest `ACCESS_FINE/COARSE/BACKGROUND_LOCATION` declarations. Until it
//! ships, `find_class` fails and subscribe/release degrade to a no-op.

use azul_layout::managers::geolocation::{
    push_location_fix, GeolocationDiffEvent, LocationFix,
};

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};

// One geolocation subscription is live at a time (the manager is
// refcount-based). A nonzero handle lets `release` target the right
// Java-side listener and lets a late fix from a torn-down subscription be
// dropped instead of applied.
#[cfg(target_os = "android")]
static SUBSCRIPTION_HANDLE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "android")]
pub fn handle_event(event: &GeolocationDiffEvent) {
    match event {
        GeolocationDiffEvent::Subscribe { config } => {
            subscribe(config.high_accuracy, config.min_interval_ms);
        }
        GeolocationDiffEvent::Reconfigure { config } => {
            // Re-issue with the new options; the Java side replaces the
            // previous request under a fresh handle.
            release();
            subscribe(config.high_accuracy, config.min_interval_ms);
        }
        GeolocationDiffEvent::Release => release(),
    }
}

#[cfg(not(target_os = "android"))]
pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
}

#[cfg(target_os = "android")]
fn subscribe(high_accuracy: bool, min_interval_ms: u32) {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    SUBSCRIPTION_HANDLE.store(handle, Ordering::Relaxed);
    let ok = attach(|env, activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/geolocation/AzulGeolocation").ok()?;
        env.call_static_method(
            class,
            "subscribe",
            "(Landroid/app/Activity;JZI)V",
            &[
                JValue::Object(&activity),
                JValue::Long(handle as i64),
                JValue::Bool(high_accuracy as u8),
                JValue::Int(min_interval_ms as i32),
            ],
        )
        .ok()?;
        Some(())
    });
    if ok.is_none() {
        // JNI / Java helper unavailable — don't leave a dangling handle.
        SUBSCRIPTION_HANDLE.store(0, Ordering::Relaxed);
    }
}

#[cfg(target_os = "android")]
fn release() {
    let handle = SUBSCRIPTION_HANDLE.swap(0, Ordering::Relaxed);
    if handle == 0 {
        return;
    }
    let _ = attach(|env, _activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/geolocation/AzulGeolocation").ok()?;
        env.call_static_method(class, "release", "(J)V", &[JValue::Long(handle as i64)])
            .ok()?;
        Some(())
    });
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. `None` if the VM/activity aren't
/// published or `f` short-circuits. Mirrors the file picker / permission
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

/// Receives a fix from `AzulGeolocation`'s `LocationCallback`. Drops fixes
/// whose `handle` doesn't match the live subscription (a late delivery from
/// one we already released), otherwise builds a `LocationFix` and parks it
/// in the async-fix channel for the next layout pass to apply.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_geolocation_AzulGeolocation_nativeOnLocationFix(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    handle: jni::sys::jlong,
    latitude_deg: jni::sys::jdouble,
    longitude_deg: jni::sys::jdouble,
    accuracy_m: jni::sys::jfloat,
    altitude_m: jni::sys::jfloat,
    altitude_accuracy_m: jni::sys::jfloat,
    heading_deg: jni::sys::jfloat,
    speed_mps: jni::sys::jfloat,
    timestamp_ms: jni::sys::jlong,
) {
    if (handle as u64) != SUBSCRIPTION_HANDLE.load(Ordering::Relaxed) {
        return;
    }
    push_location_fix(LocationFix {
        latitude_deg,
        longitude_deg,
        accuracy_m,
        altitude_m,
        altitude_accuracy_m,
        heading_deg,
        speed_mps,
        timestamp_ms: timestamp_ms as u64,
    });
}
