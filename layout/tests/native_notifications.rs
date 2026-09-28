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
