//! A `box-shadow` paints ONE shadow, not four stacked copies of it.
//!
//! The engine stores a shadow in four per-side slots
//! (`-azul-box-shadow-left/right/top/bottom`), and the `box-shadow`
//! shorthand writes the same shadow into all four. The painter pushed one
//! full `BoxShadow` item per slot, so every `box-shadow: ...` was drawn four
//! times on top of itself: a 50% black ring came out ~94% black
//! (1 - 0.5^4), and a soft elevation shadow four times as heavy as declared.
//! W3a's widget helpers had to write the bottom slot alone to get one shadow.
//!
//! Rendered headless through the CPU rasterizer: the ring pixel is the proof,
//! the display list says why.

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
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDE: f32 = 48.0;

/// `body > div(20x20, style)` laid out in a 48x48 window: the window and the
/// display list it painted.
fn laid_out(style: &str) -> (LayoutWindow, DisplayList) {
    let dom = Dom::create_body()
        .with_css("margin: 0; padding: 14px;")
        .with_child(Dom::create_div().with_css(&format!("width: 20px; height: 20px; {style}")));
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(SIDE, SIDE);
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
    (lw, dl)
}

/// Every `BoxShadow` item of the list: `(x, y, width, height)` of the box it
/// belongs to.
fn shadows(dl: &DisplayList) -> Vec<(f32, f32, f32, f32)> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::BoxShadow { bounds, .. } => {
                let r = bounds.0;
                Some((r.origin.x, r.origin.y, r.size.width, r.size.height))
            }
            _ => None,
        })
        .collect()
}

/// The rendered page: its RGBA pixels and its width in pixels.
fn rendered(lw: &LayoutWindow, dl: &DisplayList) -> (Vec<u8>, usize) {
    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        dl,
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
    let w = pm.width() as usize;
    (pm.data().to_vec(), w)
}

/// The red channel of the pixel at `(x, y)` of the rendered page.
fn red_at(lw: &LayoutWindow, dl: &DisplayList, x: usize, y: usize) -> u8 {
    let (pixels, w) = rendered(lw, dl);
    pixels[(y * w + x) * 4]
}

/// The red channel of the DARKEST pixel of the rendered page. On a white
/// page with nothing but a black shadow on it, `1 - red / 255` is the alpha
/// the shadow paints at where it is densest.
fn darkest_red(lw: &LayoutWindow, dl: &DisplayList) -> u8 {
    let (pixels, _) = rendered(lw, dl);
    pixels
        .chunks_exact(4)
        .map(|rgba| rgba[0])
        .min()
        .expect("the page has pixels")
}

/// A 6px, unblurred, 50% black spread ring around the box.
const RING: &str = "box-shadow: 0 0 0 6px rgba(0, 0, 0, 0.5);";

#[test]
fn a_box_shadow_is_one_display_list_item() {
    let (_, dl) = laid_out(RING);
    let found = shadows(&dl);
    assert_eq!(
        found.len(),
        1,
        "`box-shadow` declares ONE shadow; the painter must not push one per side slot: {found:?}"
    );
}

#[test]
fn a_half_transparent_shadow_ring_paints_half_transparent() {
    let (lw, dl) = laid_out(RING);
    let (x, y, _, h) = *shadows(&dl)
        .first()
        .expect("premise: the box has a shadow");
    // The middle of the ring's left arm: 3px left of the box, half way down.
    let px = (x - 3.0).round() as usize;
    let py = (y + h / 2.0).round() as usize;
    let red = red_at(&lw, &dl, px, py);
    // One 50% black layer over white is mid grey (~128). Two stacked would
    // be ~64, four ~16.
    assert!(
        (100..=160).contains(&red),
        "a 50% black ring over white must paint mid grey once, got red={red} at ({px}, {py}) - \
         a darker ring is the same shadow painted several times"
    );
}

/// The per-side slots still carry DIFFERENT shadows when a node declares
/// them: each distinct one paints (once).
#[test]
fn two_different_shadows_on_one_box_both_paint() {
    let (_, dl) = laid_out(
        "-azul-box-shadow-top: 0 -2px 0 0 rgba(255, 0, 0, 1); \
         -azul-box-shadow-bottom: 0 2px 0 0 rgba(0, 0, 255, 1);",
    );
    assert_eq!(
        shadows(&dl).len(),
        2,
        "two different shadows, two items: {:?}",
        shadows(&dl)
    );
}

/// A soft, see-through elevation shadow - the shape every card and popover
/// declares: offset, blurred, 50% black.
const SOFT: &str = "box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);";

/// Painted once, a shadow is nowhere denser than its colour: no pixel of a
/// `rgba(0, 0, 0, 0.5)` shadow is darker than 50% black over white. Four
/// stacked copies came out ~94% black where the shadow is solid (under the
/// box's edge), ~76% where the blur thins it to 30%.
#[test]
fn a_blurred_see_through_shadow_paints_no_denser_than_its_declared_alpha() {
    let (lw, dl) = laid_out(SOFT);
    assert_eq!(
        shadows(&dl).len(),
        1,
        "one declared shadow, one item: {:?}",
        shadows(&dl)
    );
    let darkest = darkest_red(&lw, &dl);
    let alpha = 1.0 - f32::from(darkest) / 255.0;
    assert!(
        alpha <= 0.5 + 0.03,
        "the shadow declares alpha 0.5 but paints at {alpha:.2} (darkest red={darkest}) - the \
         same shadow painted several times on top of itself"
    );
    assert!(
        alpha >= 0.15,
        "premise: the shadow paints at all (darkest red={darkest}, alpha {alpha:.2})"
    );
}
