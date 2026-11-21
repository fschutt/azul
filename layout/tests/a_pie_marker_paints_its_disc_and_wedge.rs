//! A pie marker paints its disc and its wedge.
//!
//! AzDrive's navigation pane ends in two pie markers (SPEC 2.11): an 11px
//! disc on a pale ground with a wedge in the tone's colour growing clockwise
//! from 12 o'clock. The widget builds each part as a full-size layer clipped
//! to its polygon (`Dom::with_svg_clip_path`). In the running app the
//! markers were laid out where they belong - and nothing was painted there.
//!
//! Rendered headless through the CPU rasterizer at 1x: a half-full green pie
//! in the 15 x 25 hit area at the window's corner, its disc centred at
//! (7.5, 12.5).

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
    widgets::pie_marker::{PieMarker, PieTone},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDE: f32 = 40.0;

/// The page's RGBA pixels and its width.
fn painted() -> (Vec<u8>, usize) {
    let dom = Dom::create_body()
        .with_css("margin: 0px; background: #FFFFFF;")
        .with_child(PieMarker::create(0.5, PieTone::Good).dom());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(SIDE, SIDE);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the pie lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        &dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: SIDE,
            height: SIDE,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .expect("the page renders");
    (pm.data().to_vec(), pm.width() as usize)
}

fn rgb(pixels: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * w + x) * 4;
    (pixels[i], pixels[i + 1], pixels[i + 2])
}

#[test]
fn a_half_full_pie_shows_its_wedge_in_the_tones_colour_and_its_ground_beside_it() {
    let (pixels, w) = painted();
    let (r, g, b) = rgb(&pixels, w, 10, 12);
    assert!(
        g > r + 30 && g > b + 30,
        "the wedge (right of the centre) is the Good tone #4E9A5B: got {r},{g},{b}"
    );
    let (r, g, b) = rgb(&pixels, w, 5, 12);
    assert!(
        r < 245 && (r, g, b) != (255, 255, 255) && g <= r + 10,
        "the ground (left of the centre) is the pale disc #E2DFD8: got {r},{g},{b}"
    );
    assert_eq!(rgb(&pixels, w, 30, 30), (255, 255, 255), "outside the disc the page is white");
}
