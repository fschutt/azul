//! Android Keystore keyring backend (JNI), to the model of `core/src/keyring.rs`.
//!
//! The Java helper `com.azul.keyring.AzulKeyring` (same Rust/Java split as `AzulBiometric` / `AzulGeolocation`) holds the
//! storage: one Keystore key (or key pair) PER ENTRY, the ciphertext and a meta record (level, made when) in the app's
//! preferences. What each protection level is on Android, and why it is enforced by hardware, is in that file's header.
//!
//! Three entry points:
//!
//! * [`request`], the asynchronous path of a user action (`CallbackInfo::keyring_store`, `keyring_store_protected`,
//!   `keyring_get`, `keyring_unlock`, `keyring_delete`): `store(Activity, long handle, String key, String secret, int
//!   protection, String reason, String cancel)`, `get(Activity, handle, key)`, `unlock(Activity, handle, key, reason,
//!   cancel)`, `delete(Activity, handle, key)`. The Java side answers through `nativeOnKeyringResult(handle, code,
//!   secretOrNull)` (a quiet op answers before it returns; an unlock answers from the BiometricPrompt on the UI thread),
//!   which parks the result in the app's `KeyringChannels::results` (reached through `process_app`: a JNI entry point
//!   carries no context).
//! * [`blocking`], the statics a worker thread calls (`Keyring::get_blocking` and the quiet ones): a SYNCHRONOUS Java entry
//!   `quiet(Activity, int op, String key, String secret) -> String[] {code, secret}`, never a sheet. A protected item answers
//!   `WouldPrompt` (or `Invalidated`), and the ops that would show the sheet are answered `WouldPrompt` here too.
//! * [`needs_prompt`] and [`support`], the quiet probes: `needsPrompt(Activity, String) -> int` (the order of `KeyringNeed`)
//!   and `support(Activity, int) -> int` (the order of `KeyringSupport`).
//!
//! The result codes are the contract of the Java file: 0 Stored, 1 Deleted, 2 Retrieved (the secret in the string
//! argument), 3 NotFound, 4 Denied, 5 Unavailable, 6 Error, 7 Cancelled, 8 Failed, 9 LockedOut, 10 NotEnrolled,
//! 11 Invalidated, 12 WouldPrompt ([`result_of`], tested).

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};

use azul_core::keyring::{KeyringRequest, KeyringResult};
#[cfg(any(target_os = "android", test))]
use azul_core::keyring::{KeyringNeed, KeyringProtection, KeyringSupport};
use azul_layout::managers::shared_queue::SharedQueue;

#[cfg(target_os = "android")]
use super::Interaction;

// One op in flight at a time; a nonzero handle drops a late result from a superseded op (the user could re-tap before
// the first prompt resolves).
#[cfg(target_os = "android")]
static REQUEST_HANDLE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// The result a code of the Java helper stands for.
#[must_use]
#[cfg(any(target_os = "android", test))]
pub(super) fn result_of(code: i32, secret: Option<String>) -> KeyringResult {
    match code {
        0 => KeyringResult::Stored,
        1 => KeyringResult::Deleted,
        2 => match secret {
            Some(secret) => KeyringResult::Retrieved(secret.into()),
            // A Retrieved without its secret is an answer that cannot be used.
            None => KeyringResult::Error,
        },
        3 => KeyringResult::NotFound,
        4 => KeyringResult::Denied,
        5 => KeyringResult::Unavailable,
        7 => KeyringResult::Cancelled,
        8 => KeyringResult::Failed,
        9 => KeyringResult::LockedOut,
        10 => KeyringResult::NotEnrolled,
        11 => KeyringResult::Invalidated,
        12 => KeyringResult::WouldPrompt,
        _ => KeyringResult::Error,
    }
}

/// What `needsPrompt` answered, in the order of `KeyringNeed`.
#[must_use]
#[cfg(any(target_os = "android", test))]
pub(super) fn need_of(code: i32) -> KeyringNeed {
    match code {
        0 => KeyringNeed::NotFound,
        1 => KeyringNeed::Nothing,
        2 => KeyringNeed::Unlock,
        3 => KeyringNeed::UserPresence,
        4 => KeyringNeed::Biometric,
        5 => KeyringNeed::Invalidated,
        6 => KeyringNeed::Unavailable,
        _ => KeyringNeed::Error,
    }
}

/// What `support` answered, in the order of `KeyringSupport`.
#[must_use]
#[cfg(any(target_os = "android", test))]
pub(super) fn support_of(code: i32) -> KeyringSupport {
    match code {
        0 => KeyringSupport::Hardware,
        1 => KeyringSupport::Library,
        2 => KeyringSupport::NotEnrolled,
        _ => KeyringSupport::Unavailable,
    }
}

/// The protection level as the number the Java helper takes.
#[must_use]
#[cfg(any(target_os = "android", test))]
pub(super) const fn level_of(protection: KeyringProtection) -> i32 {
    protection.code() as i32
}

#[cfg(target_os = "android")]
pub fn request(req: &KeyringRequest, results: &SharedQueue<KeyringResult>) {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    REQUEST_HANDLE.store(handle, Ordering::Relaxed);
    let ok = attach(|env, activity| {
        use jni::objects::JValue;
        let class = crate::desktop::extra::find_app_class(env, &activity, "com/azul/keyring/AzulKeyring")?;
        match req {
            KeyringRequest::Store {
                key,
                secret,
                protection,
                prompt,
            } => {
                let k = env.new_string(key.as_str()).ok()?;
                let s = env.new_string(secret.as_str()).ok()?;
                let reason = env.new_string(prompt.reason.as_str()).ok()?;
                let cancel = env.new_string(prompt.cancel_label.as_str()).ok()?;
                env.call_static_method(
                    class,
                    "store",
                    "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;)V",
                    &[
                        JValue::Object(&activity),
                        JValue::Long(handle as i64),
                        JValue::Object(&k),
                        JValue::Object(&s),
                        JValue::Int(level_of(*protection)),
                        JValue::Object(&reason),
                        JValue::Object(&cancel),
                    ],
                )
                .ok()?;
            }
            KeyringRequest::Get { key } => {
                let k = env.new_string(key.as_str()).ok()?;
                env.call_static_method(
                    class,
                    "get",
                    "(Landroid/app/Activity;JLjava/lang/String;)V",
                    &[JValue::Object(&activity), JValue::Long(handle as i64), JValue::Object(&k)],
                )
                .ok()?;
            }
            KeyringRequest::Unlock { key, prompt } => {
                let k = env.new_string(key.as_str()).ok()?;
                let reason = env.new_string(prompt.reason.as_str()).ok()?;
                let cancel = env.new_string(prompt.cancel_label.as_str()).ok()?;
                env.call_static_method(
                    class,
                    "unlock",
                    "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
                    &[
                        JValue::Object(&activity),
                        JValue::Long(handle as i64),
                        JValue::Object(&k),
                        JValue::Object(&reason),
                        JValue::Object(&cancel),
                    ],
                )
                .ok()?;
            }
            KeyringRequest::Delete { key } => {
                let k = env.new_string(key.as_str()).ok()?;
                env.call_static_method(
                    class,
                    "delete",
                    "(Landroid/app/Activity;JLjava/lang/String;)V",
                    &[JValue::Object(&activity), JValue::Long(handle as i64), JValue::Object(&k)],
                )
                .ok()?;
            }
        }
        Some(())
    });
    if ok.is_none() {
        // No Java helper, no activity: answer rather than hang.
        REQUEST_HANDLE.store(0, Ordering::Relaxed);
        results.push(KeyringResult::Unavailable);
    }
}

#[cfg(not(target_os = "android"))]
pub fn request(req: &KeyringRequest, results: &SharedQueue<KeyringResult>) {
    let _ = (req, results);
}

/// A worker thread's call (`Keyring::get_blocking`, the quiet ones): answered here on the calling thread through the
/// helper's synchronous entry, never through the Java callback and never with the user-check sheet. An op that would
/// show the sheet (an `Unlock`, a protected `Store`) is `WouldPrompt`; so is a read of a protected item (the helper answers
/// it), `Invalidated` when its key is gone.
#[cfg(target_os = "android")]
pub(super) fn blocking(req: &KeyringRequest, _interaction: Interaction) -> KeyringResult {
    // The helper's op numbers: 0 read, 1 store (an UNPROTECTED item), 2 delete.
    let (op, key, secret) = match req {
        KeyringRequest::Get { key } => (0, key, None),
        KeyringRequest::Store {
            key,
            secret,
            protection: KeyringProtection::DeviceUnlocked,
            ..
        } => (1, key, Some(secret)),
        KeyringRequest::Delete { key } => (2, key, None),
        KeyringRequest::Store { .. } | KeyringRequest::Unlock { .. } => return KeyringResult::WouldPrompt,
    };
    let answered = attach(|env, activity| {
        use jni::objects::{JObject, JObjectArray, JString, JValue};
        let class = crate::desktop::extra::find_app_class(env, &activity, "com/azul/keyring/AzulKeyring")?;
        let k = env.new_string(key.as_str()).ok()?;
        let s: JObject = match secret {
            Some(secret) => env.new_string(secret.as_str()).ok()?.into(),
            None => JObject::null(),
        };
        let out = env
            .call_static_method(
                class,
                "quiet",
                "(Landroid/app/Activity;ILjava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
                &[JValue::Object(&activity), JValue::Int(op), JValue::Object(&k), JValue::Object(&s)],
            )
            .ok()?
            .l()
            .ok()?;
        let array = JObjectArray::from(out);
        let element = |env: &mut jni::JNIEnv, at: i32| -> Option<String> {
            let item = env.get_object_array_element(&array, at).ok()?;
            if item.is_null() {
                return None;
            }
            let text = env.get_string(&JString::from(item)).ok()?;
            Some(text.into())
        };
        let code: i32 = element(env, 0)?.parse().ok()?;
        Some((code, element(env, 1)))
    });
    match answered {
        Some((code, secret)) => result_of(code, secret),
        None => KeyringResult::Unavailable,
    }
}

/// What a read of `key` needs, from the item's meta record: never a sheet.
#[cfg(target_os = "android")]
pub(super) fn needs_prompt(key: &str) -> KeyringNeed {
    attach(|env, activity| {
        use jni::objects::JValue;
        let class = crate::desktop::extra::find_app_class(env, &activity, "com/azul/keyring/AzulKeyring")?;
        let k = env.new_string(key).ok()?;
        let code = env
            .call_static_method(
                class,
                "needsPrompt",
                "(Landroid/app/Activity;Ljava/lang/String;)I",
                &[JValue::Object(&activity), JValue::Object(&k)],
            )
            .ok()?
            .i()
            .ok()?;
        Some(need_of(code))
    })
    .unwrap_or(KeyringNeed::Unavailable)
}

/// What this device does for `protection`: never a sheet.
#[cfg(target_os = "android")]
pub(super) fn support(protection: KeyringProtection) -> KeyringSupport {
    attach(|env, activity| {
        use jni::objects::JValue;
        let class = crate::desktop::extra::find_app_class(env, &activity, "com/azul/keyring/AzulKeyring")?;
        let code = env
            .call_static_method(
                class,
                "support",
                "(Landroid/app/Activity;I)I",
                &[JValue::Object(&activity), JValue::Int(level_of(protection))],
            )
            .ok()?
            .i()
            .ok()?;
        Some(support_of(code))
    })
    .unwrap_or(KeyringSupport::Unavailable)
}

/// Attach the current thread to the published JavaVM and run `f` with the `JNIEnv` + the activity `JObject`. Mirrors the
/// biometric / geolocation backend attach sequence.
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

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives a result from `AzulKeyring` (codes: see the module doc). Drops results whose `handle` doesn't match the live op.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_keyring_AzulKeyring_nativeOnKeyringResult(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    handle: jni::sys::jlong,
    code: jni::sys::jint,
    secret_or_null: jni::sys::jstring,
) {
    if (handle as u64) != REQUEST_HANDLE.load(Ordering::Relaxed) {
        return;
    }
    REQUEST_HANDLE.store(0, Ordering::Relaxed);

    let secret: Option<String> = if raw_env.is_null() || secret_or_null.is_null() {
        None
    } else if let Ok(mut env) = jni::JNIEnv::from_raw(raw_env) {
        // Materialize the owned `String` so the borrowing `JavaStr` drops before the `JString` (jni 0.21 borrow rules).
        let jstr = jni::objects::JString::from_raw(secret_or_null);
        env.get_string(&jstr).ok().map(Into::into)
    } else {
        None
    };
    // The JNI entry has no context: the App's services are where `App::run` put them.
    if let Some(services) = crate::desktop::process_app::services() {
        services.keyring.results.push(result_of(code, secret));
    }
}

#[cfg(test)]
#[path = "android_test.rs"]
mod tests;
