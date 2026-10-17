//! The damaged renderer reports the rects it actually painted.
//!
//! Overlapping damage rects are merged into their bounding box before the
//! per-rect clear and repaint, so the renderer writes pixels between them
//! that neither rect asked for (a scroll strip and a scrollbar column that
//! meet in a corner). The frame presents - and, on an ARGB8888 pool,
//! byte-converts - exactly what the renderer reports, so what it reports has
//! to be what it wrote, or those pixels go out stale or with R and B swapped.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

fn contains(rects: &[LogicalRect], x: u32, y: u32) -> bool {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    rects.iter().any(|r| {
        fx >= r.origin.x
            && fy >= r.origin.y
            && fx < r.origin.x + r.size.width
            && fy < r.origin.y + r.size.height
    })
}

#[test]
fn every_pixel_it_writes_is_inside_the_damage_it_reports() {
    let dom = Dom::create_body().with_css("margin: 0; background-color: #3c78c8;");
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 200.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    let dl = lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list.clone();

    let mut glyph_cache = GlyphCache::new();
    let opts = RenderOptions {
        width: 200.0,
        height: 200.0,
        dpi_factor: 1.0,
    };
    let mut pixmap =
        cpurender::render_with_font_manager(&dl, &rr, &lw.font_manager, opts, &mut glyph_cache)
            .unwrap();

    const MARK: [u8; 4] = [1, 2, 3, 4];
    for px in pixmap.data_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&MARK);
    }

    // Two squares overlapping on the diagonal. Their bounding box is barely
    // bigger than the pair, so the renderer merges them and paints the two
    // empty corners as well - more than was asked for, which is the case
    // this law is about.
    let requested = [rect(0.0, 0.0, 50.0, 50.0), rect(25.0, 25.0, 50.0, 50.0)];
    let state = cpurender::CpuRenderState::new(Default::default());
    let painted = cpurender::render_display_list_damaged(
        &dl,
        &mut pixmap,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &state,
        &requested,
    )
    .unwrap();

    let w = pixmap.width();
    let mut written_outside_reported = 0usize;
    let mut written_outside_requested = 0usize;
    for (i, px) in pixmap.data().chunks_exact(4).enumerate() {
        if px == MARK {
            continue;
        }
        let (x, y) = (i as u32 % w, i as u32 / w);
        if !contains(&painted, x, y) {
            written_outside_reported += 1;
        }
        if !contains(&requested, x, y) {
            written_outside_requested += 1;
        }
    }
    assert!(
        written_outside_requested > 0,
        "premise: merging the two squares paints their bounding box, i.e. more than was asked for"
    );
    assert_eq!(
        written_outside_reported, 0,
        "{written_outside_reported} painted pixels lie outside the reported damage {painted:?}"
    );

    // An L - a column and a strip meeting in the corner - is the shape whose
    // bounding box is the whole surface. It is kept apart instead of merged,
    // so here the renderer paints exactly what it was asked for, and the law
    // above holds with nothing to spare.
    for px in pixmap.data_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&MARK);
    }
    let l = [rect(0.0, 0.0, 20.0, 100.0), rect(0.0, 80.0, 100.0, 20.0)];
    let painted_l = cpurender::render_display_list_damaged(
        &dl,
        &mut pixmap,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &state,
        &l,
    )
    .unwrap();
    let mut l_outside_reported = 0usize;
    let mut l_outside_requested = 0usize;
    for (i, px) in pixmap.data().chunks_exact(4).enumerate() {
        if px == MARK {
            continue;
        }
        let (x, y) = (i as u32 % w, i as u32 / w);
        if !contains(&painted_l, x, y) {
            l_outside_reported += 1;
        }
        if !contains(&l, x, y) {
            l_outside_requested += 1;
        }
    }
    assert_eq!(
        l_outside_reported, 0,
        "{l_outside_reported} painted pixels lie outside the reported damage {painted_l:?}"
    );
    assert_eq!(
        l_outside_requested, 0,
        "an L is not worth its bounding box: {l_outside_requested} pixels painted outside it, \
         reported {painted_l:?}"
    );
}
