//! One route, start to destination: the corridor's tiles fetched in
//! parallel, read into the graph, both ends snapped to a road, A* over the
//! graph, and the corridor widened when there is no way.

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

use crate::{
    astar::astar,
    corridor::{corridor, ATTEMPTS},
    distance_m,
    format::parse_tile,
    graph::{Graph, Snap},
    mode::Mode,
    tile::TileId,
};

/// Where the tiles come from: a download, a cache, a folder.
pub trait TileSource: Sync {
    /// The bytes of `tile` (JSON, or gzipped JSON); `Ok(None)` when there is
    /// no such tile because it holds no road.
    ///
    /// # Errors
    /// The tile could not be had (the network, a broken cache file).
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String>;
}

/// How far from a road a start or a destination may be, metres: further
/// than this it has no road at all.
const MAX_SNAP_M: f64 = 2_000.0;

/// How many tiles are fetched at once.
pub const FETCH_THREADS: usize = 8;

/// What the router is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Fetching the corridor's tiles that are not read yet.
    Fetching,
    /// Searching the graph for the way.
    Searching,
}

/// A step of the router's progress, for the app to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// The attempt, from 0: each one widens the corridor.
    pub attempt: usize,
    pub stage: Stage,
    /// The corridor's tiles read so far ...
    pub tiles_done: usize,
    /// ... of this many.
    pub tiles_wanted: usize,
}

/// A road route.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The way, `(lat, lon)`, from the road point nearest the start to the
    /// road point nearest the destination.
    pub points: Vec<(f64, f64)>,
    /// Its length, metres.
    pub distance_m: f64,
    /// The time it takes, seconds.
    pub duration_s: f64,
    /// How many tiles the graph read.
    pub tiles: usize,
    /// How many nodes A* expanded, over all attempts.
    pub expanded: usize,
    /// The attempts it took, from 1.
    pub attempts: usize,
}

/// Why there is no route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// No road the mode may use near the start.
    NoRoadNearStart,
    /// No road the mode may use near the destination.
    NoRoadNearDestination,
    /// Roads at both ends, but no way between them in the widest corridor.
    NoRoute,
    /// Not one tile could be had: the reason of the first that failed.
    Fetch(String),
    /// The caller stopped it.
    Stopped,
}

/// The node ids of the start and the destination, where they meet a road.
/// OpenStreetMap's ids are positive, so these are never real nodes.
const START: i64 = -1;
const GOAL: i64 = -2;

/// Works out the road route from `from` to `to` (both `(lat, lon)`) for
/// `mode` on the tiles of `source`, reporting each step to `progress`.
/// `stop` is asked between tiles and during the search; when it says so the
/// route ends with [`RouteError::Stopped`].
///
/// # Errors
/// See [`RouteError`].
pub fn route(
    source: &dyn TileSource,
    from: (f64, f64),
    to: (f64, f64),
    mode: Mode,
    progress: &mut dyn FnMut(Progress),
    stop: &mut dyn FnMut() -> bool,
) -> Result<Route, RouteError> {
    let mut graph = Graph::new(mode);
    let mut expanded = 0usize;
    let mut first_error: Option<String> = None;
    let mut any_tile = false;
    for attempt in 0..ATTEMPTS {
        let wanted = corridor(from, to, attempt);
        let missing: Vec<TileId> = wanted.iter().copied().filter(|t| !graph.has_tile(*t)).collect();
        let mut done = wanted.len() - missing.len();
        progress(Progress {
            attempt,
            stage: Stage::Fetching,
            tiles_done: done,
            tiles_wanted: wanted.len(),
        });
        let mut stopped = false;
        fetch_parallel(source, &missing, FETCH_THREADS, &mut |tile, result| {
            match result {
                Ok(Some(bytes)) => match parse_tile(&bytes) {
                    Ok(data) => {
                        graph.add_tile(tile, &data);
                        any_tile = true;
                    }
                    Err(e) => {
                        if first_error.is_none() {
                            first_error = Some(format!("tile {}: {e}", tile.path()));
                        }
                    }
                },
                Ok(None) => {
                    graph.mark_empty(tile);
                    any_tile = true;
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(format!("tile {}: {e}", tile.path()));
                    }
                }
            }
            done += 1;
            progress(Progress {
                attempt,
                stage: Stage::Fetching,
                tiles_done: done,
                tiles_wanted: wanted.len(),
            });
            if stop() {
                stopped = true;
                return false;
            }
            true
        });
        if stopped {
            return Err(RouteError::Stopped);
        }
        if !any_tile {
            return Err(RouteError::Fetch(
                first_error.unwrap_or_else(|| "no tiles".to_string()),
            ));
        }
        let last = attempt + 1 == ATTEMPTS;
        let Some(start) = graph.snap(from, MAX_SNAP_M) else {
            if last {
                return Err(RouteError::NoRoadNearStart);
            }
            continue;
        };
        let Some(goal) = graph.snap(to, MAX_SNAP_M) else {
            if last {
                return Err(RouteError::NoRoadNearDestination);
            }
            continue;
        };
        progress(Progress {
            attempt,
            stage: Stage::Searching,
            tiles_done: done,
            tiles_wanted: wanted.len(),
        });
        let mut stopped = false;
        let found = search(&graph, start, goal, &mut || {
            stopped = stopped || stop();
            stopped
        });
        if stopped {
            return Err(RouteError::Stopped);
        }
        if let Some((points, cost, n)) = found {
            expanded += n;
            let distance = points.windows(2).map(|w| distance_m(w[0], w[1])).sum();
            #[allow(clippy::cast_precision_loss)] // ms of a trip
            let duration_s = cost as f64 / 1000.0;
            return Ok(Route {
                points,
                distance_m: distance,
                duration_s,
                tiles: graph.tile_count(),
                expanded,
                attempts: attempt + 1,
            });
        }
    }
    Err(RouteError::NoRoute)
}

/// A* from the start's road point to the destination's: the way's points
/// and its cost in ms, plus the nodes expanded; `None` without a way.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)] // ms of a trip
fn search(
    graph: &Graph,
    start: Snap,
    goal: Snap,
    stop: &mut dyn FnMut() -> bool,
) -> Option<(Vec<(f64, f64)>, u64, usize)> {
    let segments = graph.segments();
    let (s, g) = (segments[start.segment], segments[goal.segment]);
    let part = |ms: Option<u64>, fraction: f64| {
        ms.map(|ms| ((ms as f64 * fraction).round() as u64).max(1))
    };
    // The start leaves along its segment both ways it may go ...
    let mut from_start: Vec<(i64, u64)> = Vec::new();
    if let Some(ms) = part(s.forward_ms, 1.0 - start.t) {
        from_start.push((s.b, ms));
    }
    if let Some(ms) = part(s.backward_ms, start.t) {
        from_start.push((s.a, ms));
    }
    // ... and the destination is reached from its segment's ends.
    let into_goal_from_a = part(g.forward_ms, goal.t);
    let into_goal_from_b = part(g.backward_ms, 1.0 - goal.t);
    // Both on one segment: straight there, if the way along it allows.
    if start.segment == goal.segment {
        let direct = if goal.t >= start.t {
            part(s.forward_ms, goal.t - start.t)
        } else {
            part(s.backward_ms, start.t - goal.t)
        };
        if let Some(ms) = direct {
            from_start.push((GOAL, ms));
        }
    }
    let coord = |n: i64| match n {
        START => Some(start.point),
        GOAL => Some(goal.point),
        _ => graph.coord(n),
    };
    let top_speed = f64::from(graph.mode().top_speed_kmh()) / 3.6;
    let found = astar(
        START,
        GOAL,
        |n: i64| {
            let mut next: Vec<(i64, u64)> = if n == START {
                from_start.clone()
            } else {
                graph.edges_from(n).to_vec()
            };
            if n == g.a {
                if let Some(ms) = into_goal_from_a {
                    next.push((GOAL, ms));
                }
            }
            if n == g.b {
                if let Some(ms) = into_goal_from_b {
                    next.push((GOAL, ms));
                }
            }
            next
        },
        |n: i64| {
            // Straight there at the mode's top speed: never more than the
            // true time, so the way found is the fastest.
            coord(n).map_or(0, |c| {
                crate::graph::travel_ms(distance_m(c, goal.point), top_speed).saturating_sub(1)
            })
        },
        stop,
    )?;
    let points = found.path.iter().filter_map(|n| coord(*n)).collect();
    Some((points, found.cost, found.expanded))
}

/// Fetches `tiles` from `source` on `threads` threads, in order of the list
/// as far as the threads allow, and hands each answer to `on_tile` on the
/// calling thread as it arrives. `on_tile` returns `false` to stop: no tile
/// is started after that, and the ones in flight are dropped.
pub fn fetch_parallel(
    source: &dyn TileSource,
    tiles: &[TileId],
    threads: usize,
    on_tile: &mut dyn FnMut(TileId, Result<Option<Vec<u8>>, String>) -> bool,
) {
    if tiles.is_empty() {
        return;
    }
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let (next, cancelled) = (&next, &cancelled);
        for _ in 0..threads.clamp(1, 32).min(tiles.len()) {
            let tx = tx.clone();
            scope.spawn(move || loop {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(&tile) = tiles.get(index) else {
                    break;
                };
                if tx.send((tile, source.fetch(tile))).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (tile, result) in rx.iter() {
            if !on_tile(tile, result) {
                cancelled.store(true, Ordering::Relaxed);
                break;
            }
        }
    });
}
