//! Routing off the UI thread (ROUTING.md, "How the travel panel calls it"): every route the
//! travel panel asks for is worked out on an azul `Thread`, and only the reply comes back to
//! the UI thread.
//!
//! A newer request supersedes an older one. The app and its workers share the number of the
//! newest request (`latest`): a worker whose request is no longer the newest gives up at its
//! next check, and a reply that arrives anyway is dropped by its number
//! (`crate::on_route_done`). Nothing waits for a worker on the UI thread - removing a running
//! `Thread` would (its destructor waits for the worker to acknowledge).
//!
//! The worker answers ROUTING.md's first step today, the straight-line estimate
//! ([`crate::model::estimate_route`]). The routing tiles along that line (step 2) and A* over
//! them (steps 3 and 4) go HERE, behind the same request and reply, checking [`Stop`] between
//! their steps.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use azul::{
    callbacks::WriteBackCallbackType,
    option::OptionThreadSendMsg,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender, ThreadWriteBackMsg},
};

use crate::model::{estimate_route, RouteEstimate, TravelMode};

/// One route to work out: what the travel panel asks for.
pub struct RouteRequest {
    /// This request's number; the newest one's is in `latest`.
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: TravelMode,
    /// The newest request's number, shared with the app.
    pub latest: Arc<AtomicU64>,
    /// The write-back that hands the reply to the app, on the UI thread.
    pub on_done: WriteBackCallbackType,
}

/// A route the worker worked out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteReply {
    /// The request's number.
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: TravelMode,
    pub route: RouteEstimate,
    /// How long the worker worked on it, in ms.
    pub compute_ms: f64,
}

/// Why a worker stops early: its request is no longer the newest, or the window let go of it.
pub struct Stop<'a> {
    id: u64,
    latest: &'a AtomicU64,
    receiver: &'a mut ThreadReceiver,
}

impl Stop<'_> {
    /// Whether to give up now. The window's `TerminateThread` may sit behind the `Tick`s it
    /// sends every frame: the whole queue is read.
    pub fn now(&mut self) -> bool {
        if self.latest.load(Ordering::Acquire) != self.id {
            return true;
        }
        loop {
            match self.receiver.recv() {
                OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => return true,
                OptionThreadSendMsg::Some(_) => {}
                OptionThreadSendMsg::None => return false,
            }
        }
    }
}

/// The route worker, an azul `Thread`: reads the [`RouteRequest`], works the route out, sends a
/// [`RouteReply`] to the request's `on_done` - unless it was superseded first.
pub extern "C" fn route_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((id, from, to, mode, latest, on_done)) = init
        .downcast_ref::<RouteRequest>()
        .map(|r| (r.id, r.from, r.to, r.mode, r.latest.clone(), r.on_done))
    else {
        return;
    };
    let mut stop = Stop {
        id,
        latest: &latest,
        receiver: &mut receiver,
    };
    if stop.now() {
        return;
    }
    let started = std::time::Instant::now();
    let route = estimate_route(from, to, mode);
    let compute_ms = started.elapsed().as_secs_f64() * 1000.0;
    if stop.now() {
        return;
    }
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(RouteReply {
            id,
            from,
            to,
            mode,
            route,
            compute_ms,
        }),
    )));
}
