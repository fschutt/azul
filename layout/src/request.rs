//! Runtime side of the request / resume primitive.
//!
//! Every OS-facing operation the browser can only answer asynchronously is
//! split in two on every target:
//!
//! * **request** - `FileDialog::open_file(.., data, on_result) -> RequestId`
//!   registers `{RequestId, data, on_result}` here and returns the id.
//! * **resume** - once the result exists the *runtime* invokes
//!   `on_result(data, CallbackInfo, result)` as a fresh, ordinary callback
//!   activation. A resume may issue the next request, so a chain of awaits
//!   becomes a chain of resumes and the app's callbacks form a state machine
//!   over its own `RefAny`.
//!
//! This module is the one place where requests are parked. On desktop the
//! request function usually performs the (modal, blocking) OS call itself and
//! calls [`complete`] straight away; the completion is still queued rather
//! than invoked, which is what gives every target the same ordering
//! guarantee: *the result callback never runs re-entrantly inside the
//! requesting activation.* Mobile pickers, whose OS delegates answer later,
//! use [`defer`] with a poll closure the pump asks once per frame.
//!
//! The pump - [`take_completed`] - is driven by the shells after every
//! activation and from the per-frame timer/thread tick, and by the E2E runner
//! in the same places. `LayoutWindow::run_completed_requests` turns the drained
//! entries into callback invocations.
//!
//! ## Why a process-wide queue and not `CallbackInfo::push_change`
//!
//! The request functions are ordinary static API functions - they take no
//! `CallbackInfo`, because a `RequestId`-returning signature has to be
//! callable from anywhere (a timer, a thread writeback, another resume). The
//! queue is therefore owned by the runtime, not by an activation. It is the
//! single choke point a web host replaces: a lifted build classifies
//! [`complete_erased`] / [`defer`] / [`take_completed`] as boundary imports
//! and services them from JavaScript, and nothing else changes.
//!
//! ## Delivery order and ownership
//!
//! * FIFO. Nested completions (a resume that issues and immediately
//!   completes another request) are appended to the end, never recursed
//!   into.
//! * Entries are delivered by whichever window pumps next. Synchronous
//!   completions (every desktop dialog / file / http call) are pumped by the
//!   requesting window right after the requesting activation returns, so in
//!   practice a request resumes on the window that issued it.
//! * The queue holds a clone of the app's `data` until delivery, keeping the
//!   `RefAny` alive across the gap. There is no cancellation in v1.

use alloc::{boxed::Box, vec::Vec};

use azul_core::{refany::RefAny, task::RequestId};

use crate::callbacks::ResumeCallback;

/// A request whose result exists and whose callback has not run yet.
pub struct CompletedRequest {
    pub request_id: RequestId,
    /// The app's context, exactly as submitted to the request function.
    pub data: RefAny,
    pub callback: ResumeCallback,
    /// The per-operation result struct, type-erased.
    pub result: RefAny,
}

impl core::fmt::Debug for CompletedRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompletedRequest")
            .field("request_id", &self.request_id)
            .field("callback", &self.callback)
            .finish_non_exhaustive()
    }
}

/// Asked by the pump once per frame; returns the type-erased result struct
/// once the platform has answered, `None` while the request is still open.
pub type PollFn = Box<dyn FnMut() -> Option<RefAny> + Send>;

/// A request the platform answers asynchronously (a mobile file picker whose
/// delegate fires later).
pub struct PendingRequest {
    pub request_id: RequestId,
    pub data: RefAny,
    pub callback: ResumeCallback,
    pub poll: PollFn,
}

impl core::fmt::Debug for PendingRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingRequest")
            .field("request_id", &self.request_id)
            .field("callback", &self.callback)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
mod queue {
    use std::sync::Mutex;

    use super::{CompletedRequest, PendingRequest};

    pub(super) struct RequestQueue {
        pub completed: Vec<CompletedRequest>,
        pub pending: Vec<PendingRequest>,
    }

    pub(super) static REQUEST_QUEUE: Mutex<RequestQueue> = Mutex::new(RequestQueue {
        completed: Vec::new(),
        pending: Vec::new(),
    });

    pub(super) fn with_queue<R>(f: impl FnOnce(&mut RequestQueue) -> R) -> R {
        // A poisoned lock only means a callback panicked while the queue was
        // held; the queue itself is still a plain Vec pair, so keep serving.
        let mut guard = REQUEST_QUEUE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }
}

/// Registers a request whose result is already known and returns its id.
///
/// This is what every desktop request function calls after its synchronous
/// OS call: the callback runs on the next pump, never inside the caller.
pub fn complete<T: 'static>(data: RefAny, on_result: ResumeCallback, result: T) -> RequestId {
    complete_erased(data, on_result, RefAny::new(result))
}

/// [`complete`] with an already type-erased result struct.
pub fn complete_erased(data: RefAny, on_result: ResumeCallback, result: RefAny) -> RequestId {
    let request_id = RequestId::unique();
    #[cfg(feature = "std")]
    queue::with_queue(|q| {
        q.completed.push(CompletedRequest {
            request_id,
            data,
            callback: on_result,
            result,
        });
    });
    #[cfg(not(feature = "std"))]
    {
        // Without std there is no runtime pump; the request is acknowledged
        // but never resumed. Every request function documents that no_std
        // builds have no OS-facing implementation.
        drop((data, on_result, result));
    }
    request_id
}

/// Registers a request the platform will answer later. `poll` is asked once
/// per frame until it returns `Some(result)`.
pub fn defer(data: RefAny, on_result: ResumeCallback, poll: PollFn) -> RequestId {
    let request_id = RequestId::unique();
    #[cfg(feature = "std")]
    queue::with_queue(|q| {
        q.pending.push(PendingRequest {
            request_id,
            data,
            callback: on_result,
            poll,
        });
    });
    #[cfg(not(feature = "std"))]
    drop((data, on_result, poll));
    request_id
}

/// Drains every completed request, polling the pending ones first. Returns
/// them in the order they completed; an empty vector means nothing to do.
///
/// Called by the shells' pump. Resumes issued *during* delivery land in the
/// queue again and are picked up by the next call, which is how nested
/// completions stay FIFO instead of recursing.
#[must_use]
pub fn take_completed() -> Vec<CompletedRequest> {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| {
            let mut still_pending = Vec::with_capacity(q.pending.len());
            for mut p in q.pending.drain(..) {
                match (p.poll)() {
                    Some(result) => q.completed.push(CompletedRequest {
                        request_id: p.request_id,
                        data: p.data,
                        callback: p.callback,
                        result,
                    }),
                    None => still_pending.push(p),
                }
            }
            q.pending = still_pending;
            core::mem::take(&mut q.completed)
        })
    }
    #[cfg(not(feature = "std"))]
    {
        Vec::new()
    }
}

/// `true` while any request is completed-but-undelivered or still pending.
/// Shells use it to keep their frame loop ticking until the queue is empty.
#[must_use]
pub fn has_work() -> bool {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| !q.completed.is_empty() || !q.pending.is_empty())
    }
    #[cfg(not(feature = "std"))]
    {
        false
    }
}

/// Number of requests waiting on a platform answer - a debug probe for tests
/// that assert the queue does not grow without bound.
#[must_use]
pub fn pending_count() -> usize {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| q.pending.len())
    }
    #[cfg(not(feature = "std"))]
    {
        0
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use azul_core::callbacks::Update;

    use super::*;
    use crate::callbacks::CallbackInfo;

    extern "C" fn noop(_: RefAny, _: CallbackInfo, _: RefAny) -> Update {
        Update::DoNothing
    }

    #[test]
    fn ids_are_unique_and_valid() {
        let a = RequestId::unique();
        let b = RequestId::unique();
        assert!(a.is_valid() && b.is_valid());
        assert_ne!(a, b);
        assert!(!RequestId::invalid().is_valid());
    }

    #[test]
    fn complete_is_delivered_fifo_and_defer_waits_for_its_poll() {
        // Other tests share the process-wide queue; drain whatever they left.
        let _ = take_completed();

        let cb = ResumeCallback::create(noop);
        let first = complete(RefAny::new(1u8), cb.clone(), 10u32);
        let second = complete(RefAny::new(2u8), cb.clone(), 20u32);

        static POLLS: AtomicUsize = AtomicUsize::new(0);
        let deferred = defer(
            RefAny::new(3u8),
            cb,
            Box::new(|| {
                if POLLS.fetch_add(1, Ordering::SeqCst) == 0 {
                    None
                } else {
                    Some(RefAny::new(30u32))
                }
            }),
        );
        assert!(has_work());
        assert_eq!(pending_count(), 1);

        let batch = take_completed();
        let ids: Vec<RequestId> = batch.iter().map(|c| c.request_id).collect();
        assert_eq!(ids, vec![first, second]);
        assert_eq!(pending_count(), 1, "poll answered None on the first pump");

        let batch = take_completed();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].request_id, deferred);
        let mut result = batch.into_iter().next().unwrap().result;
        assert_eq!(result.downcast_ref::<u32>().map(|r| *r), Some(30));
        assert_eq!(pending_count(), 0);
        assert!(!has_work());
    }
}
