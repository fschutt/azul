//! Native notifications  -  platform dispatch. The system tray's sibling
//! (`desktop/tray/`), built the same way.
//!
//! The data model is `azul_core::notification`; the request queue, the event
//! mailbox, the routing and the headless recorder are
//! `azul_layout::managers::notification`. This module is the OS plumbing and
//! the one service that owns it.
//!
//! | | macOS / iOS | Linux | Windows | Android |
//! |---|---|---|---|---|
//! | mechanism | `UNUserNotificationCenter` (UserNotifications.framework, dlopen'd) | `org.freedesktop.Notifications` over D-Bus (the tray's libdbus + session connection), `Notify` sent without waiting; inside Flatpak `org.freedesktop.portal.Notification` on the same connection | a WinRT toast under an AUMID registered in HKCU at first use, with a COM activator for clicks after exit; the `Shell_NotifyIconW` balloon when that fails | `NotificationManager` + a channel, through `AzulNotifications.java` over JNI |
//! | buttons | a `UNNotificationCategory` per button set | the `actions` list, where the server advertises it | toast `<action>`s (none on the balloon) | up to three `Notification.Action`s |
//! | events arrive | the center's delegate, on UN's own queue | `ActionInvoked` / `NotificationClosed` (+ `ActivationToken`), in a D-Bus filter | `ToastNotification.Activated/Dismissed/Failed` on a thread-pool thread | the launch / `onNewIntent` intent and a manifest receiver, forwarded by the Java helper |
//! | can it be absent? | **yes** on macOS: an unbundled binary has no bundle identifier | **yes**: no server on the session bus | practically no | no (Android 13+ needs the permission) |
//!
//! # Flow - the tray's
//!
//! 1. A callback calls `CallbackInfo::post_notification`; the request is parked in the layout
//!    queue (a `CallbackInfo` cannot reach an OS API). A post the queue has no room for becomes a
//!    `Failed` delivery right there.
//! 2. [`dispatch_queued_requests`] hands it to the backend, on the main thread: from the capability
//!    pump at the top of every event pass (every target, mobile included), and from
//!    [`pump_notifications`], which each run loop calls next to its tray pump (the iOS display
//!    tick and the Android loop included).
//! 3. The OS reports a click on whatever thread it likes; the backend queues a `NotificationEvent`
//!    into the layout mailbox (the tray's `queue_tray_event`) and wakes the run loop where it has
//!    to.
//! 4. [`pump_notifications`] routes the mailbox to the callbacks of the notifications the events
//!    name - or to the APP-LEVEL handler (`AppConfig::notification_handler`, installed by
//!    [`set_app_handler`]) for events no live callback owns, such as the tap that launched the
//!    app - and the run loop's app-event collector (`desktop::app_events`) runs them with
//!    [`invoke_deliveries`] - through `invoke_menu_callback` against the most recently focused
//!    window (else the oldest), exactly as it runs a tray menu item's callback, with the event
//!    installed for `CallbackInfo::get_notification_event`.
//! 5. A loop with no window parks the deliveries with [`defer_deliveries`]; the next pump that has
//!    one runs them.
//!
//! # Scheduled notifications (`Notification::deliver_at`)
//!
//! A notification with a delivery time is SCHEDULED: macOS / iOS hand it to UN with a
//! `UNTimeIntervalNotificationTrigger`, a Windows toast becomes a `ScheduledToastNotification`
//! (`ToastNotifier::AddToSchedule`) - both show it also while the app is not running. The
//! freedesktop server, the portal, Android (until its alarm receiver exists) and the Windows
//! balloon cannot schedule; the service HOLDS such a notification
//! (`azul_layout::managers::notification::ScheduledNotifications`), a deadline thread wakes the
//! run loop (`loop_waker::wake`) when the earliest is due, and the pump posts it then - it shows
//! only while the process runs. A withdraw cancels a scheduled notification on every backend.
//!
//! # Failures are events
//!
//! A backend that cannot start (an unbundled macOS binary, no freedesktop server, an Android build
//! without JNI) turns every post into a `Failed` event carrying the reason, and logs it. Nothing is
//! dropped in silence, and `PlatformCapability::notifications()` reports the same reason up front.

use core::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use azul_core::notification::{Notification, NotificationEvent, OptionNotificationCallback};
use azul_css::{AzString, OptionU64};
use azul_layout::managers::{
    notification::{
        self as queue, NotificationDelivery, NotificationRegistry, NotificationRequest,
        ScheduledNotifications,
    },
    permission::{push_async_result, Capability, PermissionQuality, PermissionState},
};

use crate::desktop::extra::capability::PlatformCapability;

/// `pub` for its JNI entry point (`nativeOnNotificationEvent`), which Java
/// resolves by symbol name - like `extra::media_keys::android`.
#[cfg(all(target_os = "android", feature = "jni"))]
pub mod android;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux;
#[cfg(target_os = "windows")]
mod windows;
/// Source-text invariants for the platform code a host cannot run.
#[cfg(test)]
mod platform_invariants;

/// Set by the headless run loop (`AZ_BACKEND=headless`): no notification is
/// shown, every post is RECORDED for tests and `assert_notification`.
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// Why a target without a backend cannot show a notification.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32")),
    all(target_os = "android", feature = "jni")
)))]
const UNSUPPORTED: &str = "native notifications are not implemented on this target (macOS, iOS, \
                           Android with the `jni` feature, Linux and Windows have them)";

/// Record instead of show. Called by the headless run loop before its window
/// exists, i.e. before anything can post.
pub fn use_headless_backend() {
    HEADLESS.store(true, Ordering::Relaxed);
}

/// Install the app-level handler (`AppConfig::notification_handler`): where
/// events go that no notification callback owns. Called by `App::run` before
/// the run loop starts, so the tap that launched the app finds it.
pub fn set_app_handler(handler: OptionNotificationCallback) {
    queue::set_app_notification_handler(handler);
}

/// What must happen before the app finishes launching, so a notification
/// click that LAUNCHED the app is not lost:
///
/// * macOS and iOS: the `UNUserNotificationCenter` delegate (the response is only delivered to a
///   delegate set before launch completes) and a first read of the stored authorization. Called
///   between the app delegate and `finishLaunching` on macOS and from
///   `application:didFinishLaunchingWithOptions:` on iOS.
/// * Windows: the toast activator's COM class object, when COM started the process for a toast
///   click (`-ToastActivated`) or an earlier run registered it. Called by the Windows `run()`
///   before its first window.
///
/// Android needs nothing here: its launch intent is forwarded by `AzulActivity`.
pub fn install_launch_hooks() {
    if HEADLESS.load(Ordering::Relaxed) {
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    apple::install_launch_hooks();
    #[cfg(target_os = "windows")]
    windows::install_launch_hooks();
}

/// `applicationDidFinishLaunching:` (macOS): remember which notification, if
/// any, launched the app, so its response is reported with
/// `NotificationEvent::launched_app`. `notification` is the `NSNotification`
/// that method receives.
///
/// # Safety
///
/// `notification` must be null or a live `NSNotification`.
#[cfg(target_os = "macos")]
pub unsafe fn note_launch_notification(notification: *mut core::ffi::c_void) {
    if HEADLESS.load(Ordering::Relaxed) {
        return;
    }
    unsafe { apple::note_launch_notification(notification.cast()) };
}

/// The app became active (`applicationDidBecomeActive:` on macOS and iOS).
///
/// * Re-read the notification permission the OS keeps for this app - the user may have changed
///   it in System Settings while the app was in the background. Only macOS and iOS store a
///   decision that can change behind the app's back and be read cheaply.
/// * End the launch: on iOS the response that LAUNCHED the app arrives before the first
///   activation, so one arriving later is a tap on a running app (`launched_app` stays false).
pub fn app_became_active() {
    if HEADLESS.load(Ordering::Relaxed) {
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    apple::app_became_active();
}

/// The bundle's `CFBundleIdentifier` when this process runs from a `.app`,
/// else `None` - what `desktop::app_identity` declares on macOS and iOS.
/// Touches only `NSBundle`, never UN.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[must_use]
pub fn apple_bundle_id() -> Option<String> {
    apple::bundle_status().ok()
}

/// The notification permission as last read from `UNUserNotificationCenter`
/// (`NotDetermined` before the first reading). What the Apple permission
/// backends answer for `Capability::Notifications`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[must_use]
pub fn apple_permission_state() -> PermissionState {
    apple::permission_state()
}

/// The `ActivationToken` a freedesktop server sent with the last click on one
/// of this app's notifications, taken. On Wayland it is what lets the app
/// raise its window (`xdg_activation_v1.activate`): a click on a notification
/// carries no input serial, so nothing else may.
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
#[must_use]
pub fn take_activation_token() -> Option<String> {
    linux::take_activation_token()
}

/// No freedesktop server, no activation token.
#[cfg(not(all(target_os = "linux", not(target_arch = "wasm32"))))]
#[must_use]
pub fn take_activation_token() -> Option<String> {
    None
}

/// Whatever shows (or records) the notifications.
enum Backend {
    /// `AZ_BACKEND=headless`: record, never show.
    Headless,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Apple(apple::PlatformNotifier),
    #[cfg(all(target_os = "android", feature = "jni"))]
    Android(android::PlatformNotifier),
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    Linux(linux::PlatformNotifier),
    #[cfg(target_os = "windows")]
    Windows(windows::PlatformNotifier),
    /// No backend could start; every post fails with this reason.
    Unavailable(String),
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_backend() -> Backend {
    match apple::PlatformNotifier::new() {
        Ok(n) => Backend::Apple(n),
        Err(reason) => Backend::Unavailable(reason),
    }
}

#[cfg(all(target_os = "android", feature = "jni"))]
fn platform_backend() -> Backend {
    match android::PlatformNotifier::new() {
        Ok(n) => Backend::Android(n),
        Err(reason) => Backend::Unavailable(reason),
    }
}

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn platform_backend() -> Backend {
    match linux::PlatformNotifier::new() {
        Ok(n) => Backend::Linux(n),
        Err(reason) => Backend::Unavailable(reason),
    }
}

#[cfg(target_os = "windows")]
fn platform_backend() -> Backend {
    match windows::PlatformNotifier::new() {
        Ok(n) => Backend::Windows(n),
        Err(reason) => Backend::Unavailable(reason),
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32")),
    all(target_os = "android", feature = "jni")
)))]
fn platform_backend() -> Backend {
    Backend::Unavailable(UNSUPPORTED.to_string())
}

impl Backend {
    /// Does this backend keep a notification's delivery time itself (the OS
    /// shows it then), or must the process hold it until it is due? The
    /// headless recorder records the time as it is; an unavailable backend
    /// fails the post at once, which tells the app more than a failure at the
    /// time it was due.
    fn schedules_itself(&self) -> bool {
        match self {
            Backend::Headless | Backend::Unavailable(_) => true,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Backend::Apple(_) => true,
            #[cfg(target_os = "windows")]
            Backend::Windows(n) => n.can_schedule(),
            #[cfg(all(target_os = "android", feature = "jni"))]
            Backend::Android(_) => false,
            #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
            Backend::Linux(_) => false,
        }
    }
}

/// Milliseconds since 1970 by the wall clock (a delivery time is wall time).
fn wall_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The thread that wakes the run loop when a notification the process HOLDS
/// is due (`ScheduledNotifications`): the loops park until an event comes,
/// and nothing else would come at that time. It sleeps on the WALL clock,
/// at most 30 s at a time, so a suspend or a clock change delays a
/// notification by no more than that.
mod deadline {
    use std::sync::{Mutex, OnceLock, PoisonError};

    /// The earliest held notification's time (ms since 1970).
    static NEXT: Mutex<Option<u64>> = Mutex::new(None);
    static THREAD: OnceLock<Option<std::thread::Thread>> = OnceLock::new();
    const MAX_SLEEP_MS: u64 = 30_000;

    /// The earliest due time changed (`None`: nothing is held).
    pub(super) fn set(next: Option<u64>) {
        *NEXT.lock().unwrap_or_else(PoisonError::into_inner) = next;
        let thread = if next.is_some() {
            THREAD.get_or_init(spawn).as_ref()
        } else {
            THREAD.get().and_then(Option::as_ref)
        };
        if let Some(thread) = thread {
            thread.unpark();
        }
    }

    fn spawn() -> Option<std::thread::Thread> {
        match std::thread::Builder::new()
            .name("azul-notification-deadline".to_string())
            .spawn(run)
        {
            Ok(handle) => Some(handle.thread().clone()),
            Err(e) => {
                crate::plog_warn!(
                    "[notifications] no deadline thread ({e}): a held notification shows at the \
                     next event the loop wakes for"
                );
                None
            }
        }
    }

    fn run() {
        loop {
            let next = *NEXT.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(at) = next else {
                std::thread::park();
                continue;
            };
            let now = super::wall_clock_ms();
            if now >= at {
                {
                    let mut slot = NEXT.lock().unwrap_or_else(PoisonError::into_inner);
                    if *slot == Some(at) {
                        // The pump sets the next one after it posted this.
                        *slot = None;
                    }
                }
                crate::desktop::loop_waker::wake();
            } else {
                std::thread::park_timeout(std::time::Duration::from_millis(
                    (at - now).min(MAX_SLEEP_MS),
                ));
            }
        }
    }
}

fn start_backend() -> Backend {
    if HEADLESS.load(Ordering::Relaxed) {
        crate::plog_info!(
            "[notifications] headless: posted notifications are recorded, not shown"
        );
        return Backend::Headless;
    }
    let backend = platform_backend();
    if let Backend::Unavailable(reason) = &backend {
        // Loud on purpose, like the tray: a notification that silently never
        // appears is indistinguishable from one the user simply missed.
        crate::plog_warn!("[notifications] no notification backend: {reason}");
    }
    backend
}

/// The backend plus the registry that routes its events.
struct NotificationService {
    backend: Backend,
    registry: NotificationRegistry,
    /// Scheduled notifications the backend cannot keep, until they are due.
    held: ScheduledNotifications,
}

impl NotificationService {
    fn post(&mut self, notification: Notification) {
        let later =
            queue::wire::delivery_delay_ms(notification.deliver_at.into_option(), wall_clock_ms())
                .is_some();
        if later && !self.backend.schedules_itself() {
            self.hold(notification);
            return;
        }
        let notification = self.registry.admit(notification);
        let result: Result<(), String> = match &mut self.backend {
            Backend::Headless => {
                queue::record_posted_notification(&notification);
                Ok(())
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Backend::Apple(n) => n.post(&notification),
            #[cfg(all(target_os = "android", feature = "jni"))]
            Backend::Android(n) => n.post(&notification),
            #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
            Backend::Linux(n) => n.post(&notification),
            #[cfg(target_os = "windows")]
            Backend::Windows(n) => n.post(&notification),
            Backend::Unavailable(reason) => Err(reason.clone()),
        };
        if let Err(reason) = result {
            crate::plog_warn!(
                "[notifications] could not show {:?}: {}",
                notification.id.as_str(),
                reason
            );
            // Routed by the next pump to the notification's own callback.
            let mut event =
                NotificationEvent::failed(notification.id.clone(), AzString::from(reason));
            event.payload = notification.payload.clone();
            if !queue::queue_notification_event(event) {
                crate::plog_warn!(
                    "[notifications] the failure of {:?} was not reported to the app: the \
                     event mailbox is full",
                    notification.id.as_str()
                );
            }
        }
    }

    /// Keep a scheduled notification until it is due (a backend that cannot
    /// schedule); the deadline thread wakes the loop then.
    fn hold(&mut self, notification: Notification) {
        let notification = self.registry.admit(notification);
        if let Err(refused) = self.held.schedule(notification) {
            crate::plog_warn!(
                "[notifications] could not schedule {:?}: too many are waiting",
                refused.id.as_str()
            );
            let mut event = NotificationEvent::failed(
                refused.id.clone(),
                AzString::from_const_str(
                    "too many scheduled notifications are waiting in this process",
                ),
            );
            event.payload = refused.payload.clone();
            let _ = queue::queue_notification_event(event);
        }
        deadline::set(self.held.next_due_ms());
    }

    /// Post the held notifications that are due.
    fn post_due(&mut self) {
        if self.held.is_empty() {
            return;
        }
        for mut notification in self.held.take_due(wall_clock_ms()) {
            notification.deliver_at = OptionU64::None;
            self.post(notification);
        }
        deadline::set(self.held.next_due_ms());
    }

    fn withdraw(&mut self, id: &str) {
        self.registry.forget(id);
        if self.held.withdraw(id) {
            deadline::set(self.held.next_due_ms());
        }
        match &mut self.backend {
            Backend::Headless => {
                queue::record_withdrawn_notification(id);
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Backend::Apple(n) => n.withdraw(id),
            #[cfg(all(target_os = "android", feature = "jni"))]
            Backend::Android(n) => n.withdraw(id),
            #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
            Backend::Linux(n) => n.withdraw(id),
            #[cfg(target_os = "windows")]
            Backend::Windows(n) => n.withdraw(id),
            Backend::Unavailable(_) => {}
        }
    }

    /// Let the backend read what the OS reported since the last call. macOS,
    /// iOS and Android have nothing to read: their callbacks queue events as
    /// they happen.
    fn pump_platform(&mut self) {
        #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
        if let Backend::Linux(n) = &mut self.backend {
            n.pump();
        }
        #[cfg(target_os = "windows")]
        if let Backend::Windows(n) = &mut self.backend {
            n.pump();
        }
    }

    fn shutdown(&mut self) {
        #[cfg(target_os = "windows")]
        if let Backend::Windows(n) = &mut self.backend {
            n.shutdown();
        }
    }
}

thread_local! {
    /// The live service, owned by the event-loop thread - a thread-local, like
    /// the tray, because the macOS backend holds Objective-C objects and the
    /// Windows one a window that belong to that thread. Started lazily by the
    /// first request (or the first event, e.g. the tap that launched the
    /// app), so an app that never posts pays nothing.
    static SERVICE: RefCell<Option<NotificationService>> = const { RefCell::new(None) };
}

fn with_service<R>(f: impl FnOnce(&mut NotificationService) -> R) -> Option<R> {
    SERVICE
        .try_with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            let service = slot.get_or_insert_with(|| NotificationService {
                backend: start_backend(),
                registry: NotificationRegistry::new(),
                held: ScheduledNotifications::new(),
            });
            Some(f(service))
        })
        .ok()
        .flatten()
}

/// Hand every queued post / withdraw - and a pending permission request - to
/// the backend. Main thread only; a no-op (the service is not even started)
/// while nothing is queued.
pub fn dispatch_queued_requests() {
    if queue::take_notification_permission_request() {
        request_permission();
    }
    let requests = queue::drain_notification_requests();
    if requests.is_empty() {
        return;
    }
    let count = requests.len();
    let dispatched = with_service(|service| {
        for request in requests {
            match request {
                NotificationRequest::Post(notification) => service.post(notification),
                NotificationRequest::Withdraw(id) => service.withdraw(id.as_str()),
            }
        }
    });
    if dispatched.is_none() {
        // Only reachable from a re-entrant dispatch (the service is borrowed
        // for a post already in progress) or during thread teardown. Neither
        // happens in the run loops as written; say so if it ever does.
        crate::plog_warn!(
            "[notifications] {count} request(s) dropped: the notification service was busy"
        );
    }
}

/// `CallbackInfo::request_notification_permission`, on the main thread. The
/// answer reaches the permission manager (`Capability::Notifications`) through
/// its async channel - at once where there is nothing to ask, from the
/// prompt's completion where there is.
fn request_permission() {
    if HEADLESS.load(Ordering::Relaxed) {
        // Every post is recorded: as granted as it gets.
        push_async_result(
            Capability::Notifications,
            PermissionState::Granted(PermissionQuality::Full),
        );
        return;
    }
    platform_request_permission();
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_request_permission() {
    apple::request_permission();
}

#[cfg(all(target_os = "android", feature = "jni"))]
fn platform_request_permission() {
    android::request_permission();
}

/// freedesktop has no permission to ask for: notifications can be shown if a
/// server answers, and cannot otherwise - which no prompt changes.
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn platform_request_permission() {
    let (available, _, _) = linux::probe();
    push_async_result(
        Capability::Notifications,
        if available {
            PermissionState::Granted(PermissionQuality::Full)
        } else {
            PermissionState::Restricted
        },
    );
}

/// Windows has no prompt either; the toast notifier says whether the user
/// (or a policy) turned this app's notifications off.
#[cfg(target_os = "windows")]
fn platform_request_permission() {
    push_async_result(Capability::Notifications, windows::permission_state());
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32")),
    all(target_os = "android", feature = "jni")
)))]
fn platform_request_permission() {
    push_async_result(Capability::Notifications, PermissionState::Restricted);
}

/// Per-iteration run-loop work, next to the tray's `pump_tray`: dispatch
/// queued requests, let the backend read the OS's reports, and route the
/// mailbox. Returns the callbacks to run - deliveries that waited for a window
/// first, oldest first - with [`invoke_deliveries`] against a window, because a
/// `CallbackInfo` needs one and a notification has none. A loop with no window
/// hands them to [`defer_deliveries`].
#[must_use]
pub fn pump_notifications() -> Vec<NotificationDelivery> {
    dispatch_queued_requests();
    let mut out = queue::drain_notification_deliveries();
    let started = SERVICE
        .try_with(|cell| cell.try_borrow().map(|s| s.is_some()).unwrap_or(false))
        .unwrap_or(false);
    if !started && !queue::has_queued_events() {
        return out;
    }
    let routed = with_service(|service| {
        service.post_due();
        service.pump_platform();
        let events = queue::drain_notification_events();
        if events.is_empty() {
            return Vec::new();
        }
        service
            .registry
            .set_app_handler(queue::app_notification_handler());
        service.registry.route(events)
    })
    .unwrap_or_default();
    out.extend(routed);
    out
}

/// Keep deliveries a loop could not run - it has no window right now (the
/// last one closed, a tray-only phase) - until the next pump that has one.
/// Bounded: beyond the queue's limit the oldest waiting ones stay and the
/// newest are dropped, with a warning.
pub fn defer_deliveries(deliveries: Vec<NotificationDelivery>) {
    let count = deliveries.len();
    let mut dropped = 0usize;
    for delivery in deliveries {
        if !queue::queue_notification_delivery(delivery) {
            dropped += 1;
        }
    }
    if dropped > 0 {
        crate::plog_warn!(
            "[notifications] {dropped} of {count} event(s) dropped: no window to run them \
             against, and the queue of waiting ones is full"
        );
    } else {
        crate::plog_debug!(
            "[notifications] {count} event(s) wait for a window to run against"
        );
    }
}

/// Is a notification outstanding, or a request / event waiting? (The Linux
/// loops no longer bound their park on this: the server's signals arrive on
/// the D-Bus socket, which is in their wait set through `desktop::loop_waker`,
/// and `loop_waker::must_not_park` covers a queued request or event.)
#[must_use]
pub fn needs_polling() -> bool {
    let live = SERVICE
        .try_with(|cell| {
            cell.try_borrow()
                .map(|s| s.as_ref().is_some_and(|s| s.registry.live_count() > 0))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    live || queue::has_queued_requests() || queue::has_queued_events()
}

/// Tear the backend down before the process exits: on Windows this removes
/// the notify icon, which the shell would otherwise leave in the
/// notification area until the mouse passes over it.
pub fn shutdown() {
    let _ = SERVICE.try_with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            if let Some(service) = slot.as_mut() {
                service.shutdown();
            }
            *slot = None;
        }
    });
}

/// Run routed notification callbacks against `window`.
///
/// The SAME path a tray menu item's callback takes: `invoke_menu_callback`
/// builds the `CallbackInfo`, hands over the notification's `RefAny`, applies
/// whatever the callback changed and asks for a rebuild. The event is
/// installed for the call so the callback can read it with
/// `CallbackInfo::get_notification_event`. Returns whether anything asked for
/// a repaint; the caller owns that decision (`request_redraw` is not on the
/// `PlatformWindow` trait).
pub(crate) fn invoke_deliveries<W: crate::desktop::shell2::common::event::PlatformWindow>(
    window: &mut W,
    deliveries: Vec<NotificationDelivery>,
) -> bool {
    use azul_core::events::ProcessEventResult;

    use crate::desktop::shell2::common::event::MenuInvocation;

    let mut needs_redraw = false;
    for delivery in deliveries {
        let NotificationDelivery { callback, event } = delivery;
        let menu_callback = azul_core::menu::CoreMenuCallback {
            refany: callback.refany,
            callback: callback.callback,
        };
        let result = queue::with_current_notification_event(&event, || {
            window.invoke_menu_callback(
                menu_callback,
                MenuInvocation::Native {
                    site: "notification",
                },
            )
        });
        if !matches!(result, ProcessEventResult::DoNothing) {
            needs_redraw = true;
        }
    }
    needs_redraw
}

fn cap(available: bool, backend: String, reason: String) -> PlatformCapability {
    PlatformCapability {
        available,
        backend: AzString::from(backend),
        reason: AzString::from(reason),
    }
}

/// `PlatformCapability::notifications()`: can this process show a native
/// notification, through which backend, and if not, why. Non-destructive: it
/// never starts the backend (on macOS it must not even touch
/// `UNUserNotificationCenter` before the bundle check has passed).
#[must_use]
pub fn probe() -> PlatformCapability {
    if HEADLESS.load(Ordering::Relaxed) {
        return cap(
            false,
            "headless (recorded)".to_string(),
            "no notification server in a headless run: posts are recorded for tests and the \
             `assert_notification` E2E assertion instead"
                .to_string(),
        );
    }
    platform_probe()
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_probe() -> PlatformCapability {
    let (available, reason) = apple::probe();
    cap(available, "UNUserNotificationCenter".to_string(), reason)
}

#[cfg(all(target_os = "android", feature = "jni"))]
fn platform_probe() -> PlatformCapability {
    let (available, reason) = android::probe();
    cap(
        available,
        "NotificationManager (AzulNotifications.java)".to_string(),
        reason,
    )
}

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn platform_probe() -> PlatformCapability {
    let (available, backend, reason) = linux::probe();
    cap(available, backend, reason)
}

#[cfg(target_os = "windows")]
fn platform_probe() -> PlatformCapability {
    let (available, backend, reason) = windows::probe();
    cap(available, backend, reason)
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32")),
    all(target_os = "android", feature = "jni")
)))]
fn platform_probe() -> PlatformCapability {
    cap(false, "none".to_string(), UNSUPPORTED.to_string())
}
