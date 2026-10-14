//! Writing tiles: a whole road graph, flat (one node per place, one edge per
//! direction), cut into the tiles of one zoom.
//!
//! An edge goes into every tile its line touches, at every zoom its road
//! class belongs to, so a router finds it from any of them; its two nodes
//! come along even when they lie outside the tile. azul-routing-tiles'
//! converter uses this for Valhalla's graph, and the `routing-tiles`
//! generator ([`crate::generate`]) for a graph read from OpenStreetMap.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::{
    format::{edge_key, node_key, valhalla, Edge, Node, Restriction, Shape, Special, Tile},
    tile::{line_tiles, zoom_of_class, TileId},
};

/// A node of the flat graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatNode {
    /// Latitude in 1e-7 degrees.
    pub lat: i32,
    /// Longitude in 1e-7 degrees.
    pub lon: i32,
    /// The second node at the very same place.
    pub twin: bool,
    /// Who may pass (Valhalla access mask).
    pub access: u16,
    /// Valhalla `NodeType`.
    pub kind: u8,
    /// Road density, driving side, traffic signal ([`crate::format::node_info`]).
    pub info: u8,
}

impl FlatNode {
    /// The node's key, as a reader of the tiles computes it.
    #[must_use]
    pub fn key(&self) -> u64 {
        node_key(self.lat, self.lon, self.twin)
    }
}

/// A stretch of road's geometry, shared by its two directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatShape {
    /// The OpenStreetMap way it is part of.
    pub way_id: u64,
    /// Its street name.
    pub name: Option<String>,
    /// The points between the two end nodes, 1e-6 degrees.
    pub points: Vec<(i32, i32)>,
}

/// An edge of the flat graph. `data`'s node and shape indices are ignored;
/// `from`, `to` and `shape` index the flat graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatEdge {
    /// The start node, by index in [`FlatGraph::nodes`].
    pub from: usize,
    /// The end node, by index in [`FlatGraph::nodes`].
    pub to: usize,
    /// The shape, by index in [`FlatGraph::shapes`].
    pub shape: usize,
    /// The edge runs against the shape's direction.
    pub reversed: bool,
    /// Everything else a tile keeps about the edge.
    pub data: Edge,
}

/// The whole graph of a region.
#[derive(Debug, Clone, Default)]
pub struct FlatGraph {
    /// One node per place where roads meet or end.
    pub nodes: Vec<FlatNode>,
    /// One edge per direction a road can be travelled.
    pub edges: Vec<FlatEdge>,
    /// The geometry, one per stretch of road, shared by its two edges.
    pub shapes: Vec<FlatShape>,
    /// Restrictions, naming edges by [`FlatGraph::edge_key`].
    pub restrictions: Vec<Restriction>,
}

impl FlatGraph {
    /// The key of edge `index`, as a reader of the tiles computes it.
    #[must_use]
    pub fn edge_key(&self, index: usize) -> u64 {
        let e = &self.edges[index];
        edge_key(self.nodes[e.from].key(), self.nodes[e.to].key(), self.shapes[e.shape].way_id)
    }

    /// Edge `index`'s line, 1e-7 degrees, from its start to its end node.
    #[must_use]
    pub fn edge_points(&self, index: usize) -> Vec<(i32, i32)> {
        let e = &self.edges[index];
        let (a, b) = (&self.nodes[e.from], &self.nodes[e.to]);
        let mut points = vec![(a.lat, a.lon)];
        let middle = self.shapes[e.shape].points.iter().map(|&(lat, lon)| (lat * 10, lon * 10));
        if e.reversed {
            points.extend(middle.rev());
        } else {
            points.extend(middle);
        }
        points.push((b.lat, b.lon));
        points
    }
}

/// The tiles of zoom `z`: every edge whose class the zoom holds, in every
/// tile its line touches.
#[must_use]
pub fn cut(graph: &FlatGraph, z: u8) -> BTreeMap<TileId, Tile> {
    let deg = |(lat, lon): (i32, i32)| (f64::from(lat) * 1e-7, f64::from(lon) * 1e-7);
    let mut members: BTreeMap<TileId, BTreeSet<usize>> = BTreeMap::new();
    for (index, edge) in graph.edges.iter().enumerate() {
        if zoom_of_class(edge.data.class) > z {
            continue;
        }
        let points = graph.edge_points(index);
        for pair in points.windows(2) {
            for tile in line_tiles(deg(pair[0]), deg(pair[1]), z) {
                members.entry(tile).or_default().insert(index);
            }
        }
    }
    let keys: HashMap<usize, u64> = (0..graph.edges.len()).map(|i| (i, graph.edge_key(i))).collect();
    let mut by_to: HashMap<u64, Vec<&Restriction>> = HashMap::new();
    for r in &graph.restrictions {
        by_to.entry(r.to_edge).or_default().push(r);
    }
    members
        .into_iter()
        .map(|(id, edges)| (id, one_tile(graph, id, &edges, &keys, &by_to)))
        .collect()
}

/// One tile holding `edges` of the flat graph.
fn one_tile(
    graph: &FlatGraph,
    id: TileId,
    edges: &BTreeSet<usize>,
    keys: &HashMap<usize, u64>,
    by_to: &HashMap<u64, Vec<&Restriction>>,
) -> Tile {
    let mut tile = Tile::new(id);
    // Nodes sorted by key: by latitude, then longitude, so the deltas are small.
    let mut nodes: Vec<usize> = edges
        .iter()
        .flat_map(|&e| [graph.edges[e].from, graph.edges[e].to])
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    nodes.sort_by_key(|&n| graph.nodes[n].key());
    let node_index: HashMap<usize, u32> = nodes.iter().enumerate().map(|(i, &n)| (n, i as u32)).collect();
    for (i, &n) in nodes.iter().enumerate() {
        let node = graph.nodes[n];
        tile.nodes.push(Node { lat: node.lat, lon: node.lon, twin: node.twin, info: node.info });
        if node.access != valhalla::ALL || node.kind != 0 {
            tile.specials.push(Special { node: i as u32, access: node.access, kind: node.kind });
        }
    }
    // Edges sorted by start node, then by their local index there.
    let mut order: Vec<usize> = edges.iter().copied().collect();
    order.sort_by_key(|&e| (node_index[&graph.edges[e].from], graph.edges[e].data.local_index, e));
    let mut shape_index: HashMap<usize, u32> = HashMap::new();
    let mut name_index: HashMap<&str, u32> = HashMap::new();
    for &e in &order {
        let flat = &graph.edges[e];
        let shape = *shape_index.entry(flat.shape).or_insert_with(|| {
            let s = &graph.shapes[flat.shape];
            let name = s.name.as_deref().map(|name| {
                *name_index.entry(name).or_insert_with(|| {
                    tile.names.push(name.to_string());
                    tile.names.len() as u32 - 1
                })
            });
            tile.shapes.push(Shape { way_id: s.way_id, name, points: s.points.clone() });
            tile.shapes.len() as u32 - 1
        });
        tile.edges.push(Edge {
            from: node_index[&flat.from],
            to: node_index[&flat.to],
            shape,
            reversed: flat.reversed,
            ..flat.data
        });
    }
    let mut seen = HashSet::new();
    for &e in &order {
        for r in by_to.get(&keys[&e]).into_iter().flatten() {
            if seen.insert(*r as *const Restriction) {
                tile.restrictions.push((*r).clone());
            }
        }
    }
    tile
}
