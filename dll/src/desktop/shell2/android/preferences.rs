//! The user's accessibility and display settings on Android, read through JNI.
//!
//! The NDK has none of them (`AConfiguration_*` knows no font scale, no bold text, no Settings
//! value), so every read is a call into the framework: `Configuration.fontScale` and
//! `fontWeightAdjustment`, the factor `TypedValue.applyDimension` converts an `sp` with (non-linear
//! since Android 14), the `Settings.Global` animation scales, the `Settings.Secure` keys that carry
//! `@Readable`, the `AccessibilityManager`, the `UiModeManager`'s contrast (14+), the
//! `CaptioningManager`, `ViewConfiguration`'s touch-and-hold delay.
//!
//! No Java class is needed to READ: these are framework calls made from Rust. What each answer
//! means (the nonlinear font scale, `Integer.MAX_VALUE` as "no adjustment", a zero animation scale
//! as "Remove animations") is [`azul_css::accessibility_raw::RawAndroid::into_preferences`], tested
//! on every host. Every read is independent: one that fails (a key the app may not read since
//! Android 12, a method of a newer Android, a pending exception) leaves its field `None`, which is
//! `Unknown` in the preferences - never `Off`.
//!
//! The reads run on the native thread of `android_main`, which stays attached for its whole life:
//! a local frame releases the references of one read (this thread never returns to Java).

use azul_css::accessibility_raw::RawAndroid;

/// `TypedValue.COMPLEX_UNIT_DIP` and `COMPLEX_UNIT_SP`.
#[cfg(feature = "jni")]
const COMPLEX_UNIT_DIP: i32 = 1;
#[cfg(feature = "jni")]
const COMPLEX_UNIT_SP: i32 = 2;
/// The body text size the framework's `sp` conversion is measured at.
#[cfg(feature = "jni")]
const BODY_SP: f32 = 16.0;
/// `FLAG_CONTENT_ICONS | FLAG_CONTENT_TEXT | FLAG_CONTENT_CONTROLS` of `AccessibilityManager`.
#[cfg(feature = "jni")]
const CONTENT_FLAGS_ALL: i32 = 1 | 2 | 4;

#[cfg(feature = "jni")]
const SETTINGS_GLOBAL: &str = "android/provider/Settings$Global";
#[cfg(feature = "jni")]
const SETTINGS_SECURE: &str = "android/provider/Settings$Secure";
#[cfg(feature = "jni")]
const SETTINGS_SYSTEM: &str = "android/provider/Settings$System";

/// Read everything the app may read of the user's accessibility settings: the VM and the activity
/// are the ones the loop's `AndroidApp` holds. A null pointer, a VM that cannot be attached, an
/// activity that answers nothing: every field stays `None`.
#[cfg(feature = "jni")]
#[must_use]
pub fn read_raw(vm: *mut core::ffi::c_void, activity: *mut core::ffi::c_void) -> RawAndroid {
    let mut raw = RawAndroid::default();
    if vm.is_null() || activity.is_null() {
        return raw;
    }
    // SAFETY: `vm` is the JavaVM of the process (`AndroidApp::vm_as_ptr`), valid for its life.
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(vm.cast()) }) else {
        return raw;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return raw;
    };
    // Released when this read returns: this thread never returns to Java (see `LocalFrame`).
    let _frame = crate::desktop::extra::LocalFrame::push(&env, 96);
    // SAFETY: `activity` is the global reference of the activity object the activity layer keeps
    // alive for the whole run (`AndroidApp::activity_as_ptr`); `JObject` does not delete it.
    let activity = unsafe { jni::objects::JObject::from_raw(activity.cast()) };
    fill(&mut env, &activity, &mut raw);
    raw
}

/// Without JNI nothing can be read.
#[cfg(not(feature = "jni"))]
#[must_use]
pub fn read_raw(_vm: *mut core::ffi::c_void, _activity: *mut core::ffi::c_void) -> RawAndroid {
    RawAndroid::default()
}

#[cfg(feature = "jni")]
type Env<'a, 'b> = &'a mut jni::JNIEnv<'b>;

/// The result of a JNI call as an `Option`: a failure (a method that does not exist on this
/// Android, a `SecurityException` for a key the app may not read) clears the exception that is
/// pending - the next call on this thread must not find one - and is `None`.
#[cfg(feature = "jni")]
fn ok<T>(env: &jni::JNIEnv<'_>, result: jni::errors::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            if env.exception_check().unwrap_or(false) {
                drop(env.exception_clear());
            }
            None
        }
    }
}

/// `obj.method()` that answers an object (a service, the resources, the configuration).
#[cfg(feature = "jni")]
fn object<'l>(
    env: Env<'_, 'l>,
    obj: &jni::objects::JObject<'_>,
    method: &str,
    signature: &str,
) -> Option<jni::objects::JObject<'l>> {
    let result = env.call_method(obj, method, signature, &[]).and_then(|v| v.l());
    ok(env, result).filter(|o| !o.is_null())
}

/// `obj.method()` that answers a `boolean`.
#[cfg(feature = "jni")]
fn boolean(env: Env<'_, '_>, obj: &jni::objects::JObject<'_>, method: &str) -> Option<bool> {
    let result = env.call_method(obj, method, "()Z", &[]).and_then(|v| v.z());
    ok(env, result)
}

/// `obj.field` of type `F` / `I`.
#[cfg(feature = "jni")]
fn float_field(env: Env<'_, '_>, obj: &jni::objects::JObject<'_>, name: &str) -> Option<f32> {
    let result = env.get_field(obj, name, "F").and_then(|v| v.f());
    ok(env, result)
}

#[cfg(feature = "jni")]
fn int_field(env: Env<'_, '_>, obj: &jni::objects::JObject<'_>, name: &str) -> Option<i32> {
    let result = env.get_field(obj, name, "I").and_then(|v| v.i());
    ok(env, result)
}

/// `activity.getSystemService(name)`: the service, or `None` on an Android without it.
#[cfg(feature = "jni")]
fn service<'l>(
    env: Env<'_, 'l>,
    activity: &jni::objects::JObject<'_>,
    name: &str,
) -> Option<jni::objects::JObject<'l>> {
    let jname = env.new_string(name);
    let name = ok(env, jname)?;
    let result = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[jni::objects::JValue::Object(&name)],
        )
        .and_then(|v| v.l());
    ok(env, result).filter(|o| !o.is_null())
}

/// `Settings.<table>.getInt(resolver, key, default)`: the value, or `default` when the key is not
/// set (a toggle that was never touched is off). `None` when the read itself failed.
#[cfg(feature = "jni")]
fn setting_int(
    env: Env<'_, '_>,
    table: &str,
    resolver: &jni::objects::JObject<'_>,
    key: &str,
    default: i32,
) -> Option<i32> {
    let jkey = env.new_string(key);
    let key = ok(env, jkey)?;
    let result = env
        .call_static_method(
            table,
            "getInt",
            "(Landroid/content/ContentResolver;Ljava/lang/String;I)I",
            &[
                jni::objects::JValue::Object(resolver),
                jni::objects::JValue::Object(&key),
                jni::objects::JValue::Int(default),
            ],
        )
        .and_then(|v| v.i());
    ok(env, result)
}

/// `Settings.<table>.getFloat(resolver, key, default)`.
#[cfg(feature = "jni")]
fn setting_float(
    env: Env<'_, '_>,
    table: &str,
    resolver: &jni::objects::JObject<'_>,
    key: &str,
    default: f32,
) -> Option<f32> {
    let jkey = env.new_string(key);
    let key = ok(env, jkey)?;
    let result = env
        .call_static_method(
            table,
            "getFloat",
            "(Landroid/content/ContentResolver;Ljava/lang/String;F)F",
            &[
                jni::objects::JValue::Object(resolver),
                jni::objects::JValue::Object(&key),
                jni::objects::JValue::Float(default),
            ],
        )
        .and_then(|v| v.f());
    ok(env, result)
}

/// A toggle of `Settings`: set, not set (off), or unreadable.
#[cfg(feature = "jni")]
fn setting_flag(
    env: Env<'_, '_>,
    table: &str,
    resolver: &jni::objects::JObject<'_>,
    key: &str,
) -> Option<bool> {
    setting_int(env, table, resolver, key, 0).map(|value| value != 0)
}

/// `TypedValue.applyDimension(unit, value, metrics)`.
#[cfg(feature = "jni")]
fn apply_dimension(
    env: Env<'_, '_>,
    unit: i32,
    value: f32,
    metrics: &jni::objects::JObject<'_>,
) -> Option<f32> {
    let result = env
        .call_static_method(
            "android/util/TypedValue",
            "applyDimension",
            "(IFLandroid/util/DisplayMetrics;)F",
            &[
                jni::objects::JValue::Int(unit),
                jni::objects::JValue::Float(value),
                jni::objects::JValue::Object(metrics),
            ],
        )
        .and_then(|v| v.f());
    ok(env, result)
}

/// The reads, one after the other, each into its own field.
#[cfg(feature = "jni")]
fn fill(env: Env<'_, '_>, activity: &jni::objects::JObject<'_>, raw: &mut RawAndroid) {
    // ---- the configuration: the font scale, the bold text, the factor of an sp ----
    if let Some(resources) = object(env, activity, "getResources", "()Landroid/content/res/Resources;") {
        if let Some(configuration) = object(
            env,
            &resources,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
        ) {
            raw.font_scale = float_field(env, &configuration, "fontScale");
            raw.font_weight_adjustment = int_field(env, &configuration, "fontWeightAdjustment");
        }
        if let Some(metrics) = object(
            env,
            &resources,
            "getDisplayMetrics",
            "()Landroid/util/DisplayMetrics;",
        ) {
            let sp = apply_dimension(env, COMPLEX_UNIT_SP, BODY_SP, &metrics);
            let dip = apply_dimension(env, COMPLEX_UNIT_DIP, BODY_SP, &metrics);
            raw.sp_scale = match (sp, dip) {
                (Some(sp), Some(dip)) if dip > 0.0 => Some(sp / dip),
                _ => None,
            };
        }
    }

    // ---- the Settings tables ----
    if let Some(resolver) = object(
        env,
        activity,
        "getContentResolver",
        "()Landroid/content/ContentResolver;",
    ) {
        // an animation scale that is not set is 1.0; a negative answer would be no scale
        raw.animator_duration_scale =
            setting_float(env, SETTINGS_GLOBAL, &resolver, "animator_duration_scale", 1.0);
        raw.blur_disabled = setting_flag(env, SETTINGS_GLOBAL, &resolver, "disable_window_blurs");
        raw.invert_colors = setting_flag(
            env,
            SETTINGS_SECURE,
            &resolver,
            "accessibility_display_inversion_enabled",
        );
        raw.daltonizer_enabled = setting_flag(
            env,
            SETTINGS_SECURE,
            &resolver,
            "accessibility_display_daltonizer_enabled",
        );
        raw.daltonizer_type = setting_int(
            env,
            SETTINGS_SECURE,
            &resolver,
            "accessibility_display_daltonizer",
            -1,
        );
        raw.high_text_contrast =
            setting_flag(env, SETTINGS_SECURE, &resolver, "high_text_contrast_enabled");
        raw.magnification = setting_flag(
            env,
            SETTINGS_SECURE,
            &resolver,
            "accessibility_display_magnification_enabled",
        );
        raw.mono_audio = setting_flag(env, SETTINGS_SYSTEM, &resolver, "master_mono");
        // the haptic feedback switch is on unless it was turned off
        raw.haptic_feedback = setting_int(env, SETTINGS_SYSTEM, &resolver, "haptic_feedback_enabled", 1)
            .map(|value| value != 0);
        raw.pointer_scale = setting_float(env, SETTINGS_SYSTEM, &resolver, "pointer_scale", 1.0);
    }

    // ---- the accessibility manager: TalkBack, the user's time to take action, outline text ----
    if let Some(manager) = service(env, activity, "accessibility") {
        raw.accessibility_enabled = boolean(env, &manager, "isEnabled");
        raw.touch_exploration = boolean(env, &manager, "isTouchExplorationEnabled");
        // 0 asks for what the user wants at least: 0 when the user chose nothing
        let timeout = env
            .call_method(
                &manager,
                "getRecommendedTimeoutMillis",
                "(II)I",
                &[
                    jni::objects::JValue::Int(0),
                    jni::objects::JValue::Int(CONTENT_FLAGS_ALL),
                ],
            )
            .and_then(|v| v.i());
        raw.action_timeout_ms = ok(env, timeout);
        // Android 16: the same answer through the API, which outranks the key
        if let Some(on) = boolean(env, &manager, "isHighContrastTextEnabled") {
            raw.high_text_contrast = Some(on);
        }
    }

    // ---- the contrast level (14+), the captions, the touch-and-hold delay ----
    if let Some(manager) = service(env, activity, "uimode") {
        let level = env
            .call_method(&manager, "getContrast", "()F", &[])
            .and_then(|v| v.f());
        raw.contrast = ok(env, level);
    }
    if let Some(manager) = service(env, activity, "captioning") {
        raw.captions = boolean(env, &manager, "isEnabled");
    }
    let long_press = env
        .call_static_method("android/view/ViewConfiguration", "getLongPressTimeout", "()I", &[])
        .and_then(|v| v.i());
    raw.long_press_ms = ok(env, long_press);
}
