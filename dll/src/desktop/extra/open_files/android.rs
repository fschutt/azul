//! Android: the intents that ask the app to open something, read and answered from Rust through JNI.
//!
//! `AzulActivity` forwards every intent to `AzulOpenIntent.forward`, which calls
//! [`Java_com_azul_open_AzulOpenIntent_nativeOnIntent`] on the UI thread. What happens there is cheap and bounded:
//!
//! 1. the intent's action, type, data, `EXTRA_STREAM`, `ClipData`, `EXTRA_TEXT` and flags are read ([`read_intent`]) and handed to
//!    the pure classifier ([`azul_layout::open_intent::classify`]): a launch from the launcher, or an action this app does not
//!    take, ends here;
//! 2. where the sender offered a grant that outlives the activity (`FLAG_GRANT_PERSISTABLE_URI_PERMISSION`),
//!    `ContentResolver.takePersistableUriPermission` is asked for each file (a refusal is logged, not fatal);
//! 3. a WORKER thread does the rest: it asks the content resolver for the name, size and type of each file, opens it with
//!    `openFileDescriptor`, copies it into `<files>/azul-inbox/<token>/<name>` ([`azul_layout::open_inbox`]: the limits, the
//!    names), builds the [`OpenRequest`] (an item that could not be copied is still an item, with its reason) and parks it in the
//!    app's open queue, where the loop's pump runs the app's handler. A request that comes before the App is registered (the intent
//!    that STARTED the app) waits for it, up to ten seconds.
//!
//! The worker owns a global reference to the content resolver and a handle on the VM: it never touches the activity.

use std::{
    fs::File,
    os::fd::FromRawFd,
    path::PathBuf,
    time::{Duration, Instant},
};

use azul_core::open_files::{OpenAction, OpenRequest, OpenedItem, OpenedItemVec};
use azul_css::{AzString, OptionU64};
use azul_layout::{
    mime,
    open_inbox::{Inbox, OpenLimits},
    open_intent::{self, IntentInfo, OpenPlan, FLAG_GRANT_WRITE_URI_PERMISSION},
};
use jni::{
    objects::{GlobalRef, JObject, JString, JValue},
    JNIEnv, JavaVM,
};

/// The folder of the inbox under the app's files directory.
const INBOX_FOLDER: &str = "azul-inbox";
/// How long the worker waits for the App to be registered (the intent that started the app comes before `App::run`).
const WAIT_FOR_APP: Duration = Duration::from_secs(10);

const EXTRA_STREAM: &str = "android.intent.extra.STREAM";
const EXTRA_TEXT: &str = "android.intent.extra.TEXT";

/// A call that answers a possibly-null `String`.
fn string_of(env: &mut JNIEnv<'_>, object: &JObject<'_>, name: &str) -> Option<String> {
    let value = env.call_method(object, name, "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
    if value.is_null() {
        return None;
    }
    env.get_string(&JString::from(value)).ok().map(Into::into)
}

/// `object.toString()` of a possibly-null object.
fn to_text(env: &mut JNIEnv<'_>, object: &JObject<'_>) -> Option<String> {
    if object.is_null() {
        return None;
    }
    string_of(env, object, "toString")
}

/// A JNI result as an option: a failure clears the exception the call left pending, because the next JNI call on this thread must
/// not run with one pending (and the native method must not return to Java with one: it would be thrown at the caller).
fn tame<T>(env: &mut JNIEnv<'_>, result: Result<T, jni::errors::Error>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            None
        }
    }
}

/// The fields of `intent` the classifier reads; a field that cannot be read is empty (and the exception that came with it is
/// cleared, so it cannot poison the next call).
pub(crate) fn read_intent(env: &mut JNIEnv<'_>, intent: &JObject<'_>) -> IntentInfo {
    let mut info = IntentInfo {
        action: string_of(env, intent, "getAction").unwrap_or_default(),
        mime: string_of(env, intent, "getType").unwrap_or_default(),
        data: string_of(env, intent, "getDataString"),
        ..IntentInfo::default()
    };
    let flags = env.call_method(intent, "getFlags", "()I", &[]).and_then(|v| v.i());
    info.flags = tame(env, flags).unwrap_or(0);
    let limit = open_intent_limit();

    // EXTRA_TEXT may be any CharSequence
    let key = env.new_string(EXTRA_TEXT);
    if let Some(key) = tame(env, key) {
        let value = env
            .call_method(intent, "getCharSequenceExtra", "(Ljava/lang/String;)Ljava/lang/CharSequence;", &[JValue::Object(&key)])
            .and_then(|v| v.l());
        if let Some(value) = tame(env, value) {
            info.text = to_text(env, &value);
        }
    }
    let key = env.new_string(EXTRA_STREAM);
    if let Some(key) = tame(env, key) {
        // one stream (SEND)
        let value = env
            .call_method(intent, "getParcelableExtra", "(Ljava/lang/String;)Landroid/os/Parcelable;", &[JValue::Object(&key)])
            .and_then(|v| v.l());
        if let Some(value) = tame(env, value) {
            if !value.is_null() && env.is_instance_of(&value, "android/net/Uri").unwrap_or(false) {
                info.streams.extend(to_text(env, &value));
            }
        }
        // a list of streams (SEND_MULTIPLE)
        let list = env
            .call_method(
                intent,
                "getParcelableArrayListExtra",
                "(Ljava/lang/String;)Ljava/util/ArrayList;",
                &[JValue::Object(&key)],
            )
            .and_then(|v| v.l());
        if let Some(list) = tame(env, list) {
            if !list.is_null() {
                let count = env.call_method(&list, "size", "()I", &[]).and_then(|v| v.i());
                let count = tame(env, count).unwrap_or(0);
                for i in 0..count.min(limit) {
                    let item = env
                        .call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])
                        .and_then(|v| v.l());
                    if let Some(item) = tame(env, item) {
                        if env.is_instance_of(&item, "android/net/Uri").unwrap_or(false) {
                            info.streams.extend(to_text(env, &item));
                        }
                        let _ = env.delete_local_ref(item);
                    }
                }
            }
        }
    }
    // the clip data of the intent
    let clip = env
        .call_method(intent, "getClipData", "()Landroid/content/ClipData;", &[])
        .and_then(|v| v.l());
    if let Some(clip) = tame(env, clip) {
        if !clip.is_null() {
            let count = env.call_method(&clip, "getItemCount", "()I", &[]).and_then(|v| v.i());
            let count = tame(env, count).unwrap_or(0);
            for i in 0..count.min(limit) {
                let item = env
                    .call_method(&clip, "getItemAt", "(I)Landroid/content/ClipData$Item;", &[JValue::Int(i)])
                    .and_then(|v| v.l());
                let Some(item) = tame(env, item) else {
                    continue;
                };
                let uri = env.call_method(&item, "getUri", "()Landroid/net/Uri;", &[]).and_then(|v| v.l());
                if let Some(uri) = tame(env, uri) {
                    info.clip_uris.extend(to_text(env, &uri));
                    let _ = env.delete_local_ref(uri);
                }
                let _ = env.delete_local_ref(item);
            }
        }
    }
    info
}

/// The most items read off one intent (a hostile sender cannot make the UI thread walk a million of them).
fn open_intent_limit() -> i32 {
    i32::try_from(azul_core::open_files::MAX_ITEMS * 2).unwrap_or(128)
}

/// `AzulOpenIntent.nativeOnIntent(activity, intent, launchedApp)`: an intent reached the activity. UI thread: read, classify, ask
/// for the persistable grants, hand the copying to a worker. Never throws into Java.
///
/// # Safety
///
/// Called by the JVM with a valid environment and live local references.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_open_AzulOpenIntent_nativeOnIntent(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    activity: jni::sys::jobject,
    intent: jni::sys::jobject,
    launched_app: jni::sys::jboolean,
) {
    if raw_env.is_null() || activity.is_null() || intent.is_null() {
        return;
    }
    let Ok(mut env) = (unsafe { JNIEnv::from_raw(raw_env) }) else {
        return;
    };
    let activity = unsafe { JObject::from_raw(activity) };
    let intent = unsafe { JObject::from_raw(intent) };
    let info = read_intent(&mut env, &intent);
    let Some(plan) = open_intent::classify(&info) else {
        // a launch from the launcher, or an action this app does not take: nothing for the app to open
        return;
    };
    crate::plog_info!(
        "[open] {}: {} file(s), {} link(s), {} text(s){}",
        info.action,
        plan.files.len(),
        plan.links.len(),
        plan.texts.len(),
        if launched_app != 0 { " (the intent that started the app)" } else { "" }
    );
    let Some(job) = prepare(&mut env, &activity, plan, info.flags, launched_app != 0) else {
        return;
    };
    if let Err(e) = std::thread::Builder::new()
        .name("azul-open".to_string())
        .spawn(move || run(job))
    {
        crate::plog_warn!("[open] cannot start the worker that copies the files: {e}");
    }
    // an exception left pending would be thrown at the caller in Java when this returns
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// What the worker needs, owned: nothing of the UI thread's local references.
struct Job {
    vm: JavaVM,
    resolver: GlobalRef,
    plan: OpenPlan,
    flags: i32,
    launched_app: bool,
    inbox: PathBuf,
}

/// The persistable grants, the content resolver and the files directory, taken on the UI thread (`None` when the JVM would not
/// say: the request is dropped with the reason in the log).
fn prepare(env: &mut JNIEnv<'_>, activity: &JObject<'_>, plan: OpenPlan, flags: i32, launched_app: bool) -> Option<Job> {
    let vm = env.get_java_vm().ok()?;
    let resolver = env
        .call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])
        .and_then(|v| v.l())
        .ok()?;
    if plan.persistable {
        for uri in &plan.files {
            let granted = flags & (open_intent::FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION);
            let result = env.with_local_frame(8, |env| {
                let text = env.new_string(uri)?;
                let parsed = env
                    .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&text)])?
                    .l()?;
                env.call_method(
                    &resolver,
                    "takePersistableUriPermission",
                    "(Landroid/net/Uri;I)V",
                    &[JValue::Object(&parsed), JValue::Int(granted)],
                )?;
                Ok::<(), jni::errors::Error>(())
            });
            if result.is_err() {
                // the provider did not offer it after all (a SecurityException): the copy is made all the same
                if env.exception_check().unwrap_or(false) {
                    let _ = env.exception_clear();
                }
                crate::plog_debug!("[open] no persistable grant for {uri}");
            }
        }
    }
    let files_dir = env
        .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
        .and_then(|v| v.l())
        .ok()
        .and_then(|dir| string_of(env, &dir, "getAbsolutePath"))?;
    let resolver = env.new_global_ref(&resolver).ok()?;
    Some(Job {
        vm,
        resolver,
        plan,
        flags,
        launched_app,
        inbox: PathBuf::from(files_dir).join(INBOX_FOLDER),
    })
}

/// A PICKER's answer (`ACTION_OPEN_DOCUMENT`) imported the way a file the system opens the app with is: the persistable grants asked
/// where the picker offered them, the files copied into the inbox (`<files>/azul-inbox/<token>/<name>`, with the same limits and the
/// same reasons for an item that could not be copied) on a WORKER thread, and `done` called there with the request that holds one
/// item per file. The one thing `done` must not do is wait for the UI thread. `Err` when the worker could not be started or the
/// content resolver could not be reached (nothing was started; `done` is not called).
///
/// # Errors
///
/// Why nothing was started.
pub(crate) fn import_picked(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    plan: OpenPlan,
    flags: i32,
    done: impl FnOnce(Result<OpenRequest, String>) + Send + 'static,
) -> Result<(), String> {
    let Some(job) = prepare(env, activity, plan, flags, false) else {
        return Err("the system's content resolver could not be reached".to_string());
    };
    std::thread::Builder::new()
        .name("azul-pick".to_string())
        .spawn(move || done(build_request(&job)))
        .map(|_| ())
        .map_err(|e| format!("the worker that copies the picked files could not be started: {e}"))
}

/// The worker: copy, build the request, wait for the App, queue it, wake the loop.
fn run(job: Job) {
    let request = match build_request(&job) {
        Ok(request) => request,
        Err(why) => {
            crate::plog_warn!("[open] the request could not be built: {why}");
            return;
        }
    };
    let deadline = Instant::now() + WAIT_FOR_APP;
    loop {
        if let Some(services) = crate::desktop::process_app::services() {
            let items = request.items.as_ref().len();
            if services.open.push(request) {
                crate::plog_info!("[open] {items} item(s) queued for the app's open handler");
                crate::desktop::shell2::android::wake_event_loop();
            }
            return;
        }
        if Instant::now() >= deadline {
            crate::plog_warn!("[open] the App was not registered within {WAIT_FOR_APP:?}: the request is dropped");
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn build_request(job: &Job) -> Result<OpenRequest, String> {
    let mut env = job
        .vm
        .attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e:?}"))?;
    let mut items: Vec<OpenedItem> = Vec::new();
    let writable = job.flags & FLAG_GRANT_WRITE_URI_PERMISSION != 0 && job.plan.action == OpenAction::Edit;

    if !job.plan.files.is_empty() {
        let inbox = Inbox::new(job.inbox.clone());
        let mut request = inbox.begin(OpenLimits::default())?;
        for uri in &job.plan.files {
            let item = env
                .with_local_frame(24, |env| Ok::<OpenedItem, jni::errors::Error>(copy_one(env, &job.resolver, uri, &mut request, writable)))
                .unwrap_or_else(|e| {
                    let _ = env.exception_clear();
                    OpenedItem::file_failed(
                        AzString::from(name_from_uri(uri)),
                        AzString::from(""),
                        OptionU64::None,
                        AzString::from(uri.as_str()),
                        AzString::from(format!("the system could not be asked for this file: {e:?}")),
                    )
                });
            items.push(item);
        }
    }
    items.extend(job.plan.links.iter().map(|u| OpenedItem::url(AzString::from(u.as_str()))));
    items.extend(job.plan.texts.iter().map(|t| OpenedItem::text(AzString::from(t.as_str()))));
    Ok(OpenRequest {
        action: job.plan.action,
        items: OpenedItemVec::from_vec(items),
        launched_app: job.launched_app,
        skipped: u32::try_from(job.plan.truncated).unwrap_or(u32::MAX),
    })
}

/// The last path segment of a URI, percent-decoded: what an item is called when the provider gives no name.
fn name_from_uri(uri: &str) -> String {
    let last = uri.rsplit('/').next().unwrap_or(uri);
    azul_layout::managers::share_area::percent_decode(last).unwrap_or_else(|| last.to_string())
}

/// Ask the content resolver for one URI and copy it into the request's folder; every failure is an item with its reason.
fn copy_one(
    env: &mut JNIEnv<'_>,
    resolver: &GlobalRef,
    uri: &str,
    request: &mut azul_layout::open_inbox::InboxRequest,
    writable: bool,
) -> OpenedItem {
    let fail = |name: &str, mime: &str, size: Option<u64>, why: String| {
        OpenedItem::file_failed(
            AzString::from(name),
            AzString::from(mime),
            size.map_or(OptionU64::None, OptionU64::Some),
            AzString::from(uri),
            AzString::from(why),
        )
    };
    let parsed = match parse_uri(env, uri) {
        Ok(parsed) => parsed,
        Err(e) => return fail(&name_from_uri(uri), "", None, format!("{uri:?} is not a URI the system can read: {e:?}")),
    };
    let (display_name, size) = query_name_and_size(env, resolver, &parsed);
    let name = display_name.unwrap_or_else(|| name_from_uri(uri));
    let reported_type = env
        .call_method(resolver, "getType", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(&parsed)])
        .and_then(|v| v.l())
        .ok()
        .and_then(|value| (!value.is_null()).then(|| env.get_string(&JString::from(value)).ok().map(String::from)).flatten());
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    let kind = match reported_type {
        Some(kind) if !kind.is_empty() && kind != mime::OCTET_STREAM => kind,
        _ => mime::for_file_name(&name).to_string(),
    };

    let mode = match env.new_string("r") {
        Ok(mode) => mode,
        Err(e) => return fail(&name, &kind, size, format!("{e:?}")),
    };
    let descriptor = env
        .call_method(
            resolver,
            "openFileDescriptor",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
            &[JValue::Object(&parsed), JValue::Object(&mode)],
        )
        .and_then(|v| v.l());
    let descriptor = match descriptor {
        Ok(d) if !d.is_null() => d,
        _ => {
            let why = take_exception_text(env).unwrap_or_else(|| "the provider returned nothing".to_string());
            return fail(&name, &kind, size, format!("{name} could not be opened: {why}"));
        }
    };
    let fd = match env.call_method(&descriptor, "detachFd", "()I", &[]).and_then(|v| v.i()) {
        Ok(fd) if fd >= 0 => fd,
        other => return fail(&name, &kind, size, format!("{name}: no file descriptor ({other:?})")),
    };
    // SAFETY: `detachFd` hands over the ownership of an open descriptor: this `File` closes it.
    let mut file = unsafe { File::from_raw_fd(fd) };
    let size = size.or_else(|| file.metadata().ok().map(|m| m.len()).filter(|len| *len > 0));
    match request.copy_in(&name, &mut file, size) {
        Ok(copied) => OpenedItem::file(
            AzString::from(copied.name),
            AzString::from(kind),
            OptionU64::Some(copied.size),
            AzString::from(copied.path.to_string_lossy().into_owned()),
            AzString::from(uri),
            writable,
        ),
        Err(why) => fail(&name, &kind, size, why),
    }
}

fn parse_uri<'a>(env: &mut JNIEnv<'a>, text: &str) -> Result<JObject<'a>, jni::errors::Error> {
    let text = env.new_string(text)?;
    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&text)])?
        .l()
}

/// `(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)` of `uri`, either `None` when the provider does not say (or fails).
fn query_name_and_size(env: &mut JNIEnv<'_>, resolver: &GlobalRef, uri: &JObject<'_>) -> (Option<String>, Option<u64>) {
    let null = JObject::null();
    let cursor = env
        .call_method(
            resolver,
            "query",
            "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
            &[
                JValue::Object(uri),
                JValue::Object(&null),
                JValue::Object(&null),
                JValue::Object(&null),
                JValue::Object(&null),
            ],
        )
        .and_then(|v| v.l());
    let Ok(cursor) = cursor else {
        let _ = env.exception_clear();
        return (None, None);
    };
    if cursor.is_null() {
        return (None, None);
    }
    let mut name = None;
    let mut size = None;
    let moved = env.call_method(&cursor, "moveToFirst", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false);
    if moved {
        for (column, is_name) in [("_display_name", true), ("_size", false)] {
            let Ok(key) = env.new_string(column) else {
                continue;
            };
            let index = env
                .call_method(&cursor, "getColumnIndex", "(Ljava/lang/String;)I", &[JValue::Object(&key)])
                .and_then(|v| v.i())
                .unwrap_or(-1);
            if index < 0 {
                continue;
            }
            if is_name {
                name = env
                    .call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(index)])
                    .and_then(|v| v.l())
                    .ok()
                    .and_then(|value| (!value.is_null()).then(|| env.get_string(&JString::from(value)).ok().map(String::from)).flatten());
            } else {
                let null = env.call_method(&cursor, "isNull", "(I)Z", &[JValue::Int(index)]).and_then(|v| v.z()).unwrap_or(true);
                if !null {
                    size = env
                        .call_method(&cursor, "getLong", "(I)J", &[JValue::Int(index)])
                        .and_then(|v| v.j())
                        .ok()
                        .and_then(|len| u64::try_from(len).ok());
                }
            }
        }
    }
    let _ = env.call_method(&cursor, "close", "()V", &[]);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    (name, size)
}

/// The text of the exception a failed call left pending (cleared), if any.
fn take_exception_text(env: &mut JNIEnv<'_>) -> Option<String> {
    if !env.exception_check().unwrap_or(false) {
        return None;
    }
    let throwable = env.exception_occurred().ok()?;
    let _ = env.exception_clear();
    to_text(env, &throwable)
}
