//! Quote bars drawn by one gradient paint each colour at its length.
//!
//! AzMail's editor draws one coloured bar per quote level on the left edge of
//! a flat editable block, with a single background (AzMail exploration,
//! scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, section 5 and probe
//! `bars.html`):
//!
//! `linear-gradient(to right, red 0 3px, transparent 3px 6px, blue 6px 9px,
//! transparent 9px)`
//!
//! Every stop sits at a length, most of them twice (a stop with two positions
//! is two stops of one colour, CSS Images 4 section 3.5.1), and each colour
//! change is a hard stop. So: red over x 0..3, the page over 3..6, blue over
//! 6..9 and the page from 9 on. Today such a gradient paints nothing (a stop
//! at a length did not parse, nor did the bare `0`).
//!
//! Rendered through the CPU rasterizer like
//! `a_linear_gradient_puts_its_colours_where_css_says.rs`.
//! Not compiled by the author (house rule); expected RED.

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

const WIDTH: f32 = 120.0;
const HEIGHT: f32 = 40.0;

const QUOTE_BARS: &str = "linear-gradient(to right, red 0 3px, transparent 3px 6px, blue 6px 9px, \
                          transparent 9px)";

/// A 100 x 20 box at the window's origin, on a white page, with
/// `background: <gradient>`, rendered: RGBA pixels and the row width.
fn rendered(gradient: &str) -> (Vec<u8>, usize) {
    let dom = Dom::create_body()
        .with_css("margin: 0; padding: 0; background: #ffffff;")
        .with_child(Dom::create_div().with_css(&format!(
            "width: 100px; height: 20px; background: {gradient};"
        )));
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the page lays out");
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
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .expect("the page renders");
    let w = pm.width() as usize;
    (pm.data().to_vec(), w)
}

/// `(r, g, b)` of the pixel at `x` on the box's middle row.
fn rgb_at(pixels: &[u8], w: usize, x: usize) -> (u8, u8, u8) {
    let i = (10 * w + x) * 4;
    (pixels[i], pixels[i + 1], pixels[i + 2])
}

#[test]
fn quote_bars_paint_red_then_the_page_then_blue_then_the_page() {
    let (px, w) = rendered(QUOTE_BARS);
    let is_red = |(r, g, b): (u8, u8, u8)| r > 200 && g < 60 && b < 60;
    let is_blue = |(r, g, b): (u8, u8, u8)| b > 200 && r < 60 && g < 60;
    let is_page = |(r, g, b): (u8, u8, u8)| r > 200 && g > 200 && b > 200;

    for (x, what, ok) in [
        (1, "red (0..3px)", is_red(rgb_at(&px, w, 1))),
        (4, "the page (3..6px)", is_page(rgb_at(&px, w, 4))),
        (7, "blue (6..9px)", is_blue(rgb_at(&px, w, 7))),
        (12, "the page (after 9px)", is_page(rgb_at(&px, w, 12))),
        (90, "the page (after 9px)", is_page(rgb_at(&px, w, 90))),
    ] {
        assert!(
            ok,
            "x = {x} must be {what}, got {:?}; the whole row: {:?}",
            rgb_at(&px, w, x),
            (0..16).map(|x| rgb_at(&px, w, x)).collect::<Vec<_>>()
        );
    }
}
