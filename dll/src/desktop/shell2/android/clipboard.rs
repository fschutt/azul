//! Android clipboard transport: `android.content.ClipboardManager` ⇄
//! [`ClipboardPayload`].
//!
//! Android had NO arm in `get_system_clipboard` / `set_system_clipboard` at
//! all: it fell through to the catch-all that answers `None` / `false`, so
//! Copy reported failure (which suppresses Cut's deletion, correctly) and
//! Paste found nothing — on a platform whose clipboard is one of the oldest
//! parts of the framework.
//!
//! # Why the work happens in Java
//!
//! `ClipboardManager` is a Java-only service reached through
//! `Context.getSystemService`; there is no NDK entry point. Open-coding the
//! `getSystemService` → `getPrimaryClip` → `getItemAt(0)` → `coerceToText`
//! chain through reflection from Rust would be four fallible JNI calls per
//! read, so the chain lives in `NativeTextBridge` and this module makes one
//! static call per operation, exactly like the soft-keyboard path.
//!
//! # Flavors
//!
//! `ClipData` speaks MIME types, so the payload is tagged
//! [`Platform::Unix`] — the same vocabulary the X11 and Wayland transports
//! use, which means `text/plain` and `text/html` resolve through the shared
//! flavor table with no Android-specific arm downstream.
//!
//! A clip carries at most one HTML representation
//! (`ClipData.newHtmlText` sets both the markup and its plain-text
//! coercion), so a read produces one item with up to two flavors and a
//! write publishes `newHtmlText` when the payload has markup and
//! `newPlainText` when it does not.
//!
//! # Two platform limits worth knowing
//!
//! * **Reading is focus-gated.** Since Android 10 (API 29) `getPrimaryClip`
//!   returns `null` for an app that does not hold focus, and since Android
//!   12 a read raises a system toast. Both are the platform's choice; a
//!   `None` here can therefore mean "not focused" as well as "empty".
//! * **Writes are silently size-capped.** The system clipboard is a Binder
//!   transaction; a very large clip throws `TransactionTooLargeException` in
//!   the system server rather than at the call site. The Java side catches
//!   `Throwable` and answers `false`, so a failed copy never reports success
//!   and `CutToClipboard` will not delete the selection.

use rich_clipboard::{ClipboardPayload, Platform};

#[cfg(all(target_os = "android", feature = "jni"))]
use crate::{desktop::shell2::common::debug_server::LogCategory, log_debug};

/// `text/plain` — the MIME name `Platform::Unix` maps to `Flavor::PlainText`.
const MIME_TEXT: &str = "text/plain";
/// `text/html` — maps to `Flavor::Html`.
const MIME_HTML: &str = "text/html";

/// Read the primary clip as a typed payload.
///
/// `None` for an empty clipboard, an unfocused app (see the module note), or
/// a build without the `jni` feature.
#[cfg(all(target_os = "android", feature = "jni"))]
pub fn read_payload() -> Option<ClipboardPayload> {
    let text = call_string("getClipboardText")?;
    if text.is_empty() {
        return None;
    }
    let mut payload = ClipboardPayload::new(Platform::Unix).with(MIME_TEXT, text.into_bytes());
    // The markup, when the source published a rich clip. Absent for the
    // overwhelmingly common plain-text clip, which is not an error.
    if let Some(html) = call_string("getClipboardHtml").filter(|h| !h.is_empty()) {
        payload = payload.with(MIME_HTML, html.into_bytes());
    }
    Some(payload)
}

/// Publish a typed payload as the primary clip.
///
/// Returns whether the platform accepted it — `CutToClipboard` gates the
/// deletion of the selected text on this.
#[cfg(all(target_os = "android", feature = "jni"))]
pub fn write_payload(payload: &ClipboardPayload) -> bool {
    use rclip_core::Flavor;

    // The plain text is mandatory: a clip with markup and no coercible text
    // pastes as nothing in every plain-text field on the device.
    let Some(text) = payload
        .get(Flavor::PlainText)
        .and_then(|item| String::from_utf8(item.bytes.clone()).ok())
    else {
        return false;
    };
    let html = payload
        .get(Flavor::Html)
        .and_then(|item| String::from_utf8(item.bytes.clone()).ok());
    set_clip(&text, html.as_deref())
}

/// Plain text only — the caret-level paste path.
#[cfg(all(target_os = "android", feature = "jni"))]
pub fn get_clipboard_content() -> Option<String> {
    call_string("getClipboardText").filter(|t| !t.is_empty())
}

/// Plain text only — the caret-level copy path.
#[cfg(all(target_os = "android", feature = "jni"))]
pub fn write_to_clipboard(text: &str) -> bool {
    set_clip(text, None)
}

/// Whether the clipboard holds anything pasteable.
#[cfg(all(target_os = "android", feature = "jni"))]
#[must_use]
pub fn has_text() -> bool {
    get_clipboard_content().is_some()
}

/// One `String`-returning static call on `NativeTextBridge`.
///
/// Every failure — no JNI context yet, the class missing from this APK, a
/// pending exception — answers `None` and logs at debug: a clipboard that
/// cannot be read is not a reason to take the app down mid-paste.
#[cfg(all(target_os = "android", feature = "jni"))]
fn call_string(method: &str) -> Option<String> {
    use jni::JavaVM;

    let vm_ptr = super::java_vm_ptr();
    let activity_ptr = super::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    let result = (|| -> Result<Option<String>, String> {
        let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
            .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach_current_thread: {e:?}"))?;
        let activity =
            unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
        let class = crate::desktop::extra::find_app_class(
            &mut env,
            &activity,
            "com/azul/text/NativeTextBridge",
        )
        .ok_or_else(|| "NativeTextBridge not in this APK".to_string())?;
        let value = env
            .call_static_method(
                &class,
                method,
                "(Landroid/app/Activity;)Ljava/lang/String;",
                &[jni::objects::JValue::Object(&activity)],
            )
            .and_then(|v| v.l())
            .map_err(|e| {
                let _ = env.exception_clear();
                format!("call {method}: {e:?}")
            })?;
        if value.is_null() {
            return Ok(None);
        }
        let s: String = env
            .get_string(&jni::objects::JString::from(value))
            .map_err(|e| {
                let _ = env.exception_clear();
                format!("get_string: {e:?}")
            })?
            .into();
        Ok(Some(s))
    })();
    match result {
        Ok(v) => v,
        Err(e) => {
            log_debug!(LogCategory::Input, "[Android] clipboard {e}");
            None
        }
    }
}

/// `NativeTextBridge.setClipboard(activity, text, htmlOrNull)`.
#[cfg(all(target_os = "android", feature = "jni"))]
fn set_clip(text: &str, html: Option<&str>) -> bool {
    use jni::JavaVM;

    let vm_ptr = super::java_vm_ptr();
    let activity_ptr = super::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return false;
    }
    let result = (|| -> Result<bool, String> {
        let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }
            .map_err(|e| format!("JavaVM::from_raw: {e:?}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach_current_thread: {e:?}"))?;
        let activity =
            unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
        let class = crate::desktop::extra::find_app_class(
            &mut env,
            &activity,
            "com/azul/text/NativeTextBridge",
        )
        .ok_or_else(|| "NativeTextBridge not in this APK".to_string())?;
        let j_text = env
            .new_string(text)
            .map_err(|e| format!("new_string(text): {e:?}"))?;
        // A null second argument is the "plain clip" signal on the Java side;
        // `JObject::null()` is how jni spells it.
        let j_html = match html {
            Some(h) => env
                .new_string(h)
                .map_err(|e| format!("new_string(html): {e:?}"))?,
            None => jni::objects::JString::from(jni::objects::JObject::null()),
        };
        env.call_static_method(
            &class,
            "setClipboard",
            "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Z",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Object(&j_text),
                jni::objects::JValue::Object(&j_html),
            ],
        )
        .and_then(|v| v.z())
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("call setClipboard: {e:?}")
        })
    })();
    match result {
        Ok(ok) => ok,
        Err(e) => {
            log_debug!(LogCategory::Input, "[Android] clipboard {e}");
            false
        }
    }
}

// Without the `jni` feature there is no Java side to call: the clipboard is
// simply absent, which is what the shared layer already handles.
#[cfg(all(target_os = "android", not(feature = "jni")))]
pub fn read_payload() -> Option<ClipboardPayload> {
    None
}

#[cfg(all(target_os = "android", not(feature = "jni")))]
pub fn write_payload(_payload: &ClipboardPayload) -> bool {
    false
}

#[cfg(all(target_os = "android", not(feature = "jni")))]
pub fn get_clipboard_content() -> Option<String> {
    None
}

#[cfg(all(target_os = "android", not(feature = "jni")))]
pub fn write_to_clipboard(_text: &str) -> bool {
    false
}

#[cfg(all(target_os = "android", not(feature = "jni")))]
#[must_use]
pub fn has_text() -> bool {
    false
}
