//! Cutting a whole graph into tiles: each edge at every zoom its class
//! belongs to, in every tile its line touches, with both its nodes.

use crate::{
    build::{cut, FlatEdge, FlatGraph, FlatNode, FlatShape},
    format::{decode, encode, valhalla, Edge, Tile},
};

/// A motorway and a residential street meeting at one node.
fn sample() -> FlatGraph {
    let node = |lat: f64, lon: f64| FlatNode {
        lat: (lat * 1e7) as i32,
        lon: (lon * 1e7) as i32,
        twin: false,
        access: valhalla::ALL,
        kind: 0,
        info: 0,
    };
    let edge = |from, to, shape, class| FlatEdge {
        from,
        to,
        shape,
        reversed: false,
        data: Edge {
            from: 0,
            to: 0,
            length_m: 1000,
            speed_kmh: 100,
            access: valhalla::ALL,
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
        },
    };
    FlatGraph {
        nodes: vec![node(43.70, 7.30), node(43.74, 7.42), node(43.741, 7.425)],
        edges: vec![edge(0, 1, 0, 0), edge(1, 2, 1, 6)],
        shapes: vec![
            FlatShape { way_id: 1, name: Some("A8".into()), points: vec![] },
            FlatShape { way_id: 2, name: None, points: vec![] },
        ],
        restrictions: vec![],
    }
}

#[test]
fn a_motorway_is_on_every_zoom_and_a_side_street_only_on_zoom_14() {
    let graph = sample();
    let on = |z: u8, edge: usize| cut(&graph, z).values().any(|t| t.edges.iter().any(|e| e.length_m == 1000 && t.edge_key(e) == graph.edge_key(edge)));
    for z in [8, 11, 14] {
        assert!(on(z, 0), "the motorway on zoom {z}");
    }
    assert!(!on(8, 1) && !on(11, 1) && on(14, 1));
}

#[test]
fn a_long_edge_is_in_every_tile_it_crosses_and_each_tile_has_both_its_nodes() {
    let graph = sample();
    let tiles = cut(&graph, 14);
    // About 10 km at zoom 14 (some 2 km a tile) crosses several tiles.
    let holding: Vec<&Tile> = tiles.values().filter(|t| t.edges.iter().any(|e| t.edge_key(e) == graph.edge_key(0))).collect();
    assert!(holding.len() >= 5, "{}", holding.len());
    for tile in holding {
        let again = decode(&encode(tile)).unwrap();
        assert_eq!(&again, tile);
        assert!(again.nodes.iter().any(|n| n.key() == graph.nodes[0].key()));
        assert!(again.nodes.iter().any(|n| n.key() == graph.nodes[1].key()));
    }
}
