//! App lifecycle events: the platform-independent half.
//!
//! The data model is `azul_core::lifecycle`; the shells raise the events (`azul-dll`, `desktop/lifecycle`) from their
//! own handlers - Android's activity callbacks, iOS's application delegate - which may run on a thread of the
//! system's choosing. This is the tray's shape: a [mailbox](LifecycleEvents::queue_lifecycle_event) the event is
//! queued in, the [handler](LifecycleEvents::set_lifecycle_handler) the app installed (one for the app, not one per
//! window), and the [current event](LifecycleEvents::with_current_lifecycle_event) a delivery runs for, which
//! `CallbackInfo::get_lifecycle_event` reads.
//!
//! An app without a handler hears nothing: the events are dropped when they are [routed](route_lifecycle_events), as
//! the tray's are, so that the mailbox cannot fill up behind an app that did not ask.

use alloc::{sync::Arc, vec::Vec};
use std::sync::{Mutex, PoisonError};

use azul_core::{
    lifecycle::LifecycleEvent,
    menu::{CoreMenuCallback, OptionCoreMenuCallback},
};

use super::shared_queue::SharedQueue;

/// Bound on the mailbox: a phone raises a handful of lifecycle events a minute, so a full mailbox means the app's
/// loop does not run (a paused process): the oldest stay, the newest are dropped.
pub const MAX_QUEUED_LIFECYCLE_EVENTS: usize = 32;

/// The lifecycle events of one app: the mailbox the shells queue them in, the slot of the event whose delivery runs
/// right now, and the handler. The windows of the app share one set (`AppServices::lifecycle`); a clone is another
/// handle on the SAME mailbox, slot and handler.
#[derive(Debug, Clone)]
pub struct LifecycleEvents {
    pending: SharedQueue<LifecycleEvent>,
    current: Arc<Mutex<Option<LifecycleEvent>>>,
    handler: Arc<Mutex<OptionCoreMenuCallback>>,
}

impl Default for LifecycleEvents {
    fn default() -> Self {
        Self {
            pending: SharedQueue::default(),
            current: Arc::new(Mutex::new(None)),
            handler: Arc::new(Mutex::new(OptionCoreMenuCallback::None)),
        }
    }
}

// A queue operation hands items over or says whether the item was taken: a caller that only wants the queue emptied,
// or the event parked, rightly ignores the answer.
#[allow(clippy::must_use_candidate)]
impl LifecycleEvents {
    /// A new mailbox, slot and handler of their own, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue an event from a platform callback, on any thread. Never blocks for long and never panics (it runs inside
    /// a JNI call or an Objective-C delegate method, where unwinding would abort the process). Returns `false` when
    /// the mailbox is full and the event was dropped.
    pub fn queue_lifecycle_event(&self, event: LifecycleEvent) -> bool {
        self.pending
            .push_up_to(event, MAX_QUEUED_LIFECYCLE_EVENTS)
            .is_ok()
    }

    /// Take every queued event, in arrival order.
    pub fn drain_lifecycle_events(&self) -> Vec<LifecycleEvent> {
        self.pending.drain()
    }

    /// Is an event waiting?
    #[must_use]
    pub fn has_queued_lifecycle_events(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Install the app's lifecycle handler (`CallbackInfo::set_lifecycle_handler`), replacing any earlier one. It
    /// runs with the data the app gave it and reads the event with `CallbackInfo::get_lifecycle_event`.
    pub fn set_lifecycle_handler(&self, handler: CoreMenuCallback) {
        *self
            .handler
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = OptionCoreMenuCallback::Some(handler);
    }

    /// The installed handler, if the app set one.
    #[must_use]
    pub fn lifecycle_handler(&self) -> OptionCoreMenuCallback {
        self.handler
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Run `f` with `event` as the event `CallbackInfo::get_lifecycle_event` reports. The previous value is restored
    /// afterwards, also on unwind.
    #[allow(clippy::must_use_candidate)] // returns what `f` returns: a caller may want only its effect
    pub fn with_current_lifecycle_event<R>(&self, event: &LifecycleEvent, f: impl FnOnce() -> R) -> R {
        struct Restore<'a>(&'a Mutex<Option<LifecycleEvent>>, Option<LifecycleEvent>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                *self.0.lock().unwrap_or_else(PoisonError::into_inner) = self.1.take();
            }
        }
        let previous = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(*event);
        let _restore = Restore(&self.current, previous);
        f()
    }

    /// The lifecycle event the running callback was delivered for, or `None` outside a lifecycle delivery.
    #[must_use]
    pub fn current_lifecycle_event(&self) -> Option<LifecycleEvent> {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One lifecycle event and the handler it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleDelivery {
    pub callback: CoreMenuCallback,
    pub event: LifecycleEvent,
}

/// Route events to the app's handler, one delivery per event, in order. An app without a handler gets nothing: the
/// events are dropped.
#[must_use]
pub fn route_lifecycle_events(
    events: Vec<LifecycleEvent>,
    handler: &OptionCoreMenuCallback,
) -> Vec<LifecycleDelivery> {
    let Some(handler) = handler.as_ref() else {
        return Vec::new();
    };
    events
        .into_iter()
        .map(|event| LifecycleDelivery {
            callback: handler.clone(),
            event,
        })
        .collect()
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;
