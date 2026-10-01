//! The raster core's behaviour, stated as sentences.
//!
//! Pure pixel tests: no window. The brush tests build their dab stamps with
//! azul's `RawImage::paint_dot` (the one brush profile in azul), so they link
//! libazul like every app test (`AZ_LINK_PATH`).

use std::sync::Arc;

use super::*;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn close(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= tolerance)
}

#[track_caller]
fn assert_px(actual: [u8; 4], expected: [u8; 4]) {
    assert!(close(actual, expected, 1), "pixel {actual:?}, expected {expected:?}");
}

/// A `w` x `h` RGBA8 buffer of one colour.
fn solid(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
    c.iter().copied().cycle().take((w * h * 4) as usize).collect()
}

/// An engine over a white `w` x `h` background.
fn white_engine(w: u32, h: u32) -> TileEngine {
    TileEngine::new(Document::with_background(w, h, WHITE))
}

fn brush(color: [u8; 4], size: f32) -> BrushSettings {
    BrushSettings {
        tool: BrushTool::Brush,
        color,
        size,
        hardness: 1.0,
        opacity: 1.0,
        flow: 1.0,
        spacing: 0.1,
        pressure_size: false,
        pressure_flow: false,
    }
}

fn pt(x: f32, y: f32) -> StrokePoint {
    StrokePoint { x, y, pressure: 1.0 }
}

fn layer_pixel(engine: &TileEngine, id: LayerId, x: u32, y: u32) -> [u8; 4] {
    match &layer::find(&engine.document().layers, id).expect("layer").content {
        LayerContent::Raster(grid) => grid.pixel(x, y),
        _ => panic!("not a raster layer"),
    }
}

// ==== Rects ====

#[test]
fn two_rects_unite_to_their_bounding_box_and_an_empty_one_adds_nothing() {
    let a = IRect::new(0, 0, 10, 10);
    assert_eq!(a.union(&IRect::new(20, 5, 5, 5)), IRect::new(0, 0, 25, 10));
    assert_eq!(a.union(&IRect::new(90, 90, 0, 0)), a);
    assert_eq!(IRect::new(90, 90, 0, 0).union(&a), a);
    assert_eq!(a.intersect(&IRect::new(5, 5, 10, 10)), Some(IRect::new(5, 5, 5, 5)));
    assert_eq!(a.intersect(&IRect::new(10, 0, 5, 5)), None, "touching is not overlapping");
}

// ==== Tiles ====

#[test]
fn a_grid_reads_back_what_it_was_built_from_across_tile_borders() {
    let (w, h) = (300, 270); // two by two tiles, the last ones partial
    let mut rgba = solid(w, h, CLEAR);
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        px[0] = (i % 251) as u8;
        px[1] = (i / 251 % 251) as u8;
        px[3] = 255;
    }
    let grid = TileGrid::from_rgba(w, h, &rgba);
    assert_eq!((grid.cols(), grid.rows()), (2, 2));
    assert_eq!(grid.to_rgba(), rgba);
    assert_eq!(grid.pixel(299, 269), {
        let i = (269 * w + 299) as usize;
        [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2], rgba[i * 4 + 3]]
    });
}

#[test]
fn an_empty_grid_holds_no_tiles_until_a_pixel_is_written() {
    let mut grid = TileGrid::new(600, 600);
    assert!(grid.non_empty_tiles().is_empty(), "a new layer costs no tile memory");
    grid.set_pixel(300, 300, RED);
    assert_eq!(grid.non_empty_tiles(), vec![(1, 1)]);
    assert_eq!(grid.pixel(300, 300), RED);
    assert_eq!(grid.pixel(0, 0), CLEAR);
}

#[test]
fn a_copied_grid_shares_its_tiles_until_one_is_written() {
    let original = TileGrid::from_rgba(512, 256, &solid(512, 256, WHITE));
    let mut copy = original.clone();
    assert!(copy.shares_tile_with(&original, 0, 0) && copy.shares_tile_with(&original, 1, 0));
    copy.set_pixel(10, 10, RED);
    assert!(!copy.shares_tile_with(&original, 0, 0), "the written tile is copied");
    assert!(copy.shares_tile_with(&original, 1, 0), "the other tile is still shared");
    assert_eq!(original.pixel(10, 10), WHITE, "the original is untouched");
}

// ==== Blend modes (W3C compositing, straight RGBA8 in and out) ====

#[test]
fn normal_blending_at_half_opacity_mixes_half_way() {
    assert_px(blend::blend_rgba8(WHITE, BLUE, 0.5, BlendMode::Normal), [128, 128, 255, 255]);
}

#[test]
fn multiply_darkens_by_the_product() {
    let gray = [128, 128, 128, 255];
    assert_px(blend::blend_rgba8(gray, RED, 1.0, BlendMode::Multiply), [128, 0, 0, 255]);
    assert_px(blend::blend_rgba8(gray, WHITE, 1.0, BlendMode::Multiply), gray);
}

#[test]
fn screen_lightens_by_the_inverse_product() {
    let gray = [128, 128, 128, 255];
    // 1 - (1 - 0.502)^2 = 0.752
    assert_px(blend::blend_rgba8(gray, gray, 1.0, BlendMode::Screen), [192, 192, 192, 255]);
    assert_px(blend::blend_rgba8(gray, CLEAR_BLACK, 1.0, BlendMode::Screen), gray);
}

const CLEAR_BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn overlay_multiplies_dark_backdrops_and_screens_light_ones() {
    let mid = [128, 128, 128, 255];
    // backdrop 0.25: 2 * 0.25 * 0.502 = 0.251
    assert_px(blend::blend_rgba8([64, 64, 64, 255], mid, 1.0, BlendMode::Overlay), [64, 64, 64, 255]);
    // backdrop 0.75: 1 - 2 * 0.25 * 0.498 = 0.751
    assert_px(blend::blend_rgba8([191, 191, 191, 255], mid, 1.0, BlendMode::Overlay), [191, 191, 191, 255]);
    // a white source over a dark backdrop: 2 * 0.25 * 1 = 0.5
    assert_px(blend::blend_rgba8([64, 64, 64, 255], WHITE, 1.0, BlendMode::Overlay), [128, 128, 128, 255]);
}

#[test]
fn a_blend_mode_only_acts_where_the_backdrop_is_opaque() {
    // Over a transparent backdrop every mode is plain source-over.
    for mode in BlendMode::ALL {
        assert_px(blend::blend_rgba8(CLEAR, RED, 1.0, mode), RED);
    }
}

#[test]
fn every_blend_mode_has_a_name_that_maps_back() {
    for mode in BlendMode::ALL {
        assert_eq!(BlendMode::from_name(mode.name()), Some(mode));
    }
}
