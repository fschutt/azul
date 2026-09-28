//! Native desktop notifications  -  platform dispatch. The system tray's
//! sibling (`desktop/tray/`), built the same way.
//!
//! The data model is `azul_core::notification`; the request queue, the event
//! mailbox, the routing and the headless recorder are
//! `azul_layout::managers::notification`. This module is the OS plumbing and
//! the one service that owns it.
//!
//! | | macOS | Linux | Windows |
//! |---|---|---|---|
//! | mechanism | `UNUserNotificationCenter` (UserNotifications.framework, dlopen'd) | `org.freedesktop.Notifications` over D-Bus (the tray's libdbus + session connection) | `Shell_NotifyIconW` balloon (`NIF_INFO`) on a hidden top-level window |
//! | buttons | a `UNNotificationCategory` per button set | the `actions` list, where the server advertises it | none (dropped) |
//! | events arrive | the center's delegate, on UN's own queue | `ActionInvoked` / `NotificationClosed`, in a D-Bus filter | `NIN_BALLOON*`, in the hidden window's procedure |
//! | can it be absent? | **yes**: an unbundled binary has no bundle identifier | **yes**: no server on the session bus | practically no |
//!
//! # Flow - the tray's
//!
//! 1. A callback calls `CallbackInfo::post_notification`; the request is parked in the layout
//!    queue (a `CallbackInfo` cannot reach an OS API).
//! 2. [`dispatch_queued_requests`] hands it to the backend, on the main thread: from the capability
//!    pump at the top of every event pass (every target, mobile included), and from
//!    [`pump_notifications`], which each desktop run loop calls next to its tray pump.
//! 3. The OS reports a click on whatever thread it likes; the backend queues a `NotificationEvent`
//!    into the layout mailbox (the tray's `queue_tray_event`) and, on macOS, wakes the run loop.
//! 4. [`pump_notifications`] routes the mailbox to the callbacks of the notifications the events
//!    name, and the run loop's app-event collector (`desktop::app_events`) runs them with
//!    [`invoke_deliveries`] - through `invoke_menu_callback` against the most recently focused
//!    window (else the oldest), exactly as it runs a tray menu item's callback, with the event
//!    installed for `CallbackInfo::get_notification_event`.
//!
//! # Failures are events
//!
//! A backend that cannot start (an unbundled macOS binary, no freedesktop server, a mobile target)
//! turns every post into a `Failed` event carrying the reason, and logs it. Nothing is dropped in
//! silence, and `PlatformCapability::notifications()` reports the same reason up front.

use core::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use azul_core::notification::{Notification, NotificationEvent};
use azul_css::AzString;
use azul_layout::managers::notification::{
    self as queue, NotificationDelivery, NotificationRegistry, NotificationRequest,
};

use crate::desktop::extra::capability::PlatformCapability;

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Set by the headless run loop (`AZ_BACKEND=headless`): no notification is
/// shown, every post is RECORDED for tests and `assert_notification`.
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// Why a target without a backend cannot show a notification.
const UNSUPPORTED: &str =
    "native notifications are not implemented on this target yet (desktop only: macOS, Linux, \
     Windows)";

/// Record instead of show. Called by the headless run loop before its window
/// exists, i.e. before anything can post.
pub fn use_headless_backend() {
    HEADLESS.store(true, Ordering::Relaxed);
}

/// Whatever shows (or records) the notifications.
enum Backend {
    /// `AZ_BACKEND=headless`: record, never show.
    Headless,
    #[cfg(target_os = "macos")]
    MacOs(macos::PlatformNotifier),
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    Linux(linux::PlatformNotifier),
    #[cfg(target_os = "windows")]
    Windows(windows::PlatformNotifier),
    /// No backend could start; every post fails with this reason.
    Unavailable(String),
}

#[cfg(target_os = "macos")]
fn platform_backend() -> Backend {
    match macos::PlatformNotifier::new() {
        Ok(n) => Backend::MacOs(n),
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
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32"))
)))]
fn platform_backend() -> Backend {
    Backend::Unavailable(UNSUPPORTED.to_string())
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
}

impl NotificationService {
    fn post(&mut self, notification: Notification) {
        let notification = self.registry.admit(notification);
        let result: Result<(), String> = match &mut self.backend {
            Backend::Headless => {
                queue::record_posted_notification(&notification);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Backend::MacOs(n) => n.post(&notification),
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
            queue::queue_notification_event(NotificationEvent::failed(
                notification.id.clone(),
                AzString::from(reason),
            ));
        }
    }

    fn withdraw(&mut self, id: &str) {
        self.registry.forget(id);
        match &mut self.backend {
            Backend::Headless => {
                queue::record_withdrawn_notification(id);
            }
            #[cfg(target_os = "macos")]
            Backend::MacOs(n) => n.withdraw(id),
            #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
            Backend::Linux(n) => n.withdraw(id),
            #[cfg(target_os = "windows")]
            Backend::Windows(n) => n.withdraw(id),
            Backend::Unavailable(_) => {}
        }
    }

    /// Let the backend read what the OS reported since the last call. macOS
    /// has nothing to read: its delegate queues events as they happen.
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
    /// first request, so an app that never posts pays nothing.
    static SERVICE: RefCell<Option<NotificationService>> = const { RefCell::new(None) };
}

fn with_service<R>(f: impl FnOnce(&mut NotificationService) -> R) -> Option<R> {
    SERVICE
        .try_with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            let service = slot.get_or_insert_with(|| NotificationService {
                backend: start_backend(),
                registry: NotificationRegistry::new(),
            });
            Some(f(service))
        })
        .ok()
        .flatten()
}

/// Hand every queued post / withdraw to the backend. Main thread only; a
/// no-op (the service is not even started) while nothing is queued.
pub fn dispatch_queued_requests() {
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

/// Per-iteration run-loop work, next to the tray's `pump_tray`: dispatch
/// queued requests, let the backend read the OS's reports, and route the
/// mailbox. Returns the callbacks to run - with [`invoke_deliveries`] against
/// a window, because a `CallbackInfo` needs one and a notification has none.
#[must_use]
pub fn pump_notifications() -> Vec<NotificationDelivery> {
    dispatch_queued_requests();
    let started = SERVICE
        .try_with(|cell| cell.try_borrow().map(|s| s.is_some()).unwrap_or(false))
        .unwrap_or(false);
    if !started && !queue::has_queued_events() {
        return Vec::new();
    }
    with_service(|service| {
        service.pump_platform();
        service.registry.route(queue::drain_notification_events())
    })
    .unwrap_or_default()
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

#[cfg(target_os = "macos")]
fn platform_probe() -> PlatformCapability {
    let (available, reason) = macos::probe();
    cap(available, "UNUserNotificationCenter".to_string(), reason)
}

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn platform_probe() -> PlatformCapability {
    let (available, backend, reason) = linux::probe();
    cap(available, backend, reason)
}

#[cfg(target_os = "windows")]
fn platform_probe() -> PlatformCapability {
    let (available, reason) = windows::probe();
    cap(
        available,
        "Shell_NotifyIconW balloon (NIF_INFO)".to_string(),
        reason,
    )
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", not(target_arch = "wasm32"))
)))]
fn platform_probe() -> PlatformCapability {
    cap(false, "none".to_string(), UNSUPPORTED.to_string())
}
