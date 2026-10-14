//! Offline areas: the tiles of a rectangle, a download plan from the
//! hosting's tile list, the area pinned so the cache never prunes its
//! tiles, and the download itself.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    offline::{self, map_tiles, parse_index, plan, tiles_in, Area, OfflineArea},
    router::TileSource,
    source::MemoryTiles,
    tile::{tile_of, TileId},
};

const MONACO: Area = Area { south: 43.72, west: 7.40, north: 43.76, east: 7.44 };

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-routing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn an_area_covers_its_corner_tiles() {
    let tiles = tiles_in(MONACO, 14);
    assert!(tiles.contains(&tile_of(43.72, 7.40, 14)));
    assert!(tiles.contains(&tile_of(43.76, 7.44, 14)));
    assert_eq!(tiles_in(MONACO, 8), vec![tile_of(43.74, 7.42, 8)]);
}

#[test]
fn a_download_plan_asks_only_for_tiles_that_exist() {
    let real = tile_of(43.7397, 7.4275, 14);
    let index = parse_index(&format!("{}\t\n14/1/1 99\nnot a line\n{} 1234\n", "8/0/0 5", real.path()));
    assert_eq!(index.len(), 3, "the line that is not a tile is skipped");
    let (tiles, bytes) = plan(MONACO, &index);
    assert_eq!(tiles, vec![real]);
    assert_eq!(bytes, 1234);
}

#[test]
fn the_plan_of_monaco_is_every_tile_of_its_real_tile_list() {
    let text = include_str!("../../tests/data/monaco/tiles.txt");
    let index = parse_index(text);
    assert_eq!(index.len(), 7);
    let (tiles, bytes) = plan(MONACO, &index);
    assert_eq!(tiles.len(), 7);
    assert_eq!(bytes, 7278 + 32173 + 13348 + 122_750 + 522 + 53591 + 14548);
}

#[test]
fn the_map_needs_every_zoom_up_to_the_last() {
    let tiles = map_tiles(MONACO, 3);
    assert_eq!(tiles.iter().filter(|t| t.z == 0).count(), 1);
    assert!(tiles.iter().all(|t| t.z <= 3));
}

#[test]
fn a_pinned_area_lists_its_tiles_until_it_is_deleted() {
    let cache = temp("pins");
    let area = OfflineArea {
        name: "Monaco / Monte-Carlo".into(),
        area: MONACO,
        tiles: vec![TileId::new(14, 8530, 5973), TileId::new(8, 133, 93)],
        bytes: 60_869,
    };
    offline::pin(&cache, &area).unwrap();
    assert_eq!(offline::areas(&cache), vec![area.clone()]);
    let pinned = offline::pinned(&cache);
    assert!(pinned.contains(&TileId::new(14, 8530, 5973)) && pinned.contains(&TileId::new(8, 133, 93)));
    offline::unpin(&cache, &area.name).unwrap();
    assert!(offline::areas(&cache).is_empty());
    assert!(offline::pinned(&cache).is_empty());
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn a_download_fetches_every_tile_once_and_counts_its_bytes() {
    struct Counting(AtomicUsize, MemoryTiles);
    impl TileSource for Counting {
        fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.fetch(tile)
        }
    }
    let mut memory = MemoryTiles::default();
    memory.tiles.insert(TileId::new(14, 1, 1), vec![0; 10]);
    memory.tiles.insert(TileId::new(14, 1, 2), vec![0; 20]);
    let source = Counting(AtomicUsize::new(0), memory);
    let wanted = [TileId::new(14, 1, 1), TileId::new(14, 1, 2), TileId::new(14, 1, 3)];
    let mut steps = Vec::new();
    let done = offline::download(&source, &wanted, 2, &mut |step| {
        steps.push(step);
        true
    })
    .unwrap();
    assert_eq!((done.tiles, done.bytes, done.failed), (3, 30, 0));
    assert_eq!(source.0.load(Ordering::SeqCst), 3);
    assert_eq!(steps.last().map(|s| (s.done, s.total)), Some((3, 3)));
    // A download that is told to stop says so.
    let stopped = offline::download(&source, &wanted, 1, &mut |_| false);
    assert!(stopped.is_err());
}
