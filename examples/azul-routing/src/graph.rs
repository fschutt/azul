//! The road graph, built from tiles as they arrive.
//!
//! Nodes and edges are kept by their keys, which every tile, zoom and build
//! agrees on: an edge that arrives again in another tile (it crosses a tile
//! edge, or it is a main road that zoom 8, 11 and 14 all hold) is added once,
//! and an edge of an old cached tile meets the edges of a new tile at the
//! same nodes.

use std::collections::{HashMap, HashSet};

use crate::{
    distance_m,
    format::{edge_key, valhalla, Edge, Restriction, Tile},
    mode::Mode,
    tile::TileId,
};

/// What the graph knows about a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeInfo {
    /// `(lat, lon)` degrees.
    pub at: (f64, f64),
    /// Who may pass it (Valhalla access mask).
    pub access: u16,
    /// What it is (Valhalla `NodeType`).
    pub kind: u8,
    /// Road density, driving side, traffic signal ([`crate::format::node_info`]).
    pub info: u8,
}

/// One directed edge of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadEdge {
    /// Its key, the same in every tile, zoom and build.
    pub key: u64,
    /// The key of its start node.
    pub from: u64,
    /// The key of its end node.
    pub to: u64,
    /// The edge's attributes as the tile has them (its node and shape
    /// indices point into that tile and mean nothing here).
    pub data: Edge,
    /// The OpenStreetMap way it is part of: with the nodes, it finds the
    /// edge running the other way ([`Graph::twin`]).
    pub way_id: u64,
    /// Its street name, for the route's list of streets.
    pub name: Option<String>,
    /// Its line, `(lat, lon)`, from the start node to the end node.
    pub points: Vec<(f64, f64)>,
}

/// The compass heading (degrees clockwise from north) of `points` as it
/// leaves its first point: towards the point `metres` along it, as Valhalla
/// takes a road's heading at a node (30 m out).
#[must_use]
pub fn heading_along(points: &[(f64, f64)], metres: f64) -> f64 {
    let mut target = *points.last().unwrap_or(&(0.0, 0.0));
    let mut along = 0.0;
    for w in points.windows(2) {
        let piece = distance_m(w[0], w[1]);
        if along + piece >= metres && piece > 0.0 {
            let s = (metres - along) / piece;
            target = (w[0].0 + (w[1].0 - w[0].0) * s, w[0].1 + (w[1].1 - w[0].1) * s);
            break;
        }
        along += piece;
    }
    bearing(points.first().copied().unwrap_or(target), target)
}

/// The initial compass bearing from `a` to `b`, degrees 0-360.
#[must_use]
pub fn bearing(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlon = (b.1 - a.1).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Where a place meets the nearest edge a mode may use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    /// The edge, by its index in [`Graph::edge`].
    pub edge: u32,
    /// How far along it the point is, 0 (start) to 1 (end).
    pub t: f64,
    /// The point on the edge.
    pub point: (f64, f64),
    /// How far the place is from it, metres.
    pub distance_m: f64,
}

/// The graph of every tile read so far.
#[derive(Debug, Default)]
pub struct Graph {
    /// Every node read, by key.
    nodes: HashMap<u64, NodeInfo>,
    /// Every edge read, once each; the search names them by index here.
    edges: Vec<RoadEdge>,
    /// The index of each edge by its key: keeps an edge that arrives again
    /// in another tile single, and finds an edge's twin.
    by_key: HashMap<u64, u32>,
    /// The edges leaving each node, by index.
    out: HashMap<u64, Vec<u32>>,
    /// The restrictions over several edges, by the edge they end on.
    restrictions: HashMap<u64, Vec<Restriction>>,
    /// Every restriction once (a tile border repeats them).
    seen: HashSet<Restriction>,
    /// The tiles read (or found to have no roads): a tile is read once.
    loaded: HashSet<TileId>,
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `tile` was read (or found to have no roads).
    #[must_use]
    pub fn has_tile(&self, tile: TileId) -> bool {
        self.loaded.contains(&tile)
    }

    /// How many tiles were read.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.loaded.len()
    }

    /// How many edges the graph has.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Marks `tile` read without adding anything: the hosting has no such
    /// tile because there is no road in it.
    pub fn mark_empty(&mut self, tile: TileId) {
        self.loaded.insert(tile);
    }

    /// Adds every node, edge and restriction of `tile` not in yet.
    pub fn add_tile(&mut self, tile: &Tile) {
        self.loaded.insert(tile.id);
        let keys: Vec<u64> = tile.nodes.iter().map(crate::format::Node::key).collect();
        for (node, &key) in tile.nodes.iter().zip(&keys) {
            self.nodes.entry(key).or_insert(NodeInfo {
                at: node.latlon(),
                access: valhalla::ALL,
                kind: 0,
                info: node.info,
            });
        }
        for special in &tile.specials {
            if let Some(info) = self.nodes.get_mut(&keys[special.node as usize]) {
                info.access = special.access;
                info.kind = special.kind;
            }
        }
        for edge in &tile.edges {
            let key = tile.edge_key(edge);
            if self.by_key.contains_key(&key) {
                continue;
            }
            let index = u32::try_from(self.edges.len()).unwrap_or(u32::MAX);
            let from = keys[edge.from as usize];
            self.by_key.insert(key, index);
            self.out.entry(from).or_default().push(index);
            self.edges.push(RoadEdge {
                key,
                from,
                to: keys[edge.to as usize],
                data: *edge,
                way_id: tile.shapes.get(edge.shape as usize).map_or(0, |s| s.way_id),
                name: tile.edge_name(edge).map(str::to_string),
                points: tile.edge_points(edge),
            });
        }
        for r in &tile.restrictions {
            if self.seen.insert(r.clone()) {
                self.restrictions.entry(r.to_edge).or_default().push(r.clone());
            }
        }
    }

    /// What the graph knows about node `key`.
    #[must_use]
    pub fn node(&self, key: u64) -> Option<&NodeInfo> {
        self.nodes.get(&key)
    }

    /// Edge `index`.
    #[must_use]
    pub fn edge(&self, index: u32) -> &RoadEdge {
        &self.edges[index as usize]
    }

    /// The edges leaving node `key`, by index.
    #[must_use]
    pub fn edges_from(&self, key: u64) -> &[u32] {
        self.out.get(&key).map_or(&[], Vec::as_slice)
    }

    /// The restrictions that end on edge `key`.
    #[must_use]
    pub fn restrictions_into(&self, key: u64) -> &[Restriction] {
        self.restrictions.get(&key).map_or(&[], Vec::as_slice)
    }

    /// The heading of edge `index` as it leaves its start node.
    #[must_use]
    pub fn heading_out(&self, index: u32) -> f64 {
        heading_along(&self.edge(index).points, 30.0)
    }

    /// The heading of edge `index` as it arrives at its end node.
    #[must_use]
    pub fn heading_in(&self, index: u32) -> f64 {
        let back: Vec<(f64, f64)> = self.edge(index).points.iter().rev().copied().collect();
        (heading_along(&back, 30.0) + 180.0) % 360.0
    }

    /// The edge running the other way along the same stretch of road.
    #[must_use]
    pub fn twin(&self, index: u32) -> Option<u32> {
        let e = self.edge(index);
        self.by_key.get(&edge_key(e.to, e.from, e.way_id)).copied()
    }

    /// The nearest point to `place` on an edge `mode` may use, no further
    /// than `max_m` metres.
    #[must_use]
    pub fn snap(&self, place: (f64, f64), mode: Mode, max_m: f64) -> Option<Snap> {
        self.snaps(place, mode, max_m).into_iter().next()
    }

    /// The nearest point to `place` on each road `mode` may use within
    /// `max_m` metres, nearest first: one per road, whichever direction of it
    /// the mode may use.
    #[must_use]
    pub fn snaps(&self, place: (f64, f64), mode: Mode, max_m: f64) -> Vec<Snap> {
        let mut by_road: HashMap<u64, Snap> = HashMap::new();
        for (index, edge) in self.edges.iter().enumerate() {
            if !mode.may_use(&edge.data) {
                continue;
            }
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            if let Some(snap) = self.snap_to(place, index, max_m) {
                // Both directions of a road are one candidate.
                let road = self.twin(index).map_or(edge.key, |t| edge.key.min(self.edge(t).key));
                if by_road.get(&road).is_none_or(|s| snap.distance_m < s.distance_m) {
                    by_road.insert(road, snap);
                }
            }
        }
        let mut out: Vec<Snap> = by_road.into_values().collect();
        out.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m).then(a.edge.cmp(&b.edge)));
        out
    }

    /// How many edges `mode` can reach from the end of edge `index`
    /// (`forward`) or can come from into its start, counting up to `limit`:
    /// a road on a small island (a farm track, a car park behind a barrier)
    /// counts few.
    #[must_use]
    pub fn reach(&self, index: u32, mode: Mode, forward: bool, limit: usize) -> usize {
        let mut seen: HashSet<u32> = HashSet::from([index]);
        let mut queue = std::collections::VecDeque::from([index]);
        while let Some(e) = queue.pop_front() {
            if seen.len() >= limit {
                break;
            }
            let edge = self.edge(e);
            let node = if forward { edge.to } else { edge.from };
            if self.node(node).is_some_and(|n| !mode.may_pass(n.access)) {
                continue;
            }
            for &out in self.edges_from(node) {
                // Going back, the edges into the node are the twins of those
                // leaving it.
                let next = if forward { Some(out) } else { self.twin(out) };
                if let Some(next) = next.filter(|&n| mode.may_use(&self.edge(n).data)) {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
        seen.len()
    }

    /// The nearest point to `place` on edge `index`, if within `max_m`.
    fn snap_to(&self, place: (f64, f64), index: u32, max_m: f64) -> Option<Snap> {
        let edge = self.edge(index);
        // Near a place the earth is flat enough: metres east and north of it.
        let per_lat = 110_540.0;
        let per_lon = 111_320.0 * place.0.to_radians().cos();
        let local = |(lat, lon): (f64, f64)| ((lon - place.1) * per_lon, (lat - place.0) * per_lat);
        let length: f64 = edge.points.windows(2).map(|w| distance_m(w[0], w[1])).sum();
        let mut best: Option<Snap> = None;
        let mut along = 0.0;
        for w in edge.points.windows(2) {
            let (a, b) = (local(w[0]), local(w[1]));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l2 = dx * dx + dy * dy;
            let s = if l2 > 0.0 { (-(a.0 * dx + a.1 * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (a.0 + dx * s).hypot(a.1 + dy * s);
            let piece = distance_m(w[0], w[1]);
            if d <= max_m && best.is_none_or(|b| d < b.distance_m) {
                best = Some(Snap {
                    edge: index,
                    t: if length > 0.0 { ((along + piece * s) / length).clamp(0.0, 1.0) } else { 0.0 },
                    point: (w[0].0 + (w[1].0 - w[0].0) * s, w[0].1 + (w[1].1 - w[0].1) * s),
                    distance_m: d,
                });
            }
            along += piece;
        }
        best
    }
}
