//! Android file picker via Storage Access Framework.
//!
//! Flow:
//!
//! 1. `dispatch_open_file` registers the caller's `FilePickerHandle` in
//!    `PENDING_PICKERS` keyed by a fresh request ID.
//! 2. We use the `jni` crate to attach the current thread to the
//!    JavaVM* published by `shell2::android::publish_jni_context`,
//!    find the `com.azul.picker.AzulFilePicker` class, and invoke
//!    `pickDocument(activity, requestId, mimeTypes, allowMultiple)`.
//!    The Java side fires `Intent.ACTION_OPEN_DOCUMENT`.
//! 3. `AzulActivity.onActivityResult` routes the result back to
//!    `AzulFilePicker.onActivityResultProxy`, which reads each
//!    `content://` URI, copies it into the app's cache dir (so the
//!    caller gets a regular `file://`-style path), and calls
//!    `nativeOnResult(requestId, paths, errorOrNull)`.
//! 4. The `nativeOnResult` JNI symbol below pops the handle out of
//!    `PENDING_PICKERS` and writes the resulting status.
//!
//! Permissions: SAF intents grant per-URI read permission via the
//! intent flags — no `READ_EXTERNAL_STORAGE` / `READ_MEDIA_*` is
//! required for the picker itself.

#![allow(non_snake_case)]

#[cfg(target_os = "android")]
use std::collections::BTreeMap;
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "android")]
use std::sync::Mutex;

use azul_css::{corety::OptionString, AzString, OptionStringVec, StringVec};

use super::{FilePickerHandle, FilePickerStatus};

#[cfg(target_os = "android")]
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "android")]
static PENDING_PICKERS: Mutex<BTreeMap<u64, FilePickerHandle>> = Mutex::new(BTreeMap::new());

#[cfg(target_os = "android")]
fn allocate_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(target_os = "android")]
fn register_handle(handle: FilePickerHandle) -> u64 {
    let id = allocate_request_id();
    if let Ok(mut g) = PENDING_PICKERS.lock() {
        g.insert(id, handle);
    }
    id
}

#[cfg(target_os = "android")]
fn pop_handle(request_id: u64) -> Option<FilePickerHandle> {
    PENDING_PICKERS.lock().ok().and_then(|mut g| g.remove(&request_id))
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
    match trimmed.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" | "heif" => "image/heic",
        "pdf" => "application/pdf",
        "txt" | "text" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "text/xml",
        "zip" => "application/zip",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_owned()
}

#[cfg(target_os = "android")]
fn build_mime_array<'a>(
    env: &mut jni::JNIEnv<'a>,
    filter_descriptors: &OptionStringVec,
) -> Result<jni::objects::JObjectArray<'a>, jni::errors::Error> {
    let mut mimes: Vec<String> = Vec::new();
    if let Some(list) = filter_descriptors.as_option() {
        for s in list.as_ref().iter() {
            let m = descriptor_to_mime(s.as_str());
            if !mimes.contains(&m) {
                mimes.push(m);
            }
        }
    }
    let str_class = env.find_class("java/lang/String")?;
    let null_string = jni::objects::JObject::null();
    let arr = env.new_object_array(mimes.len() as i32, &str_class, &null_string)?;
    for (i, m) in mimes.iter().enumerate() {
        let jstr = env.new_string(m)?;
        env.set_object_array_element(&arr, i as i32, &jstr)?;
    }
    Ok(arr)
}

/// Attach the current thread to the published JavaVM and call a closure
/// with the resulting `JNIEnv`. Returns `Err` if the VM hasn't been
/// published yet (e.g. dispatch happened before `android_main`).
#[cfg(target_os = "android")]
fn with_env<R>(f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Result<R, jni::errors::Error>)
    -> Result<R, String>
{
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
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("attach_current_thread: {e:?}"))?;
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut env, activity).map_err(|e| format!("jni call: {e:?}"))
}

#[cfg(target_os = "android")]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
    filter_descriptors: OptionStringVec,
    allow_multiple: bool,
) {
    let request_id = register_handle(handle.clone());

    let result = with_env(|env, activity| {
        let mimes = build_mime_array(env, &filter_descriptors)?;
        let class = env.find_class("com/azul/picker/AzulFilePicker")?;
        env.call_static_method(
            class,
            "pickDocument",
            "(Landroid/app/Activity;J[Ljava/lang/String;Z)V",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Long(request_id as i64),
                jni::objects::JValue::Object(&mimes),
                jni::objects::JValue::Bool(allow_multiple as u8),
            ],
        )?;
        Ok(())
    });

    if let Err(msg) = result {
        let _ = pop_handle(request_id);
        handle.set_status(FilePickerStatus::Error {
            message: AzString::from(msg),
        });
    }
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

#[cfg(target_os = "android")]
pub fn dispatch_save_file(
    handle: FilePickerHandle,
    title: AzString,
    _default_path: OptionString,
) {
    let request_id = register_handle(handle.clone());

    let result = with_env(|env, activity| {
        let suggested = env.new_string(title.as_str())?;
        let class = env.find_class("com/azul/picker/AzulFilePicker")?;
        env.call_static_method(
            class,
            "saveDocument",
            "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;)V",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Long(request_id as i64),
                jni::objects::JValue::Object(&suggested),
                jni::objects::JValue::Object(&jni::objects::JObject::null()),
            ],
        )?;
        Ok(())
    });

    if let Err(msg) = result {
        let _ = pop_handle(request_id);
        handle.set_status(FilePickerStatus::Error {
            message: AzString::from(msg),
        });
    }
}

#[cfg(not(target_os = "android"))]
pub fn dispatch_save_file(handle: FilePickerHandle, _title: AzString, _default_path: OptionString) {
    handle.set_status(FilePickerStatus::Cancelled);
}

#[cfg(target_os = "android")]
pub fn dispatch_open_directory(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
) {
    let request_id = register_handle(handle.clone());

    let result = with_env(|env, activity| {
        let class = env.find_class("com/azul/picker/AzulFilePicker")?;
        env.call_static_method(
            class,
            "pickDirectory",
            "(Landroid/app/Activity;J)V",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Long(request_id as i64),
            ],
        )?;
        Ok(())
    });

    if let Err(msg) = result {
        let _ = pop_handle(request_id);
        handle.set_status(FilePickerStatus::Error {
            message: AzString::from(msg),
        });
    }
}

#[cfg(not(target_os = "android"))]
pub fn dispatch_open_directory(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
) {
    handle.set_status(FilePickerStatus::Cancelled);
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives the activity-result paths from `AzulFilePicker.nativeOnResult`.
/// Argument JNI signature: `(JLjava/lang/String;Ljava/lang/String;)V`
/// but `paths` is actually a `[Ljava/lang/String;` (the descriptor isn't
/// used by the JVM, just the method name needs to match).
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_picker_AzulFilePicker_nativeOnResult(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    request_id: i64,
    paths: jni::sys::jobjectArray,
    error_or_null: jni::sys::jstring,
) {
    if raw_env.is_null() {
        return;
    }
    let mut env = match jni::JNIEnv::from_raw(raw_env) {
        Ok(e) => e,
        Err(_) => return,
    };

    let handle = match pop_handle(request_id as u64) {
        Some(h) => h,
        None => return,
    };

    // Error string takes precedence — surface to the caller. We
    // materialize the `String` inside a small block so the `JavaStr`
    // (which borrows the local `JString`) drops before the `JString`
    // itself — appeases the borrow checker on jni 0.21.
    if !error_or_null.is_null() {
        let jstr = jni::objects::JString::from_raw(error_or_null);
        let owned: Option<String> = env.get_string(&jstr).ok().map(|s| s.into());
        let msg = owned.unwrap_or_else(|| "AzulFilePicker error (unreadable message)".to_owned());
        handle.set_status(FilePickerStatus::Error {
            message: AzString::from(msg),
        });
        return;
    }

    if paths.is_null() {
        handle.set_status(FilePickerStatus::Cancelled);
        return;
    }
    let arr = jni::objects::JObjectArray::from_raw(paths);
    let len = match env.get_array_length(&arr) {
        Ok(n) => n,
        Err(_) => {
            handle.set_status(FilePickerStatus::Cancelled);
            return;
        }
    };
    if len == 0 {
        handle.set_status(FilePickerStatus::Cancelled);
        return;
    }

    let mut out: Vec<AzString> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let elem = match env.get_object_array_element(&arr, i) {
            Ok(o) => o,
            Err(_) => continue,
        };
        if elem.is_null() {
            continue;
        }
        let jstr = jni::objects::JString::from(elem);
        // Materialize into an owned String inside the .map closure so
        // the JavaStr drops before jstr — works around the borrow-order
        // diagnostic on jni 0.21's if-let pattern.
        let owned: Option<String> = env.get_string(&jstr).ok().map(|s| s.into());
        if let Some(s) = owned {
            out.push(AzString::from(s));
        }
    }

    let status = if out.is_empty() {
        FilePickerStatus::Cancelled
    } else if out.len() == 1 {
        FilePickerStatus::Selected {
            path: OptionString::Some(out.remove(0)),
        }
    } else {
        FilePickerStatus::SelectedMultiple {
            paths: StringVec::from(out),
        }
    };
    handle.set_status(status);
}
