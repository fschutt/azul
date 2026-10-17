//! The tile format: protobuf's wire format, a tile written and read back,
//! the keys every build agrees on, a real tile of Monaco, and the tile math
//! of the three routing zooms.

use crate::{
    format::{decode, edge_key, encode, flag, gzip, node_info, valhalla, Edge, Node, Restriction, Shape, Special, Tile},
    tile::{line_tiles, tile_of, zoom_of_class, TileId},
    wire::{read_fixed64s, read_varints, undelta, unzigzag, zigzag, Reader, Writer},
};

#[test]
fn zigzag_keeps_small_numbers_of_either_sign_small() {
    for n in [0i64, 1, -1, 63, -64, i64::MAX, i64::MIN] {
        assert_eq!(unzigzag(zigzag(n)), n);
    }
    assert_eq!(zigzag(-1), 1);
    assert_eq!(zigzag(1), 2);
}

#[test]
fn a_delta_column_reads_back_as_written() {
    let values = [5i64, 7, 7, -3, 1 << 40, 0];
    let mut w = Writer::default();
    w.delta(9, values);
    let mut r = Reader::new(&w.buf);
    let (number, value) = r.field().unwrap().unwrap();
    assert_eq!(number, 9);
    let mut raw = Vec::new();
    read_varints(value, &mut raw).unwrap();
    assert_eq!(undelta(&raw), values);
    assert!(r.field().unwrap().is_none());
}

#[test]
fn an_unpacked_repeated_number_is_read_too() {
    let mut w = Writer::default();
    w.uint(3, 300);
    w.uint(3, 1);
    let mut r = Reader::new(&w.buf);
    let mut out = Vec::new();
    while let Some((_, v)) = r.field().unwrap() {
        read_varints(v, &mut out).unwrap();
    }
    assert_eq!(out, [300, 1]);
}

#[test]
fn fixed64_values_read_back_packed_or_one_by_one() {
    let mut w = Writer::default();
    w.packed_fixed64(3, &[u64::MAX, 7]);
    w.fixed64(3, 9);
    let mut r = Reader::new(&w.buf);
    let mut out = Vec::new();
    while let Some((_, v)) = r.field().unwrap() {
        read_fixed64s(v, &mut out).unwrap();
    }
    assert_eq!(out, [u64::MAX, 7, 9]);
}

#[test]
fn a_message_cut_short_is_an_error() {
    let mut w = Writer::default();
    w.bytes(1, b"hello");
    assert!(Reader::new(&w.buf[..4]).field().is_err());
}

/// Two nodes and a two-way street between them, plus a bollard.
fn sample() -> Tile {
    let mut tile = Tile::new(TileId::new(14, 8530, 5973));
    tile.nodes = vec![
        Node { lat: 437_397_000, lon: 74_275_000, twin: false, info: 3 },
        Node { lat: 437_401_000, lon: 74_289_000, twin: true, info: node_info::DRIVE_LEFT },
    ];
    tile.specials = vec![Special { node: 1, access: valhalla::PEDESTRIAN, kind: valhalla::NODE_BOLLARD }];
    tile.names = vec!["Avenue de Monte-Carlo".into()];
    tile.shapes = vec![Shape { way_id: 4_097_656, name: Some(0), points: vec![(43_739_900, 7_428_000)] }];
    let forward = Edge {
        from: 0,
        to: 1,
        length_m: 125,
        speed_kmh: 30,
        access: valhalla::ALL,
        class: 6,
        use_: 0,
        density: 0,
        flags: flag::TUNNEL | 2 << flag::SURFACE_SHIFT,
        local_index: 1,
        opp_local_index: 0,
        restrictions: 0b100,
        shape: 0,
        reversed: false,
        stop_impact: 5 << 3,
        same_name: 0,
    };
    let backward = Edge { from: 1, to: 0, reversed: true, ..forward };
    tile.edges = vec![forward, backward];
    tile.restrictions = vec![Restriction {
        to_edge: tile.edge_key(&backward),
        from_edge: 99,
        vias: vec![tile.edge_key(&forward)],
        modes: valhalla::AUTO,
        timed: true,
    }];
    tile
}

#[test]
fn a_tile_reads_back_exactly_as_it_was_written() {
    let tile = sample();
    assert_eq!(decode(&encode(&tile)).unwrap(), tile);
    assert_eq!(decode(&gzip(&encode(&tile))).unwrap(), tile);
}

#[test]
fn an_edge_against_its_shape_runs_through_the_points_backwards() {
    let tile = sample();
    let points = tile.edge_points(&tile.edges[1]);
    assert_eq!(points.len(), 3);
    assert!((points[0].1 - 7.4289).abs() < 1e-9);
    assert!((points[2].1 - 7.4275).abs() < 1e-9);
    assert_eq!(tile.edge_name(&tile.edges[1]), Some("Avenue de Monte-Carlo"));
    assert_eq!(tile.edges[0].surface(), 2);
    assert_eq!((tile.edges[0].stop_from(1), tile.edges[0].stop_from(0)), (5, 0));
    assert!(tile.edges[0].has(flag::TUNNEL));
}

#[test]
fn keys_come_from_places_and_ways_so_every_build_agrees_on_them() {
    let tile = sample();
    let (a, b) = (tile.nodes[0].key(), tile.nodes[1].key());
    assert_eq!(a, (1_337_397_000u64 << 32) | 1_874_275_000);
    assert_eq!(b >> 63, 1, "the twin bit");
    let forward = tile.edge_key(&tile.edges[0]);
    assert_eq!(forward, edge_key(a, b, 4_097_656));
    assert_ne!(forward, tile.edge_key(&tile.edges[1]), "each direction has its own key");
    // FNV-1a over nothing but zeros, the value the converter's own hash gives.
    assert_eq!(edge_key(0, 0, 0), 0x81d2_3fd7_003c_2305);
}

#[test]
fn a_tile_of_another_format_version_is_refused() {
    let mut bytes = encode(&sample());
    bytes[1] = 2; // the version field's value
    assert!(decode(&bytes).unwrap_err().contains("version 2"));
}

#[test]
fn an_edge_pointing_past_the_nodes_is_an_error() {
    let mut tile = sample();
    tile.edges[0].to = 5;
    assert!(decode(&encode(&tile)).unwrap_err().contains("edge end"));
}

#[test]
fn a_real_tile_of_monaco_reads_with_all_its_columns_in_step() {
    // Valhalla 3.9.1's Monaco as the converter wrote it, gzipped as served.
    let bytes = include_bytes!("../../tests/data/monaco/tiles/14/8530/5973.pbf");
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "served gzipped");
    let tile = decode(bytes).unwrap();
    assert_eq!(tile.id, TileId::new(14, 8530, 5973));
    assert_eq!((tile.nodes.len(), tile.edges.len(), tile.shapes.len()), (1317, 3378, 1708));
    assert!(tile.edges.iter().all(|e| tile.shapes[e.shape as usize].way_id > 0));
    assert!(tile.edges.iter().any(|e| tile.edge_name(e) == Some("Boulevard des Moulins")));
}

#[test]
fn monaco_lies_in_its_zoom_14_tile_and_the_tile_holds_it() {
    let (lat, lon) = (43.7397, 7.4275);
    let tile = tile_of(lat, lon, 14);
    assert_eq!(tile, TileId::new(14, 8530, 5973));
    let (s, w, n, e) = tile.bounds();
    assert!(s <= lat && lat <= n && w <= lon && lon <= e);
}

#[test]
fn the_line_through_the_grid_is_connected() {
    let tiles = line_tiles((52.52, 13.40), (48.14, 11.58), 11);
    for pair in tiles.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let step = a.x.abs_diff(b.x) + a.y.abs_diff(b.y);
        assert!(step <= 2, "{a:?} -> {b:?}");
    }
    assert_eq!(tiles.first(), Some(&tile_of(52.52, 13.40, 11)));
    assert_eq!(tiles.last(), Some(&tile_of(48.14, 11.58, 11)));
}

#[test]
fn road_classes_fall_into_valhallas_three_levels() {
    assert_eq!(zoom_of_class(0), 8);
    assert_eq!(zoom_of_class(2), 8);
    assert_eq!(zoom_of_class(3), 11);
    assert_eq!(zoom_of_class(4), 11);
    assert_eq!(zoom_of_class(7), 14);
}
