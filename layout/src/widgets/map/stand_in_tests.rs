//! A tile that has not arrived yet is never a blank square: its box shows
//! the coarser tile that covers it (scaled up and clipped), or the finer
//! tiles it covers, until its own drawing arrives and fades in over them.

use alloc::{sync::Arc, vec::Vec};

use azul_core::{
    dom::{Dom, NodeType, OptionDom},
    geom::LogicalSize,
    resources::{ImageRef, ImageRefHash, RawImageFormat},
};
use azul_css::AzString;

use super::{
    autotest_generated::{cache_at, layer_zoom, view, with_virtual_view_info},
    map_visible_tiles, map_widget_render, DrawnTile, MapLook, MapTileCache, MapTileId,
    TilePiece, TileStyleKey, MAX_DRAWN_TILES, TILE_FADE_SECS,
};

fn an_image() -> ImageRef {
    ImageRef::null_image(1, 1, RawImageFormat::RGBA8, Vec::new())
}

fn drawing(image: &ImageRef) -> DrawnTile {
    DrawnTile {
        image: Some(image.clone()),
        geometry_svg: AzString::from(""),
        labels: Arc::new(Vec::new()),
    }
}

const fn tile(z: u8, x: u32, y: u32) -> MapTileId {
    MapTileId { z, x, y }
}

/// `tile` drawn at `look`, long enough ago that it no longer fades.
fn settled(cache: &mut MapTileCache, tile: MapTileId, look: MapLook, image: &ImageRef) {
    let key = TileStyleKey { tile, look };
    cache.mark_tile_drawn(key, drawing(image));
    cache.arrived.remove(&key);
}

fn piece(tile: MapTileId, look: MapLook, left: f64, top: f64, size: f64, opacity: f32) -> TilePiece {
    TilePiece {
        key: TileStyleKey { tile, look },
        left,
        top,
        size,
        opacity,
    }
}

#[test]
fn a_missing_tile_stands_in_with_its_nearest_drawn_ancestor_scaled_and_offset() {
    let mut cache = cache_at(0.0, 0.0, 3.0);
    let look = cache.current_look();
    let image = an_image();
    settled(&mut cache, tile(1, 0, 0), look, &image);
    settled(&mut cache, tile(2, 1, 1), look, &image);
    // z3 (3, 2) lies in z2 (1, 1): the right half of its top half.
    assert_eq!(
        cache.tile_stack(tile(3, 3, 2), look, 50.0),
        vec![piece(tile(2, 1, 1), look, -1.0, 0.0, 2.0, 1.0)],
        "the nearest ancestor, twice the tile's size, shifted onto the tile's part of it"
    );
}

#[test]
fn with_no_closer_ancestor_the_world_tile_stands_in() {
    let mut cache = cache_at(0.0, 0.0, 4.0);
    let look = cache.current_look();
    settled(&mut cache, tile(0, 0, 0), look, &an_image());
    assert_eq!(
        cache.tile_stack(tile(4, 5, 9), look, 50.0),
        vec![piece(tile(0, 0, 0), look, -5.0, -9.0, 16.0, 1.0)]
    );
}

#[test]
fn zooming_out_a_missing_tile_shows_its_drawn_children_over_its_ancestor() {
    let mut cache = cache_at(0.0, 0.0, 2.0);
    let look = cache.current_look();
    let image = an_image();
    settled(&mut cache, tile(1, 0, 0), look, &image);
    settled(&mut cache, tile(3, 2, 2), look, &image);
    settled(&mut cache, tile(3, 3, 3), look, &image);
    assert_eq!(
        cache.tile_stack(tile(2, 1, 1), look, 50.0),
        vec![
            piece(tile(1, 0, 0), look, -1.0, -1.0, 2.0, 1.0),
            piece(tile(3, 2, 2), look, 0.0, 0.0, 0.5, 1.0),
            piece(tile(3, 3, 3), look, 0.5, 0.5, 0.5, 1.0),
        ],
        "the coarser tile under the gaps, the finer ones over it in their quarters"
    );
}

#[test]
fn a_tile_that_just_arrived_fades_in_over_its_stand_in() {
    let mut cache = cache_at(0.0, 0.0, 3.0);
    let look = cache.current_look();
    let image = an_image();
    settled(&mut cache, tile(2, 1, 1), look, &image);
    let own = TileStyleKey {
        tile: tile(3, 2, 3),
        look,
    };
    cache.mark_tile_drawn(own, drawing(&image));
    cache.arrived.insert(own, 100.0);
    let just = cache.tile_stack(own.tile, look, 100.0);
    assert_eq!(
        just,
        vec![
            piece(tile(2, 1, 1), look, 0.0, -1.0, 2.0, 1.0),
            piece(own.tile, look, 0.0, 0.0, 1.0, 0.0),
        ],
        "the coarse tile stays under the fine one while it fades in"
    );
    let half = cache.tile_stack(own.tile, look, 100.0 + TILE_FADE_SECS * 0.5);
    assert!((half[1].opacity - 0.5).abs() < 0.05, "half way: {:?}", half[1]);
    assert_eq!(
        cache.tile_stack(own.tile, look, 100.0 + TILE_FADE_SECS),
        vec![piece(own.tile, look, 0.0, 0.0, 1.0, 1.0)],
        "faded in, the tile is its own drawing alone"
    );
}

#[test]
fn a_look_switch_fades_the_new_look_in_over_the_old_one() {
    let mut cache = cache_at(0.0, 0.0, 3.0);
    let image = an_image();
    let t = tile(3, 4, 4);
    settled(&mut cache, t, MapLook::Positron, &image);
    let dark = TileStyleKey {
        tile: t,
        look: MapLook::DarkMatter,
    };
    cache.mark_tile_drawn(dark, drawing(&image));
    cache.arrived.insert(dark, 10.0);
    assert_eq!(
        cache.tile_stack(t, MapLook::DarkMatter, 10.0),
        vec![
            piece(t, MapLook::Positron, 0.0, 0.0, 1.0, 1.0),
            piece(t, MapLook::DarkMatter, 0.0, 0.0, 1.0, 0.0),
        ]
    );
}

#[test]
fn a_tile_with_nothing_drawn_anywhere_near_shows_no_drawing() {
    let cache = cache_at(0.0, 0.0, 3.0);
    let look = cache.current_look();
    assert!(cache.tile_stack(tile(3, 1, 1), look, 0.0).is_empty());
}

#[test]
fn the_drawings_a_view_stands_in_with_are_never_evicted() {
    let mut cache = cache_at(0.0, 0.0, 12.0);
    let look = cache.current_look();
    let image = an_image();
    let ancestors = [tile(0, 0, 0), tile(3, 7, 7), tile(5, 31, 0)];
    for a in ancestors {
        settled(&mut cache, a, look, &image);
        cache.stand_ins.insert(TileStyleKey { tile: a, look });
    }
    // Far more drawings than the cap, all closer to the view than the
    // stand-ins are (same zoom as the view).
    for i in 0..(MAX_DRAWN_TILES as u32 + 40) {
        settled(&mut cache, tile(12, 2048 + i % 16, 2048 + i / 16), look, &image);
    }
    cache.prune_distant_tiles();
    for a in ancestors {
        assert!(
            cache.drawn.contains_key(&TileStyleKey { tile: a, look }),
            "{a:?} stands in for a tile of the view and must stay drawn"
        );
    }
    assert!(cache.drawn.len() <= MAX_DRAWN_TILES + ancestors.len());
}

/// The images a tile box holds, bottom first.
fn images_in(tile_box: &Dom) -> Vec<ImageRefHash> {
    tile_box
        .children
        .as_slice()
        .iter()
        .filter_map(|child| match child.root.get_node_type() {
            NodeType::Image(image) => Some(image.get_hash()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_render_shows_the_parent_in_every_tile_box_whose_own_tile_is_missing() {
    let mut cache = cache_at(0.0, 0.0, 3.0);
    let look = cache.current_look();
    let parent = an_image();
    for x in 0..4 {
        for y in 0..4 {
            settled(&mut cache, tile(2, x, y), look, &parent);
        }
    }
    let tiles = map_visible_tiles(
        &view(0.0, 0.0, 3.0),
        LogicalSize::new(512.0, 512.0),
        &layer_zoom(0, 19),
    );
    let dataset = azul_core::refany::RefAny::new(cache);
    let ret = with_virtual_view_info(512.0, 512.0, |info| map_widget_render(dataset.clone(), info));
    let OptionDom::Some(view_root) = &ret.dom else {
        panic!("a finite box renders the grid");
    };
    let grid = view_root.children.as_slice().first().expect("the grid");
    let boxes = &grid.children.as_slice()[..tiles.len()];
    for (index, tile_box) in boxes.iter().enumerate() {
        assert_eq!(
            images_in(tile_box),
            vec![parent.get_hash()],
            "tile box {index} must show its parent while its own tile loads"
        );
    }
}

#[test]
fn a_view_with_nothing_fading_leaves_the_fade_clock_alone() {
    // A render that moved the fade clock to its own time would keep the fade
    // timer drawing the view sixty times a second, forever.
    let mut cache = cache_at(0.0, 0.0, 3.0);
    let look = cache.current_look();
    let image = an_image();
    for t in map_visible_tiles(
        &view(0.0, 0.0, 3.0),
        LogicalSize::new(512.0, 512.0),
        &layer_zoom(0, 19),
    ) {
        settled(&mut cache, t, look, &image);
    }
    let dataset = azul_core::refany::RefAny::new(cache);
    for _ in 0..2 {
        let _ = with_virtual_view_info(512.0, 512.0, |info| map_widget_render(dataset.clone(), info));
    }
    let mut dataset = dataset;
    let cache = dataset.downcast_ref::<MapTileCache>().expect("cache");
    assert_eq!(cache.fade_until, 0.0, "nothing faded: nothing to draw again");
}
