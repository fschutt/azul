//! Android permission backend.
//!
//! Subscribe path (`handle_event`) → `AzulPermissions.request(activity,
//! String[]{perm}, requestCode)`, which runs `Activity.requestPermissions`
//! (framework API 23+, no androidx) on the UI thread, fired only when
//! `probe_status` reports `NotDetermined`.
//! The async grant/deny lands in `AzulActivity.onRequestPermissionsResult`
//! (Java glue, pending) which forwards to the `nativeOnPermissionResult`
//! symbol below; that maps the request code back to its `Capability` and
//! parks the result in the app's `AppServices::permission` queue (reached
//! through `process_app`: a JNI entry point carries no context), where the next
//! layout pass folds it into the `PermissionManager`.
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

#[cfg(target_os = "android")]
use std::collections::BTreeMap;
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(target_os = "android")]
use std::sync::Mutex;

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

#[cfg(target_os = "android")]
use super::android_table::{needs, state_of_check, state_of_result, Needs};

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
    let _ = request(capability);
}

/// Show the runtime-permission dialog for `capability` unless it is granted already or has none at this
/// API level. Returns whether a dialog was requested; its answer arrives through
/// `nativeOnPermissionResult`. Also the entry point of `CallbackInfo::request_notification_permission`
/// (POST_NOTIFICATIONS, API 33 and up).
///
/// What is asked depends on the API level (`android_table::needs`): location as the fine + coarse pair in
/// ONE request, notifications only from API 33. A capability the user denied once is asked again: Android
/// shows the dialog a second time and answers a third request at once with a denial, which is the
/// documented flow ("rationale: explain, then ask again"); the shell used to give up at the first denial.
#[cfg(target_os = "android")]
pub fn request(capability: Capability) -> bool {
    let Some(sdk) = sdk_int() else {
        return false;
    };
    let perms = match needs(capability, sdk) {
        Needs::Ask(perms) => perms,
        Needs::NothingToAsk | Needs::NotAPermission => return false,
    };
    // Granted already (or the JNI failed): nothing to ask.
    match probe_permissions(capability, perms) {
        Some(PermissionState::NotDetermined | PermissionState::Denied) => {}
        _ => return false,
    }

    let request_code = next_request_code();
    if let Ok(mut pending) = PENDING_REQUESTS.lock() {
        pending.insert(request_code, capability);
    }
    if request_permissions(perms, request_code).is_none() {
        // JNI failed — don't leak the pending entry.
        if let Ok(mut pending) = PENDING_REQUESTS.lock() {
            pending.remove(&request_code);
        }
        return false;
    }
    true
}

#[cfg(not(target_os = "android"))]
pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
}

#[cfg(target_os = "android")]
pub fn probe_status(capability: Capability) -> PermissionState {
    let Some(sdk) = sdk_int() else {
        return PermissionState::NotDetermined;
    };
    match needs(capability, sdk) {
        Needs::Ask(perms) => {
            // JNI failure (VM not yet published, etc.) degrades to NotDetermined.
            probe_permissions(capability, perms).unwrap_or(PermissionState::NotDetermined)
        }
        // Nothing to ask at this API level: an install-time permission, usable. Except notifications
        // below API 33, which have no runtime permission but a switch the user can turn off
        // (Settings > Apps > Notifications): `NotificationManager.areNotificationsEnabled()`.
        Needs::NothingToAsk => {
            if capability == Capability::Notifications && notifications_switched_off() {
                PermissionState::Denied
            } else {
                PermissionState::Granted(PermissionQuality::Full)
            }
        }
        Needs::NotAPermission => PermissionState::NotDetermined,
    }
}

/// Whether the user turned the app's notifications off in the system settings
/// (`AzulNotifications.areEnabled(activity)`); `false` when the answer cannot be had.
#[cfg(target_os = "android")]
fn notifications_switched_off() -> bool {
    use jni::objects::JValue;

    attach(|env, activity| {
        let helper = crate::desktop::extra::find_app_class(
            env,
            &activity,
            "com/azul/notify/AzulNotifications",
        )?;
        let enabled = env
            .call_static_method(
                &helper,
                "areEnabled",
                "(Landroid/content/Context;)Z",
                &[JValue::Object(&activity)],
            )
            .and_then(|v| v.z());
        match enabled {
            Ok(on) => Some(!on),
            Err(_) => {
                let _ = env.exception_clear();
                None
            }
        }
    })
    .unwrap_or(false)
}

#[cfg(not(target_os = "android"))]
pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}

/// `Build.VERSION.SDK_INT` of the device: what `android_table::needs` keys on.
#[cfg(target_os = "android")]
fn sdk_int() -> Option<u32> {
    attach(|env, _activity| {
        let sdk = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
            .ok()?
            .i()
            .ok()?;
        u32::try_from(sdk).ok()
    })
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. Returns `None` if the VM/activity
/// aren't published yet or `f` short-circuits. Mirrors the file picker's
/// `with_env` attach sequence.
#[cfg(target_os = "android")]
fn attach<R>(f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Option<R>) -> Option<R> {
    use jni::{objects::JObject, JavaVM};

    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // Released when this helper returns: this thread never returns to Java (see `LocalFrame`).
    let _frame = crate::desktop::extra::LocalFrame::push(&env, crate::desktop::extra::LocalFrame::DEFAULT_CAPACITY);
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut env, activity)
}

/// `Context.checkSelfPermission` of each permission of a request (+ the rationale flag on a denial), as the
/// state `android_table::state_of_check` makes of them.
#[cfg(target_os = "android")]
fn probe_permissions(capability: Capability, perms: &[&str]) -> Option<PermissionState> {
    use jni::objects::JValue;

    attach(|env, activity| {
        let mut granted = Vec::with_capacity(perms.len());
        let mut rationale = false;
        for perm in perms {
            let perm_jstr = env.new_string(*perm).ok()?;

            // PackageManager.PERMISSION_GRANTED == 0, PERMISSION_DENIED == -1.
            let check = env
                .call_method(
                    &activity,
                    "checkSelfPermission",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&perm_jstr)],
                )
                .ok()?
                .i()
                .ok()?;
            granted.push(check == 0);

            if check != 0 {
                // `shouldShowRequestPermissionRationale` is true only after a real user denial; on a
                // fresh install it's false. Android can't distinguish "never asked" from "denied +
                // don't ask again" (both false) — false reads as NotDetermined so the framework will
                // attempt the prompt and learn the truth.
                rationale |= env
                    .call_method(
                        &activity,
                        "shouldShowRequestPermissionRationale",
                        "(Ljava/lang/String;)Z",
                        &[JValue::Object(&perm_jstr)],
                    )
                    .ok()?
                    .z()
                    .ok()?;
            }
        }
        Some(state_of_check(capability, &granted, rationale))
    })
}

/// `AzulPermissions.request(activity, String[] perms, requestCode)`: the helper runs
/// `Activity.requestPermissions` on the UI thread (`runOnUiThread`), ONE request for all of `perms`. This
/// runs on `android_main`'s thread, and the framework starts the permission dialog's activity from the UI
/// thread only.
///
/// The helper is an APK class, which a native-attached thread resolves only
/// through the ACTIVITY's class loader (`find_app_class`). It is the same
/// class that forwards the answer (`nativeOnPermissionResult`), so an APK
/// without it could never report one - there is nothing to fall back to.
#[cfg(target_os = "android")]
fn request_permissions(perms: &[&str], request_code: i32) -> Option<()> {
    use jni::objects::JValue;

    attach(|env, activity| {
        let helper = crate::desktop::extra::find_app_class(
            env,
            &activity,
            "com/azul/permission/AzulPermissions",
        )?;
        let str_cls = env.find_class("java/lang/String").ok()?;
        let first = env.new_string(*perms.first()?).ok()?;
        let arr = env
            .new_object_array(i32::try_from(perms.len()).ok()?, &str_cls, &first)
            .ok()?;
        for (i, perm) in perms.iter().enumerate() {
            let jstr = env.new_string(*perm).ok()?;
            env.set_object_array_element(&arr, i32::try_from(i).ok()?, &jstr)
                .ok()?;
        }
        let called = env.call_static_method(
            &helper,
            "request",
            "(Landroid/app/Activity;[Ljava/lang/String;I)V",
            &[
                JValue::Object(&activity),
                JValue::Object(&arr),
                JValue::Int(request_code),
            ],
        );
        if called.is_err() {
            // A pending exception aborts the process at the next JNI call.
            let _ = env.exception_clear();
            return None;
        }
        Some(())
    })
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives a permission result from `AzulActivity.onRequestPermissionsResult`
/// (forwarded through `AzulPermissions.nativeOnPermissionResult`): the request code, whether EVERY permission
/// of the request was granted, and whether ANY was (a user may grant the approximate location only).
/// Maps the request code back to its `Capability` and parks the resolved state in the app's permission
/// queue for the next layout pass to apply, and wakes the loop: a backgrounded loop would not see it
/// until the next event.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_permission_AzulPermissions_nativeOnPermissionResult(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    request_code: jni::sys::jint,
    all_granted: jni::sys::jboolean,
    any_granted: jni::sys::jboolean,
) {
    let capability = match PENDING_REQUESTS.lock() {
        Ok(mut pending) => pending.remove(&(request_code as i32)),
        Err(_) => None,
    };
    let Some(capability) = capability else {
        return;
    };
    let state = state_of_result(capability, all_granted != 0, any_granted != 0);
    // The JNI entry has no context: the App's services are where `App::run` put them.
    if let Some(services) = crate::desktop::process_app::services() {
        services.permission.push((capability, state));
    }
    crate::desktop::shell2::android::wake_event_loop();
}
