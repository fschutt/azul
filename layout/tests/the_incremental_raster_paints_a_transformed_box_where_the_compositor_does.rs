//! The incremental CPU raster paints a transformed box where the layered
//! compositor paints it - moved, turned - never at its layout place.
//!
//! Every CPU backend paints a frame two ways: in full through the layered
//! compositor (a transformed box is a layer, composited through its matrix),
//! and incrementally - only the damage rects - through the flat item walk
//! (`render_display_list_damaged`). The flat walk kept a stack of the
//! reference frames' matrices and nothing read it: every damage rect
//! repainted a transformed box at its LAYOUT place, over whatever the full
//! frame had painted there. A node mid-slide (the reconcile's FLIP, a theme
//! switch) left its old and new pictures on screen at once, and a box with
//! a lasting transform (`translate(-50%)`, a turned icon) grew a ghost
//! wherever a damage rect touched its layout box.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, cpurender, glyph_cache::GlyphCache, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: u32 = 200;
const H: u32 = 120;

fn laid_out(box_css: &str) -> LayoutWindow {
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0; background: #ffffff;")
        .with_child(Dom::create_div().with_css(&format!(
            "width: 40px; height: 40px; background: #ff0000; {box_css}"
        )));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W as f32, H as f32);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

/// What every live CPU backend renders a frame with (the headless backend,
/// the e2e CPU backend, a callback's screenshot): the window's LIVE GPU
/// values. A CSS transform's matrix is computed before the solve from the
/// PREVIOUS pass's sizes and refreshed against the laid-out box right after
/// it (`GpuValueCache::refresh_transform_values`); the matrix baked into the
/// display list is only the fallback for a key nothing published. On a
/// first pass that fallback had no size, so `rotate()` turned about the
/// corner (`transform-origin` 50% of nothing) - which is what this harness
/// painted while it rendered with no values at all.
fn live_state(lw: &LayoutWindow) -> cpurender::CpuRenderState {
    cpurender::CpuRenderState::from_gpu_cache(
        lw.gpu_state_manager.get_cache(DomId::ROOT_ID),
        DomId::ROOT_ID,
        &Default::default(),
    )
}

/// The frame painted in full, through the layered compositor.
fn composited(lw: &LayoutWindow) -> cpurender::AzulPixmap {
    let dl = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list;
    let rr = RendererResources::default();
    let mut glyph_cache = GlyphCache::new();
    let render_state = live_state(lw);
    let mut compositor = cpurender::CompositorState::new(W, H);
    compositor.allocate_layers_from_display_list(
        dl,
        1.0,
        &render_state.transforms,
        &render_state.opacities,
    );
    compositor
        .render_layers(
            dl,
            1.0,
            &rr,
            &lw.font_manager,
            &mut glyph_cache,
            &render_state,
        )
        .expect("the layers render");
    let mut out = cpurender::AzulPixmap::new(W, H).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    out
}

/// The frame painted incrementally: the whole window as one damage rect,
/// over a buffer that holds nothing (grey), as the damaged path paints.
fn repainted(lw: &LayoutWindow) -> cpurender::AzulPixmap {
    let dl = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list;
    let rr = RendererResources::default();
    let mut glyph_cache = GlyphCache::new();
    let render_state = live_state(lw);
    let mut out = cpurender::AzulPixmap::new(W, H).expect("a pixmap");
    out.fill(128, 128, 128, 255);
    let whole = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(W as f32, H as f32),
    );
    cpurender::render_display_list_damaged(
        dl,
        &mut out,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &render_state,
        &[whole],
    )
    .expect("the damage repaints");
    out
}

fn red_at(p: &cpurender::AzulPixmap, x: u32, y: u32) -> bool {
    let i = ((y * W + x) * 4) as usize;
    let d = p.data();
    d[i] > 200 && d[i + 1] < 60 && d[i + 2] < 60
}

#[test]
fn a_translated_box_is_repainted_where_it_is_moved_to() {
    let lw = laid_out("transform: translate(100px, 50px);");
    let full = composited(&lw);
    assert!(
        red_at(&full, 120, 70) && !red_at(&full, 20, 20),
        "harness: the compositor paints the box moved by (100, 50)"
    );
    let damaged = repainted(&lw);
    assert!(
        red_at(&damaged, 120, 70),
        "the incremental raster paints the translated box at (100, 50)-(140, 90)"
    );
    assert!(
        !red_at(&damaged, 20, 20),
        "nothing of the translated box is painted at its layout place (0, 0)-(40, 40)"
    );
}

#[test]
fn a_turned_box_is_repainted_turned() {
    // Turned 45 degrees about its centre (20, 20): a diamond. Its layout
    // corner (2, 2) is outside it, its centre inside.
    let lw = laid_out("transform: rotate(45deg);");
    let full = composited(&lw);
    assert!(
        red_at(&full, 20, 20) && !red_at(&full, 2, 2),
        "harness: the compositor paints the box turned"
    );
    let damaged = repainted(&lw);
    assert!(red_at(&damaged, 20, 20), "the turned box covers its centre");
    assert!(
        !red_at(&damaged, 2, 2),
        "the incremental raster paints the box turned: its layout corner is not covered"
    );
}

#[test]
fn the_incremental_raster_and_the_compositor_paint_the_same_transformed_frame() {
    for css in [
        "transform: translate(100px, 50px);",
        "transform: rotate(45deg);",
        "transform: translate(30px, 20px) scale(1.5);",
    ] {
        let lw = laid_out(css);
        let (full, damaged) = (composited(&lw), repainted(&lw));
        let differing = full
            .data()
            .chunks_exact(4)
            .zip(damaged.data().chunks_exact(4))
            .filter(|(a, b)| {
                a.iter()
                    .zip(b.iter())
                    .any(|(x, y)| (i16::from(*x) - i16::from(*y)).abs() > 48)
            })
            .count();
        // A few edge pixels may round differently; a misplaced box is
        // hundreds.
        assert!(
            differing <= 40,
            "`{css}`: the two paths disagree on {differing} pixels"
        );
    }
}
