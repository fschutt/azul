//! The road graph of one travel mode, built from the tiles as they arrive.
//!
//! Nodes are OpenStreetMap node ids; an edge joins two nodes that follow
//! each other on a way the mode may use, in each direction the mode may go,
//! and costs the travel time in milliseconds. A way is in every tile it has
//! a node in, and in every routing level its road kind is on, so the same
//! stretch of road arrives many times: each one (way id and place along the
//! way) is added once.

use std::collections::{HashMap, HashSet};

use crate::{distance_m, format::TileData, mode::Mode, tile::TileId};

/// One stretch of road between two nodes that follow each other on a way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// The node it starts at (the way's order).
    pub a: i64,
    /// The node it ends at.
    pub b: i64,
    /// The travel time from `a` to `b` in ms; `None` where the mode may not
    /// go that way (a one-way street).
    pub forward_ms: Option<u64>,
    /// The travel time from `b` to `a` in ms; `None` where it may not.
    pub backward_ms: Option<u64>,
}

/// Where a place meets the nearest road the mode may use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    /// The segment, by its index in the graph's segments.
    pub segment: usize,
    /// How far along the segment, from `a` (0) to `b` (1).
    pub t: f64,
    /// The point on the road, `(lat, lon)`.
    pub point: (f64, f64),
    /// How far the place is from it, metres.
    pub distance_m: f64,
}

/// The graph of one mode.
#[derive(Debug, Clone)]
pub struct Graph {
    /// The mode whose roads and speeds the edges follow.
    mode: Mode,
    /// Every node seen in a tile, `(lat, lon)`, by OSM id.
    coords: HashMap<i64, (f64, f64)>,
    /// The stretches of road added, in the order they arrived.
    segments: Vec<Segment>,
    /// The edges leaving each node: the node they reach and their cost in ms.
    edges: HashMap<i64, Vec<(i64, u64)>>,
    /// Which stretches are in already: the way's id and the stretch's place
    /// along it. Keeps a road that arrives in several tiles single.
    added: HashSet<(i64, usize)>,
    /// The tiles read so far: a wider corridor fetches only the others.
    loaded: HashSet<TileId>,
}

impl Graph {
    /// An empty graph for `mode`.
    #[must_use]
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            coords: HashMap::new(),
            segments: Vec::new(),
            edges: HashMap::new(),
            added: HashSet::new(),
            loaded: HashSet::new(),
        }
    }

    /// The graph's travel mode.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether `tile` has been read (or found to have no roads).
    #[must_use]
    pub fn has_tile(&self, tile: TileId) -> bool {
        self.loaded.contains(&tile)
    }

    /// How many tiles have been read.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.loaded.len()
    }

    /// Marks `tile` read without adding anything: the hosting has no such
    /// tile because there is no road in it.
    pub fn mark_empty(&mut self, tile: TileId) {
        self.loaded.insert(tile);
    }

    /// Adds the roads of `tile` the mode may use.
    pub fn add_tile(&mut self, tile: TileId, data: &TileData) {
        self.loaded.insert(tile);
        for &(id, lat, lon) in &data.nodes {
            self.coords.entry(id).or_insert((lat, lon));
        }
        for way in &data.ways {
            if !self.mode.allows(way) {
                continue;
            }
            let speed_ms = f64::from(self.mode.speed_kmh(way)) / 3.6;
            let forward = self.mode.may_go(way, true);
            let backward = self.mode.may_go(way, false);
            for (index, pair) in way.nodes.windows(2).enumerate() {
                let (a, b) = (pair[0], pair[1]);
                if a == b || self.added.contains(&(way.id, index)) {
                    continue;
                }
                // A stretch whose other end this tile does not know is built
                // from the tile that does.
                let (Some(&pa), Some(&pb)) = (self.coords.get(&a), self.coords.get(&b)) else {
                    continue;
                };
                self.added.insert((way.id, index));
                let ms = travel_ms(distance_m(pa, pb), speed_ms);
                let segment = Segment {
                    a,
                    b,
                    forward_ms: forward.then_some(ms),
                    backward_ms: backward.then_some(ms),
                };
                if let Some(ms) = segment.forward_ms {
                    self.edges.entry(a).or_default().push((b, ms));
                }
                if let Some(ms) = segment.backward_ms {
                    self.edges.entry(b).or_default().push((a, ms));
                }
                self.segments.push(segment);
            }
        }
    }

    /// Where node `id` is, `(lat, lon)`.
    #[must_use]
    pub fn coord(&self, id: i64) -> Option<(f64, f64)> {
        self.coords.get(&id).copied()
    }

    /// The edges leaving `id`: the nodes they reach and their cost in ms.
    #[must_use]
    pub fn edges_from(&self, id: i64) -> &[(i64, u64)] {
        self.edges.get(&id).map_or(&[], Vec::as_slice)
    }

    /// The stretches of road, as [`Snap::segment`] counts them.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The nearest point to `place` on a road the mode may use, no further
    /// than `max_m` metres; `None` when there is none.
    #[must_use]
    pub fn snap(&self, place: (f64, f64), max_m: f64) -> Option<Snap> {
        // Near a place the earth is flat enough: metres east and north of it.
        let metres_per_deg_lat = 110_540.0;
        let metres_per_deg_lon = 111_320.0 * place.0.to_radians().cos();
        let local = |(lat, lon): (f64, f64)| {
            (
                (lon - place.1) * metres_per_deg_lon,
                (lat - place.0) * metres_per_deg_lat,
            )
        };
        let mut best: Option<Snap> = None;
        for (index, segment) in self.segments.iter().enumerate() {
            let (Some(pa), Some(pb)) = (self.coord(segment.a), self.coord(segment.b)) else {
                continue;
            };
            let (a, b) = (local(pa), local(pb));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length2 = dx * dx + dy * dy;
            let t = if length2 > 0.0 {
                (-(a.0 * dx + a.1 * dy) / length2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (x, y) = (a.0 + dx * t, a.1 + dy * t);
            let d = x.hypot(y);
            if d > max_m || best.is_some_and(|s| s.distance_m <= d) {
                continue;
            }
            best = Some(Snap {
                segment: index,
                t,
                point: (pa.0 + (pb.0 - pa.0) * t, pa.1 + (pb.1 - pa.1) * t),
                distance_m: d,
            });
        }
        best
    }
}

/// The time to go `metres` at `speed` metres per second, in whole ms (at
/// least one, so no step is free).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a trip's ms fit in u64
pub fn travel_ms(metres: f64, speed: f64) -> u64 {
    if !(speed > 0.0) || !metres.is_finite() {
        return u64::MAX / 4;
    }
    ((metres / speed * 1000.0).ceil() as u64).max(1)
}
