//! The router end to end on small made-up road networks: cut into tiles by
//! the generator's own code, served from memory, routed by [`route`].

use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    astar::astar,
    corridor::corridor,
    distance_m,
    format::{parse_tile, write_tile, Access, Highway, Oneway, TileData, Way},
    generate::{cut_level, gzip, way_from_tags},
    route,
    source::{CachedTiles, MemoryTiles},
    tile::{line_tiles, tile_of, TileId, LEVELS},
    Mode, RouteError, TileSource,
};

/// A road through `nodes` (ids into the network's places).
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

/// The tiles of every level of a network, gzipped like the generator's.
fn tiles_of(ways: &[Way], places: &[(i64, (f64, f64))]) -> MemoryTiles {
    let coords: HashMap<i64, (f64, f64)> = places.iter().copied().collect();
    let mut tiles = MemoryTiles::default();
    for z in LEVELS {
        for (tile, data) in cut_level(ways, &coords, z) {
            let json = write_tile(tile, &data);
            tiles.tiles.insert(tile, gzip(json.as_bytes()).expect("gzip"));
        }
    }
    tiles
}

fn no_progress(_: crate::Progress) {}

#[test]
fn a_route_follows_the_roads_around_a_corner_not_the_straight_line() {
    // An L: east 2 km, then north 2 km.
    let places = [(1, (43.700, 7.400)), (2, (43.700, 7.425)), (3, (43.718, 7.425))];
    let tiles = tiles_of(&[road(10, Highway::Residential, &[1, 2, 3])], &places);
    let r = route(&tiles, (43.700, 7.400), (43.718, 7.425), Mode::Car, &mut no_progress, &mut || false)
        .expect("a route");
    let straight = distance_m((43.700, 7.400), (43.718, 7.425));
    let legs = distance_m(places[0].1, places[1].1) + distance_m(places[1].1, places[2].1);
    assert!((r.distance_m - legs).abs() < 5.0, "{} m along the roads, {legs} m expected", r.distance_m);
    assert!(r.distance_m > straight * 1.2, "longer than the {straight} m straight line");
    // 30 km/h on a residential road.
    assert!((r.duration_s - legs / (30.0 / 3.6)).abs() < 2.0, "{} s", r.duration_s);
    let near = |a: Option<&(f64, f64)>, b: (f64, f64)| a.is_some_and(|a| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    assert!(near(r.points.first(), places[0].1), "it starts at the start: {:?}", r.points.first());
    assert!(near(r.points.last(), places[2].1), "it ends at the destination: {:?}", r.points.last());
}

#[test]
fn a_car_keeps_to_a_one_way_street_and_a_walker_does_not() {
    // A short one-way street west, and a long detour both ways.
    let places = [
        (1, (43.700, 7.400)),
        (2, (43.700, 7.410)),
        (3, (43.706, 7.400)),
        (4, (43.706, 7.410)),
    ];
    let mut one_way = road(10, Highway::Residential, &[2, 1]);
    one_way.oneway = Oneway::Forward;
    let detour = road(11, Highway::Residential, &[1, 3, 4, 2]);
    let tiles = tiles_of(&[one_way, detour], &places);
    let car = route(&tiles, (43.700, 7.400), (43.700, 7.410), Mode::Car, &mut no_progress, &mut || false)
        .expect("a car route");
    let walk = route(&tiles, (43.700, 7.400), (43.700, 7.410), Mode::Walk, &mut no_progress, &mut || false)
        .expect("a walking route");
    let direct = distance_m(places[0].1, places[1].1);
    assert!(car.distance_m > direct * 2.0, "the car goes round: {} m", car.distance_m);
    assert!((walk.distance_m - direct).abs() < 5.0, "the walker takes the street: {} m", walk.distance_m);
}

#[test]
fn a_bridge_outside_the_first_corridor_is_found_by_widening_it() {
    // A river runs north-south between 7.410 and 7.420; the only bridge is
    // 11 km north of the line between the ends. A tile holds its nodes and
    // their neighbours along a way, so each bank's road has a node halfway:
    // the bridge's ends are then in no tile of the first corridor.
    let places = [
        (1, (43.700, 7.395)),
        (2, (43.700, 7.410)),
        (7, (43.750, 7.410)),
        (3, (43.800, 7.410)),
        (4, (43.800, 7.420)),
        (8, (43.750, 7.420)),
        (5, (43.700, 7.420)),
        (6, (43.700, 7.435)),
    ];
    let tiles = tiles_of(&[road(10, Highway::Tertiary, &[1, 2, 7, 3, 4, 8, 5, 6])], &places);
    let r = route(&tiles, (43.700, 7.400), (43.700, 7.430), Mode::Car, &mut no_progress, &mut || false)
        .expect("a route over the bridge");
    assert!(r.attempts > 1, "the first corridor holds no way: {} attempts", r.attempts);
    assert!(r.distance_m > 20_000.0, "the way over the bridge: {} m", r.distance_m);
}

#[test]
fn a_long_trip_drives_the_motorway_from_the_coarse_tiles() {
    // Streets at both ends, a motorway between them 130 km long.
    let mut places = vec![(1, (43.700, 7.400)), (2, (43.710, 7.410)), (90, (44.590, 8.390)), (91, (44.600, 8.400))];
    let mut motorway_nodes = vec![2];
    for i in 0..10 {
        let f = f64::from(i + 1) / 11.0;
        let id = 20 + i64::from(i);
        places.push((id, (43.710 + (44.590 - 43.710) * f, 7.410 + (8.390 - 7.410) * f)));
        motorway_nodes.push(id);
    }
    motorway_nodes.push(90);
    let ways = [
        road(10, Highway::Residential, &[1, 2]),
        road(11, Highway::Motorway, &motorway_nodes),
        road(12, Highway::Residential, &[90, 91]),
    ];
    let tiles = tiles_of(&ways, &places);
    let progress = std::cell::RefCell::new(Vec::new());
    let r = route(
        &tiles,
        (43.700, 7.400),
        (44.600, 8.400),
        Mode::Car,
        &mut |p| progress.borrow_mut().push(p),
        &mut || false,
    )
    .expect("a route on the motorway");
    assert!(r.distance_m > 120_000.0, "{} m", r.distance_m);
    let wanted = corridor((43.700, 7.400), (44.600, 8.400), 0);
    assert!(wanted.iter().any(|t| t.z == 8), "the middle is on zoom 8");
    assert!(
        wanted.iter().filter(|t| t.z == 14).count() < 200,
        "zoom 14 only near the ends: {} tiles",
        wanted.iter().filter(|t| t.z == 14).count()
    );
    let steps = progress.borrow();
    assert!(steps.len() > 2, "the progress is reported tile by tile");
    assert!(steps.windows(2).all(|w| w[0].tiles_done <= w[1].tiles_done || w[0].attempt < w[1].attempt));
}

#[test]
fn a_place_far_from_any_road_has_no_route_and_says_so() {
    let places = [(1, (43.700, 7.400)), (2, (43.700, 7.410))];
    let tiles = tiles_of(&[road(10, Highway::Residential, &[1, 2])], &places);
    let r = route(&tiles, (43.750, 7.400), (43.700, 7.410), Mode::Car, &mut no_progress, &mut || false);
    assert_eq!(r, Err(RouteError::NoRoadNearStart));
    let r = route(&tiles, (43.700, 7.400), (43.700, 7.500), Mode::Car, &mut no_progress, &mut || false);
    assert_eq!(r, Err(RouteError::NoRoadNearDestination));
}

#[test]
fn a_stopped_route_ends_at_once() {
    let places = [(1, (43.700, 7.400)), (2, (43.700, 7.410))];
    let tiles = tiles_of(&[road(10, Highway::Residential, &[1, 2])], &places);
    let r = route(&tiles, (43.700, 7.400), (43.700, 7.410), Mode::Car, &mut no_progress, &mut || true);
    assert_eq!(r, Err(RouteError::Stopped));
}

#[test]
fn a_tile_reads_back_as_it_was_written() {
    let mut way = road(7, Highway::SecondaryLink, &[1, 2]);
    way.oneway = Oneway::Backward;
    way.maxspeed_kmh = Some(50.0);
    way.access.bicycle = Some(false);
    way.name = Some("Avenue d'Ostende".into());
    let data = TileData {
        nodes: vec![(1, 43.7, 7.4), (2, 43.71, 7.41)],
        ways: vec![way],
    };
    let tile = TileId::new(14, 8529, 5975);
    let json = write_tile(tile, &data);
    assert_eq!(parse_tile(json.as_bytes()), Ok(data.clone()));
    assert_eq!(parse_tile(&gzip(json.as_bytes()).expect("gzip")), Ok(data));
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
fn a_straight_line_touches_every_tile_between_its_ends() {
    let tiles = line_tiles((43.70, 7.40), (43.70, 7.60), 14);
    let (a, b) = (tile_of(43.70, 7.40, 14), tile_of(43.70, 7.60, 14));
    assert_eq!(tiles.first(), Some(&a));
    assert_eq!(tiles.last(), Some(&b));
    assert_eq!(tiles.len() as u32, b.x - a.x + 1, "one row, every column");
    // A diagonal moves one column or one row per step (or both through a corner).
    let diagonal = line_tiles((43.70, 7.40), (43.80, 7.55), 14);
    for w in diagonal.windows(2) {
        let (dx, dy) = (w[0].x.abs_diff(w[1].x), w[0].y.abs_diff(w[1].y));
        assert!(dx + dy >= 1 && dx <= 1 && dy <= 1, "{w:?}");
    }
}

#[test]
fn a_wider_corridor_holds_the_narrower_one() {
    let (from, to) = ((43.70, 7.40), (43.75, 7.50));
    let first = corridor(from, to, 0);
    let second = corridor(from, to, 1);
    assert!(first.iter().all(|t| second.contains(t)));
    assert!(second.len() > first.len());
    assert!(first.iter().all(|t| t.z == 14), "a short trip is all zoom 14");
    let end = tile_of(from.0, from.1, 14);
    assert!(first[..9].contains(&end), "the ends' tiles come first");
}

#[test]
fn astar_finds_the_cheapest_path_not_the_fewest_steps() {
    // 1 -> 4 directly costs 10; 1 -> 2 -> 3 -> 4 costs 3.
    let edges: HashMap<u32, Vec<(u32, u64)>> =
        [(1, vec![(4, 10), (2, 1)]), (2, vec![(3, 1)]), (3, vec![(4, 1)])].into_iter().collect();
    let found = astar(1, 4, |n| edges.get(&n).cloned().unwrap_or_default(), |_| 0, &mut || false)
        .expect("a path");
    assert_eq!(found.path, vec![1, 2, 3, 4]);
    assert_eq!(found.cost, 3);
    assert!(astar(4, 1, |n| edges.get(&n).cloned().unwrap_or_default(), |_| 0, &mut || false).is_none());
}

#[test]
fn every_tile_is_fetched_once_and_a_stop_starts_no_more() {
    struct Counting(AtomicUsize);
    impl TileSource for Counting {
        fn fetch(&self, _: TileId) -> Result<Option<Vec<u8>>, String> {
            // A download takes a while: the answers keep pace with the threads.
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }
    let tiles: Vec<TileId> = (0..100).map(|x| TileId::new(14, x, 7)).collect();
    let source = Counting(AtomicUsize::new(0));
    let mut seen = Vec::new();
    crate::fetch_parallel(&source, &tiles, 8, &mut |t, _| {
        seen.push(t);
        true
    });
    seen.sort();
    assert_eq!(seen, tiles);
    let source = Counting(AtomicUsize::new(0));
    let mut answers = 0;
    crate::fetch_parallel(&source, &tiles, 4, &mut |_, _| {
        answers += 1;
        answers < 5
    });
    assert_eq!(answers, 5);
    assert!(source.0.load(Ordering::SeqCst) < 20, "the threads stop soon after");
}

#[test]
fn a_cached_tile_costs_no_second_fetch_in_this_session_or_the_next() {
    struct Counting(AtomicUsize, MemoryTiles);
    impl TileSource for Counting {
        fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.fetch(tile)
        }
    }
    let dir = std::env::temp_dir().join(format!("azul-routing-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (with_roads, without) = (TileId::new(14, 1, 2), TileId::new(14, 3, 4));
    let mut memory = MemoryTiles::default();
    memory.tiles.insert(with_roads, b"tile".to_vec());
    let cache = CachedTiles::new(Counting(AtomicUsize::new(0), memory.clone()), Some(dir.clone()), 1 << 20);
    for _ in 0..3 {
        assert_eq!(cache.fetch(with_roads), Ok(Some(b"tile".to_vec())));
        assert_eq!(cache.fetch(without), Ok(None));
    }
    assert_eq!(cache.inner.0.load(Ordering::SeqCst), 2, "one fetch per tile");
    let next_session = CachedTiles::new(Counting(AtomicUsize::new(0), memory), Some(dir.clone()), 1 << 20);
    assert_eq!(next_session.fetch(with_roads), Ok(Some(b"tile".to_vec())));
    assert_eq!(next_session.fetch(without), Ok(None));
    assert_eq!(next_session.inner.0.load(Ordering::SeqCst), 0, "the disk had both");
    let _ = std::fs::remove_dir_all(&dir);
}
