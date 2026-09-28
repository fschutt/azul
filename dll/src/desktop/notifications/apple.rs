//! macOS and iOS native notifications  -  `UNUserNotificationCenter`.
//!
//! UserNotifications.framework is dlopen'd and driven through `msg_send!` on
//! runtime classes, the way `extra/media_keys/apple.rs` drives MediaPlayer: an
//! app that never posts pays nothing, and a system without the framework
//! (before 10.14) degrades to "unavailable" instead of failing to launch.
//!
//! The same file serves iOS: UN is the same framework with the same API
//! there. Two things differ and are `#[cfg]`'d: the run-loop wake (macOS posts
//! an app-defined `NSEvent`; the iOS display tick pumps every frame anyway) and
//! the launch-notification lookup (`NSApplicationLaunchUserNotificationKey` is
//! AppKit). An iOS app is always a bundle, so the bundle rule below always
//! passes there.
//!
//! # The bundle rule this file is built around
//!
//! `+[UNUserNotificationCenter currentNotificationCenter]` asks LaunchServices
//! for the current process's bundle record. An UNBUNDLED executable - `cargo
//! run`, `target/release/AzWidgets` - has none, and the framework raises
//! `NSInternalInconsistencyException` ("bundleProxyForCurrentProcess is nil").
//! An Objective-C exception unwinding into Rust frames aborts the process. So
//! the center is never touched until [`bundle_status`] has confirmed that the
//! process runs from a `.app` whose Info.plist sets `CFBundleIdentifier`.
//! Unbundled, the backend is unavailable WITH that reason:
//! `PlatformCapability::notifications()` reports it up front, the service logs
//! it once, and every post becomes a `Failed` event carrying it.
//!
//! There is deliberately no fallback:
//!
//! * `NSUserNotificationCenter` (deprecated in 11.0) needs a bundle identifier just the same -
//!   `defaultUserNotificationCenter` is nil without one.
//! * Swizzling `-[NSBundle bundleIdentifier]` to borrow another app's identifier (what some crates
//!   do) makes the notification IMPERSONATE that app: it is attributed to, grouped with and
//!   permissioned as somebody else.
//! * `osascript -e 'display notification ...'` works unbundled but is attributed to Script
//!   Editor, has no buttons and reports nothing back - it would look like a working backend while
//!   every callback stayed silent.
//!
//! # The delegate is installed at LAUNCH
//!
//! A click on a notification of an app that is not running launches it and
//! delivers the response to the center's delegate - only if the delegate is
//! set "before the app finishes launching". [`install_launch_hooks`] runs
//! from the run loop's setup, between the app delegate and `finishLaunching`
//! (and from `application:didFinishLaunchingWithOptions:` on iOS), not at the
//! first post. The response's event then has no live callback in this process
//! and goes to the app-level handler, with the payload from `userInfo`.
//!
//! # Authorization
//!
//! The OS keeps the user's decision across launches; this file READS it
//! (`getNotificationSettingsWithCompletionHandler:`) at launch, whenever the
//! app becomes active (the user may have flipped the switch in System Settings
//! meanwhile) and - throttled - when `PlatformCapability::notifications()`
//! asks. So the probe no longer says "available" after the user turned
//! notifications off. Every reading also reaches the permission manager as
//! `Capability::Notifications`.
//!
//! `requestAuthorizationWithOptions:completionHandler:` prompts:
//! `CallbackInfo::request_notification_permission` asks in context, and a post
//! still asks too - after the user's first answer that returns the stored
//! decision at once, and a request added before the answer is refused. The
//! request is therefore built and added inside the completion handler. A
//! denial is a `Failed` event.
//!
//! # Foreground presentation
//!
//! UN does not show a notification posted while its app is frontmost - which
//! is exactly when a button posts one. The delegate's
//! `willPresentNotification:` answers banner + list + sound.
//!
//! # Threads
//!
//! The completion handlers and the delegate run on UN's private queue. They
//! build Foundation objects, queue into the layout mailbox, and post an
//! app-defined `NSEvent` to wake the run loop - the wake `menuItemAction:`
//! posts, which Apple documents as callable from any thread.

use std::{
    collections::BTreeMap,
    ffi::{c_char, CStr, CString},
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Mutex, OnceLock, PoisonError,
    },
    time::{Duration, Instant},
};

use azul_core::notification::{Notification, NotificationAction, NotificationEvent, NotificationSound};
use azul_css::AzString;
use azul_layout::managers::{
    notification::{queue_notification_event, wire},
    permission::{push_async_result, Capability, PermissionState},
};
use block2::{Block, RcBlock};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
    AllocAnyThread,
};
use objc2_foundation::{NSObject, NSObjectProtocol};

/// `UNAuthorizationOptionSound | UNAuthorizationOptionAlert`.
const AUTH_SOUND_ALERT: usize = (1 << 1) | (1 << 2);
/// `UNNotificationPresentationOption{Sound, Alert, List, Banner}`. `Alert` is
/// the pre-11.0 spelling of banner+list; passing both is what keeps a
/// foreground notification visible on 10.14 and 26 alike.
const PRESENT_SOUND_ALERT_LIST_BANNER: usize = (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4);
/// `UNNotificationActionOptionForeground`: pressing the button brings the app
/// forward, which is what an app that then shows something wants.
const ACTION_OPTION_FOREGROUND: usize = 1 << 2;
/// `UNNotificationCategoryOptionCustomDismissAction`: without it UN never
/// reports a dismissal at all.
const CATEGORY_OPTION_CUSTOM_DISMISS: usize = 1 << 0;

/// `UNNotificationSettings.authorizationStatus` as last READ from the OS
/// (`wire::apple_authorization_status` maps it), or [`STATUS_UNREAD`] before
/// the first reading came back.
static AUTH_STATUS: AtomicI64 = AtomicI64::new(STATUS_UNREAD);
const STATUS_UNREAD: i64 = -1;
/// `UNAuthorizationStatusDenied`.
const STATUS_DENIED: i64 = 1;
/// `UNAuthorizationStatusAuthorized`.
const STATUS_AUTHORIZED: i64 = 2;
/// `UNAuthorizationStatusProvisional`.
const STATUS_PROVISIONAL: i64 = 3;

/// The request identifier of the notification whose click LAUNCHED the app
/// (macOS: `NSApplicationLaunchUserNotificationKey`), until its response
/// arrives and is marked `launched_app`.
static LAUNCH_RESPONSE_ID: Mutex<Option<String>> = Mutex::new(None);

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    /// The global `NSApp`: nil until `+[NSApplication sharedApplication]` has
    /// run on the main thread. Read (not `sharedApplication`) from UN's queue,
    /// because calling that off the main thread would CREATE the application
    /// object there when none exists yet.
    #[allow(non_upper_case_globals)]
    static NSApp: *mut AnyObject;
    /// The `applicationDidFinishLaunching:` userInfo key holding the
    /// `UNNotificationResponse` that launched the app.
    #[allow(non_upper_case_globals)]
    static NSApplicationLaunchUserNotificationKey: *mut AnyObject;
}

fn class(name: &str) -> Option<&'static AnyClass> {
    let c = CString::new(name).ok()?;
    AnyClass::get(&c)
}

/// UserNotifications.framework, loaded once.
fn framework() -> Option<&'static libloading::Library> {
    static LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();
    LIB.get_or_init(|| {
        unsafe {
            libloading::Library::new(
                "/System/Library/Frameworks/UserNotifications.framework/UserNotifications",
            )
        }
        .ok()
    })
    .as_ref()
}

/// An autoreleased `NSString` (interior NULs dropped: `CString` refuses them).
unsafe fn nsstring(s: &str) -> *mut AnyObject {
    let Ok(c) = CString::new(s.replace('\0', "")) else {
        return core::ptr::null_mut();
    };
    let Some(cls) = class("NSString") else {
        return core::ptr::null_mut();
    };
    unsafe { msg_send![cls, stringWithUTF8String: c.as_ptr()] }
}

/// An `NSString *` as a Rust string; `None` for nil.
unsafe fn rust_string(ns: *mut AnyObject) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let p: *const c_char = unsafe { msg_send![ns, UTF8String] };
    if p.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// `-[NSError localizedDescription]`.
unsafe fn error_description(error: *mut AnyObject) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let desc: *mut AnyObject = unsafe { msg_send![error, localizedDescription] };
    unsafe { rust_string(desc) }
}

/// Read one of the framework's `NSString * const` constants.
///
/// `dlsym` returns where the VARIABLE lives, and the variable holds the
/// `NSString *`: a `Symbol<*mut *mut AnyObject>` dereferenced TWICE. One
/// dereference hands out the variable's address posing as an object - the
/// trap `media_keys/apple.rs::info_key` fell into.
unsafe fn framework_string_constant(symbol: &[u8]) -> Option<String> {
    let lib = framework()?;
    let sym: libloading::Symbol<'_, *mut *mut AnyObject> = unsafe { lib.get(symbol) }.ok()?;
    let slot: *mut *mut AnyObject = *sym;
    if slot.is_null() {
        return None;
    }
    let value: *mut AnyObject = unsafe { *slot };
    unsafe { rust_string(value) }
}

/// `UNNotificationDefaultActionIdentifier` and
/// `UNNotificationDismissActionIdentifier`, read from the framework, with the
/// documented values as the fallback.
fn action_identifiers() -> &'static (String, String) {
    static IDS: OnceLock<(String, String)> = OnceLock::new();
    IDS.get_or_init(|| unsafe {
        (
            framework_string_constant(b"UNNotificationDefaultActionIdentifier\0")
                .unwrap_or_else(|| wire::APPLE_DEFAULT_ACTION.to_string()),
            framework_string_constant(b"UNNotificationDismissActionIdentifier\0")
                .unwrap_or_else(|| wire::APPLE_DISMISS_ACTION.to_string()),
        )
    })
}

/// Is this process an app bundle UN can serve? `Ok(bundle identifier)` or the
/// reason it is not. Touches only `NSBundle`, never UN - see the module docs.
pub(super) fn bundle_status() -> Result<String, String> {
    unsafe {
        let Some(bundle_cls) = class("NSBundle") else {
            return Err("Foundation's NSBundle class is missing".to_string());
        };
        let bundle: *mut AnyObject = msg_send![bundle_cls, mainBundle];
        if bundle.is_null() {
            return Err("this process has no main bundle".to_string());
        }
        let path_ns: *mut AnyObject = msg_send![bundle, bundlePath];
        let path = rust_string(path_ns).unwrap_or_default();
        let id_ns: *mut AnyObject = msg_send![bundle, bundleIdentifier];
        let id = rust_string(id_ns).unwrap_or_default();
        if !path.ends_with(".app") {
            return Err(format!(
                "this process does not run from a .app bundle (its main bundle is {path:?}): \
                 UNUserNotificationCenter needs an app bundle whose Info.plist sets \
                 CFBundleIdentifier, and an unbundled binary such as target/release/<app> would \
                 abort inside +[UNUserNotificationCenter currentNotificationCenter]. Run the app \
                 from a .app bundle to get notifications"
            ));
        }
        if id.is_empty() {
            return Err(format!(
                "the app bundle {path:?} has no CFBundleIdentifier in its Info.plist, which \
                 UNUserNotificationCenter requires"
            ));
        }
        Ok(id)
    }
}

/// `(available, reason)` for `PlatformCapability::notifications()`. Touches
/// UN only once [`bundle_status`] passed, and then only to refresh the
/// authorization reading (throttled; the answer lands for the NEXT probe).
pub(super) fn probe() -> (bool, String) {
    if let Err(reason) = bundle_status() {
        return (false, reason);
    }
    if framework().is_none() {
        return (
            false,
            "UserNotifications.framework could not be loaded (macOS 10.14 or later is required)"
                .to_string(),
        );
    }
    refresh_authorization_throttled();
    match AUTH_STATUS.load(Ordering::Acquire) {
        STATUS_DENIED => (
            false,
            "notifications are turned off for this app (System Settings > Notifications)"
                .to_string(),
        ),
        STATUS_PROVISIONAL => (
            true,
            "provisional: notifications are delivered quietly to Notification Center".to_string(),
        ),
        STATUS_UNREAD => (
            true,
            "the notification permission has not been read from the system yet".to_string(),
        ),
        0 => (
            true,
            "the notification permission is asked by \
             CallbackInfo::request_notification_permission, or at the first post"
                .to_string(),
        ),
        _ => (true, String::new()),
    }
}

/// The notification permission as last read from the OS - what
/// `extra::permission::{macos, ios}::probe_status` answer for
/// `Capability::Notifications`.
pub(super) fn permission_state() -> PermissionState {
    match AUTH_STATUS.load(Ordering::Acquire) {
        STATUS_UNREAD => PermissionState::NotDetermined,
        status => wire::apple_authorization_status(status),
    }
}

/// Remember an authorization status and tell the permission manager, which
/// turns a change into a `PermissionChanged` event for
/// `Capability::Notifications`.
fn store_status(status: i64) {
    AUTH_STATUS.store(status, Ordering::Release);
    push_async_result(
        Capability::Notifications,
        wire::apple_authorization_status(status),
    );
}

/// The answer of a `requestAuthorization...` prompt. A grant is refined by a
/// settings read right after (it may be provisional).
fn report_permission(granted: bool) {
    store_status(if granted {
        STATUS_AUTHORIZED
    } else {
        STATUS_DENIED
    });
    if granted {
        refresh_authorization();
    }
}

/// The shared center - or `None` where touching UN would abort (unbundled)
/// or cannot work (no framework). Never calls `currentNotificationCenter`
/// before [`bundle_status`] has passed.
fn center() -> Option<*mut AnyObject> {
    bundle_status().ok()?;
    framework()?;
    let center_cls = class("UNUserNotificationCenter")?;
    let center: *mut AnyObject = unsafe { msg_send![center_cls, currentNotificationCenter] };
    (!center.is_null()).then_some(center)
}

/// Read the authorization status the OS stored for this app
/// (`getNotificationSettingsWithCompletionHandler:`). The answer arrives on
/// UN's queue and lands in [`AUTH_STATUS`] and the permission manager.
pub(super) fn refresh_authorization() {
    let Some(center) = center() else {
        return;
    };
    let handler = RcBlock::new(|settings: *mut AnyObject| {
        if settings.is_null() {
            return;
        }
        let status: isize = unsafe { msg_send![settings, authorizationStatus] };
        store_status(status as i64);
    });
    unsafe {
        let _: () = msg_send![center, getNotificationSettingsWithCompletionHandler: &*handler];
    }
}

/// [`refresh_authorization`] at most every two seconds - the capability probe
/// may be asked from a layout callback on every frame.
fn refresh_authorization_throttled() {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
    if last.is_some_and(|at| at.elapsed() < Duration::from_secs(2)) {
        return;
    }
    *last = Some(Instant::now());
    drop(last);
    refresh_authorization();
}

/// Everything that must happen before the app finishes launching: set the
/// center's delegate, so the response that launched the app is delivered at
/// all, and read the stored authorization. A no-op for an unbundled process
/// (see the module docs); idempotent.
pub(super) fn install_launch_hooks() {
    let Some(center) = center() else {
        return;
    };
    unsafe { install_delegate(center) };
    refresh_authorization();
}

/// `CallbackInfo::request_notification_permission`: show the prompt (once;
/// afterwards UN answers from the stored choice). The answer reaches the
/// permission manager; until then the capability reads `Requested`, which
/// keeps the capability pump draining.
pub(super) fn request_permission() {
    let Some(center) = center() else {
        // Unbundled: UN cannot be asked at all, and nothing the user does in
        // a prompt changes that.
        push_async_result(Capability::Notifications, PermissionState::Restricted);
        return;
    };
    unsafe { install_delegate(center) };
    push_async_result(Capability::Notifications, PermissionState::Requested);
    let handler = RcBlock::new(|granted: Bool, _error: *mut AnyObject| {
        report_permission(granted.as_bool());
    });
    unsafe {
        let _: () = msg_send![
            center,
            requestAuthorizationWithOptions: AUTH_SOUND_ALERT,
            completionHandler: &*handler
        ];
    }
}

/// `applicationDidFinishLaunching:`: if a notification click launched the
/// app, remember which one, so its response is reported with
/// `launched_app = true`. `notification` is that method's `NSNotification`.
#[cfg(target_os = "macos")]
pub(super) unsafe fn note_launch_notification(notification: *mut AnyObject) {
    if notification.is_null() {
        return;
    }
    unsafe {
        let info: *mut AnyObject = msg_send![notification, userInfo];
        if info.is_null() {
            return;
        }
        let key: *mut AnyObject = NSApplicationLaunchUserNotificationKey;
        if key.is_null() {
            return;
        }
        let response: *mut AnyObject = msg_send![info, objectForKey: key];
        if response.is_null() {
            return;
        }
        // A `UNNotificationResponse`; the deprecated NSUserNotification API
        // put an `NSUserNotification` here, which has no `notification`.
        let is_response: Bool =
            msg_send![response, respondsToSelector: objc2::sel!(notification)];
        if !is_response.as_bool() {
            return;
        }
        if let Some(id) = response_request_identifier(response) {
            *LAUNCH_RESPONSE_ID
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(id);
        }
    }
}

/// `response.notification.request.identifier`.
unsafe fn response_request_identifier(response: *mut AnyObject) -> Option<String> {
    unsafe {
        let notification: *mut AnyObject = msg_send![response, notification];
        if notification.is_null() {
            return None;
        }
        let request: *mut AnyObject = msg_send![notification, request];
        if request.is_null() {
            return None;
        }
        let id_ns: *mut AnyObject = msg_send![request, identifier];
        rust_string(id_ns)
    }
}

/// `response.notification.request.content.userInfo[APPLE_PAYLOAD_KEY]`, or
/// empty.
unsafe fn response_payload(response: *mut AnyObject) -> String {
    unsafe {
        let notification: *mut AnyObject = msg_send![response, notification];
        if notification.is_null() {
            return String::new();
        }
        let request: *mut AnyObject = msg_send![notification, request];
        if request.is_null() {
            return String::new();
        }
        let content: *mut AnyObject = msg_send![request, content];
        if content.is_null() {
            return String::new();
        }
        let info: *mut AnyObject = msg_send![content, userInfo];
        if info.is_null() {
            return String::new();
        }
        let value: *mut AnyObject = msg_send![info, objectForKey: nsstring(wire::APPLE_PAYLOAD_KEY)];
        if value.is_null() {
            return String::new();
        }
        // Only ever an NSString (we put it there), but `UTF8String` on
        // anything else would raise - check rather than trust.
        let Some(string_cls) = class("NSString") else {
            return String::new();
        };
        let is_string: Bool = msg_send![value, isKindOfClass: string_cls];
        if !is_string.as_bool() {
            return String::new();
        }
        rust_string(value).unwrap_or_default()
    }
}

/// Queue an event for the run loop and wake it: the manual loop parks in
/// `runMode:beforeDate:`, which the shared loop waker's app-defined NSEvent
/// ends (`desktop::loop_waker::wake`, callable from UN's queue). Under
/// `NSApplication.run()` the 33 ms drain timer picks the event up and the
/// posted NSEvent is discarded.
fn queue_and_wake(event: NotificationEvent) {
    queue_notification_event(event);
    // macOS: the shared loop waker posts the wake-up event (desktop::loop_waker);
    // iOS needs none - the display tick pumps notifications every frame.
    #[cfg(target_os = "macos")]
    crate::desktop::loop_waker::wake();
}

// ---- the center's delegate -------------------------------------------------

/// Ivars required by `define_class!`; the delegate holds no state (everything
/// it learns goes straight into the mailbox).
#[allow(unreachable_pub)]
pub struct NotificationDelegateIvars {
    _private: u8,
}

define_class!(
    // No `thread_kind`: UN calls the delegate on its own queue, so this class
    // must not claim main-thread-only.
    #[unsafe(super(NSObject))]
    #[name = "AzulNotificationCenterDelegate"]
    #[ivars = NotificationDelegateIvars]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    impl NotificationDelegate {
        /// A click, a button or (with the custom-dismiss category option) a
        /// dismissal.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        unsafe fn did_receive_response(
            &self,
            _center: *mut AnyObject,
            response: *mut AnyObject,
            completion: *mut Block<dyn Fn()>,
        ) {
            unsafe { handle_response(response) };
            if let Some(completion) = unsafe { completion.as_ref() } {
                completion.call(());
            }
        }

        /// Show notifications posted while the app is frontmost - see the
        /// module docs.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        unsafe fn will_present(
            &self,
            _center: *mut AnyObject,
            _notification: *mut AnyObject,
            completion: *mut Block<dyn Fn(usize)>,
        ) {
            if let Some(completion) = unsafe { completion.as_ref() } {
                completion.call((PRESENT_SOUND_ALERT_LIST_BANNER,));
            }
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(NotificationDelegateIvars { _private: 0 });
        unsafe { msg_send![super(this), init] }
    }
}

unsafe fn handle_response(response: *mut AnyObject) {
    if response.is_null() {
        return;
    }
    let (action, app_id, payload) = unsafe {
        let action_ns: *mut AnyObject = msg_send![response, actionIdentifier];
        (
            rust_string(action_ns),
            response_request_identifier(response),
            response_payload(response),
        )
    };
    let (Some(action), Some(app_id)) = (action, app_id) else {
        return;
    };
    let (default_id, dismiss_id) = action_identifiers();
    let mut event = wire::apple_response_event(&app_id, &action, default_id, dismiss_id);
    event.payload = AzString::from(payload);
    {
        let mut launch = LAUNCH_RESPONSE_ID
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if launch.as_deref() == Some(app_id.as_str()) {
            *launch = None;
            event.launched_app = true;
        }
    }
    queue_and_wake(event);
}

/// Set the delegate once per process. UN holds its delegate WEAKLY, so the
/// object is leaked on purpose - it must live as long as the center.
unsafe fn install_delegate(center: *mut AnyObject) {
    static INSTALLED: OnceLock<usize> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let delegate: *mut NotificationDelegate = Retained::into_raw(NotificationDelegate::new());
        let _: () = unsafe { msg_send![center, setDelegate: delegate as *mut AnyObject] };
        delegate as usize
    });
}

// ---- posting ---------------------------------------------------------------

/// A notification as owned Rust data, moved into the authorization handler
/// (which runs on UN's queue, after `post` returned).
struct PendingPost {
    id: String,
    title: String,
    body: String,
    icon: Option<String>,
    actions: Vec<NotificationAction>,
    sound: NotificationSound,
    payload: String,
}

impl PendingPost {
    fn from_notification(n: &Notification) -> Self {
        Self {
            id: n.id.as_str().to_string(),
            title: n.title.as_str().to_string(),
            body: n.body.as_str().to_string(),
            icon: n.icon.as_ref().map(|s| s.as_str().to_string()),
            actions: n.actions.as_ref().to_vec(),
            sound: n.sound.clone(),
            payload: n.payload.as_str().to_string(),
        }
    }

    fn fail(&self, why: String) {
        let mut event =
            NotificationEvent::failed(AzString::from(self.id.clone()), AzString::from(why));
        event.payload = AzString::from(self.payload.clone());
        queue_and_wake(event);
    }
}

/// Every category this process registered, by identifier.
/// `setNotificationCategories:` REPLACES the whole set, so each new one is
/// registered together with all the earlier ones.
static CATEGORIES: Mutex<BTreeMap<String, Vec<NotificationAction>>> = Mutex::new(BTreeMap::new());

unsafe fn make_category(id: &str, actions: &[NotificationAction]) -> Option<Retained<AnyObject>> {
    let action_cls = class("UNNotificationAction")?;
    let category_cls = class("UNNotificationCategory")?;
    let array_cls = class("NSMutableArray")?;
    unsafe {
        let list: Option<Retained<AnyObject>> = msg_send![array_cls, array];
        let list = list?;
        for action in actions {
            if action.id.as_str().is_empty() {
                continue;
            }
            let a: *mut AnyObject = msg_send![
                action_cls,
                actionWithIdentifier: nsstring(action.id.as_str()),
                title: nsstring(action.label.as_str()),
                options: ACTION_OPTION_FOREGROUND
            ];
            if !a.is_null() {
                let _: () = msg_send![&*list, addObject: a];
            }
        }
        let no_intents: Option<Retained<AnyObject>> = msg_send![array_cls, array];
        let no_intents = no_intents?;
        msg_send![
            category_cls,
            categoryWithIdentifier: nsstring(id),
            actions: &*list,
            intentIdentifiers: &*no_intents,
            options: CATEGORY_OPTION_CUSTOM_DISMISS
        ]
    }
}

/// The category for this button set, registered with the center if it is new.
unsafe fn ensure_category(center: *mut AnyObject, actions: &[NotificationAction]) -> String {
    let id = wire::apple_category_id(actions);
    let mut known = CATEGORIES.lock().unwrap_or_else(PoisonError::into_inner);
    if known.contains_key(&id) {
        return id;
    }
    known.insert(id.clone(), actions.to_vec());
    let Some(set_cls) = class("NSMutableSet") else {
        return id;
    };
    unsafe {
        let set: Option<Retained<AnyObject>> = msg_send![set_cls, set];
        let Some(set) = set else {
            return id;
        };
        for (category_id, category_actions) in known.iter() {
            if let Some(category) = make_category(category_id, category_actions) {
                let _: () = msg_send![&*set, addObject: &*category];
            }
        }
        let _: () = msg_send![center, setNotificationCategories: &*set];
    }
    id
}

/// An attachment showing `path`. UN MOVES an attachment's file into its own
/// store once it has validated it, so it gets a copy in the temp directory,
/// never the app's file.
unsafe fn attachment_for(path: &str) -> Option<Retained<AnyObject>> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let src = std::path::Path::new(path);
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let copy = std::env::temp_dir().join(format!(
        "azul-notification-{}-{}.{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed),
        ext
    ));
    if let Err(e) = std::fs::copy(src, &copy) {
        crate::plog_warn!("[notifications] icon {path:?} not attached: {e}");
        return None;
    }
    let copy_str = copy.to_str()?.to_string();
    let url_cls = class("NSURL")?;
    let attachment_cls = class("UNNotificationAttachment")?;
    unsafe {
        let url: *mut AnyObject = msg_send![url_cls, fileURLWithPath: nsstring(&copy_str)];
        if url.is_null() {
            return None;
        }
        let mut error: *mut AnyObject = core::ptr::null_mut();
        let attachment: Option<Retained<AnyObject>> = msg_send![
            attachment_cls,
            attachmentWithIdentifier: nsstring("icon"),
            URL: url,
            options: core::ptr::null_mut::<AnyObject>(),
            error: &mut error
        ];
        if attachment.is_none() {
            crate::plog_warn!(
                "[notifications] icon {path:?} not attached: {}",
                error_description(error).unwrap_or_default()
            );
            let _ = std::fs::remove_file(&copy);
        }
        attachment
    }
}

/// Build the request and add it. Runs on UN's queue, inside an autorelease
/// pool, after authorization was granted.
unsafe fn add_request(center: *mut AnyObject, post: &PendingPost) {
    let Some(content_cls) = class("UNMutableNotificationContent") else {
        return post.fail("UNMutableNotificationContent is missing".to_string());
    };
    let Some(request_cls) = class("UNNotificationRequest") else {
        return post.fail("UNNotificationRequest is missing".to_string());
    };
    unsafe {
        let content: Option<Retained<AnyObject>> = msg_send![content_cls, new];
        let Some(content) = content else {
            return post.fail("could not create the notification content".to_string());
        };
        let _: () = msg_send![&*content, setTitle: nsstring(&post.title)];
        let _: () = msg_send![&*content, setBody: nsstring(&post.body)];

        // No sound object = silent.
        let sound: *mut AnyObject = match (&post.sound, class("UNNotificationSound")) {
            (NotificationSound::Silent, _) | (_, None) => core::ptr::null_mut(),
            (NotificationSound::Default, Some(cls)) => msg_send![cls, defaultSound],
            (NotificationSound::Named(name), Some(cls)) => {
                msg_send![cls, soundNamed: nsstring(name.as_str())]
            }
        };
        if !sound.is_null() {
            let _: () = msg_send![&*content, setSound: sound];
        }

        // Always a category, even with no buttons: the custom-dismiss option
        // lives on it, and without it no dismissal is ever reported.
        let category = ensure_category(center, &post.actions);
        let _: () = msg_send![&*content, setCategoryIdentifier: nsstring(&category)];

        // The payload travels WITH the notification, so a response delivered
        // to a relaunched process still names it.
        if !post.payload.is_empty() {
            if let Some(dict_cls) = class("NSDictionary") {
                let info: *mut AnyObject = msg_send![
                    dict_cls,
                    dictionaryWithObject: nsstring(&post.payload),
                    forKey: nsstring(wire::APPLE_PAYLOAD_KEY)
                ];
                if !info.is_null() {
                    let _: () = msg_send![&*content, setUserInfo: info];
                }
            }
        }

        if let Some(icon) = post.icon.as_deref() {
            if let (Some(attachment), Some(array_cls)) = (attachment_for(icon), class("NSArray")) {
                let list: *mut AnyObject = msg_send![array_cls, arrayWithObject: &*attachment];
                if !list.is_null() {
                    let _: () = msg_send![&*content, setAttachments: list];
                }
            }
        }

        // A nil trigger delivers at once. The same identifier REPLACES a
        // delivered notification - the app id is the request identifier.
        let request: *mut AnyObject = msg_send![
            request_cls,
            requestWithIdentifier: nsstring(&post.id),
            content: &*content,
            trigger: core::ptr::null_mut::<AnyObject>()
        ];
        if request.is_null() {
            return post.fail("could not create the notification request".to_string());
        }

        let id = post.id.clone();
        let payload = post.payload.clone();
        let done = RcBlock::new(move |error: *mut AnyObject| {
            if error.is_null() {
                return;
            }
            let why = unsafe { error_description(error) }
                .unwrap_or_else(|| "UNUserNotificationCenter refused the request".to_string());
            let mut event =
                NotificationEvent::failed(AzString::from(id.clone()), AzString::from(why));
            event.payload = AzString::from(payload.clone());
            queue_and_wake(event);
        });
        let _: () = msg_send![center, addNotificationRequest: request, withCompletionHandler: &*done];
    }
}

/// The live backend. Holds the shared center, which the framework keeps for
/// the life of the process.
pub(super) struct PlatformNotifier {
    center: *mut AnyObject,
}

impl PlatformNotifier {
    pub(super) fn new() -> Result<Self, String> {
        // FIRST, before anything names UN: an unbundled process aborts in
        // `currentNotificationCenter` (module docs).
        bundle_status()?;
        if framework().is_none() {
            return Err(
                "UserNotifications.framework could not be loaded (macOS 10.14 or later is \
                 required)"
                    .to_string(),
            );
        }
        class("UNUserNotificationCenter")
            .ok_or("UNUserNotificationCenter is missing (macOS 10.14 or later is required)")?;
        let center = center()
            .ok_or("+[UNUserNotificationCenter currentNotificationCenter] returned nil")?;
        // Normally installed at launch already (`install_launch_hooks`);
        // idempotent, so a run loop that skipped that still gets it here.
        unsafe { install_delegate(center) };
        crate::plog_info!("[notifications] UNUserNotificationCenter ready");
        Ok(Self { center })
    }

    /// Ask for authorization; the handler adds the request once it is given.
    /// Errors reach the app as `Failed` events from the handlers, so this
    /// itself only fails if it cannot ask at all.
    pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
        let post = PendingPost::from_notification(notification);
        // A raw pointer is not `Send`, and the handler runs on another thread;
        // the center is a process-wide, thread-safe singleton, so its address
        // travels as an integer.
        let center = self.center as usize;
        let handler = RcBlock::new(move |granted: Bool, error: *mut AnyObject| {
            let center = center as *mut AnyObject;
            report_permission(granted.as_bool());
            if granted.as_bool() {
                objc2::rc::autoreleasepool(|_| unsafe { add_request(center, &post) });
            } else {
                let why = unsafe { error_description(error) }.unwrap_or_else(|| {
                    "notifications are turned off for this app (System Settings > \
                     Notifications)"
                        .to_string()
                });
                post.fail(why);
            }
        });
        unsafe {
            let _: () = msg_send![
                self.center,
                requestAuthorizationWithOptions: AUTH_SOUND_ALERT,
                completionHandler: &*handler
            ];
        }
        if !notification.actions.as_ref().is_empty() && notification.callback.is_none() {
            crate::plog_info!(
                "[notifications] {:?} has buttons but no callback: clicks on them reach only \
                 the app-level handler (AppConfig::notification_handler), if the app set one",
                notification.id.as_str()
            );
        }
        Ok(())
    }

    /// Remove the notification, pending or delivered.
    pub(super) fn withdraw(&mut self, id: &str) {
        let center = self.center;
        objc2::rc::autoreleasepool(|_| unsafe {
            let Some(array_cls) = class("NSArray") else {
                return;
            };
            let ids: *mut AnyObject = msg_send![array_cls, arrayWithObject: nsstring(id)];
            if ids.is_null() {
                return;
            }
            let _: () = msg_send![center, removePendingNotificationRequestsWithIdentifiers: ids];
            let _: () = msg_send![center, removeDeliveredNotificationsWithIdentifiers: ids];
        });
    }
}
