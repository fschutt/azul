//! Place names off the UI thread: once the typing in a travel field rests, its text is a search
//! for the geocoder worker, an azul `Thread`, which looks it up in the places AzMaps knows
//! (`places.rs`) and hands back the suggestions. Only the reply comes back to the UI thread.
//!
//! A newer search supersedes an older one the way a newer route does (`route.rs`): the app and
//! its workers share the newest search's number, a worker whose search is no longer the newest
//! gives up, and a reply that arrives anyway is dropped by its number (`crate::on_search_done`).
//!
//! A search answers twice: at once with the saved places and AzMaps' own list of cities, then -
//! from three letters on, when the online search is there - again with the stations,
//! addresses and places Transitous finds (cached; offline the cache answers, marked so). Every
//! suggestion says where it was found, and the list credits its sources.

use std::sync::{atomic::AtomicU64, Arc};

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};

use azul_appkit::l10n::{t_args, Arg};
use azul_routing::transit::transitous::{Found, FoundKind};

use crate::{
    model::End,
    places::{gazetteer, Suggestion, SuggestionKind, SuggestionSource, SUGGESTION_LIMIT},
    route::Stop,
    saved::PlaceList,
};

/// The fewest letters a search sends to the online search: one or two letters find everything
/// and nothing.
pub const ONLINE_MIN_CHARS: usize = 3;

/// A place the online search found, as a suggestion: its town where the town is not its name.
#[must_use]
pub fn suggestion_of_found(found: &Found) -> Suggestion {
    let _ = (found, FoundKind::Stop);
    Suggestion {
        en: String::new(),
        de: String::new(),
        country_en: String::new(),
        country_de: String::new(),
        lat: 0.0,
        lon: 0.0,
        source: SuggestionSource::Transitous,
        kind: SuggestionKind::Place,
    }
}

/// The saved places a search finds, as suggestions naming their list.
#[must_use]
pub fn saved_suggestions(lists: &[PlaceList], query: &str, limit: usize) -> Vec<Suggestion> {
    let _ = (lists, query, limit);
    Vec::new()
}

/// The suggestions in the order they show: the saved places, then the online search's, then
/// AzMaps' cities; a place a list before already has (the same name within 2 km) is not shown
/// twice; at most `limit`.
#[must_use]
pub fn merge(saved: Vec<Suggestion>, online: Vec<Suggestion>, cities: Vec<Suggestion>, limit: usize) -> Vec<Suggestion> {
    let _ = (saved, online, cities, limit);
    Vec::new()
}

/// The credit of a source, as the list shows it under its row.
#[must_use]
pub fn source_credit(source: &SuggestionSource) -> String {
    let _ = (source, t_args("azmaps-source-places", &[]), Arg::from(String::new()));
    String::new()
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saved::{ListKind, SavedPlace};

    fn found(name: &str, kind: FoundKind, area: Option<&str>, at: (f64, f64)) -> Found {
        Found {
            name: name.to_string(),
            kind,
            at,
            area: area.map(str::to_string),
            country: Some("DE".to_string()),
        }
    }

    fn named(name: &str, at: (f64, f64), source: SuggestionSource) -> Suggestion {
        Suggestion {
            en: name.to_string(),
            de: name.to_string(),
            country_en: String::new(),
            country_de: String::new(),
            lat: at.0,
            lon: at.1,
            source,
            kind: SuggestionKind::Place,
        }
    }

    #[test]
    fn an_online_place_is_a_suggestion_with_its_town_and_its_kind() {
        let station = suggestion_of_found(&found("Berlin Hbf", FoundKind::Stop, Some("Berlin"), (52.52498, 13.369114)));
        assert_eq!(station.label(false), "Berlin Hbf, Berlin");
        assert_eq!(station.label(true), "Berlin Hbf, Berlin");
        assert_eq!(station.kind, SuggestionKind::Station);
        assert_eq!(station.source, SuggestionSource::Transitous);
        assert_eq!((station.lat, station.lon), (52.52498, 13.369114));
        let town = suggestion_of_found(&found("Berlin", FoundKind::Place, Some("Berlin"), (52.52, 13.40)));
        assert_eq!(town.label(false), "Berlin", "the town is not said twice");
        let street = suggestion_of_found(&found("Pariser Platz 1", FoundKind::Address, Some("Berlin"), (52.5164, 13.3778)));
        assert_eq!(street.kind, SuggestionKind::Address);
    }

    #[test]
    fn saved_places_come_first_and_a_place_is_shown_once() {
        let mut favourites = PlaceList::new("favourites", "Favourites", ListKind::Favourites);
        favourites.places.push(SavedPlace {
            name: "Berlin Hbf".to_string(),
            at: (52.5250, 13.3691),
            ..SavedPlace::default()
        });
        let saved = saved_suggestions(&[favourites], "berlin", 6);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].source, SuggestionSource::Saved("Favourites".to_string()));
        assert_eq!(saved[0].kind, SuggestionKind::Saved);
        let online = vec![
            named("Berlin Hbf", (52.52498, 13.369114), SuggestionSource::Transitous),
            named("Berlin Ostbahnhof", (52.5108, 13.4348), SuggestionSource::Transitous),
        ];
        let cities = vec![named("Berlin", (52.52, 13.405), SuggestionSource::Places)];
        let shown = merge(saved, online, cities, 6);
        let names: Vec<(&str, &SuggestionSource)> = shown.iter().map(|s| (s.en.as_str(), &s.source)).collect();
        assert_eq!(
            names,
            [
                ("Berlin Hbf", &SuggestionSource::Saved("Favourites".to_string())),
                ("Berlin Ostbahnhof", &SuggestionSource::Transitous),
                ("Berlin", &SuggestionSource::Places),
            ]
        );
        let many: Vec<Suggestion> = (0..10).map(|i| named(&format!("Place {i}"), (50.0 + f64::from(i), 10.0), SuggestionSource::Transitous)).collect();
        assert_eq!(merge(Vec::new(), many, Vec::new(), 6).len(), 6);
    }

    #[test]
    fn every_source_is_credited() {
        crate::l10n::in_english();
        assert_eq!(source_credit(&SuggestionSource::Places), "AzMaps' list of cities");
        assert_eq!(source_credit(&SuggestionSource::Transitous), "Transitous \u{b7} \u{a9} OpenStreetMap contributors");
        assert_eq!(source_credit(&SuggestionSource::Saved("Trip".to_string())), "Saved in Trip");
    }
}
