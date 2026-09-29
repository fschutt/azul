//! The "Notifications" section: a real OS notification instead of the
//! in-window Toast the demo used to show.
//!
//! "Post a notification" posts a native notification with one action button
//! (Notification Center on macOS, the desktop's notification server on Linux,
//! a notification-area toast on Windows); "Withdraw it" takes it down again.
//! Whatever the user does with it - clicks it, presses the button, dismisses
//! it - or why it could not be shown comes back to `on_notification_event`,
//! which reads it with `CallbackInfo::get_notification_event` and shows it as
//! "Last event". "Platform support" is `PlatformCapability::notifications()`,
//! asked once at start-up: on macOS it is `false` for an unbundled binary
//! (`target/release/AzWidgets`), with the reason - run the demo from a `.app`
//! bundle to see the notification.

use azul::{
    notification::{Notification, NotificationEvent, NotificationEventType},
    prelude::*,
    widgets::*,
    window::{PlatformCapability, UiTheme},
};

use crate::{labelled, section, Showcase};

/// The demo's one notification. Posting again REPLACES the one showing.
const DEMO_NOTIFICATION: &str = "azul-widgets-demo";
/// The id of its action button, reported back when the button is pressed.
const OPEN_ACTION: &str = "show-me";
/// The payload it carries: every event about it brings this back (from the
/// platform where it carries it, from the post's record otherwise), which is
/// how an app tells WHAT a click was about - a document, a chat, a download.
const DEMO_PAYLOAD: &str = "azul-widgets-demo:show";

/// What the section shows. Lives in `Showcase::notifications`.
#[derive(Debug, Clone)]
pub(crate) struct NotificationsDemo {
    /// `PlatformCapability::notifications()`, as one line.
    capability: String,
    /// The last event the notification reported (or what the demo did).
    last_event: String,
}

impl NotificationsDemo {
    /// Ask the platform once, at start-up: on Linux the probe is a D-Bus round
    /// trip, which a layout callback should not repeat on every frame.
    pub(crate) fn probe() -> Self {
        let cap = PlatformCapability::notifications();
        let capability = match (cap.available, cap.reason.as_str().is_empty()) {
            (true, true) => format!("Available - {}", cap.backend.as_str()),
            (true, false) => format!(
                "Available - {} ({})",
                cap.backend.as_str(),
                cap.reason.as_str()
            ),
            (false, _) => format!(
                "Unavailable - {}: {}",
                cap.backend.as_str(),
                cap.reason.as_str()
            ),
        };
        Self {
            capability,
            last_event: "Nothing posted yet.".to_string(),
        }
    }
}

fn describe(event: &NotificationEvent) -> String {
    let what = describe_kind(event);
    match event.payload.as_str() {
        "" => what,
        payload => format!("{what} Payload: \"{payload}\"."),
    }
}

fn describe_kind(event: &NotificationEvent) -> String {
    match event.kind {
        NotificationEventType::Activated => "Clicked (the notification itself).".to_string(),
        NotificationEventType::ActionInvoked => {
            format!("Button pressed: \"{}\".", event.action_id.as_str())
        }
        NotificationEventType::Dismissed => {
            if event.reason.as_str().is_empty() {
                "Dismissed.".to_string()
            } else {
                format!("Dismissed ({}).", event.reason.as_str())
            }
        }
        NotificationEventType::Failed => format!("Not shown: {}", event.reason.as_str()),
    }
}

fn set_last_event(data: &mut RefAny, text: String) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.notifications.last_event = text;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The notification's own callback: every event it reports lands here.
extern "C" fn on_notification_event(mut data: RefAny, info: CallbackInfo) -> Update {
    match info.get_notification_event().into_option() {
        Some(event) => set_last_event(&mut data, describe(&event)),
        None => Update::DoNothing,
    }
}

extern "C" fn on_post(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.post_notification(
        Notification::create(DEMO_NOTIFICATION, "Azul Widget Showcase")
            .with_body("A native notification, posted by the widgets demo.")
            .with_action(OPEN_ACTION, "Show me")
            .with_payload(DEMO_PAYLOAD)
            .with_callback(data.clone(), on_notification_event),
    );
    set_last_event(
        &mut data,
        "Posted - click it, press its button or dismiss it.".to_string(),
    )
}

extern "C" fn on_withdraw(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.withdraw_notification(DEMO_NOTIFICATION);
    set_last_event(
        &mut data,
        "Withdrawn - a withdrawn notification reports nothing more.".to_string(),
    )
}

/// `theme` is the page's widget theme (the toolbar's Flat / Flora).
pub(crate) fn notifications_section(
    data: &RefAny,
    state: &NotificationsDemo,
    theme: UiTheme,
) -> Dom {
    let buttons = Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(
            Button::with_type("Post a notification", ButtonType::Primary)
                .with_on_click(data.clone(), on_post)
                .with_theme(theme)
                .dom()
                .with_css("margin-right: 8px;"),
        )
        .with_child(
            Button::create("Withdraw it")
                .with_on_click(data.clone(), on_withdraw)
                .with_theme(theme)
                .dom(),
        );
    section(
        "Notifications",
        vec![
            labelled("Native notification (one action button)", buttons),
            labelled(
                "Last event",
                Dom::create_span_with_text(state.last_event.as_str())
                    .with_css("color: system:text;"),
            ),
            labelled(
                "Platform support",
                Dom::create_span_with_text(state.capability.as_str())
                    .with_css("font-size: 12px; color: system:secondary-text;"),
            ),
        ],
    )
}
