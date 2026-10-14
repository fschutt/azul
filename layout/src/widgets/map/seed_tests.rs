//! Tiles an app ships in its binary (the world at zoom 0 and 1) are the
//! map's ground of last resort: drawn from their bytes with no download,
//! queued before anything else, never forgotten or evicted - so a first
//! start without a network still shows the world.

use alloc::{sync::Arc, vec::Vec};

use azul_core::{
    geom::OptionLogicalPosition,
    refany::RefAny,
    resources::{ImageRef, RawImageFormat},
};
use azul_css::{AzString, U8Vec};

use super::{
    autotest_generated::{cache_at, with_callback_info_at},
    mount_map, spawn_pending_tile_fetches, tile_fetch_init, CallbackInfo, DrawnTile,
    MapMountCallbackType, MapSetup, MapTileCache, MapTileId, MapTileLayer, MapWidget, TileEntry,
    TileStyleKey, WantedTiles, MAX_DRAWN_TILES,
};
use crate::thread::{ThreadCallback, ThreadSender};

const WORLD: MapTileId = MapTileId { z: 0, x: 0, y: 0 };

fn world_bytes() -> U8Vec {
    U8Vec::from_vec(alloc::vec![0x1a, 0x02, 0x78, 0x02])
}

extern "C" fn noop_worker(_: RefAny, _: ThreadSender, _: azul_core::task::ThreadReceiver) {}

extern "C" fn seeded_setup(_: RefAny, _: CallbackInfo, setup: MapSetup) -> MapSetup {
    setup.with_seed_tile(WORLD, world_bytes())
}

/// A map at zoom 8 whose setup seeds the world tile, mounted.
fn seeded_map() -> RefAny {
    let mut dom = MapWidget::create(MapTileLayer::default())
        .with_on_mount(RefAny::new(()), seeded_setup as MapMountCallbackType)
        .dom();
    let mut data = dom.root.get_dataset_mut().expect("dataset").clone();
    with_callback_info_at(OptionLogicalPosition::None, |info| mount_map(&mut data, &info));
    data
}

#[test]
fn the_map_takes_the_seeds_its_mount_hook_returns_as_tiles_it_already_has() {
    let mut data = seeded_map();
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    assert!(cache.pinned.contains(&WORLD), "the world tile is pinned");
    assert_eq!(
        cache.tile_bytes.get(&WORLD).map(|b| b.as_ref().to_vec()),
        Some(world_bytes().as_ref().to_vec()),
        "its bytes are in hand: drawing it is a restyle, never a download"
    );
    let look = cache.current_look();
    let init = tile_fetch_init(&cache, TileStyleKey { tile: WORLD, look });
    assert!(!init.bytes.as_ref().is_empty(), "the job carries the bytes: no request");
}

#[test]
fn the_seeds_are_queued_first_whatever_the_view_wants() {
    let mut data = seeded_map();
    {
        let mut cache = data.downcast_mut::<MapTileCache>().expect("cache");
        cache.fetch_callback = Some(ThreadCallback::new(noop_worker));
        cache.cascade_scheme = Some(super::MapColorScheme::Light);
        let look = cache.current_look();
        // A view at zoom 8 wanting forty tiles, far more than one pass starts.
        cache.want_tiles(
            WantedTiles {
                z: 8,
                x_min: 100,
                x_max: 107,
                y_min: 100,
                y_max: 104,
            },
            look,
        );
    }
    with_callback_info_at(OptionLogicalPosition::None, |info| {
        let mut info = info;
        spawn_pending_tile_fetches(&mut data, &mut info);
    });
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    let look = cache.current_look();
    assert!(
        matches!(
            cache.tiles.get(&TileStyleKey { tile: WORLD, look }),
            Some(TileEntry::Fetching)
        ),
        "the world tile is drawn in the first pass, before the view's own tiles"
    );
}

#[test]
fn a_seeded_tile_is_never_forgotten_or_evicted() {
    let mut cache = cache_at(10.0, 10.0, 12.0);
    let look = cache.current_look();
    cache.setup = MapSetup::new().with_seed_tile(WORLD, world_bytes());
    cache.install_seed_tiles();
    let key = TileStyleKey { tile: WORLD, look };
    // Queued, outside what the view wants: still kept.
    cache.tiles.insert(key, TileEntry::Pending);
    cache.want_tiles(
        WantedTiles {
            z: 12,
            x_min: 0,
            x_max: 1,
            y_min: 0,
            y_max: 1,
        },
        look,
    );
    cache.drop_unwanted_pending();
    assert!(cache.tiles.contains_key(&key), "a seed's job is never forgotten");
    // Drawn, and far more drawings closer to the view than it: still kept.
    let image = ImageRef::null_image(1, 1, RawImageFormat::RGBA8, Vec::new());
    let drawn = || DrawnTile {
        image: Some(image.clone()),
        geometry_svg: AzString::from(""),
        labels: Arc::new(Vec::new()),
    };
    cache.mark_tile_drawn(key, drawn());
    for i in 0..(MAX_DRAWN_TILES as u32 + 300) {
        cache.mark_tile_drawn(
            TileStyleKey {
                tile: MapTileId {
                    z: 12,
                    x: i % 32,
                    y: i / 32,
                },
                look,
            },
            drawn(),
        );
    }
    cache.prune_distant_tiles();
    assert!(cache.drawn.contains_key(&key), "the world stays drawn");
    assert!(cache.tile_bytes.contains_key(&WORLD), "and its bytes stay");
}

#[test]
fn a_tile_that_failed_offline_is_asked_for_again_while_the_view_still_wants_it() {
    use super::RETRY_FAILED_SECS;

    let mut cache = cache_at(0.0, 0.0, 4.0);
    let look = cache.current_look();
    let tile = MapTileId { z: 4, x: 8, y: 8 };
    let key = TileStyleKey { tile, look };
    let wanted = WantedTiles {
        z: 4,
        x_min: 8,
        x_max: 8,
        y_min: 8,
        y_max: 8,
    };
    cache.want_tiles_at(wanted, look, 0.0);
    cache.mark_tile_failed(key, AzString::from("fetch failed: offline"));
    cache.failed_at.insert(key, 100.0);
    cache.want_tiles_at(wanted, look, 101.0);
    assert!(
        matches!(cache.tiles.get(&key), Some(TileEntry::Failed { .. })),
        "not hammered: a failure waits a while"
    );
    cache.want_tiles_at(wanted, look, 100.0 + RETRY_FAILED_SECS);
    assert!(
        matches!(cache.tiles.get(&key), Some(TileEntry::Pending)),
        "then the view's tile is asked for again: the network may be back"
    );
}
