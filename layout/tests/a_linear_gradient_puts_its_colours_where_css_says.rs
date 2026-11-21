//! A `linear-gradient` puts its colours where CSS says.
//!
//! Found by the AzMail exploration while trying to draw one coloured bar per
//! quote level on a flat editable block
//! (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, section 5). Rendered
//! headless on the 2026-09-30 release dylib through the E2E `mount` op:
//!
//! - `linear-gradient(90deg, red, blue)` painted BLUE on the left, while
//!   `to right, red, blue` (the same direction, CSS Images 3 section 3.1.1)
//!   painted red on the left;
//! - a hard stop, `linear-gradient(90deg, red 50%, blue 50%)`, painted nothing;
//! - a stop at a length, `linear-gradient(to right, red 10px, blue 10px)`,
//!   painted nothing.
//!
//! Rendered through the CPU rasterizer like `a_box_shadow_paints_once.rs`.
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

/// A 100 x 20 box at the window's origin with `background: <gradient>`,
/// rendered: RGBA pixels and the row width in pixels.
fn rendered(gradient: &str) -> (Vec<u8>, usize) {
    let dom = Dom::create_body()
        .with_css("margin: 0; padding: 0;")
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

/// `(red, blue)` of the pixel at `x` on the box's middle row.
fn red_blue_at(pixels: &[u8], w: usize, x: usize) -> (u8, u8) {
    let i = (10 * w + x) * 4;
    (pixels[i], pixels[i + 2])
}

#[test]
fn a_90deg_gradient_runs_left_to_right_like_to_right() {
    let (px, w) = rendered("linear-gradient(90deg, red, blue)");
    let (r_left, b_left) = red_blue_at(&px, w, 2);
    let (r_right, b_right) = red_blue_at(&px, w, 97);
    assert!(
        r_left > b_left && b_right > r_right,
        "90deg points to the right: red on the left, blue on the right; got left \
         (r {r_left}, b {b_left}), right (r {r_right}, b {b_right})"
    );
}

#[test]
fn a_hard_stop_splits_the_box_into_two_colours() {
    let (px, w) = rendered("linear-gradient(90deg, red 50%, blue 50%)");
    let (r_a, b_a) = red_blue_at(&px, w, 25);
    let (r_b, b_b) = red_blue_at(&px, w, 75);
    assert!(
        r_a > 200 && b_a < 60 && b_b > 200 && r_b < 60,
        "the left half is red and the right half blue; got x=25 (r {r_a}, b {b_a}), x=75 (r \
         {r_b}, b {b_b})"
    );
}

#[test]
fn a_stop_at_a_length_is_placed_at_that_length() {
    let (px, w) = rendered("linear-gradient(to right, red 10px, blue 10px)");
    let (r_a, b_a) = red_blue_at(&px, w, 5);
    let (r_b, b_b) = red_blue_at(&px, w, 30);
    assert!(
        r_a > 200 && b_a < 60 && b_b > 200 && r_b < 60,
        "red up to 10px, blue after; got x=5 (r {r_a}, b {b_a}), x=30 (r {r_b}, b {b_b})"
    );
}
