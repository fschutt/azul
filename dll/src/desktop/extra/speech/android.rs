//! Android: the system's TextToSpeech and SpeechRecognizer, through the Java helper
//! `com.azul.speech.AzulSpeech` (scripts/android/AzulSpeech.java; the same Rust / Java split as
//! the geolocation and the file picker).
//!
//! Saying hands the text to the helper, which keeps what is said before the engine service is
//! bound and says it once it is. Hearing asks the helper to listen on the main thread (where
//! Android wants its recognizer) and waits here, on the caller's worker thread, for the one
//! answer `nativeOnHeard` brings - or for the time and a grace period, after which the answer
//! is a failure (a recognizer that never answered).
//!
//! The microphone permission (`RECORD_AUDIO`, opt-in in the manifest through
//! `AZ_ANDROID_PERMISSIONS`) is the permission manager's to ask for; without it hearing answers
//! `Denied`. Without the helper in the APK (`AZ_ANDROID_NO_JAVA=1`) everything answers
//! `Unavailable`.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Condvar, Mutex,
    },
    time::Duration,
};

use jni::objects::JValue;

use super::{heard_from_android, HeardStatus, SpeechHeard, SpeechStatus};

const HELPER: &str = "com/azul/speech/AzulSpeech";

/// How long past its own time a recognizer may take to answer.
const ANSWER_GRACE: Duration = Duration::from_secs(5);

/// The answers on their way, by listening handle (`None` while it waits).
static ANSWERS: Mutex<Option<HashMap<u64, Option<SpeechHeard>>>> = Mutex::new(None);
static ANSWERED: Condvar = Condvar::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Attaches this thread to the app's JavaVM and runs `f` with the `JNIEnv` and the activity;
/// `None` when the VM or the activity is not published, or `f` gives up.
fn attach<R>(f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Option<R>) -> Option<R> {
    use jni::{objects::JObject, JavaVM};

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

/// `AzulSpeech.<method>(..)` returning a boolean; `None` when the helper is missing or throws.
fn ask_flag(method: &str, signature: &str, with_activity: bool) -> Option<bool> {
    attach(|env, activity| {
        let class = crate::desktop::extra::find_app_class(env, &activity, HELPER)?;
        let args: Vec<JValue> = if with_activity {
            vec![JValue::Object(&activity)]
        } else {
            Vec::new()
        };
        match env.call_static_method(class, method, signature, &args).and_then(|v| v.z()) {
            Ok(answer) => Some(answer),
            Err(_) => {
                let _ = env.exception_clear();
                None
            }
        }
    })
}

pub fn say(text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let queued = attach(|env, activity| {
        let class = crate::desktop::extra::find_app_class(env, &activity, HELPER)?;
        let text = env.new_string(text).ok()?;
        let language = env.new_string(language).ok()?;
        match env
            .call_static_method(
                class,
                "say",
                "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Z)Z",
                &[
                    JValue::Object(&activity),
                    JValue::Object(&text),
                    JValue::Object(&language),
                    JValue::Bool(u8::from(interrupt)),
                ],
            )
            .and_then(|v| v.z())
        {
            Ok(queued) => Some(queued),
            Err(_) => {
                let _ = env.exception_clear();
                None
            }
        }
    });
    if queued == Some(true) {
        SpeechStatus::Queued
    } else {
        SpeechStatus::Unavailable
    }
}

pub fn stop() {
    let _ = attach(|env, activity| {
        let class = crate::desktop::extra::find_app_class(env, &activity, HELPER)?;
        if env.call_static_method(class, "stop", "()V", &[]).is_err() {
            let _ = env.exception_clear();
        }
        Some(())
    });
}

pub fn is_speaking() -> bool {
    ask_flag("isSpeaking", "()Z", false).unwrap_or(false)
}

pub fn is_available() -> bool {
    ask_flag("isAvailable", "()Z", false).unwrap_or(false)
}

pub fn listen(language: &str, timeout_ms: u32) -> SpeechHeard {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut answers) = ANSWERS.lock() {
        answers.get_or_insert_with(HashMap::new).insert(handle, None);
    }
    let asked = attach(|env, activity| {
        let class = crate::desktop::extra::find_app_class(env, &activity, HELPER)?;
        let language = env.new_string(language).ok()?;
        match env.call_static_method(
            class,
            "listen",
            "(Landroid/app/Activity;JLjava/lang/String;I)V",
            &[
                JValue::Object(&activity),
                JValue::Long(handle as i64),
                JValue::Object(&language),
                JValue::Int(i32::try_from(timeout_ms).unwrap_or(i32::MAX)),
            ],
        ) {
            Ok(_) => Some(()),
            Err(_) => {
                let _ = env.exception_clear();
                None
            }
        }
    });
    if asked.is_none() {
        take(handle);
        return SpeechHeard::none(
            HeardStatus::Unavailable,
            "there is no speech recognition in this build (the Java helper is missing)",
        );
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(u64::from(timeout_ms)) + ANSWER_GRACE;
    let Ok(mut answers) = ANSWERS.lock() else {
        return SpeechHeard::none(HeardStatus::Failed, "the speech answer's lock is poisoned");
    };
    loop {
        if let Some(heard) = answers
            .as_mut()
            .and_then(|map| map.get_mut(&handle))
            .and_then(Option::take)
        {
            if let Some(map) = answers.as_mut() {
                map.remove(&handle);
            }
            return heard;
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            if let Some(map) = answers.as_mut() {
                map.remove(&handle);
            }
            return SpeechHeard::none(HeardStatus::Failed, "the speech recognizer did not answer");
        }
        answers = match ANSWERED.wait_timeout(answers, deadline - now) {
            Ok((guard, _)) => guard,
            Err(_) => {
                return SpeechHeard::none(HeardStatus::Failed, "the speech answer's lock is poisoned")
            }
        };
    }
}

/// Forgets the listening `handle` (it was never asked, or gave up).
fn take(handle: u64) {
    if let Ok(mut answers) = ANSWERS.lock() {
        if let Some(map) = answers.as_mut() {
            map.remove(&handle);
        }
    }
}

/// A Java string argument as a Rust string (empty for null or unreadable).
unsafe fn string_of(env: &mut jni::JNIEnv<'_>, raw: jni::sys::jstring) -> String {
    if raw.is_null() {
        return String::new();
    }
    let jstr = unsafe { jni::objects::JString::from_raw(raw) };
    env.get_string(&jstr).map(Into::into).unwrap_or_default()
}

/// The recognizer's answer from `AzulSpeech`, for the listening `handle`: parked for the
/// waiting `listen`, dropped when nobody waits any more (it gave up).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_speech_AzulSpeech_nativeOnHeard(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    handle: jni::sys::jlong,
    status: jni::sys::jint,
    text: jni::sys::jstring,
    message: jni::sys::jstring,
    confidence: jni::sys::jfloat,
) {
    if raw_env.is_null() {
        return;
    }
    let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(raw_env) }) else {
        return;
    };
    let text = unsafe { string_of(&mut env, text) };
    let message = unsafe { string_of(&mut env, message) };
    let heard = heard_from_android(status, &text, &message, confidence);
    if let Ok(mut answers) = ANSWERS.lock() {
        if let Some(slot) = answers.as_mut().and_then(|map| map.get_mut(&(handle as u64))) {
            *slot = Some(heard);
            ANSWERED.notify_all();
        }
    }
}
