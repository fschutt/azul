//! The tile cache: how long each zoom is kept and who decides, a kept tile
//! that costs no fetch in this session or the next, an old tile that serves
//! when the network is gone, pruning that spares offline areas, and the
//! folder of a build with or without its manifest.

use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, SystemTime},
};

use serde_json::Value;

use crate::{
    offline::{self, Area, OfflineArea},
    router::TileSource,
    source::{tile_file, CachedTiles, FolderTiles, KeepPolicy, MemoryTiles, SETTINGS_KEY},
    tile::TileId,
};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-routing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn coarse_tiles_are_kept_longer_than_fine_ones() {
    let p = KeepPolicy::default();
    assert_eq!(p.keep_for(8), Duration::from_secs(56 * 86_400));
    assert_eq!(p.keep_for(11), Duration::from_secs(28 * 86_400));
    assert_eq!(p.keep_for(14), Duration::from_secs(14 * 86_400));
}

#[test]
fn the_users_settings_win_over_the_hostings_suggestion() {
    let all: Value = serde_json::json!({ SETTINGS_KEY: 3 });
    assert_eq!(KeepPolicy::default().with_settings(&all).days, [3, 3, 3]);
    let some: Value = serde_json::json!({ SETTINGS_KEY: {"8": 365} });
    assert_eq!(KeepPolicy::default().with_settings(&some).days, [365, 28, 14]);
    let none: Value = serde_json::json!({"theme": "flora"});
    assert_eq!(KeepPolicy::default().with_settings(&none), KeepPolicy::default());
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/monaco/manifest.json");
    let manifest = crate::manifest::Manifest::read_file(std::path::Path::new(path)).unwrap();
    assert_eq!(KeepPolicy::from_manifest(&manifest).days, [56, 28, 14]);
}

#[test]
fn a_tile_past_its_time_is_stale() {
    let p = KeepPolicy { days: [1, 1, 1] };
    let now = SystemTime::now();
    assert!(p.is_fresh(14, now - Duration::from_secs(3600), now));
    assert!(!p.is_fresh(14, now - Duration::from_secs(2 * 86_400), now));
}

/// Answers from memory, counts the questions, and fails once told to.
struct Flaky {
    calls: AtomicUsize,
    offline: AtomicBool,
    tiles: MemoryTiles,
}

impl Flaky {
    fn new(tiles: MemoryTiles) -> Self {
        Self { calls: AtomicUsize::new(0), offline: AtomicBool::new(false), tiles }
    }
}

impl TileSource for Flaky {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.offline.load(Ordering::SeqCst) {
            Err("offline".into())
        } else {
            self.tiles.fetch(tile)
        }
    }
}

#[test]
fn a_kept_tile_costs_no_fetch_and_an_old_one_serves_when_offline() {
    let dir = temp("cache-offline");
    let (tile, empty) = (TileId::new(14, 1, 2), TileId::new(14, 3, 4));
    let mut memory = MemoryTiles::default();
    memory.tiles.insert(tile, b"tile".to_vec());
    let mut cache = CachedTiles::new(Flaky::new(memory), Some(dir.clone()), 1 << 20);
    assert_eq!(cache.fetch(tile), Ok(Some(b"tile".to_vec())));
    assert_eq!(cache.fetch(tile), Ok(Some(b"tile".to_vec())));
    assert_eq!(cache.fetch(empty), Ok(None));
    assert_eq!(cache.inner.calls.load(Ordering::SeqCst), 2, "the second came from the cache");
    // Every tile is stale now, and the network is gone.
    cache.policy = KeepPolicy { days: [0, 0, 0] };
    cache.inner.offline.store(true, Ordering::SeqCst);
    assert_eq!(cache.fetch(tile), Ok(Some(b"tile".to_vec())), "the old tile serves");
    assert_eq!(cache.fetch(empty), Ok(None), "and so does an old 'no roads here'");
    assert_eq!(cache.inner.calls.load(Ordering::SeqCst), 4, "it did try the network");
    // A tile it never had is an error offline.
    assert!(cache.fetch(TileId::new(14, 5, 6)).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cached_tile_costs_no_second_fetch_in_this_session_or_the_next() {
    let dir = temp("cache-sessions");
    let (with_roads, without) = (TileId::new(14, 1, 2), TileId::new(14, 3, 4));
    let mut memory = MemoryTiles::default();
    memory.tiles.insert(with_roads, b"tile".to_vec());
    let cache = CachedTiles::new(Flaky::new(memory.clone()), Some(dir.clone()), 1 << 20);
    for _ in 0..3 {
        assert_eq!(cache.fetch(with_roads), Ok(Some(b"tile".to_vec())));
        assert_eq!(cache.fetch(without), Ok(None));
    }
    assert_eq!(cache.inner.calls.load(Ordering::SeqCst), 2, "one fetch per tile");
    assert!(tile_file(&dir, with_roads).ends_with("14/1/2.pbf"));
    assert!(tile_file(&dir, with_roads).is_file());
    let next_session = CachedTiles::new(Flaky::new(memory), Some(dir.clone()), 1 << 20);
    assert_eq!(next_session.fetch(with_roads), Ok(Some(b"tile".to_vec())));
    assert_eq!(next_session.fetch(without), Ok(None));
    assert_eq!(next_session.inner.calls.load(Ordering::SeqCst), 0, "the disk had both");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pruning_goes_oldest_first_and_never_touches_an_offline_area() {
    let dir = temp("cache-prune");
    let tiles = [TileId::new(14, 1, 1), TileId::new(14, 1, 2), TileId::new(14, 1, 3)];
    let mut memory = MemoryTiles::default();
    for tile in tiles {
        memory.tiles.insert(tile, vec![7; 100]);
    }
    let cache = CachedTiles::new(memory, Some(dir.clone()), 150);
    let now = SystemTime::now();
    for (age_days, tile) in [3u64, 2, 1].into_iter().zip(tiles) {
        cache.fetch(tile).unwrap();
        let file = std::fs::File::options().write(true).open(tile_file(&dir, tile)).unwrap();
        file.set_modified(now - Duration::from_secs(age_days * 86_400)).unwrap();
    }
    // The oldest tile belongs to an offline area.
    let area = OfflineArea {
        name: "harbour".into(),
        area: Area { south: 0.0, west: 0.0, north: 0.0, east: 0.0 },
        tiles: vec![tiles[0]],
        bytes: 100,
    };
    offline::pin(&dir, &area).unwrap();
    let left = cache.prune();
    assert!(tile_file(&dir, tiles[0]).is_file(), "the offline area's tile stays");
    assert!(!tile_file(&dir, tiles[1]).exists(), "the oldest other tile goes");
    assert!(tile_file(&dir, tiles[2]).is_file(), "the newest stays: the cache fits again");
    assert_eq!(left, 100, "the offline area counts apart");
    assert_eq!(offline::areas(&dir).len(), 1, "the area's own file stays too");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_without_a_manifest_holds_its_tiles_under_tiles() {
    let dir = temp("folder");
    let tile = TileId::new(14, 8530, 5973);
    let path = dir.join("tiles/14/8530/5973.pbf");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"tile").unwrap();
    let folder = FolderTiles { root: dir.clone() };
    assert_eq!(folder.fetch(tile), Ok(Some(b"tile".to_vec())));
    assert_eq!(folder.fetch(TileId::new(14, 1, 1)), Ok(None), "a missing file has no roads");
    let _ = std::fs::remove_dir_all(&dir);
}
