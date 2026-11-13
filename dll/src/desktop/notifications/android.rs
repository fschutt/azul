//! Android native notifications  -  `NotificationManager`, through
//! `scripts/android/AzulNotifications.java`.
//!
//! The split follows `AzulMediaSession` / `AzulSensors`: Rust decides what to
//! show and calls ONE static Java method per operation; the Java helper owns
//! what only Java does well - the framework `Notification.Builder`, the
//! channel (required from API 26, or nothing is shown), the `PendingIntent`s
//! back into the app, and the manifest `BroadcastReceiver` for dismissals.
//!
//! # How a tap comes back
//!
//! * **Running app:** the tap's `PendingIntent.getActivity` reaches the `singleTop`
//!   `AzulActivity.onNewIntent`, which hands the extras to `AzulNotifications.onIntent`,
//!   which calls [`Java_com_azul_notify_AzulNotifications_nativeOnNotificationEvent`].
//! * **Cold start:** the same intent starts the activity; `onCreate` forwards it with
//!   `launchedApp = true`. The event is queued before `android_main` even runs, and routed -
//!   to the app-level handler, since this process posted nothing - by the loop's first pump.
//! * **Swipe-away:** the delete intent reaches the manifest receiver, which may run in a process
//!   without the native library; the Java side then keeps the event in `SharedPreferences` and
//!   forwards it at the next start.
//!
//! Every intent carries the app's notification id, the action and the payload
//! (`wire::android_event` turns them into the event), and every notification
//! gets its own `PendingIntent` request codes (`wire::android_request_code`):
//! intents that differ only in extras would otherwise be the same one.
//!
//! # Permission
//!
//! Android 13+ gates posting behind the runtime permission `POST_NOTIFICATIONS`
//! (declared by the manifest template, OFF for a fresh install); posting
//! without it is dropped by the system WITHOUT an error. So a post first asks
//! the helper for the state: never asked -> the dialog is requested (the
//! existing permission backend, whose answer arrives through
//! `AzulActivity.onRequestPermissionsResult`) and this post reports `Failed`
//! with that reason; denied or switched off -> `Failed`.
//! `CallbackInfo::request_notification_permission` asks up front instead.

use azul_core::notification::{Notification, NotificationSound};
use azul_layout::managers::{
    notification::{queue_notification_event, wire},
    permission::{push_async_result, Capability, PermissionQuality, PermissionState},
};
use jni::objects::{JClass, JObject, JObjectArray, JString, JValue};

/// The helper class, in the slashed form `find_app_class` takes.
const HELPER: &str = "com/azul/notify/AzulNotifications";
/// The user-visible name of azul's channel (Settings > Apps > Notifications).
const CHANNEL_NAME: &str = "Notifications";

/// What `AzulNotifications.permissionState` answers.
const STATE_NOT_ASKED: i32 = 0;
const STATE_DENIED: i32 = 1;
const STATE_GRANTED: i32 = 2;

/// Attach to the VM, resolve the helper class through the ACTIVITY's loader
/// (this runs on a Rust thread with no Java frame, where a bare `find_class`
/// never sees an APK class) and run `f`. `Err` says why not.
fn with_helper<R, F>(f: F) -> Result<R, String>
where
    F: for<'a> FnOnce(
        &mut jni::JNIEnv<'a>,
        &JObject<'a>,
        &JClass<'a>,
    ) -> Result<R, jni::errors::Error>,
{
    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return Err("the activity is not published yet (android_main has not started)".into());
    }
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
        .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e:?}"))?;
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let class = crate::desktop::extra::find_app_class(&mut env, &activity, HELPER).ok_or_else(
        || "AzulNotifications is not in this APK (scripts/android/AzulNotifications.java)".to_string(),
    )?;
    f(&mut env, &activity, &class).map_err(|e| {
        // A pending exception left unhandled aborts the process at the next
        // JNI boundary, which would surface far from here.
        let _ = env.exception_clear();
        format!("AzulNotifications: {e:?}")
    })
}

/// `AzulNotifications.permissionState(activity)`.
fn permission_code() -> Result<i32, String> {
    with_helper(|env, activity, class| {
        env.call_static_method(
            class,
            "permissionState",
            "(Landroid/app/Activity;)I",
            &[JValue::Object(activity)],
        )?
        .i()
    })
}

/// A `String[]` of `items`.
fn string_array<'a>(
    env: &mut jni::JNIEnv<'a>,
    items: &[&str],
) -> Result<JObjectArray<'a>, jni::errors::Error> {
    let len = i32::try_from(items.len()).unwrap_or(0);
    let array = env.new_object_array(len, "java/lang/String", JObject::null())?;
    for (i, item) in items.iter().enumerate() {
        let s = env.new_string(*item)?;
        env.set_object_array_element(&array, i32::try_from(i).unwrap_or(0), &s)?;
        env.delete_local_ref(s)?;
    }
    Ok(array)
}

/// `(available, reason)` for `PlatformCapability::notifications()`.
pub(super) fn probe() -> (bool, String) {
    match permission_code() {
        Err(e) => (false, e),
        Ok(STATE_DENIED) => (
            false,
            "notifications are turned off for this app, or POST_NOTIFICATIONS was denied \
             (Settings > Apps > Notifications)"
                .to_string(),
        ),
        Ok(STATE_NOT_ASKED) => (
            true,
            "Android 13+: the POST_NOTIFICATIONS permission is not granted yet - ask with \
             CallbackInfo::request_notification_permission (the first post asks too, and fails)"
                .to_string(),
        ),
        Ok(_) => (true, String::new()),
    }
}

/// `CallbackInfo::request_notification_permission`. The answer reaches the
/// permission manager: at once when there is nothing to ask, from
/// `onRequestPermissionsResult` when the dialog was shown.
pub(super) fn request_permission() {
    match permission_code() {
        Ok(STATE_GRANTED) => push_async_result(
            Capability::Notifications,
            PermissionState::Granted(PermissionQuality::Full),
        ),
        Ok(STATE_DENIED) => push_async_result(Capability::Notifications, PermissionState::Denied),
        Ok(_) => {
            // Never asked (API 33+): the dialog. `Requested` keeps the
            // capability pump draining until the answer arrives.
            if crate::desktop::extra::permission::android::request(Capability::Notifications) {
                push_async_result(Capability::Notifications, PermissionState::Requested);
            } else {
                crate::plog_warn!(
                    "[notifications] the POST_NOTIFICATIONS dialog could not be requested"
                );
            }
        }
        Err(e) => {
            crate::plog_warn!("[notifications] cannot ask for the permission: {e}");
            push_async_result(Capability::Notifications, PermissionState::Restricted);
        }
    }
}

pub(super) struct PlatformNotifier {
    /// `Build.VERSION.SDK_INT`, for the `PendingIntent` flags.
    sdk: i32,
}

impl PlatformNotifier {
    pub(super) fn new() -> Result<Self, String> {
        let sdk = with_helper(|env, _activity, class| {
            env.call_static_method(class, "sdkInt", "()I", &[])?.i()
        })?;
        crate::plog_info!("[notifications] NotificationManager backend ready (API {sdk})");
        Ok(Self { sdk })
    }

    pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
        match permission_code()? {
            STATE_NOT_ASKED => {
                // Posting now would be dropped by the system in silence. Ask,
                // and say so; the app posts again on `PermissionChanged`.
                request_permission();
                return Err(
                    "not shown: Android 13+ needs the POST_NOTIFICATIONS permission, which is \
                     being asked for now"
                        .to_string(),
                );
            }
            STATE_DENIED => {
                return Err(
                    "not shown: notifications are turned off for this app, or \
                     POST_NOTIFICATIONS was denied (Settings > Apps > Notifications)"
                        .to_string(),
                );
            }
            _ => {}
        }

        let id = notification.id.as_str();
        let request_code = wire::android_request_code(id);
        let actions: Vec<(&str, &str)> = notification
            .actions
            .as_ref()
            .iter()
            .filter(|a| {
                let action = a.id.as_str();
                !action.is_empty()
                    && action != wire::ANDROID_DEFAULT_ACTION
                    && action != wire::ANDROID_DISMISS_ACTION
            })
            .take(wire::ANDROID_MAX_ACTIONS)
            .map(|a| (a.id.as_str(), a.label.as_str()))
            .collect();
        if notification.actions.as_ref().len() > actions.len() {
            crate::plog_info!(
                "[notifications] {id:?}: Android shows at most {} buttons; the rest are dropped",
                wire::ANDROID_MAX_ACTIONS
            );
        }
        let action_ids: Vec<&str> = actions.iter().map(|a| a.0).collect();
        let action_labels: Vec<&str> = actions.iter().map(|a| a.1).collect();
        let silent = matches!(notification.sound, NotificationSound::Silent);
        let flags = wire::android_pending_intent_flags(self.sdk);

        let error: String = with_helper(|env, activity, class| {
            let channel = env.new_string(CHANNEL_NAME)?;
            let tag = env.new_string(id)?;
            let title = env.new_string(notification.title.as_str())?;
            let body = env.new_string(notification.body.as_str())?;
            let payload = env.new_string(notification.payload.as_str())?;
            let ids = string_array(env, &action_ids)?;
            let labels = string_array(env, &action_labels)?;
            let result = env
                .call_static_method(
                    class,
                    "post",
                    "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;ILjava/lang/\
                     String;Ljava/lang/String;[Ljava/lang/String;[Ljava/lang/String;Ljava/lang/\
                     String;ZI)Ljava/lang/String;",
                    &[
                        JValue::Object(activity),
                        JValue::Object(&channel),
                        JValue::Object(&tag),
                        JValue::Int(request_code),
                        JValue::Object(&title),
                        JValue::Object(&body),
                        JValue::Object(&ids),
                        JValue::Object(&labels),
                        JValue::Object(&payload),
                        JValue::Bool(u8::from(silent)),
                        JValue::Int(flags),
                    ],
                )?
                .l()?;
            if result.is_null() {
                return Ok(String::new());
            }
            let result = JString::from(result);
            let text: String = env.get_string(&result)?.into();
            Ok(text)
        })?;
        if error.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(super) fn withdraw(&mut self, id: &str) {
        let result = with_helper(|env, activity, class| {
            let tag = env.new_string(id)?;
            env.call_static_method(
                class,
                "cancel",
                "(Landroid/app/Activity;Ljava/lang/String;)V",
                &[JValue::Object(activity), JValue::Object(&tag)],
            )?;
            Ok(())
        });
        if let Err(e) = result {
            crate::plog_warn!("[notifications] could not withdraw {id:?}: {e}");
        }
    }
}

/// A tap, a button or a dismissal, forwarded by `AzulNotifications.java`
/// (from `AzulActivity.onCreate` / `onNewIntent`, the dismiss receiver, or
/// the queue a receiver kept while the library was not loaded). Runs on the
/// Java UI thread: it only queues and wakes the loop.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_notify_AzulNotifications_nativeOnNotificationEvent(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: jni::sys::jstring,
    action: jni::sys::jstring,
    payload: jni::sys::jstring,
    launched_app: jni::sys::jboolean,
) {
    if raw_env.is_null() || id.is_null() {
        return;
    }
    let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(raw_env) }) else {
        return;
    };
    // Owned `String`s, so each borrowing `JavaStr` drops before its `JString`.
    let mut read = |raw: jni::sys::jstring| -> String {
        if raw.is_null() {
            return String::new();
        }
        let jstr = unsafe { JString::from_raw(raw) };
        let owned: Option<String> = env.get_string(&jstr).ok().map(Into::into);
        owned.unwrap_or_default()
    };
    let id = read(id);
    let action = read(action);
    let payload = read(payload);
    if id.is_empty() {
        return;
    }
    queue_notification_event(wire::android_event(
        &id,
        &action,
        &payload,
        launched_app != 0,
    ));
    // The loop may be parked with no timeout (the app is in the background
    // while a receiver runs).
    crate::desktop::shell2::android::wake_event_loop();
}
