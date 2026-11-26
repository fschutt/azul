//! What the platform needs held for multicast packets to arrive at all.
//!
//! Android: its Wi-Fi chip drops multicast frames to save power unless the app holds a
//! `WifiManager.MulticastLock` (and declares `android.permission.CHANGE_WIFI_MULTICAST_STATE`, a normal
//! permission granted at install: scripts/android/AndroidManifest.xml has it). The lock belongs to
//! the engine that asked: it is created per engine through JNI and released when the engine is
//! dropped, so two engines hold two locks and nothing is shared. Everywhere else there is nothing
//! to hold.

use super::discovery::Held;

/// Takes what the platform needs held for multicast to arrive; the returned value releases it when
/// dropped. `Err(reason)` when the platform refused (Android: no JNI context yet, or the permission
/// is not declared).
#[cfg(not(target_os = "android"))]
pub(super) fn acquire() -> Result<Option<Held>, String> {
    Ok(None)
}

/// See the non-Android version.
#[cfg(target_os = "android")]
pub(super) fn acquire() -> Result<Option<Held>, String> {
    use jni::objects::{GlobalRef, JObject, JValue};

    use crate::desktop::extra::sensors::{system_service, with_activity};

    /// The held lock; releases it on drop.
    struct Lock(GlobalRef);

    impl Drop for Lock {
        fn drop(&mut self) {
            let lock = self.0.clone();
            let _ = with_activity(|env, _activity| {
                env.call_method(&lock, "release", "()V", &[]).ok().map(|_| ())
            });
        }
    }

    const REFUSED: &str = "Android refused the Wi-Fi multicast lock (no activity yet, or the app \
                           does not declare android.permission.CHANGE_WIFI_MULTICAST_STATE)";

    let lock = with_activity(|env, activity| {
        let wifi = system_service(env, activity, "wifi")?;
        let tag = JObject::from(env.new_string("azul-iroh").ok()?);
        let lock = env
            .call_method(
                &wifi,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&tag)],
            )
            .and_then(|v| v.l())
            .ok()?;
        // One lock per engine: not counted, released once.
        env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(0)])
            .ok()?;
        env.call_method(&lock, "acquire", "()V", &[]).ok()?;
        env.new_global_ref(&lock).ok()
    });
    match lock {
        Some(lock) => Ok(Some(Box::new(Lock(lock)))),
        None => Err(REFUSED.to_string()),
    }
}
