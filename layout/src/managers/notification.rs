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
//!   callback against its first window with `invoke_menu_callback`, exactly as it invokes a tray
//!   menu item's callback. Every event is the notification's last, so the registry forgets the
//!   callback as it routes - which is what stops the `NotificationClosed` a freedesktop server
//!   sends after `ActionInvoked` from arriving as a second event.
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
    let mut q = PENDING_REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_QUEUED_REQUESTS {
        return false;
    }
    q.push(request);
    true
}

/// Take every queued request, in the order the app made them.
pub fn drain_notification_requests() -> Vec<NotificationRequest> {
    let mut q = PENDING_REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Is a request waiting? The dll's capability pump arms its wake-up timer on
/// this, so a request queued by a callback that returned `DoNothing` does not
/// wait for an unrelated event to be dispatched.
pub fn has_queued_requests() -> bool {
    PENDING_REQUESTS
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
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

// ────────── Routing ────────────────────────────────────────────────────

/// One event, routed to the callback of the notification it names. The run
/// loop invokes `callback` with `event` installed as the current event
/// ([`with_current_notification_event`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationDelivery {
    pub callback: NotificationCallback,
    pub event: NotificationEvent,
}

/// The live notifications, by the app's id, and where their events go.
///
/// Owned by the dll's notification service on the main thread. Pure - no
/// globals - so the routing rules are testable on their own.
#[derive(Debug, Default)]
pub struct NotificationRegistry {
    live: BTreeMap<String, OptionNotificationCallback>,
    generated: u64,
}

impl NotificationRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            live: BTreeMap::new(),
            generated: 0,
        }
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
        self.live.insert(
            notification.id.as_str().to_string(),
            notification.callback.clone(),
        );
        notification
    }

    /// Forget a withdrawn notification. `true` if it was live.
    pub fn forget(&mut self, id: &str) -> bool {
        self.live.remove(id).is_some()
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
    /// routed. An event for an id that is not live - never posted, already
    /// ended, or withdrawn - is dropped: that is the freedesktop close that
    /// follows a click, or the close that confirms a withdraw. A live
    /// notification without a callback ends silently.
    pub fn route(&mut self, events: Vec<NotificationEvent>) -> Vec<NotificationDelivery> {
        let mut out = Vec::new();
        for event in events {
            if let Some(OptionNotificationCallback::Some(callback)) =
                self.live.remove(event.notification_id.as_str())
            {
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

    use azul_core::notification::{Notification, NotificationAction, NotificationEvent};
    use azul_css::AzString;

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
        // FNV-1a, 64 bit: stable across runs and platforms, unlike the
        // std hasher, so a category registered by one launch is recognised
        // by the next.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for action in actions {
            eat(action.id.as_str().as_bytes());
            eat(&[0x1f]);
            eat(action.label.as_str().as_bytes());
            eat(&[0x1e]);
        }
        format!("azul.notification.actions.{hash:016x}")
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
}
