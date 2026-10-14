//! The router on real tiles: Monaco as Valhalla 3.9.1 built it from
//! OpenStreetMap (data © OpenStreetMap contributors, ODbL 1.0; see the
//! manifest), converted by azul-routing-tiles. The distances are those
//! Valhalla's own router finds on the same data (with its maneuver penalty
//! off for the car, where its two best routes are half a second apart), and
//! the edges are the ones azul-routing-tiles' reference client takes.
//!
//! Berlin is too big to keep in the repository. With `AZUL_ROUTING_BERLIN`
//! naming a Berlin build's `manifest.json` (azul-routing-tiles:
//! `scripts/build_region.sh berlin`), the Berlin trip is checked too.

use std::path::PathBuf;

use azul_routing::{manifest::Manifest, route, route_with, source::FolderTiles, Mode, Options, Route};

/// The casino of Monte-Carlo and the Stade Louis II.
const CASINO: (f64, f64) = (43.7397, 7.4275);
const STADIUM: (f64, f64) = (43.7276, 7.4155);

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/monaco/manifest.json")
}

fn go(mode: Mode) -> Route {
    let tiles = FolderTiles { root: manifest_path() };
    route(&tiles, CASINO, STADIUM, mode, &mut |_| {}, &mut || false).unwrap()
}

#[test]
fn the_manifest_names_the_data_its_licence_and_its_maker() {
    let m = Manifest::read_file(&manifest_path()).unwrap();
    assert_eq!(m.license, "ODbL-1.0");
    assert_eq!(m.attribution, "© OpenStreetMap contributors");
    assert_eq!(m.valhalla, "3.9.1");
    assert!(m.covers(CASINO.0, CASINO.1) && m.covers(STADIUM.0, STADIUM.1));
    assert_eq!(m.levels.iter().map(|l| l.z).collect::<Vec<_>>(), [8, 11, 14]);
}

#[test]
fn a_car_takes_valhallas_way_from_the_casino_to_the_stadium() {
    let car = go(Mode::Car);
    assert_eq!(car.edges.len(), 94);
    assert!((car.distance_m - 3015.0).abs() < 5.0, "{}", car.distance_m);
    // Valhalla: 331 s on this way.
    assert!((car.duration_s - 331.0).abs() < 10.0, "{}", car.duration_s);
    assert!(car.names.iter().any(|n| n == "Tunnel Dorsale"), "{:?}", car.names);
    assert_eq!(car.attempts, 1);
}

#[test]
fn a_bicycle_and_a_walker_go_their_own_shorter_ways() {
    let bike = go(Mode::Bike);
    assert!((bike.distance_m - 2359.0).abs() < 5.0, "{}", bike.distance_m);
    let walk = go(Mode::Walk);
    // Valhalla walks 2164 m; it rates footways a little differently.
    assert!((walk.distance_m - 2164.0).abs() < 70.0, "{}", walk.distance_m);
    assert!(walk.distance_m < bike.distance_m);
}

#[test]
fn the_folder_or_its_manifest_both_name_the_tiles() {
    let folder = FolderTiles { root: manifest_path().parent().unwrap().to_path_buf() };
    let car = route(&folder, CASINO, STADIUM, Mode::Car, &mut |_| {}, &mut || false).unwrap();
    assert_eq!(car.edges, go(Mode::Car).edges);
}

#[test]
fn fetching_the_corridor_ahead_changes_nothing_about_the_route() {
    let tiles = FolderTiles { root: manifest_path() };
    let lazy = Options { prefetch_threads: 0, ..Options::default() };
    for mode in [Mode::Car, Mode::Bike, Mode::Walk] {
        let without = route_with(&tiles, CASINO, STADIUM, mode, lazy, &mut |_| {}, &mut || false).unwrap();
        assert_eq!(without.edges, go(mode).edges, "{mode:?}");
    }
}

#[test]
fn berlin_routes_as_valhalla_does_when_its_tiles_are_at_hand() {
    let Some(manifest) = std::env::var_os("AZUL_ROUTING_BERLIN") else {
        eprintln!("AZUL_ROUTING_BERLIN is not set: the Berlin trip is not checked");
        return;
    };
    let tiles = FolderTiles { root: PathBuf::from(manifest) };
    // Mitte to Oberschoeneweide.
    let (from, to) = ((52.5200, 13.4050), (52.4559, 13.5274));
    let go = |mode| route(&tiles, from, to, mode, &mut |_| {}, &mut || false).unwrap();
    let car = go(Mode::Car);
    // Valhalla 3.9.1: 14.321 km, 19.2 min; the reference client 19.7 min.
    assert!((car.distance_m - 14_321.0).abs() < 20.0, "{}", car.distance_m);
    assert!((car.duration_s / 60.0 - 19.7).abs() < 1.0, "{}", car.duration_s / 60.0);
    // The reference client's ways (Valhalla's: 13.13 km by bike, 12.103 km on foot).
    assert!((go(Mode::Bike).distance_m - 12_260.0).abs() < 50.0);
    assert!((go(Mode::Walk).distance_m - 12_088.0).abs() < 50.0);
}
