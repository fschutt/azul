//! Android media session - `android.media.session.MediaSession`.
//!
//! What the lock screen, the notification shade and a Bluetooth headset's
//! buttons talk to. The Java half is `scripts/android/AzulMediaSession.java`,
//! which owns the session object: `MediaSession`, `MediaMetadata` and
//! `PlaybackState` are Java-side builders with no NDK equivalent, so the split
//! follows `AzulSensors` and `AzulGamepad`.
//!
//! # This is both halves at once, unlike every other platform
//!
//! On Linux and macOS the media KEYS and the media SESSION are separate
//! objects that happen to be gated by one flag. Here they are literally the
//! same object: a `MediaSession` receives the transport buttons through its
//! callback AND carries the metadata, so registering once does both.
//!
//! # The two directions have opposite drift hazards
//!
//! Buttons come back as ANDROID's `KEYCODE_MEDIA_*` constants, which cannot
//! drift because both sides name the same platform values - the mapping lives
//! in the shared `mod.rs` so it is compiled and TESTED on every host, not only
//! on a target this machine never runs tests for. The playback state
//! goes out as `MediaPlaybackState`'s own discriminants, which is an
//! azul-side numbering crossing a boundary - the same hazard as the sensor
//! kind codes, and it gets the same guard.

use azul_core::media_session::NowPlayingInfo;
use azul_layout::managers::media_keys::push_media_key;

use super::media_keycode_to_key;

/// A transport button arrived from the session's callback.
#[cfg(feature = "jni")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_media_AzulMediaSession_nativeOnMediaButton(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    keycode: i32,
) {
    if let Some(key) = media_keycode_to_key(keycode) {
        // Parked, not handled here: this runs on Android's media-button
        // thread and the engine's key pass belongs to the main one. Same
        // channel MPRIS, `MPRemoteCommandCenter` and SMTC use.
        push_media_key(key);
    }
}

/// Claim the media session. Idempotent and quiet.
#[cfg(feature = "jni")]
pub fn start() {
    if !azul_layout::window::expose_system_media_controls() {
        return;
    }
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    with_helper(|env, activity, class| {
        env.call_static_method(
            class,
            "start",
            "(Landroid/app/Activity;)V",
            &[jni::objects::JValue::Object(activity)],
        )
        .map(|_| ())
    });
}

/// Publish what the app is playing.
#[cfg(feature = "jni")]
pub fn publish(info: &NowPlayingInfo) {
    use jni::objects::JValue;

    with_helper(|env, activity, class| {
        let _ = activity;
        let title = env.new_string(info.title.as_str())?;
        let artist = env.new_string(info.artist.as_str())?;
        let album = env.new_string(info.album.as_str())?;
        let art = env.new_string(info.artwork_url.as_str())?;
        env.call_static_method(
            class,
            "publish",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJI)V",
            &[
                JValue::Object(&title),
                JValue::Object(&artist),
                JValue::Object(&album),
                JValue::Object(&art),
                // MILLISECONDS both ways: Android is the one platform whose
                // unit already matches what `NowPlayingInfo` stores, so there
                // is no conversion here and nothing to get wrong.
                JValue::Long(i64::try_from(info.duration_ms).unwrap_or(i64::MAX)),
                JValue::Long(i64::try_from(info.position_ms).unwrap_or(i64::MAX)),
                // The azul-side discriminant, pinned by
                // `the_playback_state_discriminants_are_the_jni_wire_codes`.
                JValue::Int(info.state as i32),
            ],
        )
        .map(|_| ())
    });
}

/// Attach to the VM, resolve the helper class and run `f`.
///
/// The class must come through the ACTIVITY's loader: this runs on a Rust
/// thread with no Java frame, so a bare `find_class` resolves against the
/// system loader and never sees an APK class.
#[cfg(feature = "jni")]
fn with_helper<F>(f: F)
where
    F: for<'a> FnOnce(
        &mut jni::JNIEnv<'a>,
        &jni::objects::JObject<'a>,
        &jni::objects::JClass<'a>,
    ) -> Result<(), jni::errors::Error>,
{
    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
            .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach_current_thread: {e:?}"))?;
        let activity =
            unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
        let class = crate::desktop::extra::find_app_class(
            &mut env,
            &activity,
            "com/azul/media/AzulMediaSession",
        )
        .ok_or_else(|| "AzulMediaSession not in this APK".to_string())?;
        f(&mut env, &activity, &class).map_err(|e| {
            // A pending exception left unhandled aborts the process at the
            // next JNI boundary, which would surface far from here.
            let _ = env.exception_clear();
            format!("AzulMediaSession: {e:?}")
        })
    })();
    if let Err(e) = result {
        crate::plog_info!("[media-session] {}", e);
    }
}

/// Without `jni` there is no Java side to call.
#[cfg(not(feature = "jni"))]
pub fn start() {}

/// Without `jni` there is no Java side to call.
#[cfg(not(feature = "jni"))]
pub fn publish(_info: &NowPlayingInfo) {}
