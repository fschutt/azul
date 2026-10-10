//! Android: the sign-in page in a Custom Tab, the redirect back through the app's own scheme
//! (`com.azul.auth.AzulAuthSession`, scripts/android).
//!
//! 1. [`start`] parks the session (id -> its answer's slot) in [`PENDING`] and calls
//!    `AzulAuthSession.start(activity, id, url, scheme, ephemeral)`: an `ACTION_VIEW` intent
//!    with the Custom Tabs session extra, so Chrome (and every Custom Tabs browser) shows the
//!    page as a tab over the app; a browser without Custom Tabs opens it as a page.
//! 2. The provider redirects to `<scheme>:...`. The manifest's `AzulAuthSession$RedirectActivity`
//!    (its intent filter names the scheme: `AZ_ANDROID_AUTH_SCHEME` in build-android.sh) hands
//!    the URL to `AzulActivity` with `CLEAR_TOP | SINGLE_TOP` - the tab closes, the running
//!    instance gets it in `onNewIntent` -, which passes it to `AzulAuthSession.onIntent`, which
//!    calls [`Java_com_azul_auth_AzulAuthSession_nativeOnRedirect`] with the URL.
//! 3. The app comes back WITHOUT the redirect (the user closed the tab): `AzulActivity.onResume`
//!    after the pause the tab caused reports a cancel the same way.
//!
//! The answer goes through `auth_session::finish` like every platform's; a session whose time
//! is up is dropped on both sides (`AzulAuthSession.cancel`), so a late redirect is ignored. A
//! sign-in the system ended by killing the process while the tab showed is lost: the new
//! process has no session waiting.

#![allow(non_snake_case)]

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
    time::{Duration, Instant},
};

use azul_layout::{
    auth_session::{self as auth, AuthSessionResult, AuthSessionStatus},
    request::PollFn,
};

/// The Java class (slashed, for `find_app_class`).
const JAVA_CLASS: &str = "com/azul/auth/AzulAuthSession";

/// A session waiting for its redirect.
struct Pending {
    slot: Arc<Mutex<Option<AuthSessionResult>>>,
    redirect_uri: String,
    /// The authorize URL (its `state` is checked on the redirect).
    authorize_url: String,
}

/// The sessions waiting, by id.
static PENDING: Mutex<BTreeMap<u64, Pending>> = Mutex::new(BTreeMap::new());

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn pending() -> std::sync::MutexGuard<'static, BTreeMap<u64, Pending>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Opens `url` (the authorize URL with its `redirect_uri`) in a Custom Tab, coming back to
/// `scheme`; the poll answers once the redirect (or the return without it) arrived, or
/// `timeout` passed.
pub(super) fn start(
    url: &str,
    scheme: &str,
    redirect_uri: &str,
    prefers_ephemeral: bool,
    timeout: Duration,
) -> Result<PollFn, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let slot: Arc<Mutex<Option<AuthSessionResult>>> = Arc::default();
    pending().insert(
        id,
        Pending {
            slot: Arc::clone(&slot),
            redirect_uri: redirect_uri.to_string(),
            authorize_url: url.to_string(),
        },
    );
    let started = crate::desktop::extra::file_picker::android::with_env(|env, activity| {
        // Via the Activity's class loader: this thread has no Java frame (see
        // `extra::find_app_class`).
        let class = crate::desktop::extra::find_app_class(env, &activity, JAVA_CLASS)
            .ok_or(jni::errors::Error::JavaException)?;
        let j_url = env.new_string(url)?;
        let j_scheme = env.new_string(scheme)?;
        env.call_static_method(
            class,
            "start",
            "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;Z)V",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Long(id as i64),
                jni::objects::JValue::Object(&j_url),
                jni::objects::JValue::Object(&j_scheme),
                jni::objects::JValue::Bool(u8::from(prefers_ephemeral)),
            ],
        )?;
        Ok(())
    });
    if let Err(why) = started {
        pending().remove(&id);
        return Err(format!(
            "the sign-in tab could not be opened ({why}); is com.azul.auth.AzulAuthSession in \
             the APK?"
        ));
    }
    Ok(super::poll_slot(
        slot,
        Instant::now() + timeout,
        redirect_uri.to_string(),
        move |timed_out| {
            pending().remove(&id);
            if timed_out {
                cancel_in_java(id);
            }
        },
    ))
}

/// Tells the Java side to drop session `id` (its time is up): a late redirect is ignored.
fn cancel_in_java(id: u64) {
    let _ = crate::desktop::extra::file_picker::android::with_env(|env, activity| {
        let class = crate::desktop::extra::find_app_class(env, &activity, JAVA_CLASS)
            .ok_or(jni::errors::Error::JavaException)?;
        env.call_static_method(
            class,
            "cancel",
            "(J)V",
            &[jni::objects::JValue::Long(id as i64)],
        )?;
        Ok(())
    });
}

/// A Java string argument as a Rust string; `None` for null.
unsafe fn string_of(env: &mut jni::JNIEnv<'_>, raw: jni::sys::jstring) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    let jstr = unsafe { jni::objects::JString::from_raw(raw) };
    let owned: Option<String> = env.get_string(&jstr).ok().map(Into::into);
    owned
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// `AzulAuthSession.nativeOnRedirect(id, urlOrNull, reasonOrNull)`: the redirect of session
/// `id` arrived (`url`), or the app came back without it (`reason`).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_auth_AzulAuthSession_nativeOnRedirect(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: i64,
    url_or_null: jni::sys::jstring,
    reason_or_null: jni::sys::jstring,
) {
    if raw_env.is_null() {
        return;
    }
    let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(raw_env) }) else {
        return;
    };
    let url = unsafe { string_of(&mut env, url_or_null) };
    let reason = unsafe { string_of(&mut env, reason_or_null) };
    let Some(session) = pending().remove(&(id as u64)) else {
        return;
    };
    let result = match url {
        Some(url) => auth::finish(&url, &session.redirect_uri, &session.authorize_url),
        None => AuthSessionResult::ended(
            AuthSessionStatus::Cancelled,
            &session.redirect_uri,
            reason.as_deref().unwrap_or("the sign-in tab was closed"),
        ),
    };
    let mut slot = session.slot.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(result);
    }
}
