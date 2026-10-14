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
//! every tile that arrives is reported to the panel. Without tiles, and for public transport
//! (a timetable question, not a road graph), it answers the straight-line estimate
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
    source::{CachedTiles, FolderTiles, HttpTiles},
    Mode, Progress, RouteError, TileSource,
};

use crate::model::{estimate_route, RouteEstimate, TravelMode};

/// Where the routing tiles come from, shared by every route worker of the session: one cache in
/// memory, one connection pool.
pub type Tiles = Arc<RoutingTiles>;

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
}

/// How big the routing tiles' cache in the data folder may grow.
const CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// How often the worker tells the panel how far it got, at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The tile source a `--routing` value names: a URL template (`{z}`, `{x}`, `{y}`) to download
/// from, or a folder as azul-routing's `routing-tiles` writes it. Either way behind a cache in
/// memory and in `cache_root` (a folder per source, so another source's tiles never answer).
#[must_use]
pub fn routing_source(template: &str, cache_root: &Path) -> Tiles {
    let template = template.trim();
    let cache = cache_root.join(format!("{:016x}", fnv(template)));
    if template.starts_with("http://") || template.starts_with("https://") {
        // The app and its version, nothing about the user.
        let agent = format!("AzMaps/{} (+https://github.com/fschutt/azul)", env!("CARGO_PKG_VERSION"));
        let http = HttpTiles::new(template, agent);
        Arc::new(RoutingTiles::Http(CachedTiles::new(http, Some(cache), CACHE_BYTES)))
    } else {
        // A folder is on disk already: the cache only keeps its tiles in memory.
        let root = PathBuf::from(template.strip_prefix("file://").unwrap_or(template));
        Arc::new(RoutingTiles::Folder(CachedTiles::new(FolderTiles { root }, None, CACHE_BYTES)))
    }
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
    /// The routing tiles; `None` answers the straight-line estimate.
    pub tiles: Option<Tiles>,
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
    /// Along the roads of the routing tiles.
    Road,
    /// The straight-line estimate: no routing tiles, or public transport.
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
    let Some((id, from, to, mode, tiles, latest, on_done, on_progress)) =
        init.downcast_ref::<RouteRequest>().map(|r| {
            (
                r.id,
                r.from,
                r.to,
                r.mode,
                r.tiles.clone(),
                r.latest.clone(),
                r.on_done,
                r.on_progress,
            )
        })
    else {
        return;
    };
    let mut stop = Stop::new(id, &latest, &mut receiver);
    if stop.now() {
        return;
    }
    let started = Instant::now();
    let estimate = estimate_route(from, to, mode);
    let mut reply = RouteReply {
        id,
        from,
        to,
        mode,
        route: estimate,
        kind: RouteKind::Estimate,
        road: Vec::new(),
        tiles: 0,
        attempts: 0,
        compute_ms: 0.0,
    };
    if let (Some(tiles), Some(road)) = (tiles, road_mode(mode)) {
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
        match azul_routing::route(&*tiles, from, to, road, &mut progress, &mut || stop.now()) {
            Ok(found) => {
                reply.route = RouteEstimate {
                    km: found.distance_m / 1000.0,
                    minutes: found.duration_s / 60.0,
                };
                reply.kind = RouteKind::Road;
                reply.road = found.points;
                reply.tiles = found.tiles;
                reply.attempts = found.attempts;
            }
            Err(RouteError::Stopped) => return,
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
    reply.compute_ms = started.elapsed().as_secs_f64() * 1000.0;
    if stop.now() {
        return;
    }
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(reply),
    )));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use azul_routing::{
        format::{Access, Highway, Oneway, Way},
        generate::{cut_level, gzip, write_to_folder},
        format::write_tile,
        tile::LEVELS,
    };

    use super::*;

    #[test]
    fn a_folder_or_a_url_names_where_the_routing_tiles_come_from() {
        let cache = std::env::temp_dir();
        assert!(matches!(*routing_source("/srv/routing", &cache), RoutingTiles::Folder(_)));
        assert!(matches!(*routing_source("file:///srv/routing", &cache), RoutingTiles::Folder(_)));
        assert!(matches!(
            *routing_source("https://example.org/routing/{z}/{x}/{y}.json.gz", &cache),
            RoutingTiles::Http(_)
        ));
        assert_ne!(fnv("https://a.example/{z}"), fnv("https://b.example/{z}"), "a cache per source");
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
                let bytes = gzip(write_tile(tile, &data).as_bytes()).expect("gzip");
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
}
