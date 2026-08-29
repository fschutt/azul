//! Landmine 4 from the damage investigation: an element that is ITSELF the
//! scroll container with DIRECT text children (no `<p>` wrapper — the
//! degenerate topology every widget avoids) captures its PRE-extension box as
//! the text clip. Scroll past the box height and `render_text`'s early-skip
//! tests the scrolled clip against the active clip and drops the whole run —
//! the box renders EMPTY even though the scrolled-to lines should be visible.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
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

const W: f32 = 300.0;
const H: f32 = 220.0;
const BOX_H: f32 = 100.0;

fn non_white_pixels_in(
    pix: &cpurender::AzulPixmap,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
) -> usize {
    let data = pix.data();
    let w = pix.width() as usize;
    let mut n = 0;
    for y in y0..y1.min(pix.height() as usize) {
        for x in x0..x1.min(w) {
            let i = (y * w + x) * 4;
            if data[i] < 240 || data[i + 1] < 240 || data[i + 2] < 240 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_scroll_container_with_direct_text_still_paints_after_scrolling_past_its_own_height() {
    std::env::set_var("AZ_SUPPRESS", "div_as_text");
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut glyphs = GlyphCache::new();
    let dom_id = DomId::ROOT_ID;

    let long_text = (0..40)
        .map(|i| format!("line {i} of the degenerate scroller"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css(
                "width: 260px; height: 100px; overflow-y: auto; background: white; font-size: \
                 14px;",
            )
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                long_text,
            )),
    );
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { background: white; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();

    let opts = RenderOptions {
        width: W,
        height: H,
        dpi_factor: 1.0,
    };
    let lr = lw.get_layout_result(&dom_id).expect("layout");
    let dl = lr.display_list.clone();

    // Sanity: unscrolled, glyphs paint inside the box.
    let state0 = cpurender::CpuRenderState::new(cpurender::ScrollOffsetMap::new());
    let pix0 = cpurender::render_with_font_manager_and_scroll(
        &dl, &rr, &lw.font_manager, opts, &mut glyphs, &state0,
    )
    .unwrap();
    let ink0 = non_white_pixels_in(&pix0, 0, 0, 260, BOX_H as usize);
    assert!(ink0 > 50, "premise: the unscrolled box paints text (ink {ink0})");

    // Scroll PAST the box's own height and re-render with the offset applied
    // the same way the shells do (scroll_id-keyed offsets at raster time).
    let mut offsets = cpurender::ScrollOffsetMap::new();
    let n_scroll_frames = dl
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                azul_layout::solver3::display_list::DisplayListItem::PushScrollFrame { .. }
            )
        })
        .count();
    assert!(
        !lr.scroll_ids.is_empty(),
        "premise: the box registered a scroll id (DL has {n_scroll_frames}          scroll frames, {} items) — if this fails, the degenerate topology          never even became scrollable: the necessity check missed the IFC's          own content extension",
        dl.items.len(),
    );
    for (_layout_idx, scroll_id) in lr.scroll_ids.iter() {
        offsets.insert(*scroll_id, (0.0, 140.0));
    }
    let state1 = cpurender::CpuRenderState::new(offsets);
    let pix1 = cpurender::render_with_font_manager_and_scroll(
        &dl, &rr, &lw.font_manager, opts, &mut glyphs, &state1,
    )
    .unwrap();
    let ink1 = non_white_pixels_in(&pix1, 0, 0, 260, BOX_H as usize);
    assert!(
        ink1 > 50,
        "scrolled 140px into a {BOX_H}px-tall degenerate scroller the box \
         paints NOTHING (ink {ink1}) — the whole run was early-skipped \
         against the pre-extension clip"
    );
}
