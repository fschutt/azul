//! The Android `<webview>` backend: one `android.webkit.WebView` per view,
//! driven through JNI by the `com.azul.webview.AzulWebView` bridge
//! (`scripts/android/AzulWebView.java`, dexed into the APK by
//! `build-android.sh`; without it the window says so and has no web views).
//!
//! The activity is a `NativeActivity`, whose window surface native code owns,
//! so each view lives in a `PopupWindow` over it: the popup is the clip (the
//! visible part of the view's box), the `WebView` in it the whole box -
//! moved, scaled and TURNED with `View` properties ([`android_place`]), so a
//! transformed page shows as such.
//!
//! Every main-frame navigation is the app's to allow: the bridge answers
//! `shouldOverrideUrlLoading` with "handled" and reports it
//! (`nativeOnNavigation`); the initial `src` and `navigate` are asked about
//! the same way before anything loads; an allowed one is loaded with
//! `load`. So no decision has to be waited for on the UI thread, at the
//! price that an allowed redirect is loaded again as a `GET` of its target
//! (what a sign-in redirect is). The bridge's callbacks run on the UI
//! thread: they only queue into [`INBOX`] and wake the loop
//! (`wake_event_loop`); the loop's pump takes them.

use std::{collections::BTreeMap, sync::Mutex};

use azul_core::webview::{
    WebViewConfig, WebViewEvent, WebViewLoadError, WebViewNavigation, WebViewStorage,
};
use azul_css::AzString;
use azul_layout::managers::webview::{WebViewId, WebViewPlacement, WebViewReport, WebViewTransform};
use jni::objects::{JClass, JObject, JValue};

use crate::desktop::shell2::common::webview::WebViewBackend;

/// The bridge class in the APK.
const BRIDGE: &str = "com/azul/webview/AzulWebView";

/// What the bridge reported, for the loop to take, and the navigations the
/// app has not answered yet.
struct Inbox {
    reports: Vec<WebViewReport>,
    last_request: u64,
    /// request -> (view, page): what `load` gets once the app allowed it.
    pending: BTreeMap<u64, (WebViewId, String)>,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    reports: Vec::new(),
    last_request: 0,
    pending: BTreeMap::new(),
});

/// Report `event` for view `id`.
fn report(id: WebViewId, event: WebViewEvent) {
    if let Ok(mut inbox) = INBOX.lock() {
        inbox.reports.push(WebViewReport {
            id,
            request: 0,
            event,
        });
    }
}

/// Ask the app about a navigation of view `id` to `url`.
fn ask(id: WebViewId, url: String, is_redirect: bool) {
    if let Ok(mut inbox) = INBOX.lock() {
        inbox.last_request += 1;
        let request = inbox.last_request;
        inbox.pending.insert(request, (id, url.clone()));
        inbox.reports.push(WebViewReport {
            id,
            request,
            event: WebViewEvent::NavigationRequested(WebViewNavigation {
                url: AzString::from(url),
                is_redirect,
            }),
        });
    }
}

/// Where the bridge puts a view, in window pixels: the clip, the page in it
/// (origin, size), its scale and turn (degrees, clockwise), and whether it
/// shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AndroidPlace {
    pub clip: [i32; 4],
    pub page: [i32; 4],
    pub scale: (f32, f32),
    pub rotation: f32,
    pub visible: bool,
}

/// The bridge's placement of a view placed at `placement` with
/// `transform` (none: untransformed), at `scale` pixels per logical one.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn android_place(
    placement: &WebViewPlacement,
    transform: Option<&WebViewTransform>,
    scale: f32,
) -> AndroidPlace {
    let px = |v: f32| (v * scale).round() as i32;
    let clip = placement.clip;
    let (origin, size, zoom, rotation) = match transform {
        Some(t) => (
            t.to_window(placement.rect, azul_core::geom::LogicalPosition::new(0.0, 0.0)),
            t.size,
            t.zoom(),
            t.rotation_degrees(),
        ),
        None => (placement.rect.origin, placement.rect.size, (1.0, 1.0), 0.0),
    };
    AndroidPlace {
        clip: [
            px(clip.origin.x),
            px(clip.origin.y),
            px(clip.size.width),
            px(clip.size.height),
        ],
        page: [
            px(origin.x - clip.origin.x),
            px(origin.y - clip.origin.y),
            px(size.width),
            px(size.height),
        ],
        scale: zoom,
        rotation,
        visible: placement.visible,
    }
}

/// Run `f` with the JNI environment, the activity and the bridge class.
fn with_bridge<T>(
    f: impl FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>, &JClass<'_>) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let vm_ptr = super::java_vm_ptr();
    let activity_ptr = super::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return Err(String::from("the Java side of the app is not up yet"));
    }
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr.cast()) }.map_err(|e| format!("{e:?}"))?;
    let mut env = vm.attach_current_thread().map_err(|e| format!("{e:?}"))?;
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let class = crate::desktop::extra::find_app_class(&mut env, &activity, BRIDGE).ok_or_else(
        || String::from("this app has no AzulWebView bridge (scripts/android/AzulWebView.java)"),
    )?;
    f(&mut env, &activity, &class).map_err(|e| {
        let _ = env.exception_clear();
        format!("AzulWebView: {e:?}")
    })
}

/// The bridge's id of view `id`.
#[allow(clippy::cast_possible_wrap)]
const fn java_id(id: WebViewId) -> i64 {
    id.0 as i64
}

/// Call the bridge's `method(activity, id, ...)`, logging a failure.
fn call(method: &str, signature: &str, id: WebViewId, rest: &[Arg]) {
    let result = with_bridge(|env, activity, class| {
        let mut args = vec![JValue::Object(activity), JValue::Long(java_id(id))];
        args.extend(rest.iter().map(|arg| match arg {
            Arg::Int(v) => JValue::Int(*v),
            Arg::Float(v) => JValue::Float(*v),
            Arg::Bool(v) => JValue::Bool(u8::from(*v)),
        }));
        env.call_static_method(class, method, signature, &args).map(|_| ())
    });
    if let Err(e) = result {
        crate::plog_warn!("[webview] {method}: {e}");
    }
}

/// The bridge's `load(activity, id, url)`.
fn load(id: WebViewId, url: &str) {
    let result = with_bridge(|env, activity, class| {
        let url = env.new_string(url)?;
        env.call_static_method(
            class,
            "load",
            "(Landroid/app/Activity;JLjava/lang/String;)V",
            &[JValue::Object(activity), JValue::Long(java_id(id)), JValue::Object(&url)],
        )
        .map(|_| ())
    });
    if let Err(e) = result {
        crate::plog_warn!("[webview] load: {e}");
    }
}

/// An argument after `(activity, id)`.
#[derive(Clone, Copy)]
enum Arg {
    Int(i32),
    Float(f32),
    Bool(bool),
}

/// The web views of the Android window.
pub struct AndroidWebViews {
    scale: f32,
    /// The last placement and transform of each view.
    placed: BTreeMap<WebViewId, (WebViewPlacement, Option<WebViewTransform>)>,
}

impl AndroidWebViews {
    /// The backend of a window at `scale` pixels per logical one.
    #[must_use]
    pub const fn new(scale: f32) -> Self {
        Self {
            scale,
            placed: BTreeMap::new(),
        }
    }

    /// The window's scale changed.
    pub fn set_scale(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.scale = scale;
            let ids: Vec<WebViewId> = self.placed.keys().copied().collect();
            for id in ids {
                self.send_place(id);
            }
        }
    }

    fn send_place(&self, id: WebViewId) {
        let Some((placement, transform)) = self.placed.get(&id) else {
            return;
        };
        let place = android_place(placement, transform.as_ref(), self.scale);
        let [cx, cy, cw, ch] = place.clip;
        let [px, py, pw, ph] = place.page;
        call(
            "place",
            "(Landroid/app/Activity;JIIIIIIIIFFFZ)V",
            id,
            &[
                Arg::Int(cx),
                Arg::Int(cy),
                Arg::Int(cw),
                Arg::Int(ch),
                Arg::Int(px),
                Arg::Int(py),
                Arg::Int(pw),
                Arg::Int(ph),
                Arg::Float(place.scale.0),
                Arg::Float(place.scale.1),
                Arg::Float(place.rotation),
                Arg::Bool(place.visible),
            ],
        );
    }
}

impl WebViewBackend for AndroidWebViews {
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String> {
        let persistent = matches!(config.storage, WebViewStorage::Persistent);
        with_bridge(|env, activity, class| {
            env.call_static_method(
                class,
                "create",
                "(Landroid/app/Activity;JZ)V",
                &[
                    JValue::Object(activity),
                    JValue::Long(java_id(id)),
                    JValue::Bool(u8::from(persistent)),
                ],
            )
            .map(|_| ())
        })?;
        self.placed.insert(id, (WebViewPlacement::HIDDEN, None));
        if !src.is_empty() {
            ask(id, src.to_string(), false);
        }
        Ok(())
    }
    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement) {
        if let Some(entry) = self.placed.get_mut(&id) {
            entry.0 = *placement;
        }
        self.send_place(id);
    }
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        if let Some(entry) = self.placed.get_mut(&id) {
            entry.1 = (!transform.is_untransformed()).then_some(*transform);
        }
        self.send_place(id);
    }
    fn navigate(&mut self, id: WebViewId, url: &str) {
        ask(id, url.to_string(), false);
    }
    fn reload(&mut self, id: WebViewId) {
        call("reload", "(Landroid/app/Activity;J)V", id, &[]);
    }
    fn go_back(&mut self, id: WebViewId) {
        call("goBack", "(Landroid/app/Activity;J)V", id, &[]);
    }
    fn decide_navigation(&mut self, _id: WebViewId, request: u64, allow: bool) {
        let pending = INBOX.lock().ok().and_then(|mut inbox| inbox.pending.remove(&request));
        if let (true, Some((id, url))) = (allow, pending) {
            load(id, &url);
        }
    }
    fn destroy(&mut self, id: WebViewId) {
        self.placed.remove(&id);
        if let Ok(mut inbox) = INBOX.lock() {
            inbox.pending.retain(|_, (view, _)| *view != id);
        }
        call("destroy", "(Landroid/app/Activity;J)V", id, &[]);
    }
    fn poll_reports(&mut self) -> Vec<WebViewReport> {
        INBOX
            .lock()
            .map(|mut inbox| core::mem::take(&mut inbox.reports))
            .unwrap_or_default()
    }
}

impl Drop for AndroidWebViews {
    fn drop(&mut self) {
        let ids: Vec<WebViewId> = self.placed.keys().copied().collect();
        for id in ids {
            call("destroy", "(Landroid/app/Activity;J)V", id, &[]);
        }
    }
}

// ───────── JNI inbound: the bridge's callbacks (UI thread) ─────────

/// A Java string as a Rust one.
unsafe fn java_string(raw_env: *mut jni::sys::JNIEnv, s: jni::sys::jstring) -> Option<String> {
    if raw_env.is_null() || s.is_null() {
        return None;
    }
    let mut env = unsafe { jni::JNIEnv::from_raw(raw_env) }.ok()?;
    let js = unsafe { jni::objects::JString::from_raw(s) };
    let out: Option<String> = env.get_string(&js).ok().map(Into::into);
    out
}

#[allow(clippy::cast_sign_loss)]
const fn view_id(id: i64) -> WebViewId {
    WebViewId(id as u64)
}

/// `AzulWebView.nativeOnNavigation(long id, String url, boolean redirect)`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_webview_AzulWebView_nativeOnNavigation(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: i64,
    url: jni::sys::jstring,
    redirect: jni::sys::jboolean,
) {
    if let Some(url) = unsafe { java_string(raw_env, url) } {
        ask(view_id(id), url, redirect != 0);
        super::wake_event_loop();
    }
}

/// `AzulWebView.nativeOnPageFinished(long id, String url)`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_webview_AzulWebView_nativeOnPageFinished(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: i64,
    url: jni::sys::jstring,
) {
    let url = unsafe { java_string(raw_env, url) }.unwrap_or_default();
    report(view_id(id), WebViewEvent::LoadFinished(AzString::from(url)));
    super::wake_event_loop();
}

/// `AzulWebView.nativeOnLoadFailed(long id, String url, String reason)`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_webview_AzulWebView_nativeOnLoadFailed(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: i64,
    url: jni::sys::jstring,
    reason: jni::sys::jstring,
) {
    let url = unsafe { java_string(raw_env, url) }.unwrap_or_default();
    let reason = unsafe { java_string(raw_env, reason) }
        .unwrap_or_else(|| String::from("the page did not load"));
    report(
        view_id(id),
        WebViewEvent::LoadFailed(WebViewLoadError {
            url: AzString::from(url),
            reason: AzString::from(reason),
        }),
    );
    super::wake_event_loop();
}

/// `AzulWebView.nativeOnTitle(long id, String title)`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_webview_AzulWebView_nativeOnTitle(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    id: i64,
    title: jni::sys::jstring,
) {
    let title = unsafe { java_string(raw_env, title) }.unwrap_or_default();
    report(view_id(id), WebViewEvent::TitleChanged(AzString::from(title)));
    super::wake_event_loop();
}
