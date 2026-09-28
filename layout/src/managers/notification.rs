//! Native notifications  -  the platform-independent half.
//!
//! The data model is `azul_core::notification`; the OS plumbing is
//! `azul-dll`'s `desktop/notifications/`. This module is everything in between,
//! and it is shaped after the system tray (`azul-dll` `desktop/tray/mod.rs`):
//!
//! ```text
//!  callback ──post/withdraw──▶ REQUEST QUEUE ──drained by the dll pump──▶ OS backend
//!                                                                           │
//!  callback ◀─invoke_menu_callback─ run loop ◀─route─ EVENT MAILBOX ◀──────┘ (any thread)
//! ```
//!
//! * **The request queue.** `CallbackInfo::post_notification` / `withdraw_notification` cannot
//!   reach an OS API - `azul-layout` has none - so they park a [`NotificationRequest`] here, the
//!   keyring's shape. The dll drains it on the main thread (its capability pump, and the run-loop
//!   pump next to the tray's) and hands each request to the backend.
//! * **The event mailbox.** The OS reports a click on whatever thread it likes: a GCD queue for the
//!   `UNUserNotificationCenter` delegate, a D-Bus dispatch for `ActionInvoked`, the window
//!   procedure for a balloon. None of them can hold a `CallbackInfo`, so they queue a
//!   [`NotificationEvent`] here - the tray's `queue_tray_event` mailbox, bounded the same way.
//! * **Routing.** [`NotificationRegistry`] maps the app's notification id to the callback the
//!   notification carries. The run loop drains the mailbox through it and invokes each routed
//!   callback with `invoke_menu_callback` against the window [`super::app_target`] picks (the
//!   most recently focused, else the oldest), exactly as it invokes a tray menu item's callback.
//!   Every event is the notification's last, so the registry forgets the callback as it routes -
//!   and remembers the id as ENDED, which is what stops the `NotificationClosed` a freedesktop
//!   server sends after `ActionInvoked` from arriving as a second event.
//! * **The app-level handler** (`AppConfig::notification_handler`, installed here with
//!   [`set_app_notification_handler`]) receives what no notification callback owns: an event for
//!   an id this process never posted (a tap that cold-launched the app, a relaunch from
//!   Notification Center) and the events of a notification posted without a callback. The
//!   notification's `payload` rides along into the event, from the platform where it carries it
//!   back and from the registry otherwise.
//! * **Deliveries that cannot run yet.** A delivery needs a window to run against; a loop with
//!   none parks it in [`queue_notification_delivery`] until one exists. A post the request queue
//!   has no room for is not dropped either: [`reject_notification`] turns it into a `Failed`
//!   delivery the same way.
//! * **The permission request.** `CallbackInfo::request_notification_permission` sets a flag the
//!   dll's dispatch reads ([`take_notification_permission_request`]); the answer comes back
//!   through the permission manager as `Capability::Notifications`.
//! * **The current event.** A callback learns which event it runs for from
//!   `CallbackInfo::get_notification_event`, which reads the slot
//!   [`with_current_notification_event`] fills for the duration of one delivery. Any other
//!   callback reads `None`.
//! * **The headless recorder.** `AZ_BACKEND=headless` has no notification server; its backend
//!   RECORDS what was posted and withdrawn instead, so a test or an AZ_E2E scenario
//!   (`assert_notification`) can assert on it.
//! * **[`wire`]**: each platform's vocabulary (freedesktop action keys and close reasons, UN
//!   action identifiers, balloon messages) translated to the one event type here, where every host
//!   compiles and tests it, rather than inside `#[cfg(target_os)]` code only that OS builds.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::cell::RefCell;

use azul_core::notification::{
    Notification, NotificationCallback, NotificationEvent, OptionNotificationCallback,
};
use azul_css::AzString;
use core::sync::atomic::{AtomicBool, Ordering};

// ────────── Request queue (callback → platform backend) ────────────────

/// What a callback asked the platform to do.
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationRequest {
    /// Show (or replace, if its id is live) a notification.
    Post(Notification),
    /// Take the notification with this id off the screen. Its callback is
    /// forgotten: a withdrawn notification reports nothing.
    Withdraw(AzString),
}

/// Bound on both queues. A target whose dll never drains the request queue
/// (a web build) and an app that never runs its loop must not grow either
/// without limit; 256 outstanding notifications is far past any real use.
pub const MAX_QUEUED_REQUESTS: usize = 256;
/// See [`MAX_QUEUED_REQUESTS`]; the tray's mailbox uses the same bound.
pub const MAX_QUEUED_EVENTS: usize = 256;

static PENDING_REQUESTS: std::sync::Mutex<Vec<NotificationRequest>> =
    std::sync::Mutex::new(Vec::new());

/// Queue a request from a callback. Returns `false` when the queue is full
/// and the request was dropped. Poison-recovering.
pub fn push_notification_request(request: NotificationRequest) -> bool {
    try_push_notification_request(request).is_ok()
}

/// [`push_notification_request`], handing the request BACK when the queue is
/// full - so a post that did not fit can still be reported
/// ([`reject_notification`]) instead of vanishing.
pub fn try_push_notification_request(
    request: NotificationRequest,
) -> Result<(), NotificationRequest> {
    let mut q = PENDING_REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_QUEUED_REQUESTS {
        return Err(request);
    }
    q.push(request);
    Ok(())
}

/// Take every queued request, in the order the app made them.
pub fn drain_notification_requests() -> Vec<NotificationRequest> {
    let mut q = PENDING_REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Is a request waiting - a post, a withdraw or a permission request? The
/// dll's capability pump arms its wake-up timer on this, so a request queued
/// by a callback that returned `DoNothing` does not wait for an unrelated
/// event to be dispatched.
pub fn has_queued_requests() -> bool {
    PERMISSION_REQUESTED.load(Ordering::Acquire)
        || PENDING_REQUESTS
            .lock()
            .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

// ────────── The permission request (callback → platform backend) ───────

static PERMISSION_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ask for the notification permission
/// (`CallbackInfo::request_notification_permission`). Several requests before
/// the dll dispatches them are one prompt.
pub fn request_notification_permission() {
    PERMISSION_REQUESTED.store(true, Ordering::Release);
}

/// Take the pending permission request, if any. The dll's dispatch calls this
/// on the main thread and asks the OS; the answer arrives through the
/// permission manager's async channel as `Capability::Notifications`.
pub fn take_notification_permission_request() -> bool {
    PERMISSION_REQUESTED.swap(false, Ordering::AcqRel)
}

// ────────── The app-level handler ──────────────────────────────────────

static APP_HANDLER: std::sync::Mutex<OptionNotificationCallback> =
    std::sync::Mutex::new(OptionNotificationCallback::None);

/// Install the app-level handler (`AppConfig::notification_handler`). The dll
/// calls this from `App::run`, before any event can arrive; process-wide,
/// because on Android `App::run` and the event loop are different threads.
pub fn set_app_notification_handler(handler: OptionNotificationCallback) {
    *APP_HANDLER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = handler;
}

/// The installed app-level handler, if any.
#[must_use]
pub fn app_notification_handler() -> OptionNotificationCallback {
    APP_HANDLER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

// ────────── Event mailbox (platform → run loop) ────────────────────────

static PENDING_EVENTS: std::sync::Mutex<Vec<NotificationEvent>> =
    std::sync::Mutex::new(Vec::new());

/// Post an event from a platform callback, on any thread. Never blocks for
/// long and never panics - it runs inside an Objective-C block, a D-Bus
/// filter or a window procedure, where unwinding would abort the process.
/// Returns `false` when the mailbox is full and the event was dropped.
pub fn queue_notification_event(event: NotificationEvent) -> bool {
    let mut q = PENDING_EVENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_QUEUED_EVENTS {
        return false;
    }
    q.push(event);
    true
}

/// Take every queued event, in arrival order. Called by the run loop.
pub fn drain_notification_events() -> Vec<NotificationEvent> {
    let mut q = PENDING_EVENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Is an event waiting to be routed?
pub fn has_queued_events() -> bool {
    PENDING_EVENTS
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

// ────────── Deliveries that wait (for a window, or never got a post) ───

static PENDING_DELIVERIES: std::sync::Mutex<Vec<NotificationDelivery>> =
    std::sync::Mutex::new(Vec::new());

/// Park a routed delivery until a run loop has a window to run it against,
/// or queue one that never went through routing (a rejected post). Returns
/// `false` when the queue is full and the delivery was dropped - bounded like
/// the mailbox, so a windowless app cannot grow it without limit.
pub fn queue_notification_delivery(delivery: NotificationDelivery) -> bool {
    let mut q = PENDING_DELIVERIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_QUEUED_EVENTS {
        return false;
    }
    q.push(delivery);
    true
}

/// Take every waiting delivery, oldest first.
pub fn drain_notification_deliveries() -> Vec<NotificationDelivery> {
    let mut q = PENDING_DELIVERIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Is a delivery waiting?
pub fn has_queued_deliveries() -> bool {
    PENDING_DELIVERIES
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

/// A post that never reached the backend - the request queue had no room -
/// reported as a `Failed` event instead of dropped: to the notification's own
/// callback as a waiting delivery, or, without one, through the mailbox,
/// where routing hands an id it never admitted to the app-level handler.
/// Returns `false` if even that queue was full.
pub fn reject_notification(notification: Notification, reason: AzString) -> bool {
    let mut event = NotificationEvent::failed(notification.id.clone(), reason);
    event.payload = notification.payload.clone();
    match notification.callback {
        OptionNotificationCallback::Some(callback) => {
            queue_notification_delivery(NotificationDelivery { callback, event })
        }
        OptionNotificationCallback::None => queue_notification_event(event),
    }
}

// ────────── Routing ────────────────────────────────────────────────────

/// One event, routed to the callback of the notification it names. The run
/// loop invokes `callback` with `event` installed as the current event
/// ([`with_current_notification_event`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationDelivery {
    pub callback: NotificationCallback,
    pub event: NotificationEvent,
}

/// What the registry keeps of a posted notification.
#[derive(Debug, Clone, PartialEq)]
struct LiveNotification {
    callback: OptionNotificationCallback,
    payload: AzString,
}

/// How many ended ids the registry remembers, to swallow their trailing
/// events (see [`NotificationRegistry::route`]).
const MAX_ENDED: usize = 256;

/// The live notifications, by the app's id, and where their events go.
///
/// Owned by the dll's notification service on the main thread. Pure - no
/// globals - so the routing rules are testable on their own.
#[derive(Debug, Default)]
pub struct NotificationRegistry {
    live: BTreeMap<String, LiveNotification>,
    /// Ids that ended in this process (routed or withdrawn), oldest first.
    ended: Vec<String>,
    /// `AppConfig::notification_handler`: where events no callback owns go.
    app_handler: OptionNotificationCallback,
    generated: u64,
}

impl NotificationRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            live: BTreeMap::new(),
            ended: Vec::new(),
            app_handler: OptionNotificationCallback::None,
            generated: 0,
        }
    }

    /// Where events go that no notification callback owns (see
    /// [`NotificationRegistry::route`]). `None` drops them.
    pub fn set_app_handler(&mut self, handler: OptionNotificationCallback) {
        self.app_handler = handler;
    }

    fn remember_ended(&mut self, id: String) {
        if self.ended.iter().any(|e| *e == id) {
            return;
        }
        if self.ended.len() >= MAX_ENDED {
            self.ended.remove(0);
        }
        self.ended.push(id);
    }

    /// Admit a notification that is about to be posted and return it as it
    /// will be posted: an empty id is replaced by a unique generated one (two
    /// anonymous posts must not replace each other), and the callback is
    /// remembered - replacing the one an earlier post under the same id left,
    /// because the platform replaces that notification too.
    pub fn admit(&mut self, mut notification: Notification) -> Notification {
        if notification.id.as_str().is_empty() {
            self.generated += 1;
            notification.id = AzString::from(format!("azul-notification-{}", self.generated));
        }
        let id = notification.id.as_str().to_string();
        self.ended.retain(|e| *e != id);
        self.live.insert(
            id,
            LiveNotification {
                callback: notification.callback.clone(),
                payload: notification.payload.clone(),
            },
        );
        notification
    }

    /// Forget a withdrawn notification. `true` if it was live. The id counts
    /// as ended: the close a server sends to confirm the withdraw is nobody's
    /// event.
    pub fn forget(&mut self, id: &str) -> bool {
        let was_live = self.live.remove(id).is_some();
        self.remember_ended(id.to_string());
        was_live
    }

    #[must_use]
    pub fn is_live(&self, id: &str) -> bool {
        self.live.contains_key(id)
    }

    #[must_use]
    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    /// Route events to the callbacks of the notifications they name.
    ///
    /// Every event ends its notification, so its entry is removed as it is
    /// routed and the id remembered as ended. Where an event goes:
    ///
    /// * a live notification with a callback: that callback;
    /// * a live notification without one: the app-level handler;
    /// * an id that ENDED in this process - routed or withdrawn - nowhere: that is the
    ///   freedesktop close that follows a click, or the close that confirms a withdraw;
    /// * an id this process never posted: the app-level handler - a tap on a notification an
    ///   earlier run of the app posted, whose callback died with that process.
    ///
    /// Without an app-level handler, an id never posted and a live
    /// notification without a callback end silently. An event that carries
    /// no payload gets the one the notification was posted with.
    pub fn route(&mut self, events: Vec<NotificationEvent>) -> Vec<NotificationDelivery> {
        let mut out = Vec::new();
        for mut event in events {
            let id = event.notification_id.as_str().to_string();
            let target = match self.live.remove(&id) {
                Some(live) => {
                    if event.payload.as_str().is_empty() {
                        event.payload = live.payload;
                    }
                    match live.callback {
                        OptionNotificationCallback::Some(callback) => Some(callback),
                        OptionNotificationCallback::None => self.app_handler.as_ref().cloned(),
                    }
                }
                None if self.ended.iter().any(|e| *e == id) => None,
                None => self.app_handler.as_ref().cloned(),
            };
            self.remember_ended(id);
            if let Some(callback) = target {
                out.push(NotificationDelivery { callback, event });
            }
        }
        out
    }
}

// ────────── The event a callback runs for ──────────────────────────────

std::thread_local! {
    /// Set for the duration of one delivery. Deliveries run on the event-loop
    /// thread, which is the only thread a `CallbackInfo` exists on.
    static CURRENT_EVENT: RefCell<Option<NotificationEvent>> = const { RefCell::new(None) };
}

/// Run `f` with `event` as the event `CallbackInfo::get_notification_event`
/// reports. The previous value is restored afterwards, also on unwind.
pub fn with_current_notification_event<R>(event: &NotificationEvent, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<NotificationEvent>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            let _ = CURRENT_EVENT.try_with(|slot| slot.replace(previous));
        }
    }
    let previous = CURRENT_EVENT.with(|slot| slot.replace(Some(event.clone())));
    let _restore = Restore(previous);
    f()
}

/// The event the running callback was delivered for, or `None` outside a
/// notification delivery.
#[must_use]
pub fn current_notification_event() -> Option<NotificationEvent> {
    CURRENT_EVENT
        .try_with(|slot| slot.borrow().clone())
        .ok()
        .flatten()
}

// ────────── The headless recorder ──────────────────────────────────────

/// One post the headless backend saw.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedNotification {
    /// As posted, with its callback removed: a recording must not keep the
    /// app's `RefAny` alive.
    pub notification: Notification,
    /// A later `withdraw_notification` named its id.
    pub withdrawn: bool,
}

/// Everything posted and withdrawn, in order. The headless backend's whole
/// job; also usable on its own by a test.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct NotificationRecorder {
    entries: Vec<RecordedNotification>,
}

impl NotificationRecorder {
    /// The oldest entries are dropped beyond this - a headless app posting
    /// on a timer must not grow the log without bound.
    pub const MAX_ENTRIES: usize = 1024;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn record_post(&mut self, notification: &Notification) {
        let mut notification = notification.clone();
        notification.callback = OptionNotificationCallback::None;
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(RecordedNotification {
            notification,
            withdrawn: false,
        });
    }

    /// Mark every recorded post with this id withdrawn. `true` if any was.
    pub fn record_withdraw(&mut self, id: &str) -> bool {
        let mut any = false;
        for entry in &mut self.entries {
            if entry.notification.id.as_str() == id && !entry.withdrawn {
                entry.withdrawn = true;
                any = true;
            }
        }
        any
    }

    #[must_use]
    pub fn entries(&self) -> &[RecordedNotification] {
        &self.entries
    }

    /// The most recent post under `id`.
    #[must_use]
    pub fn latest(&self, id: &str) -> Option<&RecordedNotification> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.notification.id.as_str() == id)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

static RECORDED: std::sync::Mutex<NotificationRecorder> =
    std::sync::Mutex::new(NotificationRecorder::new());

/// Record a post into the process-wide recorder (the headless backend).
pub fn record_posted_notification(notification: &Notification) {
    RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .record_post(notification);
}

/// Record a withdraw into the process-wide recorder. `true` if a recorded
/// post had that id.
pub fn record_withdrawn_notification(id: &str) -> bool {
    RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .record_withdraw(id)
}

/// A copy of the process-wide recording - what `assert_notification` reads.
#[must_use]
pub fn recorded_notifications() -> Vec<RecordedNotification> {
    RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entries()
        .to_vec()
}

pub fn clear_recorded_notifications() {
    RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

// ────────── Each platform's wire vocabulary ────────────────────────────

/// Translation from what each OS says to [`NotificationEvent`], and the few
/// encodings a backend needs. Pure functions, so they are built and tested on
/// every host - the backends that call them each compile on one OS only.
pub mod wire {
    use alloc::{string::String, vec::Vec};

    use azul_core::notification::{
        Notification, NotificationAction, NotificationEvent, NotificationSound,
    };
    use azul_css::AzString;

    use crate::managers::permission::{PermissionQuality, PermissionState};

    /// FNV-1a, 64 bit: stable across runs and platforms, unlike the std
    /// hasher - a name derived from it by one launch is recognised by the next.
    fn fnv1a64(parts: &[&[u8]]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for part in parts {
            for b in *part {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    /// Percent-encode everything but the RFC 3986 unreserved characters.
    fn percent_encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(char::from(b));
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    /// The inverse of [`percent_encode`]; `None` for a malformed escape or
    /// bytes that are not UTF-8.
    fn percent_decode(s: &str) -> Option<String> {
        let bytes = s.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).ok()
    }

    /// Escape text for an XML attribute or element body.
    fn xml_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                // XML 1.0 has no escape for these; drop them rather than
                // make `LoadXml` reject the whole toast.
                c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
                c => out.push(c),
            }
        }
        out
    }

    // ---- freedesktop (org.freedesktop.Notifications) ----

    /// The action key a freedesktop server reports for a click on the
    /// notification's body. It is only reported if it is in the `actions`
    /// list, which is why [`freedesktop_actions`] always puts it first.
    pub const FREEDESKTOP_DEFAULT_ACTION: &str = "default";

    /// The `actions` argument of `Notify`: a flat `[key, label, key, label,
    /// ...]` list. The body-click key comes first; an app action named
    /// `default` or with an empty id is skipped, because the event it would
    /// report could not be told apart from a body click.
    #[must_use]
    pub fn freedesktop_actions(notification: &Notification) -> Vec<String> {
        let mut out = alloc::vec![
            String::from(FREEDESKTOP_DEFAULT_ACTION),
            String::from("Open"),
        ];
        for action in notification.actions.as_ref() {
            let id = action.id.as_str();
            if id.is_empty() || id == FREEDESKTOP_DEFAULT_ACTION {
                continue;
            }
            out.push(id.to_string());
            out.push(action.label.as_str().to_string());
        }
        out
    }

    /// `ActionInvoked(id, action_key)`, with the server's id already mapped
    /// back to the app's.
    #[must_use]
    pub fn freedesktop_action_event(app_id: &str, action_key: &str) -> NotificationEvent {
        if action_key == FREEDESKTOP_DEFAULT_ACTION {
            NotificationEvent::activated(AzString::from(app_id))
        } else {
            NotificationEvent::action_invoked(AzString::from(app_id), AzString::from(action_key))
        }
    }

    /// `NotificationClosed(id, reason)`. The spec's reasons: 1 expired, 2
    /// dismissed by the user, 3 closed by `CloseNotification`, 4 undefined.
    #[must_use]
    pub fn freedesktop_closed_event(app_id: &str, reason: u32) -> NotificationEvent {
        let why = match reason {
            1 => String::from("expired"),
            2 => String::from("dismissed by the user"),
            3 => String::from("closed by CloseNotification"),
            4 => String::from("closed (the server gave no reason)"),
            other => format!("closed (unknown reason {other})"),
        };
        NotificationEvent::dismissed_because(AzString::from(app_id), AzString::from(why))
    }

    /// The `app_icon` argument: an absolute path becomes a `file://` URI
    /// (percent-encoded), anything else - an icon-theme name, a URI - is
    /// passed through.
    #[must_use]
    pub fn freedesktop_icon(icon: &str) -> String {
        if !icon.starts_with('/') {
            return icon.to_string();
        }
        let mut out = String::from("file://");
        for b in icon.bytes() {
            let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~');
            if unreserved {
                out.push(char::from(b));
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    // ---- Apple (UNUserNotificationCenter) ----

    /// Value of `UNNotificationDefaultActionIdentifier` (a click on the
    /// notification). The backend reads the framework's exported constant
    /// and falls back to this.
    pub const APPLE_DEFAULT_ACTION: &str = "com.apple.UNNotificationDefaultActionIdentifier";
    /// Value of `UNNotificationDismissActionIdentifier`. Only reported for a
    /// category registered with `UNNotificationCategoryOptionCustomDismissAction`.
    pub const APPLE_DISMISS_ACTION: &str = "com.apple.UNNotificationDismissActionIdentifier";

    /// A `UNNotificationResponse`'s `actionIdentifier`, for the notification
    /// whose request identifier is `app_id`.
    #[must_use]
    pub fn apple_response_event(
        app_id: &str,
        action_identifier: &str,
        default_id: &str,
        dismiss_id: &str,
    ) -> NotificationEvent {
        if action_identifier == default_id {
            NotificationEvent::activated(AzString::from(app_id))
        } else if action_identifier == dismiss_id {
            NotificationEvent::dismissed_because(
                AzString::from(app_id),
                AzString::from_const_str("cleared by the user"),
            )
        } else {
            NotificationEvent::action_invoked(
                AzString::from(app_id),
                AzString::from(action_identifier),
            )
        }
    }

    /// The `UNNotificationCategory` identifier for a set of buttons.
    ///
    /// UN attaches buttons through a CATEGORY registered ahead of time, so
    /// each distinct button set needs its own category - named by a hash of
    /// its ids AND labels: a category that only hashed the ids would keep
    /// showing a relabelled button under its old text.
    #[must_use]
    pub fn apple_category_id(actions: &[NotificationAction]) -> String {
        if actions.is_empty() {
            return String::from("azul.notification.plain");
        }
        // Stable across runs, so a category registered by one launch is
        // recognised by the next.
        let mut parts: Vec<&[u8]> = Vec::with_capacity(actions.len() * 4);
        for action in actions {
            parts.push(action.id.as_str().as_bytes());
            parts.push(&[0x1f]);
            parts.push(action.label.as_str().as_bytes());
            parts.push(&[0x1e]);
        }
        let hash = fnv1a64(&parts);
        format!("azul.notification.actions.{hash:016x}")
    }

    /// The `userInfo` key a notification's payload travels under
    /// (`UNNotificationContent.userInfo`), so a response delivered to a
    /// freshly launched process still carries it.
    pub const APPLE_PAYLOAD_KEY: &str = "azul.payload";

    /// `UNNotificationSettings.authorizationStatus` as a permission state:
    /// notDetermined 0, denied 1, authorized 2, provisional 3 (delivered
    /// quietly to Notification Center: a REDUCED grant), ephemeral 4 (App
    /// Clips). Anything newer reads as not determined.
    #[must_use]
    pub const fn apple_authorization_status(status: i64) -> PermissionState {
        match status {
            1 => PermissionState::Denied,
            2 => PermissionState::Granted(PermissionQuality::Full),
            3 => PermissionState::Granted(PermissionQuality::Reduced),
            4 => PermissionState::EphemeralGranted(true),
            _ => PermissionState::NotDetermined,
        }
    }

    // ---- Windows (Shell_NotifyIconW balloon, NOTIFYICON_VERSION_4) ----

    /// `LOWORD(lParam)` of the notify icon's callback message.
    pub const NIN_BALLOONSHOW: u32 = 0x0402;
    pub const NIN_BALLOONHIDE: u32 = 0x0403;
    pub const NIN_BALLOONTIMEOUT: u32 = 0x0404;
    pub const NIN_BALLOONUSERCLICK: u32 = 0x0405;

    /// The event a balloon message means for the notification it shows, or
    /// `None` for a message that ends nothing (the balloon appearing, mouse
    /// traffic over the icon).
    #[must_use]
    pub fn balloon_event(app_id: &str, code: u32) -> Option<NotificationEvent> {
        match code {
            NIN_BALLOONUSERCLICK => Some(NotificationEvent::activated(AzString::from(app_id))),
            NIN_BALLOONTIMEOUT => Some(NotificationEvent::dismissed_because(
                AzString::from(app_id),
                AzString::from_const_str("timed out or closed by the user"),
            )),
            NIN_BALLOONHIDE => Some(NotificationEvent::dismissed_because(
                AzString::from(app_id),
                AzString::from_const_str("hidden by the system"),
            )),
            _ => None,
        }
    }

    /// `s` as NUL-terminated UTF-16 for a fixed `[u16; capacity]` field of
    /// `NOTIFYICONDATAW` (`szInfoTitle` is 64, `szInfo` 256, `szTip` 128):
    /// at most `capacity - 1` code units plus the NUL, never ending on half
    /// of a surrogate pair.
    #[must_use]
    pub fn utf16_truncated(s: &str, capacity: usize) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::with_capacity(capacity.min(s.len() + 1));
        if capacity == 0 {
            return out;
        }
        let room = capacity - 1;
        let mut buf = [0u16; 2];
        for c in s.chars() {
            let units = c.encode_utf16(&mut buf);
            if out.len() + units.len() > room {
                break;
            }
            out.extend_from_slice(units);
        }
        out.push(0);
        out
    }

    // ---- Windows (WinRT toast, ToastGeneric) ----

    /// What every toast argument string azul writes starts with; anything
    /// else in `ToastActivatedEventArgs.Arguments` is not ours.
    pub const TOAST_ARGS_PREFIX: &str = "azul-notification:";
    /// A toast shows at most five buttons.
    pub const TOAST_MAX_ACTIONS: usize = 5;
    /// `ToastDismissalReason`: the user closed it.
    pub const TOAST_DISMISSED_USER_CANCELED: i32 = 0;
    /// `ToastDismissalReason`: the app hid it (`ToastNotifier::Hide`).
    pub const TOAST_DISMISSED_APPLICATION_HIDDEN: i32 = 1;
    /// `ToastDismissalReason`: the banner timed out - into the Action Center.
    pub const TOAST_DISMISSED_TIMED_OUT: i32 = 2;

    /// The `launch` / `arguments` string of a toast or one of its buttons:
    /// which notification, which action (`"default"` = the body) and the
    /// payload, percent-encoded so none of them can break the others apart.
    #[must_use]
    pub fn toast_arguments(id: &str, action: &str, payload: &str) -> String {
        format!(
            "{TOAST_ARGS_PREFIX}id={}&action={}&payload={}",
            percent_encode(id),
            percent_encode(action),
            percent_encode(payload)
        )
    }

    /// `(id, action, payload)` back out of [`toast_arguments`]; `None` for a
    /// string azul did not write.
    #[must_use]
    pub fn parse_toast_arguments(args: &str) -> Option<(String, String, String)> {
        let rest = args.strip_prefix(TOAST_ARGS_PREFIX)?;
        let mut id: Option<String> = None;
        let mut action: Option<String> = None;
        let mut payload: Option<String> = None;
        for pair in rest.split('&') {
            let (key, value) = pair.split_once('=')?;
            let value = percent_decode(value)?;
            match key {
                "id" => id = Some(value),
                "action" => action = Some(value),
                "payload" => payload = Some(value),
                _ => {}
            }
        }
        Some((id?, action?, payload.unwrap_or_default()))
    }

    /// `ToastNotification.Activated`: the body (action `default`) or a button.
    #[must_use]
    pub fn toast_activated_event(args: &str) -> Option<NotificationEvent> {
        let (id, action, payload) = parse_toast_arguments(args)?;
        let mut event = if action == FREEDESKTOP_DEFAULT_ACTION || action.is_empty() {
            NotificationEvent::activated(AzString::from(id))
        } else {
            NotificationEvent::action_invoked(AzString::from(id), AzString::from(action))
        };
        event.payload = AzString::from(payload);
        Some(event)
    }

    /// `ToastNotification.Dismissed`. Only a user's close ends the
    /// notification: a timed-out toast moved to the Action Center, where it
    /// can still be clicked, and an app-hidden one was withdrawn (the
    /// registry already forgot it).
    #[must_use]
    pub fn toast_dismissed_event(app_id: &str, reason: i32) -> Option<NotificationEvent> {
        (reason == TOAST_DISMISSED_USER_CANCELED).then(|| {
            NotificationEvent::dismissed_because(
                AzString::from(app_id),
                AzString::from_const_str("dismissed by the user"),
            )
        })
    }

    /// An image path as a toast `src`: `file:///C:/...`, or passed through
    /// when it already is a URI.
    fn toast_image_src(path: &str) -> String {
        if path.contains("://") {
            return path.to_string();
        }
        let slashed = path.replace('\\', "/");
        if slashed.starts_with('/') {
            format!("file://{slashed}")
        } else {
            format!("file:///{slashed}")
        }
    }

    /// The toast's XML (`ToastGeneric`): title, body, an image, up to
    /// [`TOAST_MAX_ACTIONS`] foreground buttons, the sound, and the id +
    /// payload in every argument string so a click reports them back.
    #[must_use]
    pub fn toast_xml(notification: &Notification) -> String {
        let id = notification.id.as_str();
        let payload = notification.payload.as_str();
        let mut xml = format!(
            "<toast launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text>",
            xml_escape(&toast_arguments(id, FREEDESKTOP_DEFAULT_ACTION, payload)),
            xml_escape(notification.title.as_str())
        );
        if !notification.body.as_str().is_empty() {
            xml.push_str(&format!(
                "<text>{}</text>",
                xml_escape(notification.body.as_str())
            ));
        }
        if let Some(icon) = notification.icon.as_ref() {
            xml.push_str(&format!(
                "<image placement=\"appLogoOverride\" src=\"{}\"/>",
                xml_escape(&toast_image_src(icon.as_str()))
            ));
        }
        xml.push_str("</binding></visual>");
        let buttons: Vec<&NotificationAction> = notification
            .actions
            .as_ref()
            .iter()
            .filter(|a| {
                !a.id.as_str().is_empty() && a.id.as_str() != FREEDESKTOP_DEFAULT_ACTION
            })
            .take(TOAST_MAX_ACTIONS)
            .collect();
        if !buttons.is_empty() {
            xml.push_str("<actions>");
            for action in buttons {
                xml.push_str(&format!(
                    "<action content=\"{}\" arguments=\"{}\" activationType=\"foreground\"/>",
                    xml_escape(action.label.as_str()),
                    xml_escape(&toast_arguments(id, action.id.as_str(), payload))
                ));
            }
            xml.push_str("</actions>");
        }
        match &notification.sound {
            NotificationSound::Silent => xml.push_str("<audio silent=\"true\"/>"),
            NotificationSound::Named(name) if name.as_str().starts_with("ms-winsoundevent:") => {
                xml.push_str(&format!("<audio src=\"{}\"/>", xml_escape(name.as_str())));
            }
            NotificationSound::Default | NotificationSound::Named(_) => {}
        }
        xml.push_str("</toast>");
        xml
    }

    /// An AppUserModelID the registry and the shell accept: letters, digits,
    /// `.`, `-` and `_` only (a backslash breaks Windows 10 up to build
    /// 19042), never empty, and at most 129 characters - a longer one is cut
    /// and suffixed with a hash of the whole, so two long ids stay distinct.
    #[must_use]
    pub fn windows_aumid(app: &str) -> String {
        const MAX: usize = 129;
        let mut out: String = app
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if out.is_empty() {
            out.push_str("azul.app");
        }
        if out.len() > MAX {
            let hash = fnv1a64(&[app.as_bytes()]);
            let suffix = format!(".{hash:016x}");
            out.truncate(MAX - suffix.len());
            out.push_str(&suffix);
        }
        out
    }

    /// Where an unpackaged app registers its AUMID (under HKCU): the key
    /// whose `DisplayName` the toast is attributed to.
    #[must_use]
    pub fn aumid_registry_key(aumid: &str) -> String {
        format!("Software\\Classes\\AppUserModelId\\{aumid}")
    }

    // ---- freedesktop: the app's identity ----

    /// The `desktop-entry` hint: the `.desktop` file's name without the
    /// extension. The executable's file name - the same default the Wayland
    /// `app_id` and the X11 `WM_CLASS` use - so a server that matches the one
    /// matches the other.
    #[must_use]
    pub fn desktop_entry(exe_path: &str) -> String {
        let name = exe_path.rsplit(|c: char| c == '/' || c == '\\').next().unwrap_or("");
        let name = name.strip_suffix(".desktop").unwrap_or(name);
        if name.is_empty() {
            String::from("azul")
        } else {
            name.to_string()
        }
    }

    // ---- Android (NotificationManager, PendingIntent extras) ----

    /// The intent action key of a tap on the notification's body.
    pub const ANDROID_DEFAULT_ACTION: &str = "default";
    /// The intent action key of the delete intent (a swipe, "Clear all").
    pub const ANDROID_DISMISS_ACTION: &str = "azul.dismiss";
    /// Android shows at most three action buttons.
    pub const ANDROID_MAX_ACTIONS: usize = 3;

    /// The base request code of a notification's `PendingIntent`s.
    ///
    /// Intents that differ only in their extras are the SAME `PendingIntent`,
    /// and `FLAG_UPDATE_CURRENT` would overwrite one notification's extras
    /// with the next's - so every notification gets its own code, stable for
    /// its id. The low four bits are left zero: the Java side adds 0 for the
    /// body, 1..=13 for the buttons and 15 for the delete intent.
    #[must_use]
    pub fn android_request_code(id: &str) -> i32 {
        let hash = fnv1a64(&[b"azul.notification.".as_slice(), id.as_bytes()]);
        // Fold to 32 bits, keep it non-negative, clear the slot nibble.
        let folded = ((hash >> 32) ^ (hash & 0xFFFF_FFFF)) as u32;
        (folded & 0x7FFF_FFF0) as i32
    }

    /// `PendingIntent` flags: `FLAG_UPDATE_CURRENT`, plus `FLAG_IMMUTABLE`
    /// wherever it exists (API 23; required when targeting 31+).
    #[must_use]
    pub const fn android_pending_intent_flags(sdk: i32) -> i32 {
        const FLAG_IMMUTABLE: i32 = 0x0400_0000;
        const FLAG_UPDATE_CURRENT: i32 = 0x0800_0000;
        if sdk >= 23 {
            FLAG_UPDATE_CURRENT | FLAG_IMMUTABLE
        } else {
            FLAG_UPDATE_CURRENT
        }
    }

    /// The event an intent back from a notification means: the body
    /// ([`ANDROID_DEFAULT_ACTION`]), the delete intent
    /// ([`ANDROID_DISMISS_ACTION`]) or a button.
    #[must_use]
    pub fn android_event(
        id: &str,
        action: &str,
        payload: &str,
        launched_app: bool,
    ) -> NotificationEvent {
        let mut event = if action == ANDROID_DEFAULT_ACTION || action.is_empty() {
            NotificationEvent::activated(AzString::from(id))
        } else if action == ANDROID_DISMISS_ACTION {
            NotificationEvent::dismissed_because(
                AzString::from(id),
                AzString::from_const_str("dismissed by the user"),
            )
        } else {
            NotificationEvent::action_invoked(AzString::from(id), AzString::from(action))
        };
        event.payload = AzString::from(payload);
        event.launched_app = launched_app;
        event
    }
}
