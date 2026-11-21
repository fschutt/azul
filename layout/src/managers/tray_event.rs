//! System-tray events: the platform-independent half.
//!
//! The data model is `azul_core::tray`; the OS plumbing is `azul-dll`'s
//! `desktop/tray/`. A tray reports two kinds of thing:
//!
//! * a pick of a menu item that carries its OWN callback - the dll hands that callback straight to
//!   the run loop, like a window menu item;
//! * everything else - a click on the icon (`Activate`), a middle click, a scroll, a context-menu
//!   request, a menu item WITHOUT a callback. The OS reports those on whatever thread it likes (a
//!   D-Bus dispatch, an AppKit action), which cannot hold a `CallbackInfo`, so they are queued
//!   into the [mailbox](queue_tray_event) here.
//!
//! Nothing used to drain that mailbox: a plain click on a tray icon never
//! reached the app. Now the dll's app-event collector drains it every loop
//! iteration and [routes](route_tray_events) each event to the tray's own
//! callback (`TrayIconData::callback`), which runs through
//! `invoke_menu_callback` - the notification's shape - with the event
//! installed as the [current event](with_current_tray_event) for
//! `CallbackInfo::get_tray_event`.

use alloc::vec::Vec;
use core::cell::RefCell;

use azul_core::{
    menu::{CoreMenuCallback, OptionCoreMenuCallback},
    tray::TrayEvent,
};

/// Bound on the mailbox: an app whose loop never drains it (or a panel that
/// floods scroll events while a callback blocks) must not grow it without
/// limit. The notification mailbox uses the same bound.
pub const MAX_QUEUED_TRAY_EVENTS: usize = 256;

static PENDING: std::sync::Mutex<Vec<TrayEvent>> = std::sync::Mutex::new(Vec::new());

/// Queue an event from a platform callback, on any thread. Never blocks for
/// long and never panics - it runs inside an Objective-C action or a D-Bus
/// handler, where unwinding would abort the process. Returns `false` when
/// the mailbox is full and the event was dropped.
pub fn queue_tray_event(event: TrayEvent) -> bool {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_QUEUED_TRAY_EVENTS {
        return false;
    }
    q.push(event);
    true
}

/// Take every queued event, in arrival order.
pub fn drain_tray_events() -> Vec<TrayEvent> {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Is an event waiting? The Linux loops do not park while one is (it was
/// queued on the loop thread itself, by a D-Bus handler, after this
/// iteration's collector ran).
pub fn has_queued_tray_events() -> bool {
    PENDING
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

/// One tray event and the callback it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayDelivery {
    pub callback: CoreMenuCallback,
    pub event: TrayEvent,
}

/// Route events to the tray's callback, one delivery per event, in order. A
/// tray without a callback delivers nothing: the events are dropped, as they
/// were before a callback could be set.
#[must_use]
pub fn route_tray_events(
    events: Vec<TrayEvent>,
    callback: &OptionCoreMenuCallback,
) -> Vec<TrayDelivery> {
    let Some(callback) = callback.as_ref() else {
        return Vec::new();
    };
    events
        .into_iter()
        .map(|event| TrayDelivery {
            callback: callback.clone(),
            event,
        })
        .collect()
}

std::thread_local! {
    /// Set for the duration of one delivery, on the event-loop thread.
    static CURRENT_EVENT: RefCell<Option<TrayEvent>> = const { RefCell::new(None) };
}

/// Run `f` with `event` as the event `CallbackInfo::get_tray_event` reports.
/// The previous value is restored afterwards, also on unwind.
pub fn with_current_tray_event<R>(event: &TrayEvent, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<TrayEvent>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            let _ = CURRENT_EVENT.try_with(|slot| slot.replace(previous));
        }
    }
    let previous = CURRENT_EVENT.with(|slot| slot.replace(Some(*event)));
    let _restore = Restore(previous);
    f()
}

/// The tray event the running callback was delivered for, or `None` outside
/// a tray delivery.
#[must_use]
pub fn current_tray_event() -> Option<TrayEvent> {
    CURRENT_EVENT
        .try_with(|slot| *slot.borrow())
        .ok()
        .flatten()
}
