//! The lifecycle mailbox, the current-event slot and the routing to the app's handler. No window.

use azul_core::{
    callbacks::CoreCallback,
    lifecycle::LifecycleEvent,
    menu::{CoreMenuCallback, OptionCoreMenuCallback},
    refany::{OptionRefAny, RefAny},
};

use super::{route_lifecycle_events, LifecycleEvents, MAX_QUEUED_LIFECYCLE_EVENTS};

fn a_handler() -> CoreMenuCallback {
    CoreMenuCallback {
        refany: RefAny::new(7_u32),
        callback: CoreCallback { cb: 1, ctx: OptionRefAny::None },
    }
}

#[test]
fn events_come_out_in_the_order_they_went_in_and_only_once() {
    let events = LifecycleEvents::new();
    assert!(!events.has_queued_lifecycle_events());
    assert!(events.queue_lifecycle_event(LifecycleEvent::Paused));
    assert!(events.queue_lifecycle_event(LifecycleEvent::LowMemory));
    assert!(events.queue_lifecycle_event(LifecycleEvent::Resumed));
    assert!(events.has_queued_lifecycle_events());
    assert_eq!(
        events.drain_lifecycle_events(),
        vec![LifecycleEvent::Paused, LifecycleEvent::LowMemory, LifecycleEvent::Resumed]
    );
    assert!(events.drain_lifecycle_events().is_empty());
    assert!(!events.has_queued_lifecycle_events());
}

#[test]
fn a_full_mailbox_drops_the_newest_and_keeps_the_oldest() {
    let events = LifecycleEvents::new();
    for _ in 0..MAX_QUEUED_LIFECYCLE_EVENTS {
        assert!(events.queue_lifecycle_event(LifecycleEvent::LowMemory));
    }
    assert!(!events.queue_lifecycle_event(LifecycleEvent::WillTerminate), "the bound is the bound");
    let drained = events.drain_lifecycle_events();
    assert_eq!(drained.len(), MAX_QUEUED_LIFECYCLE_EVENTS);
    assert!(drained.iter().all(|e| *e == LifecycleEvent::LowMemory));
}

#[test]
fn a_clone_is_another_handle_on_the_same_mailbox_and_handler() {
    let events = LifecycleEvents::new();
    let other = events.clone();
    assert!(other.queue_lifecycle_event(LifecycleEvent::Paused));
    assert_eq!(events.drain_lifecycle_events(), vec![LifecycleEvent::Paused]);
    assert!(events.lifecycle_handler().as_ref().is_none());
    other.set_lifecycle_handler(a_handler());
    assert!(events.lifecycle_handler().as_ref().is_some(), "the handler of the app, not of a window");
}

#[test]
fn the_current_event_is_the_one_being_delivered_and_is_restored_after() {
    let events = LifecycleEvents::new();
    assert_eq!(events.current_lifecycle_event(), None);
    let seen = events.with_current_lifecycle_event(&LifecycleEvent::Paused, || {
        let outer = events.current_lifecycle_event();
        // a delivery that raises another one inside (a handler that makes the app pause itself)
        let inner = events.with_current_lifecycle_event(&LifecycleEvent::Resumed, || events.current_lifecycle_event());
        (outer, inner, events.current_lifecycle_event())
    });
    assert_eq!(
        seen,
        (Some(LifecycleEvent::Paused), Some(LifecycleEvent::Resumed), Some(LifecycleEvent::Paused))
    );
    assert_eq!(events.current_lifecycle_event(), None, "nothing is current outside a delivery");
}

#[test]
fn the_current_event_is_restored_when_the_handler_panics() {
    let events = LifecycleEvents::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        events.with_current_lifecycle_event(&LifecycleEvent::Paused, || panic!("the handler failed"))
    }));
    assert!(result.is_err());
    assert_eq!(events.current_lifecycle_event(), None);
}

#[test]
fn every_event_goes_to_the_handler_in_order_and_an_app_without_one_hears_nothing() {
    let handler = OptionCoreMenuCallback::Some(a_handler());
    let routed = route_lifecycle_events(
        vec![LifecycleEvent::Paused, LifecycleEvent::WillTerminate],
        &handler,
    );
    assert_eq!(routed.len(), 2);
    assert_eq!(routed[0].event, LifecycleEvent::Paused);
    assert_eq!(routed[1].event, LifecycleEvent::WillTerminate);
    assert_eq!(routed[0].callback, a_handler_of(&handler));

    let none = OptionCoreMenuCallback::None;
    assert!(route_lifecycle_events(vec![LifecycleEvent::Paused], &none).is_empty());
}

fn a_handler_of(handler: &OptionCoreMenuCallback) -> CoreMenuCallback {
    handler.as_ref().expect("a handler").clone()
}
