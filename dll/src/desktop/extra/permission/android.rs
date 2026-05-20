//! Android permission backend.
//!
//! Subscribe path (`handle_event`) → `ActivityCompat.requestPermissions(
//! activity, permissions, requestCode)` via the JNI bridge (same pattern
//! as `AzulFilePicker`). Dangerous-level permissions (`CAMERA`,
//! `ACCESS_FINE_LOCATION`, `READ_MEDIA_IMAGES`, …) need both a manifest
//! declaration *and* the runtime prompt; special permissions
//! (`MANAGE_EXTERNAL_STORAGE`, `SYSTEM_ALERT_WINDOW`) need a settings-app
//! intent. Still a no-op below — the async prompt lands later.
//!
//! Release path → drop the matching listener (`LocationManager.removeUpdates`).
//!
//! Probe path (`probe_status`) → `Context.checkSelfPermission(perm)`
//! (framework API 23+, no androidx needed) returning `PERMISSION_GRANTED`
//! (0) / `PERMISSION_DENIED` (-1), with `shouldShowRequestPermissionRationale`
//! separating a fresh `NotDetermined` from a real `Denied`. This is
//! implemented and issues the real JNI calls; it never prompts.

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): JNI call into ActivityCompat.requestPermissions(...).
    // The synchronous read path (probe_status) is wired below.
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

/// Attach to the published JavaVM and read `checkSelfPermission` (+ the
/// rationale flag on denial). Returns `None` if the VM/activity aren't
/// published yet or any JNI call errors. Mirrors the file picker's
/// `with_env` attach sequence.
#[cfg(target_os = "android")]
fn probe_permission(perm: &str) -> Option<PermissionState> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };

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
        return Some(PermissionState::Granted {
            quality: PermissionQuality::Full,
        });
    }

    // Denied. `shouldShowRequestPermissionRationale` is true only after a
    // real user denial; on a fresh install (never prompted) it's false.
    // Android can't distinguish "never asked" from "denied + don't ask
    // again" (both false) — we treat false as NotDetermined so the
    // framework will attempt the prompt and learn the truth.
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
}
