//! A box the display list paints after a composited layer shows OVER that
//! layer in the CPU compositor - list order is paint order.
//!
//! The layered compositor gives a scroll frame, a translucent group and a
//! transformed box a pixel buffer of its own (a layer), renders the parent's
//! items without the layer's range, and composites the parent first and the
//! layer after it. Whatever the parent paints AFTER the layer's range - an
//! absolutely positioned sheet, a popup, a status bar over a list - came
//! out UNDER the layer: AzDrive's Details header and selected row showed
//! through its conflict and Properties sheets (MEETDRIVE6, 2026-10-03).

use std::collections::HashMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, cpurender, glyph_cache::GlyphCache, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: u32 = 200;
const H: u32 = 100;

/// body > `first` (a red box) ; body > a blue sheet, absolutely positioned
/// at (20, 10)-(120, 40), over it. The pixel at (50, 20).
fn pixel_under_the_sheet(first: Dom) -> [u8; 4] {
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0; background: #ffffff;")
        .with_child(first)
        .with_child(Dom::create_div().with_css(
            "position: absolute; left: 20px; top: 10px; width: 100px; height: 30px; \
             background: #0000ff;",
        ));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W as f32, H as f32);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &rr,
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let dl = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list;

    let mut glyph_cache = GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(Default::default());
    let mut compositor = cpurender::CompositorState::new(W, H);
    compositor.allocate_layers_from_display_list(dl, 1.0, &HashMap::new(), &HashMap::new());
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
    let i = ((20 * W + 50) * 4) as usize;
    let d = out.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

fn assert_blue(what: &str, [r, g, b, _]: [u8; 4]) {
    assert!(
        b > 200 && r < 60 && g < 60,
        "the absolutely positioned sheet is painted after {what}: it must show over it, got \
         rgb({r}, {g}, {b})"
    );
}

#[test]
fn a_sheet_painted_after_a_scroll_frame_shows_over_it() {
    let list = Dom::create_div()
        .with_css("width: 150px; height: 60px; overflow: auto; background: #ff0000;")
        .with_child(
            Dom::create_div().with_css("width: 140px; height: 200px; background: #ff0000;"),
        );
    assert_blue("the scrolling list", pixel_under_the_sheet(list));
}

#[test]
fn a_sheet_painted_after_a_translucent_box_shows_over_it() {
    let faded = Dom::create_div()
        .with_css("width: 150px; height: 60px; background: #ff0000; opacity: 0.5;");
    assert_blue("the translucent box", pixel_under_the_sheet(faded));
}

#[test]
fn a_sheet_painted_after_a_transformed_box_shows_over_it() {
    let moved = Dom::create_div().with_css(
        "width: 150px; height: 60px; background: #ff0000; transform: translate(5px, 5px);",
    );
    assert_blue("the transformed box", pixel_under_the_sheet(moved));
}
