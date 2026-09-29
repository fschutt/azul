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
///
/// POSTS stop at this bound (a post that does not fit is reported as
/// `Failed`, [`reject_notification`]). A WITHDRAW has no event to report a
/// refusal through, and a lost one leaves a notification on screen whose
/// callback still fires - so withdraws may use as many slots again, room
/// posts can never take.
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
    let bound = match &request {
        NotificationRequest::Post(_) => MAX_QUEUED_REQUESTS,
        // See `MAX_QUEUED_REQUESTS`: the headroom posts cannot take.
        NotificationRequest::Withdraw(_) => 2 * MAX_QUEUED_REQUESTS,
    };
    let mut q = PENDING_REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= bound {
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
/// callback, or, without one, to the app-level handler, as a waiting
/// delivery.
///
/// Never through the mailbox: routing would take the id for whatever an
/// EARLIER post under it left - hand this failure to the notification still
/// on screen (and end it, although the post that failed replaced nothing) or
/// swallow it as the trailing close of one that ended. A callback-less post
/// in an app without a handler ends silently, as routing ends its events.
/// Returns `false` if the queue of waiting deliveries was full too.
pub fn reject_notification(notification: Notification, reason: AzString) -> bool {
    let mut event = NotificationEvent::failed(notification.id.clone(), reason);
    event.payload = notification.payload.clone();
    let callback = match notification.callback {
        OptionNotificationCallback::Some(callback) => callback,
        OptionNotificationCallback::None => match app_notification_handler() {
            OptionNotificationCallback::Some(handler) => handler,
            OptionNotificationCallback::None => return true,
        },
    };
    queue_notification_delivery(NotificationDelivery { callback, event })
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
    use alloc::{collections::BTreeMap, string::String, vec::Vec};

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

    /// What the freedesktop backend does after a reply or a signal.
    #[derive(Debug, Default, Clone, PartialEq)]
    pub struct FreedesktopActions {
        /// Server ids to close (`CloseNotification`) now.
        pub close: Vec<u32>,
        /// Events to queue.
        pub events: Vec<NotificationEvent>,
    }

    /// A `Notify` whose reply has not arrived.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PendingNotify {
        app_id: String,
        /// The `replaces_id` it was sent with.
        replaces: u32,
        sent_at_ms: u64,
    }

    /// The freedesktop backend's bookkeeping between a `Notify` and its reply.
    ///
    /// `Notify` is ASYNCHRONOUS: it is sent, and its reply - the SERVER's id
    /// for the notification - is read on a later pump, so the loop never
    /// waits for a slow server. Meanwhile the app may post again under the
    /// same id or withdraw it. The rules, pure so every host tests them:
    ///
    /// * a reply maps the server's id to the app's - if its post is still the newest under that
    ///   id. A reply that arrives after the app posted again is stale and its notification is
    ///   closed, unless it is the one a newer post replaces in place; a reply after a withdraw is
    ///   closed.
    /// * A server id the app's id had before, that the newest post did not replace in place (a
    ///   repost before the first reply, a server that ignores `replaces_id`), is closed.
    /// * An error reply, or none within the timeout ([`FreedesktopPosts::expired`]), is a
    ///   `Failed` event - for the newest post only; an older one's failure is nobody's news.
    /// * A signal names a server id ([`FreedesktopPosts::app_id_of`]); `NotificationClosed`
    ///   forgets it.
    /// * The server leaving the bus took its notifications with it
    ///   ([`FreedesktopPosts::server_gone`]).
    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct FreedesktopPosts {
        /// Server id -> the app's id, for what is on screen.
        shown: BTreeMap<u32, String>,
        /// Sent, reply outstanding, by token.
        pending: BTreeMap<u64, PendingNotify>,
        /// The app's id -> the token of its newest post.
        newest: BTreeMap<String, u64>,
        next_token: u64,
    }

    impl FreedesktopPosts {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                shown: BTreeMap::new(),
                pending: BTreeMap::new(),
                newest: BTreeMap::new(),
                next_token: 0,
            }
        }

        /// A `Notify` for `app_id` is about to be sent at `now_ms`:
        /// `(token, replaces_id)` - the token its reply is reported under
        /// ([`FreedesktopPosts::replied`]), and the server id it replaces (0:
        /// none on screen).
        pub fn post(&mut self, app_id: &str, now_ms: u64) -> (u64, u32) {
            self.next_token += 1;
            let token = self.next_token;
            let replaces = self.replaces_id(app_id);
            self.pending.insert(
                token,
                PendingNotify {
                    app_id: app_id.to_string(),
                    replaces,
                    sent_at_ms: now_ms,
                },
            );
            self.newest.insert(app_id.to_string(), token);
            (token, replaces)
        }

        /// The reply of the post `token`: the server's id, or why there is
        /// none (an error reply, no reply within the timeout).
        pub fn replied(&mut self, token: u64, result: Result<u32, String>) -> FreedesktopActions {
            let mut actions = FreedesktopActions::default();
            let Some(post) = self.pending.remove(&token) else {
                return actions;
            };
            let is_newest = self.newest.get(&post.app_id) == Some(&token);
            if is_newest {
                self.newest.remove(&post.app_id);
            }
            let result = match result {
                Ok(0) => Err(String::from(
                    "the notification server answered Notify without an id",
                )),
                other => other,
            };
            match result {
                Ok(server_id) if is_newest => {
                    // Whatever this post did not replace in place is stale.
                    let stale: Vec<u32> = self
                        .shown
                        .iter()
                        .filter(|(id, app)| **id != server_id && **app == post.app_id)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in stale {
                        self.shown.remove(&id);
                        actions.close.push(id);
                    }
                    self.shown.insert(server_id, post.app_id);
                }
                Ok(server_id) => {
                    // Stale (the app posted again) or withdrawn. Keep it only
                    // if it is what is on screen for this id, or what a newer
                    // post of it replaces in place.
                    let on_screen = self.shown.get(&server_id) == Some(&post.app_id);
                    let replaced_later = self
                        .pending
                        .values()
                        .any(|p| p.app_id == post.app_id && p.replaces == server_id);
                    if !on_screen && !replaced_later {
                        actions.close.push(server_id);
                    }
                }
                Err(why) if is_newest => {
                    actions.events.push(NotificationEvent::failed(
                        AzString::from(post.app_id),
                        AzString::from(why),
                    ));
                }
                Err(_) => {}
            }
            actions
        }

        /// The tokens of the posts sent `timeout_ms` or longer before
        /// `now_ms` whose reply has not arrived. The caller gives up on each
        /// and reports it through [`FreedesktopPosts::replied`] as an error.
        #[must_use]
        pub fn expired(&self, now_ms: u64, timeout_ms: u64) -> Vec<u64> {
            self.pending
                .iter()
                .filter(|(_, p)| now_ms.saturating_sub(p.sent_at_ms) >= timeout_ms)
                .map(|(token, _)| *token)
                .collect()
        }

        /// `withdraw_notification`: the server ids to close now. A post still
        /// waiting for its reply is closed when the reply comes.
        pub fn withdraw(&mut self, app_id: &str) -> Vec<u32> {
            self.newest.remove(app_id);
            let ids: Vec<u32> = self
                .shown
                .iter()
                .filter(|(_, app)| app.as_str() == app_id)
                .map(|(id, _)| *id)
                .collect();
            for id in &ids {
                self.shown.remove(id);
            }
            ids
        }

        /// The app's id of a server id on screen - what a signal names.
        #[must_use]
        pub fn app_id_of(&self, server_id: u32) -> Option<String> {
            self.shown.get(&server_id).cloned()
        }

        /// The server id showing the app's `app_id` (0: none).
        #[must_use]
        pub fn replaces_id(&self, app_id: &str) -> u32 {
            self.shown
                .iter()
                .find(|(_, app)| app.as_str() == app_id)
                .map_or(0, |(id, _)| *id)
        }

        /// `NotificationClosed`: forget the server id - the server will not
        /// report it again.
        pub fn closed(&mut self, server_id: u32) -> Option<String> {
            self.shown.remove(&server_id)
        }

        /// The notification server left the bus (it quit, or restarted:
        /// [`freedesktop_server_left`]). Every notification it showed went
        /// with it, so each ends as `Dismissed`. A post still waiting gets
        /// its error reply from the bus.
        pub fn server_gone(&mut self) -> FreedesktopActions {
            let shown = core::mem::take(&mut self.shown);
            FreedesktopActions {
                close: Vec::new(),
                events: shown
                    .into_values()
                    .map(|app_id| {
                        NotificationEvent::dismissed_because(
                            AzString::from(app_id),
                            AzString::from_const_str(
                                "the notification server went away (it quit or restarted)",
                            ),
                        )
                    })
                    .collect(),
            }
        }
    }

    /// The name the freedesktop notification server owns on the session bus.
    pub const FREEDESKTOP_SERVER_NAME: &str = "org.freedesktop.Notifications";

    /// `org.freedesktop.DBus.NameOwnerChanged(name, old_owner, new_owner)`:
    /// did the notification server leave the bus? A restart is a change
    /// from one owner to another - the old one's notifications are gone
    /// either way. A server APPEARING (no old owner) ends nothing.
    #[must_use]
    pub fn freedesktop_server_left(name: &str, old_owner: &str, new_owner: &str) -> bool {
        let _ = new_owner;
        name == FREEDESKTOP_SERVER_NAME && !old_owner.is_empty()
    }

    // ---- the Flatpak portal (org.freedesktop.portal.Notification) ----

    /// `AddNotification`'s `notification` dictionary, as plain data.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PortalNotification {
        pub title: String,
        pub body: String,
        /// `default-action`.
        pub default_action: String,
        /// `buttons`: `(label, action)`.
        pub buttons: Vec<(String, String)>,
    }

    /// What a button's action is renamed to when the app's id for it starts
    /// with `app.`: the portal ACTIVATES such an action on the app's own D-Bus
    /// name (`org.freedesktop.Application.ActivateAction`) instead of
    /// reporting it as `ActionInvoked`, and an azul app exports no actions.
    const PORTAL_APP_ACTION_PREFIX: &str = "azul.";

    /// A notification as the portal's `AddNotification` takes it. The portal
    /// keys notifications by the app's own id - no server id to map - and
    /// attributes them to the sandbox's app id itself. The body click is
    /// the `default` action, as on the bus; an empty or reserved button id
    /// is skipped as there ([`freedesktop_actions`]).
    #[must_use]
    pub fn portal_notification(notification: &Notification) -> PortalNotification {
        let buttons = notification
            .actions
            .as_ref()
            .iter()
            .filter(|a| {
                !a.id.as_str().is_empty() && a.id.as_str() != FREEDESKTOP_DEFAULT_ACTION
            })
            .map(|a| {
                let id = a.id.as_str();
                let action = if id.starts_with("app.") {
                    format!("{PORTAL_APP_ACTION_PREFIX}{id}")
                } else {
                    id.to_string()
                };
                (a.label.as_str().to_string(), action)
            })
            .collect();
        PortalNotification {
            title: notification.title.as_str().to_string(),
            body: notification.body.as_str().to_string(),
            default_action: String::from(FREEDESKTOP_DEFAULT_ACTION),
            buttons,
        }
    }

    /// The portal's `ActionInvoked(id, action, parameter)`: the body click
    /// (`default`) or a button - under the app's own action id again.
    #[must_use]
    pub fn portal_action_event(app_id: &str, action: &str) -> NotificationEvent {
        let action = match action.strip_prefix(PORTAL_APP_ACTION_PREFIX) {
            Some(rest) if rest.starts_with("app.") => rest,
            _ => action,
        };
        freedesktop_action_event(app_id, action)
    }

    /// Does this process run in a Flatpak sandbox (`/.flatpak-info` exists,
    /// or `FLATPAK_ID` is set)? Then its session bus is filtered down to the
    /// portals, and notifications go through
    /// `org.freedesktop.portal.Notification`. Unsandboxed, the portal is NOT
    /// used: GNOME's backend drops the notifications of an app id with no
    /// `.desktop` file.
    #[must_use]
    pub fn in_flatpak_sandbox(flatpak_info_exists: bool, flatpak_id: Option<&str>) -> bool {
        flatpak_info_exists || flatpak_id.is_some_and(|id| !id.trim().is_empty())
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

    // ---- which response LAUNCHED the app (macOS, iOS, Windows) ----

    /// Which notification response started this process
    /// (`NotificationEvent::launched_app`): the one tap whose notification's
    /// callback cannot exist here, because the process did not.
    ///
    /// * **macOS** names it: `applicationDidFinishLaunching:`'s userInfo holds the response under
    ///   `NSApplicationLaunchUserNotificationKey` ([`LaunchResponseMarker::name`]). Only that
    ///   response is marked, whenever it arrives.
    /// * **iOS** names nothing for a LOCAL notification. A tap that cold-launches the app delivers
    ///   `didReceiveNotificationResponse` while the app launches, before its first
    ///   `applicationDidBecomeActive`; a tap on an app that runs (in front or in the background)
    ///   arrives after it was active once. So the first response between the launch
    ///   ([`LaunchResponseMarker::expect_first`], from `didFinishLaunching`) and the first
    ///   activation ([`LaunchResponseMarker::launch_finished`]) is the launch response.
    /// * **Windows** starts the process with `-ToastActivated` when a click on a toast of an app
    ///   that is not running reaches its COM activator ([`launched_by_toast_activation`]); the
    ///   first activation that process receives is that click ([`LaunchResponseMarker::expect_first`]).
    ///
    /// A launch marks at most one response.
    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct LaunchResponseMarker {
        /// macOS: the request identifier the launch named.
        named: Option<String>,
        /// iOS / Windows: the next response is the launch response.
        expecting_first: bool,
    }

    impl LaunchResponseMarker {
        /// Nothing launched this process for a notification (yet).
        #[must_use]
        pub const fn new() -> Self {
            Self {
                named: None,
                expecting_first: false,
            }
        }

        /// macOS: the launch named the response by its request identifier.
        pub fn name(&mut self, request_id: String) {
            self.named = Some(request_id);
        }

        /// The process was started for a response that has not arrived yet.
        pub fn expect_first(&mut self) {
            self.expecting_first = true;
        }

        /// iOS: the app became active. A response from now on is a tap on an
        /// app that was running - unless the launch NAMED it (macOS), which
        /// stays marked however late it arrives.
        pub fn launch_finished(&mut self) {
            self.expecting_first = false;
        }

        /// Judge one response, by the id of the notification it answers: did
        /// it launch the app?
        pub fn launched_app(&mut self, notification_id: &str) -> bool {
            if let Some(named) = self.named.as_deref() {
                // A named launch response is the only one that can be it.
                if named != notification_id {
                    return false;
                }
                self.named = None;
                self.expecting_first = false;
                return true;
            }
            core::mem::replace(&mut self.expecting_first, false)
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

    // ---- Windows: the toast activator (a click after the app exited) ----

    /// The switch `LocalServer32` starts the app with when a click on one of
    /// its toasts reaches its COM activator while it is not running. (COM
    /// appends `-Embedding` after it.)
    pub const TOAST_ACTIVATED_SWITCH: &str = "-ToastActivated";

    /// Was this process started by COM for a click on one of its toasts?
    /// (The command line `LocalServer32` names, see [`toast_registry_values`].)
    #[must_use]
    pub fn launched_by_toast_activation<S: AsRef<str>>(args: &[S]) -> bool {
        args.iter()
            .any(|a| a.as_ref().eq_ignore_ascii_case(TOAST_ACTIVATED_SWITCH))
    }

    /// The CLSID of the app's toast activator, as a `u128`
    /// (`GUID::from_u128`): derived from the AUMID, so every launch of the
    /// same app registers - and COM relaunches it for - the same class. A
    /// name-based UUID of version 8 ("custom", RFC 9562) over two FNV-1a
    /// hashes of the AUMID.
    #[must_use]
    pub fn toast_activator_clsid(aumid: &str) -> u128 {
        let high = fnv1a64(&[b"azul.toast-activator.".as_slice(), aumid.as_bytes()]);
        let low = fnv1a64(&[b"azul.toast-activator.low.".as_slice(), aumid.as_bytes()]);
        let mut clsid = (u128::from(high) << 64) | u128::from(low);
        // The version: the high nibble of the third group.
        clsid = (clsid & !(0xF_u128 << 76)) | (0x8_u128 << 76);
        // The variant: the two top bits of the fourth group are `10`.
        clsid = (clsid & !(0x3_u128 << 62)) | (0x2_u128 << 62);
        clsid
    }

    /// A GUID the way the registry writes one:
    /// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, upper case.
    #[must_use]
    pub fn guid_string(guid: u128) -> String {
        let hex = format!("{guid:032X}");
        format!(
            "{{{}-{}-{}-{}-{}}}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }

    /// One `REG_SZ` value an unpackaged app writes under `HKEY_CURRENT_USER`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RegistryValue {
        /// The key, relative to `HKEY_CURRENT_USER`.
        pub key: String,
        /// The value's name; empty = the key's default value.
        pub name: String,
        pub data: String,
    }

    /// Everything an unpackaged app registers for its toasts, in the order it
    /// is written:
    ///
    /// * the AUMID's `DisplayName` - the toast is attributed to it, and no toast shows without
    ///   the key (first: the one value that is not optional);
    /// * the AUMID's `CustomActivator` - the CLSID of its toast activator
    ///   ([`toast_activator_clsid`]), which the shell asks for a click the app's own process can
    ///   no longer receive;
    /// * that CLSID's `LocalServer32` - the command COM runs when the app is not running: the
    ///   quoted executable and [`TOAST_ACTIVATED_SWITCH`].
    #[must_use]
    pub fn toast_registry_values(
        aumid: &str,
        display_name: &str,
        exe_path: &str,
    ) -> Vec<RegistryValue> {
        let clsid = guid_string(toast_activator_clsid(aumid));
        let aumid_key = aumid_registry_key(aumid);
        alloc::vec![
            RegistryValue {
                key: aumid_key.clone(),
                name: String::from("DisplayName"),
                data: display_name.to_string(),
            },
            RegistryValue {
                key: aumid_key,
                name: String::from("CustomActivator"),
                data: clsid.clone(),
            },
            RegistryValue {
                key: format!("Software\\Classes\\CLSID\\{clsid}\\LocalServer32"),
                name: String::new(),
                data: format!("\"{exe_path}\" {TOAST_ACTIVATED_SWITCH}"),
            },
        ]
    }

    /// `INotificationActivationCallback::Activate(aumid, invokedArgs, ..)`:
    /// the click, translated as the in-process `Activated` event is
    /// ([`toast_activated_event`]) - if it is on one of THIS app's toasts
    /// (AUMIDs compare without case). `launched_app` is left to the caller's
    /// [`LaunchResponseMarker`].
    #[must_use]
    pub fn toast_activator_event(
        our_aumid: &str,
        aumid: &str,
        invoked_args: &str,
    ) -> Option<NotificationEvent> {
        if !aumid.eq_ignore_ascii_case(our_aumid) {
            return None;
        }
        toast_activated_event(invoked_args)
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

    // ---- the app's identity, once, for every platform ----

    /// Where an [`AppIdentity`]'s id came from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AppIdSource {
        /// Nobody named the app: the id is derived from the executable's name.
        Executable,
        /// Someone named it: the platform (a bundle's `CFBundleIdentifier`, `FLATPAK_ID`, the
        /// Android package) or the app itself (`AppConfig::app_id`) - see
        /// [`AppIdentity::resolve`].
        Declared,
    }

    /// Who the app is, to every OS service that keys something on it: the
    /// notification permission (a bundle id), the toast registration (an
    /// AUMID), the `.desktop` file a server or a compositor matches (the
    /// `desktop-entry` hint, the Wayland `app_id`, the X11 `WM_CLASS`).
    ///
    /// ONE value, each platform's form projected from it, so the shells can
    /// no longer derive the same app under different names.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AppIdentity {
        /// Reverse-DNS: `com.azul.azwidgets`, or what the platform declared.
        pub id: String,
        /// The executable's file name, without its directory or `.exe`:
        /// `AzWidgets`. Empty when it could not be read.
        pub exe_name: String,
        pub source: AppIdSource,
    }

    /// The executable's file name: no directory (`/` or `\`), no `.exe`.
    fn exe_file_name(exe_path: &str) -> String {
        let name = exe_path
            .rsplit(|c: char| c == '/' || c == '\\')
            .next()
            .unwrap_or("");
        let cut = name.len().saturating_sub(4);
        match name.get(cut..) {
            Some(ext) if cut > 0 && ext.eq_ignore_ascii_case(".exe") => name[..cut].to_string(),
            _ => name.to_string(),
        }
    }

    impl AppIdentity {
        /// Nobody named the app: `com.azul.<executable name>`, lowercased,
        /// every other run of characters one `-` - valid as a
        /// `CFBundleIdentifier`, an AUMID and a D-Bus name element alike.
        #[must_use]
        pub fn from_executable(exe_path: &str) -> Self {
            let exe_name = exe_file_name(exe_path);
            let mut tail = String::with_capacity(exe_name.len());
            for c in exe_name.chars() {
                if c.is_ascii_alphanumeric() {
                    tail.push(c.to_ascii_lowercase());
                } else if !tail.is_empty() && !tail.ends_with('-') {
                    tail.push('-');
                }
            }
            while tail.ends_with('-') {
                tail.pop();
            }
            if tail.is_empty() {
                tail.push_str("app");
            }
            Self {
                id: format!("com.azul.{tail}"),
                exe_name,
                source: AppIdSource::Executable,
            }
        }

        /// The platform or the app named it. An empty (or blank)
        /// declaration is none: the id is then derived as
        /// [`AppIdentity::from_executable`] does.
        #[must_use]
        pub fn declared(id: &str, exe_path: &str) -> Self {
            let id = id.trim();
            if id.is_empty() {
                return Self::from_executable(exe_path);
            }
            Self {
                id: id.to_string(),
                exe_name: exe_file_name(exe_path),
                source: AppIdSource::Declared,
            }
        }

        /// `CFBundleIdentifier`: letters, digits, `-` and `.` only - anything
        /// else (an underscore of a Flatpak or Android id) becomes `-`.
        #[must_use]
        pub fn apple_bundle_id(&self) -> String {
            self.id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect()
        }

        /// The Windows AppUserModelID ([`windows_aumid`]'s rules).
        #[must_use]
        pub fn windows_aumid(&self) -> String {
            windows_aumid(&self.id)
        }

        /// The freedesktop `desktop-entry` hint, and the default Wayland
        /// `app_id` and X11 `WM_CLASS` instance - one string, because a
        /// server and a compositor must find the same `.desktop` file. A
        /// declared id names that file (`<id>.desktop`, the Flatpak rule);
        /// an unnamed app's is its executable's name, the convention every
        /// toolkit's `WM_CLASS` default follows.
        #[must_use]
        pub fn desktop_entry(&self) -> String {
            match self.source {
                AppIdSource::Declared => self.id.clone(),
                AppIdSource::Executable => desktop_entry(&self.exe_name),
            }
        }

        /// What a person reads: the Windows toast's `DisplayName`, the
        /// freedesktop `app_name`.
        #[must_use]
        pub fn display_name(&self) -> String {
            if self.exe_name.is_empty() {
                String::from("Azul")
            } else {
                self.exe_name.clone()
            }
        }
    }

    /// What the PLATFORM declares the app to be. It outranks the app's own
    /// `AppConfig::app_id`: the OS keys its services on it (the notification
    /// permission, TCC and LaunchServices on a bundle id; everything on the
    /// Android package; the portal and the sandbox's `.desktop` file on the
    /// Flatpak id), and an app cannot change it at run time.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum PlatformAppId {
        /// The running `.app`'s `CFBundleIdentifier` (macOS, iOS).
        AppleBundle(String),
        /// The package of the Android manifest.
        AndroidPackage(String),
        /// `FLATPAK_ID`, inside a Flatpak sandbox (Linux).
        Flatpak(String),
    }

    impl PlatformAppId {
        /// The id the platform declared.
        #[must_use]
        pub fn id(&self) -> &str {
            match self {
                Self::AppleBundle(id) | Self::AndroidPackage(id) | Self::Flatpak(id) => id,
            }
        }

        /// Where the id comes from, for the person reading the log.
        #[must_use]
        pub fn origin(&self) -> &'static str {
            match self {
                Self::AppleBundle(_) => "the app bundle's CFBundleIdentifier",
                Self::AndroidPackage(_) => "the Android manifest package",
                Self::Flatpak(_) => "FLATPAK_ID (the Flatpak sandbox)",
            }
        }

        /// Whether the platform's id names the same app as `app_id`: equal,
        /// or - for a bundle - equal to `app_id`'s Apple form (what `azul-doc
        /// bundle macos` writes when the identifier has an underscore).
        fn names(&self, app_id: &str) -> bool {
            let id = self.id().trim();
            match self {
                Self::AppleBundle(_) => {
                    id == app_id || id == AppIdentity::declared(app_id, "").apple_bundle_id()
                }
                Self::AndroidPackage(_) | Self::Flatpak(_) => id == app_id,
            }
        }
    }

    /// The app's identity, and what became of the app's own declaration.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ResolvedAppIdentity {
        pub identity: AppIdentity,
        /// Set when the platform's id overrode a DIFFERENT `app_id`: the app
        /// asked for one name and runs under another.
        pub warning: Option<String>,
    }

    impl AppIdentity {
        /// Who the app is, from everything that may name it - ONE rule for
        /// every OS, a pure function of its inputs:
        ///
        /// 1. the platform's declaration (`platform`: a bundle's `CFBundleIdentifier`, the
        ///    Android package, `FLATPAK_ID`) wins - the OS keys its services on it and the app
        ///    cannot change it. An `app_id` that names another app is reported in `warning`;
        /// 2. else the app's own `app_id` (`AppConfig::app_id`): on Windows the AUMID and the COM
        ///    activator, on Linux the `desktop-entry` hint, the Wayland `app_id` and the X11
        ///    `WM_CLASS` defaults;
        /// 3. else the id derived from the executable ([`AppIdentity::from_executable`]).
        ///
        /// A blank id - from the app or the platform - is no declaration.
        #[must_use]
        pub fn resolve(
            app_id: &str,
            platform: Option<&PlatformAppId>,
            exe_path: &str,
        ) -> ResolvedAppIdentity {
            let app_id = app_id.trim();
            match platform.filter(|p| !p.id().trim().is_empty()) {
                Some(platform) => {
                    let identity = Self::declared(platform.id(), exe_path);
                    let warning = (!app_id.is_empty() && !platform.names(app_id)).then(|| {
                        format!(
                            "AppConfig::app_id is {app_id:?}, but {} is {:?}: the OS knows the \
                             app by the latter, so it is the app's id here. Declare the same id \
                             in both (a bundle's id comes from [package.metadata.bundle] \
                             identifier)",
                            platform.origin(),
                            identity.id
                        )
                    });
                    ResolvedAppIdentity { identity, warning }
                }
                // `declared` treats an empty id as none: the executable's.
                None => ResolvedAppIdentity {
                    identity: Self::declared(app_id, exe_path),
                    warning: None,
                },
            }
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
