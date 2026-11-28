//! System sounds on Android: the Rust half of `scripts/android/AzulSounds.java`.
//!
//! `azul_core::sound::route` names the route of a sound; `common::event::play_sound_native` hands an
//! admitted route to [`play_system_sound`], which makes ONE static Java call and turns the one
//! line the helper answers into a `SoundOutcome` (`SoundOutcome::from_helper_line`, tested on every
//! host). The helper does the platform work: `AudioManager.playSoundEffect(FX_*)` (the "Touch
//! sounds" click), the user's notification tone through `RingtoneManager`, and the camera sounds
//! through `MediaActionSound`.
//!
//! The thread: the drain runs on the native thread that `android_main` is, which is attached for the
//! whole run; the call is one `CallStaticObjectMethod` plus the class lookup, and each of the three
//! platform calls is a binder call that returns at once (the tone and the camera sound play on their
//! own threads). A [`LocalFrame`](crate::desktop::extra::LocalFrame) releases what the call
//! allocated: this thread never returns to Java.

use azul_core::sound::{SoundOutcome, SoundRoute};

/// The helper class, in the slashed form `find_app_class` takes.
#[cfg(feature = "jni")]
const HELPER: &str = "com/azul/sound/AzulSounds";

/// Play one admitted route through the Java helper and say how that went.
#[cfg(feature = "jni")]
pub fn play_system_sound(route: &SoundRoute) -> SoundOutcome {
    match with_helper(|env, activity, class| call(env, activity, class, route)) {
        Ok(line) => SoundOutcome::from_helper_line(&line),
        Err(why) => SoundOutcome::Failed(why),
    }
}

/// Without `jni` there is no way to reach the platform's sounds.
#[cfg(not(feature = "jni"))]
pub fn play_system_sound(route: &SoundRoute) -> SoundOutcome {
    SoundOutcome::Failed(format!(
        "this build has no JNI, so {} cannot play",
        route.label()
    ))
}

/// Whether the user has "Touch sounds" on (`Settings.System.SOUND_EFFECTS_ENABLED`); `None` when
/// the helper cannot be reached.
#[cfg(feature = "jni")]
#[must_use]
pub fn touch_sounds_enabled() -> Option<bool> {
    with_helper(|env, activity, class| {
        env.call_static_method(
            class,
            "touchSoundsEnabled",
            "(Landroid/app/Activity;)Z",
            &[jni::objects::JValue::Object(activity)],
        )?
        .z()
    })
    .ok()
}

/// Without `jni` the setting cannot be read.
#[cfg(not(feature = "jni"))]
#[must_use]
pub fn touch_sounds_enabled() -> Option<bool> {
    None
}

/// Attach to the VM, resolve the helper class through the ACTIVITY's loader (a Rust thread has no
/// Java frame, where a bare `find_class` never sees an APK class) and run `f`. `Err` says why not.
#[cfg(feature = "jni")]
fn with_helper<R, F>(f: F) -> Result<R, String>
where
    F: for<'a> FnOnce(
        &mut jni::JNIEnv<'a>,
        &jni::objects::JObject<'a>,
        &jni::objects::JClass<'a>,
    ) -> Result<R, jni::errors::Error>,
{
    let vm_ptr = super::java_vm_ptr();
    let activity_ptr = super::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return Err("the activity is not published yet (android_main has not started)".into());
    }
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
        .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e:?}"))?;
    // Released when this helper returns: this thread never returns to Java (see `LocalFrame`).
    let _frame = crate::desktop::extra::LocalFrame::push(
        &env,
        crate::desktop::extra::LocalFrame::DEFAULT_CAPACITY,
    );
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let class =
        crate::desktop::extra::find_app_class(&mut env, &activity, HELPER).ok_or_else(|| {
            "AzulSounds is not in this APK (scripts/android/AzulSounds.java)".to_string()
        })?;
    f(&mut env, &activity, &class).map_err(|e| {
        // A pending exception left unhandled aborts the process at the next JNI boundary.
        drop(env.exception_clear());
        format!("AzulSounds: {e:?}")
    })
}

/// The Java call of one route: the line the helper answers (`played`, `skipped:<why>`,
/// `failed:<why>`).
#[cfg(feature = "jni")]
fn call<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'a>,
    class: &jni::objects::JClass<'a>,
    route: &SoundRoute,
) -> Result<String, jni::errors::Error> {
    use jni::objects::{JObject, JValue};

    let answer = match route {
        SoundRoute::Effect { name, fallback, .. } => {
            let name = env.new_string(*name)?;
            let fallback = match fallback {
                Some(fallback) => JObject::from(env.new_string(*fallback)?),
                None => JObject::null(),
            };
            env.call_static_method(
                class,
                "playEffect",
                "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(activity),
                    JValue::Object(&name),
                    JValue::Object(&fallback),
                ],
            )?
            .l()?
        }
        SoundRoute::Ringtone { kind, .. } => {
            let kind = env.new_string(*kind)?;
            env.call_static_method(
                class,
                "playTone",
                "(Landroid/app/Activity;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(activity), JValue::Object(&kind)],
            )?
            .l()?
        }
        SoundRoute::MediaAction {
            name,
            only_if_required,
            ..
        } => {
            let name = env.new_string(*name)?;
            env.call_static_method(
                class,
                "playCamera",
                "(Landroid/app/Activity;Ljava/lang/String;Z)Ljava/lang/String;",
                &[
                    JValue::Object(activity),
                    JValue::Object(&name),
                    JValue::Bool(u8::from(*only_if_required)),
                ],
            )?
            .l()?
        }
        SoundRoute::Silent(_) | SoundRoute::Haptic(_) => {
            return Ok(String::from(
                "failed:not a route of the Android sound player",
            ));
        }
    };
    if answer.is_null() {
        return Ok(String::from("failed:the helper answered nothing"));
    }
    let answer = jni::objects::JString::from(answer);
    let text: String = env.get_string(&answer)?.into();
    Ok(text)
}
