//! Routing off the UI thread (ROUTING.md, "How the travel panel calls it"): every route the
//! travel panel asks for is worked out on an azul `Thread`, and only its progress and its reply
//! come back to the UI thread.
//!
//! A newer request supersedes an older one. The app and its workers share the number of the
//! newest request (`latest`): a worker whose request is no longer the newest gives up at its
//! next check, and a reply that arrives anyway is dropped by its number
//! (`crate::on_route_done`). Nothing waits for a worker on the UI thread - removing a running
//! `Thread` would (its destructor waits for the worker to acknowledge).
//!
//! With routing tiles (`--routing`, see [`routing_source`]) the worker works the road route out
//! on them with azul-routing: the corridor's tiles are fetched in parallel (and cached in memory
//! and in the data folder), A* runs over them, the corridor widens when there is no way, and
//! every tile that arrives is reported to the panel. Without tiles the worker asks a Valhalla
//! server (`--routing-online`, FOSSGIS's public one by default; see [`route_source`]) for the
//! road, its time and the turn-by-turn steps. With neither, and for public transport (a
//! timetable question, not a road graph), it answers the straight-line estimate
//! ([`crate::model::estimate_route`]).

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use azul::{
    callbacks::WriteBackCallbackType,
    option::OptionThreadSendMsg,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender, ThreadWriteBackMsg},
};
use azul_routing::{
    online::{OnlineError, Step, Valhalla},
    source::{CachedTiles, FolderTiles, HttpTiles, KeepPolicy},
    Manifest, Mode, Progress, RouteError, TileSource,
};

use crate::model::{estimate_route, RouteEstimate, TravelMode};

/// Where the routing tiles come from, shared by every route worker of the session: one cache in
/// memory, one connection pool.
pub type Tiles = Arc<RoutingTiles>;

/// The routing server, shared by every route worker of the session: one connection pool, its
/// requests a second apart, the session's routes remembered.
pub type Online = Arc<Valhalla>;

/// What AzMaps' downloads name themselves: the app and its version, nothing about the user.
#[must_use]
pub fn user_agent() -> String {
    format!("AzMaps/{} (+https://github.com/fschutt/azul)", env!("CARGO_PKG_VERSION"))
}

/// The routing server at `url` (a Valhalla `/route` endpoint).
#[must_use]
pub fn online_source(url: &str) -> Online {
    Arc::new(Valhalla::new(url.trim(), user_agent()).trusting(&azul_appkit::ui::trusted_roots_pem()))
}

/// The routing server at `url`, its routes kept in the session's cache of answers (on disk,
/// so a route answers again offline); with `offline` nothing is sent.
#[must_use]
pub fn online_source_cached(url: &str, cache: Arc<azul_routing::cache::ResponseCache>, offline: bool) -> Online {
    let server = Valhalla::with_cache(url.trim(), user_agent(), cache)
        .trusting(&azul_appkit::ui::trusted_roots_pem());
    server.fetcher().set_offline(offline);
    Arc::new(server)
}

/// Where a route comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSource {
    /// azul-routing's A* on the routing tiles.
    Tiles,
    /// The routing server.
    Online,
    /// The straight-line estimate.
    Estimate,
}

impl RouteSource {
    /// The word scripts read (`AZMAPS_ROAD <mode> ... <source>`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Tiles => "tiles",
            Self::Online => "online",
            Self::Estimate => "estimate",
        }
    }
}

/// Where a route for `mode` comes from: the routing tiles when there are any, else the routing
/// server when there is one, else the straight-line estimate - which public transport always
/// is (a timetable question, not a road graph).
#[must_use]
pub const fn route_source(tiles: bool, online: bool, mode: TravelMode) -> RouteSource {
    if road_mode(mode).is_none() {
        RouteSource::Estimate
    } else if tiles {
        RouteSource::Tiles
    } else if online {
        RouteSource::Online
    } else {
        RouteSource::Estimate
    }
}

/// The routing tiles of a `--routing` value ([`routing_source`]).
pub enum RoutingTiles {
    /// Downloaded, cached in memory and in the data folder.
    Http(CachedTiles<HttpTiles>),
    /// Read from a folder, kept in memory.
    Folder(CachedTiles<FolderTiles>),
}

impl TileSource for RoutingTiles {
    fn fetch(&self, tile: azul_routing::TileId) -> Result<Option<Vec<u8>>, String> {
        match self {
            Self::Http(tiles) => tiles.fetch(tile),
            Self::Folder(tiles) => tiles.fetch(tile),
        }
    }
}

impl RoutingTiles {
    /// Keeps the download cache in the data folder under its cap (oldest tiles first).
    pub fn prune(&self) {
        if let Self::Http(tiles) = self {
            tiles.prune();
        }
    }

    /// The hosting's list of the routing tiles there are and their sizes, what an offline
    /// area is planned with; a copy is kept in the cache folder. May touch the network: call
    /// it on a worker.
    ///
    /// # Errors
    /// A folder of tiles has nothing to download; the list cannot be had.
    pub fn index(&self) -> Result<std::collections::BTreeMap<azul_routing::TileId, u64>, String> {
        match self {
            Self::Http(tiles) => tiles.index(),
            Self::Folder(_) => Err("the routing tiles are a folder on this computer already".to_string()),
        }
    }

    /// The folder the downloaded tiles are kept in, where offline areas are pinned; `None`
    /// for a folder of tiles (nothing is downloaded).
    #[must_use]
    pub fn cache_dir(&self) -> Option<&Path> {
        match self {
            Self::Http(tiles) => tiles.dir.as_deref(),
            Self::Folder(_) => None,
        }
    }

    /// The routing data's credit, shown next to every route of these tiles: the tile set's
    /// own attribution from its manifest, else OpenStreetMap's, whose data every tile set is
    /// built from. A downloaded manifest is read once and kept for offline use, so this may
    /// touch the network the first time: call it on a worker.
    #[must_use]
    pub fn attribution(&self) -> String {
        let manifest = match self {
            Self::Http(tiles) => tiles.manifest(),
            Self::Folder(tiles) => tiles.manifest(),
        };
        manifest
            .ok()
            .map(|m| m.attribution)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| azul_routing::manifest::OSM_ATTRIBUTION.to_string())
    }
}

/// How big the routing tiles' cache in the data folder may grow.
const CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// How often the worker tells the panel how far it got, at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The tile source a `--routing` value names: the tiles' manifest URL (or a bare URL template
/// with `{z}`, `{x}`, `{y}`) to download from, or a folder (or its `manifest.json`) as
/// azul-routing's `routing-tiles` writes it. Either way behind a cache in memory and in
/// `cache_root` (a folder per source, so another source's tiles never answer), keeping each
/// zoom as long as the tile set suggests.
#[must_use]
pub fn routing_source(template: &str, cache_root: &Path) -> Tiles {
    routing_source_keeping(template, cache_root, None)
}

/// [`routing_source`] with the user's `routing_tiles_keep_days` setting (its text from
/// settings.json: a number of days, or an object per zoom) over what the tile set suggests.
#[must_use]
pub fn routing_source_keeping(template: &str, cache_root: &Path, keep_days: Option<&str>) -> Tiles {
    let template = template.trim();
    let cache = cache_root.join(format!("{:016x}", fnv(template)));
    if template.starts_with("http://") || template.starts_with("https://") {
        // This value's cache is in use; another value's not used for a month goes. On a
        // thread of its own: removing a cache of many tiles may take a moment.
        let (root, current) = (cache_root.to_path_buf(), cache.clone());
        let _ = std::thread::Builder::new()
            .name("azmaps routing cache".into())
            .spawn(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                mark_used(&current, now);
                for gone in prune_unused_caches(&root, &current, now) {
                    eprintln!("[azmaps] removed the unused routing cache {}", gone.display());
                }
            });
        let http = HttpTiles::new(template, user_agent())
            .trusting(&azul_appkit::ui::trusted_roots_pem());
        // The cache keeps a copy of the manifest once one was downloaded: its keep times hold
        // from the start, without asking the network.
        let manifest = Manifest::read_file(&cache.join("manifest.json")).ok();
        let policy = keep_policy(manifest.as_ref(), keep_days);
        Arc::new(RoutingTiles::Http(
            CachedTiles::new(http, Some(cache), CACHE_BYTES).with_policy(policy),
        ))
    } else {
        // A folder is on disk already: the cache only keeps its tiles in memory.
        let root = PathBuf::from(template.strip_prefix("file://").unwrap_or(template));
        let folder = FolderTiles { root };
        let policy = keep_policy(folder.manifest().ok().as_ref(), keep_days);
        Arc::new(RoutingTiles::Folder(
            CachedTiles::new(folder, None, CACHE_BYTES).with_policy(policy),
        ))
    }
}

/// How long the tiles are kept: what `manifest` suggests (the default without one), with the
/// user's setting over it. A setting that does not read as JSON is ignored.
fn keep_policy(manifest: Option<&Manifest>, keep_days: Option<&str>) -> KeepPolicy {
    let policy = manifest.map_or_else(KeepPolicy::default, KeepPolicy::from_manifest);
    match keep_days.and_then(|text| serde_json::from_str::<serde_json::Value>(text.trim()).ok()) {
        Some(value) => {
            let mut settings = serde_json::Map::new();
            settings.insert(azul_routing::source::SETTINGS_KEY.to_string(), value);
            policy.with_settings(&serde_json::Value::Object(settings))
        }
        None => policy,
    }
}

/// The file in a routing cache's folder that says when it was last used (seconds since 1970).
pub const USED_STAMP: &str = ".used";

/// Another routing value's cache unused this long is removed when a value is opened.
const UNUSED_CACHE_SECS: u64 = 30 * 86_400;

/// Notes that the routing cache `cache` is used at `now`.
pub fn mark_used(cache: &Path, now: u64) {
    if std::fs::create_dir_all(cache).is_ok() {
        let _ = std::fs::write(cache.join(USED_STAMP), now.to_string());
    }
}

/// Whether `name` is a routing cache's folder name ([`routing_source`]'s 16 hex digits).
fn is_cache_name(name: &str) -> bool {
    name.len() == 16 && name.chars().all(|c| c.is_ascii_hexdigit())
}

/// When the routing cache `dir` was last used: its stamp, else when the folder last changed.
fn last_used(dir: &Path) -> Option<u64> {
    if let Some(at) = std::fs::read_to_string(dir.join(USED_STAMP))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
    {
        return Some(at);
    }
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Removes the routing caches under `root` other than `current` that were not used for
/// [`UNUSED_CACHE_SECS`] at `now` (a value switched away from kept up to its 256 MB for
/// good). Only a routing cache's folder is touched. Returns the removed folders, sorted.
pub fn prune_unused_caches(root: &Path, current: &Path, now: u64) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == current || !path.is_dir() {
            continue;
        }
        if !entry.file_name().to_str().is_some_and(is_cache_name) {
            continue;
        }
        let used = last_used(&path).unwrap_or(now);
        if now.saturating_sub(used) >= UNUSED_CACHE_SECS && std::fs::remove_dir_all(&path).is_ok() {
            removed.push(path);
        }
    }
    removed.sort();
    removed
}

/// A stable 64-bit hash of `text` (FNV-1a): the cache folder of a tile source.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The road graph's mode for a travel mode; `None` for public transport.
#[must_use]
pub const fn road_mode(mode: TravelMode) -> Option<Mode> {
    match mode {
        TravelMode::Car => Some(Mode::Car),
        TravelMode::Walk => Some(Mode::Walk),
        TravelMode::Bike => Some(Mode::Bike),
        TravelMode::Transit => None,
    }
}

/// One route to work out: what the travel panel asks for.
pub struct RouteRequest {
    /// This request's number; the newest one's is in `latest`.
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: TravelMode,
    /// The routing tiles, else ...
    pub tiles: Option<Tiles>,
    /// ... the routing server; with neither, the straight-line estimate.
    pub online: Option<Online>,
    /// The language the server writes the steps in (`en`, `de`).
    pub language: String,
    /// The newest request's number, shared with the app.
    pub latest: Arc<AtomicU64>,
    /// The write-back that hands the reply to the app, on the UI thread.
    pub on_done: WriteBackCallbackType,
    /// The write-back that hands the worker's progress to the app.
    pub on_progress: WriteBackCallbackType,
}

/// How a route was worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKind {
    /// Along the roads: of the routing tiles, or as the routing server answered.
    Road,
    /// The straight-line estimate: no routing source, or public transport.
    Estimate,
    /// The tiles have no way: the reason, for the panel.
    NoRoute(NoRoute),
}

/// Why the routing tiles gave no way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoRoute {
    /// No road the mode may use near the start ...
    NoRoadAtStart,
    /// ... near the destination ...
    NoRoadAtDestination,
    /// ... or roads at both ends but no way between them.
    NoWay,
    /// The tiles could not be had.
    TilesFailed,
    /// The routing server could not be reached.
    Offline,
    /// The routing server asks to slow down.
    Busy,
    /// The routing server answered something else.
    ServiceFailed,
}

impl NoRoute {
    /// The panel's words for it (a key of AzMaps' resources).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoRoadAtStart => "azmaps-route-no-road-start",
            Self::NoRoadAtDestination => "azmaps-route-no-road-destination",
            Self::NoWay => "azmaps-route-none",
            Self::TilesFailed => "azmaps-route-tiles-failed",
            Self::Offline => "azmaps-route-offline",
            Self::Busy => "azmaps-route-busy",
            Self::ServiceFailed => "azmaps-route-service-failed",
        }
    }

    /// The word scripts read (`AZMAPS_NO_ROUTE <mode> <why>`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::NoRoadAtStart => "no-road-at-start",
            Self::NoRoadAtDestination => "no-road-at-destination",
            Self::NoWay => "no-way",
            Self::TilesFailed => "tiles-failed",
            Self::Offline => "offline",
            Self::Busy => "busy",
            Self::ServiceFailed => "service-failed",
        }
    }
}

/// A route the worker worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteReply {
    /// The request's number.
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: TravelMode,
    /// How long the way is and takes: along the roads for a [`RouteKind::Road`] route, else the
    /// straight-line estimate.
    pub route: RouteEstimate,
    pub kind: RouteKind,
    /// Where it came from.
    pub source: RouteSource,
    /// The routing server's turn-by-turn steps; empty for any other source.
    pub steps: Vec<Step>,
    /// The routing tiles' credit ([`RoutingTiles::attribution`]), shown next to their road
    /// route; empty for any other source (the server's credit is a fixed text).
    pub credit: String,
    /// The route is a kept one, the routing server out of reach: shown marked offline.
    pub offline: bool,
    /// The road route's places, `(lat, lon)`, from where the start meets the roads to where the
    /// destination does; empty for any other kind.
    pub road: Vec<(f64, f64)>,
    /// How many routing tiles the graph read, and how many attempts it took.
    pub tiles: usize,
    pub attempts: usize,
    /// How long the worker worked on it, in ms.
    pub compute_ms: f64,
}

/// How far a route worker got: sent to the panel while it fetches and searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteProgress {
    /// The request's number.
    pub id: u64,
    /// The router's step.
    pub progress: Progress,
}

/// Why a worker stops early: its request is no longer the newest, or the window let go of it.
pub struct Stop<'a> {
    id: u64,
    latest: &'a AtomicU64,
    receiver: &'a mut ThreadReceiver,
}

impl<'a> Stop<'a> {
    /// The checks of the worker of request `id`, against the newest request's number and the
    /// worker's own queue.
    pub fn new(id: u64, latest: &'a AtomicU64, receiver: &'a mut ThreadReceiver) -> Self {
        Self {
            id,
            latest,
            receiver,
        }
    }

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

/// The route worker, an azul `Thread`: reads the [`RouteRequest`], works the route out -
/// sending [`RouteProgress`] to `on_progress` as the tiles arrive - and sends a [`RouteReply`]
/// to `on_done`, unless it was superseded first.
pub extern "C" fn route_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some((asked, latest, on_done, on_progress)) = init.downcast_ref::<RouteRequest>().map(|r| {
        let asked = Asked {
            id: r.id,
            from: r.from,
            to: r.to,
            mode: r.mode,
            tiles: r.tiles.clone(),
            online: r.online.clone(),
            language: r.language.clone(),
        };
        (asked, r.latest.clone(), r.on_done, r.on_progress)
    }) else {
        return;
    };
    let id = asked.id;
    let mut stop = Stop::new(id, &latest, &mut receiver);
    if stop.now() {
        return;
    }
    let mut last_sent: Option<(Instant, Progress)> = None;
    let mut progress = |p: Progress| {
        // Every tile is a step; the panel needs a few a second, and every change of stage.
        let due = last_sent.map_or(true, |(at, last)| {
            at.elapsed() >= PROGRESS_EVERY || last.stage != p.stage || last.attempt != p.attempt
        });
        if due {
            last_sent = Some((Instant::now(), p));
            let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
                on_progress,
                RefAny::new(RouteProgress { id, progress: p }),
            )));
        }
    };
    let Some(reply) = work_out(&asked, &mut progress, &mut || stop.now()) else {
        return;
    };
    if stop.now() {
        return;
    }
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(reply),
    )));
}

/// What a route worker was asked: a [`RouteRequest`] without its write-backs.
pub struct Asked {
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: TravelMode,
    pub tiles: Option<Tiles>,
    pub online: Option<Online>,
    pub language: String,
}

/// Works the route of `asked` out from its source ([`route_source`]), reporting the tiles'
/// progress; `None` when `stop` said to give up first.
pub fn work_out(
    asked: &Asked,
    progress: &mut dyn FnMut(Progress),
    stop: &mut dyn FnMut() -> bool,
) -> Option<RouteReply> {
    let started = Instant::now();
    let source = route_source(asked.tiles.is_some(), asked.online.is_some(), asked.mode);
    let mut reply = RouteReply {
        id: asked.id,
        from: asked.from,
        to: asked.to,
        mode: asked.mode,
        route: estimate_route(asked.from, asked.to, asked.mode),
        kind: RouteKind::Estimate,
        source: RouteSource::Estimate,
        steps: Vec::new(),
        credit: String::new(),
        offline: false,
        road: Vec::new(),
        tiles: 0,
        attempts: 0,
        compute_ms: 0.0,
    };
    let road = road_mode(asked.mode);
    match (source, &asked.tiles, &asked.online, road) {
        (RouteSource::Tiles, Some(tiles), _, Some(road)) => {
            reply.source = RouteSource::Tiles;
            match azul_routing::route(&**tiles, asked.from, asked.to, road, progress, stop) {
                Ok(found) => {
                    reply.route = RouteEstimate {
                        km: found.distance_m / 1000.0,
                        minutes: found.duration_s / 60.0,
                    };
                    reply.kind = RouteKind::Road;
                    reply.road = found.points;
                    reply.tiles = found.tiles;
                    reply.attempts = found.attempts;
                    reply.credit = tiles.attribution();
                }
                Err(RouteError::Stopped) => return None,
                Err(why) => {
                    reply.kind = RouteKind::NoRoute(match why {
                        RouteError::NoRoadNearStart => NoRoute::NoRoadAtStart,
                        RouteError::NoRoadNearDestination => NoRoute::NoRoadAtDestination,
                        RouteError::Fetch(_) => NoRoute::TilesFailed,
                        RouteError::NoRoute | RouteError::Stopped => NoRoute::NoWay,
                    });
                }
            }
            tiles.prune();
        }
        (RouteSource::Online, _, Some(online), Some(road)) => {
            reply.source = RouteSource::Online;
            match online.route_with_origin(&[asked.from, asked.to], road, &asked.language, stop) {
                Ok(Some((found, origin))) => {
                    reply.offline = origin == azul_routing::fetch::Origin::Offline;
                    reply.route = RouteEstimate {
                        km: found.distance_m / 1000.0,
                        minutes: found.duration_s / 60.0,
                    };
                    reply.kind = RouteKind::Road;
                    reply.road = found.points;
                    reply.steps = found.steps;
                    reply.attempts = 1;
                }
                Ok(None) => return None,
                Err(why) => {
                    reply.kind = RouteKind::NoRoute(match why {
                        OnlineError::Offline(_) => NoRoute::Offline,
                        OnlineError::Busy => NoRoute::Busy,
                        OnlineError::NoRoute(_) => NoRoute::NoWay,
                        OnlineError::Server(_) => NoRoute::ServiceFailed,
                    });
                }
            }
        }
        _ => {}
    }
    reply.compute_ms = started.elapsed().as_secs_f64() * 1000.0;
    Some(reply)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use azul_routing::{
        format::{encode, gzip},
        generate::{cut_level, write_to_folder, Access, Highway, Oneway, Way},
        tile::LEVELS,
    };

    use super::*;

    #[test]
    fn the_users_keep_days_go_over_what_the_tile_set_suggests() {
        assert_eq!(keep_policy(None, None), KeepPolicy::default());
        assert_eq!(keep_policy(None, Some("7")).days, [7, 7, 7]);
        assert_eq!(keep_policy(None, Some(r#"{"14": 3}"#)).days, [56, 28, 3]);
        assert_eq!(keep_policy(None, Some("")), KeepPolicy::default(), "cleared: as suggested");
    }

    #[test]
    fn routing_tiles_without_a_manifest_credit_openstreetmap() {
        let dir = std::env::temp_dir().join(format!("azmaps-credit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let tiles = routing_source(dir.to_str().expect("a path"), &std::env::temp_dir());
        assert_eq!(tiles.attribution(), azul_routing::manifest::OSM_ATTRIBUTION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_or_a_url_names_where_the_routing_tiles_come_from() {
        // A folder of its own: opening a URL's source tidies the caches beside its own.
        let cache = std::env::temp_dir().join(format!("azmaps-route-sources-{}", std::process::id()));
        assert!(matches!(*routing_source("/srv/routing", &cache), RoutingTiles::Folder(_)));
        assert!(matches!(*routing_source("file:///srv/routing", &cache), RoutingTiles::Folder(_)));
        assert!(matches!(
            *routing_source("https://example.org/routing/{z}/{x}/{y}.json.gz", &cache),
            RoutingTiles::Http(_)
        ));
        assert_ne!(fnv("https://a.example/{z}"), fnv("https://b.example/{z}"), "a cache per source");
    }

    #[test]
    fn a_route_comes_from_the_tiles_then_the_routing_server_then_the_estimate() {
        assert_eq!(route_source(true, true, TravelMode::Car), RouteSource::Tiles);
        assert_eq!(route_source(false, true, TravelMode::Walk), RouteSource::Online);
        assert_eq!(route_source(false, false, TravelMode::Bike), RouteSource::Estimate);
        assert_eq!(route_source(true, true, TravelMode::Transit), RouteSource::Estimate);
    }

    /// A routing server on a local port that answers every request with `body`: its URL, and
    /// the requests it read.
    fn routing_server(body: String) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let url = format!("http://{}/route", listener.local_addr().expect("an address"));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
                let (mut request, mut length) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    request.push_str(&line);
                }
                let mut sent = vec![0u8; length];
                let _ = reader.read_exact(&mut sent);
                request.push_str(&String::from_utf8_lossy(&sent));
                log.lock().expect("the log").push(request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, seen)
    }

    fn asked(online: Option<Online>, mode: TravelMode) -> Asked {
        Asked {
            id: 7,
            from: (52.5163, 13.3777),
            to: (52.5200, 13.4050),
            mode,
            tiles: None,
            online,
            language: "de".to_string(),
        }
    }

    #[test]
    fn without_tiles_the_worker_asks_the_routing_server_for_the_road_its_time_and_steps() {
        let answer = serde_json::json!({
            "trip": {
                "summary": { "length": 2.4, "time": 420.0 },
                "legs": [{
                    "shape": "_izlhA~rlgdF_{geC~ywl@_kwzCn`{nI",
                    "maneuvers": [
                        { "type": 2, "instruction": "Fahren Sie Richtung Osten.", "length": 1.0, "begin_shape_index": 0, "end_shape_index": 1 },
                        { "type": 15, "instruction": "Biegen Sie links ab.", "length": 1.4, "begin_shape_index": 1, "end_shape_index": 2 },
                        { "type": 4, "instruction": "Sie haben Ihr Ziel erreicht.", "length": 0.0, "begin_shape_index": 2, "end_shape_index": 2 }
                    ]
                }]
            }
        });
        let (url, seen) = routing_server(answer.to_string());
        let reply = work_out(&asked(Some(online_source(&url)), TravelMode::Bike), &mut |_| {}, &mut || false)
            .expect("not stopped");
        assert_eq!(reply.source, RouteSource::Online);
        assert_eq!(reply.kind, RouteKind::Road);
        assert_eq!(reply.road.len(), 3);
        assert!((reply.route.km - 2.4).abs() < 1e-9 && (reply.route.minutes - 7.0).abs() < 1e-9);
        assert_eq!(reply.steps.len(), 3);
        assert_eq!(reply.steps[1].instruction, "Biegen Sie links ab.");
        let seen = seen.lock().expect("the log");
        assert_eq!(seen.len(), 1);
        let request = seen[0].to_ascii_lowercase();
        assert!(request.contains("user-agent: azmaps/"), "{request}");
        assert!(request.contains("\"costing\":\"bicycle\""), "{request}");
        assert!(request.contains("\"language\":\"de-de\""), "{request}");
        // Public transport never asks it.
        let transit = work_out(&asked(Some(online_source(&url)), TravelMode::Transit), &mut |_| {}, &mut || false)
            .expect("not stopped");
        assert_eq!((transit.source, transit.kind), (RouteSource::Estimate, RouteKind::Estimate));
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn an_unreachable_routing_server_is_said_plainly() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let url = format!("http://{}/route", closed.local_addr().expect("an address"));
        drop(closed);
        let reply = work_out(&asked(Some(online_source(&url)), TravelMode::Car), &mut |_| {}, &mut || false)
            .expect("not stopped");
        assert_eq!(reply.kind, RouteKind::NoRoute(NoRoute::Offline));
        assert_eq!(NoRoute::Offline.label(), "azmaps-route-offline");
    }

    #[test]
    fn public_transport_is_a_timetable_question_not_a_road_graph() {
        assert_eq!(road_mode(TravelMode::Transit), None);
        assert_eq!(road_mode(TravelMode::Car), Some(Mode::Car));
        assert_eq!(road_mode(TravelMode::Walk), Some(Mode::Walk));
        assert_eq!(road_mode(TravelMode::Bike), Some(Mode::Bike));
    }

    #[test]
    fn a_route_is_worked_out_on_a_folder_the_tile_generator_wrote() {
        let dir = std::env::temp_dir().join(format!("azmaps-routing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let coords: HashMap<i64, (f64, f64)> =
            [(1, (43.7300, 7.4200)), (2, (43.7300, 7.4300)), (3, (43.7380, 7.4300))].into_iter().collect();
        let street = Way {
            id: 10,
            nodes: vec![1, 2, 3],
            highway: Highway::Residential,
            oneway: Oneway::No,
            bicycle_both_ways: false,
            maxspeed_kmh: None,
            access: Access::default(),
            name: None,
        };
        for z in LEVELS {
            for (tile, data) in cut_level(&[street.clone()], &coords, z) {
                let bytes = gzip(&encode(&data));
                write_to_folder(&dir, tile, &bytes).expect("a tile written");
            }
        }
        let tiles = routing_source(dir.to_str().expect("a path"), &std::env::temp_dir());
        let found = azul_routing::route(
            &*tiles,
            (43.7300, 7.4200),
            (43.7380, 7.4300),
            Mode::Car,
            &mut |_| {},
            &mut || false,
        )
        .expect("a road route");
        assert!(found.distance_m > 1_600.0, "around the corner: {} m", found.distance_m);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_caches_of_routing_values_not_used_for_a_month_are_removed() {
        let root = std::env::temp_dir().join(format!("azmaps-route-caches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let now = 2_000_000_000_u64;
        let day = 86_400_u64;
        let folder = |name: &str, used: Option<u64>| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).expect("a folder");
            std::fs::write(dir.join("tile.bin"), b"tile").expect("a tile");
            if let Some(at) = used {
                std::fs::write(dir.join(USED_STAMP), at.to_string()).expect("a stamp");
            }
            dir
        };
        let current = folder("00000000000000aa", Some(now - 90 * day));
        let old = folder("00000000000000bb", Some(now - 31 * day));
        let recent = folder("00000000000000cc", Some(now - 2 * day));
        let not_a_cache = folder("notes", Some(now - 400 * day));
        let removed = prune_unused_caches(&root, &current, now);
        assert_eq!(removed, vec![old.clone()]);
        assert!(!old.exists(), "a month unused: gone");
        assert!(current.exists(), "the one in use stays, however old its stamp");
        assert!(recent.exists(), "used two days ago: kept for a switch back");
        assert!(not_a_cache.exists(), "only a routing cache's folder is removed");
        mark_used(&current, now);
        assert_eq!(
            std::fs::read_to_string(current.join(USED_STAMP)).expect("the stamp"),
            now.to_string()
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
