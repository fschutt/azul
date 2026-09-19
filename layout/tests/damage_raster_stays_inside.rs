//! The damaged renderer writes exactly the damage it was given.
//!
//! Overlapping damage rects used to be merged into their bounding BOX, and
//! the whole box was cleared and repainted. The box covers pixels neither rect
//! asked for (between a scroll strip and a scrollbar column that meet in a
//! corner), while the frame reports only the requested rects - so those
//! pixels were rewritten without being presented, and on an ARGB8888
//! commit-swizzle pool without being byte-converted.

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

#[test]
fn overlapping_damage_rects_repaint_their_union_and_nothing_else() {
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

    // An L: a column and a strip that overlap in the corner (0..20, 80..100).
    let column = rect(0.0, 0.0, 20.0, 100.0);
    let strip = rect(0.0, 80.0, 100.0, 20.0);
    let state = cpurender::CpuRenderState::new(Default::default());
    cpurender::render_display_list_damaged(
        &dl,
        &mut pixmap,
        1.0,
        &rr,
        &lw.font_manager,
        &mut glyph_cache,
        &state,
        &[column, strip],
    )
    .unwrap();

    let inside = |x: u32, y: u32| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        [column, strip].iter().any(|r| {
            fx >= r.origin.x
                && fy >= r.origin.y
                && fx < r.origin.x + r.size.width
                && fy < r.origin.y + r.size.height
        })
    };
    let w = pixmap.width();
    let (mut written_inside, mut written_outside) = (0usize, 0usize);
    for (i, px) in pixmap.data().chunks_exact(4).enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        if px != MARK {
            if inside(x, y) {
                written_inside += 1;
            } else {
                written_outside += 1;
            }
        }
    }
    assert!(written_inside > 0, "premise: the damage itself was repainted");
    assert_eq!(
        written_outside, 0,
        "{written_outside} pixels outside the two damage rects were rewritten (the \
         bounding box of the pair was repainted)"
    );
}
