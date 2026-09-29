//! Native notifications: the platform-independent half.
//!
//! The OS backends (UNUserNotificationCenter, `org.freedesktop.Notifications`,
//! the `Shell_NotifyIconW` balloon) live in `azul-dll` and cannot run here.
//! Everything they share can, and is what these tests pin:
//!
//! * the request queue a callback writes (`CallbackInfo::post_notification` /
//!   `withdraw_notification`) and the dll drains, in order;
//! * the event mailbox the platform callbacks write from whatever thread the OS
//!   calls them on - bounded, like the tray's;
//! * the routing of an event to the callback of the notification it names -
//!   once, however many "it closed" signals a desktop sends after a click;
//! * the delivery itself: the routed callback runs through
//!   `LayoutWindow::invoke_single_callback` (what `invoke_menu_callback` calls
//!   for a tray menu item) and reads WHICH event it is running for from
//!   `CallbackInfo::get_notification_event`;
//! * the recorder the headless backend writes, so a test or an AZ_E2E scenario
//!   can assert what an app posted;
//! * the translation of each platform's wire vocabulary (freedesktop action
//!   keys and close reasons, UN action identifiers, balloon messages) into
//!   the one event type, and the UTF-16 truncation the balloon struct needs.

use std::sync::{Arc, Mutex, MutexGuard};

use azul_core::{
    callbacks::Update,
    dom::Dom,
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    notification::{
        Notification, NotificationAction, NotificationActionVec, NotificationEvent,
        NotificationEventType, NotificationSound,
    },
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::RawWindowHandle,
};
use azul_css::{system::SystemStyle, AzString};
use azul_layout::{
    callbacks::{Callback, CallbackInfo, ExternalSystemCallbacks},
    managers::notification::{
        clear_recorded_notifications, current_notification_event, drain_notification_events,
        drain_notification_requests, has_queued_requests, push_notification_request,
        queue_notification_event, record_posted_notification, record_withdrawn_notification,
        recorded_notifications, wire, with_current_notification_event, NotificationDelivery,
        NotificationRecorder, NotificationRegistry, NotificationRequest, MAX_QUEUED_EVENTS,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The queues and the recorder are process globals (the platform callbacks
/// that write them hold nothing else), so the tests that touch them take
/// turns. Poison is ignored: one failing test must not fail the rest.
fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn s(v: &str) -> AzString {
    AzString::from(v)
}

/// What the callback under test saw.
#[derive(Debug, Default)]
struct Seen {
    events: Vec<NotificationEvent>,
    calls: usize,
}

extern "C" fn remember_event(mut data: RefAny, info: CallbackInfo) -> Update {
    let event = info.get_notification_event();
    if let Some(mut seen) = data.downcast_mut::<Seen>() {
        seen.calls += 1;
        if let Some(event) = event {
            seen.events.push(event);
        }
    }
    Update::DoNothing
}

/// A callback that asks for a notification to be posted and another to be
/// withdrawn, the way an app's button does.
extern "C" fn post_and_withdraw(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.post_notification(
        Notification::create(s("posted-from-a-callback"), s("Upload finished"))
            .with_body(s("3 files")),
    );
    info.withdraw_notification(s("stale"));
    Update::DoNothing
}

fn seen_by(data: &RefAny) -> (Vec<NotificationEvent>, usize) {
    let mut probe = data.clone();
    let seen = probe.downcast_ref::<Seen>().expect("the RefAny holds a Seen");
    (seen.events.clone(), seen.calls)
}

fn notification_with_callback(id: &str, data: &RefAny) -> Notification {
    Notification::create(s(id), s("Build finished"))
        .with_body(s("azul-dll built in 3m12s"))
        .with_action(s("open-log"), s("Open log"))
        .with_callback(data.clone(), Callback::from_ptr(remember_event))
}

/// An empty page, laid out once: what `invoke_single_callback` runs against.
fn laid_out_window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(Dom::create_body()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

/// Deliver the way the shell does: the notification's callback, invoked
/// through the single-callback path a tray menu item uses, with the event it
/// is running for installed for the duration.
fn deliver(lw: &mut LayoutWindow, delivery: NotificationDelivery) -> Update {
    let NotificationDelivery { callback, event } = delivery;
    let mut cb = Callback::from_core(callback.callback);
    let mut data = callback.refany;
    let state = lw.current_window_state.clone();
    with_current_notification_event(&event, || {
        lw.invoke_single_callback(
            &mut cb,
            &mut data,
            &RawWindowHandle::Unsupported,
            &OptionGlContextPtr::None,
            Arc::new(SystemStyle::default()),
            &ExternalSystemCallbacks::rust_internal(),
            &None,
            &state,
            &RendererResources::default(),
        )
        .1
    })
}

// ---------------------------------------------------------------------------
// The data model
// ---------------------------------------------------------------------------

#[test]
fn a_notification_is_built_with_everything_the_backends_need() {
    let n = Notification::create(s("mail-7"), s("New mail"))
        .with_body(s("From: Ada"))
        .with_icon(s("/tmp/ada.png"))
        .with_action(s("reply"), s("Reply"))
        .with_action(s("archive"), s("Archive"))
        .with_sound(NotificationSound::Silent);
    assert_eq!(n.id.as_str(), "mail-7");
    assert_eq!(n.title.as_str(), "New mail");
    assert_eq!(n.body.as_str(), "From: Ada");
    assert_eq!(n.icon.as_ref().map(AzString::as_str), Some("/tmp/ada.png"));
    let actions: Vec<(&str, &str)> = n
        .actions
        .as_ref()
        .iter()
        .map(|a| (a.id.as_str(), a.label.as_str()))
        .collect();
    assert_eq!(actions, vec![("reply", "Reply"), ("archive", "Archive")]);
    assert_eq!(n.sound, NotificationSound::Silent);
    assert!(n.callback.is_none(), "no callback until one is attached");

    let plain = Notification::create(s("x"), s("t"));
    assert_eq!(plain.body.as_str(), "");
    assert!(plain.icon.is_none());
    assert!(plain.actions.as_ref().is_empty());
    assert_eq!(plain.sound, NotificationSound::Default);

    let replaced = plain.with_actions(NotificationActionVec::from_vec(vec![
        NotificationAction::create(s("a"), s("A")),
    ]));
    assert_eq!(replaced.actions.as_ref().len(), 1);
}

#[test]
fn every_event_names_its_notification_and_what_happened() {
    let clicked = NotificationEvent::activated(s("n"));
    assert_eq!(clicked.kind, NotificationEventType::Activated);
    assert_eq!(clicked.notification_id.as_str(), "n");
    assert_eq!(clicked.action_id.as_str(), "");

    let action = NotificationEvent::action_invoked(s("n"), s("reply"));
    assert_eq!(action.kind, NotificationEventType::ActionInvoked);
    assert_eq!(action.action_id.as_str(), "reply");

    let gone = NotificationEvent::dismissed(s("n"));
    assert_eq!(gone.kind, NotificationEventType::Dismissed);

    let failed = NotificationEvent::failed(s("n"), s("no notification server"));
    assert_eq!(failed.kind, NotificationEventType::Failed);
    assert_eq!(failed.reason.as_str(), "no notification server");
}

// ---------------------------------------------------------------------------
// Queue and dispatch
// ---------------------------------------------------------------------------

#[test]
fn requests_reach_the_platform_in_the_order_the_app_made_them() {
    let _serial = serial();
    drop(drain_notification_requests());
    assert!(!has_queued_requests(), "premise: an empty queue");

    assert!(push_notification_request(NotificationRequest::Post(
        Notification::create(s("a"), s("first"))
    )));
    assert!(push_notification_request(NotificationRequest::Withdraw(s("a"))));
    assert!(push_notification_request(NotificationRequest::Post(
        Notification::create(s("b"), s("second"))
    )));
    assert!(has_queued_requests(), "the dll's pump must see the queue as armed");

    let drained = drain_notification_requests();
    let order: Vec<String> = drained
        .iter()
        .map(|r| match r {
            NotificationRequest::Post(n) => format!("post {}", n.id.as_str()),
            NotificationRequest::Withdraw(id) => format!("withdraw {}", id.as_str()),
        })
        .collect();
    assert_eq!(order, vec!["post a", "withdraw a", "post b"]);
    assert!(!has_queued_requests(), "draining empties the queue");
    assert!(drain_notification_requests().is_empty());
}

#[test]
fn a_callback_posts_and_withdraws_through_callback_info() {
    let _serial = serial();
    drop(drain_notification_requests());
    let mut lw = laid_out_window();

    let mut cb = Callback::from_ptr(post_and_withdraw);
    let mut data = RefAny::new(());
    let state = lw.current_window_state.clone();
    let (_, update) = lw.invoke_single_callback(
        &mut cb,
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    assert_eq!(update, Update::DoNothing);

    let drained = drain_notification_requests();
    assert_eq!(drained.len(), 2, "one post, one withdraw: {drained:?}");
    match &drained[0] {
        NotificationRequest::Post(n) => {
            assert_eq!(n.id.as_str(), "posted-from-a-callback");
            assert_eq!(n.body.as_str(), "3 files");
        }
        other => panic!("expected the post first, got {other:?}"),
    }
    assert_eq!(drained[1], NotificationRequest::Withdraw(s("stale")));
}

#[test]
fn the_event_mailbox_is_bounded_and_drains_in_arrival_order() {
    let _serial = serial();
    drop(drain_notification_events());

    for i in 0..MAX_QUEUED_EVENTS {
        assert!(
            queue_notification_event(NotificationEvent::dismissed(AzString::from(format!(
                "n{i}"
            )))),
            "event {i} fits"
        );
    }
    assert!(
        !queue_notification_event(NotificationEvent::dismissed(s("one-too-many"))),
        "an app that never drains must not grow the mailbox without bound"
    );

    let drained = drain_notification_events();
    assert_eq!(drained.len(), MAX_QUEUED_EVENTS);
    assert_eq!(drained[0].notification_id.as_str(), "n0");
    assert_eq!(
        drained[MAX_QUEUED_EVENTS - 1].notification_id.as_str(),
        format!("n{}", MAX_QUEUED_EVENTS - 1)
    );
    assert!(drain_notification_events().is_empty());
}

#[test]
fn an_event_routes_to_the_callback_of_the_notification_it_names() {
    let first = RefAny::new(Seen::default());
    let second = RefAny::new(Seen::default());
    let mut registry = NotificationRegistry::new();
    registry.admit(notification_with_callback("first", &first));
    registry.admit(notification_with_callback("second", &second));
    assert_eq!(registry.live_count(), 2);

    let deliveries = registry.route(vec![
        NotificationEvent::action_invoked(s("second"), s("open-log")),
        NotificationEvent::activated(s("never-posted")),
    ]);
    assert_eq!(deliveries.len(), 1, "an event for an unknown id is dropped");
    assert_eq!(deliveries[0].event.notification_id.as_str(), "second");
    assert_eq!(deliveries[0].callback.refany, second);
    assert!(registry.is_live("first"), "the other notification is untouched");
    assert!(!registry.is_live("second"));
}

#[test]
fn a_close_after_a_click_is_not_delivered_twice() {
    // A freedesktop server sends ActionInvoked and THEN NotificationClosed for
    // one click; Windows sends a hide after a click. The app hears the click.
    let data = RefAny::new(Seen::default());
    let mut registry = NotificationRegistry::new();
    registry.admit(notification_with_callback("n", &data));

    let deliveries = registry.route(vec![
        NotificationEvent::activated(s("n")),
        NotificationEvent::dismissed(s("n")),
    ]);
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].event.kind, NotificationEventType::Activated);
    assert_eq!(registry.live_count(), 0, "every event ends the notification");
    assert!(registry
        .route(vec![NotificationEvent::dismissed(s("n"))])
        .is_empty());
}

#[test]
fn a_withdrawn_notification_delivers_nothing() {
    let data = RefAny::new(Seen::default());
    let mut registry = NotificationRegistry::new();
    registry.admit(notification_with_callback("n", &data));
    assert!(registry.forget("n"));
    assert!(!registry.forget("n"), "forgetting twice reports nothing to forget");
    // The server confirms the CloseNotification the withdraw sent.
    assert!(registry
        .route(vec![NotificationEvent::dismissed(s("n"))])
        .is_empty());
}

#[test]
fn a_repost_under_the_same_id_replaces_the_callback() {
    let old = RefAny::new(Seen::default());
    let new = RefAny::new(Seen::default());
    let mut registry = NotificationRegistry::new();
    registry.admit(notification_with_callback("progress", &old));
    registry.admit(notification_with_callback("progress", &new));
    assert_eq!(registry.live_count(), 1, "same id, one notification");

    let deliveries = registry.route(vec![NotificationEvent::activated(s("progress"))]);
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].callback.refany, new);
}

#[test]
fn a_notification_without_a_callback_is_tracked_but_delivers_nothing() {
    let mut registry = NotificationRegistry::new();
    registry.admit(Notification::create(s("fire-and-forget"), s("Saved")));
    assert!(registry.is_live("fire-and-forget"));
    assert!(registry
        .route(vec![NotificationEvent::activated(s("fire-and-forget"))])
        .is_empty());
    assert!(!registry.is_live("fire-and-forget"));
}

#[test]
fn an_empty_id_is_given_a_unique_one() {
    let mut registry = NotificationRegistry::new();
    let a = registry.admit(Notification::create(s(""), s("a")));
    let b = registry.admit(Notification::create(s(""), s("b")));
    assert!(!a.id.as_str().is_empty());
    assert!(!b.id.as_str().is_empty());
    assert_ne!(a.id, b.id, "two anonymous posts must not replace each other");
    assert_eq!(registry.live_count(), 2);
}

// ---------------------------------------------------------------------------
// Event drain -> callback delivery
// ---------------------------------------------------------------------------

#[test]
fn the_delivered_callback_reads_its_event_from_callback_info() {
    let _serial = serial();
    drop(drain_notification_events());

    let data = RefAny::new(Seen::default());
    let mut registry = NotificationRegistry::new();
    registry.admit(notification_with_callback("build", &data));

    // What a platform callback does, on whatever thread the OS uses...
    assert!(queue_notification_event(NotificationEvent::action_invoked(
        s("build"),
        s("open-log")
    )));
    // ...and what the run loop does with it.
    let deliveries = registry.route(drain_notification_events());
    assert_eq!(deliveries.len(), 1);

    let mut lw = laid_out_window();
    for delivery in deliveries {
        assert_eq!(deliver(&mut lw, delivery), Update::DoNothing);
    }

    let (events, calls) = seen_by(&data);
    assert_eq!(calls, 1, "the callback ran once");
    assert_eq!(
        events,
        vec![NotificationEvent::action_invoked(s("build"), s("open-log"))],
        "and saw the event it was delivered for"
    );
    assert_eq!(
        current_notification_event(),
        None,
        "outside a delivery there is no current event"
    );
}

#[test]
fn an_ordinary_callback_sees_no_notification_event() {
    let data = RefAny::new(Seen::default());
    let mut lw = laid_out_window();
    let mut cb = Callback::from_ptr(remember_event);
    let mut d = data.clone();
    let state = lw.current_window_state.clone();
    lw.invoke_single_callback(
        &mut cb,
        &mut d,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    let (events, calls) = seen_by(&data);
    assert_eq!(calls, 1);
    assert!(events.is_empty(), "a click handler is not a notification handler");
}

#[test]
fn the_current_event_is_restored_after_a_nested_delivery() {
    let outer = NotificationEvent::activated(s("outer"));
    let inner = NotificationEvent::dismissed(s("inner"));
    with_current_notification_event(&outer, || {
        assert_eq!(current_notification_event(), Some(outer.clone()));
        with_current_notification_event(&inner, || {
            assert_eq!(current_notification_event(), Some(inner.clone()));
        });
        assert_eq!(current_notification_event(), Some(outer.clone()));
    });
    assert_eq!(current_notification_event(), None);
}

// ---------------------------------------------------------------------------
// The headless recorder
// ---------------------------------------------------------------------------

#[test]
fn the_recorder_keeps_every_post_in_order_without_its_callback() {
    let data = RefAny::new(Seen::default());
    let mut recorder = NotificationRecorder::new();
    recorder.record_post(&notification_with_callback("a", &data));
    recorder.record_post(&Notification::create(s("b"), s("second")));
    recorder.record_post(&Notification::create(s("a"), s("replaced")));

    let ids: Vec<(&str, &str)> = recorder
        .entries()
        .iter()
        .map(|r| (r.notification.id.as_str(), r.notification.title.as_str()))
        .collect();
    assert_eq!(ids, vec![("a", "Build finished"), ("b", "second"), ("a", "replaced")]);
    assert!(
        recorder.entries().iter().all(|r| r.notification.callback.is_none()),
        "a recording must not keep the app's RefAny alive"
    );
    assert_eq!(
        recorder.latest("a").map(|r| r.notification.title.as_str()),
        Some("replaced")
    );
    assert_eq!(
        recorder.entries()[0].notification.actions.as_ref()[0].id.as_str(),
        "open-log",
        "the actions are recorded as posted"
    );
}

#[test]
fn withdrawing_marks_the_recorded_posts_of_that_id() {
    let mut recorder = NotificationRecorder::new();
    recorder.record_post(&Notification::create(s("a"), s("one")));
    recorder.record_post(&Notification::create(s("b"), s("two")));
    assert!(recorder.record_withdraw("a"));
    assert!(!recorder.record_withdraw("never-posted"));
    assert!(recorder.latest("a").is_some_and(|r| r.withdrawn));
    assert!(recorder.latest("b").is_some_and(|r| !r.withdrawn));
}

#[test]
fn the_recorder_is_bounded() {
    let mut recorder = NotificationRecorder::new();
    for i in 0..(NotificationRecorder::MAX_ENTRIES + 5) {
        recorder.record_post(&Notification::create(AzString::from(format!("n{i}")), s("t")));
    }
    assert_eq!(recorder.entries().len(), NotificationRecorder::MAX_ENTRIES);
    assert_eq!(
        recorder.entries()[0].notification.id.as_str(),
        "n5",
        "the oldest entries go first"
    );
}

#[test]
fn the_process_recorder_is_what_an_e2e_assertion_reads() {
    let _serial = serial();
    clear_recorded_notifications();
    record_posted_notification(&Notification::create(s("e2e-1"), s("Hello from headless")));
    assert!(record_withdrawn_notification("e2e-1"));
    let recorded = recorded_notifications();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].notification.title.as_str(), "Hello from headless");
    assert!(recorded[0].withdrawn);
    clear_recorded_notifications();
    assert!(recorded_notifications().is_empty());
}

// ---------------------------------------------------------------------------
// Each platform's wire vocabulary -> the one event type
// ---------------------------------------------------------------------------

#[test]
fn freedesktop_actions_put_the_body_click_first_and_skip_the_reserved_key() {
    let n = Notification::create(s("n"), s("t"))
        .with_action(s("reply"), s("Reply"))
        .with_action(s("default"), s("Shadowed"))
        .with_action(s(""), s("No id"));
    assert_eq!(
        wire::freedesktop_actions(&n),
        vec!["default", "Open", "reply", "Reply"],
        "`default` is the body click; an action named `default` or with no id cannot be told \
         apart from it"
    );
}

#[test]
fn freedesktop_signals_become_events() {
    assert_eq!(
        wire::freedesktop_action_event("n", "default"),
        NotificationEvent::activated(s("n"))
    );
    assert_eq!(
        wire::freedesktop_action_event("n", "reply"),
        NotificationEvent::action_invoked(s("n"), s("reply"))
    );
    for reason in [1u32, 2, 3, 4, 99] {
        let ev = wire::freedesktop_closed_event("n", reason);
        assert_eq!(ev.kind, NotificationEventType::Dismissed, "reason {reason}");
        assert!(!ev.reason.as_str().is_empty(), "reason {reason} is explained");
    }
    assert!(wire::freedesktop_closed_event("n", 1).reason.as_str().contains("expired"));
}

#[test]
fn a_freedesktop_icon_path_becomes_a_file_uri() {
    assert_eq!(wire::freedesktop_icon("/tmp/a b.png"), "file:///tmp/a%20b.png");
    assert_eq!(wire::freedesktop_icon("dialog-information"), "dialog-information");
    assert_eq!(wire::freedesktop_icon("file:///x.png"), "file:///x.png");
}

#[test]
fn apple_responses_become_events() {
    let default_id = wire::APPLE_DEFAULT_ACTION;
    let dismiss_id = wire::APPLE_DISMISS_ACTION;
    assert_eq!(
        wire::apple_response_event("n", default_id, default_id, dismiss_id),
        NotificationEvent::activated(s("n"))
    );
    let cleared = wire::apple_response_event("n", dismiss_id, default_id, dismiss_id);
    assert_eq!(cleared.kind, NotificationEventType::Dismissed);
    assert_eq!(cleared.notification_id.as_str(), "n");
    assert_eq!(
        wire::apple_response_event("n", "reply", default_id, dismiss_id),
        NotificationEvent::action_invoked(s("n"), s("reply"))
    );
}

#[test]
fn an_apple_category_is_named_by_its_actions() {
    let a = [NotificationAction::create(s("reply"), s("Reply"))];
    let b = [NotificationAction::create(s("reply"), s("Answer"))];
    assert_eq!(wire::apple_category_id(&a), wire::apple_category_id(&a));
    assert_ne!(
        wire::apple_category_id(&a),
        wire::apple_category_id(&b),
        "a relabelled button is a different category - UN would show the old label"
    );
    assert_ne!(wire::apple_category_id(&[]), wire::apple_category_id(&a));
}

#[test]
fn balloon_messages_become_events() {
    assert_eq!(
        wire::balloon_event("n", wire::NIN_BALLOONUSERCLICK),
        Some(NotificationEvent::activated(s("n")))
    );
    assert_eq!(
        wire::balloon_event("n", wire::NIN_BALLOONTIMEOUT).map(|e| e.kind),
        Some(NotificationEventType::Dismissed)
    );
    assert_eq!(
        wire::balloon_event("n", wire::NIN_BALLOONHIDE).map(|e| e.kind),
        Some(NotificationEventType::Dismissed)
    );
    assert_eq!(wire::balloon_event("n", wire::NIN_BALLOONSHOW), None);
    assert_eq!(wire::balloon_event("n", 0x0200), None, "a mouse move is no event");
}

#[test]
fn balloon_text_is_truncated_to_its_fixed_buffer_without_splitting_a_character() {
    let short = wire::utf16_truncated("hi", 8);
    assert_eq!(short, vec![u16::from(b'h'), u16::from(b'i'), 0]);

    let long = wire::utf16_truncated("abcdefgh", 4);
    assert_eq!(long.len(), 4, "capacity includes the terminating NUL");
    assert_eq!(long[3], 0);

    // U+1F600 is a surrogate pair; with room for one more unit it is dropped
    // whole rather than cut in half.
    let emoji = wire::utf16_truncated("a\u{1F600}", 3);
    assert_eq!(emoji, vec![u16::from(b'a'), 0]);
}

// ---------------------------------------------------------------------------
// The gaps closed on 2026-09-28 (scripts/NOTIFICATIONS_RESEARCH_2026_09_28.md
// section 7): an app-level handler for events no live callback owns, a
// payload that survives the process, a full queue that reports instead of
// dropping, deliveries that wait for a window, an explicit permission
// request, and the wire vocabulary of the new backends (UN authorization,
// WinRT toasts, the freedesktop desktop entry, Android intents).
// ---------------------------------------------------------------------------

mod gaps {
    use azul_core::notification::{NotificationCallback, OptionNotificationCallback};
    use azul_layout::managers::{
        notification::{
            app_notification_handler, drain_notification_deliveries, has_queued_deliveries,
            queue_notification_delivery, set_app_notification_handler,
            take_notification_permission_request, MAX_QUEUED_REQUESTS,
        },
        permission::{PermissionQuality, PermissionState},
    };

    use super::*;

    /// The app-level handler: the same callback + data pair a notification
    /// carries, installed once per process (`AppConfig::notification_handler`).
    fn app_handler(data: &RefAny) -> NotificationCallback {
        NotificationCallback {
            refany: data.clone(),
            callback: Callback::from_ptr(remember_event).into(),
        }
    }

    /// Installs the process-wide app-level handler for one test and removes
    /// it again when dropped - also when the test fails, so a red test cannot
    /// leave a handler behind for the next one.
    struct AppHandlerForThisTest;

    impl AppHandlerForThisTest {
        fn install(data: &RefAny) -> Self {
            set_app_notification_handler(OptionNotificationCallback::Some(app_handler(data)));
            Self
        }
    }

    impl Drop for AppHandlerForThisTest {
        fn drop(&mut self) {
            set_app_notification_handler(OptionNotificationCallback::None);
        }
    }

    fn run_callback(cb: extern "C" fn(RefAny, CallbackInfo) -> Update, data: &RefAny) {
        let mut lw = laid_out_window();
        let mut cb = Callback::from_ptr(cb);
        let mut data = data.clone();
        let state = lw.current_window_state.clone();
        let (_, update) = lw.invoke_single_callback(
            &mut cb,
            &mut data,
            &RawWindowHandle::Unsupported,
            &OptionGlContextPtr::None,
            Arc::new(SystemStyle::default()),
            &ExternalSystemCallbacks::rust_internal(),
            &None,
            &state,
            &RendererResources::default(),
        );
        assert_eq!(update, Update::DoNothing);
    }

    extern "C" fn post_the_overflow(data: RefAny, mut info: CallbackInfo) -> Update {
        info.post_notification(
            Notification::create(s("overflow"), s("One too many"))
                .with_payload(s("row-17"))
                .with_callback(data, Callback::from_ptr(remember_event)),
        );
        Update::DoNothing
    }

    extern "C" fn post_a_plain_overflow(_data: RefAny, mut info: CallbackInfo) -> Update {
        info.post_notification(Notification::create(s("overflow-plain"), s("One too many")));
        Update::DoNothing
    }

    extern "C" fn ask_for_the_permission(_data: RefAny, mut info: CallbackInfo) -> Update {
        info.request_notification_permission();
        Update::DoNothing
    }

    // ---- the model ----

    #[test]
    fn a_notification_carries_a_payload_and_an_event_starts_without_one() {
        let n = Notification::create(s("mail-7"), s("New mail")).with_payload(s("thread=42"));
        assert_eq!(n.payload.as_str(), "thread=42");
        assert_eq!(Notification::create(s("x"), s("t")).payload.as_str(), "");

        let clicked = NotificationEvent::activated(s("mail-7"));
        assert_eq!(clicked.payload.as_str(), "");
        assert!(!clicked.launched_app, "only a platform that knows sets it");
    }

    // ---- the app-level handler (G2) ----

    #[test]
    fn an_event_for_a_notification_this_process_never_posted_reaches_the_app_handler() {
        // A tap that cold-launched the app (iOS, Android, a macOS relaunch):
        // the notification's own callback died with the process that posted
        // it. Today `route` drops the event: 0 deliveries, expected 1.
        let handler = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.set_app_handler(OptionNotificationCallback::Some(app_handler(&handler)));

        let mut launch =
            NotificationEvent::action_invoked(s("posted-before-a-restart"), s("reply"));
        launch.payload = s("thread=42");
        launch.launched_app = true;
        let deliveries = registry.route(vec![launch.clone()]);
        assert_eq!(deliveries.len(), 1, "the app handler owns orphaned events");
        assert_eq!(deliveries[0].callback.refany, handler);
        assert_eq!(deliveries[0].event, launch, "payload and launched_app travel as-is");

        let mut lw = laid_out_window();
        for delivery in deliveries {
            assert_eq!(deliver(&mut lw, delivery), Update::DoNothing);
        }
        let (events, calls) = seen_by(&handler);
        assert_eq!(calls, 1);
        assert_eq!(events, vec![launch], "the handler reads the event like any delivery");
    }

    #[test]
    fn a_notification_without_its_own_callback_reports_to_the_app_handler() {
        let handler = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.set_app_handler(OptionNotificationCallback::Some(app_handler(&handler)));
        registry.admit(
            Notification::create(s("fire-and-forget"), s("Saved")).with_payload(s("doc-3")),
        );

        let deliveries = registry.route(vec![NotificationEvent::activated(s("fire-and-forget"))]);
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].callback.refany, handler);
        assert_eq!(deliveries[0].event.payload.as_str(), "doc-3");
    }

    #[test]
    fn a_close_after_a_click_does_not_reach_the_app_handler_either() {
        // The freedesktop close that follows a click names an id this process
        // DID post: it is the notification's echo, not an orphan.
        let own = RefAny::new(Seen::default());
        let handler = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.set_app_handler(OptionNotificationCallback::Some(app_handler(&handler)));
        registry.admit(notification_with_callback("n", &own));

        let deliveries = registry.route(vec![
            NotificationEvent::activated(s("n")),
            NotificationEvent::dismissed(s("n")),
        ]);
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].callback.refany, own);
        assert!(registry
            .route(vec![NotificationEvent::dismissed(s("n"))])
            .is_empty());
    }

    #[test]
    fn a_withdrawn_notification_does_not_reach_the_app_handler() {
        let handler = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.set_app_handler(OptionNotificationCallback::Some(app_handler(&handler)));
        registry.admit(Notification::create(s("n"), s("t")));
        assert!(registry.forget("n"));
        assert!(
            registry
                .route(vec![NotificationEvent::dismissed(s("n"))])
                .is_empty(),
            "the close that confirms a withdraw is nobody's event"
        );
    }

    #[test]
    fn a_repost_under_an_ended_id_reports_again() {
        let data = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.admit(notification_with_callback("progress", &data));
        assert_eq!(
            registry
                .route(vec![NotificationEvent::activated(s("progress"))])
                .len(),
            1
        );
        registry.admit(notification_with_callback("progress", &data));
        assert_eq!(
            registry
                .route(vec![NotificationEvent::activated(s("progress"))])
                .len(),
            1,
            "an ended id that is posted again is live again"
        );
    }

    #[test]
    fn the_app_handler_is_set_once_for_the_process() {
        let _serial = serial();
        let handler = RefAny::new(Seen::default());
        set_app_notification_handler(OptionNotificationCallback::Some(app_handler(&handler)));
        let read = app_notification_handler();
        assert_eq!(read.as_ref().map(|h| h.refany.clone()), Some(handler));
        set_app_notification_handler(OptionNotificationCallback::None);
        assert!(app_notification_handler().is_none());
    }

    // ---- the payload (G2, G8) ----

    #[test]
    fn the_payload_comes_back_in_the_event() {
        let data = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.admit(notification_with_callback("mail", &data).with_payload(s("msg-9")));
        let deliveries = registry.route(vec![NotificationEvent::activated(s("mail"))]);
        assert_eq!(deliveries.len(), 1);
        assert_eq!(
            deliveries[0].event.payload.as_str(),
            "msg-9",
            "freedesktop and the balloon report no payload; the registry fills it in"
        );
    }

    #[test]
    fn a_payload_the_platform_reports_wins() {
        let data = RefAny::new(Seen::default());
        let mut registry = NotificationRegistry::new();
        registry.admit(notification_with_callback("mail", &data).with_payload(s("msg-9")));
        let mut from_os = NotificationEvent::activated(s("mail"));
        from_os.payload = s("from-user-info");
        let deliveries = registry.route(vec![from_os]);
        assert_eq!(deliveries[0].event.payload.as_str(), "from-user-info");
    }

    // ---- a full queue reports (G3) ----

    #[test]
    fn a_post_to_a_full_queue_reports_failed_to_its_own_callback() {
        let _serial = serial();
        drop(drain_notification_requests());
        drop(drain_notification_deliveries());
        for i in 0..MAX_QUEUED_REQUESTS {
            assert!(push_notification_request(NotificationRequest::Post(
                Notification::create(AzString::from(format!("filler-{i}")), s("t"))
            )));
        }
        let data = RefAny::new(Seen::default());
        run_callback(post_the_overflow, &data);
        drop(drain_notification_requests());

        // Today `post_notification` discards the `false` and the post is
        // gone: 0 deliveries. Expected: one `Failed`, to its own callback.
        let deliveries = drain_notification_deliveries();
        assert_eq!(deliveries.len(), 1, "{deliveries:?}");
        assert_eq!(deliveries[0].event.kind, NotificationEventType::Failed);
        assert_eq!(deliveries[0].event.notification_id.as_str(), "overflow");
        assert_eq!(deliveries[0].event.payload.as_str(), "row-17");
        assert!(
            !deliveries[0].event.reason.as_str().is_empty(),
            "the reason says why"
        );
        assert_eq!(deliveries[0].callback.refany, data);
    }

    #[test]
    fn a_post_to_a_full_queue_without_a_callback_still_reports_failed() {
        let _serial = serial();
        drop(drain_notification_requests());
        drop(drain_notification_events());
        drop(drain_notification_deliveries());
        let handler = RefAny::new(Seen::default());
        let _handler = AppHandlerForThisTest::install(&handler);
        for i in 0..MAX_QUEUED_REQUESTS {
            assert!(push_notification_request(NotificationRequest::Post(
                Notification::create(AzString::from(format!("filler-{i}")), s("t"))
            )));
        }
        run_callback(post_a_plain_overflow, &RefAny::new(()));
        drop(drain_notification_requests());

        // Straight to the app handler, as a waiting delivery - NOT through
        // the mailbox, whose routing would take the id for whatever an
        // earlier post under it left (see `follow_ups`).
        let deliveries = drain_notification_deliveries();
        assert_eq!(deliveries.len(), 1, "{deliveries:?}");
        assert_eq!(deliveries[0].event.kind, NotificationEventType::Failed);
        assert_eq!(deliveries[0].event.notification_id.as_str(), "overflow-plain");
        assert_eq!(deliveries[0].callback.refany, handler);
        assert!(
            drain_notification_events().is_empty(),
            "nothing for the router to misread"
        );
    }

    // ---- deliveries wait for a window (G7) ----

    #[test]
    fn deliveries_wait_in_order_until_a_window_takes_them() {
        let _serial = serial();
        drop(drain_notification_deliveries());
        let data = RefAny::new(Seen::default());
        let first = NotificationDelivery {
            callback: app_handler(&data),
            event: NotificationEvent::activated(s("a")),
        };
        let second = NotificationDelivery {
            callback: app_handler(&data),
            event: NotificationEvent::dismissed(s("b")),
        };
        assert!(queue_notification_delivery(first.clone()));
        assert!(queue_notification_delivery(second.clone()));
        assert!(has_queued_deliveries());
        assert_eq!(drain_notification_deliveries(), vec![first.clone(), second]);
        assert!(!has_queued_deliveries());

        for _ in 0..MAX_QUEUED_EVENTS {
            assert!(queue_notification_delivery(first.clone()));
        }
        assert!(
            !queue_notification_delivery(first),
            "a windowless app must not grow the queue without bound"
        );
        assert_eq!(drain_notification_deliveries().len(), MAX_QUEUED_EVENTS);
    }

    // ---- the permission request (G3) ----

    #[test]
    fn a_permission_request_is_queued_for_the_platform() {
        let _serial = serial();
        drop(drain_notification_requests());
        let _ = take_notification_permission_request();
        assert!(!has_queued_requests(), "premise: nothing queued");

        run_callback(ask_for_the_permission, &RefAny::new(()));
        assert!(
            has_queued_requests(),
            "the capability pump arms its timer on this, so the prompt is not late"
        );
        assert!(take_notification_permission_request());
        assert!(
            !take_notification_permission_request(),
            "one request, one prompt"
        );
        assert!(!has_queued_requests());
    }

    // ---- wire: Apple ----

    #[test]
    fn apple_authorization_statuses_become_permission_states() {
        // UNAuthorizationStatus: notDetermined 0, denied 1, authorized 2,
        // provisional 3, ephemeral 4.
        assert_eq!(
            wire::apple_authorization_status(0),
            PermissionState::NotDetermined
        );
        assert_eq!(wire::apple_authorization_status(1), PermissionState::Denied);
        assert_eq!(
            wire::apple_authorization_status(2),
            PermissionState::Granted(PermissionQuality::Full)
        );
        assert_eq!(
            wire::apple_authorization_status(3),
            PermissionState::Granted(PermissionQuality::Reduced),
            "provisional = delivered quietly"
        );
        assert!(wire::apple_authorization_status(4).is_granted());
        assert_eq!(
            wire::apple_authorization_status(99),
            PermissionState::NotDetermined
        );
        assert_eq!(wire::APPLE_PAYLOAD_KEY, "azul.payload");
    }

    // ---- wire: Windows toasts ----

    #[test]
    fn toast_arguments_carry_id_action_and_payload_both_ways() {
        let args = wire::toast_arguments("mail 7&x", "reply", "a=b&c%d é");
        assert!(args.starts_with(wire::TOAST_ARGS_PREFIX));
        assert!(!args.contains(' '), "percent-encoded: {args}");
        assert_eq!(
            wire::parse_toast_arguments(&args),
            Some((
                "mail 7&x".to_string(),
                "reply".to_string(),
                "a=b&c%d é".to_string()
            ))
        );
        assert_eq!(
            wire::parse_toast_arguments("somebody-else's launch string"),
            None
        );

        let body = wire::toast_activated_event(&wire::toast_arguments("n", "default", "p"))
            .expect("our own arguments parse");
        assert_eq!(body.kind, NotificationEventType::Activated);
        assert_eq!(body.payload.as_str(), "p");
        let button = wire::toast_activated_event(&wire::toast_arguments("n", "reply", ""))
            .expect("our own arguments parse");
        assert_eq!(
            button,
            NotificationEvent::action_invoked(s("n"), s("reply"))
        );
    }

    #[test]
    fn toast_xml_escapes_text_and_lists_the_buttons() {
        let n = Notification::create(s("n"), s("Tom & <Jerry>"))
            .with_body(s("\"quoted\""))
            .with_action(s("reply"), s("Reply"))
            .with_action(s("default"), s("Shadowed"))
            .with_payload(s("p"));
        let xml = wire::toast_xml(&n);
        assert!(xml.starts_with("<toast "), "{xml}");
        assert!(xml.contains("Tom &amp; &lt;Jerry&gt;"), "{xml}");
        assert!(xml.contains("&quot;quoted&quot;"), "{xml}");
        assert!(xml.contains("template=\"ToastGeneric\""), "{xml}");
        assert_eq!(
            xml.matches("<action ").count(),
            1,
            "`default` is the body click: {xml}"
        );
        assert!(xml.contains("content=\"Reply\""), "{xml}");
        // The arguments are percent-encoded, so `&` is the only character of
        // theirs the XML attribute has to escape.
        let launch = wire::toast_arguments("n", "default", "p").replace('&', "&amp;");
        assert!(xml.contains(&format!("launch=\"{launch}\"")), "{xml}");

        let silent = wire::toast_xml(
            &Notification::create(s("q"), s("t")).with_sound(NotificationSound::Silent),
        );
        assert!(silent.contains("<audio silent=\"true\"/>"), "{silent}");
        assert!(
            !silent.contains("<actions>"),
            "no buttons, no actions element: {silent}"
        );
    }

    #[test]
    fn a_toast_that_moves_to_the_action_center_has_not_ended() {
        assert_eq!(
            wire::toast_dismissed_event("n", wire::TOAST_DISMISSED_USER_CANCELED)
                .map(|e| e.kind),
            Some(NotificationEventType::Dismissed)
        );
        assert_eq!(
            wire::toast_dismissed_event("n", wire::TOAST_DISMISSED_TIMED_OUT),
            None,
            "a timed-out toast still sits in the Action Center and can be clicked"
        );
        assert_eq!(
            wire::toast_dismissed_event("n", wire::TOAST_DISMISSED_APPLICATION_HIDDEN),
            None,
            "the app hid it: the withdraw already ended it"
        );
    }

    #[test]
    fn the_windows_aumid_is_safe_for_the_registry() {
        assert_eq!(wire::windows_aumid("rs.azul.widgets"), "rs.azul.widgets");
        let odd = wire::windows_aumid("My App\\v2 (beta)");
        assert!(
            !odd.contains('\\'),
            "a backslash breaks Windows 10 up to 19042: {odd}"
        );
        assert!(!odd.contains(' '), "{odd}");
        assert!(!odd.is_empty());
        let long = wire::windows_aumid(&"x".repeat(300));
        assert!(long.len() <= 129, "{} chars", long.len());
        assert_ne!(
            long,
            wire::windows_aumid(&"x".repeat(299)),
            "long ids stay distinct"
        );
        assert!(!wire::windows_aumid("").is_empty());
        assert_eq!(
            wire::aumid_registry_key("rs.azul.widgets"),
            "Software\\Classes\\AppUserModelId\\rs.azul.widgets"
        );
    }

    // ---- wire: freedesktop ----

    #[test]
    fn the_desktop_entry_is_the_executable_name_like_the_wayland_app_id() {
        assert_eq!(wire::desktop_entry("/usr/bin/az-widgets"), "az-widgets");
        assert_eq!(
            wire::desktop_entry("/opt/x/org.example.App.desktop"),
            "org.example.App"
        );
        assert_eq!(wire::desktop_entry(""), "azul");
    }

    // ---- wire: Android ----

    #[test]
    fn android_request_codes_are_distinct_per_notification_and_leave_room_for_buttons() {
        let a = wire::android_request_code("mail-1");
        let b = wire::android_request_code("mail-2");
        assert_ne!(
            a, b,
            "the same code would make the two share one PendingIntent"
        );
        assert_eq!(a, wire::android_request_code("mail-1"), "stable");
        for code in [a, b, wire::android_request_code("")] {
            assert!(code >= 0, "{code}");
            assert_eq!(
                code & 0xF,
                0,
                "the low nibble numbers the buttons and the dismissal"
            );
        }
    }

    #[test]
    fn android_pending_intents_are_immutable() {
        const FLAG_IMMUTABLE: i32 = 0x0400_0000;
        const FLAG_UPDATE_CURRENT: i32 = 0x0800_0000;
        for sdk in [23, 31, 34] {
            let flags = wire::android_pending_intent_flags(sdk);
            assert_eq!(flags & FLAG_IMMUTABLE, FLAG_IMMUTABLE, "sdk {sdk}");
            assert_eq!(flags & FLAG_UPDATE_CURRENT, FLAG_UPDATE_CURRENT, "sdk {sdk}");
        }
        assert_eq!(
            wire::android_pending_intent_flags(22) & FLAG_IMMUTABLE,
            0,
            "no such flag before API 23"
        );
    }

    #[test]
    fn android_intents_become_events() {
        let tap = wire::android_event("n", wire::ANDROID_DEFAULT_ACTION, "p", true);
        assert_eq!(tap.kind, NotificationEventType::Activated);
        assert_eq!(tap.payload.as_str(), "p");
        assert!(tap.launched_app);

        let gone = wire::android_event("n", wire::ANDROID_DISMISS_ACTION, "", false);
        assert_eq!(gone.kind, NotificationEventType::Dismissed);
        assert!(!gone.launched_app);

        let button = wire::android_event("n", "reply", "", false);
        assert_eq!(
            button,
            NotificationEvent::action_invoked(s("n"), s("reply"))
        );
    }

    // -----------------------------------------------------------------------
    // Follow-ups (2026-09-29): the ways the gaps round still lost a
    // notification without a word. A post the full queue rejected went back
    // through ROUTING, which takes its id for whatever an earlier post under
    // that id left - handing the failure to the notification on screen
    // (ending it) or swallowing it as the echo of an ended one. And a
    // withdraw that met a full queue vanished, so the notification stayed up
    // and its callback still fired.
    // -----------------------------------------------------------------------

    mod follow_ups {
        use azul_core::{
            callbacks::Update,
            notification::{Notification, NotificationEvent, NotificationEventType},
            refany::RefAny,
        };
        use azul_css::AzString;
        use azul_layout::{
            callbacks::CallbackInfo,
            managers::notification::{
                app_notification_handler, drain_notification_deliveries,
                drain_notification_events, drain_notification_requests,
                push_notification_request, NotificationDelivery, NotificationRegistry,
                NotificationRequest, MAX_QUEUED_REQUESTS,
            },
        };

        use super::super::{notification_with_callback, s, serial, Seen};
        use super::{post_a_plain_overflow, run_callback, AppHandlerForThisTest};

        fn fill_the_request_queue_with_posts() {
            drop(drain_notification_requests());
            for i in 0..MAX_QUEUED_REQUESTS {
                assert!(push_notification_request(NotificationRequest::Post(
                    Notification::create(AzString::from(format!("filler-{i}")), s("t"))
                )));
            }
        }

        /// What the dll's `pump_notifications` hands the loop: the deliveries
        /// that waited, then the mailbox routed through the registry.
        fn pump(registry: &mut NotificationRegistry) -> Vec<NotificationDelivery> {
            let mut out = drain_notification_deliveries();
            out.extend(registry.route(drain_notification_events()));
            out
        }

        extern "C" fn withdraw_the_one_on_screen(_data: RefAny, mut info: CallbackInfo) -> Update {
            info.withdraw_notification(s("on-screen"));
            Update::DoNothing
        }

        #[test]
        fn a_rejected_post_does_not_end_the_live_notification_under_its_id() {
            let _serial = serial();
            drop(drain_notification_events());
            drop(drain_notification_deliveries());
            let own = RefAny::new(Seen::default());
            let handler = RefAny::new(Seen::default());
            let _handler = AppHandlerForThisTest::install(&handler);
            let mut registry = NotificationRegistry::new();
            registry.set_app_handler(app_notification_handler());
            // On screen: posted earlier, with its own callback.
            registry.admit(notification_with_callback("overflow-plain", &own));

            // A repost under the same id, without a callback, that does not fit.
            fill_the_request_queue_with_posts();
            run_callback(post_a_plain_overflow, &RefAny::new(()));
            drop(drain_notification_requests());

            // Today the rejection travels through the mailbox, and routing
            // hands its `Failed` to the notification on screen and ends it.
            let deliveries = pump(&mut registry);
            assert!(
                registry.is_live("overflow-plain"),
                "the post that failed never reached the screen, so it replaced nothing"
            );
            assert_eq!(deliveries.len(), 1, "{deliveries:?}");
            assert_eq!(deliveries[0].event.kind, NotificationEventType::Failed);
            assert_eq!(
                deliveries[0].callback.refany, handler,
                "the failure belongs to the post that failed, which has no callback of its own"
            );
        }

        #[test]
        fn a_rejected_post_under_an_ended_id_still_reports_failed() {
            let _serial = serial();
            drop(drain_notification_events());
            drop(drain_notification_deliveries());
            let handler = RefAny::new(Seen::default());
            let _handler = AppHandlerForThisTest::install(&handler);
            let mut registry = NotificationRegistry::new();
            registry.set_app_handler(app_notification_handler());
            // Posted earlier, clicked: the id has ENDED in this process.
            registry.admit(Notification::create(s("overflow-plain"), s("Saved")));
            assert_eq!(
                registry
                    .route(vec![NotificationEvent::activated(s("overflow-plain"))])
                    .len(),
                1
            );

            fill_the_request_queue_with_posts();
            run_callback(post_a_plain_overflow, &RefAny::new(()));
            drop(drain_notification_requests());

            // Today 0: routing takes the `Failed` for the trailing close of
            // the notification that ended, and swallows it.
            let deliveries = pump(&mut registry);
            assert_eq!(deliveries.len(), 1, "{deliveries:?}");
            assert_eq!(deliveries[0].event.kind, NotificationEventType::Failed);
            assert_eq!(
                deliveries[0].event.notification_id.as_str(),
                "overflow-plain"
            );
            assert_eq!(deliveries[0].callback.refany, handler);
        }

        #[test]
        fn a_withdraw_still_reaches_the_platform_when_posts_filled_the_queue() {
            let _serial = serial();
            fill_the_request_queue_with_posts();
            run_callback(withdraw_the_one_on_screen, &RefAny::new(()));

            // Today the withdraw is dropped: MAX_QUEUED_REQUESTS requests, and
            // the notification stays on screen with its callback armed.
            let requests = drain_notification_requests();
            assert_eq!(requests.len(), MAX_QUEUED_REQUESTS + 1);
            assert_eq!(
                requests.last(),
                Some(&NotificationRequest::Withdraw(s("on-screen")))
            );
        }

        #[test]
        fn posts_cannot_take_the_room_withdraws_are_given_and_withdraws_are_bounded_too() {
            let _serial = serial();
            fill_the_request_queue_with_posts();
            assert!(
                !push_notification_request(NotificationRequest::Post(Notification::create(
                    s("one-more"),
                    s("t")
                ))),
                "posts stop at the bound (and report Failed)"
            );
            for i in 0..MAX_QUEUED_REQUESTS {
                assert!(
                    push_notification_request(NotificationRequest::Withdraw(AzString::from(
                        format!("w-{i}")
                    ))),
                    "withdraw {i} fits: one for every post the queue can hold"
                );
            }
            assert!(
                !push_notification_request(NotificationRequest::Withdraw(s("one-too-many"))),
                "a target that never drains must not grow the queue without bound"
            );
            drop(drain_notification_requests());
        }
    }
}

// ---------------------------------------------------------------------------
// Platform leftovers (N1, 2026-09-29): what the per-OS backends decide that a
// host can check - which response launched the app, the one app identity, the
// Windows toast activator's registration, the freedesktop bookkeeping behind
// an asynchronous `Notify`, and the Flatpak portal's notification.
// ---------------------------------------------------------------------------

mod platforms {
    use super::wire;

    // ---- which response launched the app ----

    #[test]
    fn the_tap_that_cold_launched_the_app_is_marked_where_the_os_names_no_notification() {
        // iOS (a local notification) and a Windows COM activation: the
        // process was started for a response nobody names. The first one is it.
        let mut marker = wire::LaunchResponseMarker::new();
        marker.expect_first();
        assert!(
            marker.launched_app("posted-by-an-earlier-run"),
            "the first response of a launch that expected one launched the app"
        );
        assert!(
            !marker.launched_app("another"),
            "a launch marks at most one response"
        );
    }

    #[test]
    fn a_tap_on_an_app_that_was_already_active_did_not_launch_it() {
        // iOS: `applicationDidBecomeActive` came first - the app was running.
        let mut marker = wire::LaunchResponseMarker::new();
        marker.expect_first();
        marker.launch_finished();
        assert!(!marker.launched_app("tapped-while-running"));
    }

    #[test]
    fn a_process_that_was_not_launched_for_a_notification_marks_nothing() {
        let mut marker = wire::LaunchResponseMarker::new();
        assert!(!marker.launched_app("n"));
        marker.launch_finished();
        assert!(!marker.launched_app("n"));
    }

    #[test]
    fn macos_marks_exactly_the_response_the_launch_named() {
        let mut marker = wire::LaunchResponseMarker::new();
        marker.name("clicked-in-notification-center".to_string());
        assert!(
            !marker.launched_app("some-other-notification"),
            "only the named response launched the app"
        );
        assert!(marker.launched_app("clicked-in-notification-center"));
        assert!(
            !marker.launched_app("clicked-in-notification-center"),
            "the same id clicked again later is a tap on a running app"
        );
    }

    #[test]
    fn a_named_launch_response_stays_marked_when_it_arrives_after_the_activation() {
        // macOS may run `applicationDidBecomeActive:` before UN delivers the
        // response the launch named.
        let mut marker = wire::LaunchResponseMarker::new();
        marker.name("n".to_string());
        marker.launch_finished();
        assert!(marker.launched_app("n"));
    }

    // ---- one app identity ----

    #[test]
    fn an_unnamed_app_is_known_by_one_id_derived_from_its_executable() {
        let app = wire::AppIdentity::from_executable("/usr/bin/AzWidgets");
        assert_eq!(app.source, wire::AppIdSource::Executable);
        assert_eq!(app.exe_name, "AzWidgets");
        assert_eq!(app.id, "com.azul.azwidgets");
        assert_eq!(
            app.apple_bundle_id(),
            "com.azul.azwidgets",
            "what `azul-doc bundle macos` writes as CFBundleIdentifier"
        );
        assert_eq!(
            app.windows_aumid(),
            "com.azul.azwidgets",
            "the toast AUMID is the same id, not a second derivation"
        );
        assert_eq!(
            app.desktop_entry(),
            "AzWidgets",
            "an unnamed app's .desktop file, Wayland app_id and WM_CLASS are its executable's name"
        );
        assert_eq!(app.display_name(), "AzWidgets");
    }

    #[test]
    fn a_windows_executable_is_named_without_its_directory_or_extension() {
        let app = wire::AppIdentity::from_executable("C:\\Program Files\\Az\\AzWidgets.exe");
        assert_eq!(app.exe_name, "AzWidgets");
        assert_eq!(app.id, "com.azul.azwidgets");
        assert_eq!(app.windows_aumid(), "com.azul.azwidgets");
        assert_eq!(app.display_name(), "AzWidgets");
        assert_eq!(
            wire::AppIdentity::from_executable("C:\\x\\TOOL.EXE").exe_name,
            "TOOL"
        );
    }

    #[test]
    fn a_derived_id_uses_only_lowercase_letters_digits_and_dashes() {
        assert_eq!(
            wire::AppIdentity::from_executable("my_app 2").id,
            "com.azul.my-app-2"
        );
        assert_eq!(
            wire::AppIdentity::from_executable("/opt/azul-paint").id,
            "com.azul.azul-paint"
        );
        assert_eq!(wire::AppIdentity::from_executable("__").id, "com.azul.app");
        let unknown = wire::AppIdentity::from_executable("");
        assert_eq!(unknown.id, "com.azul.app");
        assert_eq!(unknown.desktop_entry(), "azul");
        assert_eq!(unknown.display_name(), "Azul");
    }

    #[test]
    fn a_declared_id_is_the_desktop_entry_so_server_compositor_and_portal_agree() {
        // A Flatpak: `FLATPAK_ID` names the app, its .desktop file is
        // `<id>.desktop`, and the portal attributes notifications to that id.
        let app = wire::AppIdentity::declared("org.example.Widgets", "/app/bin/widgets");
        assert_eq!(app.source, wire::AppIdSource::Declared);
        assert_eq!(app.id, "org.example.Widgets");
        assert_eq!(app.desktop_entry(), "org.example.Widgets");
        assert_eq!(app.windows_aumid(), "org.example.Widgets");
        assert_eq!(app.apple_bundle_id(), "org.example.Widgets");
        assert_eq!(app.display_name(), "widgets");
    }

    #[test]
    fn each_platform_gets_only_the_characters_it_accepts() {
        let app = wire::AppIdentity::declared("org.example.my_app", "/app/bin/my_app");
        assert_eq!(
            app.apple_bundle_id(),
            "org.example.my-app",
            "a CFBundleIdentifier has no underscore"
        );
        assert_eq!(app.windows_aumid(), "org.example.my_app");
        let blank = wire::AppIdentity::declared("", "/usr/bin/Tool");
        assert_eq!(
            blank.id, "com.azul.tool",
            "an empty declaration is no declaration"
        );
        assert_eq!(blank.source, wire::AppIdSource::Executable);
    }
}
