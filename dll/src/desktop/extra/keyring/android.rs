//! Android KeyStore keyring backend (JNI).
//!
//! `request` calls a Java helper `com.azul.keyring.AzulKeyring` (same
//! Rust/Java split as `AzulBiometric` / `AzulGeolocation`):
//! `store(Activity, long handle, String key, String secret, boolean
//! requireBiometry)`, `get(Activity, long handle, String key)`,
//! `delete(Activity, long handle, String key)`. The Java side drives the
//! Android `KeyStore` (an AES key with `setUserAuthenticationRequired(true)`
//! when biometry-bound — a `Get` then shows `BiometricPrompt`) and calls
//! back into `nativeOnKeyringResult` with a result code (+ the secret
//! string for a successful `Get`).
//!
//! Pending (non-Rust): the `AzulKeyring.java` helper plus the manifest
//! `USE_BIOMETRIC` permission. Until it ships, `find_class` fails and ops
//! resolve to `Unavailable`.

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_layout::managers::keyring::push_keyring_result;

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};

// One op in flight at a time; a nonzero handle drops a late result from a
// superseded op (the user could re-tap before the first prompt resolves).
#[cfg(target_os = "android")]
static REQUEST_HANDLE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "android")]
pub fn request(req: &KeyringRequest) {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    REQUEST_HANDLE.store(handle, Ordering::Relaxed);
    let ok = attach(|env, activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/keyring/AzulKeyring").ok()?;
        match req {
            KeyringRequest::Store {
                key,
                secret,
                require_biometry,
            } => {
                let k = env.new_string(key.as_str()).ok()?;
                let s = env.new_string(secret.as_str()).ok()?;
                env.call_static_method(
                    class,
                    "store",
                    "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;Z)V",
                    &[
                        JValue::Object(&activity),
                        JValue::Long(handle as i64),
                        JValue::Object(&k),
                        JValue::Object(&s),
                        JValue::Bool(*require_biometry as u8),
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
                    &[
                        JValue::Object(&activity),
                        JValue::Long(handle as i64),
                        JValue::Object(&k),
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
                    &[
                        JValue::Object(&activity),
                        JValue::Long(handle as i64),
                        JValue::Object(&k),
                    ],
                )
                .ok()?;
            }
        }
        Some(())
    });
    if ok.is_none() {
        REQUEST_HANDLE.store(0, Ordering::Relaxed);
        push_keyring_result(KeyringResult::Unavailable);
    }
}

#[cfg(not(target_os = "android"))]
pub fn request(req: &KeyringRequest) {
    let _ = req;
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. Mirrors the biometric / geolocation
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

/// Receives a result from `AzulKeyring`. `code` per the contract:
/// 0=Stored, 1=Deleted, 2=Retrieved (secret in `secret_or_null`),
/// 3=NotFound, 4=Denied (biometric gate failed), 5=Unavailable, else
/// Error. Drops results whose `handle` doesn't match the live op.
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

    let result = match code {
        0 => KeyringResult::Stored,
        1 => KeyringResult::Deleted,
        2 => {
            // Retrieved — read the secret jstring. Materialize the owned
            // `String` so the borrowing `JavaStr` drops before the
            // `JString` (jni 0.21 borrow rules).
            let secret: Option<String> = if raw_env.is_null() || secret_or_null.is_null() {
                None
            } else if let Ok(mut env) = jni::JNIEnv::from_raw(raw_env) {
                let jstr = jni::objects::JString::from_raw(secret_or_null);
                env.get_string(&jstr).ok().map(|s| s.into())
            } else {
                None
            };
            match secret {
                Some(s) => KeyringResult::Retrieved(s.into()),
                None => KeyringResult::Error,
            }
        }
        3 => KeyringResult::NotFound,
        4 => KeyringResult::Denied,
        5 => KeyringResult::Unavailable,
        _ => KeyringResult::Error,
    };
    push_keyring_result(result);
}
