//! The Java UI thread's side of the shell: the object Java holds, and every JNI entry point.
//!
//! NativeActivity runs `android_main` on a thread of its own, and every callback of the Java glue (the
//! `InputConnection`, the insets listener, the gesture detectors, the selection toolbar, TalkBack's
//! provider) arrives on the UI thread. Nothing here touches the `AndroidWindow`: a callback pushes an
//! event onto [`UiQueues`], sets a flag, or reads what the loop published, and wakes the loop. The loop
//! applies the events with the window it owns (`apply_ui_event` in `mod.rs`). See
//! `android_core::ui_bridge` for why, and for what the shell did before (`&mut AndroidWindow` formed on
//! the UI thread from the address of a local of `android_main`).
//!
//! The cookie Java passes back on every call is the address of a [`UiBridge`] that is leaked on
//! purpose: it lives as long as the process, so a callback that arrives after `android_main` returned
//! (a late `getTextBeforeCursor`, TalkBack's last `createAccessibilityNodeInfo`) finds a CLOSED bridge
//! and answers its default instead of reading freed stack.

use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc, Mutex,
};

use super::accessibility::AndroidAccessibilityAdapter;
use crate::desktop::shell2::android_core::{
    ime::pack_rect,
    ui_bridge::{RawGesture, UiEvent, UiQueues},
};

/// Everything the UI thread may touch. `Send + Sync` by construction: locks and atomics, and nothing of
/// the window.
pub struct UiBridge {
    /// The events and the IME mirror.
    pub queues: UiQueues,
    /// TalkBack's view of the layout: a snapshot behind a lock, and the actions it queued.
    pub accessibility: AndroidAccessibilityAdapter,
    /// Wakes the loop, which blocks without a timeout while the activity has no surface.
    waker: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl Default for UiBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBridge {
    /// An open bridge with nothing queued and no waker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queues: UiQueues::new(),
            accessibility: AndroidAccessibilityAdapter::new(),
            waker: Mutex::new(None),
        }
    }

    /// The loop's waker, made by `android_main`.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut slot) = self.waker.lock() {
            *slot = Some(Box::new(waker));
        }
    }

    /// Wake the loop from any thread: a callback queued work for it.
    pub fn wake(&self) {
        if let Ok(slot) = self.waker.lock() {
            if let Some(waker) = slot.as_ref() {
                waker();
            }
        }
    }

    /// Queue an event and wake the loop.
    pub fn push(&self, event: UiEvent) {
        self.queues.push(event);
        self.wake();
    }
}

/// The address of the published [`UiBridge`], 0 before `android_main` published it.
static ANDROID_UI_BRIDGE: AtomicI64 = AtomicI64::new(0);

/// Publish `bridge` for the Java side (`AzulActivity.nativeGetWindowPointer`) and every thread that
/// wakes the loop. The `Arc` is leaked: the cookie never dangles.
pub(super) fn publish(bridge: &Arc<UiBridge>) {
    let cookie = Arc::into_raw(Arc::clone(bridge)) as i64;
    ANDROID_UI_BRIDGE.store(cookie, Ordering::SeqCst);
}

/// The bridge behind a cookie Java passed back, or `None` for 0.
///
/// SAFETY: a non-zero cookie is an address [`publish`] leaked, valid for the life of the process.
pub(super) unsafe fn from_cookie<'a>(cookie: i64) -> Option<&'a UiBridge> {
    if cookie == 0 {
        return None;
    }
    Some(&*(cookie as *const UiBridge))
}

/// The published bridge, if any.
pub(super) fn published() -> Option<&'static UiBridge> {
    // SAFETY: see `from_cookie`.
    unsafe { from_cookie(ANDROID_UI_BRIDGE.load(Ordering::SeqCst)) }
}

/// Wake the event loop from any thread (a JNI callback that queued work for it). A no-op before
/// `android_main` ran and after it ended.
pub fn wake_event_loop() {
    if let Some(bridge) = published() {
        bridge.wake();
    }
}

/// The cookie as Java sees it (what `AzulActivity.nativeGetWindowPointer` returns, and what the loop
/// hands to the Java side when it starts something that calls back, the selection toolbar).
pub(super) fn cookie() -> i64 {
    ANDROID_UI_BRIDGE.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// AzulActivity
// ---------------------------------------------------------------------------

/// `AzulActivity.nativeGetWindowPointer()`: the cookie of the bridge, 0 until `android_main` has
/// published it (the Java side asks again).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_app_AzulActivity_nativeGetWindowPointer(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
) -> i64 {
    cookie()
}

/// `AzulActivity.nativeOnUiModeChanged(cookie, night)`: the device's night mode, azul's own encoding
/// (0 undefined, 1 light, 2 dark). Queued: the loop thread applies it (`drain_pending_theme`).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_app_AzulActivity_nativeOnUiModeChanged(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    night_mode: i32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.queues.set_theme(night_mode);
        bridge.wake();
    }
}

// ---------------------------------------------------------------------------
// NativeGestureBridge: positions arrive in PHYSICAL pixels (View coordinates); the loop converts.
// ---------------------------------------------------------------------------

fn gesture(native_ptr: i64, gesture: RawGesture) {
    // SAFETY: see `from_cookie`.
    if let Some(bridge) = unsafe { from_cookie(native_ptr) } {
        bridge.push(UiEvent::Gesture(gesture));
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gesture_NativeGestureBridge_nativeOnDoubleTap(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
) {
    gesture(native_ptr, RawGesture::DoubleTap);
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gesture_NativeGestureBridge_nativeOnLongPress(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    x: f32,
    y: f32,
    duration_ms: i64,
) {
    gesture(native_ptr, RawGesture::LongPress { x, y, duration_ms });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gesture_NativeGestureBridge_nativeOnSwipe(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    direction: i32,
) {
    gesture(native_ptr, RawGesture::Swipe { direction });
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gesture_NativeGestureBridge_nativeOnPinch(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    scale: f32,
    center_x: f32,
    center_y: f32,
    initial_distance: f32,
    current_distance: f32,
    duration_ms: i64,
    began: u8,
    ended: u8,
) {
    gesture(
        native_ptr,
        RawGesture::Pinch {
            scale,
            center_x,
            center_y,
            initial_distance,
            current_distance,
            duration_ms,
            began: began != 0,
            ended: ended != 0,
        },
    );
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gesture_NativeGestureBridge_nativeOnRotation(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    angle_radians: f32,
    center_x: f32,
    center_y: f32,
    duration_ms: i64,
) {
    gesture(
        native_ptr,
        RawGesture::Rotation {
            angle: angle_radians,
            center_x,
            center_y,
            duration_ms,
        },
    );
}

// ---------------------------------------------------------------------------
// NativeTextBridge: Android does not hand a NativeActivity keystrokes for text. The soft keyboard
// talks to an `InputConnection`, a Java object native code cannot implement, so the Java side owns a
// `BaseInputConnection` on the activity's view and forwards each edit down here. An IME composing
// Japanese produces no `KeyEvent` whatsoever.
// ---------------------------------------------------------------------------

/// Read a Java `String` argument.
unsafe fn jstring_to_string(env: *mut jni::sys::JNIEnv, s: jni::sys::jstring) -> String {
    if s.is_null() {
        return String::new();
    }
    let Ok(mut env) = jni::JNIEnv::from_raw(env) else {
        return String::new();
    };
    match env.get_string(&jni::objects::JString::from_raw(s)) {
        Ok(js) => js.into(),
        Err(_) => String::new(),
    }
}

/// Build a Java string, or null.
unsafe fn new_jstring(env: *mut jni::sys::JNIEnv, s: &str) -> jni::sys::jstring {
    let null = core::ptr::null_mut();
    let Ok(mut env) = jni::JNIEnv::from_raw(env) else {
        return null;
    };
    match env.new_string(s) {
        Ok(v) => v.into_raw(),
        Err(_) => null,
    }
}

/// The focused control's IME hints, packed into one int: `purpose | (multiline << 8)`. `0` (plain text,
/// single line) when nothing is focused.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeGetInputHints(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
) -> i32 {
    from_cookie(native_ptr).map_or(0, |b| b.queues.read_mirror().hints)
}

/// `InputConnection.getTextBeforeCursor`: up to `n` UTF-16 units before the caret, from the mirror the
/// loop published (an IME asks for this on every focus and after every commit: autocorrect, the
/// suggestion strip, swipe typing and double-space-inserts-a-period all work from it).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeGetTextBeforeCursor(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
    n: i32,
) -> jni::sys::jstring {
    let text = from_cookie(native_ptr).map_or_else(String::new, |b| b.queues.read_mirror().text_before_cursor(n));
    new_jstring(env, &text)
}

/// `InputConnection.getTextAfterCursor`: up to `n` UTF-16 units after the caret.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeGetTextAfterCursor(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
    n: i32,
) -> jni::sys::jstring {
    let text = from_cookie(native_ptr).map_or_else(String::new, |b| b.queues.read_mirror().text_after_cursor(n));
    new_jstring(env, &text)
}

/// `InputConnection.getSelectedText`: null when there is no selection, which is what the IME expects
/// (an empty string means "a selection that happens to be empty" and changes its behaviour).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeGetSelectedText(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
) -> jni::sys::jstring {
    match from_cookie(native_ptr).and_then(|b| b.queues.read_mirror().selected_text()) {
        Some(s) => new_jstring(env, &s),
        None => core::ptr::null_mut(),
    }
}

/// The selection bounds in PHYSICAL pixels for Android's floating toolbar, packed into one `jlong`
/// (`android_core::ime::pack_rect`); 0 when nothing is selected.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeGetSelectionRect(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
) -> i64 {
    from_cookie(native_ptr).map_or(0, |b| pack_rect(b.queues.read_mirror().selection_rect))
}

/// Whether a composition is currently open: the Java side asks so it can answer
/// `InputConnection.getComposingText` without duplicating state.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeIsComposing(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
) -> i32 {
    from_cookie(native_ptr).map_or(0, |b| i32::from(b.queues.read_mirror().composing))
}

/// A selection action chosen from the floating toolbar: 0 cut, 1 copy, 2 paste, 3 select all.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeSelectionAction(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
    action: i32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::SelectionAction(action));
    }
}

/// `InputConnection.commitText`: the IME finished composing and this is the result.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeCommitText(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
    text: jni::sys::jstring,
) {
    let text = jstring_to_string(env, text);
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::Commit(text));
    }
}

/// `InputConnection.setComposingText`: the IME's in-progress preedit. `cursor_pos` is Android's own
/// convention: `> 0` counts from the END of the composing text, `<= 0` from its start.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeSetComposingText(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    native_ptr: i64,
    text: jni::sys::jstring,
    cursor_pos: i32,
) {
    let text = jstring_to_string(env, text);
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::SetComposing {
            text,
            cursor: cursor_pos,
        });
    }
}

/// `InputConnection.finishComposingText`: composition ended without a separate commit (the user tapped
/// away, or the IME gave up).
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeFinishComposing(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::FinishComposing);
    }
}

/// `InputConnection.deleteSurroundingText`: the IME asking for a backspace it did not express as a key.
/// The counts are UTF-16 units.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeDeleteSurrounding(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    before: i32,
    after: i32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::DeleteSurrounding { before, after });
    }
}

/// `View.onApplyWindowInsets` - system bars, display cutout and the IME - in PHYSICAL pixels (Android
/// insets always are). The IME inset stays apart from the bars: the navigation bar is fixed for the
/// life of the window while the keyboard appears, animates and leaves. It arrives once per frame of the
/// keyboard's animation, so only the latest matters to the loop.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeOnWindowInsets(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    top_px: i32,
    bottom_px: i32,
    left_px: i32,
    right_px: i32,
    ime_px: i32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::Insets {
            top: top_px,
            bottom: bottom_px,
            left: left_px,
            right: right_px,
            ime: ime_px,
        });
    }
}

/// `AzulInputView.onPointerCaptureChange`: the framework's answer to a capture request, and the notice
/// that one was taken away.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeOnPointerCaptureChanged(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    has_capture: i32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::PointerCapture(has_capture != 0));
    }
}

/// `AzulInputView.onCapturedPointerEvent` deltas: the loop sums them.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_text_NativeTextBridge_nativeOnRawMotion(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    native_ptr: i64,
    device_id: i32,
    dx: f32,
    dy: f32,
) {
    if let Some(bridge) = from_cookie(native_ptr) {
        bridge.push(UiEvent::RawMotion {
            dx,
            dy,
            device: device_id,
        });
    }
}
