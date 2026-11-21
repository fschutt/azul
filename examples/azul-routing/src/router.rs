//! One route, start to destination, on tiles read as the search reaches
//! them.
//!
//! The search runs over edges rather than nodes, because a turn restriction
//! is about the edge you came from: "from this street you may not turn left
//! into that one". It starts on the edge nearest the start and stops on the
//! edge nearest the destination, partway along both. The costs are
//! Valhalla's ([`crate::mode`]).
//!
//! Tiles are read lazily. Whenever the search reaches a node, the tile
//! around it is read if it is not there yet, at the zoom its distance from
//! the trip's ends calls for: zoom 14 (every road) near the ends, zoom 11
//! (arterial roads) further out, zoom 8 (highways) beyond. A car keeps to
//! Valhalla's own limits (local roads within 5 km of an end, arterial roads
//! within 20 km), which also keeps a city trip from reading the whole city;
//! a bicycle or a walker uses every road on trips up to 15 km.
//!
//! The tiles the search will most likely read - around the ends and along
//! the straight line between them ([`corridor`]) - are fetched first, in
//! parallel, so that the downloads' round trips overlap. They only wait to
//! be read: a tile joins the graph when the search reaches it, fetched ahead
//! or not, so the route is the same either way.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

use crate::{
    corridor::corridor,
    distance_m,
    format::{decode, edge_key, flag},
    graph::{Graph, RoadEdge, Snap},
    mode::{Mode, Turn, CAR_MIN_FACTOR},
    tile::{around, tile_of, TileId},
};

/// Where the tiles come from: a download, a cache, a folder.
pub trait TileSource: Sync {
    /// The bytes of `tile` (gzipped protobuf, or plain); `Ok(None)` when the
    /// hosting has no such tile because no road is in it.
    ///
    /// # Errors
    /// The tile could not be had (the network, a broken cache file).
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String>;
}

/// How many tiles are fetched at once.
pub const FETCH_THREADS: usize = 8;

/// What the router is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Fetching the corridor's tiles ahead of the search.
    Fetching,
    /// Searching, and reading the tiles it reaches.
    Searching,
}

/// A step of the router's progress, for the app to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// The attempt, from 0: the second one lets every road reach four times
    /// further from the ends.
    pub attempt: usize,
    /// What the router is doing.
    pub stage: Stage,
    /// The tiles answered so far ...
    pub tiles_done: usize,
    /// ... of this many: the corridor's, and every tile the search fetched
    /// beyond it.
    pub tiles_wanted: usize,
}

/// Limits of one route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// How far from a road a start or destination may be, metres.
    pub max_snap_m: f64,
    /// How many tiles one route may read.
    pub max_tiles: usize,
    /// How many edges one search may expand.
    pub max_expanded: usize,
    /// How many threads fetch the corridor ahead of the search; 0 fetches
    /// every tile only when the search reaches it.
    pub prefetch_threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_snap_m: 2_000.0,
            max_tiles: 3_000,
            max_expanded: 3_000_000,
            prefetch_threads: FETCH_THREADS,
        }
    }
}

/// A road route.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The way, `(lat, lon)`, from the road point nearest the start to the
    /// one nearest the destination.
    pub points: Vec<(f64, f64)>,
    /// Its length, metres.
    pub distance_m: f64,
    /// The time it takes, seconds, turns and gates included.
    pub duration_s: f64,
    /// The keys of the edges it takes, in order.
    pub edges: Vec<u64>,
    /// The street names along it, each once in a row: the directions list.
    pub names: Vec<String>,
    /// How many tiles the graph read.
    pub tiles: usize,
    /// How many edges the search expanded, over all attempts.
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
    /// Roads at both ends, but no way between them within the limits.
    NoRoute,
    /// Not one tile could be had: the reason of the first that failed.
    Fetch(String),
    /// The caller stopped it.
    Stopped,
}

/// A place this close to a node is at the node: a route may start along any
/// road leaving it, and end on any road arriving (Valhalla's default
/// `node_snap_tolerance`).
const NODE_SNAP_M: f64 = 5.0;

/// A start or destination is only put on a road from which (or into which)
/// at least this many edges lead, so it never lands on an island the route
/// cannot leave (Valhalla's default `minimum_reachability`).
const MIN_REACH: usize = 50;

/// A bicycle or walking trip shorter than this is searched on zoom 14 all
/// along.
const SHORT_TRIP_M: f64 = 15_000.0;
/// How far from an end a car uses local roads, and arterial roads:
/// Valhalla's default hierarchy limits for cars (`expand_within_distance`).
const CAR_LOCAL_M: f64 = 5_000.0;
const CAR_ARTERIAL_M: f64 = 20_000.0;

/// How many edges the search expands between two questions whether to stop.
const STOP_EVERY: usize = 1024;

/// Works out the road route from `from` to `to` (both `(lat, lon)`) for
/// `mode` on the tiles of `source`, with the default [`Options`], reporting
/// each step to `progress`. `stop` is asked between tiles and during the
/// search; when it says so the route ends with [`RouteError::Stopped`].
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
    route_with(source, from, to, mode, Options::default(), progress, stop)
}

/// [`route`] with other limits.
///
/// # Errors
/// See [`RouteError`].
pub fn route_with(
    source: &dyn TileSource,
    from: (f64, f64),
    to: (f64, f64),
    mode: Mode,
    options: Options,
    progress: &mut dyn FnMut(Progress),
    stop: &mut dyn FnMut() -> bool,
) -> Result<Route, RouteError> {
    let rule = ZoomRule {
        mode,
        from,
        to,
        trip_m: distance_m(from, to),
        scale: 1.0,
    };
    let mut search = Search {
        source,
        graph: Graph::new(),
        rule,
        ready: HashMap::new(),
        first_error: None,
        any_tile: false,
        max_tiles: options.max_tiles,
        attempt: 0,
        done: 0,
        wanted: 0,
        expanded: 0,
    };
    if options.prefetch_threads > 0 {
        let likely = corridor(from, to, &|at| rule.zoom_at(at));
        search.prefetch(&likely, options.prefetch_threads, progress, stop)?;
    }
    for end in [from, to] {
        for tile in around(tile_of(end.0, end.1, 14), 1) {
            search.load(tile);
        }
    }
    progress(search.progress(Stage::Searching));
    if stop() {
        return Err(RouteError::Stopped);
    }
    if !search.any_tile {
        return Err(RouteError::Fetch(search.first_error.unwrap_or_else(|| "no tiles".into())));
    }
    let start = search.snap(from, mode, options.max_snap_m, true).ok_or(RouteError::NoRoadNearStart)?;
    let goal = search.snap(to, mode, options.max_snap_m, false).ok_or(RouteError::NoRoadNearDestination)?;
    // A long trip first keeps to main roads away from its ends; when that
    // finds nothing (the only bridge is a small road), it looks again with
    // every road four times further out.
    for (attempt, scale) in [1.0, 4.0].into_iter().enumerate() {
        search.rule.scale = scale;
        search.attempt = attempt;
        if attempt > 0 {
            progress(search.progress(Stage::Searching));
        }
        if let Some(found) = search.astar(start, goal, options.max_expanded, progress, stop)? {
            return Ok(found);
        }
        if mode != Mode::Car && search.rule.trip_m < SHORT_TRIP_M {
            break;
        }
    }
    Err(RouteError::NoRoute)
}

/// Which zoom the search reads at a place: fine near the trip's ends,
/// coarse between them.
#[derive(Debug, Clone, Copy)]
struct ZoomRule {
    /// The travel mode: a car keeps to Valhalla's hierarchy limits.
    mode: Mode,
    /// Where the trip starts ...
    from: (f64, f64),
    /// ... and ends.
    to: (f64, f64),
    /// The straight distance between them, metres.
    trip_m: f64,
    /// How much further than usual the fine zooms reach: 1, or 4 on the
    /// second attempt.
    scale: f64,
}

impl ZoomRule {
    /// The zoom whose tiles the search reads at `at`.
    fn zoom_at(&self, at: (f64, f64)) -> u8 {
        let (fine, middle) = if self.mode == Mode::Car {
            (CAR_LOCAL_M * self.scale, CAR_ARTERIAL_M * self.scale)
        } else if self.trip_m < SHORT_TRIP_M {
            return 14;
        } else {
            (
                (self.trip_m * 0.1).clamp(3_000.0, 15_000.0) * self.scale,
                (self.trip_m * 0.3).clamp(30_000.0, 100_000.0) * self.scale,
            )
        };
        let d = distance_m(at, self.from).min(distance_m(at, self.to));
        if d <= fine {
            14
        } else if d <= middle {
            11
        } else {
            8
        }
    }
}

/// The answer of a tile source for one tile.
type Answer = Result<Option<Vec<u8>>, String>;

/// The state of one route's search.
struct Search<'a> {
    /// Where the tiles come from.
    source: &'a dyn TileSource,
    /// Every tile read so far.
    graph: Graph,
    /// Which zoom the search reads where.
    rule: ZoomRule,
    /// The corridor's tiles fetched ahead, waiting until the search reaches
    /// them: reading them earlier would change which roads the search sees.
    ready: HashMap<TileId, Answer>,
    /// Why the first tile that failed failed, for [`RouteError::Fetch`].
    first_error: Option<String>,
    /// Whether any tile was had at all (one without roads counts).
    any_tile: bool,
    /// How many tiles the graph may read.
    max_tiles: usize,
    /// The attempt the search is in, for [`Progress`].
    attempt: usize,
    /// The tiles answered so far, for [`Progress`].
    done: usize,
    /// The tiles asked for so far, for [`Progress`].
    wanted: usize,
    /// The edges expanded by the attempts so far.
    expanded: usize,
}

/// The best way found to the end of one edge.
#[derive(Debug, Clone, Copy)]
struct Label {
    /// What the search minimises: time plus penalties, ms.
    cost: u64,
    /// The travel time, ms.
    time: u64,
    /// The length, metres.
    metres: f64,
    /// The edge before, `None` on a start edge.
    pred: Option<u32>,
}

/// The way found into the destination's edge.
#[derive(Debug, Clone, Copy)]
struct GoalLabel {
    /// Time plus penalties, ms.
    cost: u64,
    /// The travel time, ms.
    time: u64,
    /// The length, metres.
    metres: f64,
    /// The destination's edge.
    edge: u32,
    /// The edge before it (`None` when the start is on the same edge, before
    /// the destination).
    pred: Option<u32>,
}

impl Search<'_> {
    /// Where the router is, as [`Progress`].
    fn progress(&self, stage: Stage) -> Progress {
        Progress {
            attempt: self.attempt,
            stage,
            tiles_done: self.done,
            tiles_wanted: self.wanted.max(self.done),
        }
    }

    /// Fetches `tiles` on `threads` threads and keeps the answers until the
    /// search reaches them.
    fn prefetch(
        &mut self,
        tiles: &[TileId],
        threads: usize,
        progress: &mut dyn FnMut(Progress),
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), RouteError> {
        self.wanted += tiles.len();
        progress(self.progress(Stage::Fetching));
        let (attempt, wanted, source) = (self.attempt, self.wanted, self.source);
        let mut done = self.done;
        let mut stopped = false;
        let ready = &mut self.ready;
        fetch_parallel(source, tiles, threads, &mut |tile, answer| {
            ready.insert(tile, answer);
            done += 1;
            progress(Progress {
                attempt,
                stage: Stage::Fetching,
                tiles_done: done,
                tiles_wanted: wanted,
            });
            stopped = stop();
            !stopped
        });
        self.done = done;
        if stopped {
            return Err(RouteError::Stopped);
        }
        Ok(())
    }

    /// The nearest road to `place` that is not an island: one from which
    /// (`leaving`) or into which enough edges lead, in either direction. When
    /// no road near reaches that far (a small network), the one that reaches
    /// furthest, the nearer of equals.
    fn snap(&self, place: (f64, f64), mode: Mode, max_m: f64, leaving: bool) -> Option<Snap> {
        let mut best: Option<(usize, Snap)> = None;
        for s in self.graph.snaps(place, mode, max_m) {
            let reach = |e: u32| self.graph.reach(e, mode, leaving, MIN_REACH);
            let twin = self.graph.twin(s.edge).filter(|&t| mode.may_use(&self.graph.edge(t).data));
            let reached = reach(s.edge).max(twin.map_or(0, reach));
            if reached >= MIN_REACH {
                return Some(s);
            }
            if best.is_none_or(|(r, _)| reached > r) {
                best = Some((reached, s));
            }
        }
        best.map(|(_, s)| s)
    }

    /// Reads `tile` into the graph once: from the tiles fetched ahead, else
    /// from the source. A tile that fails is noted and skipped. Returns
    /// whether the source was asked just now.
    fn load(&mut self, tile: TileId) -> bool {
        if self.graph.has_tile(tile) || self.graph.tile_count() >= self.max_tiles {
            return false;
        }
        let (answer, fetched) = match self.ready.remove(&tile) {
            Some(answer) => (answer, false),
            None => (self.source.fetch(tile), true),
        };
        if fetched {
            self.done += 1;
            self.wanted += 1;
        }
        match answer {
            Ok(Some(bytes)) => match decode(&bytes) {
                Ok(data) => {
                    self.graph.add_tile(&data);
                    self.any_tile = true;
                }
                Err(e) => self.fail(tile, &e),
            },
            Ok(None) => {
                self.graph.mark_empty(tile);
                self.any_tile = true;
            }
            Err(e) => self.fail(tile, &e),
        }
        fetched
    }

    /// Notes a tile that could not be had or read, and leaves it out.
    fn fail(&mut self, tile: TileId, why: &str) {
        self.graph.mark_empty(tile);
        self.first_error.get_or_insert_with(|| format!("tile {}: {why}", tile.path()));
    }

    /// Whether a restriction over several edges forbids going on into edge
    /// `next` after edge `prev` (whose way back the labels hold).
    fn complex_forbids(&self, mode: Mode, next: &RoadEdge, prev: u32, labels: &HashMap<u32, Label>) -> bool {
        self.graph.restrictions_into(next.key).iter().any(|r| {
            if r.modes & mode.access_bit() == 0 || r.timed {
                return false;
            }
            let mut at = Some(prev);
            for &via in &r.vias {
                match at {
                    Some(e) if self.graph.edge(e).key == via => at = labels.get(&e).and_then(|l| l.pred),
                    _ => return false,
                }
            }
            at.is_some_and(|e| self.graph.edge(e).key == r.from_edge)
        })
    }

    /// The turn from a road arriving with heading `arriving` onto edge
    /// `next`, at a node where the edges `roads` leave (`back` is the key of
    /// the one going back the way the car came).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn turn_at(&self, arriving: f64, next: u32, back: u64, roads: &[u32], node_info: u8) -> Turn {
        let out = self.graph.heading_out(next);
        let degree = (out - arriving + 360.0).round() as u32 % 360;
        // A road to the right lies clockwise between the way out and the way
        // back; one to the left between the way back and the way out.
        let back_angle = (arriving + 180.0 - out + 360.0) % 360.0;
        let (mut right, mut left) = (false, false);
        for &other in roads {
            let e = self.graph.edge(other);
            if other == next || e.key == back {
                continue;
            }
            let angle = (self.graph.heading_out(other) - out + 360.0) % 360.0;
            if angle > 0.0 && angle < back_angle {
                right = true;
            } else if angle > back_angle {
                left = true;
            }
        }
        Turn { degree, roads: roads.len(), crossing: right && left, node_info }
    }

    /// A* over the edges, from the start's edge to the destination's,
    /// reading tiles as it goes; `Ok(None)` when there is no way within the
    /// limits.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn astar(
        &mut self,
        start: Snap,
        goal: Snap,
        max_expanded: usize,
        progress: &mut dyn FnMut(Progress),
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<Route>, RouteError> {
        let mode = self.rule.mode;
        // The edges a route may start on (or end on) and how far along: the
        // snapped edge both ways, or at a node every road there.
        let graph = &self.graph;
        let usable = |e: &u32| mode.may_use(&graph.edge(*e).data);
        let ends = |snap: Snap, leaving: bool| -> Vec<(u32, f64)> {
            let e = graph.edge(snap.edge);
            let along = f64::from(e.data.length_m) * snap.t;
            let node = if along <= NODE_SNAP_M {
                Some(e.from)
            } else if f64::from(e.data.length_m) - along <= NODE_SNAP_M {
                Some(e.to)
            } else {
                None
            };
            let mut out = vec![(snap.edge, snap.t)];
            out.extend(graph.twin(snap.edge).filter(usable).map(|twin| (twin, 1.0 - snap.t)));
            if let Some(node) = node {
                for &road in graph.edges_from(node) {
                    if leaving {
                        out.extend(Some(road).filter(usable).map(|r| (r, 0.0)));
                    } else {
                        out.extend(graph.twin(road).filter(usable).map(|r| (r, 1.0)));
                    }
                }
            }
            out
        };
        let starts: HashMap<u32, f64> = ends(start, true).into_iter().collect();
        let goals: HashMap<u32, f64> = ends(goal, false).into_iter().collect();
        let top_speed = mode.top_speed_kmh() / 3.6;
        let goal_point = goal.point;
        // The cost left can be no less than the straight way at top speed,
        // at the cheapest a car's cost per second gets.
        let cheapest = if mode == Mode::Car { CAR_MIN_FACTOR } else { 1.0 };
        let estimate = |graph: &Graph, node: u64| {
            graph.node(node).map_or(0, |n| (distance_m(n.at, goal_point) / top_speed * 1000.0 * cheapest) as u64)
        };
        // Part of an edge: its time, its cost and its metres.
        let part = |e: &RoadEdge, fraction: f64| {
            let metres = f64::from(e.data.length_m) * fraction.clamp(0.0, 1.0);
            let time = mode.time_ms(&e.data, metres);
            (time, mode.edge_cost(&e.data, time), metres)
        };

        let mut labels: HashMap<u32, Label> = HashMap::new();
        let mut best_goal: Option<GoalLabel> = None;
        // (estimated total, cost so far, edge, whether it is the goal)
        let mut open: BinaryHeap<Reverse<(u64, u64, u32, bool)>> = BinaryHeap::new();
        let offer_goal = |best: &mut Option<GoalLabel>, open: &mut BinaryHeap<_>, candidate: GoalLabel| {
            if best.is_none_or(|b: GoalLabel| candidate.cost < b.cost) {
                *best = Some(candidate);
                open.push(Reverse((candidate.cost, candidate.cost, candidate.edge, true)));
            }
        };
        for (&edge, &t) in &starts {
            let e = self.graph.edge(edge);
            let (time, cost, metres) = part(e, 1.0 - t);
            let cost = cost + mode.penalty_ms(None, &e.data, false);
            labels.insert(edge, Label { cost, time, metres, pred: None });
            open.push(Reverse((cost + estimate(&self.graph, e.to), cost, edge, false)));
            if let Some(&tg) = goals.get(&edge).filter(|&&tg| tg >= t) {
                let (time, cost, metres) = part(e, tg - t);
                let cost = cost + mode.penalty_ms(None, &e.data, false);
                offer_goal(&mut best_goal, &mut open, GoalLabel { cost, time, metres, edge, pred: None });
            }
        }

        let mut expanded = 0usize;
        let found = loop {
            let Some(Reverse((_, cost, edge, is_goal))) = open.pop() else {
                self.expanded += expanded;
                return Ok(None);
            };
            if is_goal {
                match best_goal {
                    Some(b) if b.cost == cost && b.edge == edge => break b,
                    _ => continue,
                }
            }
            let label = labels[&edge];
            if label.cost < cost {
                continue; // reached cheaper since
            }
            expanded += 1;
            if expanded > max_expanded {
                self.expanded += expanded;
                return Ok(None);
            }
            if expanded % STOP_EVERY == 0 && stop() {
                return Err(RouteError::Stopped);
            }
            let (end, prev) = {
                let e = self.graph.edge(edge);
                (e.to, e.clone())
            };
            if let Some(at) = self.graph.node(end).map(|n| n.at) {
                let z = self.rule.zoom_at(at);
                if self.load(tile_of(at.0, at.1, z)) {
                    progress(self.progress(Stage::Searching));
                    if stop() {
                        return Err(RouteError::Stopped);
                    }
                }
            }
            let Some(&node) = self.graph.node(end) else { continue };
            if !mode.may_pass(node.access) {
                continue;
            }
            let back = edge_key(prev.to, prev.from, prev.way_id);
            let arriving = if mode == Mode::Car { self.graph.heading_in(edge) } else { 0.0 };
            let roads = self.graph.edges_from(end);
            for &next_index in roads {
                let next = self.graph.edge(next_index);
                if !mode.may_use(&next.data)
                    || (next.key == back && !prev.data.has(flag::DEADEND))
                    || (mode.obeys_turn_restrictions()
                        && next.data.local_index < 8
                        && prev.data.restrictions & (1 << next.data.local_index) != 0)
                    || self.complex_forbids(mode, next, edge, &labels)
                {
                    continue;
                }
                let (turn_time, turn_cost) = if mode == Mode::Car {
                    let turn = self.turn_at(arriving, next_index, back, roads, node.info);
                    mode.turn(&prev.data, &next.data, &turn)
                } else {
                    (0, 0)
                };
                let step = mode.transition_ms(Some(&prev.data), &next.data, node.kind) + turn_time;
                let renamed = !next.data.same_street_as(prev.data.opp_local_index);
                let penalty = mode.penalty_ms(Some(&prev.data), &next.data, renamed) + turn_cost;
                if let Some(&tg) = goals.get(&next_index) {
                    let (time, cost, metres) = part(next, tg);
                    offer_goal(
                        &mut best_goal,
                        &mut open,
                        GoalLabel {
                            cost: label.cost + step + penalty + cost,
                            time: label.time + step + time,
                            metres: label.metres + metres,
                            edge: next_index,
                            pred: Some(edge),
                        },
                    );
                }
                let (time, cost, metres) = part(next, 1.0);
                let reached = Label {
                    cost: label.cost + step + penalty + cost,
                    time: label.time + step + time,
                    metres: label.metres + metres,
                    pred: Some(edge),
                };
                if labels.get(&next_index).is_none_or(|l| reached.cost < l.cost) {
                    labels.insert(next_index, reached);
                    open.push(Reverse((reached.cost + estimate(&self.graph, next.to), reached.cost, next_index, false)));
                }
            }
        };

        // The edges from the start to the destination's edge.
        let mut path = vec![found.edge];
        let mut at = found.pred;
        while let Some(e) = at {
            path.push(e);
            at = labels[&e].pred;
        }
        path.reverse();
        let mut points: Vec<(f64, f64)> = Vec::new();
        for (i, &e) in path.iter().enumerate() {
            let t0 = if i == 0 { starts.get(&e).copied().unwrap_or(0.0) } else { 0.0 };
            let t1 = if i + 1 == path.len() { goals[&e] } else { 1.0 };
            for p in cut(&self.graph.edge(e).points, t0, t1) {
                if points.last() != Some(&p) {
                    points.push(p);
                }
            }
        }
        let mut names: Vec<String> = Vec::new();
        for &e in &path {
            if let Some(name) = &self.graph.edge(e).name {
                if names.last() != Some(name) {
                    names.push(name.clone());
                }
            }
        }
        self.expanded += expanded;
        #[allow(clippy::cast_precision_loss)]
        Ok(Some(Route {
            points,
            distance_m: found.metres,
            duration_s: found.time as f64 / 1000.0,
            edges: path.iter().map(|&e| self.graph.edge(e).key).collect(),
            names,
            tiles: self.graph.tile_count(),
            expanded: self.expanded,
            attempts: self.attempt + 1,
        }))
    }
}

/// The part of the line `points` from fraction `t0` to `t1` of its length.
fn cut(points: &[(f64, f64)], t0: f64, t1: f64) -> Vec<(f64, f64)> {
    let lengths: Vec<f64> = points.windows(2).map(|w| distance_m(w[0], w[1])).collect();
    let total: f64 = lengths.iter().sum();
    if total <= 0.0 || points.len() < 2 {
        return points.to_vec();
    }
    let at = |target: f64| {
        let mut along = 0.0;
        for (i, &l) in lengths.iter().enumerate() {
            if along + l >= target || i + 1 == lengths.len() {
                let s = if l > 0.0 { ((target - along) / l).clamp(0.0, 1.0) } else { 0.0 };
                let (a, b) = (points[i], points[i + 1]);
                return (i, (a.0 + (b.0 - a.0) * s, a.1 + (b.1 - a.1) * s));
            }
            along += l;
        }
        (0, points[0])
    };
    let (i0, p0) = at(t0 * total);
    let (i1, p1) = at(t1 * total);
    let mut out = vec![p0];
    out.extend_from_slice(&points[i0 + 1..=i1]);
    out.push(p1);
    out
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
