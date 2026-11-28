//! Android file picker via the Storage Access Framework.
//!
//! The intents are built and started FROM RUST through JNI; the only Java left is the activity-result proxy the activity forces
//! (`scripts/android/AzulFilePicker.java`: `onActivityResult` of `AzulActivity` calls it, it calls the one native below with the
//! result `Intent`), and it keeps no state. The state of a request on the screen is a [`PendingPick`] parked in the App's own
//! registry (`AppServices::pickers`) under an id that rides in the low bits of the request code: no process-wide map, no counter.
//!
//! What happens, on the thread the callback runs on and then on the UI thread:
//!
//! 1. [`dispatch_open_file`] parks the request's [`FilePickerHandle`] in the registry, builds an `ACTION_OPEN_DOCUMENT` intent
//!    (`CATEGORY_OPENABLE`, the types of the filter, `EXTRA_ALLOW_MULTIPLE`) and starts it with
//!    `Activity.startActivityForResult(intent, REQUEST_CODE_BASE + id)`. A failure takes the request back out and answers it
//!    with the reason;
//! 2. the result comes to `AzulActivity.onActivityResult`, which asks `AzulFilePicker.onActivityResultProxy`; a request code
//!    that is ours reaches [`Java_com_azul_picker_AzulFilePicker_nativeOnActivityResult`];
//! 3. there the request is taken back out of the registry (an answer that comes twice finds nothing), a cancel is answered at
//!    once, and the picked URIs are IMPORTED the way a file the system opens the app with is
//!    (`open_files::android::import_picked`): copied on a worker thread into the app's inbox
//!    (`<files>/azul-inbox/<token>/<name>`, the same limits, a reason for every file that could not be copied) and answered
//!    as paths of those working copies. The UI thread copies nothing (five seconds of it is an ANR).
//!
//! Permissions: SAF intents grant per-URI read permission through the intent's flags; no storage permission is asked for.

#![allow(non_snake_case)]

use azul_css::{corety::OptionString, AzString, OptionStringVec, StringVec};

use super::{FilePickerHandle, FilePickerStatus};

/// The base of the request codes the picker uses: the id of the request rides in the low 16 bits (`AzulFilePicker.java` keeps the
/// same base and routes only the codes of this window to the native below).
#[cfg(target_os = "android")]
const REQUEST_CODE_BASE: i32 = 0x004A_5400;
/// `Activity.RESULT_OK`.
#[cfg(target_os = "android")]
const RESULT_OK: i32 = -1;
#[cfg(target_os = "android")]
const ACTION_OPEN_DOCUMENT: &str = "android.intent.action.OPEN_DOCUMENT";
#[cfg(target_os = "android")]
const CATEGORY_OPENABLE: &str = "android.intent.category.OPENABLE";
#[cfg(target_os = "android")]
const EXTRA_MIME_TYPES: &str = "android.intent.extra.MIME_TYPES";
#[cfg(target_os = "android")]
const EXTRA_ALLOW_MULTIPLE: &str = "android.intent.extra.ALLOW_MULTIPLE";

#[cfg(target_os = "android")]
const ACTION_OPEN_DOCUMENT_TREE: &str = "android.intent.action.OPEN_DOCUMENT_TREE";
/// `Intent.FLAG_GRANT_PREFIX_URI_PERMISSION`: the grant covers every document under the tree's URI.
#[cfg(target_os = "android")]
const FLAG_GRANT_PREFIX_URI_PERMISSION: i32 = 0x80;
/// How many persisted grants are normal: the system keeps 128 (512 from Android 11), and a person has a handful of folders. Past
/// this the log says so; at the system's limit the grant is refused and the request answers with the reason.
#[cfg(target_os = "android")]
const MANY_GRANTS: usize = 100;

/// What a picker on the screen was asked for.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickKind {
    /// A file (or several): imported as working copies.
    Open,
    /// A folder: the grant is kept and the answer is the locator `Documents` takes.
    Folder,
}

/// What a picker on the screen waits for: the handle the app polls.
#[cfg(target_os = "android")]
struct PendingPick {
    handle: FilePickerHandle,
    kind: PickKind,
}

/// Map a `*.png` / `image/png` / `pdf` descriptor onto its MIME type.
/// Unknown extensions fall back to `application/octet-stream` (SAF accepts
/// it but the filter won't pre-select anything).
#[cfg(target_os = "android")]
fn descriptor_to_mime(descriptor: &str) -> String {
    if descriptor.contains('/') {
        // Already a MIME type.
        return descriptor.to_owned();
    }
    let trimmed = descriptor
        .trim_start_matches('*')
        .trim_start_matches('.')
        .to_ascii_lowercase();
    azul_layout::mime::for_extension(&trimmed)
        .unwrap_or(azul_layout::mime::OCTET_STREAM)
        .to_owned()
}

/// The distinct MIME types of the filter, in order.
#[cfg(target_os = "android")]
fn filter_mimes(filter_descriptors: &OptionStringVec) -> Vec<String> {
    let mut mimes: Vec<String> = Vec::new();
    if let Some(list) = filter_descriptors.as_option() {
        for descriptor in list.as_ref() {
            let mime = descriptor_to_mime(descriptor.as_str());
            if !mimes.contains(&mime) {
                mimes.push(mime);
            }
        }
    }
    mimes
}

/// The text of the exception a failed call left pending (cleared), or what JNI itself said.
#[cfg(target_os = "android")]
fn reason(env: &mut jni::JNIEnv<'_>, error: &jni::errors::Error) -> String {
    if env.exception_check().unwrap_or(false) {
        if let Ok(throwable) = env.exception_occurred() {
            let _ = env.exception_clear();
            if let Ok(text) = env
                .call_method(&throwable, "toString", "()Ljava/lang/String;", &[])
                .and_then(|v| v.l())
            {
                if let Ok(text) = env.get_string(&jni::objects::JString::from(text)) {
                    return text.into();
                }
            }
        }
        let _ = env.exception_clear();
    }
    format!("{error}")
}

/// Attach the current thread to the published JavaVM and call a closure
/// with the resulting `JNIEnv`. Returns `Err` if the VM hasn't been
/// published yet (e.g. dispatch happened before `android_main`).
#[cfg(target_os = "android")]
pub(crate) fn with_env<R>(
    f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Result<R, jni::errors::Error>,
) -> Result<R, String> {
    use jni::JavaVM;

    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    if vm_ptr.is_null() {
        return Err("JavaVM not yet published".to_owned());
    }
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if activity_ptr.is_null() {
        return Err("Activity reference not yet published".to_owned());
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
        .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e:?}"))?;
    // Released when this helper returns: this thread never returns to Java (see `LocalFrame`).
    let _frame = crate::desktop::extra::LocalFrame::push(&env, crate::desktop::extra::LocalFrame::DEFAULT_CAPACITY);
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut env, activity).map_err(|e| format!("jni call: {e:?}"))
}

/// The intent of an open request: `ACTION_OPEN_DOCUMENT` for the types of the filter.
#[cfg(target_os = "android")]
fn open_intent<'a>(
    env: &mut jni::JNIEnv<'a>,
    mimes: &[String],
    allow_multiple: bool,
) -> Result<jni::objects::JObject<'a>, jni::errors::Error> {
    use jni::objects::{JObject, JValue};

    let action = env.new_string(ACTION_OPEN_DOCUMENT)?;
    let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
    let category = env.new_string(CATEGORY_OPENABLE)?;
    env.call_method(&intent, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&category)])?;
    // read access to what is picked; the grants are not kept (the file is COPIED: a folder is what a persisted grant is for)
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(azul_layout::open_intent::FLAG_GRANT_READ_URI_PERMISSION)],
    )?;
    let single = if let [only] = mimes {
        only.as_str()
    } else {
        "*/*"
    };
    let kind = env.new_string(single)?;
    env.call_method(&intent, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&kind)])?;
    if mimes.len() > 1 {
        let class = env.find_class("java/lang/String")?;
        let null = JObject::null();
        let array = env.new_object_array(i32::try_from(mimes.len()).unwrap_or(i32::MAX), &class, &null)?;
        for (index, mime) in (0_i32..).zip(mimes) {
            let text = env.new_string(mime)?;
            env.set_object_array_element(&array, index, &text)?;
        }
        let key = env.new_string(EXTRA_MIME_TYPES)?;
        env.call_method(
            &intent,
            "putExtra",
            "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Object(&array)],
        )?;
    }
    if allow_multiple {
        let key = env.new_string(EXTRA_ALLOW_MULTIPLE)?;
        env.call_method(
            &intent,
            "putExtra",
            "(Ljava/lang/String;Z)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Bool(1)],
        )?;
    }
    Ok(intent)
}

/// The intent of a folder request: `ACTION_OPEN_DOCUMENT_TREE` with a grant to read and write everything under the tree that the
/// person can ask the system to keep.
#[cfg(target_os = "android")]
fn folder_intent<'a>(env: &mut jni::JNIEnv<'a>) -> Result<jni::objects::JObject<'a>, jni::errors::Error> {
    use jni::objects::JValue;

    let action = env.new_string(ACTION_OPEN_DOCUMENT_TREE)?;
    let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
    let flags = azul_layout::open_intent::FLAG_GRANT_READ_URI_PERMISSION
        | azul_layout::open_intent::FLAG_GRANT_WRITE_URI_PERMISSION
        | azul_layout::open_intent::FLAG_GRANT_PERSISTABLE_URI_PERMISSION
        | FLAG_GRANT_PREFIX_URI_PERMISSION;
    env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(flags)])?;
    Ok(intent)
}

/// Park the request, build its intent with `build` and start it; a failure takes the request back out and answers it with the reason.
#[cfg(target_os = "android")]
fn start_pick(
    handle: &FilePickerHandle,
    kind: PickKind,
    build: impl for<'a> FnOnce(&mut jni::JNIEnv<'a>) -> Result<jni::objects::JObject<'a>, jni::errors::Error>,
) {
    let Some(services) = crate::desktop::process_app::services() else {
        super::fail(handle, "the app has not started its services yet, so the picker cannot be tracked");
        return;
    };
    let id = match services.pickers.register(PendingPick { handle: handle.clone(), kind }) {
        Ok(id) => id,
        Err(_) => {
            super::fail(handle, "too many pickers are waiting for an answer: finish or cancel one first");
            return;
        }
    };
    let started = with_env(|env, activity| {
        let intent = match build(env) {
            Ok(intent) => intent,
            Err(e) => return Ok(Err(reason(env, &e))),
        };
        let code = REQUEST_CODE_BASE + i32::try_from(id).unwrap_or(0);
        let sent = env.call_method(
            &activity,
            "startActivityForResult",
            "(Landroid/content/Intent;I)V",
            &[jni::objects::JValue::Object(&intent), jni::objects::JValue::Int(code)],
        );
        Ok(sent.map(|_| ()).map_err(|e| reason(env, &e)))
    });
    let failed = match started {
        Ok(Ok(())) => return,
        Ok(Err(why)) => format!("the picker could not be shown: {why}"),
        Err(why) => why,
    };
    // never started: the request does not wait for an answer that cannot come
    drop(services.pickers.take::<PendingPick>(id));
    super::fail(handle, &failed);
}

/// A folder request: the system's tree picker, whose grant is kept (see [`status_of_folder`]).
#[cfg(target_os = "android")]
pub fn dispatch_open_folder(handle: FilePickerHandle, _title: AzString, _default_path: OptionString) {
    start_pick(&handle, PickKind::Folder, folder_intent);
}

#[cfg(not(target_os = "android"))]
pub fn dispatch_open_folder(handle: FilePickerHandle, _title: AzString, _default_path: OptionString) {
    handle.set_status(FilePickerStatus::Cancelled);
}

#[cfg(target_os = "android")]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
    filter_descriptors: OptionStringVec,
    allow_multiple: bool,
) {
    let mimes = filter_mimes(&filter_descriptors);
    start_pick(&handle, PickKind::Open, move |env| open_intent(env, &mimes, allow_multiple));
}

#[cfg(not(target_os = "android"))]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
    _filter_descriptors: OptionStringVec,
    _allow_multiple: bool,
) {
    handle.set_status(FilePickerStatus::Cancelled);
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// The status of a finished import: the paths of the working copies, or the reason none could be made.
#[cfg(target_os = "android")]
fn status_of_import(request: Result<azul_core::open_files::OpenRequest, String>) -> FilePickerStatus {
    let request = match request {
        Ok(request) => request,
        Err(why) => return FilePickerStatus::Error(AzString::from(why)),
    };
    let mut paths: Vec<AzString> = Vec::new();
    let mut first_reason: Option<String> = None;
    for item in request.items.as_ref() {
        match item.path.as_ref() {
            Some(path) => paths.push(path.clone()),
            None => {
                let why = item
                    .error
                    .as_ref()
                    .map_or_else(|| "the file could not be read".to_string(), |e| format!("{}: {}", item.name.as_str(), e.as_str()));
                crate::plog_warn!("[file_picker] a picked file was not imported: {why}");
                first_reason.get_or_insert(why);
            }
        }
    }
    match paths.len() {
        0 => FilePickerStatus::Error(AzString::from(
            first_reason.unwrap_or_else(|| "nothing was picked".to_string()),
        )),
        1 => FilePickerStatus::Selected(paths.remove(0)),
        _ => FilePickerStatus::SelectedMultiple(StringVec::from(paths)),
    }
}

/// Keep the grant of the folder the person chose (`takePersistableUriPermission`, asked while the grant of the activity result is
/// fresh) and answer with its locator: the tree's root document, as `Documents` takes it.
#[cfg(target_os = "android")]
fn status_of_folder(
    env: &mut jni::JNIEnv<'_>,
    activity: &jni::objects::JObject<'_>,
    info: &azul_layout::open_intent::IntentInfo,
) -> FilePickerStatus {
    use azul_layout::{documents_saf::Locator, open_intent};
    use jni::objects::JValue;

    let Some(uri) = info.data.as_deref() else {
        return FilePickerStatus::Cancelled;
    };
    let Some(tree @ Locator::Tree { .. }) = Locator::parse(uri) else {
        return FilePickerStatus::Error(AzString::from(format!(
            "the system answered with something that is not a folder the app can keep: {uri}"
        )));
    };
    let granted = info.flags & (open_intent::FLAG_GRANT_READ_URI_PERMISSION | open_intent::FLAG_GRANT_WRITE_URI_PERMISSION);
    // an answer that does not echo the flags still granted what was asked for
    let take = if granted == 0 {
        open_intent::FLAG_GRANT_READ_URI_PERMISSION | open_intent::FLAG_GRANT_WRITE_URI_PERMISSION
    } else {
        granted
    };
    let kept = (|| {
        let resolver = env
            .call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?
            .l()?;
        let text = env.new_string(uri)?;
        let parsed = env
            .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&text)])?
            .l()?;
        let held = env
            .call_method(&resolver, "getPersistedUriPermissions", "()Ljava/util/List;", &[])
            .and_then(|list| list.l())
            .and_then(|list| env.call_method(&list, "size", "()I", &[]))
            .and_then(|count| count.i())
            .unwrap_or(0);
        if usize::try_from(held).unwrap_or(0) >= MANY_GRANTS {
            crate::plog_warn!(
                "[file_picker] {held} folders are kept already: the system keeps at most 128 (512 from Android 11); one more may be refused"
            );
        }
        env.call_method(
            &resolver,
            "takePersistableUriPermission",
            "(Landroid/net/Uri;I)V",
            &[JValue::Object(&parsed), JValue::Int(take)],
        )?;
        Ok::<(), jni::errors::Error>(())
    })();
    match kept {
        Ok(()) => FilePickerStatus::Selected(AzString::from(tree.document_uri())),
        Err(e) => FilePickerStatus::Error(AzString::from(format!(
            "access to the folder could not be kept ({}): choose it again, or another one",
            reason(env, &e)
        ))),
    }
}

/// `AzulFilePicker.nativeOnActivityResult(activity, ticket, resultCode, data)`: an activity result of the window of request codes
/// the picker owns (UI thread). A cancel is answered here; the picked files are imported by a worker. Never throws into Java.
///
/// # Safety
///
/// Called by the JVM with a valid environment and live local references.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_picker_AzulFilePicker_nativeOnActivityResult(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    activity: jni::sys::jobject,
    ticket: jni::sys::jint,
    result_code: jni::sys::jint,
    data: jni::sys::jobject,
) {
    use jni::objects::JObject;

    if raw_env.is_null() {
        return;
    }
    let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(raw_env) }) else {
        return;
    };
    let Some(services) = crate::desktop::process_app::services() else {
        return;
    };
    let Some(pending) = services.pickers.take::<PendingPick>(u64::try_from(ticket).unwrap_or(0)) else {
        // an answer that comes twice, or for a request that was taken for lost
        crate::plog_warn!("[file_picker] an activity result for request {ticket}, which nobody waits for");
        return;
    };
    let (handle, kind) = (pending.handle, pending.kind);
    if result_code != RESULT_OK || data.is_null() {
        handle.set_status(FilePickerStatus::Cancelled);
        return;
    }
    let activity = unsafe { JObject::from_raw(activity) };
    let data = unsafe { JObject::from_raw(data) };
    let info = crate::desktop::extra::open_files::android::read_intent(&mut env, &data);
    if kind == PickKind::Folder {
        handle.set_status(status_of_folder(&mut env, &activity, &info));
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        return;
    }
    let Some(plan) = azul_layout::open_intent::classify_pick(&info) else {
        // a result that holds no file: nothing was picked
        handle.set_status(FilePickerStatus::Cancelled);
        return;
    };
    let answer = handle.clone();
    let started = crate::desktop::extra::open_files::android::import_picked(&mut env, &activity, plan, info.flags, move |request| {
        answer.set_status(status_of_import(request));
    });
    if let Err(why) = started {
        super::fail(&handle, &why);
    }
    // an exception left pending would be thrown at the caller in Java when this returns
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}
