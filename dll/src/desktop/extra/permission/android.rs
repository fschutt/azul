//! Android permission backend.
//!
//! Subscribe path (`handle_event`) → `Activity.requestPermissions(
//! String[]{perm}, requestCode)` (framework API 23+, no androidx) via the
//! JNI bridge, fired only when `probe_status` reports `NotDetermined`.
//! The async grant/deny lands in `AzulActivity.onRequestPermissionsResult`
//! (Java glue, pending) which forwards to the `nativeOnPermissionResult`
//! symbol below; that maps the request code back to its `Capability` and
//! parks the result via `azul_layout::managers::permission::push_async_result`,
//! where the next layout pass folds it into the `PermissionManager`.
//!
//! Release path → no Android action for a pure permission (a permission
//! can't be un-granted; tearing down a live session is the per-feature
//! backend's job, e.g. `LocationManager.removeUpdates`).
//!
//! Probe path (`probe_status`) → `Context.checkSelfPermission(perm)`
//! returning `PERMISSION_GRANTED` (0) / `PERMISSION_DENIED` (-1), with
//! `shouldShowRequestPermissionRationale` separating a fresh
//! `NotDetermined` from a real `Denied`. Issues the real JNI calls; never
//! prompts.

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

#[cfg(target_os = "android")]
use std::collections::BTreeMap;
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(target_os = "android")]
use std::sync::Mutex;

/// requestCode → the capability it was requested for, so the async result
/// callback can recover which permission resolved.
#[cfg(target_os = "android")]
static PENDING_REQUESTS: Mutex<BTreeMap<i32, Capability>> = Mutex::new(BTreeMap::new());
#[cfg(target_os = "android")]
static REQUEST_CODE_COUNTER: AtomicU32 = AtomicU32::new(1);

/// A nonzero request code in the low 15 bits — `requestPermissions`
/// requires the code to fit in 16 bits.
#[cfg(target_os = "android")]
fn next_request_code() -> i32 {
    loop {
        let n = REQUEST_CODE_COUNTER.fetch_add(1, Ordering::Relaxed) & 0x7FFF;
        if n != 0 {
            return n as i32;
        }
    }
}

#[cfg(target_os = "android")]
pub fn handle_event(event: &PermissionDiffEvent) {
    // Only a first Subscribe prompts; Release/Reconfigure have no Android
    // action for a pure permission.
    let capability = match event {
        PermissionDiffEvent::Subscribe { capability, .. } => *capability,
        PermissionDiffEvent::Release { .. } | PermissionDiffEvent::Reconfigure { .. } => return,
    };
    let perm = match capability_to_permission(capability) {
        Some(p) => p,
        None => return,
    };
    // Only NotDetermined needs the OS dialog. Already-Granted/Denied is
    // surfaced by probe_status; a None (JNI unavailable) also means
    // "nothing to do".
    if probe_permission(perm) != Some(PermissionState::NotDetermined) {
        return;
    }

    let request_code = next_request_code();
    if let Ok(mut pending) = PENDING_REQUESTS.lock() {
        pending.insert(request_code, capability);
    }
    if request_permission(perm, request_code).is_none() {
        // JNI failed — don't leak the pending entry.
        if let Ok(mut pending) = PENDING_REQUESTS.lock() {
            pending.remove(&request_code);
        }
    }
}

#[cfg(not(target_os = "android"))]
pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
}

/// Map a `Capability` onto the single Android runtime permission that
/// gates it, or `None` for capabilities Android doesn't gate with a
/// `checkSelfPermission`-style permission (scoped-storage writes, raw
/// motion sensors, the `MediaProjection` consent dialog, or iOS-only
/// concepts). `None` → the probe reports `NotDetermined`.
#[cfg(target_os = "android")]
fn capability_to_permission(capability: Capability) -> Option<&'static str> {
    use Capability::*;
    Some(match capability {
        Camera => "android.permission.CAMERA",
        Microphone => "android.permission.RECORD_AUDIO",
        Geolocation => "android.permission.ACCESS_FINE_LOCATION",
        GeolocationBackground => "android.permission.ACCESS_BACKGROUND_LOCATION",
        PhotoLibrary => "android.permission.READ_MEDIA_IMAGES",
        Contacts => "android.permission.READ_CONTACTS",
        // Android folds reminders into the calendar permission.
        Calendars | Reminders => "android.permission.READ_CALENDAR",
        Notifications => "android.permission.POST_NOTIFICATIONS",
        Bluetooth => "android.permission.BLUETOOTH_CONNECT",
        NearbyWifi => "android.permission.NEARBY_WIFI_DEVICES",
        Biometric => "android.permission.USE_BIOMETRIC",
        // Scoped storage needs no runtime permission to *add* media;
        // accel/gyro/magnetometer are sensor-free; ScreenCapture is a
        // MediaProjection consent dialog; the rest are iOS-only.
        PhotoLibraryWrite
        | ScreenCapture
        | Motion
        | BluetoothBackground
        | LocalNetwork
        | AppTrackingTransparency => return None,
    })
}

#[cfg(target_os = "android")]
pub fn probe_status(capability: Capability) -> PermissionState {
    let perm = match capability_to_permission(capability) {
        Some(p) => p,
        None => return PermissionState::NotDetermined,
    };
    // JNI failure (VM not yet published, etc.) degrades to NotDetermined.
    probe_permission(perm).unwrap_or(PermissionState::NotDetermined)
}

#[cfg(not(target_os = "android"))]
pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. Returns `None` if the VM/activity
/// aren't published yet or `f` short-circuits. Mirrors the file picker's
/// `with_env` attach sequence.
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

/// `Context.checkSelfPermission(perm)` (+ the rationale flag on denial).
#[cfg(target_os = "android")]
fn probe_permission(perm: &str) -> Option<PermissionState> {
    use jni::objects::JValue;

    attach(|env, activity| {
        let perm_jstr = env.new_string(perm).ok()?;

        // PackageManager.PERMISSION_GRANTED == 0, PERMISSION_DENIED == -1.
        let granted = env
            .call_method(
                &activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[JValue::Object(&perm_jstr)],
            )
            .ok()?
            .i()
            .ok()?;

        if granted == 0 {
            return Some(PermissionState::Granted(PermissionQuality::Full));
        }

        // Denied. `shouldShowRequestPermissionRationale` is true only after
        // a real user denial; on a fresh install it's false. Android can't
        // distinguish "never asked" from "denied + don't ask again" (both
        // false) — we treat false as NotDetermined so the framework will
        // attempt the prompt and learn the truth.
        let rationale = env
            .call_method(
                &activity,
                "shouldShowRequestPermissionRationale",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&perm_jstr)],
            )
            .ok()?
            .z()
            .ok()?;

        Some(if rationale {
            PermissionState::Denied
        } else {
            PermissionState::NotDetermined
        })
    })
}

/// `Activity.requestPermissions(String[]{perm}, requestCode)`.
///
/// NOTE: must run on the UI thread; a hardened backend posts to the main
/// `Looper`. Called direct here (works in theory) — threading hardening is
/// a follow-up.
#[cfg(target_os = "android")]
fn request_permission(perm: &str, request_code: i32) -> Option<()> {
    use jni::objects::JValue;

    attach(|env, activity| {
        let str_cls = env.find_class("java/lang/String").ok()?;
        let perm_jstr = env.new_string(perm).ok()?;
        let arr = env.new_object_array(1, &str_cls, &perm_jstr).ok()?;
        env.call_method(
            &activity,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[JValue::Object(&arr), JValue::Int(request_code)],
        )
        .ok()?;
        Some(())
    })
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives a permission result from `AzulActivity.onRequestPermissionsResult`
/// (forwarded through `AzulPermissions.nativeOnPermissionResult`). Maps the
/// request code back to its `Capability` and parks the resolved state in
/// azul-layout's async-result channel for the next layout pass to apply.
///
/// The Java forwarding glue is pending; this is the native entry point of
/// that contract (same split as `AzulFilePicker.nativeOnResult`).
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_permission_AzulPermissions_nativeOnPermissionResult(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    request_code: jni::sys::jint,
    granted: jni::sys::jboolean,
) {
    let capability = match PENDING_REQUESTS.lock() {
        Ok(mut pending) => pending.remove(&(request_code as i32)),
        Err(_) => None,
    };
    let Some(capability) = capability else {
        return;
    };
    let state = if granted != 0 {
        PermissionState::Granted(PermissionQuality::Full)
    } else {
        PermissionState::Denied
    };
    azul_layout::managers::permission::push_async_result(capability, state);
}
