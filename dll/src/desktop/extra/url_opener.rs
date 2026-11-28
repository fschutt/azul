//! Opening a URL on a phone, where there is no opener process to spawn.
//!
//! `azul_core::url::Url::open` starts `open` / `xdg-open` / `rundll32`; Android has none of them (it answered `false`,
//! with no log line, for every URL). A link is opened by starting an ACTIVITY: `AzulUrl.open(activity, url)`
//! (scripts/android/AzulUrl.java) does it and answers whether one was started. `azul-settings:app` is the app's own
//! page in the system settings (where an app sends a user who refused a permission for good).
//!
//! A headless or scripted TEST run (`run_mode::test_hooks().open_nothing`) starts nothing and answers `true`, as the
//! desktop opener does: a test must never pop the system settings of the device it runs on.

/// Start the handler of `href`; `true` once an activity was started.
#[cfg(target_os = "android")]
#[must_use]
pub fn open(href: &str) -> bool {
    use jni::{objects::JObject, objects::JValue, JavaVM};

    if azul_core::run_mode::test_hooks().open_nothing {
        return true;
    }
    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return false;
    }
    let Ok(vm) = (unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }) else {
        return false;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return false;
    };
    // Released when this function returns: this thread never returns to Java (see `LocalFrame`).
    let _frame = crate::desktop::extra::LocalFrame::push(&env, crate::desktop::extra::LocalFrame::DEFAULT_CAPACITY);
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let Some(class) = crate::desktop::extra::find_app_class(&mut env, &activity, "com/azul/url/AzulUrl") else {
        return false;
    };
    let Ok(url) = env.new_string(href) else {
        return false;
    };
    match env
        .call_static_method(
            class,
            "open",
            "(Landroid/app/Activity;Ljava/lang/String;)Z",
            &[JValue::Object(&activity), JValue::Object(&url)],
        )
        .and_then(|v| v.z())
    {
        Ok(started) => started,
        Err(_) => {
            // A pending exception aborts the process at the next JNI call.
            let _ = env.exception_clear();
            false
        }
    }
}
