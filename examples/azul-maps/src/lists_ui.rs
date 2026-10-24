//! Saved places in the app: the lists in the sidebar (favourites, named and imported lists,
//! the recent destinations - kept on their own and clearable), a list's places on the map, the
//! star on a place's card; Settings > Places: imports (a file, a Google Takeout folder, a shared
//! link), the review of the places an import could not place by itself, and the exports.
//!
//! The lists are files of the data tree (`maps/lists/<id>.geojson`, `saved.rs`), read and
//! written by azul-appkit's file jobs on a thread; an import is read and its unplaced places
//! looked up (with the cached search) on an import worker. Nothing touches a file on the UI
//! thread.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    css::ColorU,
    dialog::{FileDialog, FileOpenResult},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    widgets::{Button, MapMarker, MapOverlay, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::{
    files::{FileJob, FileOutcome},
    l10n::{label, t_args, Arg},
    ui as kit,
};
use azul_routing::transit::transitous::Transitous;

use crate::{
    at, ids,
    import::{
        self,
        links::shared_link,
        review::{query_of, verdict, Candidate, Verdict},
        Imported, Unplaced,
    },
    saved::{
        remember_search as remember_query, searches_from_json, searches_to_json, slug, ListKind, PlaceList,
        SavedPlace, FAVOURITES, LISTS_DIR, RECENTS, RECENTS_LIMIT, SEARCHES_FILE,
    },
    MapState,
};

/// The write-back tags of the lists' file jobs.
const TAG_LISTS_LOAD: u64 = 10;
const TAG_LISTS_SAVE: u64 = 11;
const TAG_EXPORT: u64 = 12;

/// How many unplaced places one import looks up; the rest wait in the review unlooked.
const LOOKUPS: usize = 200;

/// The marker ids of the open list's places: below the transit stops' and the travel line's.
const PLACE_MARKER_BASE: u64 = u64::MAX - 100_000;

const SECTION: &str = "display: flex; flex-direction: column; gap: 4px; min-height: 0px;";
const HEAD: &str = "display: flex; flex-direction: row; align-items: center; font-size: 12px; font-weight: \
                    bold; color: system:secondary-text; @theme(flora) { font-size: 11px; text-transform: \
                    uppercase; letter-spacing: 0.12em; }";
const LISTS: &str = "display: flex; flex-direction: column; gap: 2px; max-height: 240px; overflow-y: auto; \
                     min-height: 0px;";
const PLACES: &str = "display: flex; flex-direction: column; gap: 2px; padding-left: 18px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const COLUMN: &str = "display: flex; flex-direction: column; gap: 6px;";
const REVIEW_ROW: &str = "display: flex; flex-direction: column; gap: 3px; padding: 6px; border-radius: 6px; \
                          background: system:control-background;";
const QUIET: &str = "font-size: 12px; color: system:secondary-text; white-space: normal;";

/// One place still to be placed, with what the search found for it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReviewItem {
    pub place: Unplaced,
    /// The list it goes into, by name.
    pub list: String,
    pub candidates: Vec<Candidate>,
    /// The candidate shown.
    pub choice: usize,
}

/// The saved lists and what goes on with them.
pub(crate) struct ListsState {
    /// The lists, shared with the search worker (a change makes a new one).
    pub lists: Arc<Vec<PlaceList>>,
    /// The recent searches, newest first.
    pub searches: Vec<String>,
    /// The lists' files were read (until then nothing is written).
    pub loaded: bool,
    /// The lists changed since they were last written, by id; the searches by [`SEARCHES_FILE`].
    pub dirty: BTreeSet<String>,
    /// The list shown open in the sidebar (an index into `lists`).
    pub open: Option<usize>,
    /// The key prefix of the lists' files in the data tree (`maps/lists/`).
    pub lists_prefix: String,
    /// The key of the recent searches' file.
    pub searches_key: String,
    /// The key prefix of exports (`maps/exports/`).
    pub exports_prefix: String,
    /// An import is under way.
    pub importing: bool,
    /// What the last import did, or why it failed.
    pub import_note: String,
    /// The newest import's number.
    pub import_latest: Arc<AtomicU64>,
    /// The places waiting for the user.
    pub review: Vec<ReviewItem>,
    /// The shared link being typed in.
    pub link_text: String,
    /// What the last export wrote, and where.
    pub export_note: String,
    /// The file or folder `--import` names, imported once the lists are read.
    pub pending_import: Option<String>,
    /// The addresses of places the search knows (a dropped pin's card shows its address), by
    /// the place to about 10 m.
    pub addresses: std::collections::BTreeMap<String, String>,
    /// The places whose address was asked for already (asked once a session).
    pub addresses_asked: BTreeSet<String>,
}

impl ListsState {
    pub(crate) fn new(app_key: impl Fn(&str) -> String, pending_import: Option<String>) -> Self {
        Self {
            lists: Arc::new(vec![
                PlaceList::new(FAVOURITES, "Favourites", ListKind::Favourites),
                PlaceList::new(RECENTS, "Recent", ListKind::Recents),
            ]),
            searches: Vec::new(),
            loaded: false,
            dirty: BTreeSet::new(),
            open: None,
            lists_prefix: app_key(&format!("{LISTS_DIR}/")),
            searches_key: app_key(SEARCHES_FILE),
            exports_prefix: app_key("exports/"),
            importing: false,
            import_note: String::new(),
            import_latest: Arc::new(AtomicU64::new(0)),
            review: Vec::new(),
            link_text: String::new(),
            export_note: String::new(),
            pending_import,
            addresses: std::collections::BTreeMap::new(),
            addresses_asked: BTreeSet::new(),
        }
    }

    /// Changes the lists (a new shared copy) and marks `id` to be written.
    fn change(&mut self, id: &str, f: impl FnOnce(&mut Vec<PlaceList>)) {
        let mut lists = self.lists.as_ref().clone();
        f(&mut lists);
        self.lists = Arc::new(lists);
        self.dirty.insert(id.to_string());
    }

    /// The index of the list `id`.
    fn index_of(&self, id: &str) -> Option<usize> {
        self.lists.iter().position(|l| l.id == id)
    }
}

/// The words of a list's name (the two built-in lists are named in the window's language).
fn list_name(list: &PlaceList) -> String {
    match list.kind {
        ListKind::Favourites => t_args("azmaps-list-favourites", &[]),
        ListKind::Recents => t_args("azmaps-list-recents", &[]),
        ListKind::Named => list.name.clone(),
    }
}

// ==== Reading and writing the lists ====

/// Reads the lists' files and the recent searches (at the window's creation).
pub(crate) fn load(app: &RefAny, info: &mut CallbackInfo) {
    let Some((root, prefix, searches)) = app
        .clone()
        .downcast_ref::<MapState>()
        .map(|s| (s.data_root.clone(), s.lists.lists_prefix.clone(), s.lists.searches_key.clone()))
    else {
        return;
    };
    let jobs = vec![
        FileJob::GetAll {
            prefix,
            suffix: ".geojson".to_string(),
        },
        FileJob::Get { key: searches },
    ];
    kit::spawn_file_jobs(info, &root, jobs, app.clone(), TAG_LISTS_LOAD, on_files);
}

/// Writes the lists marked changed and the recent searches (from the timer, after a change).
pub(crate) fn save_dirty(app: &RefAny, info: &mut CallbackInfo) {
    let mut state = app.clone();
    let (root, jobs) = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        if !s.lists.loaded || s.lists.dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut s.lists.dirty);
        let mut jobs = Vec::new();
        for id in dirty {
            if id == SEARCHES_FILE {
                jobs.push(FileJob::Put {
                    key: s.lists.searches_key.clone(),
                    bytes: searches_to_json(&s.lists.searches).into_bytes(),
                });
            } else if let Some(list) = s.lists.lists.iter().find(|l| l.id == id) {
                jobs.push(FileJob::Put {
                    key: format!("{}{}.geojson", s.lists.lists_prefix, list.id),
                    bytes: list.to_geojson().into_bytes(),
                });
            }
        }
        (s.data_root.clone(), jobs)
    };
    if !jobs.is_empty() {
        kit::spawn_file_jobs(info, &root, jobs, app.clone(), TAG_LISTS_SAVE, on_files);
    }
}

extern "C" fn on_files(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let mut start_import = None;
    {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        match reply.tag {
            TAG_LISTS_LOAD => {
                let mut lists = s.lists.lists.as_ref().clone();
                for outcome in reply.outcomes {
                    match outcome {
                        FileOutcome::GotAll { files, .. } => {
                            for (key, bytes) in files {
                                let id = key
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or("")
                                    .trim_end_matches(".geojson")
                                    .to_string();
                                match PlaceList::from_geojson(&id, &String::from_utf8_lossy(&bytes)) {
                                    // A list read replaces the empty one made at the start.
                                    Ok(list) => match lists.iter().position(|l| l.id == list.id) {
                                        Some(at) => lists[at] = list,
                                        None => lists.push(list),
                                    },
                                    Err(why) => println!("AZMAPS_LIST_ERROR {id} {why}"),
                                }
                            }
                        }
                        FileOutcome::Got { result: Ok(Some(bytes)), .. } => {
                            s.lists.searches = searches_from_json(&String::from_utf8_lossy(&bytes));
                        }
                        _ => {}
                    }
                }
                let places: usize = lists.iter().map(|l| l.places.len()).sum();
                println!("AZMAPS_LISTS {} {places}", lists.len());
                s.lists.lists = Arc::new(lists);
                s.lists.loaded = true;
                start_import = s.lists.pending_import.take();
            }
            TAG_EXPORT => {
                if let Some(why) = reply.outcomes.iter().find_map(FileOutcome::error) {
                    s.lists.export_note = t_args("azmaps-export-failed", &[("why", Arg::from(why))]);
                }
            }
            _ => {
                if let Some(why) = reply.outcomes.iter().find_map(FileOutcome::error) {
                    println!("AZMAPS_LIST_ERROR - {why}");
                    s.notice = t_args("azmaps-lists-unsaved", &[("why", Arg::from(why))]);
                }
            }
        }
    }
    if let Some(path) = start_import {
        import_from(&app, &mut info, Some(PathBuf::from(path)), None);
    }
    Update::RefreshDom
}

// ==== Remembering ====

/// The place a route was asked to: first among the recent destinations.
pub(crate) fn remember_destination(s: &mut MapState, name: &str, at: (f64, f64)) {
    let place = SavedPlace {
        name: name.to_string(),
        at,
        saved_s: chrono::Local::now().timestamp(),
        ..SavedPlace::default()
    };
    s.lists.change(RECENTS, |lists| {
        if let Some(recents) = lists.iter_mut().find(|l| l.id == RECENTS) {
            recents.remember(place, RECENTS_LIMIT);
        }
    });
}

/// A search the user picked a place of: first among the recent searches.
pub(crate) fn remember_search(s: &mut MapState, query: &str) {
    remember_query(&mut s.lists.searches, query);
    s.lists.dirty.insert(SEARCHES_FILE.to_string());
}

/// Puts the place at `at` into the favourites, or takes it out when it is there.
pub(crate) fn toggle_favourite(s: &mut MapState, name: &str, at: (f64, f64)) {
    let place = SavedPlace {
        name: name.to_string(),
        at,
        saved_s: chrono::Local::now().timestamp(),
        ..SavedPlace::default()
    };
    s.lists.change(FAVOURITES, |lists| {
        if let Some(favourites) = lists.iter_mut().find(|l| l.id == FAVOURITES) {
            if favourites.has_place_at(at) {
                favourites
                    .places
                    .retain(|p| azul_routing::distance_m(p.at, at) > crate::saved::SAME_PLACE_M);
            } else {
                favourites.places.push(place);
            }
        }
    });
}

/// Whether a favourite is at `at`.
pub(crate) fn is_favourite(s: &MapState, at: (f64, f64)) -> bool {
    s.lists
        .lists
        .iter()
        .any(|l| l.kind == ListKind::Favourites && l.has_place_at(at))
}

// ==== The sidebar ====

/// A list's row or a place's: which, and the app.
struct ListRef {
    app: RefAny,
    list: usize,
    place: usize,
}

/// The saved lists in the sidebar: a row per list with its count (the open one shows its
/// places; a click on one makes it the destination). Nothing while every list is empty.
pub(crate) fn saved_section(s: &MapState, app: &RefAny) -> Option<Dom> {
    let lists = &s.lists.lists;
    if lists.iter().all(|l| l.places.is_empty()) {
        return None;
    }
    let mut rows = Dom::create_div().with_css(LISTS).with_id(ids::SAVED_LISTS);
    for (index, list) in lists.iter().enumerate() {
        if list.places.is_empty() {
            continue;
        }
        let icon = match list.kind {
            ListKind::Favourites => "star",
            ListKind::Recents => "history",
            ListKind::Named => "list",
        };
        rows.add_child(
            Button::create(format!("{} ({})", list_name(list), list.places.len()))
                .with_icon(icon)
                .with_toggled(s.lists.open == Some(index))
                .with_on_click(
                    RefAny::new(ListRef {
                        app: app.clone(),
                        list: index,
                        place: 0,
                    }),
                    on_list as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::indexed("list", index)),
        );
        if s.lists.open == Some(index) {
            let mut places = Dom::create_div().with_css(PLACES);
            for (p, place) in list.places.iter().enumerate().take(100) {
                let name = if place.name.is_empty() { crate::coords(place.at.0, place.at.1) } else { place.name.clone() };
                places.add_child(
                    Button::create(name)
                        .with_icon("place")
                        .with_on_click(
                            RefAny::new(ListRef {
                                app: app.clone(),
                                list: index,
                                place: p,
                            }),
                            on_saved_place as ButtonOnClickCallbackType,
                        )
                        .dom()
                        .with_id(ids::indexed("saved-place", p)),
                );
            }
            rows.add_child(places);
        }
    }
    let mut head = Dom::create_div().with_css(HEAD).with_child(
        Dom::create_div()
            .with_css("flex-grow: 1;")
            .with_child(Dom::create_span_with_text(label("azmaps-saved"))),
    );
    if lists.iter().any(|l| l.kind == ListKind::Recents && !l.places.is_empty()) {
        head.add_child(
            Button::create("")
                .with_icon("history_toggle_off")
                .with_on_click(app.clone(), on_clear_recents as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLEAR_RECENT_DESTINATIONS)
                .with_accessibility_name(label("azmaps-clear-recent-destinations")),
        );
    }
    Some(
        Dom::create_div()
            .with_css(SECTION)
            .with_id(ids::SAVED)
            .with_child(head)
            .with_child(rows),
    )
}

/// The open list's places on the map, as dots.
pub(crate) fn markers(s: &MapState) -> Vec<MapOverlay> {
    let Some(list) = s.lists.open.and_then(|i| s.lists.lists.get(i)) else {
        return Vec::new();
    };
    let colour = match list.kind {
        ListKind::Favourites => ColorU { r: 0xf4, g: 0xb4, b: 0x00, a: 0xff },
        _ => ColorU { r: 0x1a, g: 0x73, b: 0xe8, a: 0xff },
    };
    list.places
        .iter()
        .enumerate()
        .take(500)
        .map(|(i, p)| {
            MapOverlay::Marker(
                MapMarker::create_dot(PLACE_MARKER_BASE + i as u64, at(p.at), colour)
                    .with_name(AzString::from(p.name.clone())),
            )
        })
        .collect()
}

extern "C" fn on_list(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, list)) = data.downcast_ref::<ListRef>().map(|r| (r.app.clone(), r.list)) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    s.lists.open = if s.lists.open == Some(list) { None } else { Some(list) };
    // An opened list is framed on the map.
    let points: Vec<(f64, f64)> = s
        .lists
        .open
        .and_then(|i| s.lists.lists.get(i))
        .map(|l| l.places.iter().map(|p| p.at).collect())
        .unwrap_or_default();
    if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied()) {
        let (mut south, mut west, mut north, mut east) = (first.0, first.1, first.0, first.1);
        for (lat, lon) in &points {
            south = south.min(*lat);
            north = north.max(*lat);
            west = west.min(*lon);
            east = east.max(*lon);
        }
        let _ = last;
        let inset = if s.sidebar_open { crate::SIDEBAR_INSET } else { 0.0 };
        let (lat, lon, zoom) = crate::model::fit_view((south, west), (north, east), s.size.get(), inset);
        s.viewport.zoom = zoom.min(16.0);
        s.centre_on(lat, lon);
    }
    Update::RefreshDom
}

/// A saved place clicked: it becomes the destination.
extern "C" fn on_saved_place(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, list, place)) = data.downcast_ref::<ListRef>().map(|r| (r.app.clone(), r.list, r.place)) else {
        return Update::DoNothing;
    };
    {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        let Some(p) = s.lists.lists.get(list).and_then(|l| l.places.get(place)).cloned() else {
            return Update::DoNothing;
        };
        let name = if p.name.is_empty() { crate::model::place_text(p.at.0, p.at.1) } else { p.name.clone() };
        s.travel.to = name.clone();
        s.travel.to_pick = Some(crate::Picked { label: name, at: p.at });
        s.suggestions = None;
        s.centre_on(p.at.0, p.at.1);
        s.announce_travel();
    }
    crate::adopt_field_texts(&mut info);
    crate::ask_route(&app, &mut info);
    Update::RefreshDom
}

extern "C" fn on_clear_recents(mut app: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    s.lists.change(RECENTS, |lists| {
        if let Some(recents) = lists.iter_mut().find(|l| l.id == RECENTS) {
            recents.places.clear();
        }
    });
    s.lists.searches.clear();
    s.lists.dirty.insert(SEARCHES_FILE.to_string());
    s.lists.open = None;
    println!("AZMAPS_RECENTS_CLEARED");
    Update::RefreshDom
}

// ==== Importing ====

/// What the import worker reads.
pub(crate) struct ImportRequest {
    pub id: u64,
    /// A file or a folder.
    pub path: Option<PathBuf>,
    /// A shared link.
    pub link: Option<String>,
    /// The search the unplaced places are looked up with.
    pub online: Option<Arc<Transitous>>,
    pub language: String,
    pub on_done: azul::callbacks::WriteBackCallbackType,
}

/// What the import worker read and looked up.
#[derive(Debug, Clone)]
pub(crate) struct ImportReply {
    pub id: u64,
    pub result: Result<Imported, String>,
    /// The places looked up and found clearly, already in their lists.
    pub placed: usize,
    /// The places for the user to confirm, fix or skip.
    pub review: Vec<ReviewItem>,
}

/// The import worker: reads the file, the folder or the link, looks up the places that came
/// without coordinates, places the clear ones and hands the rest to the review.
extern "C" fn import_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((id, path, link, online, language, on_done)) = init.downcast_ref::<ImportRequest>().map(|r| {
        (r.id, r.path.clone(), r.link.clone(), r.online.clone(), r.language.clone(), r.on_done)
    }) else {
        return;
    };
    let result = match (&path, &link) {
        (Some(path), _) => import::import_path(path),
        (None, Some(link)) => imported_link(link),
        (None, None) => Err("nothing to import".to_string()),
    };
    let mut placed = 0;
    let mut review = Vec::new();
    let result = result.map(|mut imported| {
        let unplaced = std::mem::take(&mut imported.unplaced);
        for (n, place) in unplaced.into_iter().enumerate() {
            let list = imported.lists.get(place.list).map(|l| l.name.clone()).unwrap_or_default();
            let candidates: Vec<Candidate> = match (&online, n < LOOKUPS) {
                (Some(online), true) => online
                    .geocode(&query_of(&place), None, &language, &mut || false)
                    .ok()
                    .flatten()
                    .map(|(found, _)| {
                        found
                            .iter()
                            .take(5)
                            .map(|f| Candidate {
                                label: f.label(),
                                at: f.at,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            match verdict(&place.name, &candidates) {
                Verdict::Certain(i) => {
                    if let Some(target) = imported.lists.get_mut(place.list) {
                        target.places.push(SavedPlace {
                            name: place.name.clone(),
                            at: candidates[i].at,
                            note: place.note.clone(),
                            address: place.address.clone(),
                            url: place.url.clone(),
                            saved_s: 0,
                        });
                        placed += 1;
                    }
                }
                Verdict::Uncertain | Verdict::NotFound => review.push(ReviewItem {
                    place,
                    list,
                    candidates,
                    choice: 0,
                }),
            }
        }
        imported
    });
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(ImportReply {
            id,
            result,
            placed,
            review,
        }),
    )));
}

/// A shared link as an import: a list "Shared links" with its place, or the place unplaced
/// when the link has no coordinates.
fn imported_link(link: &str) -> Result<Imported, String> {
    let shared = shared_link(link).ok_or_else(|| "not a link to a place".to_string())?;
    let mut list = PlaceList::new("shared-links", "Shared links", ListKind::Named);
    let mut unplaced = Vec::new();
    match shared.at {
        Some(at) => list.places.push(SavedPlace {
            name: shared.name,
            at,
            address: shared.address,
            url: link.trim().to_string(),
            ..SavedPlace::default()
        }),
        None => unplaced.push(Unplaced {
            list: 0,
            name: shared.name,
            note: String::new(),
            address: shared.address,
            url: link.trim().to_string(),
        }),
    }
    Ok(Imported {
        lists: vec![list],
        unplaced,
        skipped: 0,
    })
}

/// Imports `path` (a file or a folder) or `link` on the import worker.
pub(crate) fn import_from(app: &RefAny, info: &mut CallbackInfo, path: Option<PathBuf>, link: Option<String>) {
    let mut state = app.clone();
    let request = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        let id = s.lists.import_latest.fetch_add(1, Ordering::AcqRel) + 1;
        s.lists.importing = true;
        s.lists.import_note = t_args("azmaps-import-busy", &[]);
        ImportRequest {
            id,
            path,
            link,
            online: s.services.transit.clone(),
            language: azul_appkit::l10n::locale().chars().take(2).collect(),
            on_done: on_imported,
        }
    };
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(request), app.clone(), import_worker));
}

/// The import came back: its lists join the saved ones (a list of the same name gets the
/// places it does not have), its uncertain places wait in the review.
extern "C" fn on_imported(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = msg.downcast_ref::<ImportReply>().map(|r| r.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.lists.import_latest.load(Ordering::Acquire) != reply.id {
        return Update::DoNothing;
    }
    s.lists.importing = false;
    match reply.result {
        Err(why) => {
            println!("AZMAPS_IMPORT_FAILED {why}");
            s.lists.import_note = t_args("azmaps-import-failed", &[("why", Arg::from(why))]);
        }
        Ok(imported) => {
            let lists = imported.lists.len();
            // Every place the import read: in its list, or waiting in the review.
            let places = imported.places() + reply.review.len();
            for list in imported.lists {
                merge_list(&mut s, list);
            }
            let review = reply.review.len();
            s.lists.review.extend(reply.review);
            println!("AZMAPS_IMPORTED {lists} {places} {} {review}", reply.placed);
            s.lists.import_note = t_args(
                "azmaps-import-done",
                &[
                    ("lists", Arg::from(lists)),
                    ("places", Arg::from(places)),
                    ("review", Arg::from(review)),
                ],
            );
        }
    }
    Update::RefreshDom
}

/// Adds an imported list: into the list of the same name (the places it does not have yet),
/// else as a new list under a free id.
fn merge_list(s: &mut MapState, list: PlaceList) {
    let existing = s
        .lists
        .lists
        .iter()
        .position(|l| l.kind == ListKind::Named && l.name == list.name);
    let id = match existing {
        Some(at) => s.lists.lists[at].id.clone(),
        None => {
            let base = slug(&list.name);
            let mut id = base.clone();
            let mut n = 2;
            while s.lists.index_of(&id).is_some() || id == FAVOURITES || id == RECENTS {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        }
    };
    s.lists.change(&id, |lists| match lists.iter_mut().find(|l| l.id == id) {
        Some(target) => {
            for place in list.places {
                let known = target
                    .places
                    .iter()
                    .any(|p| p.name == place.name && azul_routing::distance_m(p.at, place.at) <= crate::saved::SAME_PLACE_M);
                if !known {
                    target.places.push(place);
                }
            }
            target.routes.extend(list.routes);
        }
        None => {
            let mut added = list;
            added.id = id.clone();
            lists.push(added);
        }
    });
}

/// The place of a review item put into its list: the shown candidate's position.
fn place_reviewed(s: &mut MapState, item: &ReviewItem) {
    let Some(candidate) = item.candidates.get(item.choice) else {
        return;
    };
    let place = SavedPlace {
        name: item.place.name.clone(),
        at: candidate.at,
        note: item.place.note.clone(),
        address: item.place.address.clone(),
        url: item.place.url.clone(),
        saved_s: 0,
    };
    let mut list = PlaceList::new(&slug(&item.list), &item.list, ListKind::Named);
    list.places.push(place);
    merge_list(s, list);
}

/// A review item's button: which item, and the app.
struct ReviewRef {
    app: RefAny,
    index: usize,
}

/// Settings > Places: import a file or a folder, paste a link, the review, export.
pub(crate) fn places_section(s: &MapState, app: &RefAny) -> kit::AppSection {
    let mut content = Dom::create_div().with_css(COLUMN);
    content.add_child(
        Dom::create_div()
            .with_css(ROW)
            .with_child(
                Button::create(label("azmaps-import-file"))
                    .with_icon("file_open")
                    .with_on_click(app.clone(), on_import_file as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::IMPORT_FILE),
            )
            .with_child(
                Button::create(label("azmaps-import-folder"))
                    .with_icon("folder_open")
                    .with_on_click(app.clone(), on_import_folder as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::IMPORT_FOLDER),
            ),
    );
    content.add_child(kit::note("azmaps-import-formats"));
    content.add_child(kit::note("azmaps-import-apple"));
    content.add_child(
        Dom::create_div()
            .with_css(ROW)
            .with_child(
                Dom::create_div().with_css("flex-grow: 1; min-width: 0px;").with_child(
                    TextInput::create()
                        .with_text(s.lists.link_text.as_str())
                        .with_placeholder(label("azmaps-import-link"))
                        .with_accessibility_name(label("azmaps-import-link"))
                        .with_on_text_input(app.clone(), on_link_text as TextInputOnTextInputCallbackType)
                        .dom()
                        .with_id(ids::IMPORT_LINK),
                ),
            )
            .with_child(
                Button::create(label("azmaps-import-link-add"))
                    .with_on_click(app.clone(), on_link_add as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::IMPORT_LINK_ADD),
            ),
    );
    if !s.lists.import_note.is_empty() {
        content.add_child(
            Dom::create_div()
                .with_css(QUIET)
                .with_id(ids::IMPORT_NOTE)
                .with_child(Dom::create_span_with_text(s.lists.import_note.as_str())),
        );
    }
    if !s.lists.review.is_empty() {
        let mut review = Dom::create_div().with_css(COLUMN).with_id(ids::IMPORT_REVIEW).with_child(
            Dom::create_div()
                .with_css(QUIET)
                .with_child(Dom::create_span_with_text(t_args(
                    "azmaps-review-head",
                    &[("count", Arg::from(s.lists.review.len()))],
                ))),
        );
        for (index, item) in s.lists.review.iter().enumerate().take(50) {
            review.add_child(review_row(item, index, app));
        }
        content.add_child(review);
    }
    content.add_child(
        Dom::create_div()
            .with_css(ROW)
            .with_child(
                Button::create(label("azmaps-export-gpx"))
                    .with_icon("download")
                    .with_on_click(app.clone(), on_export_gpx as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::EXPORT_GPX),
            )
            .with_child(
                Button::create(label("azmaps-export-geojson"))
                    .with_icon("download")
                    .with_on_click(app.clone(), on_export_geojson as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::EXPORT_GEOJSON),
            ),
    );
    if !s.lists.export_note.is_empty() {
        content.add_child(
            Dom::create_div()
                .with_css(QUIET)
                .with_id(ids::EXPORT_NOTE)
                .with_child(Dom::create_span_with_text(s.lists.export_note.as_str())),
        );
    }
    kit::AppSection {
        category: 1,
        title: t_args("azmaps-places-title", &[]),
        content,
    }
}

/// One place of the review: its name and list, the match shown, Use / Other match / Skip.
fn review_row(item: &ReviewItem, index: usize, app: &RefAny) -> Dom {
    let reference = || {
        RefAny::new(ReviewRef {
            app: app.clone(),
            index,
        })
    };
    let found = match item.candidates.get(item.choice) {
        Some(c) => t_args("azmaps-review-match", &[("place", Arg::from(c.label.clone()))]),
        None => t_args("azmaps-review-none", &[]),
    };
    let mut buttons = Dom::create_div().with_css(ROW);
    if !item.candidates.is_empty() {
        buttons.add_child(
            Button::create(label("azmaps-review-use"))
                .with_on_click(reference(), on_review_use as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::indexed("review-use", index)),
        );
    }
    if item.candidates.len() > 1 {
        buttons.add_child(
            Button::create(label("azmaps-review-next"))
                .with_on_click(reference(), on_review_next as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::indexed("review-next", index)),
        );
    }
    buttons.add_child(
        Button::create(label("azmaps-review-skip"))
            .with_on_click(reference(), on_review_skip as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::indexed("review-skip", index)),
    );
    Dom::create_div()
        .with_css(REVIEW_ROW)
        .with_id(ids::indexed("review", index))
        .with_child(Dom::create_span_with_text(format!("{} \u{b7} {}", item.place.name, item.list)))
        .with_child(Dom::create_div().with_css(QUIET).with_child(Dom::create_span_with_text(found)))
        .with_child(buttons)
}

/// The review item of a button, and the app.
fn review_of(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<ReviewRef>().map(|r| (r.app.clone(), r.index))
}

extern "C" fn on_review_use(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = review_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if index >= s.lists.review.len() {
        return Update::DoNothing;
    }
    let item = s.lists.review.remove(index);
    place_reviewed(&mut s, &item);
    println!("AZMAPS_REVIEW_USED {}", item.place.name);
    Update::RefreshDom
}

extern "C" fn on_review_next(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = review_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if let Some(item) = s.lists.review.get_mut(index) {
        item.choice = (item.choice + 1) % item.candidates.len().max(1);
    }
    Update::RefreshDom
}

extern "C" fn on_review_skip(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = review_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if index < s.lists.review.len() {
        let item = s.lists.review.remove(index);
        println!("AZMAPS_REVIEW_SKIPPED {}", item.place.name);
    }
    Update::RefreshDom
}

extern "C" fn on_import_file(app: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        label("azmaps-import-file"),
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_folder(app: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_directory(label("azmaps-import-folder"), OptionString::None, app, on_picked);
    Update::DoNothing
}

extern "C" fn on_picked(app: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    import_from(&app, &mut info, Some(PathBuf::from(path.inner.as_str())), None);
    Update::RefreshDom
}

extern "C" fn on_link_text(mut app: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = app.downcast_mut::<MapState>() {
        s.lists.link_text = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_link_add(mut app: RefAny, mut info: CallbackInfo) -> Update {
    let link = app
        .downcast_mut::<MapState>()
        .map(|mut s| std::mem::take(&mut s.lists.link_text))
        .unwrap_or_default();
    if link.trim().is_empty() {
        return Update::DoNothing;
    }
    crate::adopt_field_texts(&mut info);
    import_from(&app, &mut info, None, Some(link));
    Update::RefreshDom
}

// ==== Exporting ====

/// The seconds of now, for an export's file name.
fn stamp() -> i64 {
    chrono::Local::now().timestamp()
}

/// Exports `bytes` as `name`: offered to the system's save dialog, and kept in the data
/// folder (`maps/exports/`) either way.
fn export(app: &RefAny, info: &mut CallbackInfo, name: &str, mime: &str, bytes: Vec<u8>) {
    let offered = FileDialog::save_bytes(name, mime, bytes.clone());
    let mut state = app.clone();
    let Some((root, key)) = state.downcast_mut::<MapState>().map(|mut s| {
        let key = format!("{}{}-{}", s.lists.exports_prefix, stamp(), name.replace(' ', "-"));
        s.lists.export_note = t_args(
            if offered { "azmaps-export-offered" } else { "azmaps-export-kept" },
            &[("file", Arg::from(key.clone()))],
        );
        (s.data_root.clone(), key)
    }) else {
        return;
    };
    println!("AZMAPS_EXPORTED {key}");
    kit::spawn_file_jobs(info, &root, vec![FileJob::Put { key, bytes }], app.clone(), TAG_EXPORT, on_files);
}

extern "C" fn on_export_gpx(mut app: RefAny, mut info: CallbackInfo) -> Update {
    let Some(text) = app.downcast_ref::<MapState>().map(|s| import::gpx::to_gpx(&s.lists.lists)) else {
        return Update::DoNothing;
    };
    export(&app, &mut info, "AzMaps places.gpx", "application/gpx+xml", text.into_bytes());
    Update::RefreshDom
}

extern "C" fn on_export_geojson(mut app: RefAny, mut info: CallbackInfo) -> Update {
    // Every list's own file is GeoJSON already: the export is all of them in one collection.
    let Some(text) = app.downcast_ref::<MapState>().map(|s| {
        let mut all = PlaceList::new("azmaps", "AzMaps", ListKind::Named);
        for list in s.lists.lists.iter() {
            all.places.extend(list.places.iter().cloned());
            all.routes.extend(list.routes.iter().cloned());
        }
        all.to_geojson()
    }) else {
        return Update::DoNothing;
    };
    export(&app, &mut info, "AzMaps places.geojson", "application/geo+json", text.into_bytes());
    Update::RefreshDom
}

// ==== Addresses ====

/// What the address worker asks.
struct AddressRequest {
    /// The place to about 10 m: the address's key.
    key: String,
    at: (f64, f64),
    online: Arc<Transitous>,
    on_done: azul::callbacks::WriteBackCallbackType,
}

/// The address of a place, when the search knows one.
#[derive(Debug, Clone)]
struct AddressReply {
    key: String,
    address: Option<String>,
}

/// The address worker: the nearest address of the place (cached; offline the cache answers).
extern "C" fn address_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((key, at, online, on_done)) = init
        .downcast_ref::<AddressRequest>()
        .map(|r| (r.key.clone(), r.at, Arc::clone(&r.online), r.on_done))
    else {
        return;
    };
    let address = match online.reverse(at, &mut || false) {
        Ok(Some((Some(found), _))) => Some(found.label()),
        _ => None,
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(AddressReply { key, address }),
    )));
}

/// Asks for the address of the place at `at` (once a session; a dropped pin's card shows it).
pub(crate) fn ask_address(app: &RefAny, info: &mut CallbackInfo, at: (f64, f64)) {
    let mut state = app.clone();
    let request = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        let Some(online) = s.services.transit.clone() else {
            return;
        };
        let key = azul_routing::cache::place_key(at, 4);
        if !s.lists.addresses_asked.insert(key.clone()) {
            return;
        }
        AddressRequest {
            key,
            at,
            online,
            on_done: on_address,
        }
    };
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(request), app.clone(), address_worker));
}

extern "C" fn on_address(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = msg.downcast_ref::<AddressReply>().map(|r| r.clone()) else {
        return Update::DoNothing;
    };
    let Some(address) = reply.address else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    println!("AZMAPS_ADDRESS {} {address}", reply.key);
    s.lists.addresses.insert(reply.key, address);
    Update::RefreshDom
}

/// The address the search knows for the place at `at`.
pub(crate) fn address_of(s: &MapState, at: (f64, f64)) -> Option<&str> {
    s.lists
        .addresses
        .get(&azul_routing::cache::place_key(at, 4))
        .map(String::as_str)
}
