//! The damaged renderer reports the rects it actually painted.
//!
//! Overlapping damage rects are made disjoint before the per-rect clear and
//! repaint: two whose bounding box is barely larger than what they cover
//! become that box, so the renderer writes pixels neither rect asked for;
//! any others are cut apart (a scroll strip and a scrollbar column that meet
//! in a corner are painted as the L they are). The frame presents - and, on
//! an ARGB8888 pool, byte-converts - exactly what the renderer reports, so
//! what it reports has to be what it wrote, or those pixels go out stale or
//! with R and B swapped.

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

/// Repaints `requested` of a blue page over a marked buffer: how many written
/// pixels lie outside what the renderer reported, and outside what was asked.
fn paint(requested: &[LogicalRect]) -> (usize, usize, Vec<LogicalRect>) {
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

    let state = cpurender::CpuRenderState::new(Default::default());
    let painted = cpurender::render_display_list_damaged(
        &dl,
        &mut pixmap,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &state,
        requested,
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
        if !contains(requested, x, y) {
            written_outside_requested += 1;
        }
    }
    (written_outside_reported, written_outside_requested, painted)
}

#[test]
fn every_pixel_it_writes_is_inside_the_damage_it_reports() {
    // Two rects that overlap almost entirely: they are painted as their
    // bounding box, two corners of which neither asked for.
    let requested = [rect(0.0, 0.0, 100.0, 100.0), rect(10.0, 10.0, 100.0, 100.0)];
    let (outside_reported, outside_requested, painted) = paint(&requested);
    assert!(
        outside_requested > 0,
        "premise: the two rects are painted as their bounding box, more than was asked for"
    );
    assert_eq!(
        outside_reported, 0,
        "{outside_reported} painted pixels lie outside the reported damage {painted:?}"
    );
}

#[test]
fn an_l_of_damage_is_painted_as_the_l() {
    // A column and a strip that overlap in the corner: their bounding box is
    // nearly three times what they cover, so they are cut apart, not boxed.
    let requested = [rect(0.0, 0.0, 20.0, 100.0), rect(0.0, 80.0, 100.0, 20.0)];
    let (outside_reported, outside_requested, painted) = paint(&requested);
    assert_eq!(outside_requested, 0, "nothing outside the L is painted: {painted:?}");
    assert_eq!(
        outside_reported, 0,
        "{outside_reported} painted pixels lie outside the reported damage {painted:?}"
    );
}

/// A rounded box whose edge a damage rect boundary crosses is painted, rect by
/// rect, exactly as one full repaint paints it: the anti-aliased pixels of its
/// top row (above the boundary) come out of the first rect's pass bit for bit.
/// Cutting the geometry at the pass's clip changed their coverage by one step
/// (a slider thumb dragged across a strip boundary left a 1-LSB ring).
#[test]
fn a_shape_across_two_damage_rects_is_painted_as_in_one_pass() {
    let dom = Dom::create_body()
        .with_css("margin: 0; background-color: #ffffff;")
        .with_child(Dom::create_div().with_css(
            "margin-left: 103px; margin-top: 44px; width: 16px; height: 16px; \
             border-radius: 8px; background-color: #0d6efd;",
        ));
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut None)
        .unwrap();
    let dl = lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list.clone();
    let mut glyph_cache = GlyphCache::new();
    let opts = RenderOptions {
        width: 200.0,
        height: 100.0,
        dpi_factor: 1.0,
    };
    let full =
        cpurender::render_with_font_manager(&dl, &rr, &lw.font_manager, opts, &mut glyph_cache)
            .unwrap();
    let mut parts = full.clone_pixmap();
    for px in parts.data_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&[1, 2, 3, 4]);
    }
    let state = cpurender::CpuRenderState::new(Default::default());
    cpurender::render_display_list_damaged(
        &dl,
        &mut parts,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &state,
        &[rect(0.0, 0.0, 200.0, 45.0), rect(0.0, 45.0, 200.0, 55.0)],
    )
    .unwrap();
    let w = full.width();
    let differ: Vec<(u32, u32)> = full
        .data()
        .chunks_exact(4)
        .zip(parts.data().chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| a[..3] != b[..3])
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .collect();
    assert!(differ.is_empty(), "{} pixels differ from one full repaint: {:?}", differ.len(), &differ[..differ.len().min(8)]);
}
