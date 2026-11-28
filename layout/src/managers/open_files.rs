//! What the system asks the app to open: the platform-independent half.
//!
//! The data model is `azul_core::open_files`; the shells raise the requests (`azul-dll`, `desktop/extra/open_files`) from their own
//! entry points - Android's `onCreate` / `onNewIntent`, iOS's `application:openURL:options:` - on a thread of the system's
//! choosing, after a worker made the working copies. This is the lifecycle's shape (a [mailbox](OpenQueue::push), the
//! [handler](OpenQueue::set_handler) the app installed, the [current request](OpenQueue::with_current) a delivery runs
//! for, which `CallbackInfo::get_open_request` reads) with ONE difference: an app that has no handler YET keeps what it was
//! sent. The intent that starts an app arrives before the app's first callback has had the chance to install a handler, and
//! dropping it would lose the file the person tapped; so requests are held (at most [`MAX_HELD_REQUESTS`], the oldest dropped
//! and counted) until a handler is set.

use alloc::{sync::Arc, vec::Vec};
use std::{
    collections::VecDeque,
    sync::{Mutex, PoisonError},
};

use azul_core::{
    menu::{CoreMenuCallback, OptionCoreMenuCallback},
    open_files::OpenRequest,
};

/// How many requests are kept for an app that has not installed its handler: a person opens a handful of files; more means the
/// app never listens, and the oldest go (counted, see [`OpenQueue::dropped`]).
pub const MAX_HELD_REQUESTS: usize = 16;

/// The most items a request carries: a share of more is cut, and the app is told on the last item.
pub const MAX_ITEMS_PER_REQUEST: usize = azul_core::open_files::MAX_ITEMS;

#[derive(Debug)]
struct Inner {
    held: VecDeque<OpenRequest>,
    handler: OptionCoreMenuCallback,
    current: Option<OpenRequest>,
    dropped: usize,
    was_active: bool,
}

/// The open requests of one app: the held ones, the app's handler and the request whose delivery runs right now. The windows of
/// the app share one (`AppServices::open`); a clone is another handle on the SAME queue, handler and slot.
#[derive(Debug, Clone)]
pub struct OpenQueue {
    inner: Arc<Mutex<Inner>>,
}

impl Default for OpenQueue {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                held: VecDeque::new(),
                handler: OptionCoreMenuCallback::None,
                current: None,
                dropped: 0,
                was_active: false,
            })),
        }
    }
}

/// One request and the handler it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenDelivery {
    /// The app's handler.
    pub callback: CoreMenuCallback,
    /// What it is asked to open.
    pub request: OpenRequest,
}

impl OpenQueue {
    /// A new queue, handler and slot of their own, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queue a request from a platform thread. Never blocks for long and never panics (it runs inside a JNI call or an Objective-C
    /// delegate method, where unwinding would abort the process). A request with no item is not a request: `false`. Beyond
    /// [`MAX_HELD_REQUESTS`] the oldest is dropped (counted in [`OpenQueue::dropped`]).
    pub fn push(&self, request: OpenRequest) -> bool {
        if request.items.as_ref().is_empty() {
            return false;
        }
        let mut inner = self.lock();
        while inner.held.len() >= MAX_HELD_REQUESTS {
            inner.held.pop_front();
            inner.dropped += 1;
        }
        inner.held.push_back(request);
        true
    }

    /// The app became active (iOS `applicationDidBecomeActive:`): a request that comes before it is the one that STARTED the app.
    pub fn note_active(&self) {
        self.lock().was_active = true;
    }

    /// Whether the app has been active yet; a request made while it was not is `launched_app`.
    #[must_use]
    pub fn was_active(&self) -> bool {
        self.lock().was_active
    }

    /// How many requests were dropped because the app never installed a handler (diagnostics).
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.lock().dropped
    }

    /// How many requests wait for a handler or a window.
    #[must_use]
    pub fn held(&self) -> usize {
        self.lock().held.len()
    }

    /// Install the app's open handler (`CallbackInfo::set_open_handler`), replacing any earlier one. It runs with the data the
    /// app gave it and reads the request with `CallbackInfo::get_open_request`.
    pub fn set_handler(&self, handler: CoreMenuCallback) {
        self.lock().handler = OptionCoreMenuCallback::Some(handler);
    }

    /// The installed handler, if the app set one.
    #[must_use]
    pub fn handler(&self) -> OptionCoreMenuCallback {
        self.lock().handler.clone()
    }

    /// Take every held request as a delivery, in arrival order - when a handler is installed. Without one the requests stay.
    #[must_use]
    pub fn take_deliveries(&self) -> Vec<OpenDelivery> {
        let mut inner = self.lock();
        let Some(handler) = inner.handler.as_ref().cloned() else {
            return Vec::new();
        };
        inner
            .held
            .drain(..)
            .map(|request| OpenDelivery {
                callback: handler.clone(),
                request,
            })
            .collect()
    }

    /// Give back deliveries that had no window to run against (the first request arrives before the first window): they go to
    /// the FRONT, in their order, so nothing overtakes them.
    pub fn defer(&self, deliveries: Vec<OpenDelivery>) {
        let mut inner = self.lock();
        for delivery in deliveries.into_iter().rev() {
            if inner.held.len() >= MAX_HELD_REQUESTS {
                // the newest of the deferred ones would overflow: the OLDEST held request makes room at the back
                inner.held.pop_back();
                inner.dropped += 1;
            }
            inner.held.push_front(delivery.request);
        }
    }

    /// Run `f` with `request` as the request `CallbackInfo::get_open_request` reports. The previous value is restored afterwards,
    /// also on unwind.
    #[allow(clippy::must_use_candidate)] // returns what `f` returns: a caller may want only its effect
    pub fn with_current<R>(&self, request: &OpenRequest, f: impl FnOnce() -> R) -> R {
        struct Restore<'a>(&'a Mutex<Inner>, Option<OpenRequest>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.lock().unwrap_or_else(PoisonError::into_inner).current = self.1.take();
            }
        }
        let previous = self.lock().current.replace(request.clone());
        let _restore = Restore(&self.inner, previous);
        f()
    }

    /// The request the running callback was delivered for, or `None` outside an open delivery.
    #[must_use]
    pub fn current(&self) -> Option<OpenRequest> {
        self.lock().current.clone()
    }
}

#[cfg(test)]
#[path = "open_files_test.rs"]
mod open_files_test;
