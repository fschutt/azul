//! The tile set's manifest: read and written, the tile URL relative to it,
//! other formats refused, the coverage, and the real manifest of Monaco.

use crate::{
    manifest::{is_manifest, resolve, LevelStats, Manifest, Region},
    tile::TileId,
};

fn sample() -> Manifest {
    Manifest {
        version: crate::format::VERSION,
        build: "2026-10-14-monaco".into(),
        tiles: "tiles/{z}/{x}/{y}.pbf".into(),
        index: Some("tiles.txt".into()),
        bbox: [7.40, 43.72, 7.44, 43.76],
        regions: vec![Region {
            id: "monaco".into(),
            source: "https://download.geofabrik.de/europe/monaco-latest.osm.pbf".into(),
            osm_timestamp: "2026-10-13T20:21:00Z".into(),
            bbox: [7.40, 43.72, 7.44, 43.76],
        }],
        built_at: "2026-10-14T20:00:00Z".into(),
        valhalla: "3.9.1".into(),
        valhalla_image: "ghcr.io/valhalla/valhalla@sha256:00".into(),
        attribution: "© OpenStreetMap contributors".into(),
        attribution_url: "https://www.openstreetmap.org/copyright".into(),
        license: "ODbL-1.0".into(),
        license_url: "https://opendatacommons.org/licenses/odbl/1-0/".into(),
        keep_days: [56, 28, 14],
        levels: vec![LevelStats { z: 14, tiles: 12, bytes: 40_000 }],
    }
}

#[test]
fn a_manifest_reads_back_as_written() {
    let m = sample();
    assert_eq!(Manifest::parse(&m.to_json()).unwrap(), m);
}

#[test]
fn a_tile_url_is_relative_to_the_manifest() {
    let m = sample();
    let tile = TileId::new(14, 8530, 5973);
    assert_eq!(
        m.tile_url("https://example.github.io/routing/manifest.json", tile),
        "https://example.github.io/routing/tiles/14/8530/5973.pbf"
    );
    assert_eq!(m.tile_url("/data/monaco/manifest.json", tile), "/data/monaco/tiles/14/8530/5973.pbf");
    assert_eq!(
        m.index_url("https://example.github.io/routing/manifest.json").as_deref(),
        Some("https://example.github.io/routing/tiles.txt")
    );
    assert_eq!(resolve("https://a.example/x/manifest.json", "https://b.example/t/{z}"), "https://b.example/t/{z}");
}

#[test]
fn another_format_or_version_is_refused() {
    assert!(Manifest::parse(r#"{"format":"something"}"#).is_err());
    let newer = sample().to_json().replace("\"version\": 1", "\"version\": 2");
    assert!(Manifest::parse(&newer).unwrap_err().contains("version 2"));
}

#[test]
fn coverage_is_the_regions_built() {
    let m = sample();
    assert!(m.covers(43.7397, 7.4275));
    assert!(!m.covers(52.52, 13.40));
}

#[test]
fn a_routing_value_without_a_tile_template_names_a_manifest() {
    assert!(is_manifest("https://example.github.io/azul-routing-tiles/berlin/manifest.json"));
    assert!(is_manifest("/data/monaco/manifest.json"));
    assert!(is_manifest("/data/monaco"), "a folder holding a manifest");
    assert!(!is_manifest("https://example.org/routing/{z}/{x}/{y}.pbf"));
}

#[test]
fn the_real_manifest_of_monaco_names_its_data_licence_and_keep_times() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/monaco/manifest.json");
    let m = Manifest::read_file(std::path::Path::new(path)).unwrap();
    assert_eq!(m.attribution, "© OpenStreetMap contributors");
    assert_eq!(m.attribution_url, "https://www.openstreetmap.org/copyright");
    assert_eq!(m.license, "ODbL-1.0");
    assert_eq!(m.valhalla, "3.9.1");
    assert_eq!(m.keep_days, [56, 28, 14]);
    assert_eq!(m.index.as_deref(), Some("tiles.txt"));
    assert_eq!(m.levels.iter().map(|l| (l.z, l.tiles)).collect::<Vec<_>>(), [(8, 1), (11, 1), (14, 5)]);
    assert!(m.covers(43.7397, 7.4275) && m.covers(43.7276, 7.4155), "the casino and the stadium");
}
