//! The router on small made-up road networks: one-way streets, turn
//! restrictions at one node and over several edges, a bollard, a long trip
//! that loads fine tiles only near its ends, the corridor fetched ahead of
//! the search, progress and stopping.

use std::sync::Mutex;

use crate::{
    build::{cut, FlatEdge, FlatGraph, FlatNode, FlatShape},
    corridor::corridor,
    distance_m,
    format::{encode, gzip, valhalla, Edge, Restriction},
    mode::Mode,
    router::{route, route_with, Options, Progress, Route, RouteError, Stage, TileSource},
    source::MemoryTiles,
    tile::{tile_of, TileId, LEVELS},
};

/// A road network being made: nodes, then two-way or one-way roads.
#[derive(Default)]
struct Net {
    graph: FlatGraph,
}

impl Net {
    fn node(&mut self, lat: f64, lon: f64, access: u16) -> usize {
        self.graph.nodes.push(FlatNode {
            lat: (lat * 1e7).round() as i32,
            lon: (lon * 1e7).round() as i32,
            twin: false,
            access,
            kind: 0,
            info: 0,
        });
        self.graph.nodes.len() - 1
    }

    fn at(&self, n: usize) -> (f64, f64) {
        let node = self.graph.nodes[n];
        (f64::from(node.lat) * 1e-7, f64::from(node.lon) * 1e-7)
    }

    /// A road from `a` to `b` of Valhalla class `class`; `cars_back` says
    /// whether cars may also drive it from `b` to `a`. Returns the edges'
    /// indices, there and back.
    fn road(&mut self, a: usize, b: usize, class: u8, cars_back: bool) -> (usize, usize) {
        let way_id = self.graph.shapes.len() as u64 + 1;
        self.graph.shapes.push(FlatShape { way_id, name: Some(format!("road {way_id}")), points: vec![] });
        let length_m = distance_m(self.at(a), self.at(b)).round() as u32;
        let data = |access| Edge {
            from: 0,
            to: 0,
            length_m,
            speed_kmh: if class == 0 { 120 } else { 30 },
            access,
            class,
            use_: 0,
            density: 0,
            flags: 0,
            local_index: 0,
            opp_local_index: 0,
            restrictions: 0,
            shape: 0,
            reversed: false,
            stop_impact: 0,
            same_name: 0,
        };
        let back_access = if cars_back { valhalla::ALL } else { valhalla::ALL & !valhalla::AUTO };
        let shape = self.graph.shapes.len() - 1;
        self.graph.edges.push(FlatEdge { from: a, to: b, shape, reversed: false, data: data(valhalla::ALL) });
        self.graph.edges.push(FlatEdge { from: b, to: a, shape, reversed: true, data: data(back_access) });
        (self.graph.edges.len() - 2, self.graph.edges.len() - 1)
    }

    /// Numbers each node's edges (their local index) and tells each edge the
    /// local index of its way back, as Valhalla does.
    fn finish(&mut self) {
        let mut count = vec![0u8; self.graph.nodes.len()];
        for e in &mut self.graph.edges {
            e.data.local_index = count[e.from];
            count[e.from] += 1;
        }
        for i in 0..self.graph.edges.len() {
            let (from, to) = (self.graph.edges[i].from, self.graph.edges[i].to);
            let back = self.graph.edges.iter().find(|e| e.from == to && e.to == from).map(|e| e.data.local_index);
            self.graph.edges[i].data.opp_local_index = back.unwrap_or(127);
        }
    }

    /// Forbids turning from `from` onto `to` at the node between them.
    fn no_turn(&mut self, from: usize, to: usize) {
        let local = self.graph.edges[to].data.local_index;
        self.graph.edges[from].data.restrictions |= 1 << local;
    }

    fn tiles(&self) -> MemoryTiles {
        let mut out = MemoryTiles::default();
        for z in LEVELS {
            for (id, tile) in cut(&self.graph, z) {
                out.tiles.insert(id, gzip(&encode(&tile)));
            }
        }
        out
    }

    /// The middle of the road between nodes `a` and `b`.
    fn between(&self, a: usize, b: usize) -> (f64, f64) {
        let (p, q) = (self.at(a), self.at(b));
        ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0)
    }

    fn key(&self, edge: usize) -> u64 {
        self.graph.edge_key(edge)
    }
}

/// A 3 x 3 grid of two-way streets about 200 m apart, node `r * 3 + c`.
/// Returns the net and the edge pairs by (from node, to node).
fn grid(closed: Option<(usize, u16)>) -> (Net, std::collections::HashMap<(usize, usize), usize>) {
    let mut net = Net::default();
    for r in 0..3 {
        for c in 0..3 {
            let access = match closed {
                Some((n, access)) if n == r * 3 + c => access,
                _ => valhalla::ALL,
            };
            net.node(43.700 - r as f64 * 0.002, 7.400 + c as f64 * 0.0025, access);
        }
    }
    let mut edges = std::collections::HashMap::new();
    let mut add = |net: &mut Net, a: usize, b: usize| {
        let (there, back) = net.road(a, b, 6, true);
        edges.insert((a, b), there);
        edges.insert((b, a), back);
    };
    for r in 0..3 {
        for c in 0..3 {
            let n = r * 3 + c;
            if c < 2 {
                add(&mut net, n, n + 1);
            }
            if r < 2 {
                add(&mut net, n, n + 3);
            }
        }
    }
    (net, edges)
}

fn go(net: &Net, from: (f64, f64), to: (f64, f64), mode: Mode) -> Route {
    route(&net.tiles(), from, to, mode, &mut |_| {}, &mut || false).unwrap()
}

/// Whether the route takes edge `a` and right after it edge `b`.
fn turns(route: &Route, a: u64, b: u64) -> bool {
    route.edges.windows(2).any(|w| w[0] == a && w[1] == b)
}

#[test]
fn a_car_goes_around_a_one_way_street_a_walker_goes_against_it() {
    let (mut net, edges) = grid(None);
    // Cars may only drive west from node 5 to node 4.
    let east = edges[&(4, 5)];
    net.graph.edges[east].data.access &= !valhalla::AUTO;
    net.finish();
    let (from, to) = (net.between(3, 4), net.between(4, 5));
    let walk = go(&net, from, to, Mode::Walk);
    let car = go(&net, from, to, Mode::Car);
    assert!(walk.distance_m < 300.0, "{}", walk.distance_m);
    assert!(car.distance_m > walk.distance_m + 300.0, "car {} walk {}", car.distance_m, walk.distance_m);
    assert!(!car.edges.contains(&net.key(east)));
}

#[test]
fn a_car_obeys_a_forbidden_turn_and_a_walker_does_not() {
    let (mut net, edges) = grid(None);
    net.finish();
    // Coming north from node 7 to node 4, no turn east to node 5.
    let (north, east) = (edges[&(7, 4)], edges[&(4, 5)]);
    net.no_turn(north, east);
    let (from, to) = (net.between(7, 4), net.between(4, 5));
    let car = go(&net, from, to, Mode::Car);
    assert!(!turns(&car, net.key(north), net.key(east)), "{:?}", car.names);
    let walk = go(&net, from, to, Mode::Walk);
    assert!(turns(&walk, net.key(north), net.key(east)));
    assert!(car.distance_m > walk.distance_m);
}

#[test]
fn a_restriction_over_three_edges_is_obeyed() {
    let (mut net, edges) = grid(None);
    net.finish();
    // From 7 north to 4, on north to 1, then not east to 2.
    let (first, via, last) = (edges[&(7, 4)], edges[&(4, 1)], edges[&(1, 2)]);
    net.graph.restrictions.push(Restriction {
        to_edge: net.key(last),
        from_edge: net.key(first),
        vias: vec![net.key(via)],
        modes: valhalla::AUTO,
        timed: false,
    });
    let (from, to) = (net.between(7, 4), net.between(1, 2));
    let car = go(&net, from, to, Mode::Car);
    let forbidden = [net.key(first), net.key(via), net.key(last)];
    assert!(!car.edges.windows(3).any(|w| w == forbidden), "{:?}", car.names);
    // A bicycle is not named by it and takes the straight way.
    let bike = go(&net, from, to, Mode::Bike);
    assert!(bike.edges.windows(3).any(|w| w == forbidden));
}

#[test]
fn a_restriction_that_holds_at_times_only_is_ignored_without_a_clock() {
    let (mut net, edges) = grid(None);
    net.finish();
    let (first, last) = (edges[&(7, 4)], edges[&(4, 5)]);
    net.graph.restrictions.push(Restriction {
        to_edge: net.key(last),
        from_edge: net.key(first),
        vias: vec![],
        modes: valhalla::AUTO,
        timed: true,
    });
    let car = go(&net, net.between(7, 4), net.between(4, 5), Mode::Car);
    assert!(turns(&car, net.key(first), net.key(last)));
}

#[test]
fn a_bollard_stops_cars_and_lets_walkers_through() {
    let (mut net, _) = grid(Some((4, valhalla::PEDESTRIAN | valhalla::BICYCLE)));
    net.finish();
    let (from, to) = (net.between(3, 4), net.between(4, 5));
    let walk = go(&net, from, to, Mode::Walk);
    let car = go(&net, from, to, Mode::Car);
    assert!(walk.distance_m < 300.0);
    assert!(car.distance_m > 700.0, "around the block: {}", car.distance_m);
}

#[test]
fn start_and_destination_on_one_street_go_straight_along_it() {
    let (mut net, edges) = grid(None);
    net.finish();
    let (a, b) = (net.at(3), net.at(4));
    let from = (a.0, a.1 + (b.1 - a.1) * 0.2);
    let to = (a.0, a.1 + (b.1 - a.1) * 0.8);
    let walk = go(&net, from, to, Mode::Walk);
    assert_eq!(walk.edges, vec![net.key(edges[&(3, 4)])]);
    assert!((walk.distance_m - 0.6 * distance_m(a, b)).abs() < 2.0, "{}", walk.distance_m);
}

#[test]
fn a_place_far_from_any_road_has_no_route() {
    let (mut net, _) = grid(None);
    net.finish();
    let far = (43.80, 7.40);
    let result = route(&net.tiles(), net.at(0), far, Mode::Car, &mut |_| {}, &mut || false);
    assert_eq!(result, Err(RouteError::NoRoadNearDestination));
}

/// Remembers every tile asked for.
struct Counting<S> {
    inner: S,
    asked: Mutex<Vec<TileId>>,
}

impl<S: TileSource> TileSource for Counting<S> {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        self.asked.lock().unwrap().push(tile);
        self.inner.fetch(tile)
    }
}

#[test]
fn a_long_trip_reads_fine_tiles_only_near_its_ends() {
    // A motorway 200 km north, in 20 stretches, with a side street at each end.
    let mut net = Net::default();
    let chain: Vec<usize> = (0..=20).map(|i| net.node(43.0 + f64::from(i) * 0.09, 7.0, valhalla::ALL)).collect();
    for pair in chain.windows(2) {
        net.road(pair[0], pair[1], 0, true);
    }
    let start = net.node(43.0, 7.01, valhalla::ALL);
    let goal = net.node(44.8, 7.01, valhalla::ALL);
    net.road(start, chain[0], 6, true);
    net.road(chain[20], goal, 6, true);
    net.finish();
    let source = Counting { inner: net.tiles(), asked: Mutex::new(Vec::new()) };
    let (from, to) = (net.at(start), net.at(goal));
    let found = route(&source, from, to, Mode::Car, &mut |_| {}, &mut || false).unwrap();
    assert!(found.distance_m > 199_000.0, "{}", found.distance_m);
    let asked = source.asked.into_inner().unwrap();
    assert!(asked.iter().any(|t| t.z == 8), "the middle came from zoom 8");
    for tile in asked.iter().filter(|t| t.z == 14) {
        let (s, w, n, e) = tile.bounds();
        let centre = ((s + n) / 2.0, (w + e) / 2.0);
        let near = distance_m(centre, from).min(distance_m(centre, to));
        assert!(near < 20_000.0, "a zoom 14 tile {} km from both ends", near / 1000.0);
    }
}

#[test]
fn an_edge_that_arrives_in_many_tiles_is_in_the_graph_once() {
    let (mut net, _) = grid(None);
    net.finish();
    let mut graph = crate::graph::Graph::new();
    let tiles = net.tiles();
    for bytes in tiles.tiles.values() {
        graph.add_tile(&crate::format::decode(bytes).unwrap());
    }
    assert_eq!(graph.edge_count(), net.graph.edges.len());
}

#[test]
fn a_destination_next_to_a_short_dead_end_track_goes_to_the_street_network() {
    let (mut net, _) = grid(None);
    // A farm track of two edges in a field east of the grid: nearer to the
    // destination than any street, but no car gets there from the streets.
    let a = net.node(43.698, 7.4065, valhalla::ALL);
    let b = net.node(43.698, 7.4070, valhalla::ALL);
    net.road(a, b, 7, true);
    net.finish();
    let destination = (43.6982, 7.4068);
    let car = go(&net, net.between(0, 1), destination, Mode::Car);
    let end = *car.points.last().unwrap();
    assert!(distance_m(end, net.at(a)) > 50.0 && distance_m(end, net.at(b)) > 50.0, "{end:?}");
}

#[test]
fn the_corridor_is_fetched_ahead_and_the_route_stays_the_same_without_it() {
    let (mut net, edges) = grid(None);
    net.finish();
    let (north, east) = (edges[&(7, 4)], edges[&(4, 5)]);
    net.no_turn(north, east);
    let (from, to) = (net.between(7, 4), net.between(1, 2));
    let tiles = net.tiles();
    let mut steps: Vec<Progress> = Vec::new();
    let ahead = route(&tiles, from, to, Mode::Car, &mut |p| steps.push(p), &mut || false).unwrap();
    let lazy = Options { prefetch_threads: 0, ..Options::default() };
    let without = route_with(&tiles, from, to, Mode::Car, lazy, &mut |_| {}, &mut || false).unwrap();
    assert_eq!(ahead.edges, without.edges);
    assert_eq!(ahead.distance_m, without.distance_m);
    assert_eq!(steps.first().map(|p| p.stage), Some(Stage::Fetching), "the corridor comes first");
    assert_eq!(steps.last().map(|p| p.stage), Some(Stage::Searching));
    assert!(steps.windows(2).all(|w| w[0].tiles_done <= w[1].tiles_done || w[0].attempt < w[1].attempt));
    assert!(steps.iter().all(|p| p.tiles_done <= p.tiles_wanted));
}

#[test]
fn the_corridor_holds_the_ends_first_and_the_zoom_each_place_asks_for() {
    let (from, to) = ((43.70, 7.40), (44.60, 8.40));
    let near_an_end = |at: (f64, f64)| distance_m(at, from).min(distance_m(at, to));
    let tiles = corridor(from, to, &|at| if near_an_end(at) < 5_000.0 { 14 } else { 8 });
    // Nearest an end first: each end's own tile is among the few around it.
    for end in [from, to] {
        let at = tiles.iter().position(|t| *t == tile_of(end.0, end.1, 14));
        assert!(at.is_some_and(|i| i < 20), "{end:?} at {at:?}");
    }
    assert!(tiles.iter().any(|t| t.z == 8), "the middle on zoom 8");
    for t in tiles.iter().filter(|t| t.z == 14) {
        assert!(near_an_end(t.centre()) < 10_000.0, "a zoom 14 tile {} km out", near_an_end(t.centre()) / 1000.0);
    }
    let mut unique = tiles.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), tiles.len(), "each tile once");
}

#[test]
fn a_stopped_route_ends_at_once() {
    let (mut net, _) = grid(None);
    net.finish();
    let result = route(&net.tiles(), net.at(0), net.at(8), Mode::Car, &mut |_| {}, &mut || true);
    assert_eq!(result, Err(RouteError::Stopped));
}

#[test]
fn without_any_tile_the_route_says_why() {
    struct Offline;
    impl TileSource for Offline {
        fn fetch(&self, _: TileId) -> Result<Option<Vec<u8>>, String> {
            Err("offline".into())
        }
    }
    let result = route(&Offline, (43.70, 7.40), (43.71, 7.41), Mode::Walk, &mut |_| {}, &mut || false);
    assert!(matches!(result, Err(RouteError::Fetch(ref why)) if why.contains("offline")), "{result:?}");
}
