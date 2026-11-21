//! A card leaving a transformed column starts its exit where it was painted.
//!
//! A node with `-azul-animation-out` that leaves the DOM stays on screen as
//! a zombie: the retained frame's picture, cut at the node's rect and moved
//! by its exit track. The rect was the node's LAID-OUT rect, rounded to
//! whole pixels, so under a transform - a toast card in `ToastStack`'s
//! column (`left: 50%; transform: translateX(-50%)`) - the cut was taken
//! half a column to the right of the card: the exit showed the card's right
//! half where the card had been, and empty column beside it. The cut is now
//! the rect the node is painted at (the raster's mapping through the
//! transforms and scroll offsets around it).

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

const W: f32 = 400.0;
const H: f32 = 200.0;

/// A column centred by `translateX(-50%)` at the foot of the window, and
/// `card` (a 160 x 40 red card that fades out as it leaves) in it.
fn page(card: bool) -> Dom {
    let (keyframes, _) = azul_css::parser2::new_from_str(
        "@keyframes leave { from { opacity: 1; } to { opacity: 0; } }",
    );
    let mut column = Dom::create_div().with_css(
        "position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%); \
         display: flex; flex-direction: column; align-items: center;",
    );
    column = column.with_child(
        Dom::create_div().with_css("width: 120px; height: 30px; background: #2244cc;"),
    );
    if card {
        column = column.with_child(Dom::create_div().with_css(
            "width: 160px; height: 40px; background: #dd2222; \
             -azul-animation-out: leave 400ms linear;",
        ));
    }
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("margin: 0; width: 100%; height: 100%; background: #ffffff;")
            .with_component_css(keyframes)
            .with_child(column),
    )
}

fn lay_out(lw: &mut LayoutWindow, dom: Dom) {
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let _ = lw.prepare_frame_cpu();
}

/// The frame a CPU backend paints in full: the layered compositor, then the
/// zombies over it.
fn frame(lw: &LayoutWindow) -> cpurender::AzulPixmap {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    let state = cpurender::CpuRenderState::from_gpu_cache(
        lw.gpu_state_manager.get_cache(DomId::ROOT_ID),
        DomId::ROOT_ID,
        &cpurender::ScrollOffsetMap::new(),
    );
    let rr = RendererResources::default();
    let mut glyphs = GlyphCache::new();
    let mut compositor = cpurender::CompositorState::new(W as u32, H as u32);
    compositor.allocate_layers_from_display_list(dl, 1.0, &state.transforms, &state.opacities);
    compositor
        .render_layers(dl, 1.0, &rr, &lw.font_manager, &mut glyphs, &state)
        .expect("the layers render");
    let mut out = cpurender::AzulPixmap::new(W as u32, H as u32).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    lw.composite_zombies_cpu(&mut out, 1.0, &rr, &mut glyphs);
    out
}

fn red_columns(p: &cpurender::AzulPixmap, y: u32) -> (u32, u32) {
    let w = p.width();
    let row = &p.data()[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
    let red: Vec<u32> = row
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, px)| px[0] > 150 && px[1] < 120 && px[2] < 120)
        .map(|(x, _)| x as u32)
        .collect();
    (
        red.first().copied().unwrap_or(u32::MAX),
        red.last().copied().unwrap_or(0),
    )
}

#[test]
fn a_card_leaving_a_transformed_column_is_cut_where_it_was_painted() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws;
    lay_out(&mut lw, page(true));
    let before = frame(&lw);
    // The card is the column's second child; its row is 20 px above the foot.
    let y = (H - 20.0 - 20.0) as u32;
    let (x0, x1) = red_columns(&before, y);
    assert!(x0 < x1, "harness: the card is painted ({x0}..{x1})");

    lay_out(&mut lw, page(false));
    assert_eq!(lw.zombies.len(), 1, "harness: the card leaves as a zombie");
    let cut: LogicalRect = *lw.zombies[0]
        .rects
        .values()
        .next()
        .expect("the card's cut");
    let painted = LogicalRect::new(
        LogicalPosition::new(x0 as f32, cut.origin.y),
        LogicalSize::new((x1 - x0 + 1) as f32, cut.size.height),
    );
    assert!(
        (cut.origin.x - painted.origin.x).abs() <= 1.0 && (cut.size.width - painted.size.width).abs() <= 1.0,
        "the exit cuts the card at {cut:?}, the card was painted at x {x0}..={x1}"
    );

    // The exit's first frame shows the card where it was, whole.
    let _ = lw.tick_animations(0.0);
    let after = frame(&lw);
    assert_eq!(
        red_columns(&after, y),
        (x0, x1),
        "the leaving card's first frame is drawn where the card was, all of it"
    );
}
