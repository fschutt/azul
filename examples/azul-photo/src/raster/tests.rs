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

// ==== Layers, compositing, dirty tiles ====

#[test]
fn a_new_layer_lands_above_the_active_one_and_becomes_active() {
    let mut e = white_engine(64, 64);
    let background = e.active_layer().expect("the background is active");
    e.apply(Op::NewLayer { name: "Sky".into() }).unwrap();
    let sky = e.active_layer().unwrap();
    assert_ne!(sky, background);
    let order: Vec<LayerId> = e.document().layers.iter().map(|l| l.id).collect();
    assert_eq!(order, vec![background, sky], "bottom to top");
}

#[test]
fn the_composite_shows_a_layer_at_its_opacity() {
    let mut e = white_engine(64, 64);
    e.apply(Op::NewLayer { name: "Blue".into() }).unwrap();
    let blue = e.active_layer().unwrap();
    e.apply(Op::FillSelection(BLUE)).unwrap(); // no selection: the whole layer
    e.take_dirty();
    assert_px(e.sample(10, 10), BLUE);
    e.apply(Op::SetOpacity(blue, 0.5)).unwrap();
    e.take_dirty();
    assert_px(e.sample(10, 10), [128, 128, 255, 255]);
    e.apply(Op::SetVisible(blue, false)).unwrap();
    e.take_dirty();
    assert_px(e.sample(10, 10), WHITE);
}

#[test]
fn a_layer_in_multiply_mode_darkens_the_layers_below() {
    let mut e = TileEngine::new(Document::with_background(32, 32, [128, 128, 128, 255]));
    e.apply(Op::NewLayer { name: "Red".into() }).unwrap();
    let red = e.active_layer().unwrap();
    e.apply(Op::FillSelection(RED)).unwrap();
    e.apply(Op::SetBlend(red, BlendMode::Multiply)).unwrap();
    e.take_dirty();
    assert_px(e.sample(0, 0), [128, 0, 0, 255]);
}

#[test]
fn painting_a_small_dab_composites_only_the_tiles_it_touched() {
    let mut e = white_engine(1024, 1024); // 4 x 4 tiles
    let first = e.take_dirty().expect("a new document is dirty everywhere");
    assert_eq!(first, IRect::new(0, 0, 1024, 1024));
    assert_eq!(e.take_dirty(), None, "nothing changed since");
    e.apply(Op::NewLayer { name: "Ink".into() }).unwrap();
    let _ = e.take_dirty();
    e.begin_stroke(brush(RED, 8.0), pt(300.0, 300.0)).unwrap();
    e.end_stroke();
    let dirty = e.take_dirty().expect("the dab changed pixels");
    assert_eq!(dirty, IRect::new(256, 256, 256, 256), "one tile, not the canvas");
    assert_px(e.sample(300, 300), RED);
    assert_px(e.sample(600, 600), WHITE);
}

#[test]
fn a_group_composites_its_children_and_then_applies_its_own_opacity() {
    let mut e = white_engine(16, 16);
    e.apply(Op::NewLayer { name: "Red".into() }).unwrap();
    e.apply(Op::FillSelection(RED)).unwrap();
    let red = e.active_layer().unwrap();
    e.apply(Op::NewGroup).unwrap();
    let group = e.active_layer().unwrap();
    e.apply(Op::MoveLayer { id: red, to: Placement::IntoGroup(group) }).unwrap();
    assert!(matches!(
        &layer::find(&e.document().layers, group).unwrap().content,
        LayerContent::Group(children) if children.len() == 1 && children[0].id == red
    ));
    e.apply(Op::SetOpacity(group, 0.5)).unwrap();
    e.take_dirty();
    assert_px(e.sample(4, 4), [255, 128, 128, 255]);
}

#[test]
fn reordering_puts_a_layer_above_another() {
    let mut e = white_engine(8, 8);
    e.apply(Op::NewLayer { name: "A".into() }).unwrap();
    let a = e.active_layer().unwrap();
    e.apply(Op::FillSelection(RED)).unwrap();
    e.apply(Op::NewLayer { name: "B".into() }).unwrap();
    let b = e.active_layer().unwrap();
    e.apply(Op::FillSelection(GREEN)).unwrap();
    e.take_dirty();
    assert_px(e.sample(0, 0), GREEN);
    e.apply(Op::MoveLayer { id: a, to: Placement::Above(b) }).unwrap();
    e.take_dirty();
    assert_px(e.sample(0, 0), RED);
}

#[test]
fn duplicate_copies_and_merge_down_flattens_two_layers_into_one() {
    let mut e = white_engine(8, 8);
    e.apply(Op::NewLayer { name: "Half blue".into() }).unwrap();
    let top = e.active_layer().unwrap();
    e.apply(Op::FillSelection(BLUE)).unwrap();
    e.apply(Op::SetOpacity(top, 0.5)).unwrap();
    e.apply(Op::DuplicateLayer(top)).unwrap();
    assert_eq!(e.document().layers.len(), 3);
    let copy = e.active_layer().unwrap();
    assert_ne!(copy, top);
    e.apply(Op::DeleteLayer(copy)).unwrap();
    e.take_dirty();
    let before = e.sample(3, 3);
    e.apply(Op::MergeDown(top)).unwrap();
    assert_eq!(e.document().layers.len(), 1, "merged into the background");
    e.take_dirty();
    assert_px(e.sample(3, 3), before);
}

#[test]
fn a_locked_layer_refuses_paint() {
    let mut e = white_engine(16, 16);
    let bg = e.active_layer().unwrap();
    e.apply(Op::SetLocked(bg, true)).unwrap();
    assert_eq!(e.begin_stroke(brush(RED, 4.0), pt(8.0, 8.0)), Err(EngineError::Locked));
    assert_eq!(e.apply(Op::FillSelection(RED)), Err(EngineError::Locked));
    assert_eq!(layer_pixel(&e, bg, 8, 8), WHITE);
}

// ==== Adjustments: non-destructive layers ====

#[test]
fn the_adjustments_map_colours_as_documented() {
    let c = [0.2, 0.4, 0.6];
    let inv = Adjustment::Invert.apply(c);
    assert!((inv[0] - 0.8).abs() < 1e-5 && (inv[2] - 0.4).abs() < 1e-5);
    let t = Adjustment::Threshold { level: 128 };
    assert_eq!(t.apply([0.9, 0.9, 0.9]), [1.0, 1.0, 1.0]);
    assert_eq!(t.apply([0.1, 0.2, 0.1]), [0.0, 0.0, 0.0]);
    let levels = Adjustment::Levels {
        in_black: 51,
        in_white: 204,
        gamma: 1.0,
        out_black: 0,
        out_white: 255,
    };
    let l = levels.apply([0.2, 0.8, 0.5]);
    assert!(l[0].abs() < 0.01 && (l[1] - 1.0).abs() < 0.01 && (l[2] - 0.5).abs() < 0.01, "{l:?}");
    let identity = Adjustment::Curves { points: vec![(0, 0), (255, 255)] };
    let i = identity.apply(c);
    assert!(i.iter().zip(c.iter()).all(|(a, b)| (a - b).abs() < 0.01), "{i:?}");
    let darker = Adjustment::Curves { points: vec![(0, 0), (128, 64), (255, 255)] };
    assert!(darker.apply([0.5, 0.5, 0.5])[0] < 0.3);
    let cyan = Adjustment::HueSaturation { hue: 180.0, saturation: 0.0, lightness: 0.0 }
        .apply([1.0, 0.0, 0.0]);
    assert!(cyan[0] < 0.01 && cyan[1] > 0.99 && cyan[2] > 0.99, "{cyan:?}");
    let gray = Adjustment::HueSaturation { hue: 0.0, saturation: -1.0, lightness: 0.0 }
        .apply([1.0, 0.0, 0.0]);
    assert!((gray[0] - gray[1]).abs() < 0.01 && (gray[1] - gray[2]).abs() < 0.01, "{gray:?}");
    let brighter = Adjustment::BrightnessContrast { brightness: 0.2, contrast: 0.0 }
        .apply([0.5, 0.5, 0.5]);
    assert!((brighter[0] - 0.7).abs() < 0.01, "{brighter:?}");
    let flat = Adjustment::BrightnessContrast { brightness: 0.0, contrast: -1.0 }
        .apply([0.9, 0.1, 0.5]);
    assert!(flat.iter().all(|v| (v - 0.5).abs() < 0.01), "no contrast is mid gray: {flat:?}");
}

#[test]
fn an_adjustment_layer_changes_the_composite_but_not_the_pixels_below() {
    let mut e = TileEngine::new(Document::with_background(16, 16, [51, 102, 153, 255]));
    let bg = e.active_layer().unwrap();
    e.apply(Op::NewAdjustment(Adjustment::Invert)).unwrap();
    let inv = e.active_layer().unwrap();
    e.take_dirty();
    assert_px(e.sample(1, 1), [204, 153, 102, 255]);
    assert_eq!(layer_pixel(&e, bg, 1, 1), [51, 102, 153, 255], "non-destructive");
    e.apply(Op::SetOpacity(inv, 0.5)).unwrap();
    e.take_dirty();
    assert_px(e.sample(1, 1), [128, 128, 128, 255]);
    e.apply(Op::DeleteLayer(inv)).unwrap();
    e.take_dirty();
    assert_px(e.sample(1, 1), [51, 102, 153, 255]);
}

// ==== Selections: masks ====

#[test]
fn rect_selections_add_subtract_and_intersect() {
    let a = Mask::from_shape(20, 20, &Shape::Rect(IRect::new(0, 0, 10, 10)));
    let b = Mask::from_shape(20, 20, &Shape::Rect(IRect::new(5, 5, 10, 10)));
    let count = |m: &Mask| m.data.iter().filter(|v| **v >= 128).count();
    assert_eq!(count(&a), 100);
    assert_eq!(count(&a.combine(&b, SelectMode::Add)), 175);
    assert_eq!(count(&a.combine(&b, SelectMode::Subtract)), 75);
    assert_eq!(count(&a.combine(&b, SelectMode::Intersect)), 25);
    assert_eq!(count(&a.combine(&b, SelectMode::Replace)), 100);
    assert_eq!(a.combine(&b, SelectMode::Intersect).bounds(), Some(IRect::new(5, 5, 5, 5)));
    assert_eq!(count(&a.invert()), 300);
}

#[test]
fn an_ellipse_selection_covers_its_area_with_soft_edges() {
    let m = Mask::from_shape(100, 100, &Shape::Ellipse(IRect::new(10, 10, 80, 60)));
    let area: f32 = m.data.iter().map(|v| *v as f32 / 255.0).sum();
    let expected = std::f32::consts::PI * 40.0 * 30.0;
    assert!((area - expected).abs() / expected < 0.02, "area {area} vs {expected}");
    assert_eq!(m.get(50, 40), 255, "the centre is in");
    assert_eq!(m.get(10, 10), 0, "the bounding box corner is out");
    assert!(m.data.iter().any(|v| *v > 0 && *v < 255), "the rim is anti-aliased");
}

#[test]
fn a_lasso_polygon_selects_its_inside() {
    // A right triangle with legs of 40: area 800.
    let tri = Shape::Polygon(vec![(10.0, 10.0), (50.0, 10.0), (10.0, 50.0)]);
    let m = Mask::from_shape(64, 64, &tri);
    let area: f32 = m.data.iter().map(|v| *v as f32 / 255.0).sum();
    assert!((area - 800.0).abs() < 20.0, "area {area}");
    assert_eq!(m.get(15, 15), 255);
    assert_eq!(m.get(45, 45), 0);
}

#[test]
fn feathering_softens_a_hard_edge_but_keeps_the_inside() {
    let hard = Mask::from_shape(64, 64, &Shape::Rect(IRect::new(16, 16, 32, 32)));
    let soft = hard.feather(4.0);
    assert_eq!(soft.get(32, 32), 255, "deep inside stays selected");
    assert_eq!(soft.get(2, 2), 0, "far outside stays unselected");
    let edge = soft.get(16, 32);
    assert!(edge > 60 && edge < 200, "the edge is half selected: {edge}");
}

#[test]
fn the_magic_wand_selects_similar_connected_pixels_within_its_tolerance() {
    // Left half: reds that differ by 10, right half: blue; a red island far right.
    let (w, h) = (40u32, 10u32);
    let mut rgba = solid(w, h, BLUE);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if x < 20 {
                rgba[i..i + 4].copy_from_slice(&[(240 + (x % 2) * 10) as u8, 0, 0, 255]);
            } else if x >= 36 {
                rgba[i..i + 4].copy_from_slice(&[245, 0, 0, 255]);
            }
        }
    }
    let grid = TileGrid::from_rgba(w, h, &rgba);
    let count = |m: &Mask| m.data.iter().filter(|v| **v >= 128).count();
    let tight = Mask::magic_wand(&grid, 0, 0, 5, true);
    assert_eq!(count(&tight), 10, "tolerance 5 keeps the seed column: its neighbours differ by 10");
    let loose = Mask::magic_wand(&grid, 0, 0, 12, true);
    assert_eq!(count(&loose), 200, "tolerance 12 spans the whole red half");
    let global = Mask::magic_wand(&grid, 0, 0, 12, false);
    assert_eq!(count(&global), 240, "not contiguous: the far island too");
}

#[test]
fn a_selection_confines_fills_and_paint() {
    let mut e = white_engine(32, 32);
    let bg = e.active_layer().unwrap();
    e.apply(Op::Select(Shape::Rect(IRect::new(0, 0, 16, 32)), SelectMode::Replace)).unwrap();
    e.apply(Op::FillSelection(RED)).unwrap();
    assert_eq!(layer_pixel(&e, bg, 4, 4), RED);
    assert_eq!(layer_pixel(&e, bg, 20, 4), WHITE);
    e.begin_stroke(brush(BLUE, 10.0), pt(16.0, 20.0)).unwrap();
    e.end_stroke();
    assert_px(layer_pixel(&e, bg, 14, 20), BLUE);
    assert_eq!(layer_pixel(&e, bg, 18, 20), WHITE, "outside the selection");
    e.apply(Op::InvertSelection).unwrap();
    e.apply(Op::FillSelection(GREEN)).unwrap();
    assert_eq!(layer_pixel(&e, bg, 20, 4), GREEN);
    assert_eq!(layer_pixel(&e, bg, 4, 4), RED);
    e.apply(Op::Deselect).unwrap();
    assert!(e.document().selection.is_none());
}

#[test]
fn the_bucket_fills_the_connected_region_under_the_click() {
    let mut e = white_engine(20, 20);
    let bg = e.active_layer().unwrap();
    e.apply(Op::DrawShape { shape: Shape::Rect(IRect::new(0, 9, 20, 2)), color: BLUE }).unwrap();
    e.apply(Op::FloodFill { x: 5, y: 2, color: RED, tolerance: 10, contiguous: true }).unwrap();
    assert_eq!(layer_pixel(&e, bg, 5, 2), RED);
    assert_eq!(layer_pixel(&e, bg, 19, 0), RED);
    assert_eq!(layer_pixel(&e, bg, 5, 9), BLUE, "the line stops the fill");
    assert_eq!(layer_pixel(&e, bg, 5, 15), WHITE, "the other side is not connected");
}

// ==== Brush engine ====

#[test]
fn a_hard_dab_paints_its_disc_and_nothing_else() {
    let mut e = white_engine(64, 64);
    let bg = e.active_layer().unwrap();
    e.begin_stroke(brush(RED, 10.0), pt(20.0, 20.0)).unwrap();
    e.end_stroke();
    assert_px(layer_pixel(&e, bg, 20, 20), RED);
    assert_px(layer_pixel(&e, bg, 22, 18), RED);
    assert_eq!(layer_pixel(&e, bg, 27, 20), WHITE, "outside the radius");
    assert_eq!(layer_pixel(&e, bg, 40, 40), WHITE);
}

#[test]
fn a_stroke_never_builds_up_past_its_opacity() {
    let mut e = white_engine(64, 64);
    e.apply(Op::NewLayer { name: "Glaze".into() }).unwrap();
    let glaze = e.active_layer().unwrap();
    let mut settings = brush(BLUE, 12.0);
    settings.opacity = 0.5;
    e.begin_stroke(settings, pt(10.0, 32.0)).unwrap();
    e.stroke_to(pt(50.0, 32.0));
    e.stroke_to(pt(10.0, 32.0)); // back over the same pixels
    e.end_stroke();
    let a = layer_pixel(&e, glaze, 30, 32)[3];
    assert!(a.abs_diff(128) <= 2, "one stroke caps at its opacity: alpha {a}");
    // A second stroke builds up.
    e.begin_stroke(settings, pt(30.0, 32.0)).unwrap();
    e.end_stroke();
    let a2 = layer_pixel(&e, glaze, 30, 32)[3];
    assert!(a2.abs_diff(191) <= 3, "a new stroke paints over the first: alpha {a2}");
}

#[test]
fn a_low_flow_builds_up_within_one_stroke() {
    let mut e = white_engine(64, 64);
    e.apply(Op::NewLayer { name: "Airbrush".into() }).unwrap();
    let id = e.active_layer().unwrap();
    let mut settings = brush(BLUE, 12.0);
    settings.flow = 0.2;
    e.begin_stroke(settings, pt(32.0, 32.0)).unwrap();
    e.end_stroke();
    let once = layer_pixel(&e, id, 32, 32)[3];
    e.apply(Op::DeleteLayer(id)).unwrap();
    e.apply(Op::NewLayer { name: "Airbrush 2".into() }).unwrap();
    let id = e.active_layer().unwrap();
    e.begin_stroke(settings, pt(32.0, 32.0)).unwrap();
    for _ in 0..10 {
        e.stroke_to(pt(33.0, 32.0));
        e.stroke_to(pt(32.0, 32.0));
    }
    e.end_stroke();
    let many = layer_pixel(&e, id, 32, 32)[3];
    assert!(once.abs_diff(51) <= 3, "one dab at flow 0.2: alpha {once}");
    assert!(many > 200, "dabs over the same spot add up to the opacity: alpha {many}");
}

#[test]
fn the_eraser_takes_alpha_away() {
    let mut e = white_engine(32, 32);
    let bg = e.active_layer().unwrap();
    let mut eraser = brush(RED, 10.0);
    eraser.tool = BrushTool::Eraser;
    e.begin_stroke(eraser, pt(16.0, 16.0)).unwrap();
    e.end_stroke();
    assert_eq!(layer_pixel(&e, bg, 16, 16)[3], 0, "erased to transparent");
    assert_eq!(layer_pixel(&e, bg, 2, 2), WHITE);
    e.take_dirty();
    // The composite shows the transparency (the canvas draws its checkerboard there).
    assert_eq!(e.sample(16, 16)[3], 0);
}

#[test]
fn pen_pressure_scales_the_dab_when_asked_to() {
    let mut e = white_engine(64, 64);
    let bg = e.active_layer().unwrap();
    let mut settings = brush(RED, 20.0);
    settings.pressure_size = true;
    e.begin_stroke(settings, StrokePoint { x: 32.0, y: 32.0, pressure: 0.25 }).unwrap();
    e.end_stroke();
    assert_px(layer_pixel(&e, bg, 32, 32), RED);
    assert_eq!(layer_pixel(&e, bg, 32 + 6, 32), WHITE, "a quarter pressure is a quarter size");
    settings.pressure_size = false;
    e.begin_stroke(settings, StrokePoint { x: 10.0, y: 10.0, pressure: 0.25 }).unwrap();
    e.end_stroke();
    assert_px(layer_pixel(&e, bg, 10 + 6, 10), RED, "without the mapping the size stays");
}

#[test]
fn the_pencil_paints_hard_pixels_without_anti_aliasing() {
    let mut e = white_engine(32, 32);
    let bg = e.active_layer().unwrap();
    let mut pencil = brush(BLUE, 5.0);
    pencil.tool = BrushTool::Pencil;
    pencil.hardness = 0.0; // ignored: a pencil is always hard
    e.begin_stroke(pencil, pt(16.0, 16.0)).unwrap();
    e.end_stroke();
    for y in 10..22 {
        for x in 10..22 {
            let p = layer_pixel(&e, bg, x, y);
            assert!(p == WHITE || p == BLUE, "pencil pixel ({x},{y}) is {p:?}");
        }
    }
    assert_eq!(layer_pixel(&e, bg, 16, 16), BLUE);
}

#[test]
fn the_clone_stamp_copies_from_its_source_offset() {
    let mut e = white_engine(64, 32);
    let bg = e.active_layer().unwrap();
    e.apply(Op::DrawShape { shape: Shape::Rect(IRect::new(0, 0, 16, 32)), color: GREEN }).unwrap();
    let mut stamp = brush(RED, 8.0);
    stamp.tool = BrushTool::Clone { dx: -40.0, dy: 0.0 };
    e.begin_stroke(stamp, pt(48.0, 16.0)).unwrap();
    e.end_stroke();
    assert_px(layer_pixel(&e, bg, 48, 16), GREEN);
    assert_eq!(layer_pixel(&e, bg, 30, 16), WHITE);
}
