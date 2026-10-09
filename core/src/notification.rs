//! Native desktop notifications  -  platform-agnostic model.
//!
//! The same three-layer split as the system tray ([`crate::tray`]):
//!
//! * this module is the DATA the backends agree on;
//! * `azul_layout::managers::notification` holds the queues (a callback's request out to the
//!   platform, the platform's event back in), the routing of an event to the notification it names,
//!   and the headless recorder;
//! * `azul-dll` (`desktop/notifications/`) is the OS plumbing: `UNUserNotificationCenter` on
//!   macOS, `org.freedesktop.Notifications` over D-Bus on Linux, a `Shell_NotifyIconW` balloon on
//!   Windows.
//!
//! # Why the API has this shape
//!
//! * **The APP names each notification.** macOS keys a request by a string identifier, a
//!   freedesktop server hands back a `u32` it chose itself, and Windows shows one balloon per notify
//!   icon. None of those survives as the app's handle, so [`Notification::id`] is a string the app
//!   picks: posting the same id again REPLACES the notification, `withdraw` takes it, and every
//!   [`NotificationEvent`] carries it.
//! * **The result comes back to a callback the notification carries** - exactly how a tray menu
//!   item carries its callback. The platform queues the event, the run loop routes it to the
//!   notification's [`NotificationCallback`] and invokes that against a window, and
//!   `CallbackInfo::get_notification_event` tells the callback what happened.
//! * **Every event ends the notification.** A click, an action button, a dismissal and a failure
//!   are each the LAST thing a notification reports: after any of them its callback is forgotten.
//!   That is what keeps one click from arriving twice - a freedesktop server reports `ActionInvoked`
//!   and then `NotificationClosed` for the same click.
//! * **Posting can fail, and the failure is an event** ([`NotificationEventType::Failed`], with a
//!   reason), not a silently missing banner. An unbundled macOS binary, a Linux session with no
//!   notification server, a denied permission and a full request queue all end up there.
//! * **A notification can outlive the process that posted it**, and its callback cannot. A tap
//!   that cold-launches the app (the normal case on iOS and Android, a relaunch on macOS) names a
//!   notification this process never posted. Such events - and those of a notification posted
//!   without a callback - go to the APP-LEVEL handler, `AppConfig::notification_handler`, and the
//!   one piece of app data that survives the restart is [`Notification::payload`], which comes
//!   back as [`NotificationEvent::payload`].

use azul_css::{AzString, OptionString, OptionU64};

use crate::{callbacks::CoreCallback, refany::RefAny};

/// One button on a notification.
///
/// Where the platform shows buttons: macOS (as `UNNotificationAction`s of a
/// category) and freedesktop servers that advertise the `actions` capability.
/// Windows balloons have none; the actions are dropped there, and
/// `PlatformCapability::notifications()` says so.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NotificationAction {
    /// Stable id, reported back as [`NotificationEvent::action_id`]. Must be
    /// non-empty; `"default"` is reserved (it is the freedesktop key of a
    /// click on the notification's body).
    pub id: AzString,
    /// The button's text.
    pub label: AzString,
}

impl NotificationAction {
    #[must_use]
    pub const fn create(id: AzString, label: AzString) -> Self {
        Self { id, label }
    }
}

impl_option!(
    NotificationAction,
    OptionNotificationAction,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    NotificationAction,
    NotificationActionVec,
    NotificationActionVecDestructor,
    NotificationActionVecDestructorType,
    NotificationActionVecSlice,
    OptionNotificationAction
);
impl_vec_debug!(NotificationAction, NotificationActionVec);
impl_vec_clone!(
    NotificationAction,
    NotificationActionVec,
    NotificationActionVecDestructor
);
impl_vec_partialeq!(NotificationAction, NotificationActionVec);

/// What a notification sounds like.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C, u8)]
pub enum NotificationSound {
    /// The platform's notification sound.
    #[default]
    Default,
    /// No sound. (freedesktop `suppress-sound`, Windows `NIIF_NOSOUND`.)
    Silent,
    /// A named sound: an `.aiff`/`.caf` in the app bundle's resources on
    /// macOS, a freedesktop sound-theme name (`"message-new-instant"`) on
    /// Linux. Windows plays its default sound.
    Named(AzString),
}

/// The callback a notification reports to, and the data it gets.
///
/// The same pair a tray / window menu item carries
/// ([`crate::menu::CoreMenuCallback`]), and invoked through the same path:
/// the event is routed to it by the run loop and it runs with a
/// `CallbackInfo` built from the app's most recently focused window (else its
/// oldest), which is what lets it change app state and return
/// `Update::RefreshDom`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NotificationCallback {
    /// Handed to the callback when an event for this notification arrives.
    pub refany: RefAny,
    /// Reads the event with `CallbackInfo::get_notification_event`.
    pub callback: CoreCallback,
}

impl_option!(
    NotificationCallback,
    OptionNotificationCallback,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// A native notification, as posted by `CallbackInfo::post_notification`.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Notification {
    /// The app's name for this notification. Posting another notification
    /// with the same id REPLACES this one; `withdraw_notification` takes it;
    /// every event carries it. An empty id is given a unique generated one,
    /// which such a notification can then not be withdrawn by.
    pub id: AzString,
    /// The bold first line.
    pub title: AzString,
    /// The text under the title. May be empty.
    pub body: AzString,
    /// Path of an image file to show with the notification (PNG is the safe
    /// choice; on Windows only a `.ico` file is shown). On Linux a
    /// freedesktop icon-theme name is accepted too. The platform's app icon
    /// is always shown by the system regardless.
    pub icon: OptionString,
    /// Buttons, in order. See [`NotificationAction`] for where they appear.
    pub actions: NotificationActionVec,
    pub sound: NotificationSound,
    /// App data handed back in every [`NotificationEvent::payload`] of this
    /// notification - a message id, a route, a document path. Unlike the
    /// callback's `RefAny` it survives the process: it rides along with the
    /// notification itself (UN `userInfo`, the Android `Intent`, the toast's
    /// launch arguments), so the app-level handler of a freshly launched
    /// process can still tell which notification was tapped. Empty = none.
    pub payload: AzString,
    /// Where events for this notification go. `None`: events go to the
    /// app-level handler (`AppConfig::notification_handler`) if the app set
    /// one, and nowhere otherwise (a fire-and-forget notification).
    pub callback: OptionNotificationCallback,
    /// When to show it: an instant in milliseconds since 1970 (UTC). `None`
    /// (the default) shows it at once, and so does a time in the past or
    /// less than a second away. A later time SCHEDULES it: the OS keeps it
    /// and shows it then - also while the app is not running - where it can
    /// (macOS and iOS: a `UNTimeIntervalNotificationTrigger`; Windows: a
    /// scheduled toast, delivered within about five minutes); elsewhere
    /// (Linux, Android, the Windows balloon) the running process holds it and
    /// posts it when it is due. Withdrawing the id cancels it. Repeats are
    /// the app's: it schedules each occurrence (same id = replaced).
    pub deliver_at: OptionU64,
}

impl Notification {
    /// A notification with a title, no body, no icon, no actions, the
    /// platform's default sound and no callback.
    #[must_use]
    pub const fn create(id: AzString, title: AzString) -> Self {
        Self {
            id,
            title,
            body: AzString::from_const_str(""),
            icon: OptionString::None,
            actions: NotificationActionVec::from_const_slice(&[]),
            sound: NotificationSound::Default,
            payload: AzString::from_const_str(""),
            callback: OptionNotificationCallback::None,
            deliver_at: OptionU64::None,
        }
    }

    /// Show it at `unix_ms` (milliseconds since 1970, UTC) instead of now -
    /// see [`Notification::deliver_at`].
    #[must_use]
    pub const fn with_deliver_at(mut self, unix_ms: u64) -> Self {
        self.deliver_at = OptionU64::Some(unix_ms);
        self
    }

    /// See [`Notification::payload`].
    #[must_use]
    pub fn with_payload(mut self, payload: AzString) -> Self {
        self.payload = payload;
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: AzString) -> Self {
        self.body = body;
        self
    }

    /// See [`Notification::icon`].
    #[must_use]
    pub fn with_icon(mut self, path: AzString) -> Self {
        self.icon = OptionString::Some(path);
        self
    }

    /// Append one button.
    #[must_use]
    pub fn with_action(mut self, id: AzString, label: AzString) -> Self {
        let mut actions = self.actions.as_ref().to_vec();
        actions.push(NotificationAction::create(id, label));
        self.actions = NotificationActionVec::from_vec(actions);
        self
    }

    /// Replace all buttons.
    #[must_use]
    pub fn with_actions(mut self, actions: NotificationActionVec) -> Self {
        self.actions = actions;
        self
    }

    #[must_use]
    pub fn with_sound(mut self, sound: NotificationSound) -> Self {
        self.sound = sound;
        self
    }

    /// Report this notification's click / action / dismissal / failure to
    /// `callback`, with `data`. Inside it, `CallbackInfo::get_notification_event`
    /// says which of them it was.
    #[must_use]
    pub fn with_callback<I: Into<CoreCallback>>(mut self, data: RefAny, callback: I) -> Self {
        self.callback = OptionNotificationCallback::Some(NotificationCallback {
            refany: data,
            callback: callback.into(),
        });
        self
    }
}

/// What happened to a notification.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum NotificationEventType {
    /// The user clicked the notification itself (not one of its buttons).
    Activated,
    /// The user clicked a button; [`NotificationEvent::action_id`] says which.
    ActionInvoked,
    /// The notification went away without being clicked: closed by the user,
    /// expired, or hidden by the system. [`NotificationEvent::reason`] says
    /// which where the platform tells (freedesktop, Windows). macOS reports a
    /// dismissal only when the user clears it from Notification Center.
    Dismissed,
    /// The notification could not be shown. [`NotificationEvent::reason`]
    /// says why (no notification server, an unbundled macOS binary, the user
    /// denied the permission, an unsupported target, ...).
    Failed,
}

/// One thing that happened to a posted notification. Every event is the
/// notification's last (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NotificationEvent {
    /// [`Notification::id`] of the notification it happened to.
    pub notification_id: AzString,
    /// For [`NotificationEventType::ActionInvoked`]: the button's
    /// [`NotificationAction::id`]. Empty otherwise.
    pub action_id: AzString,
    /// For `Dismissed` and `Failed`: a human-readable why, where known.
    /// Empty otherwise.
    pub reason: AzString,
    /// The [`Notification::payload`] the notification was posted with. Filled
    /// in from the platform where it carries it back (macOS/iOS `userInfo`,
    /// Android extras, toast arguments) - which is what makes it reach an
    /// app-level handler in a process that did not post the notification -
    /// and from the posting process's own record otherwise.
    pub payload: AzString,
    /// What happened.
    pub kind: NotificationEventType,
    /// `true` when this event is what started the process: a tap on a
    /// notification of an app that was not running. Set where the platform
    /// says so (the Android launch `Intent`, macOS's
    /// `NSApplicationLaunchUserNotificationKey`, a Windows toast activator
    /// that COM started the app for with `-ToastActivated`) and on iOS, which
    /// names nothing for a local notification, for the first response that
    /// arrives before the app first became active. `false` elsewhere (on
    /// Linux a click on a notification of an exited app reaches no process).
    pub launched_app: bool,
}

impl NotificationEvent {
    #[must_use]
    pub const fn activated(notification_id: AzString) -> Self {
        Self {
            kind: NotificationEventType::Activated,
            notification_id,
            action_id: AzString::from_const_str(""),
            reason: AzString::from_const_str(""),
            payload: AzString::from_const_str(""),
            launched_app: false,
        }
    }

    #[must_use]
    pub const fn action_invoked(notification_id: AzString, action_id: AzString) -> Self {
        Self {
            kind: NotificationEventType::ActionInvoked,
            notification_id,
            action_id,
            reason: AzString::from_const_str(""),
            payload: AzString::from_const_str(""),
            launched_app: false,
        }
    }

    #[must_use]
    pub const fn dismissed(notification_id: AzString) -> Self {
        Self {
            kind: NotificationEventType::Dismissed,
            notification_id,
            action_id: AzString::from_const_str(""),
            reason: AzString::from_const_str(""),
            payload: AzString::from_const_str(""),
            launched_app: false,
        }
    }

    /// A dismissal with the platform's reason attached.
    #[must_use]
    pub const fn dismissed_because(notification_id: AzString, reason: AzString) -> Self {
        Self {
            kind: NotificationEventType::Dismissed,
            notification_id,
            action_id: AzString::from_const_str(""),
            reason,
            payload: AzString::from_const_str(""),
            launched_app: false,
        }
    }

    #[must_use]
    pub const fn failed(notification_id: AzString, reason: AzString) -> Self {
        Self {
            kind: NotificationEventType::Failed,
            notification_id,
            action_id: AzString::from_const_str(""),
            reason,
            payload: AzString::from_const_str(""),
            launched_app: false,
        }
    }
}

impl_option!(
    NotificationEvent,
    OptionNotificationEvent,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
