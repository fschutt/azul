//! The generator: an OpenStreetMap extract cut into the same tiles a
//! Valhalla build gives - ways split where they meet, Valhalla's classes,
//! uses and access masks per direction - written with a manifest and a tile
//! list, and routed on.

use std::{collections::HashMap, io::Read};

use crate::{
    build::cut,
    distance_m,
    format::{encode, flag, gzip, valhalla},
    generate::{cut_level, flatten, generate, rfc3339, way_from_tags, write_manifest, write_to_folder, Access, Highway, Oneway, Way},
    manifest::Manifest,
    mode::Mode,
    offline::parse_index,
    pbf::tests::extract,
    router::route,
    source::{FolderTiles, MemoryTiles},
    tile::LEVELS,
};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-routing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn road(id: i64, highway: Highway, nodes: &[i64]) -> Way {
    Way {
        id,
        nodes: nodes.to_vec(),
        highway,
        oneway: Oneway::No,
        bicycle_both_ways: false,
        maxspeed_kmh: None,
        access: Access::default(),
        name: None,
    }
}

/// An L of residential streets with a side street at its corner, and a
/// building that is no road.
fn l_extract() -> Vec<u8> {
    extract(
        &[(1, 43.700, 7.400), (2, 43.700, 7.4175), (3, 43.710, 7.4175), (4, 43.700, 7.430)],
        &[
            (10, vec![1, 2, 3], vec![("highway", "residential"), ("name", "Rue A")]),
            (11, vec![2, 4], vec![("highway", "residential")]),
            (12, vec![1, 3, 4, 1], vec![("building", "yes")]),
        ],
    )
}

fn open(file: &[u8]) -> impl FnMut() -> Result<Box<dyn Read>, String> + '_ {
    move || Ok(Box::new(std::io::Cursor::new(file.to_vec())) as Box<dyn Read>)
}

#[test]
fn osm_tags_become_a_routing_way_and_other_ways_are_left_out() {
    let tags = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    };
    let way = way_from_tags(5, vec![1, 2], &tags(&[("highway", "primary"), ("oneway", "yes"), ("maxspeed", "30 mph")]))
        .expect("a road");
    assert_eq!(way.oneway, Oneway::Forward);
    assert!((way.maxspeed_kmh.expect("a limit") - 48.28).abs() < 0.1);
    assert!(way_from_tags(6, vec![1, 2], &tags(&[("building", "yes")])).is_none());
    assert!(way_from_tags(7, vec![1, 2], &tags(&[("highway", "pedestrian"), ("area", "yes")])).is_none());
    let roundabout = way_from_tags(8, vec![1, 2, 1], &tags(&[("highway", "tertiary"), ("junction", "roundabout")]))
        .expect("a road");
    assert_eq!(roundabout.oneway, Oneway::Forward);
}

#[test]
fn ways_are_cut_where_they_meet_and_keep_their_bends() {
    let coords: HashMap<i64, (f64, f64)> =
        [(1, (43.700, 7.400)), (5, (43.701, 7.405)), (2, (43.700, 7.410)), (3, (43.710, 7.410)), (4, (43.700, 7.420))]
            .into_iter()
            .collect();
    let ways = [road(10, Highway::Residential, &[1, 5, 2, 3]), road(11, Highway::Residential, &[2, 4])];
    let graph = flatten(&ways, &coords);
    assert_eq!(graph.nodes.len(), 4, "the ends and the corner where the side street meets, not the bend");
    assert_eq!(graph.edges.len(), 6, "three stretches, both ways");
    let bend = graph.edges.iter().position(|e| graph.shapes[e.shape].points.len() == 1).expect("the bend");
    assert_eq!(graph.edge_points(bend).len(), 3);
    // Each edge knows its way back, as Valhalla's opposing edge index.
    for e in &graph.edges {
        let back = graph.edges.iter().find(|b| b.from == e.to && b.to == e.from && b.shape == e.shape).expect("a way back");
        assert_eq!(e.data.opp_local_index, back.data.local_index);
    }
    let length: u32 = graph.edges.iter().filter(|e| !e.reversed && e.shape == graph.edges[bend].shape).map(|e| e.data.length_m).sum();
    let along = distance_m((43.700, 7.400), (43.701, 7.405)) + distance_m((43.701, 7.405), (43.700, 7.410));
    assert!((f64::from(length) - along).abs() < 1.0, "{length} m along the bend, {along} m expected");
}

#[test]
fn one_ways_footways_and_motorways_become_valhallas_classes_and_access_masks() {
    let coords: HashMap<i64, (f64, f64)> = (1..=8).map(|i| (i, (43.70 + f64::from(i as i32) * 0.01, 7.40))).collect();
    let mut one_way = road(10, Highway::Residential, &[1, 2]);
    one_way.oneway = Oneway::Forward;
    let footway = road(11, Highway::Footway, &[3, 4]);
    let mut motorway = road(12, Highway::Motorway, &[5, 6]);
    motorway.oneway = Oneway::Forward;
    let link = road(13, Highway::MotorwayLink, &[7, 8]);
    let graph = flatten(&[one_way, footway, motorway, link], &coords);
    let edge = |way: u64, reversed: bool| {
        graph.edges.iter().find(|e| graph.shapes[e.shape].way_id == way && e.reversed == reversed).expect("an edge").data
    };
    let all = valhalla::AUTO | valhalla::BICYCLE | valhalla::PEDESTRIAN;
    assert_eq!(edge(10, false).access, all);
    assert_eq!(edge(10, true).access, valhalla::PEDESTRIAN, "against a one-way only on foot");
    assert_eq!(edge(11, false).access, valhalla::PEDESTRIAN);
    assert_eq!(edge(12, false).access, valhalla::AUTO);
    assert_eq!(edge(12, true).access, 0);
    assert_eq!((edge(12, false).class, edge(10, false).class, edge(11, false).class), (0, 6, 7));
    assert_eq!(edge(13, false).use_, valhalla::USE_RAMP);
    assert!(edge(13, false).has(flag::LINK));
    assert_eq!(edge(11, false).use_, valhalla::USE_FOOTWAY);
    // A motorway is on every zoom, a footway only on zoom 14.
    assert!(!cut(&graph, 8).is_empty());
    assert!(cut(&graph, 8).values().all(|t| t.edges.iter().all(|e| e.class <= 2)));
}

#[test]
fn an_extract_becomes_tiles_a_car_routes_on() {
    let file = l_extract();
    let mut written = MemoryTiles::default();
    let generated = generate(&mut open(&file), &mut |tile, bytes| {
        written.tiles.insert(tile, bytes.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(generated.ways, 2, "the building is no road");
    assert_eq!(generated.edges, 6, "the corner splits the L: three stretches, both ways");
    assert_eq!(generated.tiles.iter().map(|&(z, _)| z).collect::<Vec<_>>(), LEVELS);
    assert!(generated.tiles.iter().all(|&(z, n)| (z == 14) == (n > 0)), "streets are on zoom 14 only");
    assert_eq!(generated.index.len(), written.tiles.len());
    let (corner, end) = ((43.700, 7.4175), (43.710, 7.4175));
    let car = route(&written, (43.700, 7.400), end, Mode::Car, &mut |_| {}, &mut || false).unwrap();
    let legs = distance_m((43.700, 7.400), corner) + distance_m(corner, end);
    assert!((car.distance_m - legs).abs() < 5.0, "{} m along the roads, {legs} m expected", car.distance_m);
    assert_eq!(car.names, ["Rue A"]);
}

#[test]
fn a_folder_the_generator_wrote_routes_from_its_manifest() {
    let dir = temp("generated");
    let file = l_extract();
    let generated = generate(&mut open(&file), &mut |tile, bytes| write_to_folder(&dir, tile, bytes)).unwrap();
    let manifest = write_manifest(&dir, &generated, "l-street", "l-street.osm.pbf", "2026-10-14T20:00:00Z").unwrap();
    assert_eq!(manifest.build, "2026-10-14-l-street");
    let read = Manifest::read_file(&dir).unwrap();
    assert_eq!(read, manifest);
    assert_eq!(read.attribution, "© OpenStreetMap contributors");
    assert_eq!(read.license, "ODbL-1.0");
    assert!(read.covers(43.705, 7.41));
    let index = parse_index(&std::fs::read_to_string(dir.join("tiles.txt")).unwrap());
    assert_eq!(index, generated.index);
    let tiles = FolderTiles { root: dir.join("manifest.json") };
    let car = route(&tiles, (43.700, 7.400), (43.710, 7.4175), Mode::Car, &mut |_| {}, &mut || false).unwrap();
    assert!(car.distance_m > 2_400.0, "{}", car.distance_m);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ways_cut_level_by_level_route_from_a_plain_folder() {
    let dir = temp("levels");
    let coords: HashMap<i64, (f64, f64)> =
        [(1, (43.7300, 7.4200)), (2, (43.7300, 7.4300)), (3, (43.7380, 7.4300))].into_iter().collect();
    let street = road(10, Highway::Residential, &[1, 2, 3]);
    for z in LEVELS {
        for (tile, data) in cut_level(&[street.clone()], &coords, z) {
            write_to_folder(&dir, tile, &gzip(&encode(&data))).unwrap();
        }
    }
    let tiles = FolderTiles { root: dir.clone() };
    let found = route(&tiles, (43.7300, 7.4200), (43.7380, 7.4300), Mode::Car, &mut |_| {}, &mut || false).unwrap();
    assert!(found.distance_m > 1_600.0, "around the corner: {} m", found.distance_m);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_build_time_is_written_as_rfc_3339() {
    assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(rfc3339(1_760_472_394), "2025-10-14T20:06:34Z");
    assert_eq!(rfc3339(4_102_444_800), "2100-01-01T00:00:00Z");
}
