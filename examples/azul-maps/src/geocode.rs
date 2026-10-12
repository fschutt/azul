//! Place names off the UI thread: once the typing in a travel field rests, its text is a search
//! for the geocoder worker, an azul `Thread`, which looks it up in the places AzMaps knows
//! (`places.rs`) and hands back the suggestions. Only the reply comes back to the UI thread.
//!
//! A newer search supersedes an older one the way a newer route does (`route.rs`): the app and
//! its workers share the newest search's number, a worker whose search is no longer the newest
//! gives up, and a reply that arrives anyway is dropped by its number (`crate::on_search_done`).

use std::sync::{atomic::AtomicU64, Arc};

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};

use crate::{
    model::End,
    places::{gazetteer, Suggestion, SUGGESTION_LIMIT},
    route::Stop,
};

/// One search: a travel field's text as it rested.
pub struct SearchRequest {
    /// This search's number; the newest one's is in `latest`.
    pub id: u64,
    /// The field it is for.
    pub end: End,
    pub query: String,
    /// Where the map looks: of two places that match alike the nearer comes first.
    pub near: Option<(f64, f64)>,
    /// The newest search's number, shared with the app.
    pub latest: Arc<AtomicU64>,
    /// The write-back that hands the reply to the app, on the UI thread.
    pub on_done: WriteBackCallbackType,
}

/// The places a search found.
pub struct SearchReply {
    /// The search's number.
    pub id: u64,
    pub end: End,
    /// The text it searched: a reply for text the field no longer holds is dropped.
    pub query: String,
    pub suggestions: Vec<Suggestion>,
    /// How long the worker searched, in ms.
    pub compute_ms: f64,
}

/// The geocoder worker, an azul `Thread`: reads the [`SearchRequest`], searches, sends a
/// [`SearchReply`] to the request's `on_done` - unless it was superseded first.
pub extern "C" fn search_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((id, end, query, near, latest, on_done)) = init.downcast_ref::<SearchRequest>().map(|r| {
        (r.id, r.end, r.query.clone(), r.near, r.latest.clone(), r.on_done)
    }) else {
        return;
    };
    let mut stop = Stop::new(id, &latest, &mut receiver);
    if stop.now() {
        return;
    }
    let started = std::time::Instant::now();
    let suggestions = gazetteer().search(&query, near, SUGGESTION_LIMIT);
    let compute_ms = started.elapsed().as_secs_f64() * 1000.0;
    if stop.now() {
        return;
    }
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(SearchReply {
            id,
            end,
            query,
            suggestions,
            compute_ms,
        }),
    )));
}
